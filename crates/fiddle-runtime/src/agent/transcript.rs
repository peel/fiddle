use crate::gateway::Redaction;
use rig_agent::agent::hook::{
    AgentHook, CompletionCall, CompletionCallAction, CompletionResponse, HookContext,
    InvalidToolCallAction, InvalidToolCallContext, ObservationAction, ToolCall, ToolCallAction,
    ToolResultAction, ToolResultEvent,
};
use rig_core::completion::message::AssistantContent;
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const SWITCH: &str = "FIDDLE_TRANSCRIPT";

pub const ON: &str = "1";

pub const DIRECTORY: &str = "transcript";

pub const FIELD_LIMIT: usize = 16_384;

pub const FILE_LIMIT_BYTES: usize = 8 * 1024 * 1024;

const WITHHELD: &str = "fiddle holds no credential to redact, so it withholds this text";

pub const ELAPSED: &str = "elapsed_ms";

pub const TOOK: &str = "duration_ms";

pub fn cut_note() -> String {
    format!("\n[fiddle cut this text at {FIELD_LIMIT} characters]")
}

#[derive(Debug, thiserror::Error)]
#[error(
    "{SWITCH} accepts only {ON}, and this run set it to {given:?}; unset it to \
     record no transcript"
)]
pub struct SwitchUnknown {
    pub given: String,
}

pub fn requested(value: Option<&str>) -> Result<bool, SwitchUnknown> {
    match value.map(str::trim) {
        None | Some("") => Ok(false),
        Some(ON) => Ok(true),
        Some(given) => Err(SwitchUnknown {
            given: given.to_string(),
        }),
    }
}

pub struct Record {
    kind: &'static str,
    numbers: Vec<(&'static str, u64)>,
    texts: Vec<(&'static str, String)>,
}

impl Record {
    pub fn of(kind: &'static str) -> Self {
        Record {
            kind,
            numbers: Vec::new(),
            texts: Vec::new(),
        }
    }

    pub fn number(mut self, name: &'static str, value: u64) -> Self {
        self.numbers.push((name, value));
        self
    }

    pub fn text(mut self, name: &'static str, value: &str) -> Self {
        self.texts.push((name, value.to_string()));
        self
    }

    fn rendered(self, redaction: &Redaction, elapsed: Duration) -> serde_json::Value {
        let mut fields = serde_json::Map::new();
        fields.insert(
            "record".to_string(),
            serde_json::Value::String(self.kind.to_string()),
        );
        fields.insert(
            ELAPSED.to_string(),
            serde_json::Value::from(elapsed.as_millis() as u64),
        );
        for (name, value) in self.numbers {
            fields.insert(name.to_string(), serde_json::Value::from(value));
        }
        for (name, value) in self.texts {
            fields.insert(name.to_string(), safe(redaction, &value));
        }
        serde_json::Value::Object(fields)
    }
}

fn safe(redaction: &Redaction, text: &str) -> serde_json::Value {
    let Some(held) = redaction.redacted(text, FIELD_LIMIT) else {
        return serde_json::Value::String(WITHHELD.to_string());
    };
    serde_json::Value::String(match held.cut {
        true => format!("{}{}", held.text, cut_note()),
        false => held.text,
    })
}

#[derive(Clone, Debug, Default)]
pub struct Wrote {
    pub records: u64,
    pub dropped: u64,
    pub bytes: usize,
    pub failure: Option<String>,
}

impl Wrote {
    pub fn began(&self) -> bool {
        self.records > 0 || self.dropped > 0 || self.failure.is_some()
    }
}

#[derive(Default)]
struct State {
    file: Option<std::fs::File>,
    began: Option<Instant>,
    wrote: Wrote,
}

impl State {
    fn elapsed(&mut self) -> Duration {
        match self.began {
            Some(began) => began.elapsed(),
            None => {
                self.began = Some(Instant::now());
                Duration::ZERO
            }
        }
    }
}

#[derive(Clone)]
pub struct Transcripts {
    path: PathBuf,
    state: Arc<Mutex<State>>,
}

impl std::fmt::Debug for Transcripts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Transcripts({})", self.path.display())
    }
}

impl Transcripts {
    pub fn writing_to(path: PathBuf) -> Self {
        Transcripts {
            path,
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    pub fn under(report_dir: &Path, name: &str) -> Self {
        Transcripts::writing_to(report_dir.join(DIRECTORY).join(format!("{name}.jsonl")))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn wrote(&self) -> Wrote {
        self.locked().wrote.clone()
    }

    pub fn append(&self, redaction: &Redaction, record: Record) {
        let mut state = self.locked();
        if state.wrote.failure.is_some() {
            return;
        }
        let elapsed = state.elapsed();
        let line = match serde_json::to_string(&record.rendered(redaction, elapsed)) {
            Ok(rendered) => format!("{rendered}\n"),
            Err(source) => {
                state.wrote.failure = Some(source.to_string());
                return;
            }
        };
        if state.wrote.bytes + line.len() > FILE_LIMIT_BYTES {
            state.wrote.dropped += 1;
            return;
        }
        if state.file.is_none() {
            match create(&self.path) {
                Ok(file) => state.file = Some(file),
                Err(source) => {
                    state.wrote.failure = Some(source.to_string());
                    return;
                }
            }
        }
        let file = state.file.as_mut().expect("the file was just opened");
        if let Err(source) = file.write_all(line.as_bytes()).and_then(|()| file.flush()) {
            state.wrote.failure = Some(source.to_string());
            return;
        }
        state.wrote.bytes += line.len();
        state.wrote.records += 1;
    }

    fn locked(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn create(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::File::create(path)
}

pub const BRIEF: &str = "brief";

pub const SENT: &str = "sent";

pub const RECEIVED: &str = "received";

pub const TOOL: &str = "tool";

pub const INVALID: &str = "invalid";

pub const SPENT: &str = "spent";

pub const FINISH: &str = "finish";

#[derive(Clone)]
pub struct TranscriptHook {
    transcripts: Transcripts,
    redaction: Redaction,
    starts: Arc<Mutex<HashMap<String, Instant>>>,
}

impl TranscriptHook {
    pub fn recording(transcripts: Transcripts, redaction: Redaction) -> Self {
        TranscriptHook {
            transcripts,
            redaction,
            starts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn append(&self, record: Record) {
        self.transcripts.append(&self.redaction, record);
    }

    fn started(&self) -> std::sync::MutexGuard<'_, HashMap<String, Instant>> {
        self.starts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn took(&self, record: Record, call: &str) -> Record {
        match self.started().remove(call) {
            Some(began) => record.number(TOOK, began.elapsed().as_millis() as u64),
            None => record,
        }
    }
}

fn rendered<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .unwrap_or_else(|source| format!("fiddle could not render this: {source}"))
}

fn returned(turn: u64, block: &AssistantContent) -> Record {
    let record = Record::of(RECEIVED).number("turn", turn);
    match block {
        AssistantContent::Text(text) => record.text("text", &text.text),
        AssistantContent::ToolCall(call) => record
            .text("tool", &call.function.name)
            .text("args", &call.function.arguments.to_string()),
        AssistantContent::Reasoning(reasoning) => record.text("reasoning", &rendered(reasoning)),
        AssistantContent::Image(_) => record.text("image", "an image block"),
    }
}

impl AgentHook for TranscriptHook {
    async fn on_completion_call(
        &self,
        _ctx: &HookContext,
        event: CompletionCall<'_>,
    ) -> CompletionCallAction {
        self.append(
            Record::of(SENT)
                .number("turn", event.turn as u64)
                .number("history", event.history.len() as u64)
                .text("prompt", &rendered(event.prompt)),
        );
        CompletionCallAction::Continue
    }

    async fn on_completion_response(
        &self,
        ctx: &HookContext,
        event: CompletionResponse<'_>,
    ) -> ObservationAction {
        let turn = ctx.turn() as u64;
        self.append(
            Record::of(SPENT)
                .number("turn", turn)
                .number("blocks", event.content.len() as u64)
                .number("input_tokens", event.usage.input_tokens)
                .number("output_tokens", event.usage.output_tokens),
        );
        for block in event.content.iter() {
            self.append(returned(turn, block));
        }
        ObservationAction::Continue
    }

    async fn on_tool_call(&self, _ctx: &HookContext, event: ToolCall<'_>) -> ToolCallAction {
        self.started()
            .insert(event.internal_call_id.to_string(), Instant::now());
        ToolCallAction::Run
    }

    async fn on_tool_result(
        &self,
        ctx: &HookContext,
        event: ToolResultEvent<'_>,
    ) -> ToolResultAction {
        let record = self.took(
            Record::of(TOOL).number("turn", ctx.turn() as u64),
            event.internal_call_id,
        );
        self.append(
            record
                .text("tool", event.tool_name)
                .text("args", event.args)
                .text("result", &event.presentation.render()),
        );
        ToolResultAction::Keep
    }

    async fn on_invalid_tool_call(
        &self,
        ctx: &HookContext,
        event: &InvalidToolCallContext,
    ) -> Option<InvalidToolCallAction> {
        self.append(
            Record::of(INVALID)
                .number("turn", ctx.turn() as u64)
                .text("tool", &event.tool_name)
                .text("args", event.args.as_deref().unwrap_or(""))
                .text("offered", &event.available_tools.join(", ")),
        );
        None
    }
}

fn finish_reason<T: serde::Serialize>(raw: &T) -> Option<String> {
    let rendered = serde_json::to_value(raw).ok()?;
    rendered["choices"][0]["finish_reason"]
        .as_str()
        .map(str::to_string)
}

#[derive(Clone)]
pub struct TranscriptModel<M> {
    model: M,
    hook: Option<TranscriptHook>,
    calls: Arc<AtomicU64>,
}

impl<M> TranscriptModel<M> {
    pub fn wrapping(model: M, hook: Option<TranscriptHook>) -> Self {
        TranscriptModel {
            model,
            hook,
            calls: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl<M> rig_core::completion::CompletionModel for TranscriptModel<M>
where
    M: rig_core::completion::CompletionModel,
{
    type Response = M::Response;
    type StreamingResponse = M::StreamingResponse;
    type Client = M::Client;

    fn make(client: &Self::Client, model: impl Into<String>) -> Self {
        TranscriptModel::wrapping(M::make(client, model), None)
    }

    async fn completion(
        &self,
        request: rig_core::completion::CompletionRequest,
    ) -> Result<
        rig_core::completion::CompletionResponse<Self::Response>,
        rig_core::completion::CompletionError,
    > {
        let turn = self.calls.fetch_add(1, Ordering::Relaxed) + 1;
        let answered = self.model.completion(request).await;
        if let (Some(hook), Ok(response)) = (&self.hook, &answered) {
            if let Some(reason) = finish_reason(&response.raw_response) {
                hook.append(
                    Record::of(FINISH)
                        .number("turn", turn)
                        .text("reason", &reason),
                );
            }
        }
        answered
    }

    async fn stream(
        &self,
        request: rig_core::completion::CompletionRequest,
    ) -> Result<
        rig_core::streaming::StreamingCompletionResponse<Self::StreamingResponse>,
        rig_core::completion::CompletionError,
    > {
        self.model.stream(request).await
    }

    fn composes_native_output_with_tools(&self) -> bool {
        self.model.composes_native_output_with_tools()
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use std::path::Path;

    pub fn lines(path: &Path) -> Vec<serde_json::Value> {
        std::fs::read_to_string(path)
            .expect("the transcript is on disk")
            .lines()
            .map(|line| serde_json::from_str(line).expect("every line is one JSON object"))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::*;
    use super::*;

    const SECRET: &str = "sk-transcript-must-not-appear-4d10";

    #[test]
    fn an_unset_switch_is_off_and_an_unknown_value_is_refused() {
        assert!(!requested(None).unwrap(), "an unset switch records nothing");
        assert!(!requested(Some("")).unwrap(), "an empty switch is unset");
        assert!(!requested(Some("   ")).unwrap(), "so is a switch of spaces");
        assert!(
            requested(Some(ON)).unwrap(),
            "{ON} is the one value that is on"
        );

        let refused = requested(Some("true")).expect_err("only 1 turns it on");
        assert!(
            refused.to_string().contains("true") && refused.to_string().contains(SWITCH),
            "the refusal must name the variable and the value: {refused}"
        );
    }

    #[test]
    fn nothing_is_created_until_a_record_is_appended() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = Transcripts::under(dir.path(), "a-run");

        assert!(
            !transcripts.path().exists(),
            "a transcript that recorded nothing must leave no file"
        );
        assert!(!transcripts.wrote().began());

        transcripts.append(&Redaction::of(SECRET), Record::of("brief"));
        assert!(transcripts.path().exists());
        assert_eq!(transcripts.wrote().records, 1);
    }

    #[test]
    fn a_record_carries_the_text_and_never_the_credential() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = Transcripts::under(dir.path(), "a-run");
        let redaction = Redaction::of(SECRET);

        transcripts.append(
            &redaction,
            Record::of("received")
                .number("turn", 3)
                .text("content", &format!("the key {SECRET} was refused")),
        );

        let records = lines(transcripts.path());
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["record"], "received");
        assert_eq!(records[0]["turn"], 3);
        let content = records[0]["content"].as_str().unwrap();
        assert!(
            !content.contains(SECRET),
            "the credential reached the transcript: {content}"
        );
        assert!(
            content.contains(crate::gateway::REDACTED) && content.contains("was refused"),
            "the text must survive with the credential marked: {content}"
        );
    }

    #[test]
    fn every_record_carries_an_elapsed_value_that_never_decreases() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = Transcripts::under(dir.path(), "a-run");
        let redaction = Redaction::of(SECRET);

        transcripts.append(&redaction, Record::of(BRIEF).text("task", "repair it"));
        for turn in 1..=3 {
            std::thread::sleep(Duration::from_millis(2));
            transcripts.append(&redaction, Record::of(SENT).number("turn", turn));
        }

        let records = lines(transcripts.path());
        assert_eq!(records.len(), 4);
        let elapsed: Vec<u64> = records
            .iter()
            .map(|record| {
                record[ELAPSED]
                    .as_u64()
                    .unwrap_or_else(|| panic!("every record carries its elapsed time: {record}"))
            })
            .collect();
        assert_eq!(elapsed[0], 0, "the first record is the origin: {elapsed:?}");
        assert!(
            elapsed.windows(2).all(|pair| pair[0] <= pair[1]),
            "a time that falls down the file is worse than none: {elapsed:?}"
        );
        assert!(
            elapsed[3] > 0,
            "the elapsed value must advance with the run: {elapsed:?}"
        );
    }

    #[test]
    fn a_finish_reason_is_read_from_the_response_the_provider_returned() {
        assert_eq!(
            finish_reason(&serde_json::json!({"choices": [{"finish_reason": "length"}]}))
                .as_deref(),
            Some("length")
        );
        assert_eq!(
            finish_reason(&serde_json::json!({"choices": []})),
            None,
            "a response holding no choice names no reason"
        );
        assert_eq!(
            finish_reason(&serde_json::json!({"usage": {"input_tokens": 1}})),
            None,
            "a provider that reports no reason must not have one invented for it"
        );
    }

    #[test]
    fn a_field_is_withheld_when_the_redaction_holds_no_credential() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = Transcripts::under(dir.path(), "a-run");

        transcripts.append(
            &Redaction::unknown(),
            Record::of("received").text("content", "a reply nobody can promise is safe"),
        );

        let records = lines(transcripts.path());
        assert_eq!(
            records[0]["content"], WITHHELD,
            "a path that cannot redact must not write the text: {:?}",
            records[0]
        );
    }

    #[test]
    fn a_field_past_the_limit_is_cut_and_says_it_was_cut() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = Transcripts::under(dir.path(), "a-run");

        transcripts.append(
            &Redaction::of(SECRET),
            Record::of("tool").text("result", &"x".repeat(FIELD_LIMIT * 2)),
        );

        let records = lines(transcripts.path());
        let result = records[0]["result"].as_str().unwrap();
        let note = cut_note();
        let kept = result
            .strip_suffix(note.as_str())
            .unwrap_or_else(|| panic!("a cut field must name the bound: {result:?}"));
        assert_eq!(
            kept.chars().count(),
            FIELD_LIMIT,
            "the bound is {FIELD_LIMIT} characters"
        );
    }

    #[test]
    fn the_file_stops_at_its_bound_and_counts_what_it_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = Transcripts::under(dir.path(), "a-run");
        let redaction = Redaction::of(SECRET);
        let filler = "y".repeat(FIELD_LIMIT);

        for _ in 0..(FILE_LIMIT_BYTES / FIELD_LIMIT) + 4 {
            transcripts.append(&redaction, Record::of("tool").text("result", &filler));
        }

        let wrote = transcripts.wrote();
        assert!(wrote.dropped > 0, "the bound was never reached: {wrote:?}");
        assert!(
            wrote.bytes <= FILE_LIMIT_BYTES,
            "the file passed its bound: {wrote:?}"
        );
        assert_eq!(
            std::fs::metadata(transcripts.path()).unwrap().len() as usize,
            wrote.bytes,
            "the count and the file must agree"
        );
    }

    #[test]
    fn a_destination_that_cannot_be_written_is_reported_and_not_retried() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("a-file");
        std::fs::write(&blocked, "not a directory").unwrap();
        let transcripts = Transcripts::writing_to(blocked.join(DIRECTORY).join("a-run.jsonl"));
        let redaction = Redaction::of(SECRET);

        transcripts.append(&redaction, Record::of("brief").text("task", "repair it"));
        transcripts.append(&redaction, Record::of("sent").text("prompt", "repair it"));

        let wrote = transcripts.wrote();
        assert!(
            wrote.failure.is_some(),
            "a destination that refused the write must say so: {wrote:?}"
        );
        assert_eq!(wrote.records, 0);
        assert!(wrote.began(), "a failure is a fact the run must report");
    }
}

#[cfg(test)]
mod hook_tests {
    use super::tests_support::*;
    use super::*;
    use crate::agent::tools::tests::test_host;
    use crate::agent::ReadFile;
    use rig_agent::completion::Prompt;
    use rig_agent::tool::ToolContext;
    use rig_agent::AgentBuilder;
    use rig_core::test_utils::{MockCompletionModel, MockTurn};

    const SECRET: &str = "sk-hook-must-not-appear-77c2";

    #[tokio::test]
    async fn one_run_records_what_was_sent_what_came_back_and_what_a_tool_answered() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = Transcripts::under(dir.path(), "a-run");
        let (host, _g) = test_host();
        let model = MockCompletionModel::new([
            MockTurn::tool_call(
                "call_1",
                "read_file",
                serde_json::json!({"path":"src/lib.rs"}),
            ),
            MockTurn::text(format!("the key {SECRET} was refused")),
        ]);
        let agent = AgentBuilder::new(model)
            .tool(ReadFile)
            .add_hook(TranscriptHook::recording(
                transcripts.clone(),
                Redaction::of(SECRET),
            ))
            .build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(3)
            .await
            .expect("the run completes");

        let records = lines(transcripts.path());
        let kinds: Vec<&str> = records
            .iter()
            .map(|record| record["record"].as_str().unwrap())
            .collect();
        assert!(
            kinds.contains(&SENT) && kinds.contains(&RECEIVED) && kinds.contains(&TOOL),
            "the transcript must hold both sides of every turn: {kinds:?}"
        );

        let answered = records
            .iter()
            .find(|record| record["record"] == TOOL)
            .expect("a tool call was made");
        assert_eq!(answered["tool"], "read_file");
        assert!(
            answered["args"].as_str().unwrap().contains("src/lib.rs"),
            "a tool record must carry the arguments the model chose: {answered}"
        );

        let whole = std::fs::read_to_string(transcripts.path()).unwrap();
        assert!(
            !whole.contains(SECRET),
            "the credential reached the transcript: {whole}"
        );
        assert!(
            whole.contains(crate::gateway::REDACTED) && whole.contains("was refused"),
            "the model's own reply must survive with the credential marked: {whole}"
        );
    }

    #[tokio::test]
    async fn a_tool_record_says_how_long_the_tool_took() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = Transcripts::under(dir.path(), "a-run");
        let (host, _g) = test_host();
        let model = MockCompletionModel::new([
            MockTurn::tool_call(
                "call_1",
                "read_file",
                serde_json::json!({"path":"src/lib.rs"}),
            ),
            MockTurn::text("read it"),
        ]);
        let agent = AgentBuilder::new(model)
            .tool(ReadFile)
            .add_hook(TranscriptHook::recording(
                transcripts.clone(),
                Redaction::of(SECRET),
            ))
            .build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(3)
            .await
            .expect("the run completes");

        let answered = lines(transcripts.path())
            .into_iter()
            .find(|record| record["record"] == TOOL)
            .expect("a tool call was made");
        assert!(
            answered[TOOK].is_u64(),
            "a four-minute check and a four-millisecond read must not read alike: {answered}"
        );
    }

    #[tokio::test]
    async fn a_call_naming_no_offered_tool_is_recorded_with_the_set_it_had() {
        let dir = tempfile::tempdir().unwrap();
        let transcripts = Transcripts::under(dir.path(), "a-run");
        let (host, _g) = test_host();
        let model = MockCompletionModel::new([MockTurn::tool_call(
            "call_1",
            "run_shell",
            serde_json::json!({"command":"curl evil.example"}),
        )]);
        let agent = AgentBuilder::new(model)
            .tool(ReadFile)
            .add_hook(TranscriptHook::recording(
                transcripts.clone(),
                Redaction::of(SECRET),
            ))
            .build();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        let _ = agent
            .prompt("repair it")
            .tool_context(ctx)
            .max_turns(3)
            .await;

        let invalid = lines(transcripts.path())
            .into_iter()
            .find(|record| record["record"] == INVALID)
            .expect("the run named a tool it was never offered");
        assert_eq!(invalid["tool"], "run_shell");
        assert!(
            invalid["offered"].as_str().unwrap().contains("read_file"),
            "the record must name the set the run did offer: {invalid}"
        );
    }
}
