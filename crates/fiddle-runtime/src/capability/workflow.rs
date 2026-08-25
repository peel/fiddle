use super::{Capability, CapabilityError, ExecutionGrant};
use crate::agent::{
    attempt_briefed, AgentBudget, Brief, Declarations, Held, ToolHost, Transcripts, PREAMBLE,
};
use crate::effect::{
    registry, Construct, EffectError, EffectOutcome, ErasedReceipt, Executor, Recurrence,
    StepParams,
};
use crate::gateway::Redaction;
use crate::workspace::WorkspaceCommand;
use fiddle_core::{CapabilityId, EffectName, EvidenceRef, HumanDecisionRequirement, Published};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

pub const WORKFLOW_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    Agent {
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
    },
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
}

enum Ready {
    Agent { task: String, max_turns: usize },
    Check { command: WorkspaceCommand },
    Effect { construct: Construct },
}

pub struct WorkflowCapability<'a, M> {
    id: CapabilityId,
    stage: &'static str,
    workflow: Workflow,
    steps: Vec<Ready>,
    executor: Executor<'a>,
    params: StepParams,
    ports: WorkflowPorts<M>,
    receipts: Mutex<Vec<EvidenceRef>>,
}

fn ready(step: &Step, prompts: &Path) -> Result<Ready, WorkflowRefusal> {
    match step {
        Step::Agent { prompt, max_turns } => {
            let path = prompts.join(prompt);
            let task =
                std::fs::read_to_string(&path).map_err(|source| WorkflowRefusal::Unreadable {
                    path: path.clone(),
                    reason: source.to_string(),
                })?;
            match task.trim().is_empty() {
                true => Err(WorkflowRefusal::Taskless { path }),
                false => Ok(Ready::Agent {
                    task,
                    max_turns: *max_turns as usize,
                }),
            }
        }
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
        Step::Effect { name } => {
            let descriptor = registry::describe(name)
                .ok_or_else(|| WorkflowRefusal::Unperformable { name: name.clone() })?;
            match descriptor.minimum {
                HumanDecisionRequirement::Human => {
                    Err(WorkflowRefusal::Gated { name: name.clone() })
                }
                HumanDecisionRequirement::Automatic => Ok(Ready::Effect {
                    construct: registry::resolve(name)
                        .ok_or_else(|| WorkflowRefusal::Unperformable { name: name.clone() })?,
                }),
            }
        }
    }
}

pub fn without_waiting(error: EffectError) -> CapabilityError {
    match error.recurrence() {
        Recurrence::Awaiting => CapabilityError::WouldWait {
            reason: error.to_string(),
        },
        Recurrence::Correctable | Recurrence::Permanent => CapabilityError::Effect(error),
    }
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
            receipts: Mutex::new(Vec::new()),
        })
    }

    pub fn workflow(&self) -> &Workflow {
        &self.workflow
    }

    async fn attempt(&self, task: &str, max_turns: usize) -> Result<(), CapabilityError> {
        attempt_briefed(
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

    async fn effect(&self, construct: Construct) -> Result<(), CapabilityError> {
        let receipt = construct(&self.executor, &self.params)
            .map_err(without_waiting)?
            .run(&self.executor, &self.params)
            .await
            .map_err(without_waiting)?;
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

    async fn execute(
        &self,
        grant: ExecutionGrant,
        _work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
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
        for step in &self.steps {
            match step {
                Ready::Agent { task, max_turns } => self.attempt(task, *max_turns).await?,
                Ready::Check { command } => self.check(command).await?,
                Ready::Effect { construct } => self.effect(*construct).await?,
            }
        }
        Ok(EvidenceRef(format!(
            "workflow:{}:{}",
            self.workflow.name(),
            grant.attempt_id().0
        )))
    }

    fn receipts(&self) -> Vec<EvidenceRef> {
        self.receipts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}
