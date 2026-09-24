mod fixture;

use fiddle_runtime::agent::{
    attempt, judge_briefed, AgentBudget, AgentError, Brief, Direction, RepairReport, ToolHost,
    ToolReceipts, Verdict,
};
use fiddle_runtime::core::AttemptId;
use fiddle_runtime::workspace::{DeclaredCommand, Workspace, WorkspaceCommand};
use fiddle_runtime::Redaction;
use rig_core::completion::message::ToolChoice;
use rig_core::completion::{
    CompletionError, CompletionModel, CompletionRequest, CompletionResponse,
};
use rig_core::streaming::StreamingCompletionResponse;
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

const REPAIRED: &str = "pub fn f() -> u8 { 1 }\n";

const RECORDED_ENVELOPE: &str =
    include_str!("../../../tests/fixtures/gateway-real/repair-report-answer.json");

const RECORDED_STRING: &str =
    include_str!("../../../tests/fixtures/gateway-real/repair-report-string.json");

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

fn redaction() -> Redaction {
    Redaction::of("sk-mock-must-not-appear-0d1e")
}

fn budget() -> AgentBudget {
    AgentBudget {
        max_turns: 8,
        max_tokens: 4096,
        max_tokens_total: None,
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

    let report = attempt(
        model,
        &redaction(),
        host.clone(),
        budget(),
        Direction::Fresh,
        None,
    )
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
        &redaction(),
        host,
        AgentBudget {
            max_turns: 2,
            ..budget()
        },
        Direction::Fresh,
        None,
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
        &redaction(),
        host,
        AgentBudget {
            max_changed_files: 1,
            ..budget()
        },
        Direction::Fresh,
        None,
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
        &redaction(),
        host.clone(),
        AgentBudget {
            max_changed_files: 2,
            ..budget()
        },
        Direction::Fresh,
        None,
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
async fn the_report_a_real_gateway_enveloped_completes_the_attempt() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([MockTurn::text(RECORDED_ENVELOPE)]);

    let report = attempt(model, &redaction(), host, budget(), Direction::Fresh, None)
        .await
        .expect("the report of 2026-09-03, envelope and all, is one this build reads");

    assert_eq!(
        report.changed_files,
        ["pkg/service/batch_processor.go"],
        "the whole attempt, and not only the parse, has to carry the enveloped report through"
    );
    assert!(
        report.claimed_complete,
        "and the completion that report claimed survives the envelope. This lane is where \
         the recorded value is read, because `nothing_in_this_workspace_decides_on_claimed_complete` \
         refuses every read of the field under `src` that is not a plain recording, and an \
         assertion is not one"
    );
    assert!(
        report
            .summary
            .starts_with("Implemented Option A from the ticket:"),
        "and so does the summary: {}",
        report.summary
    );
    assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
    assert_eq!(report.quoted_from_a_comment, None);
}

#[tokio::test]
async fn the_report_a_real_gateway_double_encoded_completes_the_attempt() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([MockTurn::text(RECORDED_STRING)]);

    let report = attempt(model, &redaction(), host, budget(), Direction::Fresh, None)
        .await
        .expect(
            "the report of 2026-09-03, envelope and JSON string and all, is one this build reads",
        );

    assert_eq!(
        report.changed_files,
        ["pkg/service/batch_processor.go"],
        "the whole attempt, and not only the parse, has to carry the double-encoded report \
         through"
    );
    assert!(
        report.claimed_complete,
        "and the completion that report claimed survives both layers. This lane is where \
         the recorded value is read, because `nothing_in_this_workspace_decides_on_claimed_complete` \
         refuses every read of the field under `src` that is not a plain recording, and an \
         assertion is not one"
    );
    assert_eq!(
        report.quoted_from_a_comment.as_deref(),
        Some(
            "Option B. More-reliable long-term. The bare metrics should be still \
             type-compatible as described."
        ),
        "and so does the comment the agent quoted, which is the field this recorded run \
         carries and the enveloped one does not: {:?}",
        report.quoted_from_a_comment
    );
    assert!(
        report
            .summary
            .starts_with("The ticket's description offered Option A"),
        "and so does the summary: {}",
        report.summary
    );
    assert!(report.findings.is_empty(), "{:?}", report.findings);
}

#[tokio::test]
async fn malformed_structured_output_is_a_protocol_error_not_a_default() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new(
        (0..=fiddle_runtime::agent::RETURNS)
            .map(|_| MockTurn::text("this is not the schema"))
            .collect::<Vec<_>>(),
    );

    let outcome = attempt(model, &redaction(), host, budget(), Direction::Fresh, None).await;

    assert!(
        matches!(outcome, Err(AgentError::Protocol { .. })),
        "a report that does not parse must never become a default-valued one: {outcome:?}"
    );
}

#[tokio::test]
async fn a_report_beside_prose_is_refused_on_every_turn_and_never_read_off_the_front() {
    let (host, _g) = test_host();
    let beside_prose = format!(
        "Here is my report, with the check passing:\n{RECORDED_ENVELOPE}\nLet me know if you \
         need anything else."
    );
    let model = MockCompletionModel::new(
        (0..=fiddle_runtime::agent::RETURNS)
            .map(|_| MockTurn::text(&beside_prose))
            .collect::<Vec<_>>(),
    );

    let refused = attempt(model, &redaction(), host, budget(), Direction::Fresh, None).await;

    let Err(AgentError::Protocol { reason }) = &refused else {
        panic!(
            "a report beside prose is not a report. A reader that finds the first value it can \
             parse in the text is choosing among candidates, which is the line the fifth \
             wrapping shape drew, and rig's typed fallback does exactly that. It returned: \
             {refused:?}"
        );
    };
    assert!(
        reason.starts_with("the report did not match the schema:")
            && reason.contains(&format!(
                "after {} of its turns were returned",
                fiddle_runtime::agent::RETURNS
            )),
        "the same answer was returned twice and then refused, and the refusal says so: {reason}"
    );
}

#[tokio::test]
async fn a_repair_that_answers_prose_is_returned_to_the_shape_and_reports() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([
        MockTurn::text("All done, the off-by-one is fixed and the check passes."),
        MockTurn::text(RECORDED_ENVELOPE),
    ]);

    let report = attempt(model, &redaction(), host, budget(), Direction::Fresh, None)
        .await
        .expect("prose is returned once and the report on the next turn is read");

    assert_eq!(report.changed_files, ["pkg/service/batch_processor.go"]);
}

#[tokio::test]
async fn a_tool_error_is_returned_to_the_model_which_can_recover() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([
        MockTurn::tool_call("c1", "read_file", json!({"path": "../nope"})),
        MockTurn::tool_call("c2", "read_file", json!({"path": "src/lib.rs"})),
        report_turn("recovered", false),
    ]);

    let report = attempt(
        model,
        &redaction(),
        host.clone(),
        budget(),
        Direction::Fresh,
        None,
    )
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

    let outcome = attempt(model, &redaction(), host, budget(), Direction::Fresh, None).await;

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
    let outcome = attempt(model, &redaction(), host, budget(), Direction::Fresh, None).await;
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
        &redaction(),
        host,
        AgentBudget {
            deadline: Duration::from_millis(200),
            ..budget()
        },
        Direction::Fresh,
        None,
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
        &redaction(),
        host.clone(),
        AgentBudget {
            tool_timeout: Duration::from_millis(100),
            ..budget()
        },
        Direction::Fresh,
        None,
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

const JUDGE_PREAMBLE_FOR_TESTS: &str = "You are judging one change.";

const COMBINERS: [&str; 3] = ["oneOf", "allOf", "anyOf"];

fn schemas_of(model: &MockCompletionModel) -> Vec<(String, serde_json::Value)> {
    let requests = model.requests();
    assert!(
        !requests.is_empty(),
        "no request reached the model, so the schemas below are the schemas of nothing"
    );
    requests[0]
        .tools
        .iter()
        .map(|tool| (tool.name.clone(), tool.parameters.clone()))
        .collect()
}

fn combiners_at_the_top_of(schemas: &[(String, serde_json::Value)]) -> Vec<String> {
    schemas
        .iter()
        .flat_map(|(name, schema)| {
            COMBINERS
                .iter()
                .filter(|combiner| schema.get(*combiner).is_some())
                .map(move |combiner| format!("{name}.input_schema.{combiner}: {schema}"))
        })
        .collect()
}

#[tokio::test]
async fn no_tool_schema_this_build_sends_carries_a_top_level_combiner() {
    let (repairing, _g) = test_host();
    let repairer = MockCompletionModel::new([report_turn("nothing", true)]);
    let _ = attempt(
        repairer.clone(),
        &redaction(),
        repairing,
        budget(),
        Direction::Fresh,
        None,
    )
    .await;

    let (judging, _j) = test_host();
    let judge =
        MockCompletionModel::new([MockTurn::text(json!({"verdict": "accepted"}).to_string())]);
    let _ = judge_briefed(
        judge.clone(),
        &redaction(),
        judging,
        budget(),
        Brief {
            preamble: JUDGE_PREAMBLE_FOR_TESTS,
            task: "Judge this.",
        },
        None,
    )
    .await;

    let structured = [
        (
            "the repairer's report",
            schemars::schema_for!(RepairReport).to_value(),
        ),
        (
            "the judge's verdict",
            schemars::schema_for!(Verdict).to_value(),
        ),
    ];
    let refused = combiners_at_the_top_of(
        &structured
            .iter()
            .map(|(named, schema)| ((*named).to_string(), schema.clone()))
            .collect::<Vec<_>>(),
    );
    assert!(
        refused.is_empty(),
        "the report's schema reaches the provider as `response_format`, and a gateway that \
         fronts Anthropic lifts it into a prepended tool, so a top-level combiner there is \
         refused as `tools.0.custom.input_schema`. The verdict's schema travels in the \
         evaluation's preamble instead, and is held to the same rule so that moving it back \
         onto the wire cannot bring a combiner with it. {} of the two carry one:\n{}",
        refused.len(),
        refused.join("\n")
    );

    for (offered, model, expected) in [
        (
            "the repairer",
            &repairer,
            [
                "edit_file",
                "list_files",
                "read_file",
                "run_check",
                "search_files",
                "write_file",
            ]
            .as_slice(),
        ),
        (
            "the judge",
            &judge,
            ["list_files", "read_file", "search_files"].as_slice(),
        ),
    ] {
        let schemas = schemas_of(model);
        let mut named: Vec<&str> = schemas.iter().map(|(name, _)| name.as_str()).collect();
        named.sort_unstable();
        assert_eq!(
            named, expected,
            "{offered} is sent its own tools and no others. No synthetic output tool is among \
             them: the repairer's `OutputMode::Native` puts the schema on the wire as \
             `response_format`, the judge's `OutputMode::Prompted` puts it in the preamble, \
             and neither advertises a tool. A count-only assertion here would pass whether or \
             not one arrived"
        );
        let offenders = combiners_at_the_top_of(&schemas);
        assert!(
            offenders.is_empty(),
            "Anthropic refuses `oneOf`, `allOf` or `anyOf` at the top of a tool's \
             input_schema, and {offered} is sent {} such schema(s), so this build cannot \
             talk to that provider at all:\n{}",
            offenders.len(),
            offenders.join("\n")
        );
    }
}

const A_VERDICT_THE_GATEWAY_ENVELOPED: &str = r#"{"parameters": {"verdict": "accepted"}}"#;

const A_VERDICT_THE_GATEWAY_DOUBLE_ENCODED: &str =
    r#"{"parameters": "{\"verdict\": \"accepted\"}"}"#;

#[derive(Clone)]
struct ObeysItsToolChoice {
    answer: String,
    seen: Arc<Mutex<Vec<Option<ToolChoice>>>>,
    schemas: Arc<Mutex<Vec<Option<serde_json::Value>>>>,
    preambles: Arc<Mutex<Vec<Option<String>>>>,
}

impl ObeysItsToolChoice {
    fn answering(answer: &str) -> Self {
        ObeysItsToolChoice {
            answer: answer.to_string(),
            seen: Arc::new(Mutex::new(Vec::new())),
            schemas: Arc::new(Mutex::new(Vec::new())),
            preambles: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn choices_it_was_sent(&self) -> Vec<Option<ToolChoice>> {
        self.seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn output_schemas_it_was_sent(&self) -> Vec<Option<serde_json::Value>> {
        self.schemas
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn preambles_it_was_sent(&self) -> Vec<Option<String>> {
        self.preambles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn calls(&self) -> usize {
        self.choices_it_was_sent().len()
    }
}

impl CompletionModel for ObeysItsToolChoice {
    type Response = <MockCompletionModel as CompletionModel>::Response;
    type StreamingResponse = <MockCompletionModel as CompletionModel>::StreamingResponse;
    type Client = ();

    fn make(_: &Self::Client, _: impl Into<String>) -> Self {
        ObeysItsToolChoice::answering("")
    }

    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
        let choice = request.tool_choice.clone();
        self.schemas
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(
                request
                    .output_schema
                    .clone()
                    .map(schemars::Schema::to_value),
            );
        self.preambles
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(
                request
                    .chat_history
                    .iter()
                    .find_map(|message| match message {
                        rig_core::completion::Message::System { content } => Some(content.clone()),
                        _ => None,
                    })
                    .or_else(|| request.preamble.clone()),
            );
        let turn = {
            let mut seen = self
                .seen
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            seen.push(choice.clone());
            seen.len()
        };
        let scripted = match choice {
            Some(ToolChoice::Required) => MockTurn::tool_call(
                format!("read-{turn}"),
                "read_file",
                json!({"path": "src/lib.rs"}),
            ),
            _ => MockTurn::text(&self.answer),
        };
        MockCompletionModel::new([scripted])
            .completion(request)
            .await
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError> {
        MockCompletionModel::new([MockTurn::text(&self.answer)])
            .stream(request)
            .await
    }
}

fn judging() -> Brief<'static> {
    Brief {
        preamble: JUDGE_PREAMBLE_FOR_TESTS,
        task: "Judge this.",
    }
}

fn bounded_at(max_turns: usize) -> AgentBudget {
    AgentBudget {
        max_turns,
        ..budget()
    }
}

const THE_DOCUMENTS_BOUND: usize = 60;

const THE_SHIPPED_DOCUMENT: &str = include_str!("../../../workflows/toil.toml");

#[test]
fn the_evaluations_bound_the_lane_mirrors_is_the_documents() {
    let document: fiddle_runtime::capability::WorkflowFile =
        toml::from_str(THE_SHIPPED_DOCUMENT).expect("the shipped document parses");
    let evaluations: Vec<u32> = document
        .steps
        .iter()
        .filter_map(|step| match step {
            fiddle_runtime::capability::Step::Evaluate { max_turns, .. } => Some(*max_turns),
            _ => None,
        })
        .collect();
    assert_eq!(
        evaluations,
        vec![THE_DOCUMENTS_BOUND as u32],
        "the lanes above bound the evaluation at the number the shipped document gives it, and \
         the two moved apart. On 2026-09-05 the shipped 12 was raised to 60 on the evidence of \
         three live evaluations that took 14, 29 and 24 turns; a lane still saying 12 would be \
         measuring a document nobody ships"
    );
}

#[tokio::test]
async fn the_evaluation_answers_inside_the_bound_the_document_gives_it() {
    let (host, _g) = test_host();
    let gateway = ObeysItsToolChoice::answering(A_VERDICT_THE_GATEWAY_ENVELOPED);

    let verdict = judge_briefed(
        gateway.clone(),
        &redaction(),
        host,
        bounded_at(THE_DOCUMENTS_BOUND),
        judging(),
        None,
    )
    .await
    .unwrap_or_else(|error| {
        panic!(
            "the evaluation has to answer against a gateway that obeys the tool choice fiddle \
             sends, and it spent {} of {THE_DOCUMENTS_BOUND} turns instead: {error}",
            gateway.calls()
        )
    });

    assert_eq!(verdict, Verdict::Accepted {});
    assert!(
        gateway.calls() < THE_DOCUMENTS_BOUND,
        "the fix has to show as termination and not as a larger budget, and this run took all \
         {THE_DOCUMENTS_BOUND} turns the document allows"
    );
    assert_eq!(
        gateway.choices_it_was_sent(),
        vec![Some(ToolChoice::Auto)],
        "the evaluation is read-only, so `required` leaves it no move but to read again. \
         What it is sent, and how many times, is the whole of this lane"
    );
}

const ANSWER_AS_TEXT: &str =
    "Your answer is the text of your final message, and no tool carries it";

#[tokio::test]
async fn a_judge_that_names_its_verdict_as_a_tool_is_returned_to_the_text_and_answers() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([
        MockTurn::tool_call("c1", "verdict", json!({})),
        MockTurn::text(A_VERDICT_THE_GATEWAY_ENVELOPED),
    ]);

    let verdict = judge_briefed(model, &redaction(), host, budget(), judging(), None)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "a call to a tool this run did not offer is returned to the model, not the \
                 end of the attempt. On 2026-09-04 one such call, `verdict` with `{{}}`, ended \
                 a live run after the agent step had finished. This one ended with: {error}"
            )
        });

    assert_eq!(
        verdict,
        Verdict::Accepted {},
        "the verdict that arrives is the one the model wrote as text on its next turn"
    );
}

#[tokio::test]
async fn a_judge_that_keeps_inventing_a_tool_ends_after_the_returns_and_its_arguments_are_never_read(
) {
    let (host, _g) = test_host();
    let turns = fiddle_runtime::agent::RETURNS + 1;
    let model = MockCompletionModel::new(
        (0..turns)
            .map(|at| {
                MockTurn::tool_call(format!("c{at}"), "verdict", json!({"verdict": "accepted"}))
            })
            .collect::<Vec<_>>(),
    );

    let refused = judge_briefed(model, &redaction(), host, budget(), judging(), None).await;

    let Err(AgentError::Protocol { reason }) = &refused else {
        panic!(
            "a call named `verdict` carrying a valid verdict as its arguments is not a \
             verdict, and a run that keeps making it ends: {refused:?}"
        );
    };
    assert!(
        reason.contains("the model called the tool verdict")
            && reason.contains("list_files")
            && reason.contains(&format!(
                "after {} of its turns were returned",
                fiddle_runtime::agent::RETURNS
            ))
            && reason.contains("unoffered_tool"),
        "the reason names the tool, the tools this run offers, the returns spent and the rule \
         the last one failed: {reason}"
    );
}

#[tokio::test]
async fn a_repair_that_names_its_report_as_a_tool_is_returned_to_the_text_and_reports() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([
        MockTurn::tool_call("c1", "report", json!({})),
        MockTurn::text(RECORDED_ENVELOPE),
    ]);

    let report = attempt(model, &redaction(), host, budget(), Direction::Fresh, None)
        .await
        .expect("the repair step is returned to the text the same way the evaluation is");

    assert_eq!(report.changed_files, ["pkg/service/batch_processor.go"]);
}

#[tokio::test]
async fn both_preambles_say_the_answer_is_the_text_and_no_tool_carries_it() {
    let (host, _g) = test_host();
    let judging_gateway = ObeysItsToolChoice::answering(A_VERDICT_THE_GATEWAY_ENVELOPED);
    judge_briefed(
        judging_gateway.clone(),
        &redaction(),
        host,
        budget(),
        Brief {
            preamble: fiddle_runtime::agent::JUDGE_PREAMBLE,
            task: "Judge this.",
        },
        None,
    )
    .await
    .expect("the evaluation answers");
    let (host, _g) = test_host();
    let repairing_gateway = ObeysItsToolChoice::answering(RECORDED_ENVELOPE);
    attempt(
        repairing_gateway.clone(),
        &redaction(),
        host,
        budget(),
        Direction::Fresh,
        None,
    )
    .await
    .expect("the repair answers");

    for (step, gateway) in [
        ("evaluation", judging_gateway),
        ("repair", repairing_gateway),
    ] {
        let preambles = gateway.preambles_it_was_sent();
        let system = preambles
            .first()
            .cloned()
            .flatten()
            .unwrap_or_else(|| panic!("the {step} sends a system message"));
        assert!(
            system.contains(ANSWER_AS_TEXT),
            "the {step}'s preamble, read off the request and not off the constant, has to say \
             where the answer goes: {system}"
        );
    }
}

#[tokio::test]
async fn the_transcript_records_the_return_of_an_invented_tool_under_its_own_rule() {
    let (host, _g) = test_host();
    let dir = tempfile::tempdir().expect("a directory for the transcript");
    let transcripts = fiddle_runtime::agent::transcript::Transcripts::under(dir.path(), "invented");
    let model = MockCompletionModel::new([
        MockTurn::tool_call("c1", "verdict", json!({})),
        MockTurn::text(A_VERDICT_THE_GATEWAY_ENVELOPED),
    ]);

    judge_briefed(
        model,
        &redaction(),
        host,
        budget(),
        judging(),
        Some(&transcripts),
    )
    .await
    .expect("the return lets the evaluation answer on its next turn");

    let returned: Vec<serde_json::Value> = std::fs::read_to_string(transcripts.path())
        .expect("the transcript is on disk")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("one JSON object"))
        .filter(|record| record["record"] == fiddle_runtime::agent::transcript::RETURNED)
        .collect();
    assert_eq!(
        returned.len(),
        1,
        "one invented call is one return, and the transcript holds exactly that many: \
         {returned:?}"
    );
    assert_eq!(returned[0]["rule"], "unoffered_tool", "{:?}", returned[0]);
    assert_eq!(returned[0]["returns"], 1, "{:?}", returned[0]);
    assert!(
        returned[0]["reason"]
            .as_str()
            .is_some_and(|reason| reason.contains("verdict") && reason.contains("list_files")),
        "the record names the tool the model called and the tools this run offers: {:?}",
        returned[0]
    );
}

#[tokio::test]
async fn the_stub_obeys_the_tool_choice_it_is_sent() {
    let gateway = ObeysItsToolChoice::answering(A_VERDICT_THE_GATEWAY_ENVELOPED);

    let obliged = gateway
        .completion_request(rig_core::completion::Message::user("judge this"))
        .tool_choice(ToolChoice::Required)
        .send()
        .await
        .expect("the stub answers every request");
    let free = gateway
        .completion_request(rig_core::completion::Message::user("judge this"))
        .tool_choice(ToolChoice::Auto)
        .send()
        .await
        .expect("the stub answers every request");

    assert!(
        matches!(
            obliged.choice.first(),
            rig_core::completion::AssistantContent::ToolCall(_)
        ),
        "a gateway that obeys `required` calls a tool and does not answer, which is the whole \
         behaviour the two lanes above rest on. It returned {:?}",
        obliged.choice
    );
    assert!(
        matches!(
            free.choice.first(),
            rig_core::completion::AssistantContent::Text(_)
        ),
        "and answers when it is not obliged, so the passing lane above is not passing against \
         a stub that answers whatever it is sent. It returned {:?}",
        free.choice
    );
}

#[tokio::test]
async fn the_repair_step_answers_a_gateway_that_obeys_the_tool_choice_it_is_sent() {
    let (host, _g) = test_host();
    let gateway = ObeysItsToolChoice::answering(RECORDED_ENVELOPE);

    let report = attempt(
        gateway.clone(),
        &redaction(),
        host,
        bounded_at(4),
        Direction::Fresh,
        None,
    )
    .await
    .unwrap_or_else(|error| {
        panic!(
            "the repair step has to answer against a gateway that obeys the tool choice fiddle \
             sends. No output tool is offered on either path, so the report can only be the \
             assistant's final text, which `required` forbade. On 2026-09-04 a gateway that \
             obeyed it spent 24 and then 120 turns reading and wrote nothing. This run spent \
             {} of 4 turns: {error}",
            gateway.calls()
        )
    });

    assert_eq!(
        report.changed_files,
        ["pkg/service/batch_processor.go"],
        "the answer that arrives is the recorded report, envelope and all"
    );
    assert!(
        gateway.calls() < 4,
        "the fix has to show as termination and not as a larger budget, and this run took all \
         4 turns the lane allows"
    );
    assert_eq!(
        gateway.choices_it_was_sent(),
        vec![Some(ToolChoice::Auto)],
        "the repair step permits an answer on every turn it sends, so a gateway that obeys the \
         choice has a way to hand back the report. What it is sent, and how many times, is the \
         whole of this lane"
    );
}

#[tokio::test]
async fn an_answer_that_is_not_a_verdict_is_refused_and_the_refusal_names_the_verdict() {
    for answered in [
        "I read the change and it looks fine to me.",
        r#"{"parameters": {"summary": "an envelope holding no verdict"}}"#,
        r#"{"verdict": "maybe"}"#,
        r#"{"parameters": "{\"summary\": \"a string holding no verdict\"}"}"#,
        r#"{"parameters": "\"{\\\"verdict\\\": \\\"accepted\\\"}\""}"#,
        "Here is my verdict:\n{\"verdict\": \"accepted\"}",
        "```json\n{\"verdict\": \"accepted\"}\n```\nand that is my verdict.",
        "```json\n{\"verdict\": \"accepted\"}\n```\n```json\n{\"verdict\": \"rejected\"}\n```",
        "```json\n```",
    ] {
        let (host, _g) = test_host();
        let refused = judge_briefed(
            MockCompletionModel::new(
                (0..=fiddle_runtime::agent::RETURNS)
                    .map(|_| MockTurn::text(answered))
                    .collect::<Vec<_>>(),
            ),
            &redaction(),
            host,
            budget(),
            judging(),
            None,
        )
        .await;

        let Err(AgentError::Protocol { reason }) = &refused else {
            panic!("an answer that is not a verdict is never a verdict: {answered} -> {refused:?}");
        };
        assert!(
            reason.starts_with("the verdict did not match the schema:"),
            "the refusal has to name what could not be read, and this one says {reason:?} of \
             {answered:?}"
        );
        assert!(
            reason.contains(&format!(
                "after {} of its turns were returned; the last return failed the unreadable_answer rule",
                fiddle_runtime::agent::RETURNS
            )),
            "the same answer was returned to the model {} times before the refusal stood, and the \
             refusal says so: {reason:?} of {answered:?}",
            fiddle_runtime::agent::RETURNS
        );
    }
}

#[tokio::test]
async fn a_judge_that_answers_prose_is_returned_to_the_shape_and_answers() {
    let (host, _g) = test_host();
    let dir = tempfile::tempdir().expect("a directory for the transcript");
    let transcripts = fiddle_runtime::agent::transcript::Transcripts::under(dir.path(), "prose");
    let model = MockCompletionModel::new([
        MockTurn::text(
            "Everything checks out. The change renamed the metric and left the rest alone, \
             matching exactly what the ticket's binding decision comments required.",
        ),
        MockTurn::text(A_VERDICT_THE_GATEWAY_ENVELOPED),
    ]);

    let verdict = judge_briefed(
        model,
        &redaction(),
        host,
        budget(),
        judging(),
        Some(&transcripts),
    )
    .await
    .unwrap_or_else(|error| {
        panic!(
            "prose is not a verdict, and on 2026-09-04 one such answer ended a live run after the \
             agent step had finished. It is returned to the model instead: {error}"
        )
    });

    assert_eq!(verdict, Verdict::Accepted {});
    let returned: Vec<serde_json::Value> = std::fs::read_to_string(transcripts.path())
        .expect("the transcript is on disk")
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("one JSON object"))
        .filter(|record| record["record"] == fiddle_runtime::agent::transcript::RETURNED)
        .collect();
    assert_eq!(
        returned.len(),
        1,
        "one prose answer is one return: {returned:?}"
    );
    assert_eq!(
        returned[0]["rule"], "unreadable_answer",
        "{:?}",
        returned[0]
    );
    assert!(
        returned[0]["reason"]
            .as_str()
            .is_some_and(|reason| reason.starts_with("the verdict did not match the schema:")),
        "the record carries the refusal the model was shown: {:?}",
        returned[0]
    );
}

#[tokio::test]
async fn the_verdict_a_gateway_envelopes_is_read_and_a_bare_one_still_reads() {
    for (answered, expected) in [
        (A_VERDICT_THE_GATEWAY_ENVELOPED, Verdict::Accepted {}),
        (A_VERDICT_THE_GATEWAY_DOUBLE_ENCODED, Verdict::Accepted {}),
        (r#"{"verdict": "accepted"}"#, Verdict::Accepted {}),
        (
            r#"```json
{"verdict": "rejected", "findings": ["it renamed a symbol the ticket never named"]}
```"#,
            Verdict::Rejected {
                findings: vec!["it renamed a symbol the ticket never named".to_string()],
            },
        ),
    ] {
        let (host, _g) = test_host();
        let read = judge_briefed(
            MockCompletionModel::new([MockTurn::text(answered)]),
            &redaction(),
            host,
            budget(),
            judging(),
            None,
        )
        .await
        .unwrap_or_else(|error| panic!("this build reads {answered:?}: {error}"));
        assert_eq!(read, expected, "of {answered:?}");
    }
}

const RECORDED_FENCE: &str =
    include_str!("../../../tests/fixtures/gateway-real/review-answer.json");

fn the_text_the_gateway_fenced() -> String {
    let response: serde_json::Value =
        serde_json::from_str(RECORDED_FENCE).expect("the recorded gateway response is JSON");
    response["choices"][0]["message"]["content"]
        .as_str()
        .expect("the recorded response carries the answer as text")
        .to_string()
}

fn in_the_fence_the_gateway_sent(payload: &str) -> String {
    let content = the_text_the_gateway_fenced();
    let opens = content
        .find('{')
        .expect("the recorded text carries one object");
    let closes = content
        .rfind('}')
        .expect("the recorded text carries one object")
        + 1;
    format!("{}{payload}{}", &content[..opens], &content[closes..])
}

fn in_the_envelope_the_gateway_sent(payload: serde_json::Value) -> String {
    let mut body: serde_json::Value =
        serde_json::from_str(RECORDED_ENVELOPE).expect("the recorded gateway body is JSON");
    let envelope = body
        .as_object_mut()
        .expect("the recorded body is an object");
    assert!(
        envelope.len() == 1 && envelope["parameters"].is_object(),
        "the recorded envelope is one `parameters` member holding an object, and a fixture of \
         another shape would make this a shape nobody recorded: {envelope:?}"
    );
    envelope.insert("parameters".to_string(), payload);
    body.to_string()
}

fn in_the_string_the_gateway_sent(payload: serde_json::Value) -> String {
    let mut body: serde_json::Value =
        serde_json::from_str(RECORDED_STRING).expect("the recorded gateway body is JSON");
    let envelope = body
        .as_object_mut()
        .expect("the recorded body is an object");
    assert!(
        envelope.len() == 1 && envelope["parameters"].is_string(),
        "the recorded envelope is one `parameters` member holding a string, and a fixture of \
         another shape would make this a shape nobody recorded: {envelope:?}"
    );
    envelope.insert(
        "parameters".to_string(),
        serde_json::Value::String(payload.to_string()),
    );
    body.to_string()
}

#[tokio::test]
async fn a_verdict_reads_in_each_shape_a_recorded_gateway_body_arrived_in() {
    let content = the_text_the_gateway_fenced();
    let opens = content.find('{').expect("one object");
    let closes = content.rfind('}').expect("one object") + 1;
    assert_eq!(
        in_the_fence_the_gateway_sent(&content[opens..closes]),
        content,
        "the framing is read off the recorded body, so putting the body's own object back \
         reproduces it byte for byte"
    );
    assert!(
        content.starts_with(" ```json\n") && content.ends_with("\n```"),
        "and the framing is a fence with a leading space, as the gateway sent it, so a fixture \
         normalised to bare JSON would prove nothing: {content:?}"
    );

    let rejected = json!({
        "verdict": "rejected",
        "findings": ["src/lib.rs names a second function the ticket never asked for"],
    });
    let a_rejection = Verdict::Rejected {
        findings: vec!["src/lib.rs names a second function the ticket never asked for".to_string()],
    };
    for (shape, answered, expected) in [
        (
            "the fence the review's answer arrived in",
            in_the_fence_the_gateway_sent(r#"{"verdict": "accepted"}"#),
            Verdict::Accepted {},
        ),
        (
            "that fence around a rejection",
            in_the_fence_the_gateway_sent(&rejected.to_string()),
            a_rejection.clone(),
        ),
        (
            "the envelope the report arrived in",
            in_the_envelope_the_gateway_sent(json!({"verdict": "accepted"})),
            Verdict::Accepted {},
        ),
        (
            "the string the report arrived in",
            in_the_string_the_gateway_sent(rejected.clone()),
            a_rejection.clone(),
        ),
    ] {
        let (host, _g) = test_host();
        let read = judge_briefed(
            MockCompletionModel::new([MockTurn::text(&answered)]),
            &redaction(),
            host,
            budget(),
            judging(),
            None,
        )
        .await
        .unwrap_or_else(|error| {
            panic!("a verdict in {shape} is a shape a live run has produced: {error}\n{answered}")
        });
        assert_eq!(read, expected, "in {shape}: {answered}");
    }
}

#[tokio::test]
async fn an_evaluation_that_answers_nothing_is_told_apart_from_one_that_answers_wrongly() {
    let (host, _g) = test_host();
    let unanswered = judge_briefed(
        MockCompletionModel::new(
            (0..=fiddle_runtime::agent::RETURNS)
                .map(|_| MockTurn::text("   "))
                .collect::<Vec<_>>(),
        ),
        &redaction(),
        host,
        budget(),
        judging(),
        None,
    )
    .await;
    let Err(AgentError::Protocol { reason }) = &unanswered else {
        panic!("blank text is not a verdict: {unanswered:?}");
    };
    assert!(
        reason.starts_with("the model returned no final content at all"),
        "a blank answer is named as no answer, so the person reading the log does not go \
         looking for a field in it: {reason}"
    );
    assert!(
        reason.contains(&format!(
            "after {} of its turns were returned",
            fiddle_runtime::agent::RETURNS
        )),
        "and a blank answer was returned before the refusal stood: {reason}"
    );
}

#[tokio::test]
async fn the_evaluation_sends_no_structured_output_schema_and_asks_for_the_verdict_in_its_preamble()
{
    let (host, _g) = test_host();
    let gateway = ObeysItsToolChoice::answering(A_VERDICT_THE_GATEWAY_ENVELOPED);
    let verdict = judge_briefed(
        gateway.clone(),
        &redaction(),
        host,
        budget(),
        judging(),
        None,
    )
    .await
    .unwrap_or_else(|error| panic!("the evaluation answers before the request is read: {error}"));
    assert_eq!(verdict, Verdict::Accepted {});

    let schemas = gateway.output_schemas_it_was_sent();
    assert_eq!(
        schemas.len(),
        1,
        "one turn answered, so one request was sent, and the request below is that one and \
         not one this lane built for itself"
    );
    assert_eq!(
        schemas,
        vec![None],
        "the evaluation's request carries no structured-output schema, so the OpenAI-compatible \
         provider has nothing to build a `response_format` from. \
         `the_schema_the_repair_step_sends_is_the_reports_own_and_the_newtype_moves_nothing` \
         reads `Some` off the same field on the repair side, so this `None` is the evaluation's \
         choice and not a field the capture cannot see: {schemas:?}"
    );

    let preambles = gateway.preambles_it_was_sent();
    let system = preambles[0]
        .clone()
        .expect("the evaluation's request opens with a system message");
    assert!(
        system.starts_with(JUDGE_PREAMBLE_FOR_TESTS),
        "the brief the caller gave still opens the system message: {system}"
    );
    let asked_for =
        serde_json::to_string(schemars::schema_for!(Verdict).as_value()).expect("a schema is JSON");
    assert!(
        system.contains(&asked_for),
        "the shape asked for is the verdict's own schema, whole, and it travels in the system \
         message rather than as a provider constraint. rig-agent 0.41.0 appends it there under \
         `OutputMode::Prompted`, and this lane reads it off the request the model was sent and \
         not off the builder: {system}"
    );
    assert!(
        system.contains("Respond with ONLY a single JSON object"),
        "and the sentence asking for one object comes with it, so the model is told what to do \
         with the schema and not only shown it: {system}"
    );
}

#[tokio::test]
async fn the_schema_the_repair_step_sends_is_the_reports_own_and_the_newtype_moves_nothing() {
    let (host, _g) = test_host();
    let model = MockCompletionModel::new([report_turn("nothing", true)]);
    let _ = attempt(
        model.clone(),
        &redaction(),
        host,
        budget(),
        Direction::Fresh,
        None,
    )
    .await;

    let requests = model.requests();
    assert!(
        !requests.is_empty(),
        "no request reached the model, so the schema below would be the schema of nothing"
    );
    let sent = requests[0]
        .output_schema
        .clone()
        .expect("the repair step's request carries a structured-output schema")
        .to_value();
    assert_eq!(
        sent,
        schemars::schema_for!(RepairReport).to_value(),
        "the repair side is held the same way the verdict side is, and read off the request \
         rather than off two schemas this lane generated for itself. It is built from \
         `Reported`, so this equality is that newtype's delegation"
    );
    assert_eq!(
        sent["title"],
        json!("RepairReport"),
        "and the title the provider names the wire payload by is the report's own: {sent}"
    );
}
