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

pub use receipt::{EffectError, EffectReceipt, ObservedState};

use crate::git::GitCli;
use crate::github::{GhCli, GhError};
use fiddle_core::{
    combine, effect_id, payload_hash, CapabilityId, DeploymentRule, EffectId, EffectKind,
    HumanDecisionRequirement, Observation, PayloadHash, PolicyDecision, ProposedEffect,
    VerificationState,
};
use std::path::PathBuf;
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

/// Where the executor writes down which step it is on.
///
/// This is an observation seam of the same family as [`crate::ports`] — a trait
/// the runtime calls unconditionally, with the sink supplied by whoever built
/// the executor. The default sink discards; the deterministic suite records and
/// asserts the order. Unlike a configuration value that parses and is read by
/// nothing, this is *called* on every execution: what varies is where the
/// steps go, not whether they are produced.
pub trait EffectTrace: Send + Sync {
    fn step(&self, step: ExecutionStep);
}

/// The sink for a run that is not being observed.
struct NoTrace;

impl EffectTrace for NoTrace {
    fn step(&self, _step: ExecutionStep) {}
}

static NO_TRACE: NoTrace = NoTrace;

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
    /// stops before spawning rather than after.
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
/// the identity rather than folded into it so that a request widened after
/// approval is visible against an unchanged effect.
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
}

impl<'a> Executor<'a> {
    /// Build an executor for one capability, one project and one invocation.
    ///
    /// `project` and `invocation_ref` are here rather than in [`ProposedEffect`]
    /// because they are the run's identity and not the proposal's: a capability
    /// that could name them could name a different run's effect.
    pub fn new(
        capability: CapabilityId,
        project: String,
        invocation_ref: String,
        deployment: &'a dyn DeploymentPolicy,
        ctx: &'a EffectContext,
    ) -> Self {
        Self {
            capability,
            project,
            invocation_ref,
            deployment,
            ctx,
            trace: &NO_TRACE,
        }
    }

    /// Send this executor's step order somewhere it can be read.
    pub fn observed_by(mut self, trace: &'a dyn EffectTrace) -> Self {
        self.trace = trace;
        self
    }

    /// The capability this executor proposes on behalf of.
    ///
    /// Readable so a caller can check the binding it was handed rather than
    /// discover the mismatch at step 1; step 1 is still what refuses.
    pub fn capability(&self) -> CapabilityId {
        self.capability
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
        self.trace.step(ExecutionStep::ValidateCapability);
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
        self.trace.step(ExecutionStep::DeriveIdentity);
        let effect_id = effect_id(&self.project, &self.invocation_ref, kind, &proposed.target);
        let payload_hash = payload_hash(&proposed.payload);

        // 3. Does the desired postcondition already hold? Before the mutation,
        //    and before policy: an effect the world already satisfies is not a
        //    request to act on, so there is nothing left to authorize.
        self.trace.step(ExecutionStep::InspectPostcondition);
        match operation.inspect(self.ctx).await {
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
        self.trace.step(ExecutionStep::CombinePolicy);
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
        self.trace.step(ExecutionStep::Authorize);
        let authorized = AuthorizedEffect {
            effect_id: effect_id.clone(),
            payload_hash: payload_hash.clone(),
            operation,
        };

        // 7. Delegate. This is the only line in the process that changes
        //    anything outside it, and it is reached exactly once per call.
        self.trace.step(ExecutionStep::Apply);
        let dispatched = authorized.operation.apply(self.ctx, &authorized).await;

        // 8. Observe the postcondition. Whatever the dispatch said, the world is
        //    the authority — a response that never arrived cannot be believed,
        //    and a response that claimed success cannot be either.
        self.trace.step(ExecutionStep::ObservePostcondition);
        let observed = authorized.operation.inspect(self.ctx).await;

        match observed {
            // The world agrees, however the dispatch ended. This arm is what
            // turns a lost answer into a settled one, and it is reached by the
            // 422 and the killed-`gh` paths alike. The mutation is not
            // re-dispatched to get here; it is read.
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
                        "the write was not observed and its answer was lost: {error}"
                    ),
                }),
                // The adapter claimed success and the world does not show it.
                // Believing the response over the world is precisely what step 8
                // exists to prevent.
                Ok(()) => Err(EffectError::Unresolved {
                    kind,
                    reason: "the adapter reported success and the postcondition was not observed"
                        .to_string(),
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
                _ => Err(EffectError::Unresolved {
                    kind,
                    reason: format!(
                        "the outcome was unknown and the postcondition could not be read: {read_error}"
                    ),
                }),
            },
        }
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
