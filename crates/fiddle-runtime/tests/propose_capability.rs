//! The first run: produce a change, publish a draft, ask, and stop.
//!
//! Everything here is offline, credential-free and always runs. The forge is the
//! scripted `gh` in `tests/gh_stub/`, reached through the product's own
//! `cli.program` seam; the remote is a bare repository on a path, pushed to by the
//! real `git` the product spawns; the model is `MockCompletionModel`, reached
//! through the generic parameter the capability already carries. Nothing fake
//! enters the product to make any of that possible.
//!
//! # Why the assertions are about the world and not about the return value
//!
//! The claim this capability makes is *what happened out there and in what
//! order*: a branch, then a draft, then a question, and then nothing. So the
//! branch is read out of the bare repository, the pull request create and its
//! `draft` flag are read out of the request the scripted `gh` recorded, the
//! comment is read out of the collection it was posted to, and the published tree
//! is read out of the remote with `git show`. The step order is read off the
//! [`EffectTrace`] the executor announces to, which is what makes the *order*
//! assertable rather than only the endpoints — a suite that inferred it from the
//! objects would pass on an implementation that asked first and published
//! afterwards.
//!
//! # The two halves this file has to keep apart
//!
//! [`a_first_run_publishes_a_branch_a_draft_and_a_question_then_waits`] and
//! [`an_attempt_whose_check_failed_publishes_nothing_and_asks_nothing`] are the
//! same fixture, the same check and the same tool: only the *content* the script
//! writes differs. So a `write_file` that silently did nothing could not make the
//! first pass, and a shell that believed the model's `claimed_complete` could not
//! make the second fail. Both scripts claim completion, which is the point — the
//! claim is evidence beside the exit code, never instead of it.

mod fixture;
mod support;

use fiddle_core::{
    effect_id, parse_marker, payload_hash, DeploymentRule, EffectKind, EvidenceRef, NextAction,
    Observation, PROPOSE_CHANGE, PUBLISH_CHANGE,
};
use fiddle_runtime::agent::AgentBudget;
use fiddle_runtime::capability::{
    attempt_worktree, Capability, CapabilityError, ExecutionGrant, ProposeChange, ProposeConfig,
};
use fiddle_runtime::effect::{
    EffectContext, EffectTrace, ExecutionStep, Executor, IntegrationOperation, ReadRetry,
    Recurrence,
};
use fiddle_runtime::git::GitCli;
use fiddle_runtime::github::{branch_name, pull_request_ready_target, EnsurePullRequestReady};
use fiddle_runtime::human::InteractionRef;
use fiddle_runtime::workspace::WorkspaceCommand;
use fiddle_runtime::GhCli;
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use support::{Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// The repository the question is asked in.
const REPO: &str = "acme/r";

/// The owner the head branch lives under, which is [`REPO`]'s own owner here.
const HEAD_OWNER: &str = "acme";

/// The branch a publication is proposed into.
const BASE: &str = "main";

/// The work item every run here is about.
const WORK_ID: &str = "w-1";

/// The attempt every scenario executes under. Fixed rather than minted, so the
/// evidence reference these tests assert on is a function of the run rather than
/// of the clock.
const ATTEMPT: &str = "01JQZX0000000000000000000";

/// The number the scripted forge assigns the first pull request in a world.
/// Seven rather than one, so an assertion cannot pass by accident against an
/// index or a count.
const PR: u64 = 7;

/// A generous process bound for a stub that answers immediately, and for a check
/// that compiles a crate with no dependencies.
const PATIENT: Duration = Duration::from_secs(180);

// ---------------------------------------------------------------------------
// The world one proposal runs against
// ---------------------------------------------------------------------------

/// The forge, the remote, the fixture and the recorded step order.
///
/// The two adapters see the *same* remote through different doors — `git` writes
/// to it over a path, the scripted `gh` reads its ref files — which is what makes
/// "the postcondition was read back rather than assumed" a real claim here.
struct World {
    dir: TempDir,
    remote: PathBuf,
    fixture: PathBuf,
    steps: Mutex<Vec<(EffectKind, &'static str)>>,
}

impl EffectTrace for World {
    fn step(&self, kind: EffectKind, step: ExecutionStep) {
        self.steps.lock().unwrap().push((kind, step.as_str()));
    }
}

impl World {
    /// An empty remote, an unrepaired fixture pointing at it, and a forge holding
    /// nothing.
    fn fresh() -> Self {
        let dir = TempDir::new().unwrap();
        // `remote.git` is the name the scripted `gh` looks for beside its own
        // scratch directory; see `tests/gh_stub/gh_stub.rs`.
        let remote = dir.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        fixture::git(&remote, &["init", "-q", "--bare", "."]);
        // Empty, and stays empty: it is what a real `gh` would be pinned to, and
        // beside an absent `HOME` it is what makes an operator's keyring
        // unreachable.
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        // A conversation that really is empty, said on purpose. The scripted `gh`
        // panics on an unscripted page rather than answering an empty one — an
        // absent file is an oversight, and a fixture that defaulted it would let a
        // test assert "no question has been asked" against a world it never
        // built. The comments this run posts are appended to this page by the
        // stub, which is how the same read sees them afterwards.
        std::fs::create_dir_all(dir.path().join("issue-comments")).unwrap();
        std::fs::write(dir.path().join("issue-comments/page-1.json"), "[]").unwrap();

        let fixture = fixture::broken_crate(dir.path());
        fixture::git(
            &fixture,
            &["remote", "add", "origin", &remote.display().to_string()],
        );

        World {
            dir,
            remote,
            fixture,
            steps: Mutex::new(Vec::new()),
        }
    }

    fn workspace_root(&self) -> PathBuf {
        self.dir.path().join("workspaces")
    }

    /// The tree the push publishes, derived the way the capability derives it —
    /// through the product's own function, so this fixture cannot agree with a
    /// capability that computed a different path.
    fn work(&self) -> PathBuf {
        attempt_worktree(&self.workspace_root(), PROJECT, INVOCATION_REF)
    }

    /// A context whose `gh` is the scripted one, whose `git` is the real one, and
    /// whose worktree is the one this run's attempt will work in.
    fn ctx(&self) -> EffectContext {
        self.ctx_publishing_from(self.work())
    }

    fn ctx_publishing_from(&self, work: PathBuf) -> EffectContext {
        EffectContext::new(
            GhCli::new(
                PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
                // The scratch directory arrives in `argv` because the adapter's
                // environment has room for exactly five names.
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
            work,
            CancellationToken::new(),
        )
    }

    /// The branch this run's identity produces, recomputed rather than read back.
    fn branch(&self) -> String {
        branch_name(PROJECT, INVOCATION_REF)
    }

    /// Every branch the remote holds, in ref order.
    fn branches(&self) -> Vec<String> {
        self.git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    /// What the published branch points at, according to the remote.
    fn published_sha(&self) -> String {
        self.git_says(
            &self.remote,
            &["rev-parse", &format!("refs/heads/{}", self.branch())],
        )
    }

    /// One file of the published commit, as the remote holds it.
    ///
    /// Read out of the bare repository rather than out of the worktree the attempt
    /// worked in — the worktree is gone by then, and a tree read from it would be
    /// a claim about a directory rather than about what was published.
    fn published_file(&self, path: &str) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["show", &format!("refs/heads/{}:{path}", self.branch())])
            .current_dir(&self.remote)
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn git_says(&self, dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Every request the scripted `gh` recorded, in arrival order.
    fn requests(&self) -> Vec<serde_json::Value> {
        let dir = self.dir.path().join("requests");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        files
            .iter()
            .filter_map(|file| serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok())
            .collect()
    }

    /// The `argv` of every recorded request, flattened.
    fn argvs(&self) -> Vec<Vec<String>> {
        self.requests()
            .iter()
            .map(|request| {
                request["argv"]
                    .as_array()
                    .map(|argv| {
                        argv.iter()
                            .filter_map(|a| a.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect()
    }

    /// The bodies of the `POST`s to one path suffix, in arrival order.
    ///
    /// Counted from the requests rather than from the objects, because that is the
    /// number a sequence that failed to stop would move and the object count might
    /// not.
    fn posts_to(&self, suffix: &str) -> Vec<serde_json::Value> {
        self.requests()
            .iter()
            .filter(|request| {
                let argv: Vec<String> = request["argv"]
                    .as_array()
                    .map(|argv| {
                        argv.iter()
                            .filter_map(|a| a.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                argv.iter().any(|a| a == "POST")
                    && argv.iter().any(|a| a.trim_end().ends_with(suffix))
            })
            .map(|request| {
                serde_json::from_str(request["body"].as_str().unwrap_or("{}"))
                    .unwrap_or(serde_json::Value::Null)
            })
            .collect()
    }

    /// The pull requests this world was asked to create.
    fn pull_request_creates(&self) -> Vec<serde_json::Value> {
        self.posts_to("/pulls")
    }

    /// The comments this world was asked to post, oldest first.
    fn posted_comments(&self) -> Vec<String> {
        self.posts_to("/comments")
            .iter()
            .map(|body| body["body"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    /// Put the pull request the world already holds where a by-number read can
    /// find it.
    ///
    /// The scripted `gh` answers `GET /repos/{o}/{r}/pulls/{n}` from a file and
    /// panics when there is none, deliberately — an unscripted read is an
    /// oversight and not a scenario. A *continuing* process reads the pull request
    /// for its head, so a world that is to be continued in has to hold one.
    fn pull_request_at(&self, number: u64, head_sha: &str, draft: bool) {
        let dir = self.dir.path().join("pulls_by_number");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{number}.json")),
            json!({
                "number": number,
                "draft": draft,
                "state": "open",
                "node_id": "PR_kwDOabcdef",
                "head": { "sha": head_sha },
            })
            .to_string(),
        )
        .unwrap();
    }

    /// Which effects the executor entered `apply` for, in order.
    ///
    /// The mutation, and never the proposal: this is the list "the walk performed
    /// exactly these three things, in this order" is stated in.
    fn effects_performed(&self) -> Vec<EffectKind> {
        self.steps_matching(ExecutionStep::Apply)
    }

    /// Which effects were put to the executor at all, in order.
    ///
    /// Step 1 is announced before anything else happens, so an effect that was
    /// proposed and refused appears here and not in
    /// [`World::effects_performed`] — which is the distinction the gated effect's
    /// absence has to be stated in.
    fn effects_proposed(&self) -> Vec<EffectKind> {
        self.steps_matching(ExecutionStep::ValidateCapability)
    }

    fn steps_matching(&self, step: ExecutionStep) -> Vec<EffectKind> {
        self.steps
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, entered)| *entered == step.as_str())
            .map(|(kind, _)| *kind)
            .collect()
    }

    fn steps(&self) -> Vec<(EffectKind, &'static str)> {
        self.steps.lock().unwrap().clone()
    }
}

// ---------------------------------------------------------------------------
// The capability under test
// ---------------------------------------------------------------------------

/// The configuration every scenario below runs under, with `check` and
/// `workspace_root` left for the scenario to vary.
fn config(world: &World, check: WorkspaceCommand) -> ProposeConfig {
    ProposeConfig {
        repo: REPO.to_string(),
        head_owner: HEAD_OWNER.to_string(),
        base: BASE.to_string(),
        title: "propose the change".to_string(),
        body: "opened by fiddle".to_string(),
        project: PROJECT.to_string(),
        fixture: world.fixture.clone(),
        workspace_root: world.workspace_root(),
        check,
        budget: AgentBudget {
            max_turns: 8,
            max_tokens: 4096,
            deadline: Duration::from_secs(300),
            max_changed_files: 16,
            tool_timeout: PATIENT,
        },
        cancel: CancellationToken::new(),
    }
}

/// M1's own check, unchanged: the package's test suite, offline.
///
/// The fixture genuinely fails it until `src/lib.rs` is edited, which is what
/// makes every verdict below a verdict about the tree rather than about the model.
fn the_projects_own_check() -> WorkspaceCommand {
    WorkspaceCommand {
        program: "cargo".to_string(),
        args: vec!["test".to_string(), "--offline".to_string()],
        timeout: PATIENT,
    }
}

/// A check that passes over any tree at all, for the one case that is about a
/// tree nobody changed.
fn a_check_that_always_passes() -> WorkspaceCommand {
    WorkspaceCommand {
        program: "git".to_string(),
        args: vec!["status".to_string(), "--porcelain".to_string()],
        timeout: PATIENT,
    }
}

/// The model writes the fix, runs the check itself, and reports.
fn repairs() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call(
            "c1",
            "write_file",
            json!({"path": "src/lib.rs", "contents": fixture::REPAIRED}),
        ),
        MockTurn::tool_call("c2", "run_check", json!({})),
        MockTurn::text(
            r#"{"changed_files":["src/lib.rs"],"summary":"fixed","claimed_complete":true}"#,
        ),
    ]
}

/// The model reads one file, changes nothing, and says it is done.
fn claims_success() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call("c1", "read_file", json!({"path": "src/lib.rs"})),
        MockTurn::text(r#"{"changed_files":[],"summary":"all good","claimed_complete":true}"#),
    ]
}

/// The model writes something *else*, so a run that attempted twice would publish
/// a different commit and be visible in the tree.
fn repairs_differently() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call(
            "c1",
            "write_file",
            json!({"path": "src/lib.rs", "contents": "pub fn last_index(len: usize) -> usize { len - 1 } // again\n"}),
        ),
        MockTurn::text(
            r#"{"changed_files":["src/lib.rs"],"summary":"again","claimed_complete":true}"#,
        ),
    ]
}

fn grant_for(capability: fiddle_core::CapabilityId) -> ExecutionGrant {
    ExecutionGrant::authorise(
        &NextAction::Execute {
            capability_id: capability,
        },
        &fiddle_core::AttemptId(ATTEMPT.to_string()),
    )
    .expect("an Execute derivation authorises")
}

/// One whole execution, and the capability afterwards so its receipts and its
/// publication can be read.
///
/// The capability is built here rather than by each test because four things have
/// to agree for a run to be legal at all — the executor's binding, the
/// configuration's project, the context's worktree and the grant — and a test
/// that assembled them itself would be free to get one wrong for a reason it did
/// not intend.
async fn run(
    world: &World,
    script: Vec<MockTurn>,
    check: WorkspaceCommand,
) -> (
    Result<EvidenceRef, CapabilityError>,
    Vec<EvidenceRef>,
    Option<fiddle_core::Publication>,
) {
    run_with(world, script, check, PROPOSE_CHANGE, None).await
}

/// The same, with the executor's binding and the published-from tree open to a
/// test that is about one of them.
async fn run_with(
    world: &World,
    script: Vec<MockTurn>,
    check: WorkspaceCommand,
    bound_to: fiddle_core::CapabilityId,
    publishing_from: Option<PathBuf>,
) -> (
    Result<EvidenceRef, CapabilityError>,
    Vec<EvidenceRef>,
    Option<fiddle_core::Publication>,
) {
    let ctx = match publishing_from {
        Some(work) => world.ctx_publishing_from(work),
        None => world.ctx(),
    };
    let deployment = Deployment(DeploymentRule::Allow);
    let executor = Executor::new(
        bound_to,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        world,
        ReadRetry::none(),
    );
    let capability = ProposeChange::new(
        executor,
        &ctx,
        MockCompletionModel::new(script),
        config(world, check),
    );

    let outcome = capability
        .execute(grant_for(PROPOSE_CHANGE), WORK_ID, INVOCATION_REF)
        .await;
    (outcome, capability.receipts(), capability.publication())
}

/// The request id, the gated effect id and the payload digest a *fresh* process
/// would derive for one pull request at one revision.
///
/// Recomputed from the canonical inputs rather than read back out of the marker,
/// so an assertion about the marker cannot pass on a build that invented an
/// identity and wrote it down consistently.
fn identity_at(
    head_sha: &str,
) -> (
    fiddle_core::DecisionRequestId,
    fiddle_core::EffectId,
    fiddle_core::PayloadHash,
) {
    let effect = effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsurePullRequestReady,
        &pull_request_ready_target(REPO, PR, head_sha),
    );
    let request = fiddle_core::decision_request_id(PROJECT, INVOCATION_REF, &effect);
    let payload = payload_hash(
        &EnsurePullRequestReady::new(REPO.to_string(), PR, head_sha.to_string()).payload(),
    );
    (request, effect, payload)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// It names itself, it names its own stage, and the stage is not another
/// capability's vocabulary — the defect `Capability::stage` exists because of.
///
/// # What this test does *not* say, and why
///
/// The bean asks for `["stub_mark", "fixture_repair", "publish_change",
/// "propose_change"]` out of `CAPABILITIES`, and that list is deliberately still
/// three long. An id in it is a claim that an operator can pass it to
/// `--capability`, and `every_registered_capability_can_be_selected` in the
/// binary enforces exactly that by walking the array and requiring each id to
/// name a selection the CLI can build. `propose_change` has no selection yet and
/// cannot have one until `resolve_forge` learns to publish from a worktree that
/// does not exist when the forge is resolved — it reads that tree's `HEAD` — so
/// the entry and the selection belong to the CLI task, together. Registering it
/// here first would advertise a capability nothing can run, which is the defect
/// the binary's test exists to catch.
///
/// So this asserts the id the capability answers to, which is the half that is
/// this task's, and pins the absence with its reason so the pair is not forgotten.
#[tokio::test]
async fn the_fourth_capability_names_itself_and_names_its_own_stage() {
    let ids: Vec<&str> = fiddle_runtime::CAPABILITIES
        .iter()
        .map(|capability| capability.0)
        .collect();
    assert_eq!(ids, ["stub_mark", "fixture_repair", "publish_change"]);
    assert_eq!(PROPOSE_CHANGE.0, "propose_change");

    let world = World::fresh();
    let ctx = world.ctx();
    let deployment = Deployment(DeploymentRule::Allow);
    let executor = Executor::new(
        PROPOSE_CHANGE,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        &world,
        ReadRetry::none(),
    );
    let capability = ProposeChange::new(
        executor,
        &ctx,
        MockCompletionModel::new(repairs()),
        config(&world, the_projects_own_check()),
    );
    assert_eq!(capability.id(), PROPOSE_CHANGE);
    assert_eq!(capability.stage(), "propose");
    assert!(
        capability.publication().is_none(),
        "a capability that has not run has reached no forge"
    );
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

/// The walk, in order, and nothing out of it. The first run performs three
/// automatic effects and then stops.
#[tokio::test]
async fn a_first_run_publishes_a_branch_a_draft_and_a_question_then_waits() {
    let world = World::fresh();
    let (outcome, _, _) = run(&world, repairs(), the_projects_own_check()).await;

    let error = outcome.expect_err("a run that asked a question produced no evidence");
    assert!(
        matches!(error, CapabilityError::AwaitingDecision { .. }),
        "got {error:?}"
    );
    assert_eq!(
        error.recurrence(),
        Recurrence::Awaiting,
        "waiting is not failing, and exit 10 is what says so"
    );

    assert_eq!(
        world.effects_performed(),
        [
            EffectKind::EnsureBranchPublished,
            EffectKind::EnsurePullRequest,
            EffectKind::PublishDecisionRequest,
        ],
        "{:?}",
        world.steps()
    );

    // And each of the three really reached the world, read from three different
    // doors: the remote's refs, the create the forge recorded, and the collection
    // the comment was posted to.
    assert_eq!(world.branches(), [world.branch()]);
    let creates = world.pull_request_creates();
    assert_eq!(creates.len(), 1, "{creates:?}");
    assert_eq!(
        creates[0]["draft"],
        json!(true),
        "the pull request is opened as a draft, because the transition out of \
         draft is the gated act: {}",
        creates[0]
    );
    assert_eq!(
        creates[0]["head"],
        json!(format!("{HEAD_OWNER}:{}", world.branch()))
    );
    assert_eq!(world.posted_comments().len(), 1);
}

/// The gated effect is not proposed on the first run at all. Proposing it and
/// catching the refusal would work, and would also mean a deployment document
/// that said `deny` produced a denial rather than a question.
#[tokio::test]
async fn the_gated_effect_is_not_proposed_before_there_is_an_answer() {
    let world = World::fresh();
    let _ = run(&world, repairs(), the_projects_own_check()).await;

    assert!(!world
        .effects_performed()
        .contains(&EffectKind::EnsurePullRequestReady));
    assert!(
        !world
            .effects_proposed()
            .contains(&EffectKind::EnsurePullRequestReady),
        "the effect must not reach the executor at all: {:?}",
        world.effects_proposed()
    );
    // The mutation has exactly one spelling — `markPullRequestReadyForReview`
    // through GraphQL — so a run that had performed it some other way is visible
    // here rather than only in the trace this same run wrote.
    assert!(
        !world
            .argvs()
            .iter()
            .any(|argv| argv.iter().any(|arg| arg == "graphql")),
        "no GraphQL call may be made before there is an answer: {:?}",
        world.argvs()
    );
}

/// The hybrid half: the change comes from a bounded attempt, and the commit that
/// is published is the one the attempt produced.
///
/// Read out of the remote, so this is a claim about what a reviewer would open. It
/// is also what makes the derived worktree path load-bearing rather than
/// incidental: the branch effect pushes `HEAD` out of the context's tree, so a
/// capability whose attempt worked anywhere else would publish the fixture's own
/// commit — which the second assertion below rules out.
#[tokio::test]
async fn the_published_commit_is_what_the_attempt_left_behind() {
    let world = World::fresh();
    let (_, _, _) = run(&world, repairs(), the_projects_own_check()).await;

    assert_eq!(
        world.published_file("src/lib.rs").as_deref(),
        Some(fixture::REPAIRED),
        "the published tree carries the file the attempt wrote"
    );
    assert_ne!(
        world.published_sha(),
        world.git_says(&world.fixture, &["rev-parse", "HEAD"]),
        "and it is a new commit, not the tree the attempt started from"
    );
    // The fixture itself is untouched: the attempt lived and died in a worktree.
    assert_eq!(fixture::changed_files(&world.fixture), Vec::<String>::new());
}

/// M1's rule, unmoved: the capability's outcome is decided by the check it runs
/// itself, over the tree the attempt actually left. A model's claim is evidence
/// beside the exit code that overruled it, never instead of it.
#[tokio::test]
async fn an_attempt_whose_check_failed_publishes_nothing_and_asks_nothing() {
    let world = World::fresh();
    let (outcome, _, publication) = run(&world, claims_success(), the_projects_own_check()).await;

    let error = outcome.expect_err("a failing check earns nothing");
    assert!(
        !matches!(error, CapabilityError::AwaitingDecision { .. }),
        "must not ask about a failure: {error:?}"
    );
    match &error {
        CapabilityError::CheckFailed {
            claimed, exit_code, ..
        } => {
            assert!(
                *claimed,
                "the claim is carried as evidence, so it must be recorded"
            );
            assert_ne!(*exit_code, 0, "the check is what decided this");
        }
        other => panic!("a failing check must be reported as such, got {other:?}"),
    }
    assert_eq!(error.recurrence(), Recurrence::Correctable);

    assert_eq!(world.effects_performed(), []);
    assert_eq!(
        world.effects_proposed(),
        [],
        "nothing was even proposed: {:?}",
        world.steps()
    );
    assert_eq!(world.branches(), Vec::<String>::new());
    assert_eq!(world.posted_comments(), Vec::<String>::new());
    // And the run still says what it saw of the forge, which is nothing.
    let review = publication
        .expect("a publication is reported on every arm")
        .review;
    assert!(
        matches!(review, Observation::Unavailable { .. }),
        "an unpublished run has read no forge and must not claim to: {review:?}"
    );
}

/// A passing check over a tree nobody changed is not a change to propose.
///
/// The one arm where the check and the model agree and there is still nothing to
/// do. Publishing an empty commit would ask a person to approve a change that
/// does not exist.
#[tokio::test]
async fn an_attempt_that_changed_nothing_publishes_nothing_and_asks_nothing() {
    let world = World::fresh();
    let (outcome, _, _) = run(&world, claims_success(), a_check_that_always_passes()).await;

    let error = outcome.expect_err("there is nothing to propose");
    assert!(
        matches!(error, CapabilityError::NothingProposed),
        "got {error:?}"
    );
    assert_eq!(
        error.recurrence(),
        Recurrence::Correctable,
        "a later attempt may still produce something"
    );
    assert_eq!(world.effects_performed(), []);
    assert_eq!(world.branches(), Vec::<String>::new());
    assert_eq!(world.posted_comments(), Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// What the capability holds, and what it cannot do
// ---------------------------------------------------------------------------

/// It holds no credential and constructs no client — it receives an executor
/// already bound to its own id, and step 1 refuses a proposal under any other.
#[tokio::test]
async fn the_capability_cannot_propose_under_another_capabilitys_name() {
    let world = World::fresh();
    let (outcome, _, _) = run_with(
        &world,
        repairs(),
        the_projects_own_check(),
        PUBLISH_CHANGE,
        None,
    )
    .await;

    let error = outcome.expect_err("a proposal under another name is refused");
    assert!(
        error.to_string().contains("cannot propose for"),
        "got {error}"
    );
    assert_eq!(
        world.effects_performed(),
        [],
        "a refusal at step 1 reaches nothing"
    );
    assert_eq!(world.branches(), Vec::<String>::new());
    assert_eq!(world.posted_comments(), Vec::<String>::new());
}

/// A context that publishes from anywhere other than the tree this run's attempt
/// works in is refused before anything is read or written.
///
/// The hazard is specific: `EnsureBranchPublished` pushes `HEAD` out of the
/// context's worktree, so the two disagreeing means publishing a commit this run
/// never made — with a payload naming the commit it *did* make, and a
/// postcondition read that then disagrees with both. Checked rather than assumed,
/// and checked before the attempt, which is why the assertions below are that
/// nothing at all happened.
#[tokio::test]
async fn a_context_publishing_from_another_tree_is_refused_before_anything_runs() {
    let world = World::fresh();
    let elsewhere = world.dir.path().join("somewhere-else");
    let (outcome, _, _) = run_with(
        &world,
        repairs(),
        the_projects_own_check(),
        PROPOSE_CHANGE,
        Some(elsewhere.clone()),
    )
    .await;

    let error = outcome.expect_err("the two trees have to be one tree");
    assert!(
        matches!(error, CapabilityError::PublishesElsewhere { .. }),
        "got {error:?}"
    );
    assert_eq!(error.recurrence(), Recurrence::Permanent);
    assert!(
        error.to_string().contains("somewhere-else"),
        "the diagnostic names the tree it was pointed at: {error}"
    );
    assert!(
        !world.workspace_root().exists(),
        "a refused run must not even prepare a workspace"
    );
    assert!(
        world.requests().is_empty(),
        "and must not read the forge: {:?}",
        world.argvs()
    );
}

/// The capability names no credential and constructs no client.
///
/// A source-level assertion because that is the level the property is at: the type
/// system cannot express "names no secret", and the alternative — asserting that
/// no token *reached* GitHub — would pass on a capability that held one and
/// happened not to use it this time. `publish_change` is asserted the same way in
/// `effect_protocol.rs`, and this is the same list.
///
/// The last pair is this capability's own: it writes **no change set**, on any
/// path. A correlation marker says *this invocation accounts for this work*, and
/// the next invocation completes on it without executing — so a marker written by
/// a run that is waiting for an answer would stop the very process that was
/// supposed to read one.
#[test]
fn the_capability_holds_no_credential_and_accounts_for_no_work() {
    let source = include_str!("../src/capability/propose.rs");
    for named in ["GH_TOKEN", "FIDDLE_GITHUB_TOKEN", "token"] {
        assert!(
            !source.contains(named),
            "the capability names no credential, and it names `{named}`"
        );
    }
    for constructed in ["GhCli", "GitCli", "EffectContext::new"] {
        assert!(
            !source.contains(constructed),
            "the capability constructs no client, and it constructs `{constructed}`"
        );
    }
    for written in ["ChangeSetState", "write_atomically", "correlation_key"] {
        assert!(
            !source.contains(written),
            "a suspended run accounts for no work, and this names `{written}`"
        );
    }
}

// ---------------------------------------------------------------------------
// What a suspended run leaves behind
// ---------------------------------------------------------------------------

/// Receipts and publication survive the suspending arm, which is M1's and M2's
/// rule and is exactly what an operator needs from a run that stopped part-way.
#[tokio::test]
async fn a_suspended_run_still_reports_what_it_did_reach() {
    let world = World::fresh();
    let (_, receipts, publication) = run(&world, repairs(), the_projects_own_check()).await;

    let publication = publication.expect("a publication is reported on every arm");
    match &publication.review {
        Observation::Available {
            value, revision, ..
        } => {
            assert_eq!(value.pull_request, Some(PR));
            assert_eq!(value.branch.as_deref(), Some(world.branch().as_str()));
            assert_eq!(value.state.as_deref(), Some("open"));
            assert_eq!(
                revision.as_deref(),
                Some(world.published_sha().as_str()),
                "the revision is the one the remote was observed to hold"
            );
        }
        other => panic!("a published run describes its review, got {other:?}"),
    }
    assert!(
        matches!(publication.verification, Observation::NotApplicable { .. }),
        "this capability requests no check, so it makes no claim about CI: {:?}",
        publication.verification
    );

    // The tool summary, the attempt's own reference, and one per effect.
    let rendered: Vec<&str> = receipts.iter().map(|entry| entry.0.as_str()).collect();
    assert!(receipts.len() >= 3, "{rendered:?}");
    assert!(
        rendered.contains(&"tools:2"),
        "an attempt's tool calls are counted even when nothing went wrong: {rendered:?}"
    );
    assert!(
        rendered
            .iter()
            .any(|entry| *entry == format!("propose:1:{ATTEMPT}")),
        "the evidence names what git saw change and the attempt it was granted: {rendered:?}"
    );
    let kinds: Vec<&str> = rendered
        .iter()
        .filter(|entry| entry.starts_with("effect:"))
        .map(|entry| entry.split(':').nth(1).unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "ensure_branch_published",
            "ensure_pull_request",
            "publish_decision_request"
        ],
        "{rendered:?}"
    );
}

/// The question a person reads and the question this run says it is waiting on
/// are the same question.
///
/// **This is the duplicate-id hazard, stated from the outside.**
/// `HumanDecisionRequest` carries the request id twice — as its own field and
/// inside `binding` — and only `binding.request` is rendered into the marker. A
/// producer that filled the two from two derivations, or a consumer that read the
/// other one, would publish a marker naming one question and then look for
/// another: it would find nothing, conclude it had not asked yet, and post again
/// on every attempt forever. So the marker is parsed back out of the comment the
/// world really received and required to name the id the run is waiting on — and
/// every field of it is recomputed here from canonical inputs, so this cannot pass
/// on a build that invented an identity and then wrote it down consistently.
///
/// **Why agreement is the assertion.** A single derivation has no direct
/// observable — nothing outside the capability can see how many times an id was
/// computed. What it *does* have is a consequence: two values built from one
/// derivation cannot disagree, and two built from two derivations have no reason
/// to agree. So the only honest way to test "the id came from one place" is to
/// take the two places it surfaces — the marker on the conversation and the
/// request the error names — and require them to be the same string. Twenty-four
/// tests passed over this bug elsewhere because every one of them built the two
/// ids agreeing; this one takes them from the world instead.
#[tokio::test]
async fn the_suspended_run_waits_on_the_question_the_comment_carries() {
    let world = World::fresh();
    let (outcome, _, _) = run(&world, repairs(), the_projects_own_check()).await;

    let (request, effect, payload) = identity_at(&world.published_sha());
    let (waiting_on, interaction, question) = match outcome {
        Err(CapabilityError::AwaitingDecision {
            request,
            interaction,
            question,
        }) => (request, interaction, question),
        other => panic!("a first run suspends, got {other:?}"),
    };
    assert_eq!(waiting_on, request);

    let comments = world.posted_comments();
    assert_eq!(comments.len(), 1, "{comments:?}");
    let binding = parse_marker(&comments[0]).expect("the comment carries a marker");
    assert_eq!(
        binding.request, waiting_on,
        "the marker names this question"
    );
    assert_eq!(
        binding.effect, effect,
        "and the effect an approval would gate"
    );
    assert_eq!(binding.payload, payload, "and the payload it was shown");
    assert_eq!(binding.head_sha, world.published_sha());

    // The question a person reads is a question, and the body carries it.
    assert!(comments[0].contains(&question), "{}", comments[0]);
    assert!(
        comments[0].contains("ready for review?"),
        "yes and no both have to mean something: {}",
        comments[0]
    );
    // And the conversation the run names is the comment that was created.
    match interaction {
        InteractionRef::GitHubPullRequestComment { repo, pr, comment } => {
            assert_eq!(repo, REPO);
            assert_eq!(pr, PR);
            assert_ne!(comment, 0, "a comment id nobody sent names no comment");
        }
    }
}

/// A second process, with no memory of the first, finds its own question rather
/// than asking it again.
///
/// Nothing local survives between the two runs — the worktree was removed when the
/// first capability's execution ended, and the second holds a different
/// capability value with empty receipts — so the only thing the second run can
/// recognise its own work from is the world. It is given a model that would write
/// something *different*, which is what makes "it did not attempt again" an
/// assertion about the published tree rather than about an internal counter.
#[tokio::test]
async fn a_second_process_finds_its_own_question_and_does_not_ask_twice() {
    let world = World::fresh();
    let first = run(&world, repairs(), the_projects_own_check()).await;
    assert!(first.0.is_err());
    let published = world.published_sha();
    // The world the second process reads is the world the first one left, and the
    // by-number read is arranged through the stub's own seed rather than by the
    // code under test.
    world.pull_request_at(PR, &published, true);

    // A second capability value, with empty receipts and no worktree, over the
    // same forge — which is all a fresh process has.
    let (outcome, receipts, _) = run(&world, repairs_differently(), the_projects_own_check()).await;

    let error = outcome.expect_err("the question stands, so the run is still waiting");
    match &error {
        CapabilityError::AwaitingDecision { request, .. } => {
            assert_eq!(*request, identity_at(&published).0, "the same question");
        }
        other => panic!("a run whose question is unanswered waits, got {other:?}"),
    }
    assert_eq!(
        world.posted_comments().len(),
        1,
        "no second question was posted"
    );
    assert_eq!(
        world.published_sha(),
        published,
        "and no second commit was published, so the attempt did not run again"
    );
    assert_eq!(
        world.published_file("src/lib.rs").as_deref(),
        Some(fixture::REPAIRED),
        "the tree is still the first attempt's"
    );
    assert!(
        receipts.contains(&EvidenceRef("tools:0".to_string())),
        "a continuation calls no tool, because it runs no attempt: {receipts:?}"
    );
    // The three effects of the *first* run only. The second proposed nothing: it
    // read, recognised its own question, and stopped.
    assert_eq!(
        world.effects_performed(),
        [
            EffectKind::EnsureBranchPublished,
            EffectKind::EnsurePullRequest,
            EffectKind::PublishDecisionRequest,
        ],
        "{:?}",
        world.steps()
    );
}

/// A pull request with no question on it is resumed by asking, not by attempting
/// again.
///
/// The state a process interrupted between the create and the comment leaves
/// behind, which is a third answer to "is this a first run" that neither of the
/// other two covers. A second attempt would produce a different commit, the push
/// would then be a refused non-fast-forward, and the run would be stuck for good;
/// the change is already out there, and what is missing is the question.
#[tokio::test]
async fn a_published_change_nobody_has_been_asked_about_is_resumed_by_asking() {
    let world = World::fresh();
    // Arrange the world the interrupted run left: the branch pushed and the pull
    // request open, with nothing on its conversation. Both are put there with the
    // test's own tools rather than by the code under test.
    let fixed = world.dir.path().join("fixed");
    fixture::git(
        &world.fixture,
        &[
            "worktree",
            "add",
            "--detach",
            "-q",
            &fixed.display().to_string(),
            "HEAD",
        ],
    );
    std::fs::write(fixed.join("src/lib.rs"), fixture::REPAIRED).unwrap();
    fixture::git(&fixed, &["add", "src/lib.rs"]);
    fixture::git(
        &fixed,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "the interrupted run's commit",
        ],
    );
    let published = world.git_says(&fixed, &["rev-parse", "HEAD"]);
    fixture::git(
        &fixed,
        &[
            "push",
            "-q",
            "origin",
            &format!("HEAD:refs/heads/{}", world.branch()),
        ],
    );
    std::fs::write(
        world.dir.path().join("pulls_seed"),
        json!([{
            "head": format!("{HEAD_OWNER}:{}", world.branch()),
            "base": BASE,
            "title": "opened before the interruption",
        }])
        .to_string(),
    )
    .unwrap();
    world.pull_request_at(PR, &published, true);

    let (outcome, _, _) = run(&world, repairs_differently(), the_projects_own_check()).await;

    let error = outcome.expect_err("the question is what was missing, so the run asks it");
    assert!(
        matches!(error, CapabilityError::AwaitingDecision { .. }),
        "got {error:?}"
    );
    assert_eq!(
        world.effects_performed(),
        [EffectKind::PublishDecisionRequest],
        "only the question: the change was already published: {:?}",
        world.steps()
    );
    assert_eq!(world.posted_comments().len(), 1);
    assert_eq!(
        world.published_sha(),
        published,
        "the commit that was already out there is the one the question is about"
    );
    assert_eq!(
        parse_marker(&world.posted_comments()[0])
            .expect("a marker")
            .head_sha,
        published
    );
}
