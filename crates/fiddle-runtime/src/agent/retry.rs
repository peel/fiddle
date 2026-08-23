use crate::agent::transcript::{Record, Transcripts, RETRY, UNANSWERED};
use crate::gateway::Redaction;
use rig_core::completion::{CompletionError, CompletionModel, CompletionRequest};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub const RETRIES: usize = 2;

const EMPTY: [&str; 2] = [
    "Response contained no message or tool call (empty)",
    "Response contained no choices",
];

pub fn empty_response(error: &CompletionError) -> Option<&str> {
    match error {
        CompletionError::ResponseError(reason) if EMPTY.contains(&reason.as_str()) => Some(reason),
        _ => None,
    }
}

#[derive(Clone)]
pub struct RetryingModel<M> {
    model: M,
    bound: usize,
    spent: Arc<Mutex<usize>>,
    calls: Arc<AtomicU64>,
    transcripts: Option<Transcripts>,
    redaction: Redaction,
}

impl<M> RetryingModel<M> {
    pub fn bounded(
        model: M,
        bound: usize,
        redaction: &Redaction,
        transcripts: Option<&Transcripts>,
    ) -> Self {
        RetryingModel {
            model,
            bound,
            spent: Arc::new(Mutex::new(0)),
            calls: Arc::new(AtomicU64::new(0)),
            transcripts: transcripts.cloned(),
            redaction: redaction.clone(),
        }
    }

    pub fn retried(&self) -> usize {
        *self.locked()
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, usize> {
        self.spent
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn allowed(&self) -> Option<usize> {
        let mut spent = self.locked();
        if *spent >= self.bound {
            return None;
        }
        *spent += 1;
        Some(*spent)
    }

    fn record(&self, kind: &'static str, turn: u64, retries: usize, reason: &str) {
        let Some(transcripts) = &self.transcripts else {
            return;
        };
        transcripts.append(
            &self.redaction,
            Record::of(kind)
                .number("turn", turn)
                .number("retries", retries as u64)
                .number("bound", self.bound as u64)
                .text("reason", reason),
        );
    }
}

impl<M> CompletionModel for RetryingModel<M>
where
    M: CompletionModel,
{
    type Response = M::Response;
    type StreamingResponse = M::StreamingResponse;
    type Client = M::Client;

    fn make(client: &Self::Client, model: impl Into<String>) -> Self {
        RetryingModel::bounded(M::make(client, model), RETRIES, &Redaction::unknown(), None)
    }

    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> Result<rig_core::completion::CompletionResponse<Self::Response>, CompletionError> {
        let turn = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        loop {
            let answered = self.model.completion(request.clone()).await;
            let empty = match &answered {
                Err(error) => empty_response(error).map(str::to_string),
                Ok(_) => None,
            };
            let Some(reason) = empty else {
                return answered;
            };
            let Some(retries) = self.allowed() else {
                self.record(UNANSWERED, turn, self.retried(), &reason);
                return answered;
            };
            self.record(RETRY, turn, retries, &reason);
        }
    }

    async fn stream(
        &self,
        request: CompletionRequest,
    ) -> Result<
        rig_core::streaming::StreamingCompletionResponse<Self::StreamingResponse>,
        CompletionError,
    > {
        self.model.stream(request).await
    }

    fn composes_native_output_with_tools(&self) -> bool {
        self.model.composes_native_output_with_tools()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tools::tests::test_host;
    use crate::agent::transcript::{tests_support::lines, TranscriptHook, TranscriptModel, SENT};
    use crate::agent::ReadFile;
    use rig_agent::completion::Prompt;
    use rig_agent::tool::ToolContext;
    use rig_agent::AgentBuilder;
    use rig_core::providers::openai;
    use rig_core::test_utils::{MockCompletionModel, MockTurn};

    const SECRET: &str = "sk-retry-must-not-appear-31a8";

    #[derive(Clone)]
    struct Answers<M> {
        inner: M,
        emptily: Arc<Vec<u64>>,
        always: bool,
        calls: Arc<AtomicU64>,
    }

    impl<M> Answers<M> {
        fn on(inner: M, calls: &[u64]) -> Self {
            Answers {
                inner,
                emptily: Arc::new(calls.to_vec()),
                always: false,
                calls: Arc::new(AtomicU64::new(0)),
            }
        }

        fn always(inner: M) -> Self {
            Answers {
                inner,
                emptily: Arc::new(Vec::new()),
                always: true,
                calls: Arc::new(AtomicU64::new(0)),
            }
        }

        fn served(&self) -> u64 {
            self.calls.load(Ordering::Relaxed)
        }
    }

    impl<M> CompletionModel for Answers<M>
    where
        M: CompletionModel,
    {
        type Response = M::Response;
        type StreamingResponse = M::StreamingResponse;
        type Client = M::Client;

        fn make(client: &Self::Client, model: impl Into<String>) -> Self {
            Answers::on(M::make(client, model), &[])
        }

        async fn completion(
            &self,
            request: CompletionRequest,
        ) -> Result<rig_core::completion::CompletionResponse<Self::Response>, CompletionError>
        {
            let call = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
            if self.always || self.emptily.contains(&call) {
                return Err(CompletionError::ResponseError(EMPTY[0].to_string()));
            }
            self.inner.completion(request).await
        }

        async fn stream(
            &self,
            request: CompletionRequest,
        ) -> Result<
            rig_core::streaming::StreamingCompletionResponse<Self::StreamingResponse>,
            CompletionError,
        > {
            self.inner.stream(request).await
        }
    }

    fn a_reading_repair(turns: usize) -> MockCompletionModel {
        let mut script: Vec<MockTurn> = (0..turns)
            .map(|_| {
                MockTurn::tool_call(
                    "call_1",
                    "read_file",
                    serde_json::json!({"path": "src/lib.rs"}),
                )
            })
            .collect();
        script.push(MockTurn::text("read it"));
        MockCompletionModel::new(script)
    }

    struct Ran {
        finished: bool,
        served: u64,
        records: Vec<serde_json::Value>,
        retried: usize,
    }

    async fn run<M>(model: Answers<M>, bound: usize, max_turns: usize) -> Ran
    where
        M: CompletionModel + 'static,
    {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = Transcripts::under(dir.path(), "a-run");
        let redaction = Redaction::of(SECRET);
        let (host, _g) = test_host();
        let retrying = RetryingModel::bounded(model.clone(), bound, &redaction, Some(&transcripts));
        let hook = TranscriptHook::recording(transcripts.clone(), redaction.clone());
        let agent = AgentBuilder::new(TranscriptModel::wrapping(
            retrying.clone(),
            Some(hook.clone()),
        ))
        .tool(ReadFile)
        .add_hook(hook)
        .build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        let finished = agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(max_turns)
            .await
            .is_ok();

        Ran {
            finished,
            served: model.served(),
            records: match transcripts.wrote().records {
                0 => Vec::new(),
                _ => lines(transcripts.path()),
            },
            retried: retrying.retried(),
        }
    }

    fn of_kind(records: &[serde_json::Value], kind: &str) -> Vec<serde_json::Value> {
        records
            .iter()
            .filter(|record| record["record"] == kind)
            .cloned()
            .collect()
    }

    fn an_empty_completion() -> serde_json::Value {
        serde_json::json!({
            "id": "chatcmpl-empty",
            "object": "chat.completion",
            "created": 0,
            "model": "a-model",
            "system_fingerprint": null,
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": ""},
                "logprobs": null,
                "finish_reason": "stop",
            }],
            "usage": null,
        })
    }

    fn converted(body: serde_json::Value) -> CompletionError {
        let raw: openai::completion::CompletionResponse =
            serde_json::from_value(body).expect("the gateway body deserializes");
        rig_core::completion::CompletionResponse::<openai::completion::CompletionResponse>::try_from(
            raw,
        )
        .expect_err("a response carrying nothing cannot be converted")
    }

    #[test]
    fn the_text_the_provider_path_returns_for_an_empty_response_is_the_text_this_retries() {
        let carried_nothing = converted(an_empty_completion());
        assert_eq!(
            empty_response(&carried_nothing),
            Some(EMPTY[0]),
            "a text this code guesses at is a retry that never fires: \
             {carried_nothing}"
        );

        let mut no_choice = an_empty_completion();
        no_choice["choices"] = serde_json::json!([]);
        let held_no_choice = converted(no_choice);
        assert_eq!(
            empty_response(&held_no_choice),
            Some(EMPTY[1]),
            "a 200 holding no choice is the same absence: {held_no_choice}"
        );
    }

    #[test]
    fn an_answer_is_never_retried() {
        let answers = [
            CompletionError::ProviderError("the gateway answered 429".to_string()),
            CompletionError::ResponseError(
                "Response did not contain a valid message or tool call".to_string(),
            ),
            CompletionError::ResponseError("expected value at line 1 column 1".to_string()),
        ];

        for answer in answers {
            assert!(
                empty_response(&answer).is_none(),
                "a retry loop over a deterministic failure spends money to learn \
                 nothing: {answer}"
            );
        }
    }

    #[tokio::test]
    async fn a_response_that_arrives_after_one_empty_one_finishes_the_attempt() {
        let ran = run(Answers::on(a_reading_repair(1), &[1]), RETRIES, 6).await;

        assert!(ran.finished, "one empty response must not end the attempt");
        assert_eq!(
            ran.served, 3,
            "the empty response, the retry, and the turn after it"
        );
        assert_eq!(ran.retried, 1, "one empty response spends one retry");

        let retried = of_kind(&ran.records, RETRY);
        assert_eq!(
            retried.len(),
            1,
            "a retry that leaves no record turns an intermittent fault into an \
             invisible one: {:?}",
            ran.records
        );
        assert_eq!(retried[0]["turn"], 1, "{:?}", retried[0]);
        assert_eq!(retried[0]["retries"], 1, "{:?}", retried[0]);
        assert_eq!(retried[0]["bound"], RETRIES as u64, "{:?}", retried[0]);
        assert!(
            retried[0]["reason"].as_str().unwrap().contains("empty"),
            "the record must name what came back: {:?}",
            retried[0]
        );
        assert!(
            of_kind(&ran.records, UNANSWERED).is_empty(),
            "a turn that was answered is not unanswered: {:?}",
            ran.records
        );
    }

    #[tokio::test]
    async fn a_provider_that_only_answers_emptily_ends_the_attempt_after_the_stated_bound() {
        let ran = run(Answers::always(a_reading_repair(1)), RETRIES, 6).await;

        assert!(
            !ran.finished,
            "an attempt that never got an answer must fail"
        );
        assert_eq!(
            ran.served,
            RETRIES as u64 + 1,
            "the first call and {RETRIES} retries, and no more"
        );
        assert_eq!(ran.retried, RETRIES);

        let retried = of_kind(&ran.records, RETRY);
        assert_eq!(
            retried.len(),
            RETRIES,
            "each retry is recorded: {:?}",
            ran.records
        );
        let unanswered = of_kind(&ran.records, UNANSWERED);
        assert_eq!(
            unanswered.len(),
            1,
            "the file must say why it stops, or a reader guesses: {:?}",
            ran.records
        );
        assert_eq!(unanswered[0]["turn"], 1, "{:?}", unanswered[0]);
        assert_eq!(
            unanswered[0]["retries"], RETRIES as u64,
            "{:?}",
            unanswered[0]
        );
    }

    #[tokio::test]
    async fn the_bound_counts_the_retries_of_one_attempt_and_not_of_one_turn() {
        let ran = run(Answers::on(a_reading_repair(2), &[1, 3, 5]), RETRIES, 6).await;

        assert!(
            !ran.finished,
            "the third empty response has no retry left, so the attempt ends"
        );
        assert_eq!(ran.retried, RETRIES);
        assert_eq!(
            ran.served, 5,
            "two turns cost two calls each, and the fifth is refused"
        );

        let retried = of_kind(&ran.records, RETRY);
        let turns: Vec<u64> = retried
            .iter()
            .map(|record| record["turn"].as_u64().unwrap())
            .collect();
        assert_eq!(
            turns,
            vec![1, 2],
            "one retry on each of two turns spends the whole allowance: {:?}",
            ran.records
        );
        assert_eq!(
            of_kind(&ran.records, UNANSWERED)[0]["turn"],
            3,
            "{:?}",
            ran.records
        );
    }

    #[tokio::test]
    async fn a_retry_names_the_turn_the_request_was_sent_under() {
        let ran = run(Answers::on(a_reading_repair(2), &[2]), RETRIES, 6).await;

        assert!(ran.finished, "stderr of the attempt: {:?}", ran.records);
        let sent: Vec<u64> = of_kind(&ran.records, SENT)
            .iter()
            .map(|record| record["turn"].as_u64().unwrap())
            .collect();
        let retried = of_kind(&ran.records, RETRY);
        assert_eq!(retried.len(), 1, "{:?}", ran.records);
        let turn = retried[0]["turn"].as_u64().unwrap();
        assert!(
            sent.contains(&turn),
            "a retry under a turn number no request carries cannot be placed in \
             the file: {turn} against {sent:?}"
        );
        assert_eq!(turn, 2, "the second turn is the one that came back empty");
    }

    #[tokio::test]
    async fn a_bound_of_none_retries_nothing_and_records_the_refusal() {
        let ran = run(Answers::always(a_reading_repair(1)), 0, 6).await;

        assert!(!ran.finished);
        assert_eq!(ran.served, 1, "a bound of none asks the gateway once");
        assert!(of_kind(&ran.records, RETRY).is_empty());
        assert_eq!(of_kind(&ran.records, UNANSWERED).len(), 1);
    }
}
