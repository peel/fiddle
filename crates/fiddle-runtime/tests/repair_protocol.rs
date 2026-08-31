mod fixture;

use fiddle_runtime::agent::AgentBudget;
use fiddle_runtime::capability::{Capability, ExecutionGrant, FixtureRepair, RepairConfig};
use fiddle_runtime::core::{correlation_key, AttemptId, NextAction, RunOutcome, FIXTURE_REPAIR};
use fiddle_runtime::journal::FileJournal;
use fiddle_runtime::orchestration::{self, Addressed, RunContext, RunReport};
use fiddle_runtime::workspace::WorkspaceCommand;
use fiddle_runtime::Redaction;
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

#[tokio::test]
async fn the_published_count_is_gits_however_the_attempt_wrote_the_ignore_rules() {
    let f = broken_fixture();
    let report = f
        .run(
            vec![
                MockTurn::tool_call(
                    "c1",
                    "write_file",
                    json!({"path": "src/lib.rs", "contents": fixture::REPAIRED}),
                ),
                MockTurn::tool_call(
                    "c2",
                    "write_file",
                    json!({"path": ".gitignore", "contents": "*\n"}),
                ),
                MockTurn::tool_call(
                    "c3",
                    "write_file",
                    json!({"path": "decoy.rs", "contents": "// unasked for\n"}),
                ),
                MockTurn::text(
                    r#"{"changed_files":["src/lib.rs"],"summary":"one file","claimed_complete":true}"#,
                ),
            ],
            f.config(),
        )
        .await;

    assert_eq!(
        report.outcome,
        RunOutcome::Completed,
        "the repair is real, so the check passes: {:?}",
        report.outcome
    );
    assert!(
        evidence_of(&report).contains(&format!("repair:3:{ATTEMPT}")),
        "the source, the ignore rule and the file it was written to hide are three \
         changes: {:?}",
        evidence_of(&report)
    );
}

fn evidence_of(report: &RunReport) -> Vec<String> {
    assert_eq!(report.executions.len(), 1, "{:?}", report.executions);
    report.executions[0]
        .evidence
        .iter()
        .map(|reference| reference.0.clone())
        .collect()
}

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

#[tokio::test]
async fn a_path_escape_is_refused_and_mutates_nothing() {
    let f = broken_fixture();
    let outside = f.workspace_root().join("escape.txt");

    let report = f.run(writes_to("../escape.txt"), f.config()).await;

    assert_check_refused(&f, &report);
    assert!(
        !outside.exists(),
        "the refusal came after the write: {}",
        outside.display()
    );
}

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

    assert_retryable_because(&report, "the model called the tool delete_everything");
    assert_retryable_because(&report, "run_check");
    assert_earned_nothing(&f, &report);
}

#[tokio::test]
async fn malformed_structured_output_fails_the_run() {
    let f = broken_fixture();

    let report = f
        .run(vec![MockTurn::text("this is not the schema")], f.config())
        .await;

    assert_retryable_because(&report, "the report did not match the schema");
    assert_earned_nothing(&f, &report);
}

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

struct Fixture {
    dir: tempfile::TempDir,
    repo: PathBuf,
}

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
    fn config(&self) -> RepairConfig {
        RepairConfig {
            fixture: self.repo.clone(),
            workspace_root: self.workspace_root(),
            stub_root: self.stub_root(),
            project: PROJECT.to_string(),
            check: WorkspaceCommand {
                program: "cargo".to_string(),
                args: vec!["test".to_string(), "--offline".to_string()],
                timeout: Duration::from_secs(180),
            },
            commands: std::sync::Arc::new(Vec::new()),
            command_timeout: Duration::from_secs(180),
            budget: AgentBudget {
                max_turns: 8,
                max_tokens: 4096,
                deadline: Duration::from_secs(300),
                max_changed_files: 16,
                tool_timeout: Duration::from_secs(180),
            },
            redaction: Redaction::unknown(),
            transcripts: None,
            cancel: tokio_util::sync::CancellationToken::new(),
        }
    }

    async fn run(&self, script: Vec<MockTurn>, config: RepairConfig) -> RunReport {
        let capability = FixtureRepair::new(MockCompletionModel::new(script), config);
        let attempt = AttemptId(ATTEMPT.to_string());
        let journal = FileJournal::new(&self.report_dir(), SLUG, &attempt, INVOCATION_REF);
        orchestration::run(&RunContext {
            project: PROJECT,
            invocation_ref: INVOCATION_REF,
            addressed: Addressed::WorkItem(WORK_ID),
            attempt: &attempt,
            work_items: &StubWorkItemPort::new(self.stub_root()),
            changes: &StubChangePort::new(self.stub_root()),
            capability: &capability,
            journal: &journal,
            cancel: &tokio_util::sync::CancellationToken::new(),
        })
        .await
    }

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

    fn marker(&self, work_id: &str) -> Option<String> {
        let path = self.stub_root().join(format!("changes/{work_id}.json"));
        let text = std::fs::read_to_string(path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        value["marker"].as_str().map(str::to_string)
    }

    fn check_passes(&self) -> bool {
        fixture::check(&self.repo).success()
    }

    fn changed_files(&self) -> Vec<String> {
        fixture::changed_files(&self.repo)
    }
}

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

fn lies() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call("c1", "read_file", json!({"path": "src/lib.rs"})),
        completion_claim(),
    ]
}

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

fn completion_claim() -> MockTurn {
    MockTurn::text(r#"{"changed_files":[],"summary":"all good","claimed_complete":true}"#)
}

fn grant() -> ExecutionGrant {
    ExecutionGrant::authorise(
        &NextAction::Execute {
            capability_id: FIXTURE_REPAIR,
        },
        &AttemptId(ATTEMPT.to_string()),
    )
    .expect("an Execute derivation authorises")
}

fn refusal(report: &RunReport) -> String {
    match &report.outcome {
        RunOutcome::Retryable { reason } => reason.to_string(),
        other => panic!("expected a retryable run, got {other:?}"),
    }
}

fn assert_retryable_because(report: &RunReport, expected: &str) {
    let reason = refusal(report);
    assert!(
        reason.contains(expected),
        "the wrong failure fired: expected {expected:?}, got {reason:?}"
    );
}

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

fn assert_earned_nothing(f: &Fixture, report: &RunReport) {
    assert_eq!(
        f.marker(WORK_ID),
        None,
        "a repair that did not pass its check must leave no correlation marker"
    );
    failed_once(report);
    assert_no_workspace_survived(f);
}

fn assert_check_refused(f: &Fixture, report: &RunReport) {
    assert_retryable_because(report, "so nothing was earned");
    assert!(
        refusal(report).contains("the model claimed completion: true"),
        "the claim is carried as evidence beside the exit code that overruled it: {}",
        refusal(report)
    );
    assert_earned_nothing(f, report);
}

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
