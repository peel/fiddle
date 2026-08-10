//! The mandatory authorization boundary for every external mutation.
//!
//! The whole of M2 rests on one distinction, and [`EffectOutcome`] is where it
//! is written down: **a lost answer is not a failed write.** A request whose
//! response never arrived may have landed, and the only honest thing to say
//! about it is that nobody knows. Collapsing that third value into either of the
//! other two produces a duplicate external effect — report a landed write as
//! failed and the retry performs it twice; report a refused one as committed and
//! the world never gets the change at all.
//!
//! [`Executor::execute`] is the PRD's execution order implemented literally, and
//! two of its steps carry the milestone.
//!
//! **Step 3 is inspected before the mutation**, so an effect that already
//! happened is recognised rather than repeated. That it comes *before* step 4
//! matters in the other direction too: an effect the world already satisfies is
//! never put to policy, so it cannot be refused for a rule it no longer needs.
//!
//! **Step 8 is its mirror.** The postcondition is read *back* rather than
//! inferred from the response, because a response that never arrived is exactly
//! the case this exists for. An `Unknown` is settled by looking, and by looking
//! only — a retry there is how duplicates are born — and an `Unknown` whose read
//! itself fails stays [`EffectError::Unresolved`] rather than degrading to one of
//! the two confident answers.
//!
//! The order is a contract rather than an implementation detail, which is why it
//! is *observable*: [`Executor`] reports each [`ExecutionStep`] it enters to an
//! [`EffectTrace`] before doing the work behind it. Without that, a test could
//! only assert that policy and the mutation both happened, and would pass on an
//! implementation that authorized first and asked afterwards.

pub mod receipt;

pub use receipt::{EffectError, EffectReceipt, ObservedState, Recurrence};

use crate::git::GitCli;
use crate::github::{GhCli, GhError, RetryAdvice};
use fiddle_core::{
    combine, effect_id, payload_hash, CapabilityId, DeploymentRule, EffectId, EffectKind,
    HumanDecisionRequirement, Observation, PayloadHash, PolicyDecision, ProposedEffect,
    VerificationState,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// The three-valued result an ambiguous write forces.
///
/// Serialized in `snake_case` because it reaches a published bundle, where the
/// consumer matching on it is not this crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectOutcome {
    /// The change is known to have landed, because it was read back.
    Committed,
    /// The change is known not to have landed, because something refused it in
    /// terms that leave no room for it having happened anyway.
    NotCommitted,
    /// Nobody knows. Resolved by reading the world, never by retrying the
    /// mutation.
    Unknown,
}

/// One step of the authorization order, named.
///
/// A closed enum rather than a string for the same reason [`EffectKind`] is: the
/// order is a contract, and a contract spelled by whoever happened to be writing
/// a log line is not one. [`ExecutionStep::as_str`] is the single spelling.
///
/// Steps 5 and 9 of the PRD's nine have no variant here, and deliberately so.
/// Obtaining the authenticated adapter handle is not a moment in this design —
/// the handle is [`EffectContext::gh`], already resolved before the executor was
/// built — and returning the receipt is the function returning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionStep {
    /// 1. The proposing capability is the one this executor is bound to.
    ValidateCapability,
    /// 2. Identity and payload hash, from canonical inputs alone.
    DeriveIdentity,
    /// 3. Does the desired postcondition already hold?
    InspectPostcondition,
    /// 4. Capability minimum combined with deployment policy.
    CombinePolicy,
    /// 6. The envelope for this exact payload.
    Authorize,
    /// 7. Delegate to the adapter.
    Apply,
    /// 8. Read the world back.
    ObservePostcondition,
}

impl ExecutionStep {
    /// The step's stable name.
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStep::ValidateCapability => "validate_capability",
            ExecutionStep::DeriveIdentity => "derive_identity",
            ExecutionStep::InspectPostcondition => "inspect_postcondition",
            ExecutionStep::CombinePolicy => "combine_policy",
            ExecutionStep::Authorize => "authorize",
            ExecutionStep::Apply => "apply",
            ExecutionStep::ObservePostcondition => "observe_postcondition",
        }
    }
}

/// Where the executor writes down which step it is on, and for which effect.
///
/// This is an observation seam of the same family as [`crate::ports`] — a trait
/// the runtime calls unconditionally, with the sink supplied by whoever built the
/// executor. The deterministic suite records and asserts the order; production
/// passes [`crate::journal::AttemptTrace`], which sinks the walk into the
/// attempt's own journal.
///
/// **There is no default sink, and that is deliberate.** One was offered until
/// M2's exactly-once bean, and what it produced was a seam whose only real
/// implementor was the test suite: an executor built without `observed_by`
/// silently discarded everything, so a production path could go dark by
/// omission rather than by decision. [`Executor::new`] therefore takes the sink,
/// and a caller that has nothing to do with a step has to say so by passing a
/// sink that discards — which is a line of code somebody wrote, not a default
/// nobody noticed.
///
/// The [`EffectKind`] is carried beside the step because the seven steps repeat
/// once per effect, and a record that could not say *which* mutation `Apply` was
/// entered for would tell a recovery only that something may have happened —
/// which is the question, not the answer.
pub trait EffectTrace: Send + Sync {
    fn step(&self, kind: EffectKind, step: ExecutionStep);
}

/// What the deployment document says about each effect kind.
///
/// A trait rather than a concrete table because the table is a *configuration*
/// type and lives in the CLI crate; the executor needs only the question
/// answered. It can never weaken a capability's own minimum whatever it says —
/// that is [`combine`]'s job, not this trait's.
pub trait DeploymentPolicy: Send + Sync {
    fn rule_for(&self, kind: EffectKind) -> DeploymentRule;
}

/// Everything an operation needs to reach the outside world.
///
/// The `gh` here is the PRD's step 5, the authenticated adapter handle, resolved
/// once when the executor is built rather than obtained per effect: there is one
/// credential-carrying construction in this process ([`crate::github::cli`]) and
/// this is where it is handed to the operations that use it.
///
/// `git` and `work` are the second half of the same arrangement, and they are
/// here for a reason worth stating: publishing a branch is not an API call. A
/// ref can only be created pointing at an object the remote already holds, so
/// the objects and the ref go up together in one `git push`, out of the
/// worktree the attempt did its work in. Both are resolved once, beside the
/// `gh`, so the two credential-carrying constructions this process has are
/// handed to operations from one place rather than built per effect.
pub struct EffectContext {
    pub gh: GhCli,
    /// The one `git` that pushes. Its credential channel and environment are
    /// [`crate::git::publish`]'s subject and nothing here re-argues them.
    pub git: GitCli,
    /// The worktree whose `HEAD` is published. One per run, because an attempt
    /// works in one checkout; an operation that could name another would be
    /// naming work this run never did.
    pub work: PathBuf,
    /// The run's cancellation. An operation passes it down so a cancelled run
    /// stops before spawning **where it can** — and the "where it can" is not a
    /// hedge, it is the distinction the whole classification rests on. A
    /// cancellation noticed before a child exists is knowledge that nothing was
    /// sent; one that arrives with the child already running is an ambiguous write.
    /// Both adapters split those into separate error variants; see
    /// [`GhError::CancelledAfterSpawn`](crate::github::GhError::CancelledAfterSpawn).
    pub cancel: CancellationToken,
}

impl EffectContext {
    pub fn new(gh: GhCli, git: GitCli, work: PathBuf, cancel: CancellationToken) -> Self {
        Self {
            gh,
            git,
            work,
            cancel,
        }
    }
}

/// Where a wait is actually spent.
///
/// A seam for the same reason [`EffectTrace`] is one: the schedule this module
/// computes is a *decision*, and a decision that can only be observed by
/// spending it cannot be asserted. Production sleeps ([`SleepingClock`]); the
/// deterministic suite records the sequence and returns immediately, so the
/// backoff can be pinned without a test that takes as long as the backoff does.
///
/// Nothing fake enters the product to make that possible — this is the same
/// arrangement as `[github] cli = { program, args }`, a real seam a real
/// deployment could substitute at.
#[async_trait::async_trait]
pub trait ReadClock: Send + Sync + std::fmt::Debug {
    /// Wait `delay` before the postcondition is read again.
    async fn wait(&self, delay: Duration);
}

/// The clock a deployment runs on.
#[derive(Debug)]
pub struct SleepingClock;

#[async_trait::async_trait]
impl ReadClock for SleepingClock {
    async fn wait(&self, delay: Duration) {
        tokio::time::sleep(delay).await;
    }
}

/// The bounded budget a postcondition read may spend settling.
///
/// # Why this exists at all, and why it is only about the read
///
/// Real GitHub does not answer its own writes immediately. A dispatched
/// workflow run is reliably absent from `GET .../actions/workflows/<f>/runs`
/// for a moment after the dispatch that created it, and `GET
/// .../git/ref/heads/<b>` has answered **404 right after the push that created
/// the branch**, with the branch and the sha verified correct seconds later.
/// Both were measured against real GitHub rather than reasoned about.
///
/// Without a budget both reach [`EffectError::Unresolved`], which is *correct*
/// but pushes the wait onto the caller as an entire fresh process. With one,
/// the read waits for the answer it already has good reason to expect.
///
/// **The budget is bounded, and exhausting it is still `Unresolved`.** A read
/// that waited indefinitely would turn "the write did not land" into "wait
/// longer, then claim success", which is worse than the ambiguity it replaces:
/// the ambiguity at least sends somebody to look.
#[derive(Clone, Debug)]
pub struct ReadRetry {
    attempts: u32,
    initial: Duration,
    max: Duration,
    clock: Arc<dyn ReadClock>,
}

impl ReadRetry {
    /// A budget served by the deployment's own clock.
    ///
    /// `attempts` counts *reads*, not waits, and is floored at one: a budget of
    /// zero reads is not a stricter policy, it is a postcondition that is never
    /// observed, which would make every effect unresolved.
    pub fn bounded(attempts: u32, initial: Duration, max: Duration) -> Self {
        Self::served_by(attempts, initial, max, Arc::new(SleepingClock))
    }

    /// A budget of one read: look once, and take the answer.
    ///
    /// The behaviour every effect had before this existed, kept nameable so a
    /// caller that wants it says so rather than getting it by forgetting.
    pub fn none() -> Self {
        Self::bounded(1, Duration::ZERO, Duration::ZERO)
    }

    /// The same budget, spending its waits somewhere other than a real clock.
    pub fn served_by(
        attempts: u32,
        initial: Duration,
        max: Duration,
        clock: Arc<dyn ReadClock>,
    ) -> Self {
        Self {
            attempts: attempts.max(1),
            initial,
            max,
            clock,
        }
    }

    /// How many reads this budget allows in total.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// How long to wait before read number `attempt + 1`.
    ///
    /// `attempt` is 1 for the wait after the first read, so the un-jittered
    /// series is `initial`, `2 × initial`, `4 × initial`, … capped at [`max`].
    ///
    /// **A `Retry-After` wins.** It is the only wait in this system GitHub
    /// chose rather than fiddle, and guessing over an explicit instruction is
    /// how a client earns a secondary rate limit. It is still capped at `max`,
    /// because `max` is the operator's bound on how long one read may block a
    /// run — a server asking for longer than that is answered by spending the
    /// budget and reporting `Unresolved`, which is a caller who can decide,
    /// rather than by sleeping past the bound the document set.
    ///
    /// [`max`]: ReadRetry::max
    pub fn delay(&self, attempt: u32, advice: RetryAdvice, effect: &EffectId) -> Duration {
        if let Some(asked) = advice.retry_after {
            return asked.min(self.max);
        }
        let doublings = 1u32
            .checked_shl(attempt.saturating_sub(1))
            .unwrap_or(u32::MAX);
        let step = self
            .initial
            .checked_mul(doublings)
            .unwrap_or(self.max)
            .min(self.max);
        jitter(step, effect)
    }
}

/// Spread the wait over the lower half of its step, deterministically.
///
/// Jitter is here to decorrelate *concurrent fiddle processes*, which is a real
/// herd: many attempts against one repository, each having just written and
/// each about to read back on the same schedule. It is derived from the effect
/// identity rather than from a random source, so two processes working
/// different effects wait differently while the same effect is reproducible —
/// a backoff nobody can reproduce is a backoff nobody can assert.
///
/// The window is `[step/2, step]`, and the factor is drawn **once per effect**
/// rather than once per wait. Both halves of that matter: a factor that also
/// varied with the attempt would make the series go *backwards* once the
/// ceiling caps the step — two consecutive waits of `max × 0.9` and `max × 0.6`
/// — which is a backoff that backs off less the longer it waits. Held constant,
/// the schedule is `f × initial`, `2f × initial`, … capped, and so is
/// non-decreasing by construction, whatever `f` turned out to be.
fn jitter(step: Duration, effect: &EffectId) -> Duration {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in effect.0.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let half = step.as_nanos() / 2;
    let extra = half.saturating_mul(u128::from(hash % 1_001)) / 1_000;
    Duration::from_nanos(u64::try_from(half + extra).unwrap_or(u64::MAX))
}

/// What counts as a settled read at one of the two places the executor looks.
///
/// The two are deliberately not the same predicate, and that asymmetry is the
/// difference between a useful backoff and one that slows every run down.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Settle {
    /// **Step 3.** `Ok(None)` here is *knowledge* — the postcondition is absent
    /// and the mutation is what fixes it — so waiting for it to change would
    /// delay every effect that has never run by the whole budget, to no end.
    /// Only a failed *look* is worth waiting on.
    WhenTheLookSucceeds,
    /// **Step 8.** `Ok(None)` here may be a write that landed and has not
    /// surfaced yet, which is exactly what GitHub was measured doing. Absence
    /// is retried, and so is a failed look.
    WhenThePostconditionAppears,
}

/// One postcondition read, and how many looks it took.
struct Settled<S> {
    observed: Result<Option<S>, GhError>,
    /// Reads performed, always at least one. Carried so a diagnostic can say
    /// that waiting was tried rather than leaving an operator to wonder.
    reads: u32,
}

/// One concrete external result, in the two halves the executor needs from it.
///
/// `inspect` and `apply` are separate methods rather than one `execute` because
/// the executor calls `inspect` **twice** — once before the mutation to find out
/// whether it is needed, and once after to find out whether it happened. An
/// operation that folded the read into the write could not answer the second
/// question at all, which is the question the milestone is about.
///
/// `minimum` is the capability's own floor for this operation, declared in Rust
/// by whoever wrote it, and is one of the two inputs to [`combine`].
#[async_trait::async_trait]
pub trait IntegrationOperation: Send + Sync + Sized {
    /// What a successful observation of the postcondition looks like.
    type State: ObservedState + Send;

    /// Whether this operation will ever act unattended.
    fn minimum(&self) -> HumanDecisionRequirement;

    /// The canonical payload this operation would apply — the full request,
    /// normalized and order-stable, in the same spelling a
    /// [`ProposedEffect`](fiddle_core::ProposedEffect) carries.
    ///
    /// This is the second half of the identity/payload split, and it is on the
    /// trait rather than on each concrete operation so that step 6 can ask it of
    /// *every* operation. A proposal names a payload and an operation performs
    /// one, and until this existed nothing tied the two together: a capability
    /// could have the executor derive, hash and authorize one request while the
    /// adapter sent another, and the identity would be unchanged, so it would
    /// arrive looking like the same work.
    ///
    /// **No default.** For the reason [`EffectTrace`] has no default sink: an
    /// operation that answered "nothing in particular" by omission would opt
    /// itself out of the step 6 check without anybody deciding to, and the check
    /// would then hold for the operations somebody remembered.
    fn payload(&self) -> String;

    /// Read the world. `Ok(None)` is knowledge — the postcondition is absent —
    /// and never a failure to look.
    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<Self::State>, GhError>;

    /// Perform the mutation. Reached only with an [`AuthorizedEffect`] in hand,
    /// so an adapter cannot be called without identity, payload and policy
    /// having been checked for this exact request.
    async fn apply(
        &self,
        ctx: &EffectContext,
        authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError>;
}

/// A runtime capability token: proof that identity, policy and payload were
/// checked for this exact request.
///
/// Every field is private and no constructor is offered, so the only place a
/// value of this type comes into existence is [`Executor::execute`], inside this
/// module — the same construction [`crate::capability::ExecutionGrant`] uses to
/// make "the capability is never executed from a blocked derivation" a property
/// of the types rather than of somebody's control flow. It is a runtime token
/// and not durable approval state: it lives for one call and is never written
/// down.
///
/// The three fields are the effect's identity, the digest of the payload it was
/// approved for, and the operation itself. The payload hash is carried *beside*
/// the identity rather than folded into it so that a request that is not the one
/// approved is visible against an unchanged effect — and
/// [`Executor::execute`] is where it is looked at: immediately after this value
/// is built, the digest it carries is compared against
/// [`IntegrationOperation::payload`]'s, and a mismatch refuses the mutation. The
/// "payload was checked" in the sentence above is that comparison and not a
/// figure of speech.
///
/// Nothing outside this module can build one. The path resolves —
///
/// ```
/// use fiddle_runtime::effect::AuthorizedEffect;
/// fn takes_an_envelope<T>(_: &AuthorizedEffect<T>) {}
/// ```
///
/// — and the struct literal does not:
///
/// ```compile_fail
/// use fiddle_runtime::effect::AuthorizedEffect;
/// use fiddle_runtime::core::{EffectId, PayloadHash};
///
/// let forged: AuthorizedEffect<()> = AuthorizedEffect {
///     effect_id: EffectId("0000000000000000".to_string()),
///     payload_hash: PayloadHash("0000000000000000".to_string()),
///     operation: (),
/// };
/// ```
pub struct AuthorizedEffect<T> {
    effect_id: EffectId,
    payload_hash: PayloadHash,
    operation: T,
}

impl<T> AuthorizedEffect<T> {
    /// The identity this effect was authorized under — what an adapter names the
    /// effect by out there, so a later process can find it again.
    pub fn effect_id(&self) -> &EffectId {
        &self.effect_id
    }

    /// The digest of the exact payload that was approved.
    ///
    /// Read by [`Executor::execute`] at step 6, against the payload the operation
    /// would actually apply. That is its only caller, and it is the point of the
    /// field: a private field nothing reads would be a record of an approval
    /// nobody checks.
    pub fn payload_hash(&self) -> &PayloadHash {
        &self.payload_hash
    }
}

/// The effect executor, bound to one capability.
///
/// Bound rather than parameterised per call: the runtime hands each capability
/// an executor already carrying that capability's id, and step 1 refuses any
/// proposal made under another. A capability — or a tool a model selected —
/// therefore cannot claim another capability's identity when proposing an
/// effect, because the identity is not something the proposal gets to supply.
pub struct Executor<'a> {
    capability: CapabilityId,
    project: String,
    invocation_ref: String,
    deployment: &'a dyn DeploymentPolicy,
    ctx: &'a EffectContext,
    trace: &'a dyn EffectTrace,
    /// What a postcondition read may spend settling. See [`ReadRetry`], and
    /// note that nothing in this struct is a budget for the *mutation*.
    read_retry: ReadRetry,
}

impl<'a> Executor<'a> {
    /// Build an executor for one capability, one project and one invocation,
    /// reporting its step order to `trace`.
    ///
    /// `project` and `invocation_ref` are here rather than in [`ProposedEffect`]
    /// because they are the run's identity and not the proposal's: a capability
    /// that could name them could name a different run's effect.
    ///
    /// `trace` is required rather than defaulted — see [`EffectTrace`] for why an
    /// optional sink was the wrong shape.
    ///
    /// `read_retry` is required for that same reason. A defaulted budget would
    /// be a wall-clock policy nobody wrote down: a caller that wanted one read
    /// would get a backoff by omission, and a caller that wanted the backoff
    /// would get one read by omission. [`ReadRetry::none`] is the name for the
    /// first, so choosing it is a line of code somebody wrote.
    pub fn new(
        capability: CapabilityId,
        project: String,
        invocation_ref: String,
        deployment: &'a dyn DeploymentPolicy,
        ctx: &'a EffectContext,
        trace: &'a dyn EffectTrace,
        read_retry: ReadRetry,
    ) -> Self {
        Self {
            capability,
            project,
            invocation_ref,
            deployment,
            ctx,
            trace,
            read_retry,
        }
    }

    /// The project half of this run's identity.
    ///
    /// # Why this is exposed at all
    ///
    /// Because an operation whose *lookup* happens at step 3 — before step 6
    /// mints the envelope — has to derive the same identity the executor will,
    /// and therefore has to be handed the same pair.
    /// [`EnsureCheckRequested::new`](crate::github::EnsureCheckRequested::new)
    /// is that operation: the dispatched run is *named* by the identity it
    /// computes and *found* by the identity it computes, so a caller that built
    /// the operation from one pair and the executor from another would name a
    /// run by one identity and look it up by the other. Every attempt would then
    /// find nothing and dispatch again — an unbounded supply of workflow runs,
    /// which is the failure this milestone exists to prevent.
    ///
    /// [`EnsureCheckRequested::apply`](crate::effect::IntegrationOperation::apply)
    /// refuses before the request when the two disagree, and that guard stays
    /// the backstop. This accessor is the other half: with the executor's own
    /// pair readable, a caller has no reason to hold a second copy, so the two
    /// cannot drift.
    pub fn project(&self) -> &str {
        &self.project
    }

    /// The invocation reference half of this run's identity. See
    /// [`Executor::project`] for why it is readable.
    pub fn invocation_ref(&self) -> &str {
        &self.invocation_ref
    }

    /// Read what CI says about one exact head.
    ///
    /// A *read*, so it mints no envelope, takes no policy decision and reaches
    /// [`IntegrationOperation`] not at all — there is nothing to authorize about
    /// looking. It lives on the executor anyway, and that placement is the
    /// point: the executor is a capability's whole window on the outside world.
    /// A capability that had to reach [`EffectContext::gh`] itself to ask this
    /// question would be holding the credential, which is the arrangement this
    /// type exists to prevent.
    ///
    /// Fails closed the way [`crate::github::observe_checks`] does — an
    /// unreadable CI is [`Observation::Unavailable`] and never an empty
    /// [`VerificationState`], which would read as "nothing is failing".
    pub async fn observe_checks(
        &self,
        repo: &str,
        head_sha: &str,
        required: &[String],
    ) -> Observation<VerificationState> {
        crate::github::observe_checks(&self.ctx.gh, repo, head_sha, required, &self.ctx.cancel)
            .await
    }

    /// Walk the authorization order for one proposed effect.
    ///
    /// The comments below are numbered with the PRD's steps, and the order they
    /// appear in is the contract. Each step is announced to the trace *before*
    /// the work behind it, so a recorded order that reaches `combine_policy`
    /// really did get past the postcondition inspection first.
    pub async fn execute<O>(
        &self,
        proposed: ProposedEffect,
        operation: O,
    ) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, EffectError>
    where
        O: IntegrationOperation,
    {
        let kind = proposed.kind;

        // 1. The proposing capability is the one this executor is bound to.
        //    First, and before anything looks at the world: a proposal made
        //    under someone else's name must not even be inspected for.
        self.trace.step(kind, ExecutionStep::ValidateCapability);
        if proposed.capability != self.capability {
            return Err(EffectError::PolicyDenied {
                kind,
                reason: format!(
                    "an executor bound to {} cannot propose for {}",
                    self.capability.0, proposed.capability.0
                ),
            });
        }

        // 2. Identity and payload hash, from canonical inputs alone — no clock,
        //    no counter, no local state — so the process that has to recognise
        //    this effect after a crash recomputes the same identity.
        self.trace.step(kind, ExecutionStep::DeriveIdentity);
        let effect_id = effect_id(&self.project, &self.invocation_ref, kind, &proposed.target);
        let payload_hash = payload_hash(&proposed.payload);

        // 3. Does the desired postcondition already hold? Before the mutation,
        //    and before policy: an effect the world already satisfies is not a
        //    request to act on, so there is nothing left to authorize.
        self.trace.step(kind, ExecutionStep::InspectPostcondition);
        match self
            .read_until_settled(&operation, &effect_id, Settle::WhenTheLookSucceeds)
            .await
            .observed
        {
            Ok(Some(state)) => {
                return Ok(receipt(
                    effect_id,
                    payload_hash,
                    proposed.target,
                    EffectOutcome::Committed,
                    state,
                ))
            }
            Ok(None) => {}
            Err(error) => return Err(adapter_failure(kind, error)),
        }

        // 4. Capability minimum combined with deployment policy. The document
        //    may strengthen this and may never weaken it; `combine` owns that
        //    rule and this is where its answer is acted on.
        self.trace.step(kind, ExecutionStep::CombinePolicy);
        match combine(operation.minimum(), self.deployment.rule_for(kind)) {
            PolicyDecision::Allow => {}
            PolicyDecision::Deny { reason } => {
                return Err(EffectError::PolicyDenied { kind, reason })
            }
            // M2 defines the variant and consumes it here. The channel that
            // would answer it is M3's; until then this fails closed and says
            // so, which is the correct behaviour for a decision channel that
            // does not exist yet — and it is what keeps the variant from
            // shipping inert.
            PolicyDecision::RequireHumanDecision { reason } => {
                return Err(EffectError::HumanDecisionRequired { kind, reason })
            }
        }

        // 5-6. The adapter handle is already resolved (`ctx.gh`); the envelope
        //      is minted here for this exact payload and nothing else.
        self.trace.step(kind, ExecutionStep::Authorize);
        let authorized = AuthorizedEffect {
            effect_id: effect_id.clone(),
            payload_hash: payload_hash.clone(),
            operation,
        };

        // ...and "this exact payload" is checked rather than asserted. The
        // envelope was minted for the payload the *proposal* carried; the
        // operation about to be applied has a canonical payload of its own, and
        // this is where the two are required to be the same request.
        //
        // This is the half of the identity/payload split that had no consumer.
        // The identity is derived from the target and never from the payload —
        // deliberately, so that rewording a pull request does not open a second
        // one — which means a proposal and an operation that disagree about the
        // request agree about the *identity*, and the disagreement arrives
        // looking like the same work. Approval is minted at step 6 and spent at
        // step 7; if the payload can change in between, then what step 4 allowed
        // and what step 7 performs are two different things.
        //
        // Refused **before** step 7, so nothing reaches the outside world under
        // an approval given for another request — the same shape as
        // `EnsureCheckRequested::apply`'s identity guard, which refuses before
        // dispatching for the same reason, and hoisted here so that all three
        // operations get it and none of them has to remember to.
        //
        // Step 3's early return needs no such check and deliberately does not
        // have one: it applies nothing. This exists to stop a *mutation* being
        // performed under the wrong approval, and an effect the world already
        // satisfies has no mutation to mis-authorize.
        let applying = fiddle_core::payload_hash(&authorized.operation.payload());
        if authorized.payload_hash() != &applying {
            return Err(EffectError::PayloadDiverged {
                kind,
                approved: authorized.payload_hash().clone(),
                applying,
            });
        }

        // 7. Delegate. This is the only line in the process that changes
        //    anything outside it, and it is reached exactly once per call.
        //
        //    There is no loop around this line and there must never be one. The
        //    read below retries; the write does not, because a read is
        //    idempotent and a write whose answer was lost is not. Making the two
        //    symmetric would manufacture exactly the duplicate external effect
        //    this milestone exists to prevent — see `read_until_settled`.
        self.trace.step(kind, ExecutionStep::Apply);
        let dispatched = authorized.operation.apply(self.ctx, &authorized).await;

        // 8. Observe the postcondition. Whatever the dispatch said, the world is
        //    the authority — a response that never arrived cannot be believed,
        //    and a response that claimed success cannot be either.
        //
        //    This is where the budget is spent, because this is the read that
        //    can be racing a write that really landed: GitHub does not list a
        //    dispatched run, or answer for a just-pushed ref, the instant it
        //    accepted either.
        self.trace.step(kind, ExecutionStep::ObservePostcondition);
        let settled = self
            .read_until_settled(
                &authorized.operation,
                &effect_id,
                Settle::WhenThePostconditionAppears,
            )
            .await;
        // Named once so both unresolved leaves say the same thing about the
        // budget. A diagnostic that read as though the executor looked once and
        // gave up would send an operator to add a wait that is already there.
        let spent = spent(settled.reads);

        match settled.observed {
            // The world agrees, however the dispatch ended. This arm is what
            // turns a lost answer into a settled one, and it is reached by the
            // 422, the killed-`gh` and the cancelled-`gh` paths alike. The
            // mutation is not re-dispatched to get here; it is read.
            Ok(Some(state)) => Ok(receipt(
                effect_id,
                payload_hash,
                proposed.target,
                EffectOutcome::Committed,
                state,
            )),
            // More objects than the postcondition allows is a state to report,
            // never a set to pick from — including when the extra one appeared
            // during this very call.
            Err(GhError::Duplicate { count }) => Err(EffectError::DuplicateState { kind, count }),
            Ok(None) => match dispatched {
                // A refusal that leaves no room for the write having happened,
                // against a world that agrees it did not. The refusal stands as
                // the answer; calling this unresolved would send a caller to
                // investigate a settled failure.
                Err(error) if error.outcome() == EffectOutcome::NotCommitted => {
                    Err(adapter_failure(kind, error))
                }
                // The answer was lost and the world does not show the write. It
                // may still be in flight, so this is not `NotCommitted`.
                Err(error) => Err(EffectError::Unresolved {
                    kind,
                    reason: format!(
                        "the write was not observed{spent} and its answer was lost: {error}"
                    ),
                }),
                // The adapter claimed success and the world does not show it.
                // Believing the response over the world is precisely what step 8
                // exists to prevent — and so is believing the *budget*: a read
                // that has run out of attempts has not turned an absence into a
                // success, it has only stopped asking.
                Ok(()) => Err(EffectError::Unresolved {
                    kind,
                    reason: format!(
                        "the adapter reported success and the postcondition was \
                         not observed{spent}"
                    ),
                }),
            },
            Err(read_error) => match dispatched {
                // A confident refusal is not made less confident by a read that
                // failed afterwards.
                Err(error) if error.outcome() == EffectOutcome::NotCommitted => {
                    Err(adapter_failure(kind, error))
                }
                // Everything else: nobody knows, and the read that was supposed
                // to settle it did not. This must stay its own leaf rather than
                // degrading to `Committed` or `NotCommitted` — a caller told
                // "failed" here retries a write that may have landed.
                //
                // **The dispatch failure is named beside the read failure**, and
                // that is not decoration. This leaf is the only place where the
                // classification of a lost answer is the whole of what the
                // executor had left to reason from, so a diagnostic that quoted
                // only the read would tell an operator that something is
                // unresolved without telling them *what* may have landed — and a
                // suite reading this field could not tell which ambiguity it was
                // handed. That is how M2's harness came to inject a killed child
                // four times over without ever observing how it was classified.
                unsettled => Err(EffectError::Unresolved {
                    kind,
                    reason: format!(
                        "the outcome was unknown{} and the postcondition could \
                         not be read{spent}: {read_error}",
                        match &unsettled {
                            Err(error) => format!(" ({error})"),
                            // The adapter claimed success and the read that would
                            // have confirmed it failed. There is no second failure
                            // to name.
                            Ok(()) => String::new(),
                        }
                    ),
                }),
            },
        }
    }

    /// Read the world until it settles, or until the budget runs out.
    ///
    /// # Retry the READ. Never the mutation.
    ///
    /// This asymmetry is the milestone's central rule, and this function is
    /// where it is enforced rather than merely believed. The next person here
    /// will be tempted to make it symmetric — a failed `apply` looks like the
    /// same kind of transient problem a failed `inspect` does — so:
    ///
    /// A read is idempotent by nature. Asking again costs a request and risks
    /// nothing, so waiting for a consistent answer is free in the only currency
    /// that matters. A mutation is not. Re-dispatching a write whose answer was
    /// lost is precisely how a duplicate branch, a duplicate pull request or a
    /// second workflow run is born, and `Unknown` exists in
    /// [`EffectOutcome`] so that the ambiguity is resolved by *looking*.
    /// [`IntegrationOperation::apply`] is called exactly once per
    /// [`Executor::execute`], on every path, and no branch in this function
    /// changes that — it cannot, because it is not given anything that could
    /// dispatch one.
    ///
    /// Running out of attempts returns the *last* observation unchanged, so the
    /// caller's own reasoning decides what it means. Exhaustion never invents an
    /// answer, and in particular never turns an absence into a success.
    async fn read_until_settled<O: IntegrationOperation>(
        &self,
        operation: &O,
        effect: &EffectId,
        settle: Settle,
    ) -> Settled<O::State> {
        let mut reads: u32 = 0;
        loop {
            let observed = operation.inspect(self.ctx).await;
            reads += 1;

            let advice = match &observed {
                // Settled: the world says the postcondition holds.
                Ok(Some(_)) => return Settled { observed, reads },
                Ok(None) => match settle {
                    Settle::WhenTheLookSucceeds => return Settled { observed, reads },
                    // Nothing was said about waiting, so the backoff supplies
                    // its own number.
                    Settle::WhenThePostconditionAppears => RetryAdvice::default(),
                },
                // A refusal, a runner that will not repair itself, a read nothing
                // was started for, or a second matching object.
                // `is_worth_reading_again` is where each of those is judged, and
                // it is a different question from `outcome`: a garbled answer is
                // `Unknown` and still not worth another read. A `false` here is a
                // settled answer that happens to be a failure.
                Err(error) if !error.is_worth_reading_again() => {
                    return Settled { observed, reads }
                }
                Err(error) => error.advice(),
            };

            if reads >= self.read_retry.attempts() {
                return Settled { observed, reads };
            }

            let delay = self.read_retry.delay(reads, advice, effect);
            tokio::select! {
                // A cancelled run must stop rather than finish waiting. It
                // leaves with the last observation it has, which is an honest
                // one: nobody looked again.
                //
                // **This is about how long a cancelled run may spend, not about
                // what its failure meant.** A mutation whose answer was lost to
                // the same cancellation is still `Unknown`
                // ([`GhError::CancelledAfterSpawn`]), so the arms below report
                // `Unresolved` rather than a settled failure, and the fresh
                // process that follows settles it by reading. What this arm does
                // cost is the *settling read on the cancelled path*: step 8's
                // single `inspect` above is itself refused before spawning,
                // because the token it is given is the cancelled one. Letting one
                // read through would need a second cancellation channel in
                // `EffectContext` and would make a `^C` non-prompt, so it is
                // recorded in `docs/BACKLOG.md` rather than taken here.
                _ = self.ctx.cancel.cancelled() => return Settled { observed, reads },
                _ = self.read_retry.clock.wait(delay) => {}
            }
        }
    }
}

/// How the budget was spent, phrased for the end of an unresolved diagnostic.
///
/// Empty for a single read, so the ordinary message is unchanged and only a run
/// that really waited says it did.
fn spent(reads: u32) -> String {
    match reads {
        0 | 1 => String::new(),
        reads => format!(" over {reads} reads"),
    }
}

/// One receipt, built from an observation rather than from a response.
///
/// Written once rather than at each of its two call sites, so the two cannot
/// drift into describing the same observation differently.
fn receipt<S: ObservedState>(
    effect_id: EffectId,
    payload_hash: PayloadHash,
    target: String,
    outcome: EffectOutcome,
    state: S,
) -> EffectReceipt<S::Value> {
    EffectReceipt {
        effect_id,
        payload_hash,
        target,
        outcome,
        postcondition: state.describe(),
        external_ref: state.reference(),
        value: state.into_value(),
    }
}

/// Carry an adapter failure into the effect vocabulary.
///
/// [`GhError::Duplicate`] is the one that does not stay an adapter failure: the
/// count is the actionable half, and burying it inside a `source` would leave a
/// caller to parse it back out of a message.
fn adapter_failure(kind: EffectKind, error: GhError) -> EffectError {
    match error {
        GhError::Duplicate { count } => EffectError::DuplicateState { kind, count },
        source => EffectError::Adapter { kind, source },
    }
}
