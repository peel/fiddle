mod fixture;

use fiddle_runtime::agent::{attempt, AgentBudget, AgentError, Direction, ToolHost, ToolReceipts};
use fiddle_runtime::core::AttemptId;
use fiddle_runtime::workspace::{DeclaredCommand, Workspace, WorkspaceCommand};
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const REPAIRED: &str = "pub fn f() -> u8 { 1 }\n";

fn test_host() -> (ToolHost, tempfile::TempDir) {
    test_host_declaring(Vec::new())
}

fn test_host_declaring(commands: Vec<DeclaredCommand>) -> (ToolHost, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let repo = fixture::trivial_repo(dir.path());
    let cancel = CancellationToken::new();
    let workspace = Workspace::create(
        &repo,
        &dir.path().join("ws"),
        &AttemptId("01JQZX0000000000000000000".to_string()),
        cancel.clone(),
    )
    .expect("a workspace");

    let host = ToolHost {
        workspace: Arc::new(workspace),
        cancel,
        check: WorkspaceCommand {
            program: "git".to_string(),
            args: vec!["rev-parse".to_string(), "--is-inside-work-tree".to_string()],
            timeout: Duration::from_secs(30),
        },
        commands: Arc::new(commands),
        command_timeout: Duration::from_secs(30),
        receipts: Arc::new(Mutex::new(ToolReceipts::default())),
    };
    (host, dir)
}

fn budget() -> AgentBudget {
    AgentBudget {
        max_turns: 8,
        max_tokens: 4096,
        deadline: Duration::from_secs(60),
        max_changed_files: 16,
        tool_timeout: Duration::from_secs(60),
    }
}

fn report_turn(summary: &str, complete: bool) -> MockTurn {
    MockTurn::text(
        json!({"changed_files": [], "summary": summary, "claimed_complete": complete}).to_string(),
    )
}

#[tokio::test]
async fn a_scripted_model_drives_the_real_tools() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([
        MockTurn::tool_call("c1", "read_file", json!({"path": "src/lib.rs"})),
        MockTurn::tool_call(
            "c2",
            "write_file",
            json!({"path": "src/lib.rs", "contents": REPAIRED}),
        ),
        MockTurn::text(
            r#"{"changed_files":["src/lib.rs"],"summary":"fixed","claimed_complete":true}"#,
        ),
    ]);

    let report = attempt(model, host.clone(), budget(), Direction::Fresh)
        .await
        .expect("the attempt completes");

    assert!(report.claimed_complete);
    assert_eq!(
        host.workspace.changed_files().unwrap().len(),
        1,
        "the tools must have mutated the real workspace, not a transcript"
    );
    assert_eq!(
        std::fs::read_to_string(host.workspace.root().join("src/lib.rs")).unwrap(),
        REPAIRED
    );
}

#[tokio::test]
async fn the_turn_budget_is_enforced_by_the_runtime() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new(
        (0..6)
            .map(|i| MockTurn::tool_call(format!("c{i}"), "list_files", json!({})))
            .collect::<Vec<_>>(),
    );

    let outcome = attempt(
        model,
        host,
        AgentBudget {
            max_turns: 2,
            ..budget()
        },
        Direction::Fresh,
    )
    .await;

    match outcome {
        Err(AgentError::Bounded { reason }) => assert!(
            reason.contains("turn budget of 2"),
            "the wrong bound fired: {reason}"
        ),
        other => panic!("a run that outran its turn budget must be Bounded: {other:?}"),
    }
}

#[tokio::test]
async fn exceeding_the_changed_file_cap_fails_the_attempt() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([
        MockTurn::tool_call("c1", "write_file", json!({"path": "a.rs", "contents": "x"})),
        MockTurn::tool_call("c2", "write_file", json!({"path": "b.rs", "contents": "x"})),
        MockTurn::text(r#"{"changed_files":[],"summary":"","claimed_complete":true}"#),
    ]);

    let outcome = attempt(
        model,
        host,
        AgentBudget {
            max_changed_files: 1,
            ..budget()
        },
        Direction::Fresh,
    )
    .await;

    match outcome {
        Err(AgentError::Bounded { reason }) => assert!(
            reason.contains("2 files changed") && reason.contains("cap is 1"),
            "the model CLAIMED zero changed files; the cap must count git's: {reason}"
        ),
        other => panic!("the changed-file cap must fire: {other:?}"),
    }
}

#[tokio::test]
async fn an_ignore_rule_the_model_wrote_cannot_lift_the_changed_file_cap() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([
        MockTurn::tool_call(
            "c1",
            "write_file",
            json!({"path": ".gitignore", "contents": "*\n"}),
        ),
        MockTurn::tool_call("c2", "write_file", json!({"path": "a.rs", "contents": "x"})),
        MockTurn::tool_call("c3", "write_file", json!({"path": "b.rs", "contents": "x"})),
        MockTurn::tool_call("c4", "write_file", json!({"path": "c.rs", "contents": "x"})),
        MockTurn::text(r#"{"changed_files":[],"summary":"","claimed_complete":true}"#),
    ]);

    let outcome = attempt(
        model,
        host.clone(),
        AgentBudget {
            max_changed_files: 2,
            ..budget()
        },
        Direction::Fresh,
    )
    .await;

    match outcome {
        Err(AgentError::Bounded { reason }) => assert!(
            reason.contains("4 files changed") && reason.contains("cap is 2"),
            "the model wrote the ignore rule; it must not have written the count: {reason}"
        ),
        other => panic!("the changed-file cap must fire on the true count: {other:?}"),
    }
    assert_eq!(
        host.workspace.changed_files().unwrap().len(),
        4,
        "the ignore rule and the three files it was written to hide"
    );
}

#[tokio::test]
async fn malformed_structured_output_is_a_protocol_error_not_a_default() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([MockTurn::text("this is not the schema")]);

    let outcome = attempt(model, host, budget(), Direction::Fresh).await;

    assert!(
        matches!(outcome, Err(AgentError::Protocol { .. })),
        "a report that does not parse must never become a default-valued one: {outcome:?}"
    );
}

#[tokio::test]
async fn a_tool_error_is_returned_to_the_model_which_can_recover() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([
        MockTurn::tool_call("c1", "read_file", json!({"path": "../nope"})),
        MockTurn::tool_call("c2", "read_file", json!({"path": "src/lib.rs"})),
        report_turn("recovered", false),
    ]);

    let report = attempt(model, host.clone(), budget(), Direction::Fresh)
        .await
        .expect("a refused tool call does not end the run");

    assert_eq!(report.summary, "recovered");
    assert!(!report.claimed_complete);

    let receipts = host.receipts();
    assert_eq!(
        receipts
            .calls
            .iter()
            .map(|call| call.outcome)
            .collect::<Vec<_>>(),
        vec!["refused", "ok"],
        "the model was told its first call was refused and issued a second: {receipts:?}"
    );
}

#[tokio::test]
async fn a_provider_fault_is_told_apart_from_a_misbehaving_model() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([MockTurn::tool_call("c1", "list_files", json!({}))]);

    let outcome = attempt(model, host, budget(), Direction::Fresh).await;

    assert!(
        matches!(outcome, Err(AgentError::Provider { .. })),
        "a completion that never arrived is the gateway's fault: {outcome:?}"
    );
}

#[tokio::test]
async fn cancelling_mid_attempt_stops_the_attempt_rather_than_waiting_for_it() {
    let (mut host, _g) = test_host();
    host.check = WorkspaceCommand {
        program: "sleep".to_string(),
        args: vec!["30".to_string()],
        timeout: Duration::from_secs(60),
    };
    let model = MockCompletionModel::new([
        MockTurn::tool_call("c1", "run_check", json!({})),
        report_turn("unreachable", true),
    ]);

    let canceller = host.cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        canceller.cancel();
    });

    let started = Instant::now();
    let outcome = attempt(model, host, budget(), Direction::Fresh).await;
    let elapsed = started.elapsed();

    assert!(
        matches!(outcome, Err(AgentError::Cancelled)),
        "a cancelled attempt must never be reported as anything else: {outcome:?}"
    );
    assert!(
        elapsed >= Duration::from_millis(100),
        "the attempt ended before the token was cancelled, so nothing mid-flight was tested"
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "cancellation must end the attempt, not wait out the check it was running"
    );
}

#[tokio::test]
async fn the_deadline_bounds_an_attempt_that_would_otherwise_run_on() {
    let (mut host, _g) = test_host();
    host.check = WorkspaceCommand {
        program: "sleep".to_string(),
        args: vec!["30".to_string()],
        timeout: Duration::from_secs(60),
    };
    let model = MockCompletionModel::new([
        MockTurn::tool_call("c1", "run_check", json!({})),
        report_turn("unreachable", true),
    ]);

    let started = Instant::now();
    let outcome = attempt(
        model,
        host,
        AgentBudget {
            deadline: Duration::from_millis(200),
            ..budget()
        },
        Direction::Fresh,
    )
    .await;

    let elapsed = started.elapsed();
    match outcome {
        Err(AgentError::Bounded { reason }) => assert!(
            reason.contains("deadline"),
            "the wrong bound fired: {reason}"
        ),
        other => panic!("an attempt that outran the wall clock is Bounded: {other:?}"),
    }
    assert!(
        elapsed >= Duration::from_millis(200) && elapsed < Duration::from_secs(10),
        "the deadline must interrupt the attempt, not report on it afterwards: {elapsed:?}"
    );
}

#[tokio::test]
async fn the_budgets_tool_timeout_bounds_a_single_tool_without_ending_the_run() {
    let (mut host, _g) = test_host();
    host.check = WorkspaceCommand {
        program: "sleep".to_string(),
        args: vec!["30".to_string()],
        timeout: Duration::from_secs(60),
    };
    let model = MockCompletionModel::new([
        MockTurn::tool_call("c1", "run_check", json!({})),
        report_turn("the check did not finish", false),
    ]);

    let started = Instant::now();
    let report = attempt(
        model,
        host.clone(),
        AgentBudget {
            tool_timeout: Duration::from_millis(100),
            ..budget()
        },
        Direction::Fresh,
    )
    .await
    .expect("one tool outrunning its bound is not the whole attempt failing");

    assert_eq!(report.summary, "the check did not finish");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the budget's tool timeout did not tighten the host's own"
    );
    let receipts = host.receipts();
    assert_eq!(receipts.calls.len(), 1, "{receipts:?}");
    assert_eq!(
        receipts.calls[0].outcome, "failed",
        "a tool the host's bound killed is a failure, not a cancellation: {receipts:?}"
    );
}
