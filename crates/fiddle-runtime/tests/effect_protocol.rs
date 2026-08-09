//! The effect executor's protocol, proven in process and offline.
//!
//! Every case here is an *ambiguity* case: what the executor does when it does
//! not know whether a write landed. None of them reaches GitHub, and none of
//! them spawns a process — the world is a scripted [`IntegrationOperation`], so
//! the properties the milestone turns on are decided by the executor rather
//! than by whatever a network happened to do that afternoon.
//!
//! The one rule underneath all of it: **`Unknown` is resolved by reading the
//! world, never by retrying the mutation.** A retry there is how a duplicate
//! external effect is born, so the mutation dispatch count is asserted directly
//! rather than inferred from an outcome.

use async_trait::async_trait;
use fiddle_core::{
    effect_id, payload_hash, CapabilityId, DeploymentRule, EffectKind, HumanDecisionRequirement,
    ProposedEffect, FIXTURE_REPAIR, STUB_MARK,
};
use fiddle_runtime::effect::{
    AuthorizedEffect, DeploymentPolicy, EffectContext, EffectError, EffectOutcome, EffectTrace,
    ExecutionStep, Executor, IntegrationOperation, ObservedState,
};
use fiddle_runtime::{GhCli, GhError};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const PROJECT: &str = "acme/widget";
const INVOCATION_REF: &str = "beans:w-1";
const TARGET: &str = "refs/heads/fiddle/abc";
const PAYLOAD: &str = r#"{"sha":"deadbeef"}"#;

// ---------------------------------------------------------------------------
// The scripted world
// ---------------------------------------------------------------------------

/// What the outside world does when the executor talks to it.
///
/// Each variant is one of the situations the executor exists to tell apart, and
/// they are deliberately stated as *world behaviour* rather than as expected
/// outcomes: the test says what happened out there, and the executor is what
/// decides what that means.
#[derive(Clone, Copy, Debug)]
enum Script {
    /// The postcondition already holds. Nothing should be written.
    AlreadySatisfied,
    /// The ordinary path: absent, written, then observed.
    AbsentThenWritten,
    /// The write really lands and the answer is really lost — the shape the
    /// scripted `gh` reproduces by mutating and *then* dying.
    WriteLandsAnswerLost,
    /// The answer is lost and the postcondition read then fails too, so nothing
    /// settles the question.
    WriteLostReadFails,
    /// Two objects match where at most one was the postcondition.
    TwoMatch,
    /// GitHub refused in terms that leave no room for the write having landed,
    /// and the world agrees it did not.
    ConfidentRefusal,
    /// The adapter reported success and the world does not show it. Neither
    /// half is evidence enough on its own.
    SuccessWithoutPostcondition,
}

/// Everything that happened out there, recorded in order.
///
/// `writes` and `dispatches` are separate on purpose: a mutation that was
/// *asked for* and a mutation that *changed something* are the two numbers a
/// duplicate hides between.
#[derive(Debug)]
struct World {
    script: Script,
    landed: AtomicBool,
    dispatches: AtomicUsize,
    writes: AtomicUsize,
    calls: Mutex<Vec<&'static str>>,
    steps: Mutex<Vec<&'static str>>,
}

impl World {
    fn new(script: Script) -> Self {
        Self {
            script,
            landed: AtomicBool::new(false),
            dispatches: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
            calls: Mutex::new(Vec::new()),
            steps: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, call: &'static str) {
        self.calls.lock().unwrap().push(call);
    }

    /// How many times the world actually changed.
    fn mutations(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }

    /// How many times a mutation was dispatched, landed or not. The number a
    /// retry would move and a postcondition read would not.
    fn mutation_requests(&self) -> usize {
        self.dispatches.load(Ordering::SeqCst)
    }

    /// Did the executor go and look after the answer was lost?
    fn read_after_unknown(&self) -> bool {
        let calls = self.calls.lock().unwrap();
        match calls.iter().position(|call| *call == "apply") {
            Some(at) => calls[at + 1..].contains(&"inspect"),
            None => false,
        }
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}

/// The executor writes down which step of the authorization order it is on, and
/// the world keeps the list. This is what makes the *order* assertable rather
/// than only the endpoints.
impl EffectTrace for World {
    fn step(&self, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

/// What the deployment document says. One rule for every kind, because the
/// combination rule itself is exhaustively tested in `fiddle-core`.
struct Deployment(DeploymentRule);

impl DeploymentPolicy for Deployment {
    fn rule_for(&self, _kind: EffectKind) -> DeploymentRule {
        self.0
    }
}

/// The observed postcondition of the scripted operation.
#[derive(Debug)]
struct BranchState {
    sha: String,
}

impl ObservedState for BranchState {
    type Value = String;

    fn describe(&self) -> String {
        format!("branch at {}", self.sha)
    }

    fn reference(&self) -> Option<String> {
        Some(self.sha.clone())
    }

    fn into_value(self) -> String {
        self.sha
    }
}

/// A scripted operation. It never reaches `ctx`, which is why this suite proves
/// the executor's protocol without a process, a credential or a network.
struct ScriptedOperation<'w> {
    world: &'w World,
    minimum: HumanDecisionRequirement,
}

#[async_trait]
impl IntegrationOperation for ScriptedOperation<'_> {
    type State = BranchState;

    fn minimum(&self) -> HumanDecisionRequirement {
        self.minimum
    }

    async fn inspect(&self, _ctx: &EffectContext) -> Result<Option<BranchState>, GhError> {
        self.world.record("inspect");
        let present = || {
            Ok(Some(BranchState {
                sha: "deadbeef".to_string(),
            }))
        };
        match self.world.script {
            Script::AlreadySatisfied => present(),
            Script::TwoMatch => Err(GhError::Duplicate { count: 2 }),
            // The read itself fails only *after* the write was attempted; the
            // first look has to succeed or the executor would never get as far
            // as the case under test.
            Script::WriteLostReadFails => match self.world.landed.load(Ordering::SeqCst) {
                false => Ok(None),
                true => Err(GhError::Http {
                    status: 500,
                    message: "the postcondition could not be read".to_string(),
                }),
            },
            Script::ConfidentRefusal | Script::SuccessWithoutPostcondition => Ok(None),
            Script::AbsentThenWritten | Script::WriteLandsAnswerLost => {
                match self.world.landed.load(Ordering::SeqCst) {
                    false => Ok(None),
                    true => present(),
                }
            }
        }
    }

    async fn apply(
        &self,
        _ctx: &EffectContext,
        authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        self.world.record("apply");
        self.world.dispatches.fetch_add(1, Ordering::SeqCst);
        // The envelope reaches the adapter carrying the identity that was
        // derived for this exact request; an adapter that needs to name the
        // effect out there has this and nothing else to name it with.
        assert_eq!(
            authorized.effect_id(),
            &effect_id(
                PROJECT,
                INVOCATION_REF,
                EffectKind::EnsureBranchPublished,
                TARGET
            ),
            "the envelope must carry the identity derived for this request"
        );

        let land = |world: &World| {
            world.landed.store(true, Ordering::SeqCst);
            world.writes.fetch_add(1, Ordering::SeqCst);
        };
        match self.world.script {
            Script::AbsentThenWritten => {
                land(self.world);
                Ok(())
            }
            // Both halves of an ambiguous write, in one place: the world really
            // changed and the answer really did not come back.
            Script::WriteLandsAnswerLost => {
                land(self.world);
                Err(GhError::Killed("signal".to_string()))
            }
            Script::WriteLostReadFails => {
                self.world.landed.store(true, Ordering::SeqCst);
                Err(GhError::Killed("signal".to_string()))
            }
            Script::ConfidentRefusal => Err(GhError::Http {
                status: 403,
                message: "resource not accessible".to_string(),
            }),
            Script::SuccessWithoutPostcondition => Ok(()),
            Script::AlreadySatisfied | Script::TwoMatch => {
                panic!("this world must never be written to")
            }
        }
    }
}

/// One executor, one world, one deployment rule, held together so the executor
/// can borrow them all.
struct Harness {
    world: World,
    ctx: EffectContext,
    deployment: Deployment,
    capability: CapabilityId,
    minimum: HumanDecisionRequirement,
}

impl Harness {
    fn new(script: Script) -> Self {
        Self {
            world: World::new(script),
            ctx: unreachable_context(),
            deployment: Deployment(DeploymentRule::Allow),
            capability: FIXTURE_REPAIR,
            minimum: HumanDecisionRequirement::Automatic,
        }
    }

    fn with_policy(mut self, minimum: HumanDecisionRequirement, rule: DeploymentRule) -> Self {
        self.minimum = minimum;
        self.deployment = Deployment(rule);
        self
    }

    fn executor(&self) -> Executor<'_> {
        Executor::new(
            self.capability,
            PROJECT.to_string(),
            INVOCATION_REF.to_string(),
            &self.deployment,
            &self.ctx,
        )
        .observed_by(&self.world)
    }

    fn operation(&self) -> ScriptedOperation<'_> {
        ScriptedOperation {
            world: &self.world,
            minimum: self.minimum,
        }
    }
}

/// A context nothing in this suite reaches.
///
/// The scripted operation ignores it, so the `gh` inside it is never spawned
/// and the program path is deliberately one that does not exist: if a future
/// change made the executor talk to GitHub behind the operation's back, these
/// tests would fail loudly rather than quietly acquire a dependency on a
/// network.
fn unreachable_context() -> EffectContext {
    EffectContext::new(
        GhCli::new(
            PathBuf::from("/nonexistent/gh"),
            Vec::new(),
            String::new(),
            "GH_TOKEN",
            PathBuf::from("/nonexistent"),
            Duration::from_secs(1),
        ),
        CancellationToken::new(),
    )
}

fn branch_effect() -> ProposedEffect {
    proposed_by(FIXTURE_REPAIR)
}

fn proposed_by(capability: CapabilityId) -> ProposedEffect {
    ProposedEffect {
        capability,
        kind: EffectKind::EnsureBranchPublished,
        target: TARGET.to_string(),
        payload: PAYLOAD.to_string(),
    }
}

// ---------------------------------------------------------------------------
// The envelope
// ---------------------------------------------------------------------------

/// The envelope is only worth having if it cannot be forged. Receiving one must
/// prove identity, policy and payload were checked for this exact request.
///
/// The structural half of this proof is the `compile_fail` doctest on
/// [`AuthorizedEffect`] itself: this file is a *separate crate*, so if a struct
/// literal or a public constructor existed, that doctest would compile and fail
/// the suite. What is asserted here is the source-level half — that no
/// constructor is offered under a name a caller could reach for.
#[test]
fn the_authorization_envelope_has_no_public_constructor() {
    let source = include_str!("../src/effect/mod.rs");
    assert!(
        !source.contains("pub fn authorize") && !source.contains("pub const fn authorize"),
        "AuthorizedEffect must not be constructible outside the executor"
    );
    // Every field is private, so no struct literal works either.
    assert!(source.contains("pub struct AuthorizedEffect<T> {\n    effect_id:"));
}

// ---------------------------------------------------------------------------
// Step 3 before step 4, and both before the mutation
// ---------------------------------------------------------------------------

/// The postcondition is inspected *before* the mutation, so an effect that has
/// already happened is never performed a second time.
#[tokio::test]
async fn an_existing_postcondition_short_circuits_the_mutation() {
    let harness = Harness::new(Script::AlreadySatisfied);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.mutations(),
        0,
        "nothing was written; the world already agreed"
    );
    assert_eq!(
        harness.world.mutation_requests(),
        0,
        "and nothing was even dispatched"
    );
    assert_eq!(receipt.postcondition, "branch at deadbeef");
    assert_eq!(receipt.external_ref.as_deref(), Some("deadbeef"));
}

/// The stronger half of the same rule, and the one an endpoint-only test would
/// miss: an effect the world already satisfies is never *asked about*. Policy
/// is not consulted, so an effect that has already happened cannot be refused
/// for a rule it no longer needs.
#[tokio::test]
async fn an_already_satisfied_effect_is_never_put_to_policy() {
    let harness = Harness::new(Script::AlreadySatisfied)
        // The strictest policy there is. If the order were inverted, this would
        // deny an effect that has already happened.
        .with_policy(HumanDecisionRequirement::Human, DeploymentRule::Deny);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition"
        ],
        "the walk stops at the inspection; policy is never reached"
    );
}

/// The order is the contract, not an implementation detail: policy must be
/// consulted after the postcondition inspection (so an already-done effect is
/// never refused for a rule it no longer needs) and before the mutation (so a
/// refused effect never happens). A test that only checks the endpoints would
/// pass on an implementation that authorized first and asked afterwards.
#[tokio::test]
async fn the_nine_steps_happen_in_the_specified_order() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
            "authorize",
            "apply",
            "observe_postcondition",
        ]
    );
    // The steps are what the executor says it did; the calls are what the world
    // saw. Both are asserted, so a step emitted without the work behind it
    // would not pass.
    assert_eq!(harness.world.calls(), ["inspect", "apply", "inspect"]);
}

// ---------------------------------------------------------------------------
// What an unknown outcome resolves to
// ---------------------------------------------------------------------------

/// The rule the milestone turns on, in the executor rather than in an adapter.
#[tokio::test]
async fn an_unknown_outcome_is_resolved_by_reading_never_by_retrying() {
    let harness = Harness::new(Script::WriteLandsAnswerLost);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.mutation_requests(),
        1,
        "the mutation was dispatched exactly once"
    );
    assert_eq!(
        harness.world.mutations(),
        1,
        "and it landed exactly once, which is the property"
    );
    assert!(
        harness.world.read_after_unknown(),
        "the executor went and looked"
    );
    assert_eq!(
        harness.world.calls(),
        ["inspect", "apply", "inspect"],
        "a read settled it; no second dispatch appears anywhere in the walk"
    );
}

/// A read that itself fails leaves the effect unresolved and says so, rather
/// than degrading to one of the two confident answers.
#[tokio::test]
async fn an_unreadable_postcondition_leaves_the_effect_unresolved() {
    let harness = Harness::new(Script::WriteLostReadFails);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "expected Unresolved, got {error:?}"
    );
    assert_eq!(
        harness.world.mutation_requests(),
        1,
        "an unresolved outcome is still never retried"
    );
}

/// The mirror of the previous case: the adapter claimed success and the world
/// does not show it. Believing the response over the world is exactly what step
/// 8 exists to prevent, so this is unresolved too rather than committed.
#[tokio::test]
async fn a_dispatch_that_claimed_success_without_a_postcondition_is_unresolved() {
    let harness = Harness::new(Script::SuccessWithoutPostcondition);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "expected Unresolved, got {error:?}"
    );
}

/// A refusal that leaves no room for the write having happened, against a world
/// that agrees. Here the refusal stands as the answer — reporting this one
/// `Unresolved` would send a caller to investigate a settled failure.
#[tokio::test]
async fn a_confident_refusal_the_world_agrees_with_stays_a_failure() {
    let harness = Harness::new(Script::ConfidentRefusal);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            EffectError::Adapter {
                source: GhError::Http { status: 403, .. },
                ..
            }
        ),
        "expected the refusal to stand, got {error:?}"
    );
    assert_eq!(harness.world.mutations(), 0);
}

/// Two matching objects is a state to report, not a set to pick from.
#[tokio::test]
async fn more_than_one_matching_object_is_a_duplicate_state_error() {
    let harness = Harness::new(Script::TwoMatch);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::DuplicateState { count: 2, .. }),
        "expected DuplicateState with the count, got {error:?}"
    );
    assert_eq!(
        harness.world.mutation_requests(),
        0,
        "an unaccounted-for object is never written over"
    );
}

// ---------------------------------------------------------------------------
// Policy, consumed
// ---------------------------------------------------------------------------

/// M2 has no decision channel, so a capability minimum demanding one fails
/// closed and names what would satisfy it. This is what stops the variant
/// shipping inert, the way `agent.max_capability_attempts` did.
#[tokio::test]
async fn a_human_decision_requirement_fails_closed_naming_m3() {
    let harness = Harness::new(Script::AbsentThenWritten)
        .with_policy(HumanDecisionRequirement::Human, DeploymentRule::Allow);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    let rendered = format!("{error}");
    assert!(
        matches!(error, EffectError::HumanDecisionRequired { .. }),
        "expected HumanDecisionRequired, got {error:?}"
    );
    assert!(
        rendered.contains("M3"),
        "a refusal must name what would satisfy it: {rendered}"
    );
    assert_eq!(
        harness.world.mutation_requests(),
        0,
        "a refused effect never happens"
    );
    assert_eq!(
        harness.world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
        ],
        "the walk stops at the combination; nothing is authorized"
    );
}

/// The deployment's own refusal is the other half of the same consumption.
#[tokio::test]
async fn a_denied_deployment_rule_refuses_before_the_mutation() {
    let harness = Harness::new(Script::AbsentThenWritten)
        .with_policy(HumanDecisionRequirement::Automatic, DeploymentRule::Deny);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::PolicyDenied { .. }),
        "expected PolicyDenied, got {error:?}"
    );
    assert_eq!(harness.world.mutation_requests(), 0);
}

/// A capability cannot claim another capability's identity when proposing.
#[tokio::test]
async fn an_executor_is_bound_to_one_capability() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let error = harness
        .executor()
        .execute(proposed_by(STUB_MARK), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::PolicyDenied { .. }),
        "expected PolicyDenied, got {error:?}"
    );
    let rendered = format!("{error}");
    assert!(
        rendered.contains("fixture_repair") && rendered.contains("stub_mark"),
        "the refusal must name both capabilities: {rendered}"
    );
    assert_eq!(
        harness.world.calls(),
        Vec::<&str>::new(),
        "validation precedes every look at the world"
    );
    assert_eq!(
        harness.world.steps(),
        ["validate_capability"],
        "and precedes every other step"
    );
}

// ---------------------------------------------------------------------------
// The receipt
// ---------------------------------------------------------------------------

/// The receipt carries the identity a *fresh* process would recompute, so a
/// later run can recognise this effect with nothing but its canonical inputs.
#[tokio::test]
async fn the_receipt_carries_the_recomputable_identity_and_payload_hash() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(
        receipt.effect_id,
        effect_id(
            PROJECT,
            INVOCATION_REF,
            EffectKind::EnsureBranchPublished,
            TARGET
        )
    );
    assert_eq!(receipt.payload_hash, payload_hash(PAYLOAD));
    assert_eq!(receipt.target, TARGET);
    assert_eq!(receipt.value, "deadbeef");
}
