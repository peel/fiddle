//! The deterministic protocol suite: this milestone's gate.
//!
//! Everything here is free, offline and always runs. The model is
//! `MockCompletionModel`, an ordinary test dependency of this crate rather than
//! a seam in the product: `attempt` and `FixtureRepair` are generic over Rig's
//! own `CompletionModel`, so a script substitutes where a gateway would, and
//! nothing in `src/` knows a test is happening. There is no transcript provider
//! and no test-only runtime mode, and there is no credential or socket anywhere
//! in this file.
//!
//! # Why the success path lives here and not in a real-model smoke test
//!
//! A scripted model writes *known-correct* content, so the chain
//! "check passed ⇒ Completed ⇒ marker written" is proven with zero model
//! dependence. Without it the suite would be satisfiable by a `write_file` that
//! silently did nothing, because "check failed ⇒ Retryable" is legal behaviour
//! and every adversarial case below asserts exactly that. The two halves are
//! deliberately the same fixture, the same check and the same tool: only the
//! *contents* the script writes differ between
//! [`the_success_path_is_proven_without_any_model_dependence`] and
//! [`a_model_claiming_success_over_a_broken_fixture_is_disbelieved`], so a tool
//! that no-ops cannot make the first pass, and a shell that believed the model
//! could not make the second fail.
//!
//! # Why these are `fiddle-runtime` integration tests and not black-box
//!
//! Their claim is about the *shell's* response to a model input, not about the
//! assembled binary. `m0_skeleton` remains the black-box proof of the
//! deterministic path.
//!
//! # Why almost everything is driven through `orchestration::run`
//!
//! Several of these refusals already have unit tests in `agent/`, `workspace/`
//! and `capability/repair.rs`, and those assert about an inner `Result`. What is
//! asserted here is the *run outcome and the marker* — that a refusal survives
//! the whole shell, that a bounded attempt is retryable rather than complete,
//! and above all that no path but a passing check ever records a correlation
//! key. A refusal that were somehow swallowed between the tool and the report
//! would pass every unit test and fail every scenario in this file.

mod fixture;

use fiddle_runtime::agent::AgentBudget;
use fiddle_runtime::capability::{Capability, ExecutionGrant, FixtureRepair, RepairConfig};
use fiddle_runtime::core::{correlation_key, AttemptId, NextAction, RunOutcome, FIXTURE_REPAIR};
use fiddle_runtime::journal::FileJournal;
use fiddle_runtime::orchestration::{self, RunContext, RunReport};
use fiddle_runtime::workspace::WorkspaceCommand;
use fiddle_runtime::{StubChangePort, StubWorkItemPort};
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

const WORK_ID: &str = "fiddle-m1-demo";
const INVOCATION_REF: &str = "beans:fiddle-m1-demo";
const SLUG: &str = "beans-fiddle-m1-demo";
const PROJECT: &str = "icecube";
const ATTEMPT: &str = "01JQZX0000000000000000000";

// ---------------------------------------------------------------------------
// The fixture builder's own test
// ---------------------------------------------------------------------------

/// The fixture has to be able to fail before anything asserted about a verdict
/// means anything.
///
/// Written against `tests/fixture.rs`'s builder rather than against a copy,
/// because the builder this suite drives and the builder that proves itself must
/// be the same one. It lives in this file rather than in `fixture.rs` for a
/// mechanical reason: `fixture.rs` is `mod fixture;` in three other test
/// binaries *and* a test target of its own, so a `#[test]` there would run four
/// nested `cargo test` invocations to prove one thing once.
///
/// The changed-file assertion is the second half and the one that is easy to
/// omit: running the check writes `target/` and — even for a package with no
/// dependencies — `Cargo.lock`, and unless both are ignored the repaired tree
/// reports paths nobody edited.
#[test]
fn the_fixture_starts_broken_and_is_repairable_offline() {
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::broken_crate(dir.path());

    let before = fixture::check(&repo);
    assert!(
        !before.success(),
        "the fixture must fail its own check before repair: {}",
        before.output
    );

    std::fs::write(repo.join("src/lib.rs"), fixture::REPAIRED).unwrap();
    let after = fixture::check(&repo);
    assert!(after.success(), "and pass it after: {}", after.output);

    assert_eq!(
        fixture::changed_files(&repo),
        vec!["src/lib.rs".to_string()],
        "with target/ and Cargo.lock ignored, the only changed path is the one that was edited"
    );
}

// ---------------------------------------------------------------------------
// The success path
// ---------------------------------------------------------------------------

/// **The one thing no real-model test may assert.**
///
/// `execute` returning `Ok` is itself the proof that the shell's own check
/// passed: that is the single path past the exit-code branch in
/// `capability/repair.rs`, and the fixture underneath it genuinely fails
/// `cargo test` until `src/lib.rs` is edited. So a `write_file` that resolved a
/// path, reported bytes written and touched nothing would fail here — while
/// still satisfying every adversarial case in this file, each of which expects
/// the run to earn nothing.
///
/// The last pair is the stability property, end to end: the marker this
/// capability wrote is the one the *next* invocation's assessment recognises,
/// through the real derivation rather than through a comparison this test makes
/// up.
///
/// What is deliberately *not* asserted is that the *fixture* now passes its
/// check, and the two assertions in the middle say the opposite on purpose. M1
/// delivers a verdict, not a repair: the worktree is per-attempt and removed
/// however the execution ends, so what an accepted repair leaves behind is the
/// marker and the evidence reference, and getting the repair itself out is M2's
/// branch and pull request. The claim an assertion about the fixture's own tree
/// would be reaching for is carried instead by that evidence reference, which
/// names what *git* saw change inside the worktree — one file, which with a
/// passing check can only be the source that was broken.
#[tokio::test]
async fn the_success_path_is_proven_without_any_model_dependence() {
    let f = broken_fixture();
    let evidence = FixtureRepair::new(MockCompletionModel::new(repairs()), f.config())
        .execute(grant(), WORK_ID, INVOCATION_REF)
        .await
        .expect("the shell's own check must pass after the repair");

    assert_eq!(
        evidence.0,
        format!("repair:1:{ATTEMPT}"),
        "git saw exactly one file change, which with a passing check can only be the source"
    );
    assert_eq!(
        f.marker(WORK_ID),
        Some(correlation_key(PROJECT, INVOCATION_REF)),
        "the marker must be the one the next invocation's assessment expects"
    );

    // The verdict, not the delivery.
    assert!(
        !f.check_passes(),
        "M1 leaves the repair in a worktree it then removes; the fixture is untouched"
    );
    assert_eq!(f.changed_files(), Vec::<String>::new());
    assert_no_workspace_survived(&f);

    let second = f.run(repairs(), f.config()).await;
    assert_eq!(second.outcome, RunOutcome::Completed);
    assert!(
        second.executions.is_empty(),
        "a satisfied world must not execute again: {:?}",
        second.executions
    );
}

// ---------------------------------------------------------------------------
// What the bundle says the attempt did
// ---------------------------------------------------------------------------

/// **The published evidence names the tools that actually ran.**
///
/// Everything else in this file asserts what an attempt *earned*. This pair
/// asserts what it *did*, and the distinction is not academic: for the whole of
/// this milestone's development, `FixtureRepair` built a `ToolHost`, handed it
/// to `attempt`, and never read `host.receipts()` back — so the bundle said
/// nothing at all about tool use, however much or little there had been.
///
/// The receipts are a summary rather than the raw records, because
/// `EvidenceRef` is a string and the report schema is a published contract. The
/// summary answers what a bundle is actually asked: were any tools called,
/// which, and how did each go.
#[tokio::test]
async fn the_published_evidence_names_the_tools_that_ran() {
    let f = broken_fixture();
    let report = f.run(repairs(), f.config()).await;

    assert_eq!(report.outcome, RunOutcome::Completed);
    let evidence = evidence_of(&report);
    assert!(
        evidence.contains(&"tools:2".to_string()),
        "the script calls exactly two tools: {evidence:?}"
    );
    assert!(
        evidence.contains(&"tool:write_file:ok:1".to_string()),
        "{evidence:?}"
    );
    assert!(
        evidence.contains(&"tool:run_check:ok:1".to_string()),
        "{evidence:?}"
    );
    assert!(
        evidence.contains(&format!("repair:1:{ATTEMPT}")),
        "the reference the capability earned still leads: {evidence:?}"
    );
}

/// **An attempt that called nothing says so, out loud.**
///
/// `tools:0` is the shape of the defect this evidence exists to make visible: a
/// model that answers with the structured report on its first turn, calls no
/// tool, changes nothing, and is refused by a check that was judging an
/// untouched tree. From outside the process that is indistinguishable from a
/// model that tried and lost — unless the bundle says which it was.
///
/// Reached here with a scripted model that claims completion immediately, which
/// is exactly what every model on the gateway did while the agent pinned native
/// structured output for the whole run.
#[tokio::test]
async fn an_attempt_that_called_no_tools_publishes_tools_zero() {
    let f = broken_fixture();
    let report = f.run(vec![completion_claim()], f.config()).await;

    assert_retryable_because(&report, "the check exited");
    assert_eq!(
        evidence_of(&report),
        vec!["tools:0".to_string()],
        "a model that called nothing must be published as having called nothing"
    );
    assert_eq!(
        f.marker(WORK_ID),
        None,
        "and it must still have earned nothing"
    );
}

/// The evidence of the one execution `report` recorded, as plain strings.
fn evidence_of(report: &RunReport) -> Vec<String> {
    assert_eq!(report.executions.len(), 1, "{:?}", report.executions);
    report.executions[0]
        .evidence
        .iter()
        .map(|reference| reference.0.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// The adversarial cases
// ---------------------------------------------------------------------------

/// **The centre of this milestone, through the whole shell.**
///
/// The unit test in `capability/repair.rs` asserts the typed
/// `CapabilityError::CheckFailed`. What is asserted here is the run: a model
/// that says it finished over a fixture that is still broken yields
/// `Retryable`, not `Completed`, and records no change set for the next
/// invocation to mistake for its own.
#[tokio::test]
async fn a_model_claiming_success_over_a_broken_fixture_is_disbelieved() {
    let f = broken_fixture();
    let report = f.run(lies(), f.config()).await;

    assert!(
        matches!(report.outcome, RunOutcome::Retryable { .. }),
        "a failed check is retryable, not completed: {:?}",
        report.outcome
    );
    assert!(
        refusal(&report).contains("so nothing was earned"),
        "the check is what decided this: {}",
        refusal(&report)
    );
    assert_eq!(
        f.marker(WORK_ID),
        None,
        "and it must not have recorded a change set"
    );
    failed_once(&report);
}

/// A `..` component is refused by the syntactic half of the path check, and the
/// run that spent its attempt on it earns nothing.
///
/// The tool-level refusal is already proven in `agent/tools.rs`. The addition
/// here is that the refusal is not swallowed on the way out: the model goes on
/// to claim it wrote the file, the check overrules the claim, and the file it
/// named is not on disk.
#[tokio::test]
async fn a_path_escape_is_refused_and_mutates_nothing() {
    let f = broken_fixture();
    // Resolved against the worktree root, whose parent is the workspace root.
    let outside = f.workspace_root().join("escape.txt");

    let report = f.run(writes_to("../escape.txt"), f.config()).await;

    assert_check_refused(&f, &report);
    assert!(
        !outside.exists(),
        "the refusal came after the write: {}",
        outside.display()
    );
}

/// An absolute path is refused by the same parse, under a different rule, and
/// with a target that is nowhere near the workspace.
#[tokio::test]
async fn an_absolute_path_is_refused() {
    let f = broken_fixture();
    let outside = f.dir.path().join("pwned.txt");

    let report = f
        .run(writes_to(&outside.to_string_lossy()), f.config())
        .await;

    assert_check_refused(&f, &report);
    assert!(
        !outside.exists(),
        "an absolute path must never reach the filesystem: {}",
        outside.display()
    );
}

/// The half a parse cannot decide: a syntactically innocent path whose
/// *resolution* leaves the workspace.
///
/// The symlink is committed to the fixture, so the worktree the attempt branches
/// carries it — which is how a real repository would carry one. Only
/// `Workspace::resolve`, which canonicalises before opening anything, can refuse
/// this, and it has to refuse it *before* the write: `std::fs::write` follows a
/// link and creates the file at the far end.
#[tokio::test]
async fn a_symlink_out_of_the_workspace_is_refused() {
    let f = broken_fixture();
    let outside = f.plant_escaping_symlink();

    let report = f.run(writes_to("escape/pwned.txt"), f.config()).await;

    assert_check_refused(&f, &report);
    assert!(
        !outside.join("pwned.txt").exists(),
        "a write followed the link out of the workspace: {}",
        outside.display()
    );
}

/// A tool name that is not in the set is the model saying something false, and
/// the run ends on it having changed nothing.
///
/// `AuditHook` records the call under `unknown_tool`, no tool body runs, and
/// `classify` sorts Rig's `UnknownToolCall` into `Protocol` rather than
/// `Bounded` — the tool set is a bound, but naming a tool outside it is not
/// reaching a limit.
#[tokio::test]
async fn an_unregistered_tool_name_mutates_nothing() {
    let f = broken_fixture();

    let report = f
        .run(
            vec![
                MockTurn::tool_call("c1", "delete_everything", json!({"path": "/"})),
                completion_claim(),
            ],
            f.config(),
        )
        .await;

    assert_retryable_because(&report, "the model called a tool that does not exist");
    assert_earned_nothing(&f, &report);
}

/// A final message that is not the schema fails the run rather than becoming a
/// default-valued report.
///
/// The attempt produced no report at all, so the check is never run: there is
/// nothing for it to be a check *of*. That is the one short-circuit
/// `capability/repair.rs` allows, and it is not the model's claim deciding
/// anything — a bounded or malformed attempt has no claim to decide with.
#[tokio::test]
async fn malformed_structured_output_fails_the_run() {
    let f = broken_fixture();

    let report = f
        .run(vec![MockTurn::text("this is not the schema")], f.config())
        .await;

    assert_retryable_because(&report, "the report did not match the schema");
    assert_earned_nothing(&f, &report);
}

/// The cap is counted against git's changed set, never against the model's own
/// list — so a model that touched two files and claimed none is still refused.
#[tokio::test]
async fn exceeding_the_changed_file_cap_fails_the_run() {
    let f = broken_fixture();
    let mut config = f.config();
    config.budget.max_changed_files = 1;

    let report = f
        .run(
            vec![
                MockTurn::tool_call("c1", "write_file", json!({"path": "a.rs", "contents": "x"})),
                MockTurn::tool_call("c2", "write_file", json!({"path": "b.rs", "contents": "x"})),
                MockTurn::text(
                    r#"{"changed_files":[],"summary":"nothing at all","claimed_complete":true}"#,
                ),
            ],
            config,
        )
        .await;

    assert_retryable_because(&report, "2 files changed, and the cap is 1");
    assert_earned_nothing(&f, &report);
}

/// A run that loops is stopped by the turn budget, and the reason names that
/// bound rather than the deadline or the file cap, both of which are wide open
/// here.
#[tokio::test]
async fn exceeding_the_turn_budget_fails_the_run() {
    let f = broken_fixture();
    let mut config = f.config();
    config.budget.max_turns = 2;

    let report = f
        .run(
            (0..6)
                .map(|i| MockTurn::tool_call(format!("c{i}"), "list_files", json!({})))
                .collect(),
            config,
        )
        .await;

    assert_retryable_because(&report, "the turn budget of 2 was exhausted");
    assert_earned_nothing(&f, &report);
}

/// Cancellation mid-attempt ends the run and leaves nothing behind.
///
/// The check is a long sleep, so the attempt is genuinely in flight when the
/// token is cancelled: this is not a pre-cancelled token being noticed on the
/// way in. The workspace assertion is the one that matters — a cancellation that
/// ended the future without removing the worktree would leak a directory the
/// next attempt's `worktree add` would collide with.
#[tokio::test]
async fn a_cancelled_attempt_leaves_the_fixture_unmutated() {
    let f = broken_fixture();
    let mut config = f.config();
    config.check = WorkspaceCommand {
        program: "sleep".to_string(),
        args: vec!["30".to_string()],
        timeout: Duration::from_secs(60),
    };

    let canceller = config.cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        canceller.cancel();
    });

    let started = std::time::Instant::now();
    let report = f
        .run(
            vec![
                MockTurn::tool_call("c1", "run_check", json!({})),
                completion_claim(),
            ],
            config,
        )
        .await;
    let elapsed = started.elapsed();

    assert_retryable_because(&report, "the attempt was cancelled");
    assert!(
        elapsed >= Duration::from_millis(100) && elapsed < Duration::from_secs(20),
        "cancellation must interrupt the attempt rather than outlive its check: {elapsed:?}"
    );
    assert_earned_nothing(&f, &report);
    assert_eq!(
        f.changed_files(),
        Vec::<String>::new(),
        "the fixture a cancelled attempt branched from must be untouched"
    );
}

/// A refused tool call is a message to the model, not the end of the run.
///
/// The unit test in `tests/agent.rs` proves the model is *told*. What is proven
/// here is the consequence: an attempt whose first call was refused and whose
/// second was a real repair reaches a passing check and completes, marker and
/// all. A shell that treated any refusal as fatal would fail this while passing
/// every other case in the file.
#[tokio::test]
async fn a_tool_error_is_recoverable_within_the_same_attempt() {
    let f = broken_fixture();

    let report = f
        .run(
            vec![
                MockTurn::tool_call(
                    "c1",
                    "write_file",
                    json!({"path": "../nope.rs", "contents": "x"}),
                ),
                MockTurn::tool_call(
                    "c2",
                    "write_file",
                    json!({"path": "src/lib.rs", "contents": fixture::REPAIRED}),
                ),
                MockTurn::text(
                    r#"{"changed_files":["src/lib.rs"],"summary":"recovered","claimed_complete":true}"#,
                ),
            ],
            f.config(),
        )
        .await;

    assert_eq!(
        report.outcome,
        RunOutcome::Completed,
        "a recovered attempt whose check passes is complete: {:?}",
        report.outcome
    );
    assert_eq!(
        f.marker(WORK_ID),
        Some(correlation_key(PROJECT, INVOCATION_REF))
    );
    assert_eq!(report.executions.len(), 1);
    assert_eq!(report.executions[0].status, "completed");
    assert_eq!(
        report.executions[0].evidence[0].0,
        format!("repair:1:{ATTEMPT}"),
        "the refused write must not be counted as a change"
    );
}

// One refusal deliberately has no scenario here: a grant naming *another*
// capability. It is the case that genuinely only makes sense below this level —
// `run` derives the action and hands the capability the grant it derived, so the
// two cannot disagree through the orchestration at all, and the mismatch is only
// expressible by calling `execute` directly. `capability/repair.rs` already
// tests it there, and repeating it here would be a duplicate that proves nothing
// the unit test does not.

// ---------------------------------------------------------------------------
// The world these scenarios run in
// ---------------------------------------------------------------------------

/// A disposable project: a genuinely broken crate as a git repository, a place
/// for per-attempt worktrees, a report directory, and the fixture root both
/// observation ports read.
struct Fixture {
    dir: tempfile::TempDir,
    repo: PathBuf,
}

/// A broken crate plus the stub world an `Execute` derivation needs: one open
/// work item and no change set recorded.
fn broken_fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let repo = fixture::broken_crate(dir.path());
    let f = Fixture { dir, repo };
    std::fs::create_dir_all(f.stub_root().join("work")).unwrap();
    std::fs::create_dir_all(f.stub_root().join("changes")).unwrap();
    std::fs::write(
        f.stub_root().join(format!("work/{WORK_ID}.json")),
        format!(r#"{{"id":"{WORK_ID}","status":"open"}}"#),
    )
    .unwrap();
    f
}

impl Fixture {
    /// Everything the capability needs, with bounds loose enough that a scenario
    /// which is not about a bound never trips one.
    fn config(&self) -> RepairConfig {
        RepairConfig {
            fixture: self.repo.clone(),
            workspace_root: self.workspace_root(),
            stub_root: self.stub_root(),
            project: PROJECT.to_string(),
            attempt: AttemptId(ATTEMPT.to_string()),
            check: WorkspaceCommand {
                program: "cargo".to_string(),
                args: vec!["test".to_string(), "--offline".to_string()],
                timeout: Duration::from_secs(180),
            },
            budget: AgentBudget {
                max_turns: 8,
                max_tokens: 4096,
                deadline: Duration::from_secs(300),
                max_changed_files: 16,
                tool_timeout: Duration::from_secs(180),
            },
            cancel: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// One whole run of the M1 shell over this fixture, driven by `script`.
    ///
    /// The ports, the journal and the capability are built here rather than in
    /// each scenario because none of them is what a scenario varies: the script
    /// and the config are, and they are the two arguments.
    async fn run(&self, script: Vec<MockTurn>, config: RepairConfig) -> RunReport {
        let capability = FixtureRepair::new(MockCompletionModel::new(script), config);
        let journal = FileJournal::new(
            &self.report_dir(),
            SLUG,
            &AttemptId(ATTEMPT.to_string()),
            INVOCATION_REF,
        );
        orchestration::run(&RunContext {
            project: PROJECT,
            invocation_ref: INVOCATION_REF,
            work_id: WORK_ID,
            work_items: &StubWorkItemPort::new(self.stub_root()),
            changes: &StubChangePort::new(self.stub_root()),
            capability: &capability,
            journal: &journal,
        })
        .await
    }

    /// Commit a symlink pointing out of the repository, so the worktree branched
    /// from it carries one too, and return where it points.
    fn plant_escaping_symlink(&self) -> PathBuf {
        let outside = self.dir.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, self.repo.join("escape")).unwrap();
        fixture::git(&self.repo, &["add", "-A"]);
        fixture::git(
            &self.repo,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "a symlink out of the tree",
            ],
        );
        // Git has to have recorded it as a *link* — mode 120000 — or the
        // worktree would check out an ordinary directory and the scenario would
        // pass by refusing something that was never a symlink escape at all.
        let listed = std::process::Command::new("git")
            .args(["ls-files", "-s", "escape"])
            .current_dir(&self.repo)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&listed.stdout).starts_with("120000"),
            "the fixture committed something that is not a symlink: {}",
            String::from_utf8_lossy(&listed.stdout)
        );
        outside
    }

    fn stub_root(&self) -> PathBuf {
        self.dir.path().join("stub-state")
    }

    fn workspace_root(&self) -> PathBuf {
        self.dir.path().join("workspaces")
    }

    fn report_dir(&self) -> PathBuf {
        self.dir.path().join("reports")
    }

    /// The correlation marker recorded for `work_id`, read the way the change
    /// port reads it, or `None` when nothing wrote one.
    fn marker(&self, work_id: &str) -> Option<String> {
        let path = self.stub_root().join(format!("changes/{work_id}.json"));
        let text = std::fs::read_to_string(path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        value["marker"].as_str().map(str::to_string)
    }

    /// Whether the *fixture repository* passes the check — which after an M1
    /// repair it does not, the repair having lived in a worktree that is gone.
    fn check_passes(&self) -> bool {
        fixture::check(&self.repo).success()
    }

    /// What the fixture repository reports as changed. Empty is the assertion:
    /// an attempt writes to its worktree and never to the tree it branched from.
    fn changed_files(&self) -> Vec<String> {
        fixture::changed_files(&self.repo)
    }
}

// ---------------------------------------------------------------------------
// Scripts
// ---------------------------------------------------------------------------

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
fn lies() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call("c1", "read_file", json!({"path": "src/lib.rs"})),
        completion_claim(),
    ]
}

/// The model writes to `path` and then claims it finished, whatever happened.
///
/// The claim is the point: every escape scenario has the model insist it wrote
/// the file, so what refuses it is visibly the check and the filesystem rather
/// than the model's own account.
fn writes_to(path: &str) -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call(
            "c1",
            "write_file",
            json!({"path": path, "contents": "pwned"}),
        ),
        completion_claim(),
    ]
}

/// A well-formed final report claiming completion and no changes.
fn completion_claim() -> MockTurn {
    MockTurn::text(r#"{"changed_files":[],"summary":"all good","claimed_complete":true}"#)
}

fn grant() -> ExecutionGrant {
    ExecutionGrant::authorise(&NextAction::Execute {
        capability_id: FIXTURE_REPAIR,
    })
    .expect("an Execute derivation authorises")
}

// ---------------------------------------------------------------------------
// Assertions
// ---------------------------------------------------------------------------

/// The reason a retryable run gave, or a panic naming what it said instead.
///
/// Asserted against rather than a matched enum variant on purpose: `run` renders
/// the capability's failure with `Display`, so this is the exact text an
/// operator is shown, and each expectation below is a fragment of exactly one
/// error's message — `CheckFailed`, `AgentError::Bounded`,
/// `AgentError::Protocol` and `AgentError::Cancelled` are told apart by it.
fn refusal(report: &RunReport) -> String {
    match &report.outcome {
        RunOutcome::Retryable { reason } => reason.clone(),
        other => panic!("expected a retryable run, got {other:?}"),
    }
}

/// The run was retryable for the named reason, and for no other.
fn assert_retryable_because(report: &RunReport, expected: &str) {
    let reason = refusal(report);
    assert!(
        reason.contains(expected),
        "the wrong failure fired: expected {expected:?}, got {reason:?}"
    );
}

/// Exactly one execution, recorded as having failed — and still saying what it
/// did before it failed.
///
/// # What this assertion used to say, and why that was the bug
///
/// It used to require `evidence.is_empty()`, on the reasoning that "a failed
/// execution has nothing to point at". That reasoning was wrong, and it was
/// wrong in the direction that hides things: an execution which failed is
/// exactly when an operator most needs to know what it *did*. Worse, the
/// assertion actively defended the gap — a change that started publishing tool
/// receipts on the failing arm would have been failed by this suite as a
/// regression.
///
/// What the gap concealed: a repair capability calling **no tools at all**, for
/// every model on the gateway, surfacing as an ordinary failed check that
/// nothing outside the process could tell apart from a model that tried and
/// lost. `tools:0` is now a thing a bundle can say, and this is the assertion
/// that requires it to be said one way or the other.
fn failed_once(report: &RunReport) {
    assert_eq!(report.executions.len(), 1, "{:?}", report.executions);
    assert_eq!(report.executions[0].capability_id, FIXTURE_REPAIR);
    assert_eq!(report.executions[0].status, "failed");
    let evidence = &report.executions[0].evidence;
    assert!(
        evidence.iter().any(|e| e.0.starts_with("tools:")),
        "a failed execution must still say what its tools did, even when the \
         answer is `tools:0`: {evidence:?}"
    );
}

/// Nothing was earned and nothing survived: no marker, one failed execution, and
/// an empty workspace root.
fn assert_earned_nothing(f: &Fixture, report: &RunReport) {
    assert_eq!(
        f.marker(WORK_ID),
        None,
        "a repair that did not pass its check must leave no correlation marker"
    );
    failed_once(report);
    assert_no_workspace_survived(f);
}

/// The specific shape every path-refusal scenario ends in: the tool declined,
/// the model claimed completion anyway, and the check the shell ran itself
/// overruled it.
fn assert_check_refused(f: &Fixture, report: &RunReport) {
    assert_retryable_because(report, "so nothing was earned");
    assert!(
        refusal(report).contains("the model claimed completion: true"),
        "the claim is carried as evidence beside the exit code that overruled it: {}",
        refusal(report)
    );
    assert_earned_nothing(f, report);
}

/// The workspace root exists and holds nothing.
///
/// The existence half is not decoration: "empty" over a directory that was never
/// created is the vacuous truth of an attempt that never prepared a workspace,
/// which would satisfy every teardown assertion in this file for the wrong
/// reason.
fn assert_no_workspace_survived(f: &Fixture) {
    assert!(
        f.workspace_root().exists(),
        "the attempt never prepared a workspace, so nothing about teardown was proven"
    );
    let leftovers: Vec<_> = std::fs::read_dir(f.workspace_root())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "the attempt left a workspace behind: {leftovers:?}"
    );
}
