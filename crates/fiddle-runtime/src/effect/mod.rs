pub mod receipt;
pub mod registry;

pub use receipt::{EffectError, EffectReceipt, ObservedState, Recurrence};
pub use registry::{describe, install, registered, EffectDescriptor, RegistryError, BUILT_IN};

use crate::git::GitCli;
use crate::github::{GhCli, GhError, RetryAdvice};
use fiddle_core::{
    combine, effect_id, payload_hash, CapabilityId, DecisionBinding, DeploymentRule, EffectId,
    EffectName, HumanDecisionRequirement, InterpretedHumanDecision, Observation, PayloadHash,
    PolicyDecision, ProposedEffect, VerificationState,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectOutcome {
    Committed,
    NotCommitted,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectPhase {
    Inspect,
    Apply,
}

pub trait AdapterError: std::error::Error + Send + Sync + 'static {
    fn outcome(&self, phase: EffectPhase) -> EffectOutcome;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionStep {
    ValidateCapability,
    DeriveIdentity,
    InspectPostcondition,
    CombinePolicy,
    ResolveDecision,
    Authorize,
    Apply,
    ObservePostcondition,
}

impl ExecutionStep {
    pub fn as_str(&self) -> &'static str {
        match self {
            ExecutionStep::ValidateCapability => "validate_capability",
            ExecutionStep::DeriveIdentity => "derive_identity",
            ExecutionStep::InspectPostcondition => "inspect_postcondition",
            ExecutionStep::CombinePolicy => "combine_policy",
            ExecutionStep::ResolveDecision => "resolve_decision",
            ExecutionStep::Authorize => "authorize",
            ExecutionStep::Apply => "apply",
            ExecutionStep::ObservePostcondition => "observe_postcondition",
        }
    }
}

pub trait EffectTrace: Send + Sync {
    fn step(&self, kind: &EffectName, step: ExecutionStep);
}

pub trait DeploymentPolicy: Send + Sync {
    fn rule_for(&self, kind: &EffectName) -> DeploymentRule;
}

pub struct EffectContext {
    pub gh: GhCli,
    pub git: GitCli,
    pub work: PathBuf,
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

#[async_trait::async_trait]
pub trait ReadClock: Send + Sync + std::fmt::Debug {
    async fn wait(&self, delay: Duration);
}

#[derive(Debug)]
pub struct SleepingClock;

#[async_trait::async_trait]
impl ReadClock for SleepingClock {
    async fn wait(&self, delay: Duration) {
        tokio::time::sleep(delay).await;
    }
}

#[derive(Clone, Debug)]
pub struct ReadRetry {
    attempts: u32,
    initial: Duration,
    max: Duration,
    clock: Arc<dyn ReadClock>,
}

impl ReadRetry {
    pub fn bounded(attempts: u32, initial: Duration, max: Duration) -> Self {
        Self::served_by(attempts, initial, max, Arc::new(SleepingClock))
    }

    pub fn none() -> Self {
        Self::bounded(1, Duration::ZERO, Duration::ZERO)
    }

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

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Settle {
    WhenTheLookSucceeds,
    WhenThePostconditionAppears,
}

struct Settled<S> {
    observed: Result<Option<S>, GhError>,
    reads: u32,
}

#[async_trait::async_trait]
pub trait IntegrationOperation: Send + Sync + Sized {
    type State: ObservedState + Send;

    type Error: AdapterError;

    fn minimum(&self) -> HumanDecisionRequirement;

    fn payload(&self) -> String;

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<Self::State>, Self::Error>;

    async fn apply(
        &self,
        ctx: &EffectContext,
        authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), Self::Error>;
}

pub struct ResolvedDecision {
    binding: DecisionBinding,
}

impl ResolvedDecision {
    pub fn approved(binding: DecisionBinding, verdict: &InterpretedHumanDecision) -> Option<Self> {
        matches!(verdict, InterpretedHumanDecision::Approve).then_some(Self { binding })
    }

    pub fn binding(&self) -> &DecisionBinding {
        &self.binding
    }
}

/// ```
/// use fiddle_runtime::effect::AuthorizedEffect;
/// fn takes_an_envelope<T>(_: &AuthorizedEffect<T>) {}
/// ```
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
    pub fn effect_id(&self) -> &EffectId {
        &self.effect_id
    }

    pub fn payload_hash(&self) -> &PayloadHash {
        &self.payload_hash
    }
}

pub struct Executor<'a> {
    capability: CapabilityId,
    project: String,
    invocation_ref: String,
    deployment: &'a dyn DeploymentPolicy,
    ctx: &'a EffectContext,
    trace: &'a dyn EffectTrace,
    read_retry: ReadRetry,
}

impl<'a> Executor<'a> {
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

    pub fn project(&self) -> &str {
        &self.project
    }

    pub fn invocation_ref(&self) -> &str {
        &self.invocation_ref
    }

    pub fn git(&self) -> &GitCli {
        &self.ctx.git
    }

    pub async fn observe_checks(
        &self,
        repo: &str,
        head_sha: &str,
        required: &[String],
    ) -> Observation<VerificationState> {
        crate::github::observe_checks(&self.ctx.gh, repo, head_sha, required, &self.ctx.cancel)
            .await
    }

    pub async fn execute<O>(
        &self,
        proposed: ProposedEffect,
        operation: O,
    ) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, EffectError>
    where
        O: IntegrationOperation<Error = GhError>,
    {
        self.walk(proposed, operation, None).await
    }

    pub async fn execute_decided<O>(
        &self,
        proposed: ProposedEffect,
        operation: O,
        decision: &ResolvedDecision,
    ) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, EffectError>
    where
        O: IntegrationOperation<Error = GhError>,
    {
        self.walk(proposed, operation, Some(decision)).await
    }

    async fn walk<O>(
        &self,
        proposed: ProposedEffect,
        operation: O,
        decision: Option<&ResolvedDecision>,
    ) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, EffectError>
    where
        O: IntegrationOperation<Error = GhError>,
    {
        let kind = proposed.kind.clone();

        if registry::describe(&kind).is_none() {
            return Err(EffectError::UnknownEffect { kind });
        }

        self.trace.step(&kind, ExecutionStep::ValidateCapability);
        if proposed.capability != self.capability {
            return Err(EffectError::PolicyDenied {
                kind: kind.clone(),
                reason: format!(
                    "an executor bound to {} cannot propose for {}",
                    self.capability.0, proposed.capability.0
                ),
            });
        }

        self.trace.step(&kind, ExecutionStep::DeriveIdentity);
        let effect_id = effect_id(
            &self.project,
            &self.invocation_ref,
            kind.as_str(),
            &proposed.target,
        );
        let payload_hash = payload_hash(&proposed.payload);

        self.trace.step(&kind, ExecutionStep::InspectPostcondition);
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
            Err(error) => return Err(adapter_failure(&kind, error)),
        }

        self.trace.step(&kind, ExecutionStep::CombinePolicy);
        match combine(operation.minimum(), self.deployment.rule_for(&kind)) {
            PolicyDecision::Allow => {}
            PolicyDecision::Deny { reason } => {
                return Err(EffectError::PolicyDenied {
                    kind: kind.clone(),
                    reason,
                })
            }
            PolicyDecision::RequireHumanDecision { reason } => match decision {
                None => {
                    return Err(EffectError::HumanDecisionRequired {
                        kind: kind.clone(),
                        reason,
                    })
                }
                Some(decision) => {
                    self.trace.step(&kind, ExecutionStep::ResolveDecision);
                    let binding = decision.binding();

                    if binding.effect != effect_id {
                        return Err(EffectError::HumanDecisionRequired {
                            kind: kind.clone(),
                            reason: format!(
                                "the decision in hand answers effect {} and this is {}, \
                                 so nothing has answered it yet: {reason}",
                                binding.effect.0, effect_id.0
                            ),
                        });
                    }

                    if binding.payload != payload_hash {
                        return Err(EffectError::PayloadDiverged {
                            kind: kind.clone(),
                            approved: binding.payload.clone(),
                            applying: payload_hash.clone(),
                        });
                    }
                }
            },
        }

        self.trace.step(&kind, ExecutionStep::Authorize);
        let authorized = AuthorizedEffect {
            effect_id: effect_id.clone(),
            payload_hash: payload_hash.clone(),
            operation,
        };

        let applying = fiddle_core::payload_hash(&authorized.operation.payload());
        if authorized.payload_hash() != &applying {
            return Err(EffectError::PayloadDiverged {
                kind: kind.clone(),
                approved: authorized.payload_hash().clone(),
                applying,
            });
        }

        self.trace.step(&kind, ExecutionStep::Apply);
        let dispatched = authorized.operation.apply(self.ctx, &authorized).await;

        self.trace.step(&kind, ExecutionStep::ObservePostcondition);
        let settled = self
            .read_until_settled(
                &authorized.operation,
                &effect_id,
                Settle::WhenThePostconditionAppears,
            )
            .await;
        let spent = spent(settled.reads);

        match settled.observed {
            Ok(Some(state)) => Ok(receipt(
                effect_id,
                payload_hash,
                proposed.target,
                EffectOutcome::Committed,
                state,
            )),
            Err(GhError::Duplicate { count }) => Err(EffectError::DuplicateState {
                kind: kind.clone(),
                count,
            }),
            Ok(None) => match dispatched {
                Err(error) if error.outcome(EffectPhase::Apply) == EffectOutcome::NotCommitted => {
                    Err(adapter_failure(&kind, error))
                }
                Err(error) => Err(EffectError::Unresolved {
                    kind: kind.clone(),
                    reason: format!(
                        "the write was not observed{spent} and its answer was lost: {error}"
                    ),
                }),
                Ok(()) => Err(EffectError::Unresolved {
                    kind: kind.clone(),
                    reason: format!(
                        "the adapter reported success and the postcondition was \
                         not observed{spent}"
                    ),
                }),
            },
            Err(read_error) => match dispatched {
                Err(error) if error.outcome(EffectPhase::Apply) == EffectOutcome::NotCommitted => {
                    Err(adapter_failure(&kind, error))
                }
                unsettled => Err(EffectError::Unresolved {
                    kind: kind.clone(),
                    reason: format!(
                        "the outcome was unknown{} and the postcondition could \
                         not be read{spent}: {read_error}",
                        match &unsettled {
                            Err(error) => format!(" ({error})"),
                            Ok(()) => String::new(),
                        }
                    ),
                }),
            },
        }
    }

    async fn read_until_settled<O: IntegrationOperation<Error = GhError>>(
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
                Ok(Some(_)) => return Settled { observed, reads },
                Ok(None) => match settle {
                    Settle::WhenTheLookSucceeds => return Settled { observed, reads },
                    Settle::WhenThePostconditionAppears => RetryAdvice::default(),
                },
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
                _ = self.ctx.cancel.cancelled() => return Settled { observed, reads },
                _ = self.read_retry.clock.wait(delay) => {}
            }
        }
    }
}

fn spent(reads: u32) -> String {
    match reads {
        0 | 1 => String::new(),
        reads => format!(" over {reads} reads"),
    }
}

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

fn adapter_failure(kind: &EffectName, error: GhError) -> EffectError {
    match error {
        GhError::Duplicate { count } => EffectError::DuplicateState {
            kind: kind.clone(),
            count,
        },
        source => EffectError::Adapter {
            kind: kind.clone(),
            source,
        },
    }
}
