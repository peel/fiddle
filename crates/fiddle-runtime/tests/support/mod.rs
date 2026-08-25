#![allow(dead_code)]

pub mod cve;

#[allow(unused_imports)]
pub use cve::wiz_stub;

use async_trait::async_trait;
use fiddle_core::{
    effect_id, CapabilityId, DeploymentRule, EffectName, HumanDecisionRequirement, ProposedEffect,
    ENSURE_BRANCH_PUBLISHED, FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    AuthorizedEffect, DeploymentPolicy, EffectContext, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ObservedState, ReadClock, ReadRetry,
};
use fiddle_runtime::git::GitCli;
use fiddle_runtime::{GhCli, GhError, RetryAdvice};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const PROJECT: &str = "acme/widget";
pub const INVOCATION_REF: &str = "beans:w-1";
pub const TARGET: &str = "refs/heads/fiddle/abc";
pub const PAYLOAD: &str = r#"{"sha":"deadbeef"}"#;

#[derive(Clone, Copy, Debug)]
pub enum Script {
    AlreadySatisfied,
    AbsentThenWritten,
    WriteLandsAnswerLost,
    WriteLostReadFails,
    TwoMatch,
    ConfidentRefusal,
    SuccessWithoutPostcondition,
    PostconditionSurfacesLate,
    WriteLandsAnswerLostAndSurfacesLate,
    RateLimitedThenSettles,
}

impl Script {
    pub const ALL: [Script; 10] = [
        Script::AlreadySatisfied,
        Script::AbsentThenWritten,
        Script::WriteLandsAnswerLost,
        Script::WriteLostReadFails,
        Script::TwoMatch,
        Script::ConfidentRefusal,
        Script::SuccessWithoutPostcondition,
        Script::PostconditionSurfacesLate,
        Script::WriteLandsAnswerLostAndSurfacesLate,
        Script::RateLimitedThenSettles,
    ];

    pub fn index(self) -> usize {
        match self {
            Script::AlreadySatisfied => 0,
            Script::AbsentThenWritten => 1,
            Script::WriteLandsAnswerLost => 2,
            Script::WriteLostReadFails => 3,
            Script::TwoMatch => 4,
            Script::ConfidentRefusal => 5,
            Script::SuccessWithoutPostcondition => 6,
            Script::PostconditionSurfacesLate => 7,
            Script::WriteLandsAnswerLostAndSurfacesLate => 8,
            Script::RateLimitedThenSettles => 9,
        }
    }
}

pub const SCRIPTED_RETRY_AFTER: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub struct RecordingClock(Mutex<Vec<Duration>>);

#[async_trait]
impl ReadClock for RecordingClock {
    async fn wait(&self, delay: Duration) {
        self.0.lock().unwrap().push(delay);
    }
}

impl RecordingClock {
    pub fn waits(&self) -> Vec<Duration> {
        self.0.lock().unwrap().clone()
    }
}

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

    pub fn mutations(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }

    pub fn mutation_requests(&self) -> usize {
        self.dispatches.load(Ordering::SeqCst)
    }

    pub fn read_after_unknown(&self) -> bool {
        let calls = self.calls.lock().unwrap();
        match calls.iter().position(|call| *call == "apply") {
            Some(at) => calls[at + 1..].contains(&"inspect"),
            None => false,
        }
    }

    pub fn reads(&self) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| **call == "inspect")
            .count()
    }

    fn looks_since_the_write(&self) -> usize {
        let calls = self.calls.lock().unwrap();
        match calls.iter().rposition(|call| *call == "apply") {
            Some(at) => calls[at + 1..].iter().filter(|c| **c == "inspect").count(),
            None => 0,
        }
    }

    pub fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    pub fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().clone()
    }
}

impl EffectTrace for World {
    fn step(&self, _kind: &EffectName, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

pub struct Deployment(pub DeploymentRule);

impl DeploymentPolicy for Deployment {
    fn rule_for(&self, _kind: &EffectName) -> DeploymentRule {
        self.0
    }
}

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

pub struct ScriptedOperation<'w> {
    world: &'w World,
    minimum: HumanDecisionRequirement,
}

#[async_trait]
impl IntegrationOperation for ScriptedOperation<'_> {
    type State = BranchState;

    type Error = GhError;

    fn minimum(&self) -> HumanDecisionRequirement {
        self.minimum
    }

    fn payload(&self) -> String {
        PAYLOAD.to_string()
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
            Script::WriteLostReadFails => match self.world.landed.load(Ordering::SeqCst) {
                false => Ok(None),
                true => Err(GhError::Http {
                    status: 500,
                    message: "the postcondition could not be read".to_string(),
                    advice: RetryAdvice::default(),
                }),
            },
            Script::ConfidentRefusal | Script::SuccessWithoutPostcondition => Ok(None),
            Script::AbsentThenWritten | Script::WriteLandsAnswerLost => {
                match self.world.landed.load(Ordering::SeqCst) {
                    false => Ok(None),
                    true => present(),
                }
            }
            Script::PostconditionSurfacesLate | Script::WriteLandsAnswerLostAndSurfacesLate => {
                match self.world.landed.load(Ordering::SeqCst) {
                    false => Ok(None),
                    true => match self.world.looks_since_the_write() {
                        1 => Ok(None),
                        _ => present(),
                    },
                }
            }
            Script::RateLimitedThenSettles => match self.world.landed.load(Ordering::SeqCst) {
                false => Ok(None),
                true => match self.world.looks_since_the_write() {
                    1 => Err(GhError::Http {
                        status: 429,
                        message: "API rate limit exceeded".to_string(),
                        advice: RetryAdvice {
                            retry_after: Some(SCRIPTED_RETRY_AFTER),
                            rate_limit_remaining: Some(0),
                        },
                    }),
                    _ => present(),
                },
            },
        }
    }

    async fn apply(
        &self,
        _ctx: &EffectContext,
        authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        self.world.record("apply");
        self.world.dispatches.fetch_add(1, Ordering::SeqCst);
        assert_eq!(
            authorized.effect_id(),
            &effect_id(PROJECT, INVOCATION_REF, ENSURE_BRANCH_PUBLISHED, TARGET),
            "the envelope must carry the identity derived for this request"
        );

        let land = |world: &World| {
            world.landed.store(true, Ordering::SeqCst);
            world.writes.fetch_add(1, Ordering::SeqCst);
        };
        match self.world.script {
            Script::AbsentThenWritten | Script::PostconditionSurfacesLate => {
                land(self.world);
                Ok(())
            }
            Script::WriteLandsAnswerLost | Script::WriteLandsAnswerLostAndSurfacesLate => {
                land(self.world);
                Err(GhError::Killed("signal".to_string()))
            }
            Script::WriteLostReadFails => {
                self.world.landed.store(true, Ordering::SeqCst);
                Err(GhError::Killed("signal".to_string()))
            }
            Script::RateLimitedThenSettles => {
                land(self.world);
                Ok(())
            }
            Script::ConfidentRefusal => Err(GhError::Http {
                status: 403,
                message: "resource not accessible".to_string(),
                advice: RetryAdvice::default(),
            }),
            Script::SuccessWithoutPostcondition => Ok(()),
            Script::AlreadySatisfied | Script::TwoMatch => {
                panic!("this world must never be written to")
            }
        }
    }
}

pub struct Harness {
    pub world: World,
    ctx: EffectContext,
    deployment: Deployment,
    capability: CapabilityId,
    minimum: HumanDecisionRequirement,
    read_retry: ReadRetry,
    clock: Arc<RecordingClock>,
}

impl Harness {
    pub fn new(script: Script) -> Self {
        Self {
            world: World::new(script),
            ctx: unreachable_context(),
            deployment: Deployment(DeploymentRule::Allow),
            capability: FIXTURE_REPAIR,
            minimum: HumanDecisionRequirement::Automatic,
            read_retry: ReadRetry::none(),
            clock: Arc::new(RecordingClock::default()),
        }
    }

    pub fn with_policy(mut self, minimum: HumanDecisionRequirement, rule: DeploymentRule) -> Self {
        self.minimum = minimum;
        self.deployment = Deployment(rule);
        self
    }

    pub fn with_read_retry(mut self, attempts: u32, initial: Duration, max: Duration) -> Self {
        self.read_retry = ReadRetry::served_by(attempts, initial, max, self.clock.clone());
        self
    }

    pub fn waits(&self) -> Vec<Duration> {
        self.clock.waits()
    }

    pub fn read_retry(&self) -> &ReadRetry {
        &self.read_retry
    }

    pub fn executor(&self) -> Executor<'_> {
        Executor::new(
            self.capability,
            PROJECT.to_string(),
            INVOCATION_REF.to_string(),
            &self.deployment,
            &self.ctx,
            &self.world,
            self.read_retry.clone(),
        )
    }

    pub fn executor_observed_by<'a>(&'a self, trace: &'a dyn EffectTrace) -> Executor<'a> {
        Executor::new(
            self.capability,
            PROJECT.to_string(),
            INVOCATION_REF.to_string(),
            &self.deployment,
            &self.ctx,
            trace,
            self.read_retry.clone(),
        )
    }

    pub fn operation(&self) -> ScriptedOperation<'_> {
        ScriptedOperation {
            world: &self.world,
            minimum: self.minimum,
        }
    }
}

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
        kind: EffectName::shipped(ENSURE_BRANCH_PUBLISHED),
        target: TARGET.to_string(),
        payload: PAYLOAD.to_string(),
    }
}
