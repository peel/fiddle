//! Produce a change, publish it as a draft, ask about it, and stop.
//!
//! The build's first **hybrid** capability: M1's bounded attempt produces the
//! change, M1's check decides whether it was earned, M2's operations publish it,
//! and M3's [`PublishDecisionRequest`] asks a person the one question fiddle is
//! not entitled to answer for itself. The walk is
//!
//! ```text
//! 1  a bounded attempt in a detached worktree, judged by this capability's own check
//! 2  EnsureBranchPublished    Automatic
//! 3  EnsurePullRequest        Automatic, draft: true
//! 4  PublishDecisionRequest   Automatic
//! 5  Err(AwaitingDecision) — Recurrence::Awaiting, exit 10, the process ends
//! ```
//!
//! and the transition out of draft — [`EnsurePullRequestReady`], the one `Human`
//! minimum in this build — is **not proposed here at all**. Five things about
//! that walk are worth stating, because each of them is a decision rather than an
//! implementation detail.
//!
//! # The gated effect is not proposed, rather than proposed and refused
//!
//! Proposing it and catching [`HumanDecisionRequired`](crate::effect::EffectError::HumanDecisionRequired) would also
//! suspend the run, and would be wrong in a way that only shows up in a
//! deployment document: `combine` answers [`Deny`](fiddle_core::PolicyDecision::Deny) before it
//! answers `RequireHumanDecision`, so an operator who wrote `deny` for
//! `ensure_pull_request_ready` would get a *denial* — exit 20, a concluded run —
//! where a question was owed. A run that has not asked anybody anything yet has
//! nothing for policy to be asked about, so it asks nobody.
//!
//! # M1's rule is unmoved: the check decides, and the model's claim is evidence
//!
//! [`RepairReport::claimed_complete`](crate::agent::RepairReport::claimed_complete)
//! travels exactly as far as [`CapabilityError::CheckFailed`], recorded beside
//! the exit code that overruled it, and no branch anywhere reads it. A capability
//! whose check did not pass publishes nothing and asks nothing — not a branch, not
//! a draft, and above all not a question, because a question is a claim that there
//! is something worth deciding about.
//!
//! # Which path a run takes is read out of the world, never remembered
//!
//! Nothing local is consulted: no marker, no flag, no journal. The capability
//! recomputes its own branch name from the run's canonical inputs, asks the forge
//! whether that branch already has an open pull request, and — when it has —
//! reads that pull request's conversation for a comment carrying *this run's*
//! request marker. So a fresh process with no history takes the correct branch,
//! which is the property M3 exists to prove.
//!
//! The read has a third answer, and it is the one nobody plans for: **a pull
//! request exists and no question has been asked about it**, which is where a
//! process interrupted between the create and the comment left the world. That
//! run resumes by asking, and deliberately does not attempt again — a second
//! attempt would produce a *different* commit, the push would then be a refused
//! non-fast-forward, and the run would be stuck for good. The change is already
//! out there; what is missing is the question.
//!
//! # The worktree is per-run, not per-attempt, because the publisher has to name it
//!
//! [`EnsureBranchPublished`] pushes `HEAD` out of [`EffectContext::work`], and
//! that context is built *before* this capability runs — before an
//! [`AttemptId`] exists, since one is minted inside
//! [`attempt`](crate::orchestration::attempt). A worktree at
//! `<root>/<attempt-id>` therefore cannot be the tree the push publishes, and a
//! run whose attempt worked in one tree while the push published another would
//! propose a commit nobody wrote.
//!
//! So the path is *derived*, by [`attempt_worktree`], from the same two canonical
//! inputs the branch name comes from. The process that builds the context and the
//! capability that creates the worktree call one function and cannot drift, which
//! is exactly the arrangement [`Executor::project`] exists for. What it costs is
//! that a worktree a crashed process left behind is not stepped over: `git
//! worktree add` refuses, and the run reports a correctable failure rather than
//! silently publishing from a tree somebody else's attempt was working in.
//!
//! [`ProposeChange::execute`] refuses outright when the context publishes from
//! anywhere other than the derived path, so the agreement is checked rather than
//! assumed.
//!
//! # No change set is recorded, on any path this capability has
//!
//! A correlation marker says *this invocation accounts for this work*, and the
//! next invocation's assessment completes on it without executing. A suspended
//! run has not earned that: the work is a question nobody has answered, and a
//! marker written here would make the very process that was supposed to read the
//! answer derive [`NextAction::Complete`](fiddle_core::NextAction) and never run.
//! The marker belongs after the transition a person approved, which is the
//! continuation's business.

use super::{Capability, CapabilityError, ExecutionGrant};
use crate::agent::{attempt, AgentBudget, ToolHost, ToolReceipts};
use crate::effect::{
    EffectContext, EffectOutcome, EffectReceipt, Executor, IntegrationOperation, ObservedState,
};
use crate::github::{
    branch_name, EnsureBranchPublished, EnsurePullRequest, EnsurePullRequestReady, GhError,
    PullRequest,
};
use crate::human::{InteractionRef, PublishDecisionRequest};
use crate::workspace::{Workspace, WorkspaceCommand, WorkspacePath};
use fiddle_core::{
    decision_request_id, effect_id, payload_hash, AttemptId, CapabilityId, DecisionBinding,
    EffectKind, EvidenceRef, HumanDecisionRequest, Observation, ProposedEffect, Publication,
    Published, ReviewState, SourceRef, WorkRef,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The origin this capability's earned evidence is named under.
const PROPOSE_ORIGIN: &str = "propose";

/// The state a pull request this run just observed is in.
///
/// A constant for [`super::publish`]'s reason: the lookup below is `state=open`
/// and a create answers with an open pull request, so the only pull request this
/// capability can settle on is an open one.
const OPEN: &str = "open";

/// The identity a commit fiddle makes carries.
///
/// Supplied with `-c` on the one `git commit` this capability runs rather than
/// read from a configuration file, because [`Workspace::run`] points `HOME` at a
/// throwaway directory: there is no `.gitconfig` for git to read, which is the
/// property that keeps an operator's global settings — and their signing key —
/// out of what fiddle commits. A deployment that wants its own name here is a
/// document key nobody has asked for yet.
const COMMITTER: [&str; 2] = ["user.name=fiddle", "user.email=fiddle@invalid"];

/// The tool names this crate registers, and the only ones an evidence reference
/// may repeat.
///
/// [`super::repair`]'s list and [`super::repair`]'s reason: `AuditHook` records a
/// call to a tool that does not exist under the name the **model** chose, which
/// is model-authored and unbounded, and evidence is published. The duplication is
/// deliberate — `publish_change` and `fixture_repair` are untouchable this
/// milestone, so the shared home for these two helpers has to wait for the next
/// change to one of them.
const REGISTERED_TOOLS: [&str; 4] = ["read_file", "write_file", "list_files", "run_check"];

/// What a call to anything outside [`REGISTERED_TOOLS`] is counted as.
const FOREIGN_TOOL: &str = "unregistered";

/// Where this run's attempt works, and the worktree whose `HEAD` the branch
/// effect publishes.
///
/// Derived from `(project, invocation_ref)` and from nothing else, so that the
/// caller building the [`EffectContext`] and the capability creating the worktree
/// arrive at one path without communicating — see this module's documentation for
/// why the [`AttemptId`] cannot be that path's name.
///
/// The leaf is [`branch_name`]'s own answer with its separator flattened, rather
/// than a second digest over the same inputs: a name derived *from* the branch
/// name cannot come to disagree with it, while a second derivation could. It is
/// `fiddle-` followed by 16 lowercase hex characters, so it is a legal directory
/// name for every possible input rather than for the well-behaved ones.
pub fn attempt_worktree(workspace_root: &Path, project: &str, invocation_ref: &str) -> PathBuf {
    workspace_root.join(branch_name(project, invocation_ref).replace('/', "-"))
}

/// Everything [`ProposeChange`] needs that is not the model and not the executor.
///
/// One struct rather than a dozen constructor arguments, for the reason
/// [`RepairConfig`](super::RepairConfig) and [`PublishConfig`](super::PublishConfig)
/// are each one: every field is a deployment decision an operator configures and
/// none is derivable from the others.
///
/// **No credential is in here, and there is nowhere to put one.** The `gh` and the
/// `git` that carry one were resolved before the executor was built and live
/// behind it.
pub struct ProposeConfig {
    /// `owner/name`, as the API path spells it.
    pub repo: String,

    /// The owner the head branch lives under. Separate from `repo`'s owner
    /// because a head may come from a fork, and because the label is what the
    /// pull request lookup matches on.
    pub head_owner: String,

    /// The branch the pull request is opened against.
    pub base: String,

    /// The pull request's title. Payload: read by people, hashed for
    /// detectability, matched on by nothing.
    pub title: String,

    /// The pull request's body. Payload, as above.
    pub body: String,

    /// The project half of the run's identity.
    ///
    /// Held even though [`Executor::project`] carries the same value, because
    /// [`Capability::execute`] refuses a configuration where the two differ
    /// rather than letting a run publish under a name its own effects were not
    /// derived from.
    pub project: String,

    /// The repository the attempt branches a worktree from, and never writes to.
    pub fixture: PathBuf,

    /// Where the per-run worktree is created. The path itself is
    /// [`attempt_worktree`]'s to derive.
    pub workspace_root: PathBuf,

    /// The check that decides whether this attempt earned anything.
    ///
    /// Run by this capability over the tree the attempt left, whatever the model
    /// said about itself — and run a second time even when the model ran it
    /// through `run_check`, because that result is a message in a transcript and
    /// this one is the verdict.
    pub check: WorkspaceCommand,

    /// What one bounded attempt runs inside. M1's five bounds, unwidened.
    pub budget: AgentBudget,

    /// Stops the attempt, the tools, the check and the commit together.
    pub cancel: tokio_util::sync::CancellationToken,
}

impl ProposeConfig {
    /// Whether the project this configuration names is the one the executor
    /// derives its effect identities from.
    fn project_agrees_with(&self, executor: &Executor<'_>) -> bool {
        self.project == executor.project()
    }
}

/// One change: produced, published as a draft, and asked about.
///
/// Borrows its executor and the context that executor was built from, and the
/// lifetime is the design. An *owned* [`EffectContext`] would be an owned
/// `gh` construction, which is a held credential; what this holds instead is
/// a borrow of something somebody else built, bound and resolved.
///
/// # Why the context is here at all, when [`super::publish`] manages without one
///
/// Because this capability has to *read* two things no effect reads for it: which
/// pull request its branch already has, and whether that pull request's
/// conversation already carries this run's question. Both decide whether the
/// attempt runs at all, so both happen before any effect is proposed, and
/// [`Executor`] offers no route to them — [`Executor::observe_checks`] is the
/// only read on it, and it answers a different question.
///
/// The narrower shape would be a read method on the executor per question, which
/// is what design §6.4 asked for and what `observe_checks` is the precedent for;
/// the narrowest would be a port with exactly these three reads on it. Neither is
/// written here, because `effect/mod.rs` is not this task's file and because the
/// continuation this capability grows into calls
/// [`validate::resolve`](crate::human::validate::resolve), whose implemented
/// signature takes an `&EffectContext` outright. What is done instead is to make
/// the borrow *checkable*: the context is required to publish from the worktree
/// this capability is about to create, so a context built for another run is
/// refused before anything happens rather than after something has.
pub struct ProposeChange<'a, M> {
    executor: Executor<'a>,
    /// The same context the executor was built with. See the type's
    /// documentation for what this is for and what it costs.
    ctx: &'a EffectContext,
    model: M,
    config: ProposeConfig,
    /// What each completed effect left behind, appended as it happens.
    ///
    /// Held here rather than only on the success path so [`Capability::receipts`]
    /// can read it after an execution that *suspended* — which for this
    /// capability is every successful one.
    receipts: Mutex<Vec<EvidenceRef>>,
    /// The record the tools append to. The host gets a clone of this same `Arc`,
    /// so there is one record and no copy-back step an early return could skip.
    tools: Arc<Mutex<ToolReceipts>>,
    /// What this run has established about the forge so far.
    observed: Mutex<Observed>,
    /// The pair the orchestration reads back after the execution.
    publication: Mutex<Option<Publication>>,
}

/// The external references a publication is described by, as they become known.
///
/// `None` means *not observed*, never *not there*: a run that never got past the
/// branch has not read the forge and found no pull request, and the difference is
/// what keeps [`ProposeChange::publication`] from claiming a review it did not
/// earn.
#[derive(Default)]
struct Observed {
    branch: Option<String>,
    head_sha: Option<String>,
    pull_request: Option<u64>,
    /// Why the forge could not be described, when it could not be.
    failure: Option<String>,
}

/// What an attempt left behind, once its check has passed.
///
/// The workspace travels with the commit because the push publishes out of it:
/// [`Workspace`] removes its worktree on `Drop`, so a value that dropped it here
/// would leave the branch effect pushing from a directory that no longer exists.
struct Produced {
    workspace: Arc<Workspace>,
    /// The commit the attempt's tree was committed as, read back from `git`.
    sha: String,
    /// How many files git saw change, for the evidence reference.
    changed: usize,
}

impl<'a, M> ProposeChange<'a, M>
where
    M: rig_core::completion::CompletionModel + 'static,
{
    /// A capability that will run `model` under `config`, proposing through
    /// `executor` and reading through `ctx`.
    ///
    /// The executor is expected to be bound to `propose_change`; one that is not
    /// is refused by the executor's own step 1 on the first proposal, which is
    /// the check that belongs to the executor rather than to its callers.
    pub fn new(
        executor: Executor<'a>,
        ctx: &'a EffectContext,
        model: M,
        config: ProposeConfig,
    ) -> Self {
        ProposeChange {
            executor,
            ctx,
            model,
            config,
            receipts: Mutex::new(Vec::new()),
            tools: Arc::new(Mutex::new(ToolReceipts::default())),
            observed: Mutex::new(Observed::default()),
            publication: Mutex::new(None),
        }
    }

    /// The branch this run publishes, recomputed the way a fresh process would.
    fn branch(&self) -> String {
        branch_name(self.executor.project(), self.executor.invocation_ref())
    }

    /// The worktree this run's attempt works in, and the tree the push publishes.
    fn worktree(&self) -> PathBuf {
        attempt_worktree(
            &self.config.workspace_root,
            self.executor.project(),
            self.executor.invocation_ref(),
        )
    }

    /// The draft pull request this run proposes, or looks for.
    ///
    /// Built here rather than at the two call sites so that the pull request the
    /// capability *finds* and the one it would *open* are the same head, the same
    /// base and the same lookup. `draft: true` is the whole of M3's change to
    /// M2's operation: the transition out of draft is the gated act, and this
    /// field is only the state the proposal starts in.
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

    /// The open pull request this run's branch already has, if it has one.
    ///
    /// Reached through [`EnsurePullRequest::inspect`] rather than through a read
    /// written here, so the number this capability decides on is the number the
    /// effect would settle on — including the refusal when two open pull requests
    /// match one head and base, which is a state to report and never a set to
    /// pick from.
    async fn opened(&self, branch: &str) -> Result<Option<PullRequest>, GhError> {
        self.draft_pull_request(branch).inspect(self.ctx).await
    }

    /// The revision one pull request's head is at.
    ///
    /// The pull request is the authority on its own head, which is why this reads
    /// the object rather than the ref: the gated effect is *this pull request at
    /// this revision*, and its identity is derived over what the pull request
    /// says. The same path [`EnsurePullRequestReady`] reads, so a world that can
    /// answer the continuation can answer this.
    ///
    /// Checked rather than defaulted. A `200` carrying no head sha is a `gh`
    /// answering something this client cannot read, and defaulting it would derive
    /// a request identity for a revision nobody named.
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

    /// The question this run would ask about `pr` at `head_sha`.
    ///
    /// # The request id is derived once and written twice
    ///
    /// [`HumanDecisionRequest`] carries it as its own `request` field *and* inside
    /// `binding`, and only `binding.request` is rendered into the marker — so a
    /// producer that filled the two from two derivations could publish a marker
    /// naming one question and then look for another, find nothing, conclude it
    /// had not asked yet, and post again on every attempt forever. Here the
    /// binding is built first and the outer field is a clone of its id, so the two
    /// cannot disagree; everything downstream reads
    /// [`PublishDecisionRequest`]'s own accessor, which reads the binding.
    ///
    /// The evidence is what this run has done so far, which makes the rendered
    /// body vary between a first run and a resumed one. That is safe and is worth
    /// saying why: the request's identity is derived from its *target* —
    /// `{repo}#{pr}:{request_id}` — so two bodies for one question are one effect,
    /// and step 3 recognises the comment that is already there rather than
    /// comparing what it would have written.
    fn question_about(&self, work_id: &str, pr: u64, head_sha: &str) -> HumanDecisionRequest {
        let repo = &self.config.repo;
        // The gated effect, built exactly as the continuation will build it, so
        // the identity a person is asked about is the identity that will be spent.
        let ready = EnsurePullRequestReady::new(repo.clone(), pr, head_sha.to_string());
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
            request: binding.request.clone(),
            invocation_ref: self.executor.invocation_ref().to_string(),
            work_ref: Some(WorkRef(work_id.to_string())),
            capability: self.id(),
            binding,
            question: format!("May fiddle mark pull request {repo}#{pr} ready for review?"),
            rationale: format!(
                "The change was produced by one bounded attempt and passed the check \
                 fiddle ran itself over the tree that attempt left. It is published as \
                 a draft at {head_sha}; marking it ready is the step fiddle will not \
                 take on its own."
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

    /// Propose one effect, record its receipt as evidence, and hand back the
    /// observed value.
    ///
    /// Every effect goes through here so that "the receipt reaches the bundle" is
    /// one statement rather than three that could disagree — including on the arm
    /// where the *next* effect fails, which is the arm the ordering property is
    /// about.
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
        let proposed = ProposedEffect {
            // Its own id, and not a parameter. A capability that could name the
            // proposing capability could propose in another's name; this one
            // cannot, and the executor refuses it at step 1 if it tries.
            capability: self.id(),
            kind,
            target,
            payload,
        };
        let receipt = self.executor.execute(proposed, operation).await?;
        self.receipts
            .lock()
            .unwrap()
            .push(receipt_evidence(kind, &receipt));
        Ok(receipt)
    }

    /// One bounded attempt, judged by this capability's own check, committed.
    ///
    /// The order is the rule: the attempt runs, the check runs over whatever tree
    /// it left, and only a check that exited 0 reaches the commit. A model that
    /// claimed completion over a tree the check refuses gets
    /// [`CapabilityError::CheckFailed`] carrying its claim beside the exit code
    /// that overruled it, and nothing is published.
    async fn produce(&self) -> Result<Produced, CapabilityError> {
        let worktree = self.worktree();
        // `Workspace::create` puts the worktree at `<root>/<name>`, so the derived
        // path is split into exactly that pair. The name is not an attempt id and
        // nothing treats it as one — it is a directory, and the attempt this
        // execution belongs to is the one on the grant, which the evidence in
        // [`ProposeChange::walk`] quotes.
        let root = worktree.parent().unwrap_or(&self.config.workspace_root);
        let name = AttemptId(
            worktree
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        );

        // Held for the whole of this execution and dropped at the end of it on
        // every path out, because the Drop guard is what removes the worktree —
        // and held *past this function*, because the push happens out of it.
        let workspace = Arc::new(Workspace::create(
            &self.config.fixture,
            root,
            &name,
            self.config.cancel.clone(),
        )?);

        let host = ToolHost {
            workspace: Arc::clone(&workspace),
            cancel: self.config.cancel.clone(),
            check: self.config.check.clone(),
            receipts: Arc::clone(&self.tools),
        };

        // One attempt, bounded. An attempt that failed produced no change, so
        // there is nothing below for the check to be a check *of*.
        let report = attempt(self.model.clone(), host, self.config.budget.clone()).await?;

        // Verified by the shell, independently, whatever the report said.
        let check = workspace.run(&self.config.check).await?;
        // Asked of git rather than of the report: a changed-file list the model
        // authored is a claim about a tree fiddle can go and look at.
        let changed = workspace.changed_files()?;

        if check.exit_code != 0 {
            return Err(CapabilityError::CheckFailed {
                claimed: report.claimed_complete,
                exit_code: check.exit_code,
                stderr: check.stderr,
            });
        }
        // A passing check over a tree nobody changed is not a change to propose.
        // Publishing an empty commit would ask a person to approve nothing, and
        // reporting success would account for work that was never done — so this
        // is neither, and a later attempt may still produce something.
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

    /// Commit exactly the files git saw change, and read back what that commit is.
    ///
    /// `add -f` over the named paths, and not `add -A`: the list comes from
    /// [`Workspace::changed_files`], which answers under the ignore rules the
    /// project had committed *before* the attempt began. An `add` that honoured
    /// the worktree's own rules would let an attempt that wrote `*` into
    /// `.gitignore` decide what gets published — and the check would then have
    /// passed over a tree that is not the tree the commit carries, which is the
    /// one disagreement this capability must not be able to publish.
    ///
    /// The bound is [`AgentBudget::tool_timeout`], the ceiling the host already
    /// set on any single program this attempt runs. A second bound would be a
    /// wall-clock policy nobody wrote down.
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

        // The sha is asked of git rather than derived, because a commit's identity
        // is a function of its content *and* of the moment it was made, and this
        // is the value the branch effect's payload names and its postcondition is
        // compared against.
        Ok(self
            .git(workspace, vec!["rev-parse".to_string(), "HEAD".to_string()])
            .await?
            .trim()
            .to_string())
    }

    /// One `git` inside the workspace, or the failure it reported.
    ///
    /// Through [`Workspace::run`], so it inherits the four-name environment, the
    /// working directory and the process-group bound every other program this
    /// attempt runs is subject to — a `git` spawned beside that would be a second
    /// environment to keep in step, which is what the workspace is one runner for.
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

    /// The branch, then the draft pull request, each one's receipt recorded
    /// before the next is proposed.
    ///
    /// A refusal returns, and that is the property rather than merely how `?`
    /// behaves: a draft pull request that policy denies must not be followed by a
    /// question, because the question would be about a proposal nothing has made.
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
            // The sha the remote was *observed* to hold, never the one the push
            // reported — which is the whole reason the receipt carries it.
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

    /// Publish the question, and stop.
    ///
    /// The [`InteractionRef`] comes off the receipt rather than being assembled
    /// here, so the conversation a suspended run names is the one the world was
    /// observed to hold the comment on.
    ///
    /// The id carried out is `binding.request` — read through the request's own
    /// binding, which is the field the marker is rendered from. Anything else
    /// would name a question that is not the one on the conversation.
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

    /// The branch a run takes when its question is already on the conversation.
    ///
    /// **Task 11b replaces this body**, with the validation order, the bounded
    /// interpretation, and the transition an approval alone authorises. Until
    /// then the honest answer is the one the run started with: the question
    /// stands, nobody's answer has been read, and the run is waiting — so it
    /// suspends again on the *same* request and posts nothing further, which is
    /// what [`PublishDecisionRequest`]'s own postcondition would enforce anyway.
    fn awaiting(
        &self,
        request: HumanDecisionRequest,
        interaction: InteractionRef,
    ) -> CapabilityError {
        CapabilityError::AwaitingDecision {
            request: request.binding.request,
            interaction,
            question: format!(
                "{} (this build publishes the question and does not yet read answers)",
                request.question
            ),
        }
    }

    /// Pair what reached the forge with what CI says, for a reader of a run that
    /// stopped part-way.
    ///
    /// Called on both arms — and for this capability *every* arm is one that
    /// stopped part-way, because a run that did everything asked of it still ends
    /// in an `Err`.
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

        // Bounded rather than trusted: it is rendered from an `EffectError` whose
        // `Adapter` arm quotes whatever came back from out there.
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
                    // Only alongside a pull request. A state naming no object
                    // would be this capability describing nothing.
                    state: pull_request.map(|_| OPEN.to_string()),
                },
                source: source(),
                revision: head_sha.clone(),
            },
            // Nothing was read back. `Unavailable` and never an `Available` review
            // with every field `None`, which would be the positive claim *the
            // forge was read and holds nothing*.
            _ => Observation::Unavailable {
                source: source(),
                reason: unreadable("no branch was observed"),
            },
        };

        Publication {
            review,
            // **`NotApplicable`, and that is a claim about this capability rather
            // than about CI.** `propose_change` requests no check — it publishes a
            // draft and asks a question — so "what does CI say about this head"
            // is a question it never asked. `Unavailable` would say the source
            // failed, and an `Available` empty `VerificationState` would read as
            // *nothing is failing*, which is the collapse M0's fail-closed rule
            // exists to prevent. `publish_change` is the capability that dispatches
            // a check and therefore the one that answers for one.
            verification: Observation::NotApplicable {
                reason: "propose_change requests no check, so it makes no claim about CI"
                    .to_string(),
            },
        }
    }

    /// The walk, once the run has been proved to be the one this capability was
    /// built for.
    ///
    /// Separated from [`Capability::execute`] so that the publication is observed
    /// on every arm of it from one place, rather than at each of the returns
    /// below.
    async fn walk(
        &self,
        grant: &ExecutionGrant,
        work_id: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        let branch = self.branch();

        // The world decides which path this is, and nothing local is consulted.
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

            // Reached through the operation's own `inspect`, so "is this run's
            // question already on the conversation" is answered by the same
            // marker comparison, the same page bound and the same duplicate
            // refusal the effect itself would apply.
            let published = asking
                .inspect(self.ctx)
                .await
                .map_err(|error| adapter(EffectKind::PublishDecisionRequest, error))?;
            return match published {
                Some(published) => Err(self.awaiting(request, published.into_value())),
                // A pull request with no question on it: a run that stopped
                // between the create and the comment. The change is already
                // published, so this resumes by asking rather than by attempting
                // again — see this module's documentation for what a second
                // attempt would cost.
                None => self.ask(work_id, pull_request.number, &head_sha).await,
            };
        }

        // A first run. The attempt is the only thing here that can produce a
        // change, and its workspace is held until the push has happened.
        let produced = self.produce().await?;
        let pull_request = self.publish(&branch, &produced.sha).await?;
        // Named before the question is asked, so a reader of the receipts can see
        // what the attempt did even on the arm where the comment never landed.
        self.receipts.lock().unwrap().insert(
            0,
            EvidenceRef(format!(
                "{PROPOSE_ORIGIN}:{}:{}",
                produced.changed,
                grant.attempt_id().0
            )),
        );
        let asked = self.ask(work_id, pull_request, &produced.sha).await;
        // Explicit rather than left to the end of the scope: the worktree is
        // removed here, *after* the push and after the question, and a reader
        // should not have to work out from a `Drop` where that happens.
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

    /// This capability's own word for its own step, beside M0's `mark`, M1's
    /// `repair` and M2's `publish`. There is no neutral stage name, which is why
    /// [`Capability::stage`] has no default and why a `fixture_repair` run once
    /// published `stage: "mark"`.
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
        // The executor was built for a run, and this is that run — or this
        // capability publishes under one name while the bundle and the journal are
        // filed under another. Checked before anything is read, so a misbound
        // executor provably reaches no forge.
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
        // And the tree the push publishes is the tree the attempt will work in.
        // See this module's documentation: the two are one derived path, and a
        // context pointed anywhere else would publish a commit this run never
        // made.
        let worktree = self.worktree();
        if self.ctx.work != worktree {
            return Err(CapabilityError::PublishesElsewhere {
                publishing: self.ctx.work.clone(),
                working: worktree,
            });
        }

        let outcome = self.walk(&grant, work_id).await;
        // Recorded before the forge is described, so an `Unavailable` review can
        // say *why* rather than only that it saw nothing.
        if let Err(error) = &outcome {
            self.observed.lock().unwrap().failure = Some(error.to_string());
        }
        // Read on both arms and before the result is propagated: whatever the run
        // concluded, what reached the forge is what a reader has to be told.
        *self.publication.lock().unwrap() = Some(self.observe());
        outcome
    }

    /// The tool summary first, then one reference per effect that produced a
    /// receipt, in the order they were proposed.
    ///
    /// Both halves, because this capability has both: an attempt whose model
    /// called no tool at all is the shape of M1's central defect and is invisible
    /// from outside a process unless something says so, and an effect that reached
    /// the forge is what an operator opens.
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

/// Carry an adapter failure from one of this capability's own reads into the
/// capability vocabulary.
///
/// A read that failed is not an absence — the rule M2's operations each state for
/// themselves — and here it decides *which path the run takes*, so reading an
/// outage as "no pull request yet" would run a second attempt over work that is
/// already published and then fail to push it. It becomes an
/// [`EffectError::Adapter`](crate::effect::EffectError::Adapter) so that the
/// recurrence table, and therefore the exit code, is the one every other adapter
/// failure in the run gets.
///
/// The kind is the effect the read was *about* rather than one blanket value: a
/// diagnostic that named the question comment when the pull request lookup was
/// what failed would send an operator to the wrong endpoint.
fn adapter(kind: EffectKind, error: GhError) -> CapabilityError {
    CapabilityError::Effect(crate::effect::EffectError::Adapter {
        kind,
        source: error,
    })
}

/// One effect receipt, as an evidence reference a bundle can carry.
///
/// [`super::publish`]'s renderer and its argument, duplicated rather than shared
/// because `publish_change` is not modified this milestone: its canonical strings
/// gate four of M2's suites, and moving a private helper out of it would touch a
/// file this task has no business touching. The next change to either capability
/// is where the two converge.
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

/// One attempt's tool receipts, summarised.
///
/// [`super::repair`]'s summary, duplicated for the reason above. The leading
/// `tools:<n>` is emitted **even when `n` is zero**, which is the whole reason it
/// exists: an attempt in which the model called nothing is the exact shape of the
/// defect that made every model on the gateway fail, and it is invisible from
/// outside a process unless something says so out loud.
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

/// Bound and flatten one externally-authored string.
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

    /// The worktree name is the branch name with its one separator flattened, so
    /// it is a single directory rather than a nested pair — and it is derived from
    /// the branch name itself rather than from a second hash over the same
    /// inputs, which is what stops the two from drifting apart.
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

    /// Two runs of one project are two worktrees, or one would publish the
    /// other's commit.
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
