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
    let model = MockCompletionModel::new([MockTurn::text("this is not the schema")]);

    let outcome = attempt(model, &redaction(), host, budget(), Direction::Fresh, None).await;

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
        "a structured-output schema reaches the provider as `response_format`, and a \
         gateway that fronts Anthropic lifts it into a prepended tool, so a top-level \
         combiner there is refused as `tools.0.custom.input_schema`. {} of them carry \
         one:\n{}",
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
             them, because `prompt_typed` pins `OutputMode::Native`, and a count-only assertion \
             here would pass whether or not one arrived"
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
}

impl ObeysItsToolChoice {
    fn answering(answer: &str) -> Self {
        ObeysItsToolChoice {
            answer: answer.to_string(),
            seen: Arc::new(Mutex::new(Vec::new())),
            schemas: Arc::new(Mutex::new(Vec::new())),
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

const THE_DOCUMENTS_BOUND: usize = 12;

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
async fn the_repair_step_still_obliges_a_tool_call_and_a_gateway_that_obeys_leaves_it_no_answer() {
    let (host, _g) = test_host();
    let gateway = ObeysItsToolChoice::answering(RECORDED_ENVELOPE);

    let unanswered = attempt(
        gateway.clone(),
        &redaction(),
        host,
        bounded_at(4),
        Direction::Fresh,
        None,
    )
    .await;

    assert!(
        matches!(unanswered, Err(AgentError::Bounded { .. })),
        "this lane pins a defect rather than a fix. `prompt_typed` pins `OutputMode::Native`, so \
         no output tool is offered and the answer can only be the assistant's final text, which \
         `required` forbids. The repair step survives in production because the recorded gateway \
         returned `stop` with text anyway, not because fiddle offered it a way to answer. It \
         returned {unanswered:?}"
    );
    assert_eq!(
        gateway.choices_it_was_sent(),
        vec![Some(ToolChoice::Required); 4],
        "and it is `required` on every turn, so the repair step's escape is the gateway's \
         leniency and not this build's design"
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
    ] {
        let (host, _g) = test_host();
        let refused = judge_briefed(
            MockCompletionModel::new([MockTurn::text(answered)]),
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
    }
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

#[tokio::test]
async fn the_schema_the_evaluation_sends_is_the_verdicts_own_and_the_newtype_moves_nothing() {
    let (host, _g) = test_host();
    let gateway = ObeysItsToolChoice::answering(A_VERDICT_THE_GATEWAY_ENVELOPED);
    let _ = judge_briefed(
        gateway.clone(),
        &redaction(),
        host,
        budget(),
        judging(),
        None,
    )
    .await;

    let schemas = gateway.output_schemas_it_was_sent();
    assert_eq!(
        schemas.len(),
        1,
        "one turn answered, so one request was sent, and the schema below is that request's \
         and not a schema this lane generated for itself"
    );
    let sent = schemas[0]
        .clone()
        .expect("the evaluation's request carries a structured-output schema");
    assert_eq!(
        sent,
        schemars::schema_for!(Verdict).to_value(),
        "the request carries the verdict's own schema, whole and field for field. It is built \
         from `Judged` and not from the `.output_schema::<Verdict>()` the builder names, which \
         `from_agent` overrides the way it overrides `output_mode`, so this equality is the \
         newtype's delegation and nothing else holds it. Comparing one member of a locally \
         generated schema would have passed whatever the request held"
    );
    assert_eq!(
        sent["title"],
        json!("Verdict"),
        "and the provider reads `response_format.json_schema.name` off that title, so a newtype \
         that named itself would rename the wire payload. rig builds that `response_format` and \
         this build does not, which is why \
         `no_schema_a_toil_run_sends_carries_a_combiner_at_the_top_of_itself` reads the wire \
         form off the socket and this lane reads the request: {sent}"
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
