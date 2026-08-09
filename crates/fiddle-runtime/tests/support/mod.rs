//! The scripted world every effect suite is written against.
//!
//! Extracted from `effect_protocol.rs` when a second effect suite arrived, and
//! extracted rather than copied for one reason: a fixture that exists twice
//! drifts, and two suites asserting the *same* protocol against two subtly
//! different worlds prove less than one suite does. The executor's contract is a
//! single contract, so the world it is asked about is a single world.
//!
//! Nothing here reaches a process, a credential or a network. The scripted
//! [`IntegrationOperation`] never touches its [`EffectContext`], which is what
//! lets the protocol be decided by the executor rather than by whatever a
//! network happened to do that afternoon.

// This module is compiled once per test binary and no single suite needs every
// helper — the pull-request suite drives real operations and reaches only
// `Deployment` and the constants, which does not make the scripted operation
// beside them dead code.
#![allow(dead_code)]

use async_trait::async_trait;
use fiddle_core::{
    effect_id, CapabilityId, DeploymentRule, EffectKind, HumanDecisionRequirement, ProposedEffect,
    FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    AuthorizedEffect, DeploymentPolicy, EffectContext, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ObservedState,
};
use fiddle_runtime::git::GitCli;
use fiddle_runtime::{GhCli, GhError};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const PROJECT: &str = "acme/widget";
pub const INVOCATION_REF: &str = "beans:w-1";
pub const TARGET: &str = "refs/heads/fiddle/abc";
pub const PAYLOAD: &str = r#"{"sha":"deadbeef"}"#;

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
pub enum Script {
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
pub struct World {
    script: Script,
    landed: AtomicBool,
    dispatches: AtomicUsize,
    writes: AtomicUsize,
    calls: Mutex<Vec<&'static str>>,
    steps: Mutex<Vec<&'static str>>,
}

impl World {
    pub fn new(script: Script) -> Self {
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
    pub fn mutations(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }

    /// How many times a mutation was dispatched, landed or not. The number a
    /// retry would move and a postcondition read would not.
    pub fn mutation_requests(&self) -> usize {
        self.dispatches.load(Ordering::SeqCst)
    }

    /// Did the executor go and look after the answer was lost?
    pub fn read_after_unknown(&self) -> bool {
        let calls = self.calls.lock().unwrap();
        match calls.iter().position(|call| *call == "apply") {
            Some(at) => calls[at + 1..].contains(&"inspect"),
            None => false,
        }
    }

    pub fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    pub fn calls(&self) -> Vec<&'static str> {
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
pub struct Deployment(pub DeploymentRule);

impl DeploymentPolicy for Deployment {
    fn rule_for(&self, _kind: EffectKind) -> DeploymentRule {
        self.0
    }
}

/// The observed postcondition of the scripted operation.
#[derive(Debug)]
pub struct BranchState {
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
pub struct ScriptedOperation<'w> {
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
pub struct Harness {
    pub world: World,
    ctx: EffectContext,
    deployment: Deployment,
    capability: CapabilityId,
    minimum: HumanDecisionRequirement,
}

impl Harness {
    pub fn new(script: Script) -> Self {
        Self {
            world: World::new(script),
            ctx: unreachable_context(),
            deployment: Deployment(DeploymentRule::Allow),
            capability: FIXTURE_REPAIR,
            minimum: HumanDecisionRequirement::Automatic,
        }
    }

    pub fn with_policy(mut self, minimum: HumanDecisionRequirement, rule: DeploymentRule) -> Self {
        self.minimum = minimum;
        self.deployment = Deployment(rule);
        self
    }

    pub fn executor(&self) -> Executor<'_> {
        Executor::new(
            self.capability,
            PROJECT.to_string(),
            INVOCATION_REF.to_string(),
            &self.deployment,
            &self.ctx,
        )
        .observed_by(&self.world)
    }

    pub fn operation(&self) -> ScriptedOperation<'_> {
        ScriptedOperation {
            world: &self.world,
            minimum: self.minimum,
        }
    }
}

/// A context nothing in the protocol half reaches.
///
/// The scripted operation ignores it, so neither the `gh` nor the `git` inside
/// it is ever spawned and both program paths are deliberately ones that do not
/// exist: if a future change made the executor talk to GitHub — or push —
/// behind the operation's back, these tests would fail loudly rather than
/// quietly acquire a dependency on a network.
pub fn unreachable_context() -> EffectContext {
    EffectContext::new(
        GhCli::new(
            PathBuf::from("/nonexistent/gh"),
            Vec::new(),
            String::new(),
            "GH_TOKEN",
            PathBuf::from("/nonexistent"),
            Duration::from_secs(1),
        ),
        unreachable_git(),
        PathBuf::from("/nonexistent"),
        CancellationToken::new(),
    )
}

/// A `git` that cannot be run, for the suites whose subject is not a push.
///
/// Named rather than inlined because it is an assertion: an operation that grew
/// a push behind the executor's back would fail loudly here instead of quietly
/// acquiring a second mutation channel.
pub fn unreachable_git() -> GitCli {
    GitCli::new(
        PathBuf::from("/nonexistent/git"),
        String::new(),
        "FIDDLE_GITHUB_TOKEN",
        Duration::from_secs(1),
    )
}

pub fn branch_effect() -> ProposedEffect {
    proposed_by(FIXTURE_REPAIR)
}

pub fn proposed_by(capability: CapabilityId) -> ProposedEffect {
    ProposedEffect {
        capability,
        kind: EffectKind::EnsureBranchPublished,
        target: TARGET.to_string(),
        payload: PAYLOAD.to_string(),
    }
}
