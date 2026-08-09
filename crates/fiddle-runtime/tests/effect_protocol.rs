//! The effect executor's protocol, and the first operation that composes it.
//!
//! Two halves, and the split between them is the point rather than an
//! arrangement of convenience.
//!
//! The **protocol half** is every *ambiguity* case: what the executor does when
//! it does not know whether a write landed. None of it reaches GitHub and none
//! of it spawns a process — the world is a scripted [`IntegrationOperation`], so
//! the properties the milestone turns on are decided by the executor rather than
//! by whatever a network happened to do that afternoon. The one rule underneath
//! all of it: **`Unknown` is resolved by reading the world, never by retrying
//! the mutation.** A retry there is how a duplicate external effect is born, so
//! the mutation dispatch count is asserted directly rather than inferred from an
//! outcome.
//!
//! The **branch half** is `ensure_branch_published` end to end, and it must be
//! asked of something real: a **bare repository on disk** pushed to by the
//! product's own `git`, and the **scripted `gh`** answering the ref read out of
//! that same repository. A fixture answering the read from its own idea of what
//! a push does would be asserting this file's assumptions about git rather than
//! git's behaviour — and "a divergent ref is refused as a non-fast-forward" is
//! precisely a claim about git's behaviour, since it is the claim that stands in
//! for the ownership trailer the design dropped. Still offline, still
//! credential-free: a path remote authenticates nobody.

use async_trait::async_trait;
use fiddle_core::{
    effect_id, payload_hash, CapabilityId, DeploymentRule, EffectKind, HumanDecisionRequirement,
    ProposedEffect, FIXTURE_REPAIR, STUB_MARK,
};
use fiddle_runtime::effect::{
    AuthorizedEffect, DeploymentPolicy, EffectContext, EffectError, EffectOutcome, EffectReceipt,
    EffectTrace, ExecutionStep, Executor, IntegrationOperation, ObservedState,
};
use fiddle_runtime::git::{GitCli, GitError};
use fiddle_runtime::github::{branch_name, BranchRef, EnsureBranchPublished};
use fiddle_runtime::{GhCli, GhError};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;
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

/// A context nothing in the protocol half reaches.
///
/// The scripted operation ignores it, so neither the `gh` nor the `git` inside
/// it is ever spawned and both program paths are deliberately ones that do not
/// exist: if a future change made the executor talk to GitHub — or push —
/// behind the operation's back, these tests would fail loudly rather than
/// quietly acquire a dependency on a network.
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
        GitCli::new(
            PathBuf::from("/nonexistent/git"),
            String::new(),
            "FIDDLE_GITHUB_TOKEN",
            Duration::from_secs(1),
        ),
        PathBuf::from("/nonexistent"),
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

// ---------------------------------------------------------------------------
// One real branch
// ---------------------------------------------------------------------------

/// The repository the scripted `gh` answers for, and the one the API paths name.
const REPO: &str = "o/r";

/// A generous bound for children that answer immediately. Nothing in this half
/// is about the deadline; `github_cli` and `git_publish` own the process bounds
/// and this file inherits them rather than restating them.
const PATIENT: Duration = Duration::from_secs(60);

/// Run a setup `git` in `dir` and insist it succeeded.
///
/// Setup runs under the ambient environment on purpose — it is the test
/// arranging a world, not the code under test — but identity and the initial
/// branch are pinned with `-c` so that an operator's global configuration
/// cannot change what the fixture is.
fn git_setup(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.email=fiddle@example.invalid",
            "-c",
            "user.name=fiddle",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git is on PATH for the test process");
    assert!(
        output.status.success(),
        "setup `git {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// One `git` question, answered as a trimmed string.
fn git_says(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The world one branch effect runs against: a bare repository standing in for
/// GitHub, a worktree holding the work, and the scripted `gh` that answers reads
/// out of the first of those.
///
/// The two adapters see the *same* remote through different doors — `git` writes
/// to it over a path, the scripted `gh` reads its ref files — which is what makes
/// "the postcondition was read back rather than assumed" a real claim here. A
/// stub answering from its own memory of what it had been asked would agree with
/// a push that never happened.
struct Remote {
    dir: TempDir,
    remote: PathBuf,
    work: PathBuf,
    steps: Mutex<Vec<&'static str>>,
}

impl EffectTrace for Remote {
    fn step(&self, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl Remote {
    /// An empty remote and a worktree with one commit pointing at it.
    fn empty() -> Self {
        let dir = TempDir::new().unwrap();
        // `remote.git` is the name the scripted `gh` looks for beside its own
        // scratch directory; see `tests/gh_stub/gh_stub.rs`.
        let remote = dir.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git_setup(&remote, &["init", "-q", "--bare", "."]);
        // Empty, and stays empty: it is what a real `gh` would be pinned to.
        std::fs::create_dir_all(dir.path().join("config")).unwrap();

        let this = Self {
            work: dir.path().join("work"),
            remote,
            dir,
            steps: Mutex::new(Vec::new()),
        };
        this.worktree("work", "one");
        this
    }

    /// A working repository with one commit whose content is `content`, and an
    /// `origin` pointing at the bare repository.
    fn worktree(&self, name: &str, content: &str) -> PathBuf {
        let work = self.dir.path().join(name);
        std::fs::create_dir_all(&work).unwrap();
        git_setup(&work, &["init", "-q", "."]);
        std::fs::write(work.join("file"), content).unwrap();
        git_setup(&work, &["add", "file"]);
        git_setup(&work, &["commit", "-q", "-m", name]);
        git_setup(
            &work,
            &[
                "remote",
                "add",
                "origin",
                &self.remote.display().to_string(),
            ],
        );
        work
    }

    /// Put `worktree`'s commit on `branch` before the effect runs.
    ///
    /// Arranged with the test's own `git` rather than with the adapter under
    /// test, so a world this file claims to have built is not built by the code
    /// the assertions are about.
    fn seed(&self, worktree: &Path, branch: &str) {
        git_setup(
            worktree,
            &["push", "-q", "origin", &format!("HEAD:refs/heads/{branch}")],
        );
    }

    /// The commit the work is sitting on: what a publish intends.
    fn head(&self) -> String {
        git_says(&self.work, &["rev-parse", "HEAD"])
    }

    /// Every branch the remote holds, in ref order.
    fn branches(&self) -> Vec<String> {
        git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    /// What one branch of the remote points at.
    fn branch_sha(&self, branch: &str) -> String {
        git_says(
            &self.remote,
            &["rev-parse", &format!("refs/heads/{branch}")],
        )
    }

    /// A context whose `gh` is the scripted one and whose `git` is the real one.
    fn context(&self) -> EffectContext {
        self.context_reading_with(PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")))
    }

    /// The same, with the `gh` program named — so a `gh` that cannot answer at
    /// all can be handed to the same operation.
    fn context_reading_with(&self, gh: PathBuf) -> EffectContext {
        EffectContext::new(
            GhCli::new(
                gh,
                // The scratch directory arrives in `argv` because the adapter's
                // environment has room for exactly five names; see
                // `tests/gh_stub/gh_stub.rs`.
                vec![
                    "--stub-dir".to_string(),
                    self.dir.path().display().to_string(),
                ],
                "ghp_never_reaches_a_network".to_string(),
                "FIDDLE_GITHUB_TOKEN",
                self.dir.path().join("config"),
                PATIENT,
            ),
            GitCli::new(
                PathBuf::from("git"),
                // Never used: a path remote authenticates nobody, which is what
                // keeps this lane credential-free while still running the exact
                // environment the product builds.
                "ghp_never_used_by_a_path_remote".to_string(),
                "FIDDLE_GITHUB_TOKEN",
                PATIENT,
            ),
            self.work.clone(),
            CancellationToken::new(),
        )
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }
}

/// The branch this run publishes, recomputed the way a fresh process would.
fn published_branch() -> String {
    branch_name(PROJECT, INVOCATION_REF)
}

/// The operation under test, aimed at `intended`.
fn branch_operation(intended: &str) -> EnsureBranchPublished {
    EnsureBranchPublished::new(REPO.to_string(), published_branch(), intended.to_string())
}

/// Walk the authorization order for one branch effect.
///
/// Generic over the operation so that the 422 cases below can substitute a
/// mutation while keeping the *real* read — which is the half the criterion is
/// actually about.
async fn publish_the_branch<O>(
    remote: &Remote,
    ctx: &EffectContext,
    intended: &str,
    operation: O,
) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, EffectError>
where
    O: IntegrationOperation,
{
    let deployment = Deployment(DeploymentRule::Allow);
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectKind::EnsureBranchPublished,
        target: fiddle_runtime::github::branch_target(&published_branch()),
        payload: serde_json::json!({ "repo": REPO, "sha": intended }).to_string(),
    };
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        ctx,
    )
    .observed_by(remote)
    .execute(proposed, operation)
    .await
}

/// The name is the durable remote locator — the thing a fresh process has to
/// find a branch by after its own answer was lost — so it must fall out of
/// canonical inputs and nothing else.
///
/// The syntax assertions are not decoration. A name is rejected by git if it
/// contains `..`, ends a component in `.lock`, or begins with `-`, and a name
/// that reached the far end and failed there would be a failure this adapter
/// could have prevented. They hold *structurally* here — the digest is hex — but
/// they are asserted because the construction is what guarantees it, and a
/// construction can be changed. `an_absent_ref_is_published_and_then_read_back`
/// is the other half of the same claim: it pushes this exact name through
/// `GitCli::publish`, whose own boundary check is the one a real `git` would
/// have applied.
#[test]
fn the_branch_name_is_derived_and_stable() {
    let first = branch_name("acme/widget", "beans:w-1");
    assert_eq!(first, branch_name("acme/widget", "beans:w-1"));
    assert!(
        first.starts_with("fiddle/"),
        "namespaced, so a human can see whose it is: {first}"
    );
    // Both canonical inputs move the name, or two runs would publish over each
    // other's work under one ref.
    assert_ne!(first, branch_name("acme/widget", "beans:w-2"));
    assert_ne!(first, branch_name("acme/other", "beans:w-1"));

    // The identity's own derivation, reused rather than a second hash invented.
    assert_eq!(
        first,
        format!(
            "fiddle/{}",
            effect_id(
                "acme/widget",
                "beans:w-1",
                EffectKind::EnsureBranchPublished,
                "acme/widget"
            )
            .0
        )
    );

    // git's own ref rules.
    assert!(!first.contains(".."));
    assert!(!first.ends_with(".lock"));
    assert!(!first.split('/').any(|part| part.ends_with(".lock")));
    assert!(!first.starts_with(['-', '.', '/']));
    assert!(!first.ends_with(['.', '/']));
    assert!(first
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/-_".contains(c)));

    // Every input, not only the well-behaved ones: the encoding underneath is
    // length-prefixed, so a project carrying a separator, a NUL or a refspec
    // metacharacter still produces a name git will take.
    for project in [
        "",
        "a\0b",
        "+force:me",
        "../../etc",
        "x".repeat(500).as_str(),
    ] {
        let name = branch_name(project, "beans:w-1");
        assert!(
            name.strip_prefix("fiddle/")
                .is_some_and(|id| id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit())),
            "{project:?} produced {name}"
        );
    }
}

/// An absent ref is the only state that licenses a push — and a 404 is how the
/// remote says so.
///
/// This is `m2-branch-404-is-knowledge` in its ordinary form: the first
/// inspection of an empty remote is a 404, and it has to come back as "not
/// there" rather than as a failure to look, or the very first publish of every
/// run would fail closed.
#[tokio::test]
async fn an_absent_ref_is_published_and_then_read_back() {
    let remote = Remote::empty();
    let ctx = remote.context();
    let head = remote.head();

    let receipt = publish_the_branch(&remote, &ctx, &head, branch_operation(&head))
        .await
        .expect("an absent ref is published");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        remote.branches(),
        [published_branch()],
        "exactly one branch, at the deterministic name"
    );
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some(head.as_str()),
        "the observed sha, read back out of the remote rather than assumed"
    );
    assert_eq!(receipt.value.branch, published_branch());
    assert_eq!(receipt.value.sha, head);
    assert_eq!(
        remote.steps().last(),
        Some(&"observe_postcondition"),
        "the receipt is built from the read that follows the push"
    );
}

/// The same rule stated on its own, and its fail-closed edge beside it.
///
/// A 404 is a read that *succeeded* and returned an absence; a `gh` that could
/// not answer at all is a source that could not be read. M0's rule is that the
/// two are never equivalent, and this is that rule at the GitHub boundary: the
/// first is `Ok(None)` and licenses a push, the second is an error and stops
/// one. Collapsing them in either direction is a defect — one way the first
/// publish never happens, the other way an outage looks like an empty remote and
/// gets pushed over.
#[tokio::test]
async fn a_404_is_knowledge_and_an_unreadable_source_is_not() {
    let remote = Remote::empty();
    let operation = branch_operation(&remote.head());

    assert_eq!(
        operation.inspect(&remote.context()).await.unwrap(),
        None,
        "the remote answered 404: the ref is absent, and that is knowledge"
    );

    let unreadable = remote.context_reading_with(PathBuf::from("/nonexistent/gh"));
    let error = operation
        .inspect(&unreadable)
        .await
        .expect_err("a source that could not be read is never an absent ref");
    assert!(
        matches!(error, GhError::Malformed(_)),
        "expected the read to fail, got {error:?}"
    );
}

/// A ref already at the intended sha is the postcondition, not a conflict.
///
/// The steps are asserted rather than a push count, which is the stronger
/// claim: not merely that nothing landed, but that the executor never dispatched
/// a mutation at all. A fresh process meeting the world a previous one built is
/// exactly this case, and it is the whole of the recovery.
#[tokio::test]
async fn a_ref_already_at_the_intended_sha_is_already_satisfied() {
    let remote = Remote::empty();
    let head = remote.head();
    remote.seed(&remote.work, &published_branch());

    let ctx = remote.context();
    let receipt = publish_the_branch(&remote, &ctx, &head, branch_operation(&head))
        .await
        .expect("the postcondition already holds");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.external_ref.as_deref(), Some(head.as_str()));
    assert_eq!(
        remote.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition"
        ],
        "the walk stops at the inspection; nothing is pushed and nothing is even \
         put to policy"
    );
    assert_eq!(remote.branches(), [published_branch()]);
    assert_eq!(remote.branch_sha(&published_branch()), head);
}

/// A ref at the deterministic name pointing somewhere else is the case §5.5
/// deliberately left to git rather than to a commit trailer: the push is
/// attempted and refused as a non-fast-forward, so a divergent branch is
/// reported and never overwritten.
///
/// This is the assertion that the dropped ownership check left no hole, and it
/// has to be asked of a real `git` against a real remote — a fixture answering
/// "rejected" would be this file agreeing with its own belief about git, when
/// git's behaviour is the entire load-bearing claim.
///
/// Three things are asserted, and they are three different claims. The refusal
/// carries git's own verdict rather than a generic failure, so a caller can tell
/// a divergence from an outage. The ref still points where it did, so nothing
/// was forced. And no second branch appeared, so nothing routed around the
/// refusal by publishing elsewhere.
#[tokio::test]
async fn a_ref_at_our_name_pointing_elsewhere_is_refused_not_overwritten() {
    let remote = Remote::empty();
    let other = remote.worktree("other", "another");
    let theirs = git_says(&other, &["rev-parse", "HEAD"]);
    remote.seed(&other, &published_branch());
    let head = remote.head();
    assert_ne!(head, theirs, "the two worktrees must really diverge");

    let ctx = remote.context();
    let error = publish_the_branch(&remote, &ctx, &head, branch_operation(&head))
        .await
        .expect_err("a ref that is not an ancestor cannot fast-forward");

    assert!(
        matches!(
            error,
            EffectError::Adapter {
                source: GhError::Push(GitError::NonFastForward { .. }),
                ..
            }
        ),
        "expected git's own non-fast-forward verdict, got {error:?}"
    );
    assert_eq!(
        remote.branch_sha(&published_branch()),
        theirs,
        "the refused push must not have moved the ref"
    );
    assert_eq!(
        remote.branches(),
        [published_branch()],
        "and must not have added one beside it"
    );
    assert!(
        remote.steps().contains(&"apply"),
        "the judgment belongs to git, so the push has to actually be attempted"
    );
}

// ---------------------------------------------------------------------------
// A 422, resolved by looking
// ---------------------------------------------------------------------------

/// A ref creation that answers **422** while the real read decides what it meant.
///
/// The 422 is injected rather than produced, and the reason is worth stating
/// plainly. Creating a ref through the API is not how this operation publishes —
/// a ref can only point at an object the remote already holds, so the objects
/// and the ref go up together in one `git push`, and `git` has no 422 to give.
/// What GitHub answers when a ref creation collides is nonetheless the thing
/// this rule exists for, verified against the real service during planning:
/// exactly `422 {"message":"Reference already exists"}`, a status that covers
/// malformed input, invalid ref syntax, spam protection and "it is already
/// there" alike, and therefore means nothing on its face.
///
/// So only the *answer* is scripted. [`inspect`](IntegrationOperation::inspect)
/// delegates to the production operation, and `lands` decides whether the world
/// really changed first — which is the only difference between the two cases,
/// and the only thing that can tell them apart.
struct RefCreationAnswering422 {
    inner: EnsureBranchPublished,
    /// Whether the ref creation really landed before its answer came back.
    lands: bool,
}

#[async_trait]
impl IntegrationOperation for RefCreationAnswering422 {
    type State = BranchRef;

    fn minimum(&self) -> HumanDecisionRequirement {
        self.inner.minimum()
    }

    /// The production read, unmodified. This is the half the criterion is about.
    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<BranchRef>, GhError> {
        self.inner.inspect(ctx).await
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        // The effect lands and *then* the answer is lost — that order, and not
        // the other one, is what makes this an ambiguous write rather than a
        // failed one.
        if self.lands {
            ctx.git
                .publish(&ctx.work, self.inner.branch(), &ctx.cancel)
                .await
                .map_err(GhError::Push)?;
        }
        Err(GhError::Http {
            status: 422,
            message: "Reference already exists".to_string(),
        })
    }
}

fn ref_creation_answering_422(intended: &str, lands: bool) -> RefCreationAnswering422 {
    RefCreationAnswering422 {
        inner: branch_operation(intended),
        lands,
    }
}

/// A 422 whose ref then matches is a success the naive reading would have called
/// a failure.
///
/// `GhError::outcome` maps 422 to `Unknown` precisely so the executor cannot act
/// on it, and what settles it is the ref read — matching, so the postcondition
/// holds. The mutation is dispatched once and never again: the world is
/// consulted, not the request repeated.
#[tokio::test]
async fn a_422_is_resolved_by_reading_the_ref_not_by_believing_it() {
    let remote = Remote::empty();
    let ctx = remote.context();
    let head = remote.head();

    let receipt = publish_the_branch(
        &remote,
        &ctx,
        &head,
        ref_creation_answering_422(&head, true),
    )
    .await
    .expect("a 422 whose ref is there is the postcondition, not a failure");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.external_ref.as_deref(), Some(head.as_str()));
    assert_eq!(
        remote.branches(),
        [published_branch()],
        "exactly one branch: the answer was lost, not the write"
    );
    assert_eq!(
        remote
            .steps()
            .iter()
            .filter(|step| **step == "apply")
            .count(),
        1,
        "an unknown outcome is resolved by reading, never by dispatching again"
    );
}

/// And a 422 that was genuinely a bad request stays a failure.
///
/// The other direction, and the one that stops "422 means it already exists"
/// from being written down as a rule. Nothing is in the remote, so nothing
/// resolves the ambiguity, and the effect stays unresolved rather than being
/// reported as either confident answer — a caller told "failed" here would retry
/// a write that might have landed.
#[tokio::test]
async fn a_422_with_no_matching_ref_is_still_a_failure() {
    let remote = Remote::empty();
    let ctx = remote.context();
    let head = remote.head();

    let error = publish_the_branch(
        &remote,
        &ctx,
        &head,
        ref_creation_answering_422(&head, false),
    )
    .await
    .expect_err("a 422 with no ref behind it is not a success");

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "expected Unresolved, got {error:?}"
    );
    assert!(
        format!("{error}").contains("422"),
        "the refusal must carry what GitHub actually said: {error}"
    );
    assert!(
        remote.branches().is_empty(),
        "no branch was created, and none was invented by reading"
    );
}
