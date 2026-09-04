use super::commit;
use super::{Capability, CapabilityError, Executed, ExecutionInput};
use crate::agent::{
    attempt_briefed, judge_briefed, AgentBudget, Brief, Declarations, Held, RepairReport, ToolHost,
    Transcripts, Verdict, JUDGE_PREAMBLE, PREAMBLE,
};
use crate::effect::{
    registry, Construct, EffectError, EffectOutcome, ErasedReceipt, Executor, IntegrationOperation,
    Recurrence, StepOutputs, StepParams,
};
use crate::gateway::Redaction;
use crate::jira::AddComment;
use crate::toil::{Change, Eligible, Quoted, Scope};
use crate::workspace::WorkspaceCommand;
use fiddle_core::{
    correlation_key, CapabilityId, ChangeSetState, EffectName, EvidenceRef,
    HumanDecisionRequirement, ProposedEffect, Published, WorkItemState, JIRA_COMMENT_ADDED,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const WORKFLOW_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    Agent {
        prompt: PathBuf,
        max_turns: u32,
    },
    Evaluate {
        prompt: PathBuf,
        max_turns: u32,
    },
    Check {
        program: String,
        args: Vec<String>,
        timeout_secs: u64,
    },
    Effect {
        name: EffectName,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reaching: Option<String>,
    },
    Commit {},
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowFile {
    pub version: u32,
    pub name: String,
    pub stage: String,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Workflow {
    name: String,
    stage: String,
    steps: Vec<Step>,
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum WorkflowError {
    #[error("a workflow with no step does no work")]
    NoSteps,

    #[error("this build reads workflow version {WORKFLOW_VERSION}, and the document says {0}")]
    Version(u32),
}

fn validate(steps: &[Step]) -> Result<(), WorkflowError> {
    match steps.is_empty() {
        true => Err(WorkflowError::NoSteps),
        false => Ok(()),
    }
}

impl Workflow {
    pub fn new(name: String, stage: String, steps: Vec<Step>) -> Result<Self, WorkflowError> {
        validate(&steps)?;
        Ok(Workflow { name, stage, steps })
    }

    pub fn to_file(&self) -> WorkflowFile {
        WorkflowFile {
            version: WORKFLOW_VERSION,
            name: self.name.clone(),
            stage: self.stage.clone(),
            steps: self.steps.clone(),
        }
    }
}

impl TryFrom<WorkflowFile> for Workflow {
    type Error = WorkflowError;

    fn try_from(file: WorkflowFile) -> Result<Self, WorkflowError> {
        match file.version == WORKFLOW_VERSION {
            true => Workflow::new(file.name, file.stage, file.steps),
            false => Err(WorkflowError::Version(file.version)),
        }
    }
}

impl Workflow {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn stage(&self) -> &str {
        &self.stage
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}

pub const WORKFLOW: CapabilityId = CapabilityId("workflow");

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum WorkflowRefusal {
    #[error("a workflow filed under `{filed}` proposes its effects under `{proposing}`")]
    Misbound {
        filed: CapabilityId,
        proposing: CapabilityId,
    },

    #[error("the prompt at {path} could not be read: {reason}")]
    Unreadable { path: PathBuf, reason: String },

    #[error("the prompt at {path} says nothing, so the step has no task")]
    Taskless { path: PathBuf },

    #[error("`{name}` is not an effect this build performs, so no step could perform it")]
    Unperformable { name: EffectName },

    #[error(
        "`{name}` gates on a human decision, and a version {WORKFLOW_VERSION} workflow \
         runs to an end or fails"
    )]
    Gated { name: EffectName },
}

pub struct WorkflowPorts<M> {
    pub model: M,
    pub host: ToolHost,
    pub budget: AgentBudget,
    pub redaction: Redaction,
    pub transcripts: Option<Transcripts>,
    pub prompts: PathBuf,
    pub stub_root: PathBuf,
}

enum Ready {
    Agent {
        task: String,
        max_turns: usize,
    },
    Evaluate {
        task: String,
        max_turns: usize,
    },
    Check {
        command: WorkspaceCommand,
    },
    Effect {
        construct: Construct,
        reaching: Option<String>,
    },
    Commit,
}

pub struct WorkflowCapability<'a, M> {
    id: CapabilityId,
    stage: &'static str,
    workflow: Workflow,
    steps: Vec<Ready>,
    executor: Executor<'a>,
    params: StepParams,
    ports: WorkflowPorts<M>,
    scope: Option<Scope>,
    qualification: Option<Eligible>,
    receipts: Mutex<Vec<EvidenceRef>>,
    entered: Mutex<Vec<StepOutputs>>,
}

fn task_in(prompt: &Path, prompts: &Path) -> Result<String, WorkflowRefusal> {
    let path = prompts.join(prompt);
    let task = std::fs::read_to_string(&path).map_err(|source| WorkflowRefusal::Unreadable {
        path: path.clone(),
        reason: source.to_string(),
    })?;
    match task.trim().is_empty() {
        true => Err(WorkflowRefusal::Taskless { path }),
        false => Ok(task),
    }
}

fn quoted_ticket(admitted: Option<&Eligible>, work_item: Option<&WorkItemState>) -> Option<String> {
    let quoted = match admitted {
        Some(admitted) => admitted.quoted.clone(),
        None => {
            let work_item = work_item?;
            Quoted::of(&crate::toil::ticket_text(
                work_item.summary.as_deref().unwrap_or_default(),
                work_item.description.as_deref(),
                &[],
            ))
        }
    };
    match quoted.text().is_empty() {
        true => None,
        false => Some(quoted.fenced()),
    }
}

fn task_carrying(task: &str, quoted: Option<&String>) -> String {
    match quoted {
        Some(quoted) => format!("{task}\n\n{quoted}"),
        None => task.to_string(),
    }
}

fn ready(step: &Step, prompts: &Path) -> Result<Ready, WorkflowRefusal> {
    match step {
        Step::Agent { prompt, max_turns } => Ok(Ready::Agent {
            task: task_in(prompt, prompts)?,
            max_turns: *max_turns as usize,
        }),
        Step::Evaluate { prompt, max_turns } => Ok(Ready::Evaluate {
            task: task_in(prompt, prompts)?,
            max_turns: *max_turns as usize,
        }),
        Step::Check {
            program,
            args,
            timeout_secs,
        } => Ok(Ready::Check {
            command: WorkspaceCommand {
                program: program.clone(),
                args: args.clone(),
                timeout: Duration::from_secs(*timeout_secs),
            },
        }),
        Step::Commit {} => Ok(Ready::Commit),
        Step::Effect { name, reaching } => {
            let descriptor = registry::describe(name)
                .ok_or_else(|| WorkflowRefusal::Unperformable { name: name.clone() })?;
            match descriptor.minimum {
                HumanDecisionRequirement::Human => {
                    Err(WorkflowRefusal::Gated { name: name.clone() })
                }
                HumanDecisionRequirement::Automatic => Ok(Ready::Effect {
                    construct: registry::resolve(name)
                        .ok_or_else(|| WorkflowRefusal::Unperformable { name: name.clone() })?,
                    reaching: reaching.clone(),
                }),
            }
        }
    }
}

pub const STOPPED_BY_A_QUESTION: &str =
    "the attempt changed nothing and named a question the ticket has to answer before the \
     change can be made";

pub fn stopped_by(question: &str) -> String {
    format!("{STOPPED_BY_A_QUESTION}: {question}")
}

pub fn without_waiting(error: EffectError) -> CapabilityError {
    match error.recurrence() {
        Recurrence::Awaiting => CapabilityError::WouldWait {
            reason: error.to_string(),
        },
        Recurrence::Correctable | Recurrence::Permanent => CapabilityError::Effect(error),
    }
}

const THE_NOTE_REACHED_NO_WORK_ITEM: &str = "rejection_unpublished";

fn rejection_note(work_item: &str, findings: &[Published]) -> String {
    let mut told = vec![
        format!(
            "fiddle took `{work_item}` on, changed the project for it, and then rejected its \
             own change. This comment is the whole reason."
        ),
        "What the evaluation of that change found:".to_string(),
    ];
    told.extend(findings.iter().map(|finding| format!("- {finding}")));
    told.push(
        "The change reached no branch and no pull request, so there is nothing to review and \
         this issue is where it was."
            .to_string(),
    );
    told.push(format!(
        "What would change that: run `{work_item}` again, or write onto it what the findings \
         above show this run read the wrong way."
    ));
    told.join("\n")
}

fn evidence_of(receipt: &ErasedReceipt) -> EvidenceRef {
    let outcome = match receipt.outcome {
        EffectOutcome::Committed => "committed",
        EffectOutcome::NotCommitted => "not_committed",
        EffectOutcome::Unknown => "unknown",
    };
    let flattened: String = receipt
        .postcondition
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    EvidenceRef(format!(
        "effect:{}:{}:{outcome}:{}:{}",
        receipt.kind.as_str(),
        receipt.effect_id.0,
        receipt.external_ref.as_deref().unwrap_or("-"),
        Published::of(flattened).as_str(),
    ))
}

impl<'a, M> WorkflowCapability<'a, M>
where
    M: rig_core::completion::CompletionModel + 'static,
{
    pub fn new(
        id: CapabilityId,
        stage: &'static str,
        workflow: Workflow,
        executor: Executor<'a>,
        params: StepParams,
        ports: WorkflowPorts<M>,
    ) -> Result<Self, WorkflowRefusal> {
        if params.capability != id {
            return Err(WorkflowRefusal::Misbound {
                filed: id,
                proposing: params.capability,
            });
        }
        let steps = workflow
            .steps()
            .iter()
            .map(|step| ready(step, &ports.prompts))
            .collect::<Result<Vec<Ready>, WorkflowRefusal>>()?;
        Ok(WorkflowCapability {
            id,
            stage,
            workflow,
            steps,
            executor,
            params,
            ports,
            scope: None,
            qualification: None,
            receipts: Mutex::new(Vec::new()),
            entered: Mutex::new(Vec::new()),
        })
    }

    pub fn bounded_by(mut self, scope: Scope) -> Self {
        self.scope = Some(scope);
        self
    }

    pub fn qualified_by(mut self, admitted: Eligible) -> Self {
        self.qualification = Some(admitted);
        self
    }

    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    pub fn earned_on_entering_each_step(&self) -> Vec<StepOutputs> {
        self.entered
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    async fn attempt(&self, task: &str, max_turns: usize) -> Result<RepairReport, CapabilityError> {
        let report = attempt_briefed(
            self.ports.model.clone(),
            &self.ports.redaction,
            self.ports.host.clone(),
            AgentBudget {
                max_turns,
                ..self.ports.budget.clone()
            },
            Brief {
                preamble: PREAMBLE,
                task,
            },
            Held {
                shown: &[],
                declarations: Declarations::Unchecked,
            },
            self.ports.transcripts.as_ref(),
        )
        .await?;
        Ok(report)
    }

    fn declined(&self, report: &RepairReport) -> Result<Option<Published>, CapabilityError> {
        let Some(question) = report.question() else {
            return Ok(None);
        };
        if !self.ports.host.workspace.changed_files()?.is_empty() {
            return Ok(None);
        }
        Ok(Some(Published::of(stopped_by(question))))
    }

    fn within_scope(&self) -> Result<(), CapabilityError> {
        let Some(scope) = self.scope else {
            return Ok(());
        };
        let workspace = &self.ports.host.workspace;
        let measured = Change {
            files_changed: workspace.changed_files()?.len(),
            diff_lines: workspace.changed_lines()?,
        };
        Ok(scope.admits(&measured)?)
    }

    async fn evaluate(
        &self,
        task: &str,
        max_turns: usize,
        params: &mut StepParams,
    ) -> Result<(), CapabilityError> {
        let verdict = judge_briefed(
            self.ports.model.clone(),
            &self.ports.redaction,
            self.ports.host.clone(),
            AgentBudget {
                max_turns,
                ..self.ports.budget.clone()
            },
            Brief {
                preamble: JUDGE_PREAMBLE,
                task,
            },
            self.ports.transcripts.as_ref(),
        )
        .await?;
        params.earned.record_verdict(verdict)?;
        Ok(())
    }

    async fn check(&self, command: &WorkspaceCommand) -> Result<(), CapabilityError> {
        let result = self.ports.host.workspace.run(command).await?;
        match result.exit_code {
            0 => Ok(()),
            exit_code => Err(CapabilityError::CheckFailed {
                claimed: false,
                exit_code,
                stderr: result.stderr,
            }),
        }
    }

    async fn commit(&self, params: &mut StepParams) -> Result<(), CapabilityError> {
        let workspace = Arc::clone(&self.ports.host.workspace);
        let changed = workspace.changed_files()?;
        if changed.is_empty() {
            return Ok(());
        }
        let head = commit::commit_changed(
            &workspace,
            &changed,
            &commit::message(self.executor.project(), self.executor.invocation_ref()),
            self.ports.budget.tool_timeout,
        )
        .await?;
        params.earned.record_head_sha(&head)?;
        Ok(())
    }

    fn record_change_set(&self, work_id: &str) -> Result<(), CapabilityError> {
        let state = ChangeSetState {
            marker: Some(correlation_key(
                self.executor.project(),
                self.executor.invocation_ref(),
            )),
        };
        let destination = self.ports.stub_root.join(format!("changes/{work_id}.json"));
        super::stub::write_atomically(&destination, &state).map_err(|source| {
            CapabilityError::Write {
                path: destination.clone(),
                source,
            }
        })
    }

    async fn tell_the_work_item(&self, findings: &[Published]) {
        let Some(admitted) = self.qualification.as_ref() else {
            return;
        };
        let recorded = match self.publish_rejection(admitted, findings).await {
            Ok(receipt) => evidence_of(&receipt),
            Err(why) => EvidenceRef(format!(
                "{THE_NOTE_REACHED_NO_WORK_ITEM}:{}:{}",
                admitted.work_item,
                Published::of(why)
            )),
        };
        self.receipts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(recorded);
    }

    async fn publish_rejection(
        &self,
        admitted: &Eligible,
        findings: &[Published],
    ) -> Result<ErasedReceipt, String> {
        let comment = AddComment::new(
            admitted.work_item.clone(),
            &admitted.revision,
            rejection_note(&admitted.work_item, findings),
            self.executor.project(),
            self.executor.invocation_ref(),
        )
        .map_err(|refused| refused.to_string())?;
        let kind = comment.kind();
        let proposed = ProposedEffect {
            capability: self.executor.capability(),
            kind: EffectName::shipped(JIRA_COMMENT_ADDED),
            target: comment.target(),
            payload: comment.payload(),
        };
        let receipt = self
            .executor
            .execute(proposed, comment)
            .await
            .map_err(|refused| refused.to_string())?;
        Ok(ErasedReceipt::of(kind, receipt))
    }

    async fn effect(
        &self,
        construct: Construct,
        params: &mut StepParams,
    ) -> Result<(), CapabilityError> {
        let receipt = construct(&self.executor, params)
            .map_err(without_waiting)?
            .run(&self.executor, params)
            .await
            .map_err(without_waiting)?;
        params.earned.record(&receipt)?;
        self.receipts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(evidence_of(&receipt));
        Ok(())
    }
}

#[async_trait::async_trait]
impl<M> Capability for WorkflowCapability<'_, M>
where
    M: rig_core::completion::CompletionModel + 'static,
{
    fn id(&self) -> CapabilityId {
        self.id
    }

    fn stage(&self) -> &'static str {
        self.stage
    }

    async fn execute(&self, input: ExecutionInput<'_>) -> Result<Executed, CapabilityError> {
        let ExecutionInput {
            grant,
            work_id,
            invocation_ref,
            work_item,
        } = input;
        if grant.capability_id() != self.id() {
            return Err(CapabilityError::NotAuthorised {
                granted: grant.capability_id(),
                requested: self.id(),
            });
        }
        if invocation_ref != self.executor.invocation_ref() {
            return Err(CapabilityError::Misbound {
                bound: self.executor.invocation_ref().to_string(),
                asked: invocation_ref.to_string(),
            });
        }
        let quoted = quoted_ticket(self.qualification.as_ref(), work_item);
        let mut params = StepParams {
            earned: StepOutputs::default(),
            ..self.params.clone()
        }
        .observing(work_item);
        for step in &self.steps {
            self.entered
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(params.earned.clone());
            match step {
                Ready::Agent { task, max_turns } => {
                    let report = self
                        .attempt(&task_carrying(task, quoted.as_ref()), *max_turns)
                        .await?;
                    if let Some(finding) = self.declined(&report)? {
                        return Ok(Executed::Rejected {
                            findings: vec![finding],
                        });
                    }
                    self.within_scope()?
                }
                Ready::Evaluate { task, max_turns } => {
                    self.evaluate(
                        &task_carrying(task, quoted.as_ref()),
                        *max_turns,
                        &mut params,
                    )
                    .await?
                }
                Ready::Check { command } => self.check(command).await?,
                Ready::Commit => self.commit(&mut params).await?,
                Ready::Effect {
                    construct,
                    reaching,
                } => {
                    params.reaching = reaching.clone();
                    self.effect(*construct, &mut params).await?
                }
            }
            if matches!(params.earned.verdict(), Some(Verdict::Rejected { .. })) {
                break;
            }
        }
        match params.earned.verdict() {
            Some(Verdict::Rejected { findings }) => {
                let findings: Vec<Published> = findings.iter().map(Published::of).collect();
                self.tell_the_work_item(&findings).await;
                Ok(Executed::Rejected { findings })
            }
            Some(Verdict::Accepted {}) | None => {
                self.record_change_set(work_id)?;
                Ok(Executed::Earned(EvidenceRef(format!(
                    "workflow:{}:{}",
                    self.workflow.name(),
                    grant.attempt_id().0
                ))))
            }
        }
    }

    fn receipts(&self) -> Vec<EvidenceRef> {
        self.receipts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn qualification(&self) -> Option<&Eligible> {
        self.qualification.as_ref()
    }
}
