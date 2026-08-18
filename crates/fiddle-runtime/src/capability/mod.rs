//! What fiddle can do to the world, and the proof it is allowed to.
//!
//! A capability is the only thing in fiddle that *changes* anything, so the
//! interesting design question is not what it does but what it takes to reach
//! it. Design §4.4 states the rule: `execute` is reached only via
//! [`NextAction::Execute`]. That rule is made structural here rather than
//! enforced by a well-placed `if` — [`Capability::execute`] demands an
//! [`ExecutionGrant`], and the only way to obtain one is to hand
//! [`ExecutionGrant::authorise`] a derivation that said `Execute`. A caller who
//! forgets the check cannot compile, which is a stronger guarantee than a
//! caller who remembers it today.
//!
//! This module holds the contract — the trait, the grant, the failures, and the
//! list of ids this build answers to. Each capability lives in its own child
//! module beside it: [`stub`] holds [`StubMark`], which writes this invocation's
//! correlation key into the fixture change set. It makes no network call, no
//! model call, and no `git` invocation, so the same fixture and the same
//! invocation reference always produce byte-identical output — which is what
//! makes the two-invocation stability proof checkable. [`repair`] holds
//! [`FixtureRepair`], which does the opposite of all three — it calls a model,
//! spawns processes, and branches a git worktree — and is therefore where the
//! question of what may be *believed* becomes sharp. Its answer is stated in
//! that module: the check decides, and the model's account of itself is carried
//! as evidence and consulted nowhere. [`publish`] holds [`PublishChange`], which
//! is the first capability that changes something *outside* this machine — and
//! is therefore the first one that holds no means of doing so. It proposes three
//! effects to an [`Executor`](crate::effect::Executor) already bound to its own
//! id and receives three receipts; the credential, the client and the
//! authorization envelope are all on the other side of that seam. [`propose`]
//! holds [`ProposeChange`], the first **hybrid** one: it puts [`repair`]'s
//! bounded attempt and its own check in front of [`publish`]'s operations, and
//! then does the thing none of the other three can — it stops and asks, and a
//! later process comes back and reads the answer. [`cve`] holds
//! [`GroupMigration`], which is not a capability at all but the one *step* inside
//! M4's that consults a model — and is therefore where the opposite question from
//! [`repair`]'s is settled: not what may be believed of what a model says, but how
//! little it may be told. Every other decision in that milestone is arithmetic
//! over facts, and none of the arithmetic is in the prompt. The run that asks ends in an
//! `Err` on the path where everything worked, which is what
//! [`Recurrence::Awaiting`](crate::effect::Recurrence::Awaiting) is for; the run
//! that finds an approval is the only one in this build that performs an effect a
//! person had to authorize. [`mitigate`] holds [`CveMitigate`], which is M4's
//! capability and the only one whose interesting property is that it *composes*:
//! it decides nothing itself, and every branch in it is a match on a value some
//! other module computed. It is also the first capability whose invocation names
//! no work item — `cve` stands alone — which is why
//! [`Addressed`](crate::orchestration::Addressed) exists and why
//! [`fiddle_core::assess`]'s trackerless reading finally has something reaching it.

pub mod cve;
pub mod mitigate;
pub mod propose;
pub mod publish;
pub mod repair;
pub mod stub;

pub use cve::{
    land, record_fold, ForbiddenShape, Git, GroupMigration, GroupStatus, InWorktree,
    MigrationAttempt, MigrationConfig, NeedsWork,
};
pub use mitigate::{CveMitigate, MitigateConfig};
pub use propose::{attempt_worktree, ProposeChange, ProposeConfig};
pub use publish::{PublishChange, PublishConfig};
pub use repair::{FixtureRepair, RepairConfig};
pub use stub::StubMark;

use crate::human::validate::DecisionError;
use crate::human::InteractionRef;
use fiddle_core::{
    AttemptId, CapabilityId, DecisionRequestId, EvidenceRef, NextAction, Publication, Published,
    RunDisposition, TreeObservation,
};
use std::path::PathBuf;

/// Every capability this build can execute.
///
/// The single source of the known-id list: the CLI validates `--capability`
/// against it, so a build that gains a capability offers it and names it in a
/// diagnostic without anyone remembering to update a second list.
///
pub const CAPABILITIES: [CapabilityId; 5] = [
    fiddle_core::STUB_MARK,
    fiddle_core::FIXTURE_REPAIR,
    fiddle_core::PUBLISH_CHANGE,
    fiddle_core::PROPOSE_CHANGE,
    fiddle_core::CVE_MITIGATE,
];

/// Proof that a derivation authorised an execution, as part of a named attempt.
///
/// The fields are private and the only constructor is
/// [`ExecutionGrant::authorise`], so a value of this type cannot exist unless
/// some [`NextAction`] was `Execute`. That is the whole point: "the capability
/// is never executed from a blocked derivation" stops being a property of the
/// orchestration's control flow and becomes a property of the types, checkable
/// by the compiler at every call site that will ever exist.
///
/// # Why the attempt id is here
///
/// Because a grant is not "you may do this"; it is "**this attempt** authorises
/// you to do this", and a capability that needs to say which attempt it was has
/// nowhere else to get an honest answer. The alternative — the one this
/// replaced — was for the caller assembling a capability to mint an id of its
/// own and hand it over in the capability's configuration. That produced two
/// real, unique ids that did not name each other:
/// [`crate::orchestration::attempt`] minted the one the journal and the bundle
/// are filed under, `main.rs` minted the one
/// [`FixtureRepair`](repair::FixtureRepair) named its worktree and its evidence
/// after, and `repair:<changed>:<attempt>` therefore pointed at a bundle that
/// did not exist. A reference whose *format* implies a cross-reference that
/// does not hold is worse than one carrying no identifier at all.
///
/// Minting stays where it was — once, in `attempt`, so no caller can hand in a
/// duplicate and collide two bundles on one path. What changed is that the id
/// now *travels* to the capability along the one channel that already means
/// "you are authorised, as part of this run", instead of being minted a second
/// time at the edge.
///
/// No longer `Copy`, because [`AttemptId`] owns a `String`. It is passed by
/// value into [`Capability::execute`] exactly once per execution, so the clone
/// is per-attempt rather than per-call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionGrant {
    capability_id: CapabilityId,
    attempt: AttemptId,
}

impl ExecutionGrant {
    /// A grant for `action` as part of `attempt`, if and only if `action`
    /// authorises an execution.
    ///
    /// `Complete` and `Blocked` yield `None`, and there is no other way in.
    pub fn authorise(action: &NextAction, attempt: &AttemptId) -> Option<Self> {
        match action {
            NextAction::Execute { capability_id } => Some(ExecutionGrant {
                capability_id: *capability_id,
                attempt: attempt.clone(),
            }),
            NextAction::Complete | NextAction::Blocked { .. } => None,
        }
    }

    /// The capability the derivation named.
    pub fn capability_id(&self) -> CapabilityId {
        self.capability_id
    }

    /// The attempt this execution is part of — the same id the journal record
    /// and the published bundle are filed under, so a capability quoting it in
    /// its evidence names a document a reader can go and open.
    pub fn attempt_id(&self) -> &AttemptId {
        &self.attempt
    }
}

/// Something fiddle can do that changes the world.
///
/// `async`, because the capabilities this crate is growing towards spend their
/// time waiting: a model turn, a subprocess, a `git` invocation. The one M0
/// capability writes a single file and never yields, so it simply returns
/// immediately — the cost of the signature is paid by the caller's executor,
/// not by the work.
///
/// Boxed by `#[async_trait]` rather than written as a bare `async fn` in the
/// trait, and that is not a stylistic choice. A bare `async fn` in a trait is
/// not object-safe — its return type is per-implementation and unnameable — and
/// [`crate::RunContext`] reaches a capability through a `&dyn Capability`
/// precisely so the orchestration depends on this seam rather than on whichever
/// capability the build happens to ship. `#[async_trait]` erases the future into
/// a `Pin<Box<dyn Future + Send>>`, which keeps the trait object. One allocation
/// per execution, against a call that is about to spawn a process or wait on a
/// model, is not a trade worth losing the seam over.
#[async_trait::async_trait]
pub trait Capability: Send + Sync {
    /// The identity this capability is derived and reported under.
    fn id(&self) -> CapabilityId;

    /// The observable stage a [`ProgressEntry`](fiddle_core::ProgressEntry) for
    /// this capability is filed under.
    ///
    /// # Why the capability names it, and why there is no default
    ///
    /// A published bundle's `stage` is the vocabulary a reader uses to say
    /// *which part of the work this line is about*, so it belongs to whoever
    /// knows the parts. The orchestration does not: it holds a
    /// `&dyn Capability` precisely so it need not know which one it is holding,
    /// and the one thing it must not do is invent a name on the capability's
    /// behalf. It did exactly that until this method existed — a single
    /// `const STAGE: &str = "mark"` in [`crate::orchestration`], which is
    /// [`StubMark`]'s one step — and so a `fixture_repair` run published
    /// `{"capability_id":"fixture_repair","stage":"mark", …}`.
    ///
    /// **Deliberately not defaulted**, unlike [`Capability::receipts`]. That
    /// method defaults to the empty list, which is the neutral value: a
    /// capability with nothing to say about itself says nothing, and no reader
    /// is misled. There is no neutral stage name. Any default would be some
    /// capability's real vocabulary applied to every other one, which is
    /// verbatim the defect above — so the third capability this build gains has
    /// to name its own stage or fail to compile, rather than silently inheriting
    /// the first one's.
    ///
    /// `&'static str` rather than `String`: a stage is a fixed name from a
    /// closed set the implementation knows at compile time, not something
    /// computed per execution.
    fn stage(&self) -> &'static str;

    /// Do the thing, and hand back what a reader can go and check.
    ///
    /// The `grant` argument is not consulted for permission by convention; it
    /// *is* the permission, and an implementation must reject a grant naming a
    /// different capability rather than doing that capability's work.
    async fn execute(
        &self,
        grant: ExecutionGrant,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError>;

    /// What this capability observed of its own execution, whether or not that
    /// execution succeeded.
    ///
    /// # Why this is a second method rather than part of `execute`'s return
    ///
    /// Because the interesting case is the failing one. [`Capability::execute`]
    /// returns `Result<EvidenceRef, _>`, so everything it can say about *how* it
    /// ran travels on the `Ok` arm — and an execution that failed is precisely
    /// when an operator most needs to know what it did before it failed. That
    /// gap is not hypothetical: it is what let a repair capability call no tools
    /// at all, for every model, and surface as an ordinary failed check that
    /// nothing outside the process could distinguish from a model that tried and
    /// lost. Widening the return type would close it too, at the cost of
    /// changing every implementation and every call site of the seam the
    /// orchestration is built on. A separate accessor the orchestration consults
    /// on **both** arms closes it without moving anything.
    ///
    /// Defaulted to empty, so a capability with nothing to observe about itself
    /// — [`StubMark`], which writes one file and never yields — is unaffected,
    /// and M0's bundles keep the bytes they have always had.
    ///
    /// Read *after* the execution, which is why it takes `&self` and why an
    /// implementation with something to report needs interior mutability.
    fn receipts(&self) -> Vec<EvidenceRef> {
        Vec::new()
    }

    /// What this capability saw of a forge, if it reached one.
    ///
    /// # Why the capability answers this and not the orchestration
    ///
    /// Because the orchestration reaches no forge. [`crate::orchestration::observe`]
    /// consults two *local* ports and is shared with the read-only `inspect`,
    /// which is offline and credential-free for every value of `--capability`;
    /// a review read made there would take that property away from a command
    /// that only ever wanted to say what it would do. So the only participant in
    /// a run that can honestly report a pull request is the one that opened it.
    ///
    /// [`fiddle_core::WorkStateView::without_publication`] already says this in
    /// the other direction: a capability that publishes nothing has not looked
    /// for a review and found none — the question does not apply — and "a
    /// capability that *can* see a review builds the view itself". This method
    /// is the channel it builds it through.
    ///
    /// `None` is the neutral answer, the same neutrality [`Capability::receipts`]
    /// gets from the empty list: a capability that reached no forge says nothing,
    /// the view keeps its `NotApplicable` pair, and M0's and M1's bundles are
    /// byte-identical to what they were. It is deliberately not
    /// `Some(Publication)` holding two `NotApplicable`s, because that would be
    /// every capability answering a question only one of them can be asked.
    ///
    /// Read *after* the execution, on both arms, for the reason `receipts` is:
    /// an execution that failed part-way is precisely when an operator most
    /// needs to know what did reach the forge before it stopped.
    fn publication(&self) -> Option<Publication> {
        None
    }

    /// Which revision this capability's attempt worked at, if it made a tree at
    /// all.
    ///
    /// # Why this is a third accessor and not part of [`Capability::publication`]
    ///
    /// Because the two are independent facts about a run and pairing them would
    /// make one of the honest combinations unsayable.
    /// [`PublishChange`](publish::PublishChange) reaches a forge and creates no
    /// worktree; a capability could equally work in one and reach no forge. A
    /// `Publication` grown a third field would force every caller of one to have
    /// an answer about the other.
    ///
    /// `None` is the neutral answer, the same neutrality
    /// [`Capability::receipts`] gets from the empty list and
    /// [`Capability::publication`] from `None`: a capability that made no
    /// worktree chose no revision, the view carries no `tree` key at all, and
    /// every bundle published before this existed is byte-identical. It is
    /// deliberately not a [`TreeObservation`] of empty strings, which would be
    /// the positive claim *the attempt ran at nothing*.
    ///
    /// Read *after* the execution, on both arms, for the reason the other two
    /// are: a run that made a worktree and then failed in it is precisely when
    /// an operator needs to know which revision it was looking at.
    fn tree_observation(&self) -> Option<TreeObservation> {
        None
    }

    /// What this capability's run came to, where it has a disposition table of
    /// its own.
    ///
    /// # Why a fourth accessor rather than something on the evidence reference
    ///
    /// Because a capability's conclusion is not the same thing as a pointer to
    /// an artefact, and squeezing it into one produced exactly the defect this
    /// exists to close: `cve_mitigate` computed the row, wrote the verdict
    /// report, and published `cve:<count>:<attempt>` — a locator that names
    /// neither the outcome nor the reason. Five of Design §3's seven rows were
    /// therefore indistinguishable from outside the process, and a distinction
    /// only the process can make is not one an operator, a workflow or a
    /// mutation test can act on.
    ///
    /// # Why `Option`, and why that keeps every earlier bundle unchanged
    ///
    /// `None` is the neutral answer for [`Capability::tree_observation`]'s
    /// reason, one level further out: *which row of the table did this run
    /// reach* is not a question a capability with no table can be asked at all,
    /// so the bundle carries no `disposition` key rather than a defaulted one.
    /// M0's `stub_mark`, M1's `fixture_repair`, M2's `propose_change` and
    /// `publish_change` answer `None` and their bundles are byte-identical.
    ///
    /// Read *after* the execution, on both arms, for the reason the other three
    /// are — and here the failing arm is the one that matters most:
    /// [`Reason::ScanUnusable`](crate::evaluate::Reason::ScanUnusable) is a row
    /// of the table reached by returning an error, so a bundle that only asked
    /// on success would drop the one row Design §3 calls the milestone most
    /// likely to get wrong.
    fn disposition(&self) -> Option<RunDisposition> {
        None
    }
}

/// Why an execution did not produce evidence.
///
/// Every variant names the path or the identity involved, because a capability
/// failure surfaces to an operator as a run outcome's `reason` and a bare
/// "write failed" would leave them nothing to act on.
#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    /// The grant authorised a different capability than the one asked to run.
    ///
    /// Unreachable through the M0 orchestration, which only ever asks the
    /// capability the derivation named — but the check belongs to the
    /// capability, so that adding a second one cannot make the mismatch
    /// possible without also making it an error.
    #[error("capability `{requested}` was asked to run under a grant for `{granted}`")]
    NotAuthorised {
        granted: CapabilityId,
        requested: CapabilityId,
    },

    /// The change set could not be recorded.
    #[error("could not record the change set at {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The capability finished, and the check it is answerable to did not pass.
    ///
    /// The variant that carries this milestone's central rule. `exit_code` is
    /// what decided the outcome; `claimed` is what the model said about itself
    /// and is here *because* it is not consulted — recording a claim beside the
    /// verdict that overruled it is how a reader can see that the two were
    /// different things. Nothing in this crate branches on it, and a future
    /// caller that did would be reintroducing exactly the trust this variant
    /// exists to have removed.
    #[error(
        "the check exited {exit_code}, so nothing was earned \
         (the model claimed completion: {claimed}): {stderr}"
    )]
    CheckFailed {
        claimed: bool,
        exit_code: i32,
        stderr: String,
    },

    /// The workspace the capability needed could not be prepared, used, or
    /// interrogated.
    #[error("the workspace could not be used: {0}")]
    Workspace(#[from] crate::workspace::WorkspaceError),

    /// The bounded attempt produced no report, so there is nothing to verify.
    #[error("the attempt produced no report: {0}")]
    Agent(#[from] crate::agent::AgentError),

    /// An effect this capability proposed did not produce a receipt.
    ///
    /// Carried whole rather than flattened into a string, so the distinction the
    /// whole of M2 turns on survives the trip out of a capability: a caller — or
    /// a reader of the run's reason — can still tell
    /// [`EffectError::PolicyDenied`](crate::effect::EffectError::PolicyDenied)
    /// from [`EffectError::Unresolved`](crate::effect::EffectError::Unresolved),
    /// and a lost answer from a refused write.
    #[error("{0}")]
    Effect(#[from] crate::effect::EffectError),

    /// The capability published a question and stopped. Not a failure: the run
    /// is waiting, which is what [`fiddle_core::RunOutcome::Suspended`]
    /// promises.
    ///
    /// # Why a wait is an `Err` at all
    ///
    /// Because this type's own sentence is *why an execution did not produce
    /// evidence*, and "it is waiting for a person" belongs to that set — an
    /// execution that put a question on a conversation has no
    /// [`EvidenceRef`] for the thing it was asked to do, because that thing has
    /// not been done. The cleaner shape is to widen the success arm to
    /// `Evidence | Suspended`, since waiting is not failing; it was rejected on
    /// cost, because that signature is the seam all four capabilities and the
    /// orchestration's success path are built on and M0's and M1's lanes both
    /// drive it. What makes the chosen shape safe is
    /// [`crate::effect::Recurrence`]: the wait gets its own value there rather
    /// than borrowing a failure's, and every match on it is exhaustive.
    ///
    /// # Why the whole [`InteractionRef`] and not the request id
    ///
    /// Because [`crate::orchestration`] files the progress entry that tells a
    /// reader *where to look*, and it can only do that from a value it was
    /// given. A [`DecisionRequestId`] identifies the question to a later
    /// process re-deriving it; it does not tell a person which pull request to
    /// open. Both are carried, and the rendering is
    /// [`InteractionRef`]'s single [`Display`](std::fmt::Display), so the
    /// bundle, this diagnostic and the human line cannot disagree about how one
    /// conversation is named.
    ///
    /// The message names the conversation for that reason, rather than
    /// deferring it to whichever caller happens to render one: the outcome's
    /// `reason` is built from this text, and a reason that named only the
    /// question would leave a reader of the `--json` payload with nowhere to go.
    #[error("awaiting a human decision at {interaction} on request {}: {question}", request.0)]
    AwaitingDecision {
        request: DecisionRequestId,
        interaction: InteractionRef,
        question: String,
    },

    /// Somebody who may decide read the question and said no.
    ///
    /// # Why a refusal is `Permanent` and not `Awaiting`
    ///
    /// Because the question has been answered. `Awaiting` says *nothing is wrong
    /// and nothing will change until something outside this process does*, and
    /// that is exactly false here: the thing outside this process has already
    /// happened. Repeating the invocation re-derives the same request id, finds
    /// the same conversation, selects the same last authorized reply and reads it
    /// the same way — which is what [`crate::effect::Recurrence::Permanent`]
    /// means, and is the row
    /// [`PolicyDenied`](crate::effect::EffectError::PolicyDenied) already uses
    /// for a decision that a repeat re-derives.
    ///
    /// It is not `Correctable` either. There is no obstacle in front of the run
    /// for an operator to remove; a person considered the change and declined it,
    /// and inviting a retry would present that as a transient failure.
    ///
    /// The reason is [`Published`] rather than `String` because it is a span of
    /// text somebody outside this process wrote, arriving by way of a model that
    /// read them. `Published::of` is the only way to put such text where a reader
    /// will see it, and this reaches a run outcome's `reason`.
    #[error("a person refused request {}: {reason}", request.0)]
    DecisionRejected {
        request: DecisionRequestId,
        reason: Published,
    },

    /// The validation order could not establish a decision one way or the other.
    ///
    /// Ten distinct refusals travel in here, and they are carried whole rather
    /// than flattened for [`CapabilityError::Effect`]'s reason: the distinctions
    /// are the point. "Two comments name this question", "fiddle's own question
    /// has been edited" and "the conversation could not be read" send an operator
    /// to three different places, and a capability that rendered them into one
    /// string would have thrown away the only thing that tells them apart.
    ///
    /// [`CapabilityError::recurrence`] is where each of the ten is given an exit
    /// row, and that match is deliberately exhaustive over
    /// [`DecisionError`] rather than a catch-all: the next refusal
    /// the walk grows is a question the compiler asks whoever adds it.
    #[error("no decision could be established for request {}: {source}", request.0)]
    DecisionUnresolved {
        request: DecisionRequestId,
        #[source]
        source: DecisionError,
    },

    /// The attempt's check passed over a tree nobody changed, so there is
    /// nothing to propose.
    ///
    /// Neither of the two nearby answers is honest. Publishing an empty commit
    /// would open a draft and ask a person to approve a change that does not
    /// exist; reporting success would account for work that was never done, and
    /// this is the capability whose whole point is that the *change* is the
    /// deliverable. So it is a failure — and a correctable one, because a later
    /// attempt over the same fixture may well produce something.
    ///
    /// Distinct from [`CapabilityError::CheckFailed`] on purpose: the check
    /// passed, and a diagnostic saying otherwise would send an operator to look
    /// at a check that is working.
    #[error("the attempt changed no file, so there is nothing to propose")]
    NothingProposed,

    /// The context a capability was given publishes from a tree that is not the
    /// one its attempt works in.
    ///
    /// Only [`ProposeChange`] can reach this, and it is
    /// the same family of refusal as [`CapabilityError::Misbound`]: two values
    /// that have to name one thing, checked before anything is read rather than
    /// reconciled afterwards. What makes it worth a variant of its own is what
    /// the confusion would produce — [`crate::github::EnsureBranchPublished`]
    /// pushes `HEAD` out of the context's worktree, so a run whose attempt
    /// worked somewhere else would publish a commit it never made, with a
    /// payload hash naming the commit it did make and a postcondition read that
    /// then disagrees with both.
    #[error("this run publishes from {publishing} and its attempt works in {working}")]
    PublishesElsewhere {
        publishing: PathBuf,
        working: PathBuf,
    },

    /// The scan this capability's whole run is derived from produced nothing it
    /// can use.
    ///
    /// Carried whole rather than rendered into a string, and that is the entire
    /// point of the variant. [`ScanError::recurrence`](crate::scanner::ScanError::recurrence)
    /// is a six-row table decided per variant — a scanner that is not installed
    /// is not the same row as a container daemon that is down — and it was
    /// written before anything wrapped a `ScanError` into a `CapabilityError`,
    /// so there was no seam for it to reach an exit code through. This is that
    /// seam: without it every one of the six rows would arrive at
    /// [`CapabilityError::recurrence`] as one arm and be given one answer.
    #[error("{0}")]
    Scan(#[from] crate::scanner::ScanError),

    /// The scanner wrote a document this build cannot project.
    ///
    /// Distinct from [`CapabilityError::Scan`] because the scan *succeeded*: the
    /// program ran, wrote a report and said which image it read. What failed is
    /// this build's reading of it, and the two send an operator to opposite
    /// places — one to the scanner or the host, the other to a version mismatch
    /// between fiddle and the document shape it was written against.
    #[error("{0}")]
    Projection(#[from] crate::cve::project::ProjectionError),

    /// Which branch this run adds to could not be decided.
    #[error("{0}")]
    Plan(#[from] crate::capability::cve::PlanError),

    /// The already-fixed set could not be established.
    ///
    /// A failure and never an empty set, which is the whole reason it is here: a
    /// dedup that could not read the branch's commits would otherwise report that
    /// nothing had been fixed yet, and the run would re-propose every advisory the
    /// branch already carries — against a tree whose `go.mod` is already past the
    /// fix, under a security fix's commit message.
    #[error("{0}")]
    Dedup(#[from] crate::cve::dedup::DedupError),

    /// The executor a capability was built with names a different run than the
    /// one asking it to execute.
    ///
    /// A capability that derives its effect identities from its executor — which
    /// is the only way to be sure they match the ones the executor will derive —
    /// would otherwise publish under one invocation's name while the bundle,
    /// the journal and the change-set marker were filed under another's. There
    /// is no run this is correct for, so it is refused before anything is
    /// proposed rather than reconciled.
    #[error("this executor is bound to `{bound}` and the run is `{asked}`")]
    Misbound { bound: String, asked: String },
}

impl CapabilityError {
    /// Which exit row a run that reached this failure belongs in.
    ///
    /// [`crate::orchestration::run`] turns a capability `Err` into a
    /// [`fiddle_core::RunOutcome`], and until this existed it turned *every* one
    /// of them into `Retryable`. That was right while the only ways to fail were
    /// M1's — a write, a check, a workspace, a bounded attempt, all four of them
    /// obstacles a repeat gets past — and M2 then added effect failures behind
    /// the same arm without revisiting it. This is the revisit, and it is one
    /// exhaustive `match` so that the next variant's author is asked the same
    /// question by the compiler.
    ///
    /// Two arms delegate, because two of the things a capability wraps can fail
    /// in more than one family and already own the table that says which:
    /// [`CapabilityError::Effect`], to
    /// [`EffectError::recurrence`](crate::effect::EffectError::recurrence)'s
    /// six-way table, and [`CapabilityError::Scan`], to
    /// [`ScanError::recurrence`](crate::scanner::ScanError::recurrence)'s. The
    /// second arrived with M4 and closed a table that had been computing an
    /// answer nothing read.
    pub fn recurrence(&self) -> crate::effect::Recurrence {
        use crate::effect::Recurrence;
        match self {
            // M1's four, unchanged and deliberately so. A change set that could
            // not be written, a check that did not pass, a workspace that could
            // not be prepared and an attempt that produced no report are each an
            // obstacle in front of the run: fix the permission, let the model
            // try again, and the same invocation succeeds.
            //
            // M3's `NothingProposed` joins them, and for the same test rather
            // than by resemblance: an attempt that changed nothing is an attempt
            // that may change something next time, so a repeat is worth
            // inviting. It is deliberately not `Permanent` — that would tell a
            // caller to give up on a fixture the next attempt might repair.
            //
            // # What pins each of these five, measured arm by arm
            //
            // **The false version, shown rather than swapped out.** This said
            // *"`attempt.rs` and `repair_protocol.rs` pin all four, and none of
            // them moves here."* Every clause was wrong: `attempt.rs` pins none of
            // them, `repair_protocol.rs` pins two, and the one pin on `Write` is
            // *here*, in this crate's own `src/`.
            //
            // Each arm was flipped to `Permanent` in turn against
            // `cargo test -p fiddle-runtime --no-fail-fast` — 20 result lines,
            // baseline 424 passed / 0 failed. `--no-fail-fast` matters: the `Write`
            // pin is in the lib binary, which cargo runs first, so a fail-fast run
            // stops there having measured 1 binary of 20.
            //
            // - `Write` — **one** test, 423/1, in neither named file:
            //   `orchestration::tests::a_capability_failure_is_retryable_and_recorded`.
            //   It is also **conditional**: it returns early when the outcome is
            //   `Completed`, which is what an identity able to write a mode-0500
            //   directory produces. Under such an identity — root in a container —
            //   this arm is pinned by nothing at all.
            // - `CheckFailed` — **six**, 418/6:
            //   `propose_capability::an_attempt_whose_check_failed_publishes_nothing_and_asks_nothing`,
            //   and in `repair_protocol.rs`
            //   `a_path_escape_is_refused_and_mutates_nothing`,
            //   `a_model_claiming_success_over_a_broken_fixture_is_disbelieved`,
            //   `an_attempt_that_called_no_tools_publishes_tools_zero`,
            //   `an_absolute_path_is_refused`,
            //   `a_symlink_out_of_the_workspace_is_refused`.
            // - `Workspace` — **one**, 423/1:
            //   `workspace::a_revision_the_fixture_can_only_fetch_is_refused_by_name_and_nothing_fetches`,
            //   which names `Recurrence` directly and claims in its own comment to
            //   be this crate's only assertion over the arm. It is — now measured
            //   rather than asserted.
            // - `NothingProposed` — **one**, 423/1:
            //   `propose_capability::an_attempt_that_changed_nothing_publishes_nothing_and_asks_nothing`.
            // - `Agent` — **five**, 419/5, all in `repair_protocol.rs`:
            //   `an_unregistered_tool_name_mutates_nothing`,
            //   `a_cancelled_attempt_leaves_the_fixture_unmutated`,
            //   `exceeding_the_turn_budget_fails_the_run`,
            //   `exceeding_the_changed_file_cap_fails_the_run`,
            //   `malformed_structured_output_fails_the_run`.
            //
            // `repair_protocol.rs` pins two of the five and does it **by
            // behaviour**, never naming the type: its `refusal` helper panics
            // unless the outcome is `RunOutcome::Retryable`. That is why a grep for
            // `Recurrence` over its 909 lines returns 0 while it genuinely accounts
            // for ten of the fourteen tests above — and it is why the claim this
            // replaces read as plausible.
            //
            // `attempt.rs` pins none of the five: all 11 of its tests stay green
            // under all five flips. It does assert `RunOutcome::Retryable` three
            // times, but over the attempt journal and the report bundle —
            // orchestration's own publication failures, upstream of any
            // `CapabilityError` — so none of them reaches this table.
            CapabilityError::Write { .. }
            | CapabilityError::CheckFailed { .. }
            | CapabilityError::Workspace(_)
            | CapabilityError::NothingProposed
            | CapabilityError::Agent(_) => Recurrence::Correctable,

            // The internal-consistency refusals. The first two are unreachable
            // through [`crate::orchestration::run`] — a grant is only ever issued
            // for the capability the derivation named, and an executor is only
            // ever bound to the run that built it — so those two arms change no
            // observable behaviour and are written the honest way regardless.
            //
            // `PublishesElsewhere` is the one of the three a *document* can
            // produce, and it belongs on this row for the same test: the two
            // paths are derived from the run's own inputs, so a repeat under the
            // same configuration derives the same disagreement. It is exit 20
            // rather than 11 because there is nothing here for a retry to get
            // past; what has to change is what the caller was built with.
            CapabilityError::NotAuthorised { .. }
            | CapabilityError::Misbound { .. }
            | CapabilityError::PublishesElsewhere { .. } => Recurrence::Permanent,

            // A person said no. See the variant for why this row and not one of
            // the other two.
            CapabilityError::DecisionRejected { .. } => Recurrence::Permanent,

            // The ten refusals of the validation order, each given the row its
            // own evidence earns. Written out rather than defaulted, because the
            // two families here are genuinely different and a blanket answer
            // would be wrong about half of them: a read that failed is an
            // obstacle a repeat gets past, and a marker naming another effect is
            // a fact about the conversation that a repeat re-derives.
            CapabilityError::DecisionUnresolved { source, .. } => match source {
                // Read failures and races. Each of these can be true on one walk
                // and false on the next without anybody doing anything about it:
                // a listing that failed, a comment deleted between two reads, a
                // reply edited between the listing and the re-read, a head that
                // moved while the walk was running. A repeat re-reads.
                DecisionError::Unreadable(_)
                | DecisionError::RequestAbsent(_)
                | DecisionError::ReplyEdited { .. }
                | DecisionError::HeadMoved { .. } => Recurrence::Correctable,
                // And a state somebody has to change out there before any repeat
                // of this invocation can get further: two comments naming one
                // question, fiddle's own question rewritten — which fiddle has no
                // path that does, and whose evidence is a timestamp pair that
                // never returns to agreeing. `DuplicateRequest` is on this row
                // for the same test rather than by resemblance: the walk chooses
                // no candidate replies at all while there are two request
                // comments, so a repeat re-derives the same refusal until a
                // person deletes one.
                DecisionError::DuplicateRequest { .. }
                | DecisionError::RequestEdited { .. }
                | DecisionError::ForeignEffect { .. }
                | DecisionError::ForeignPayload { .. }
                | DecisionError::NotOpen
                // `AlreadyReady` is here for completeness and is not reached
                // through `propose_change`: that capability answers it by
                // proposing the gated effect and letting the executor's step 3
                // observe the postcondition it already satisfies. A caller that
                // did surface it would be reporting a transition that happened,
                // which no repeat undoes.
                | DecisionError::AlreadyReady => Recurrence::Permanent,
            },

            // **The variant that is not a failure.** Everything else in this
            // table answers "would repeating get past this"; this one answers
            // that there is nothing to get past. The run asked a person and
            // stopped, which is what it was built to do, and neither of the
            // other two rows describes it: 11 invites a repeat that would ask
            // the same question again, and 20 tells a caller to give up on a
            // run an answer would finish. See [`Recurrence::Awaiting`].
            CapabilityError::AwaitingDecision { .. } => Recurrence::Awaiting,

            // The variant M2 added, and the one with three families inside it
            // since `HumanDecisionRequired` moved to the row above.
            CapabilityError::Effect(error) => error.recurrence(),

            // **The second delegation, and the reason the sentence above this
            // table used to say "only `Effect` delegates".** A scan can fail in
            // six ways across two families — a program that is not installed and
            // a container daemon that is down are not one row — and
            // `ScanError::recurrence` is the table that says which is which. It
            // was written with no caller: nothing wrapped a `ScanError` into a
            // `CapabilityError`, so the six rows it computed reached no exit
            // code. Answering here rather than delegating would be a seventh
            // opinion, and the one it would get wrong is the one that matters —
            // `DaemonUnreachable` is exit 11 and `Missing` is exit 20.
            CapabilityError::Scan(error) => error.recurrence(),

            // A document this build cannot read is the same shape of fact as an
            // artefact it cannot parse — `ScanError::Unparseable`'s row, and for
            // that row's reason: the same scanner over the same image writes the
            // same bytes back, so a repeat re-derives the refusal. What has to
            // change is a scanner version or this build, and neither is reached
            // by running the invocation again.
            CapabilityError::Projection(_) => Recurrence::Permanent,

            // The two halves of the branch decision, and they are opposite rows.
            // `PlanError`'s own doc says why they are two variants: a read that
            // failed says nothing about the world and invites another attempt; a
            // world that was read perfectly well and found to be one this run
            // must not act in is a state somebody has to change out there. That
            // is `Refusal::HeadOutsideThePushablePrefix`, whose own doc says a
            // person has to move a label, and no repeat does it for them.
            CapabilityError::Plan(error) => match error {
                cve::PlanError::Read(_) => Recurrence::Correctable,
                cve::PlanError::Refused(_) => Recurrence::Permanent,
            },

            // Dedup, and its three failures are two families for the reasons
            // above. A `git` that could not be run and a resolver that could not
            // be asked are obstacles a repeat gets past; a clone with no history
            // to read is not — `--depth` and a missing `origin/<base>` are both
            // properties of the checkout this deployment performs, so the next
            // invocation of *this* one reads the same nothing.
            CapabilityError::Dedup(error) => match error {
                crate::cve::dedup::DedupError::ShallowHistory { .. } => Recurrence::Permanent,
                crate::cve::dedup::DedupError::Git { .. }
                | crate::cve::dedup::DedupError::Resolver(_) => Recurrence::Correctable,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::STUB_MARK;

    const WORK_ID: &str = "fiddle-m0-demo";
    const INVOCATION_REF: &str = "beans:fiddle-m0-demo";
    const ATTEMPT: &str = "01JQZX0000000000000000000";

    fn grant() -> ExecutionGrant {
        ExecutionGrant::authorise(
            &NextAction::Execute {
                capability_id: STUB_MARK,
            },
            &AttemptId(ATTEMPT.to_string()),
        )
        .expect("an Execute derivation authorises")
    }

    /// **The seam survives becoming async.**
    ///
    /// The regression this guards: a bare `async fn` in a trait is not
    /// object-safe, and [`crate::RunContext`] holds a `&dyn Capability`. The
    /// binding below is spelled out with its type rather than inferred, so a
    /// signature that stopped being object-safe fails to compile here rather
    /// than at the orchestration's call site — which is the assertion.
    #[tokio::test]
    async fn a_capability_is_still_usable_as_a_trait_object() {
        let dir = tempfile::tempdir().unwrap();
        let marking = StubMark::new(dir.path(), "icecube");
        let capability: &dyn Capability = &marking;
        assert_eq!(capability.id(), STUB_MARK);
        assert!(capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .is_ok());
    }

    /// The known-id list is the one source the CLI validates `--capability`
    /// against, so a build that can run a capability has to name it here.
    /// The known-id list is the one source the CLI validates `--capability`
    /// against, so a build that can run a capability has to name it here — and,
    /// by `every_registered_capability_can_be_selected` in the binary, a build
    /// that names one here has to have a selection for it. The two move
    /// together, which is why the fourth id and that selection arrive in one
    /// change.
    #[test]
    fn every_capability_this_build_has_is_registered() {
        assert_eq!(
            CAPABILITIES,
            [
                STUB_MARK,
                fiddle_core::FIXTURE_REPAIR,
                fiddle_core::PUBLISH_CHANGE,
                fiddle_core::PROPOSE_CHANGE,
                fiddle_core::CVE_MITIGATE
            ]
        );
    }

    /// The fail-closed rule, stated against the type rather than against a
    /// branch: the two non-executing derivations yield no grant at all, so no
    /// call to `execute` can be written from them.
    #[test]
    fn only_an_execute_derivation_yields_a_grant() {
        let attempt = AttemptId(ATTEMPT.to_string());
        assert_eq!(grant().capability_id(), STUB_MARK);
        assert_eq!(
            ExecutionGrant::authorise(&NextAction::Complete, &attempt),
            None
        );
        assert_eq!(
            ExecutionGrant::authorise(
                &NextAction::Blocked {
                    reason: "unobservable".into()
                },
                &attempt
            ),
            None
        );
    }

    /// A grant carries the attempt it was issued under, so a capability quoting
    /// an attempt id in its evidence quotes the one its bundle is filed under
    /// rather than one it minted for itself.
    #[test]
    fn a_grant_names_the_attempt_it_was_issued_under() {
        assert_eq!(grant().attempt_id(), &AttemptId(ATTEMPT.to_string()));
    }

    /// **A scan failure keeps the exit row its own table computed.**
    ///
    /// [`crate::scanner::ScanError::recurrence`] is six variants across two
    /// families and it was written with no caller: nothing wrapped a `ScanError`
    /// into a `CapabilityError`, so the row it decided reached no exit code. The
    /// assertion is over *two* variants from *opposite* families, and that is
    /// what makes it discriminating — an arm answering `Correctable` for the
    /// whole variant, or `Permanent` for the whole variant, fails exactly one of
    /// these two. A single row would pass under a blanket answer.
    ///
    /// The consequence is the process exit code: `main.rs` maps
    /// `Retryable` to 11 and `Failed` to 20, so a scanner that is not installed
    /// tells automation to stop and a container daemon that is down tells it to
    /// come back.
    #[test]
    fn a_scan_failure_is_given_the_row_its_own_table_decided() {
        use crate::effect::Recurrence;
        use crate::scanner::ScanError;

        let missing = CapabilityError::Scan(ScanError::Missing {
            program: PathBuf::from("/nowhere/wizcli"),
            reason: "No such file or directory".to_string(),
        });
        let daemon = CapabilityError::Scan(ScanError::DaemonUnreachable {
            stderr: "cannot connect".to_string(),
        });

        assert_eq!(
            missing.recurrence(),
            ScanError::Missing {
                program: PathBuf::from("/nowhere/wizcli"),
                reason: "No such file or directory".to_string(),
            }
            .recurrence(),
            "the capability must delegate rather than answer for itself"
        );
        assert_eq!(missing.recurrence(), Recurrence::Permanent);
        assert_eq!(daemon.recurrence(), Recurrence::Correctable);
    }

    /// The branch decision's two halves are opposite rows, which is why
    /// `PlanError` has two variants at all: a read that failed says nothing about
    /// the world, and a world that was read and found unusable is a state
    /// somebody has to change out there.
    #[test]
    fn a_branch_that_could_not_be_read_and_one_that_was_refused_are_different_rows() {
        use crate::effect::Recurrence;

        let unread = CapabilityError::Plan(cve::PlanError::Read(crate::GhError::Timeout(
            std::time::Duration::from_secs(1),
        )));
        let refused = CapabilityError::Plan(cve::PlanError::Refused(
            cve::Refusal::HeadOutsideThePushablePrefix {
                number: 7,
                head: "someones/branch".to_string(),
                prefix: cve::PUSHABLE_PREFIX,
            },
        ));

        assert_eq!(unread.recurrence(), Recurrence::Correctable);
        assert_eq!(refused.recurrence(), Recurrence::Permanent);
    }

    /// The same shape once more, over dedup: a `git` that could not be run is an
    /// obstacle, and a clone with no history in it is the checkout this
    /// deployment performs.
    #[test]
    fn a_truncated_history_is_not_the_same_row_as_a_git_that_would_not_run() {
        use crate::cve::dedup::DedupError;
        use crate::effect::Recurrence;

        let unrunnable = CapabilityError::Dedup(DedupError::Git {
            repo: "/tmp/r".to_string(),
            command: "log".to_string(),
            message: "no such file".to_string(),
        });
        let shallow = CapabilityError::Dedup(DedupError::ShallowHistory {
            repo: "/tmp/r".to_string(),
            why: "the clone is shallow".to_string(),
        });

        assert_eq!(unrunnable.recurrence(), Recurrence::Correctable);
        assert_eq!(shallow.recurrence(), Recurrence::Permanent);
    }
}
