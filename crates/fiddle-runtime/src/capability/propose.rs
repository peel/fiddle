//! Produce a change, publish it as a draft, ask about it, stop — and come back for
//! the answer.
//!
//! The build's first **hybrid** capability: M1's bounded attempt produces the
//! change, M1's check decides whether it was earned, M2's operations publish it,
//! and M3's [`PublishDecisionRequest`] asks a person the one question fiddle is
//! not entitled to answer for itself. The walk a *first* run takes is
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
//! minimum in this build — is **not proposed there at all**. A later run, with no
//! memory of that one, finds the question already on the conversation and takes the
//! other walk:
//!
//! ```text
//! 1  validate::resolve — eight steps, of which six are deterministic and precede
//!    the one bounded model call
//! 2  Approve   → EnsurePullRequestReady through Executor::execute_decided, Ok
//!    Reject    → Err(DecisionRejected)  — Permanent, exit 20, a person said no
//!    Redirect  → Err(AwaitingDecision)  — Awaiting, exit 10, naming the instruction
//!    Unclear   → Err(AwaitingDecision)  — Awaiting, exit 10, and nothing is posted
//!    no answer → Err(AwaitingDecision)  — Awaiting, exit 10, the question stands
//! ```
//!
//! **That approve arm is the only production caller of
//! [`Executor::execute_decided`] in this build**, and therefore the only path on
//! which an operation declaring a `Human` minimum ever commits. A decided path
//! nothing walks is the shape M2's `RequireHumanDecision` was criticised for, and
//! this is what stops that criticism applying twice.
//!
//! # A continuation holds no workspace, and must not need one
//!
//! [`EffectContext::work`] is the worktree a push publishes from, and
//! [`EnsureBranchPublished`] is the only operation that reads it. The approve,
//! reject, redirect and unclear paths propose no such operation — the branch is
//! already published, and the two `gh` calls involved touch no checkout — so a
//! process that cannot create a workspace at all completes them. That is not a
//! convenience: it is what makes a continuation from a fresh process, after the
//! first run's worktree was removed, something the suite can prove offline.
//!
//! Six things about the two walks are worth stating, because each is a decision
//! rather than an implementation detail.
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
//! # A change set is recorded on the one arm that concluded, and on no other
//!
//! A correlation marker says *this invocation accounts for this work*, and the
//! next invocation's assessment completes on it without executing. A suspended
//! run has not earned that: the work is a question nobody has answered, and a
//! marker written there would make the very process that was supposed to read the
//! answer derive [`NextAction::Complete`](fiddle_core::NextAction) and never run.
//! That is the prohibition, it is about *which* arm rather than about the file,
//! and it has not moved.
//!
//! What has moved is the other side of it. This capability once recorded nothing
//! **on any path**, including the path on which a person's approval had been read
//! and the transition performed. That was not a design: it was a source-level test
//! asserting the file named none of the machinery — since reworded to
//! `the_capability_holds_no_credential_and_accounts_for_work_in_one_place`, which is
//! where the split between the two halves is argued — held in place because a
//! converged sibling's evaluation had passed the property as it stood. The debt is
//! paid — `fiddle-usp7` — and what it cost while it stood is worth recording,
//! because the second cost is the one nobody predicted:
//!
//! - **A caller retrying never terminated.** The capability completed, the
//!   transition landed exactly once, and the post-execution re-derivation then
//!   found the work unaccounted for and concluded
//!   [`RunOutcome::Retryable`](fiddle_core::RunOutcome) — exit 11, from a
//!   process that had mutated nothing.
//! - **Exit 11 meant two things at once, and that silently weakened every test
//!   written after it.** It was what a *successful* continuation earned, and also
//!   what a continuation that refused at step 5 earned, since an unreadable
//!   comment is an adapter failure and adapter failures are retryable. A test
//!   asserting the code and not the effect could not tell *the transition
//!   happened* from *the conversation could not be read* — measured, not reasoned:
//!   three of `fiddle-565u`'s inversions over the by-id read came back green
//!   against an acceptance test that asserted only the number.
//!
//! [`ProposeChange::walk`] is the one place the write happens and states which two
//! arms reach it; [`ProposeChange::record_change_set`] is what it writes.

use super::stub::write_atomically;
use super::{Capability, CapabilityError, ExecutionGrant};
use crate::agent::{attempt, AgentBudget, ToolHost, ToolReceipts};
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
use crate::human::{InteractionRef, PublishDecisionRequest};
use crate::workspace::{Workspace, WorkspaceCommand, WorkspacePath};
use fiddle_core::{
    correlation_key, decision_request_id, effect_id, payload_hash, AttemptId, CapabilityId,
    ChangeSetState, DecisionBinding, EffectKind, EvidenceRef, HumanDecisionRequest,
    InterpretedHumanDecision, Observation, ProposedEffect, Publication, Published, ReviewState,
    SourceRef, WorkRef,
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

/// How many pages of a conversation the continuation may read before the read is a
/// refusal rather than a truncation.
///
/// **The same bound [`PublishDecisionRequest::inspect`] applies**, and it has to
/// be: this capability asks one conversation two questions — *is my question
/// already here* through the operation's own `inspect`, and *has anybody answered
/// it* through [`resolve`] — and a run whose two reads saw different amounts of
/// the same conversation could find its question and then miss the reply below it.
///
/// It is stated twice because `human`'s copy is a private module constant and
/// `human/mod.rs` is not this task's file. That is a duplication to collapse the
/// next time either file is touched, by making the constant there `pub`; until
/// then the drift risk is recorded here rather than left for somebody to find. It
/// is not a deployment key: nobody has asked to configure how much of a
/// conversation fiddle reads, and a document value that could disagree with the
/// operation's own bound would be worse than a constant that cannot.
const CONVERSATION_PAGES: u32 = 10;

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
///
/// # This resolves a tension rather than working around one
///
/// [`EffectContext::work`] already says what it is, in its own words:
///
/// > The worktree whose `HEAD` is published. **One per run**, because an attempt
/// > works in one checkout; an operation that could name another would be naming
/// > work this run never did.
///
/// *Per run* — which is what this function derives, and what
/// [`Workspace::create`]'s `<root>/<attempt-id>` is not. The field's claim and
/// the workspace's path disagreed, and the field was the aspirational half:
/// there was no run-scoped path for a caller to point it at. This is that path,
/// so the sentence above is now true of the value it describes.
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

    /// Where the change set a concluded continuation records is written.
    ///
    /// The same field [`PublishConfig`](super::PublishConfig) and
    /// [`RepairConfig`](super::RepairConfig) carry, resolved from the same
    /// `[stub] root` document key, because the file is read by one reader that
    /// does not know which capability wrote it — see
    /// [`ProposeChange::record_change_set`].
    pub stub_root: PathBuf,

    /// The check that decides whether this attempt earned anything.
    ///
    /// Run by this capability over the tree the attempt left, whatever the model
    /// said about itself — and run a second time even when the model ran it
    /// through `run_check`, because that result is a message in a transcript and
    /// this one is the verdict.
    pub check: WorkspaceCommand,

    /// What one bounded attempt runs inside. M1's five bounds, unwidened.
    pub budget: AgentBudget,

    /// The immutable numeric ids of the people this deployment nominated to
    /// decide.
    ///
    /// Ids and not logins, and not `author_association` either. A login can be
    /// changed and the vacated name reclaimed, so an allowlist matching one would
    /// let a renamed-and-reclaimed account inherit an approver's authority; and an
    /// association says what somebody's relationship to the repository is rather
    /// than whether *this deployment* nominated them. The design records declining
    /// the `collaborators/{user}/permission` endpoint in favour of exactly this
    /// list.
    ///
    /// An empty list authorizes nobody, and the type admits one because nothing
    /// at this layer can refuse it — a `Vec` is a `Vec`. **No document can express
    /// one**, though: `[github.decision] authorized` has no default and is refused
    /// empty at the parse boundary, because a deployment that can publish a
    /// question and can never accept an answer suspends every run for ever.
    ///
    /// So the empty case is not a deployment a reader will meet in a file, and it
    /// is still worth pinning: `decision_protocol`'s
    /// `a_deployment_that_nominated_nobody_authorizes_nobody` drives it, because
    /// the failure that matters is the opposite one — a check deleted from the walk
    /// authorizes *everybody*, and the schema refusing an empty list must not be
    /// the only thing standing between a caller and that.
    pub deciders: Vec<u64>,

    /// What the one interpretation call runs inside.
    ///
    /// Beside [`ProposeConfig::budget`] rather than folded into it, because the
    /// two bound different things: that one bounds a tool-using attempt over a
    /// checkout, and this one bounds a single completion that is handed one
    /// comment and answers with one small object. There is no turn count here,
    /// and its absence is [`InterpretationBounds`]'s own decision — a second turn
    /// would be a second chance at an approval.
    pub interpretation: InterpretationBounds,

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
    /// Where the validation order writes down which of its eight steps it is on.
    ///
    /// A borrow held for the run, exactly as [`Executor`] holds its
    /// [`EffectTrace`](crate::effect::EffectTrace), and required rather than
    /// defaulted for that trait's reason: a sink that discarded by omission would
    /// let a production path go dark without anybody deciding it should. The two
    /// traits are separate — see [`DecisionTrace`] — so a caller supplies both,
    /// and in this build one value implements both and the two orders end up in
    /// one place.
    decisions: &'a dyn DecisionTrace,
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

    /// The effect a person is being asked about, rebuilt from canonical inputs.
    ///
    /// One function for both of the runs that need it — the one that asks and the
    /// one that acts — so the identity and the payload a person was shown are the
    /// identity and the payload that get spent. Two constructions of the same
    /// operation would agree today and have no reason to keep agreeing, and the
    /// disagreement would be invisible: the identity is derived over the target
    /// and never over the payload, so a widened payload arrives looking like the
    /// same work. The executor's step 4 catches that, and this is what makes it
    /// something that never has to.
    fn gated(&self, pr: u64, head_sha: &str) -> EnsurePullRequestReady {
        EnsurePullRequestReady::new(self.config.repo.clone(), pr, head_sha.to_string())
    }

    /// The question this run would ask about `pr` at `head_sha`.
    ///
    /// # The request id is derived once and written once
    ///
    /// [`decision_request_id`] is called once, into `binding.request`, which is the
    /// only place [`HumanDecisionRequest`] holds it and the only one
    /// [`fiddle_core::render_marker`] can put on a conversation. A producer that
    /// derived it twice could publish a marker naming one question and then look for
    /// another, find nothing, conclude it had not asked yet, and post again on every
    /// attempt forever — so there is one derivation here, and everything downstream
    /// reads [`PublishDecisionRequest`]'s own accessor, which reads the binding.
    ///
    /// The evidence is what this run has done so far, which makes the rendered
    /// body vary between a first run and a resumed one. That is safe and is worth
    /// saying why: the request's identity is derived from its *target* —
    /// `{repo}#{pr}:{request_id}` — so two bodies for one question are one effect,
    /// and step 3 recognises the comment that is already there rather than
    /// comparing what it would have written.
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
        self.proposing(kind, target, payload, operation, None).await
    }

    /// The same, with an approval a person gave available to the executor's step 4.
    ///
    /// The only route in this build to
    /// [`Executor::execute_decided`], and therefore the only
    /// path on which an operation whose
    /// [`IntegrationOperation::minimum`] is
    /// [`Human`](fiddle_core::HumanDecisionRequirement::Human) can commit. What
    /// makes it safe is that the decision is not believed here: the executor
    /// re-derives this effect's identity and payload digest for itself and
    /// compares both against the approval, so a decision given for another
    /// question or another request refuses inside the executor rather than being
    /// vouched for by this caller.
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

    /// One proposal, whichever of the executor's two entry points it goes
    /// through.
    ///
    /// One private body rather than two public ones, for the reason
    /// [`Executor::walk`](crate::effect::Executor) gives about itself: the thing
    /// that must not vary between the decided path and the undecided one is
    /// everything except step 4's third input. Here that is the proposal's
    /// construction and the recording of its receipt — and the recording is the
    /// half worth insisting on, because "the receipt reaches the bundle" has to
    /// stay one statement rather than two that could disagree on the arm where
    /// the *next* effect fails.
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
            // Its own id, and not a parameter. A capability that could name the
            // proposing capability could propose in another's name; this one
            // cannot, and the executor refuses it at step 1 if it tries.
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

    /// The branch a run takes when its question is already on the conversation:
    /// read the answer, and act only on an approval.
    ///
    /// # Nothing here decides anything the shell has not already decided
    ///
    /// [`resolve`] owns the whole eight-step order — which comment is this run's
    /// question, whether the marker authenticates against a recomputed identity,
    /// which replies are candidates, whether any of them changed since they were
    /// listed, whether the pull request is still open at the revision it was asked
    /// about, and only then the one bounded model call. This function *calls* it
    /// and branches on what it answered. That is the difference the criterion is
    /// about: an unauthorized reply reaches no model on this path either, not
    /// because the capability checks an allowlist but because the walk it delegates
    /// to does, before the model exists.
    ///
    /// # Four verdicts, and exactly one of them mutates
    ///
    /// The branch is written as the *conversion* rather than as four arms that
    /// agree to behave: [`ResolvedDecision::approved`] is the only constructor of
    /// step 4's third input and it takes the verdict, so
    /// [`Reject`](InterpretedHumanDecision::Reject),
    /// [`Redirect`](InterpretedHumanDecision::Redirect) and
    /// [`Unclear`](InterpretedHumanDecision::Unclear) have no spelling that
    /// reaches the executor's decided path. The binding it is given is the one
    /// **this process derived**, in [`ProposeChange::question_about`], and never
    /// the one parsed off the comment — the marker is what a forger can copy, and
    /// [`resolve`]'s steps 3 and 8 are what established that the conversation's
    /// copy agrees with this derivation.
    ///
    /// A rejection concludes the run. A redirect and an unclear reply leave it
    /// waiting on the *same* request and publish nothing: the effect has not
    /// moved, so the request identity has not moved, so
    /// [`PublishDecisionRequest`]'s own postcondition would suppress a second post
    /// anyway. A follow-up comment would need a second identity for the same
    /// question, which is an effect kind this build does not have.
    async fn continue_from(
        &self,
        request: HumanDecisionRequest,
        interaction: InteractionRef,
        pr: u64,
        head_sha: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        let gated = self.gated(pr, head_sha);
        let target = gated.target();
        let payload = gated.payload();
        let walk = DecisionWalk {
            repo: &self.config.repo,
            pr,
            max_pages: CONVERSATION_PAGES,
            // From the executor, like every other identity this capability
            // derives, so a misbound run cannot resolve a decision under a name
            // its own effects were not derived from.
            project: self.executor.project(),
            invocation_ref: self.executor.invocation_ref(),
            kind: EffectKind::EnsurePullRequestReady,
            target: &target,
            // The bytes and not the digest: step 8's arithmetic belongs inside
            // step 8, and a caller that hashed it here would be doing the
            // comparison's half outside the walk that reports it.
            payload: &payload,
            allowlist: &self.config.deciders,
        };

        let resolution = match resolve(
            self.ctx,
            &walk,
            // fiddle's own text, and the same string the request comment carries.
            // `interpret` takes the question as text so that it cannot receive an
            // identity; see `human::validate`'s documentation before composing
            // anything into this.
            &request.question,
            self.model.clone(),
            &self.config.interpretation,
            self.decisions,
        )
        .await
        {
            Ok(resolution) => resolution,
            // The one refusal that is not a refusal. See
            // [`ProposeChange::already_ready`].
            Err(DecisionError::AlreadyReady) => return self.already_ready(pr, head_sha).await,
            Err(source) => {
                return Err(CapabilityError::DecisionUnresolved {
                    request: request.binding.request,
                    source,
                })
            }
        };

        // Destructured rather than read field by field, because `ignored` has to
        // outlive the `answer` this moves out: every arm below reports who was
        // declined, and a partial move would make the one arm that did not the
        // easiest to write.
        let DecisionResolution {
            answer, ignored, ..
        } = resolution;

        let Some(HumanAnswer {
            interpreted,
            acted_on,
        }) = answer
        else {
            // Not a refusal and not a model call: nobody the deployment nominated
            // has replied, which is the state a suspended run exists in.
            //
            // **And "nobody" is exactly what a reader must not be left with when
            // somebody did answer.** A stranger's approval, a bot's, an app's are all
            // read, declined, and — until this arm carried them — dropped, so the run
            // reported an unanswered question against a conversation that had three
            // replies in it. `ignored` is where the walk wrote down which and why.
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
            (None, InterpretedHumanDecision::Redirect { instruction }) => Err(self.awaiting(
                &request,
                interaction,
                &ignored,
                format!(
                    "comment {} asks for something else instead, and attempting again \
                     is not implemented: {instruction}",
                    acted_on.comment
                ),
            )),
            (None, InterpretedHumanDecision::Unclear) => Err(self.awaiting(
                &request,
                interaction,
                &ignored,
                format!(
                    "comment {} could not be read as a decision, so the question stands",
                    acted_on.comment
                ),
            )),
            // Unreachable by construction: `approved` answers `Some` for exactly
            // this verdict. It is answered rather than left to `unreachable!()`
            // because a panic inside a capability takes the whole run's record
            // with it, and the fail-closed answer — nothing performed, the
            // question still standing — is available and costs a line.
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

    /// Perform the transition a person authorised.
    ///
    /// The **only** production path in this build that reaches
    /// [`Executor::execute_decided`], and therefore the only one on which an
    /// operation declaring a `Human` minimum commits. The operation is rebuilt
    /// here from [`ProposeChange::gated`] rather than carried in from the
    /// question, so the target and the payload the executor compares the approval
    /// against come from the same one derivation the question was asked about.
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

    /// The pull request is already out of draft, so the transition this run was
    /// about has happened.
    ///
    /// [`DecisionError::AlreadyReady`] is a refusal of the *validation order* —
    /// there is no decision left to establish — and it is not a failure of the
    /// run. An invocation reaching here has walked the whole thing again: it found
    /// its own pull request, derived the same question, found the same comment, and
    /// the walk then refused because the object is no longer a draft. Reporting
    /// that as an error would make a completed run fail on its next invocation.
    ///
    /// # When this is reached, now that a concluded arm records a change set
    ///
    /// Not on an ordinary repeat. [`ProposeChange::walk`] records the marker when
    /// this arm answers `Ok`, so the *next* invocation's pre-execution derivation
    /// answers [`NextAction::Complete`](fiddle_core::NextAction) and this capability
    /// is never granted at all. What is left is the residual case — a change set
    /// lost, a `[stub] root` moved, or a pull request somebody took out of draft by
    /// hand — and the answer here has to be right for it, which is why this arm
    /// accounts for the work exactly as the approve path does. Withholding the
    /// marker on the ground that this invocation performed nothing would leave that
    /// case reporting `Retryable` for ever over work that is done: the livelock
    /// `fiddle-usp7` fixed, surviving in the one place still able to reach it.
    ///
    /// This is also why the sentence that used to open this comment — *"this
    /// capability records no change set on any path, so a later invocation walks the
    /// whole thing again"* — was not merely stale but the wrong argument. It offered
    /// the absent marker as the *reason* this arm is survivable, when the absent
    /// marker was the thing making the run report `Retryable`.
    ///
    /// So the effect is proposed through the **undecided** entry point, and the
    /// executor's own ordering is what makes that correct rather than a way of
    /// slipping past the gate: step 3 inspects the postcondition *before* step 4
    /// combines policy, so an already-ready pull request settles at step 3 with
    /// [`EffectOutcome::Committed`] and no decision is required to *observe* a
    /// completed effect. [`EnsurePullRequestReady::inspect`] states that ordering
    /// and `ready_effect.rs`'s
    /// `an_already_ready_pull_request_is_the_postcondition` pins it.
    ///
    /// Nothing can be mutated here. If the pull request were re-drafted between
    /// the walk's read and this one, step 3 would answer *absent*, step 4 would
    /// find a `Human` minimum with no decision in hand, and the effect would
    /// refuse as
    /// [`HumanDecisionRequired`](crate::effect::EffectError::HumanDecisionRequired)
    /// — `Awaiting`, exit 10, go and answer the current question.
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

    /// Record this invocation's correlation key as the change set for the work
    /// item.
    ///
    /// Deliberately identical to what [`super::publish`], [`StubMark`](super::StubMark)
    /// and [`FixtureRepair`](super::FixtureRepair) write, through the same atomic
    /// write and to the same derived path: the assessment that reads it does not
    /// know or care which capability produced it, and four capabilities writing
    /// subtly different files for one reader is a defect waiting for a change of
    /// capability to expose it.
    ///
    /// Called from [`ProposeChange::walk`] and from nowhere else. See the call
    /// site for which arms reach it and why the two that do have equal claim.
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

    /// The run is waiting, and this is what it is waiting for.
    ///
    /// `because` says which of the four ways of not having an answer this is, and
    /// it rides in the `question` field because that is what a run outcome's
    /// `reason` is built from: a diagnostic naming only the request id would leave
    /// a reader of the `--json` payload with nothing to act on.
    ///
    /// The id carried out is `binding.request` — the one field the marker is
    /// rendered from, and so the only id a person or a later process can find the
    /// question by. A run that reported any other would name a question nobody can
    /// look up.
    ///
    /// # `declined` is the half a reader was previously not told
    ///
    /// `resolve` records every comment it read and did not count, with the reason, and
    /// nothing consumed it: `IgnoredReply` was built in `human::validate` and read
    /// nowhere else in the workspace, so a suspension announced *"nobody who may decide
    /// has answered it yet"* against a conversation that might hold three replies from
    /// three people who were each declined for a different reason. "Nobody answered"
    /// and "somebody answered and may not decide" are different states and only one of
    /// them is one an operator can fix.
    ///
    /// Every entry is reported, **including fiddle's own question**, which
    /// `select_candidates` declines as `Ignored::RequestComment`. Filtering it would be
    /// this function deciding which of the walk's observations a reader deserves, and
    /// the distinct reasons are what let a reader tell fiddle's own question from
    /// somebody who tried to answer — which is the whole reason
    /// [`Ignored`](crate::human::validate::Ignored) is a closed enum with one spelling
    /// each rather than a written-out string.
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

    /// The declined replies, as a reader sees them: which comment, who wrote it, and
    /// why it was not counted.
    ///
    /// # Three things this rendering has to get right
    ///
    /// **The reason is the reason and never the author.** Each entry carries the
    /// comment number, then the author's immutable numeric id — which is the field the
    /// allowlist matches, so it is the one an operator would edit — and then
    /// `Ignored::as_str`. An id where a reason belongs would be worse than the silence
    /// this replaces, because silence does not mislead.
    ///
    /// **The reasons stay distinct.** They are taken from `Ignored::as_str` rather than
    /// re-worded here, so the three spellings
    /// `every_reason_a_reply_was_declined_has_exactly_one_spelling` keeps apart are the
    /// three a reader sees. Two reasons collapsing into one phrase would leave an
    /// operator unable to tell "not on the allowlist" — which they can fix by editing
    /// the allowlist — from "not a person", which they cannot.
    ///
    /// # The empty branch is **unreachable in this build**, and is kept deliberately
    ///
    /// Said plainly because an earlier version of this doc described it as a case — *"a
    /// conversation nobody has written on still reads as the plain statement that nobody
    /// has answered"* — and that case cannot occur. `select_candidates` declines the
    /// request comment itself as `Ignored::RequestComment`, and the request comment is by
    /// construction on the conversation the walk just found it in, so `declined` holds at
    /// least one entry at every one of [`ProposeChange::awaiting`]'s call sites. Verified
    /// by inversion: `panic!` in the branch below fails no test.
    ///
    /// It is kept for the reason an unreachable fail-closed arm is worth keeping — the
    /// alternative is not "no branch", it is falling through to the formatting below with
    /// nothing to format, which renders *"; 0 comments were read and not counted: "*: a
    /// sentence that states a count nobody asked for and then trails off after a colon.
    /// A reader would be worse served by that than by the plain statement, and it would
    /// be a defect introduced by tidiness.
    ///
    /// It becomes reachable the moment `select_candidates` stops declining the request
    /// comment, which is a change somebody could reasonably make — the request comment is
    /// not a *reply* in any sense a reader cares about — so this is a guard with a
    /// foreseeable future rather than dead code.
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

    /// Pair what reached the forge with what CI says.
    ///
    /// Called on both arms of [`Capability::execute`], and both arms are now
    /// reachable: a run that asks a question ends in `Err(AwaitingDecision)` and a
    /// run that finds an approval ends in `Ok`. Until the continuation existed
    /// every arm this capability had stopped part-way, and this comment said so;
    /// the approve path is what made that false.
    ///
    /// Which is why the read stays on both arms rather than moving to the failure
    /// path: what reached the forge is what a reader has to be told either way, and
    /// a `Publication` observed only when something went wrong would leave the one
    /// run that completed unable to say what it published.
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
                // The question is out there. Whether anybody has answered it is
                // the conversation's to say, and the validation order's to read.
                Some(published) => {
                    let evidence = self
                        .continue_from(
                            request,
                            published.into_value(),
                            pull_request.number,
                            &head_sha,
                        )
                        .await?;
                    // The `?` above is the gate, and it is why this is the one
                    // place the marker is written. `continue_from` answers `Ok`
                    // on exactly two arms and both have concluded that the gated
                    // transition is accounted for: the approve path, which
                    // performed it under a decision, and
                    // [`ProposeChange::already_ready`], which put the same
                    // operation to the executor and had its postcondition read
                    // back as [`EffectOutcome::Committed`]. A rejection, a
                    // redirect, an unclear reply and an unanswered question all
                    // leave through `Err`, and none of them reaches this line —
                    // which is the property that matters, because a marker
                    // written by a run that is *waiting* would stop the very
                    // process that was supposed to read the answer.
                    //
                    // Written here rather than inside the two arms so that
                    // "concluded" has one spelling. Writing it in `act_on` alone
                    // would leave a run reaching `already_ready` with its work
                    // done and unaccounted for, which is the defect this fixes in
                    // its residual case rather than a different one.
                    //
                    // **And the second arm cannot attribute one run's work to
                    // another, which is what makes covering it safe rather than
                    // merely useful.** Everything above is reached only through
                    // `opened(&branch)`, and [`branch_name`] is a function of the
                    // project and the invocation reference — so a run holding a
                    // different reference computes a different branch, finds no
                    // pull request for it, and takes the first-run path instead.
                    // `already_ready` is therefore reachable only by the run whose
                    // own marker it writes, and the write is idempotent across
                    // repeats of that run because `correlation_key` is derived
                    // from the same two inputs.
                    self.record_change_set(work_id)?;
                    Ok(evidence)
                }
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
