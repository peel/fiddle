use super::stub::write_atomically;
use super::{Capability, CapabilityError, ExecutionGrant};
use crate::agent::{attempt, AgentBudget, Direction, ToolHost, ToolReceipts};
use crate::effect::{
    EffectContext, EffectOutcome, EffectReceipt, Executor, IntegrationOperation, ObservedState,
    ResolvedDecision,
};
use crate::github::{
    branch_name, EnsureBranchPublished, EnsurePullRequest, EnsurePullRequestReady, GhError,
    PullRequest,
};
use crate::human::interpret::InterpretationBounds;
use crate::human::validate::{
    resolve, DecisionError, DecisionResolution, DecisionTrace, DecisionWalk, HumanAnswer,
    IgnoredReply,
};
use crate::human::{InteractionRef, PublishDecisionRequest, CONVERSATION_PAGES};
use crate::workspace::{DeclaredCommand, Workspace, WorkspaceCommand, WorkspacePath};
use fiddle_core::{
    correlation_key, decision_request_id, effect_id, payload_hash, AttemptId, CapabilityId,
    ChangeSetState, DecisionBinding, EffectKind, EvidenceRef, HumanDecisionRequest,
    InterpretedHumanDecision, Observation, ProposedEffect, Publication, Published, ReviewState,
    SourceRef, WorkRef,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const PROPOSE_ORIGIN: &str = "propose";

const REDIRECT_ORIGIN: &str = "redirect";

const OPEN: &str = "open";

pub(super) const COMMITTER: [&str; 2] = ["user.name=fiddle", "user.email=fiddle@invalid"];

const REGISTERED_TOOLS: [&str; 5] = [
    "read_file",
    "edit_file",
    "write_file",
    "list_files",
    "run_check",
];

const FOREIGN_TOOL: &str = "unregistered";

pub fn attempt_worktree(workspace_root: &Path, project: &str, invocation_ref: &str) -> PathBuf {
    workspace_root.join(branch_name(project, invocation_ref).replace('/', "-"))
}

pub struct ProposeConfig {
    pub repo: String,

    pub head_owner: String,

    pub base: String,

    pub title: String,

    pub body: String,

    pub project: String,

    pub fixture: PathBuf,

    pub workspace_root: PathBuf,

    pub stub_root: PathBuf,

    pub check: WorkspaceCommand,

    pub commands: std::sync::Arc<Vec<DeclaredCommand>>,

    pub command_timeout: std::time::Duration,

    pub budget: AgentBudget,

    pub deciders: Vec<u64>,

    pub interpretation: InterpretationBounds,

    pub cancel: tokio_util::sync::CancellationToken,
}

impl ProposeConfig {
    fn project_agrees_with(&self, executor: &Executor<'_>) -> bool {
        self.project == executor.project()
    }
}

pub struct ProposeChange<'a, M> {
    executor: Executor<'a>,
    ctx: &'a EffectContext,
    decisions: &'a dyn DecisionTrace,
    model: M,
    config: ProposeConfig,
    receipts: Mutex<Vec<EvidenceRef>>,
    tools: Arc<Mutex<ToolReceipts>>,
    observed: Mutex<Observed>,
    publication: Mutex<Option<Publication>>,
}

#[derive(Default)]
struct Observed {
    branch: Option<String>,
    head_sha: Option<String>,
    pull_request: Option<u64>,
    failure: Option<String>,
}

struct Produced {
    workspace: Arc<Workspace>,
    sha: String,
    changed: usize,
}

impl<'a, M> ProposeChange<'a, M>
where
    M: rig_core::completion::CompletionModel + 'static,
{
    pub fn new(
        executor: Executor<'a>,
        ctx: &'a EffectContext,
        decisions: &'a dyn DecisionTrace,
        model: M,
        config: ProposeConfig,
    ) -> Self {
        ProposeChange {
            executor,
            ctx,
            decisions,
            model,
            config,
            receipts: Mutex::new(Vec::new()),
            tools: Arc::new(Mutex::new(ToolReceipts::default())),
            observed: Mutex::new(Observed::default()),
            publication: Mutex::new(None),
        }
    }

    fn branch(&self) -> String {
        branch_name(self.executor.project(), self.executor.invocation_ref())
    }

    fn worktree(&self) -> PathBuf {
        attempt_worktree(
            &self.config.workspace_root,
            self.executor.project(),
            self.executor.invocation_ref(),
        )
    }

    fn draft_pull_request(&self, branch: &str) -> EnsurePullRequest {
        EnsurePullRequest::new(
            self.config.repo.clone(),
            self.config.head_owner.clone(),
            branch.to_string(),
            self.config.base.clone(),
            self.config.title.clone(),
            self.config.body.clone(),
            true,
        )
    }

    async fn opened(&self, branch: &str) -> Result<Option<PullRequest>, GhError> {
        self.draft_pull_request(branch).inspect(self.ctx).await
    }

    async fn head_of(&self, pr: u64) -> Result<String, GhError> {
        let path = format!("/repos/{}/pulls/{pr}", self.config.repo);
        let response = self
            .ctx
            .gh
            .api("GET", &path, None, &self.ctx.cancel)
            .await?;
        response.body["head"]["sha"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| {
                GhError::Malformed(format!(
                    "{path} answered {} with no head sha",
                    response.status
                ))
            })
    }

    fn gated(&self, pr: u64, head_sha: &str) -> EnsurePullRequestReady {
        EnsurePullRequestReady::new(self.config.repo.clone(), pr, head_sha.to_string())
    }

    fn question_about(&self, work_id: &str, pr: u64, head_sha: &str) -> HumanDecisionRequest {
        let repo = &self.config.repo;
        let ready = self.gated(pr, head_sha);
        let effect = effect_id(
            self.executor.project(),
            self.executor.invocation_ref(),
            EffectKind::EnsurePullRequestReady,
            &ready.target(),
        );
        let binding = DecisionBinding {
            request: decision_request_id(
                self.executor.project(),
                self.executor.invocation_ref(),
                &effect,
            ),
            effect,
            payload: payload_hash(&ready.payload()),
            head_sha: head_sha.to_string(),
        };

        HumanDecisionRequest {
            invocation_ref: self.executor.invocation_ref().to_string(),
            work_ref: Some(WorkRef(work_id.to_string())),
            capability: self.id(),
            binding,
            question: format!("May fiddle mark pull request {repo}#{pr} ready for review?"),
            rationale: format!(
                "The change was produced by one bounded attempt and passed the check \
                 fiddle ran itself over the tree that attempt left. It is published as \
                 a draft at {head_sha}; marking it ready is the step fiddle will not \
                 take on its own.\n\nThis question is about commit {head_sha} and \
                 nothing else. If an earlier question from fiddle stands above it \
                 naming a different commit, that commit is no longer this pull \
                 request's head: the earlier question was about a revision that no \
                 longer exists, answering it now does nothing, and this question \
                 supersedes it. Fiddle leaves it standing rather than editing or \
                 deleting it, because a comment fiddle wrote and never touches is what \
                 lets it refuse a request comment somebody else has edited."
            ),
            risks: vec![
                "Marking it ready puts the change in front of reviewers and starts \
                 whatever the repository does on a ready pull request."
                    .to_string(),
                "The check that passed is the one this deployment configured, and it \
                 is not a review."
                    .to_string(),
            ],
            alternatives: vec![
                "Leave it as a draft: replying with anything other than an approval \
                 changes nothing out here."
                    .to_string(),
                "Say what to do differently, and fiddle will attempt the change again \
                 rather than proceed with this one."
                    .to_string(),
            ],
            evidence: self.receipts(),
        }
    }

    async fn propose<O>(
        &self,
        kind: EffectKind,
        target: String,
        payload: String,
        operation: O,
    ) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, CapabilityError>
    where
        O: IntegrationOperation,
    {
        self.proposing(kind, target, payload, operation, None).await
    }

    async fn propose_decided<O>(
        &self,
        kind: EffectKind,
        target: String,
        payload: String,
        operation: O,
        decision: &ResolvedDecision,
    ) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, CapabilityError>
    where
        O: IntegrationOperation,
    {
        self.proposing(kind, target, payload, operation, Some(decision))
            .await
    }

    async fn proposing<O>(
        &self,
        kind: EffectKind,
        target: String,
        payload: String,
        operation: O,
        decision: Option<&ResolvedDecision>,
    ) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, CapabilityError>
    where
        O: IntegrationOperation,
    {
        let proposed = ProposedEffect {
            capability: self.id(),
            kind,
            target,
            payload,
        };
        let receipt = match decision {
            None => self.executor.execute(proposed, operation).await?,
            Some(decision) => {
                self.executor
                    .execute_decided(proposed, operation, decision)
                    .await?
            }
        };
        self.receipts
            .lock()
            .unwrap()
            .push(receipt_evidence(kind, &receipt));
        Ok(receipt)
    }

    async fn produce(&self) -> Result<Produced, CapabilityError> {
        self.produce_from("HEAD", Direction::Fresh).await
    }

    async fn produce_from(
        &self,
        revision: &str,
        direction: Direction<'_>,
    ) -> Result<Produced, CapabilityError> {
        let worktree = self.worktree();
        let root = worktree.parent().unwrap_or(&self.config.workspace_root);
        let name = AttemptId(
            worktree
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        );

        let workspace = Arc::new(Workspace::create_at(
            &self.config.fixture,
            root,
            &name,
            revision,
            self.config.cancel.clone(),
        )?);

        let host = ToolHost {
            workspace: Arc::clone(&workspace),
            cancel: self.config.cancel.clone(),
            check: self.config.check.clone(),
            commands: Arc::clone(&self.config.commands),
            command_timeout: self.config.command_timeout,
            receipts: Arc::clone(&self.tools),
        };

        let report = attempt(
            self.model.clone(),
            host,
            self.config.budget.clone(),
            direction,
        )
        .await?;

        let check = workspace.run(&self.config.check).await?;
        let changed = workspace.changed_files()?;

        if check.exit_code != 0 {
            return Err(CapabilityError::CheckFailed {
                claimed: report.claimed_complete,
                exit_code: check.exit_code,
                stderr: check.stderr,
            });
        }
        if changed.is_empty() {
            return Err(CapabilityError::NothingProposed);
        }

        let sha = self.commit(&workspace, &changed).await?;
        Ok(Produced {
            workspace,
            sha,
            changed: changed.len(),
        })
    }

    async fn commit(
        &self,
        workspace: &Workspace,
        changed: &[WorkspacePath],
    ) -> Result<String, CapabilityError> {
        let mut add = vec!["add".to_string(), "-f".to_string(), "--".to_string()];
        add.extend(changed.iter().map(|path| path.as_str().to_string()));
        self.git(workspace, add).await?;

        let mut commit: Vec<String> = COMMITTER
            .iter()
            .flat_map(|setting| ["-c".to_string(), (*setting).to_string()])
            .collect();
        commit.extend([
            "commit".to_string(),
            "-q".to_string(),
            "-m".to_string(),
            format!(
                "{}: {}",
                self.config.project,
                self.executor.invocation_ref()
            ),
        ]);
        self.git(workspace, commit).await?;

        Ok(self
            .git(workspace, vec!["rev-parse".to_string(), "HEAD".to_string()])
            .await?
            .trim()
            .to_string())
    }

    async fn git(
        &self,
        workspace: &Workspace,
        args: Vec<String>,
    ) -> Result<String, CapabilityError> {
        let command = WorkspaceCommand {
            program: "git".to_string(),
            args: args.clone(),
            timeout: self.config.budget.tool_timeout,
        };
        let result = workspace.run(&command).await?;
        match result.exit_code {
            0 => Ok(result.stdout),
            _ => Err(CapabilityError::Workspace(
                crate::workspace::WorkspaceError::Git {
                    command: args.join(" "),
                    stderr: result.stderr,
                },
            )),
        }
    }

    async fn publish(&self, branch: &str, head_sha: &str) -> Result<u64, CapabilityError> {
        let publish_branch = EnsureBranchPublished::new(
            self.config.repo.clone(),
            branch.to_string(),
            head_sha.to_string(),
        );
        let published = self
            .propose(
                EffectKind::EnsureBranchPublished,
                publish_branch.target(),
                publish_branch.payload(),
                publish_branch,
            )
            .await?;
        {
            let mut observed = self.observed.lock().unwrap();
            observed.branch = Some(published.value.branch.clone());
            observed.head_sha = Some(published.value.sha.clone());
        }

        let open = self.draft_pull_request(branch);
        let opened = self
            .propose(
                EffectKind::EnsurePullRequest,
                open.target(),
                open.payload(),
                open,
            )
            .await?;
        self.observed.lock().unwrap().pull_request = Some(opened.value.number);
        Ok(opened.value.number)
    }

    async fn ask(
        &self,
        work_id: &str,
        pr: u64,
        head_sha: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        let request = self.question_about(work_id, pr, head_sha);
        let ask = PublishDecisionRequest::new(self.config.repo.clone(), pr, request.clone());
        let receipt = self
            .propose(
                EffectKind::PublishDecisionRequest,
                ask.target(),
                ask.payload(),
                ask,
            )
            .await?;

        Err(CapabilityError::AwaitingDecision {
            request: request.binding.request,
            interaction: receipt.value,
            question: request.question,
        })
    }

    async fn continue_from(
        &self,
        request: HumanDecisionRequest,
        interaction: InteractionRef,
        pr: u64,
        head_sha: &str,
        work_id: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        let gated = self.gated(pr, head_sha);
        let target = gated.target();
        let payload = gated.payload();
        let walk = DecisionWalk {
            repo: &self.config.repo,
            pr,
            max_pages: CONVERSATION_PAGES,
            project: self.executor.project(),
            invocation_ref: self.executor.invocation_ref(),
            kind: EffectKind::EnsurePullRequestReady,
            target: &target,
            payload: &payload,
            allowlist: &self.config.deciders,
        };

        let resolution = match resolve(
            self.ctx,
            &walk,
            &request.question,
            self.model.clone(),
            &self.config.interpretation,
            self.decisions,
        )
        .await
        {
            Ok(resolution) => resolution,
            Err(DecisionError::AlreadyReady) => return self.already_ready(pr, head_sha).await,
            Err(source) => {
                return Err(CapabilityError::DecisionUnresolved {
                    request: request.binding.request,
                    source,
                })
            }
        };

        let DecisionResolution {
            answer, ignored, ..
        } = resolution;

        let Some(HumanAnswer {
            interpreted,
            acted_on,
        }) = answer
        else {
            return Err(self.awaiting(
                &request,
                interaction,
                &ignored,
                "nobody who may decide has answered it yet".to_string(),
            ));
        };

        match (
            ResolvedDecision::approved(request.binding.clone(), &interpreted),
            &interpreted,
        ) {
            (Some(decision), _) => self.act_on(pr, head_sha, &decision).await,
            (None, InterpretedHumanDecision::Reject { reason }) => {
                Err(CapabilityError::DecisionRejected {
                    request: request.binding.request.clone(),
                    reason: reason.clone(),
                })
            }
            (None, InterpretedHumanDecision::Redirect { instruction }) => {
                self.redirect(work_id, head_sha, instruction, acted_on.comment, &ignored)
                    .await
            }
            (None, InterpretedHumanDecision::Unclear) => Err(self.awaiting(
                &request,
                interaction,
                &ignored,
                format!(
                    "comment {} could not be read as a decision, so the question stands",
                    acted_on.comment
                ),
            )),
            (None, InterpretedHumanDecision::Approve) => Err(self.awaiting(
                &request,
                interaction,
                &ignored,
                "an approval was read and could not be bound to the question it \
                 answered"
                    .to_string(),
            )),
        }
    }

    async fn redirect(
        &self,
        work_id: &str,
        head_sha: &str,
        instruction: &Published,
        comment: u64,
        declined: &[IgnoredReply],
    ) -> Result<EvidenceRef, CapabilityError> {
        self.receipts.lock().unwrap().push(EvidenceRef(format!(
            "{REDIRECT_ORIGIN}:{comment}:{}{}",
            one_line(instruction.as_str()),
            Self::and_who_was_not_counted(declined)
        )));

        let produced = self
            .produce_from(head_sha, Direction::Redirected(instruction.as_str()))
            .await?;
        let published = self.publish(&self.branch(), &produced.sha).await?;
        self.receipts.lock().unwrap().push(EvidenceRef(format!(
            "{PROPOSE_ORIGIN}:{}",
            produced.changed
        )));
        let asked = self.ask(work_id, published, &produced.sha).await;
        drop(produced.workspace);
        asked
    }

    async fn act_on(
        &self,
        pr: u64,
        head_sha: &str,
        decision: &ResolvedDecision,
    ) -> Result<EvidenceRef, CapabilityError> {
        let gated = self.gated(pr, head_sha);
        let receipt = self
            .propose_decided(
                EffectKind::EnsurePullRequestReady,
                gated.target(),
                gated.payload(),
                gated,
                decision,
            )
            .await?;
        Ok(receipt_evidence(
            EffectKind::EnsurePullRequestReady,
            &receipt,
        ))
    }

    async fn already_ready(&self, pr: u64, head_sha: &str) -> Result<EvidenceRef, CapabilityError> {
        let gated = self.gated(pr, head_sha);
        let receipt = self
            .propose(
                EffectKind::EnsurePullRequestReady,
                gated.target(),
                gated.payload(),
                gated,
            )
            .await?;
        Ok(receipt_evidence(
            EffectKind::EnsurePullRequestReady,
            &receipt,
        ))
    }

    fn record_change_set(&self, work_id: &str) -> Result<(), CapabilityError> {
        let state = ChangeSetState {
            marker: Some(correlation_key(
                &self.config.project,
                self.executor.invocation_ref(),
            )),
        };
        let destination = self
            .config
            .stub_root
            .join(format!("changes/{work_id}.json"));
        write_atomically(&destination, &state).map_err(|source| CapabilityError::Write {
            path: destination.clone(),
            source,
        })
    }

    fn awaiting(
        &self,
        request: &HumanDecisionRequest,
        interaction: InteractionRef,
        declined: &[IgnoredReply],
        because: String,
    ) -> CapabilityError {
        CapabilityError::AwaitingDecision {
            request: request.binding.request.clone(),
            interaction,
            question: format!(
                "{} — {because}{}",
                request.question,
                Self::and_who_was_not_counted(declined)
            ),
        }
    }

    fn and_who_was_not_counted(declined: &[IgnoredReply]) -> String {
        if declined.is_empty() {
            return String::new();
        }
        let each: Vec<String> = declined
            .iter()
            .map(|reply| {
                format!(
                    "comment {} by {} ({})",
                    reply.comment,
                    reply.author.id,
                    reply.reason.as_str()
                )
            })
            .collect();
        format!(
            "; {} read and not counted: {}",
            match each.len() {
                1 => "1 comment was".to_string(),
                many => format!("{many} comments were"),
            },
            each.join(", ")
        )
    }

    fn observe(&self) -> Publication {
        let source = || SourceRef(format!("github:{}", self.config.repo));
        let (branch, head_sha, pull_request, failure) = {
            let observed = self.observed.lock().unwrap();
            (
                observed.branch.clone(),
                observed.head_sha.clone(),
                observed.pull_request,
                observed.failure.clone(),
            )
        };

        let unreadable = |what: &str| {
            Published::of(match &failure {
                Some(why) => format!("{what}, so the forge was not read: {why}"),
                None => format!("{what}, so the forge was not read"),
            })
            .as_str()
            .to_string()
        };

        let review = match (&branch, head_sha.is_some()) {
            (Some(branch), true) => Observation::Available {
                value: ReviewState {
                    branch: Some(branch.clone()),
                    pull_request,
                    state: pull_request.map(|_| OPEN.to_string()),
                },
                source: source(),
                revision: head_sha.clone(),
            },
            _ => Observation::Unavailable {
                source: source(),
                reason: unreadable("no branch was observed"),
            },
        };

        Publication {
            review,
            verification: Observation::NotApplicable {
                reason: "propose_change requests no check, so it makes no claim about CI"
                    .to_string(),
            },
        }
    }

    async fn walk(
        &self,
        grant: &ExecutionGrant,
        work_id: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        let branch = self.branch();

        let opened = self
            .opened(&branch)
            .await
            .map_err(|error| adapter(EffectKind::EnsurePullRequest, error))?;
        if let Some(pull_request) = opened {
            let head_sha = self
                .head_of(pull_request.number)
                .await
                .map_err(|error| adapter(EffectKind::EnsurePullRequest, error))?;
            {
                let mut observed = self.observed.lock().unwrap();
                observed.branch = Some(branch.clone());
                observed.head_sha = Some(head_sha.clone());
                observed.pull_request = Some(pull_request.number);
            }
            let request = self.question_about(work_id, pull_request.number, &head_sha);
            let asking = PublishDecisionRequest::new(
                self.config.repo.clone(),
                pull_request.number,
                request.clone(),
            );

            let published = asking
                .inspect(self.ctx)
                .await
                .map_err(|error| adapter(EffectKind::PublishDecisionRequest, error))?;
            return match published {
                Some(published) => {
                    let evidence = self
                        .continue_from(
                            request,
                            published.into_value(),
                            pull_request.number,
                            &head_sha,
                            work_id,
                        )
                        .await?;
                    self.record_change_set(work_id)?;
                    Ok(evidence)
                }
                None => self.ask(work_id, pull_request.number, &head_sha).await,
            };
        }

        let produced = self.produce().await?;
        let pull_request = self.publish(&branch, &produced.sha).await?;
        self.receipts.lock().unwrap().insert(
            0,
            EvidenceRef(format!(
                "{PROPOSE_ORIGIN}:{}:{}",
                produced.changed,
                grant.attempt_id().0
            )),
        );
        let asked = self.ask(work_id, pull_request, &produced.sha).await;
        drop(produced.workspace);
        asked
    }
}

#[async_trait::async_trait]
impl<M> Capability for ProposeChange<'_, M>
where
    M: rig_core::completion::CompletionModel + 'static,
{
    fn id(&self) -> CapabilityId {
        fiddle_core::PROPOSE_CHANGE
    }

    fn stage(&self) -> &'static str {
        "propose"
    }

    async fn execute(
        &self,
        grant: ExecutionGrant,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        if grant.capability_id() != self.id() {
            return Err(CapabilityError::NotAuthorised {
                granted: grant.capability_id(),
                requested: self.id(),
            });
        }
        if invocation_ref != self.executor.invocation_ref()
            || !self.config.project_agrees_with(&self.executor)
        {
            return Err(CapabilityError::Misbound {
                bound: format!(
                    "{}/{}",
                    self.executor.project(),
                    self.executor.invocation_ref()
                ),
                asked: format!("{}/{invocation_ref}", self.config.project),
            });
        }
        let worktree = self.worktree();
        if self.ctx.work != worktree {
            return Err(CapabilityError::PublishesElsewhere {
                publishing: self.ctx.work.clone(),
                working: worktree,
            });
        }

        let outcome = self.walk(&grant, work_id).await;
        if let Err(error) = &outcome {
            self.observed.lock().unwrap().failure = Some(error.to_string());
        }
        *self.publication.lock().unwrap() = Some(self.observe());
        outcome
    }

    fn receipts(&self) -> Vec<EvidenceRef> {
        let mut evidence = tool_evidence(
            &self
                .tools
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        evidence.extend(self.receipts.lock().unwrap().iter().cloned());
        evidence
    }

    fn publication(&self) -> Option<Publication> {
        self.publication.lock().unwrap().clone()
    }
}

fn adapter(kind: EffectKind, error: GhError) -> CapabilityError {
    CapabilityError::Effect(crate::effect::EffectError::Adapter {
        kind,
        source: error,
    })
}

fn receipt_evidence<T>(kind: EffectKind, receipt: &EffectReceipt<T>) -> EvidenceRef {
    let outcome = match receipt.outcome {
        EffectOutcome::Committed => "committed",
        EffectOutcome::NotCommitted => "not_committed",
        EffectOutcome::Unknown => "unknown",
    };
    EvidenceRef(format!(
        "effect:{}:{}:{outcome}:{}:{}",
        kind.as_str(),
        receipt.effect_id.0,
        receipt.external_ref.as_deref().unwrap_or("-"),
        one_line(&receipt.postcondition),
    ))
}

fn tool_evidence(receipts: &ToolReceipts) -> Vec<EvidenceRef> {
    let mut counts: std::collections::BTreeMap<(&str, &str), usize> =
        std::collections::BTreeMap::new();
    for call in &receipts.calls {
        let tool = REGISTERED_TOOLS
            .iter()
            .find(|known| **known == call.tool)
            .copied()
            .unwrap_or(FOREIGN_TOOL);
        *counts.entry((tool, call.outcome)).or_default() += 1;
    }

    let mut evidence = vec![EvidenceRef(format!("tools:{}", receipts.calls.len()))];
    evidence.extend(
        counts
            .into_iter()
            .map(|((tool, outcome), count)| EvidenceRef(format!("tool:{tool}:{outcome}:{count}"))),
    );
    evidence
}

fn one_line(text: &str) -> String {
    let flattened: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    Published::of(flattened).as_str().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_worktree_is_named_after_the_branch_this_run_publishes() {
        let root = Path::new("/tmp/w");
        let path = attempt_worktree(root, "acme/widget", "beans:w-1");

        assert_eq!(path, root.join("fiddle-6d5aa806964432bc"));
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            branch_name("acme/widget", "beans:w-1").replace('/', "-"),
        );
        assert_eq!(
            path.parent(),
            Some(root),
            "one directory, not a nested pair"
        );
    }

    #[test]
    fn two_runs_do_not_share_a_worktree() {
        let root = Path::new("/tmp/w");
        assert_ne!(
            attempt_worktree(root, "acme/widget", "beans:w-1"),
            attempt_worktree(root, "acme/widget", "beans:w-2")
        );
        assert_ne!(
            attempt_worktree(root, "acme/widget", "beans:w-1"),
            attempt_worktree(root, "acme/other", "beans:w-1")
        );
    }
}
