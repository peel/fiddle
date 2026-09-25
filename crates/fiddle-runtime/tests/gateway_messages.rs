use fiddle_runtime::{completion_model, GatewayModel, Protocol};
use rig_core::completion::{CompletionModel, Message, ToolDefinition};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

struct Sent {
    head: String,
    body: serde_json::Value,
}

impl Sent {
    fn path(&self) -> &str {
        self.head
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("")
    }

    fn carries_header(&self, name: &str) -> bool {
        self.head
            .lines()
            .any(|line| line.to_ascii_lowercase().starts_with(&format!("{name}:")))
    }

    fn breakpoints(&self) -> usize {
        self.body.to_string().matches("\"cache_control\"").count()
    }
}

async fn answering(answer: serde_json::Value) -> (String, tokio::task::JoinHandle<Sent>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port is free");
    let address = listener.local_addr().expect("the port has an address");
    let served = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("the client connects");
        let mut read = Vec::new();
        let mut chunk = [0u8; 8192];
        let (head, body) = loop {
            let n = socket
                .read(&mut chunk)
                .await
                .expect("the request is readable");
            assert!(n > 0, "the client closed before sending a whole request");
            read.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&read).to_string();
            let Some(end) = text.find("\r\n\r\n") else {
                continue;
            };
            let head = text[..end].to_string();
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            if read.len() >= end + 4 + length {
                break (head, read[end + 4..end + 4 + length].to_vec());
            }
        };
        let reply = answer.to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
             connection: close\r\n\r\n{reply}",
            reply.len()
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("the answer is writable");
        Sent {
            head,
            body: serde_json::from_slice(&body).expect("the request body is JSON"),
        }
    });
    (format!("http://{address}"), served)
}

fn messages_answer(cache_read: u64) -> serde_json::Value {
    serde_json::json!({
        "id": "msg_stub",
        "type": "message",
        "role": "assistant",
        "model": "a-model",
        "content": [{"type": "text", "text": "done"}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": {
            "input_tokens": 12,
            "output_tokens": 3,
            "cache_read_input_tokens": cache_read,
            "cache_creation_input_tokens": 0,
        },
    })
}

fn chat_answer() -> serde_json::Value {
    serde_json::json!({
        "id": "chatcmpl-stub",
        "object": "chat.completion",
        "created": 0,
        "model": "a-model",
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": "done"},
            "finish_reason": "stop",
        }],
        "usage": {"prompt_tokens": 12, "completion_tokens": 3, "total_tokens": 15},
    })
}

fn model(protocol: Protocol, base_url: &str) -> GatewayModel {
    completion_model(
        protocol,
        base_url,
        "sk-loopback-only".to_string(),
        "LITELLM_API_KEY",
        "a-model",
    )
    .expect("a well-formed endpoint and credential build a model")
    .model
}

fn a_tool(name: &str) -> ToolDefinition {
    ToolDefinition {
        name: name.to_string(),
        description: format!("the {name} tool"),
        parameters: serde_json::json!({"type": "object", "properties": {}}),
    }
}

async fn a_turn_with_history(
    model: GatewayModel,
) -> rig_core::completion::CompletionResponse<fiddle_runtime::GatewayResponse> {
    model
        .completion_request(Message::user("the third message"))
        .preamble("the system prompt".to_string())
        .messages([
            Message::user("the first message"),
            Message::assistant("the second message"),
        ])
        .tool(a_tool("read_file"))
        .tool(a_tool("search_files"))
        .max_tokens(16)
        .send()
        .await
        .expect("the loopback answers")
}

#[tokio::test]
async fn a_messages_request_marks_the_system_prompt_the_last_tool_and_the_last_message() {
    let (base_url, served) = answering(messages_answer(0)).await;
    a_turn_with_history(model(Protocol::Messages, &base_url)).await;
    let sent = served.await.expect("the loopback served one request");

    assert_eq!(sent.path(), "/v1/messages", "{}", sent.head);
    assert!(
        sent.carries_header("x-api-key"),
        "the Messages protocol authenticates with x-api-key: {}",
        sent.head
    );

    let body = &sent.body;
    let system = body["system"]
        .as_array()
        .expect("the system prompt is blocks");
    assert!(
        system
            .last()
            .is_some_and(|block| block["cache_control"]["type"] == "ephemeral"),
        "the system prompt carries a breakpoint, so every turn reads it back: {body}"
    );
    let tools = body["tools"].as_array().expect("the tools are sent");
    assert!(
        tools
            .last()
            .is_some_and(|tool| tool["cache_control"]["type"] == "ephemeral"),
        "the last tool carries a breakpoint, so the whole tool list is cached: {body}"
    );
    let messages = body["messages"].as_array().expect("the history is sent");
    let last = messages.last().expect("the turn carries a message");
    assert!(
        last["content"]
            .as_array()
            .and_then(|content| content.last())
            .is_some_and(|block| block["cache_control"]["type"] == "ephemeral"),
        "the last message carries a breakpoint, so the next turn reads this history from \
         the cache instead of paying for it again: {body}"
    );
    assert!(
        !messages[0].to_string().contains("cache_control"),
        "the first message carries none, so the row is not passing on a request that marks \
         every block: {body}"
    );
    assert!(
        (3..=4).contains(&sent.breakpoints()),
        "the API allows four breakpoints and refuses a fifth: {} in {body}",
        sent.breakpoints()
    );
}

#[tokio::test]
async fn a_chat_completions_request_carries_no_breakpoint() {
    let (base_url, served) = answering(chat_answer()).await;
    a_turn_with_history(model(Protocol::ChatCompletions, &base_url)).await;
    let sent = served.await.expect("the loopback served one request");

    assert!(
        sent.path().ends_with("/chat/completions"),
        "the default protocol is unchanged: {}",
        sent.head
    );
    assert_eq!(
        sent.breakpoints(),
        0,
        "the gateway loses a breakpoint on this route, so none is sent: {}",
        sent.body
    );
}

#[tokio::test]
async fn a_base_url_written_for_chat_completions_still_reaches_the_messages_route() {
    let (base_url, served) = answering(messages_answer(0)).await;
    a_turn_with_history(model(Protocol::Messages, &format!("{base_url}/v1"))).await;
    let sent = served.await.expect("the loopback served one request");

    assert_eq!(
        sent.path(),
        "/v1/messages",
        "a deployment that kept its `/v1` suffix must not send `/v1/v1/messages`"
    );
}

#[tokio::test]
async fn a_messages_answer_reports_what_it_read_from_the_cache() {
    let (base_url, served) = answering(messages_answer(1_800)).await;
    let answered = a_turn_with_history(model(Protocol::Messages, &base_url)).await;
    served.await.expect("the loopback served one request");

    assert_eq!(answered.usage.cached_input_tokens, 1_800);
    assert_eq!(
        answered.usage.total_tokens, 1_815,
        "the total counts what was read from the cache, so a bound on it still sees the \
         whole context"
    );
}

#[tokio::test]
async fn a_streamed_request_is_refused_rather_than_answered_in_part() {
    let model = model(Protocol::Messages, "http://127.0.0.1:9");
    let request = model
        .completion_request(Message::user("stream this"))
        .build();
    let Err(refused) = model.stream(request).await else {
        panic!("this build does not stream, so a streamed request must fail");
    };
    assert!(
        refused.to_string().contains("does not stream"),
        "the refusal says why: {refused}"
    );
}

#[tokio::test]
#[ignore = "spends against a real gateway; run by name with LITELLM_API_KEY and FIDDLE_GATEWAY_URL exported"]
async fn a_real_gateway_reads_back_the_prefix_the_turn_before_it_cached() {
    let (Ok(credential), Ok(base_url)) = (
        std::env::var("LITELLM_API_KEY"),
        std::env::var("FIDDLE_GATEWAY_URL"),
    ) else {
        panic!("export LITELLM_API_KEY and FIDDLE_GATEWAY_URL to run this lane");
    };
    let model_name = std::env::var("FIDDLE_GATEWAY_MODEL").unwrap_or("claude-sonnet-5".into());
    let model = completion_model(
        Protocol::Messages,
        &base_url,
        credential,
        "LITELLM_API_KEY",
        &model_name,
    )
    .expect("the gateway model builds")
    .model;
    let unique = format!("{:?}", std::time::SystemTime::now());
    let preamble = format!(
        "{unique} {}",
        "A reviewer reads this paragraph before it answers. ".repeat(220)
    );

    let mut read = Vec::new();
    for _ in 0..2 {
        let answered = model
            .completion_request(Message::user("say ok"))
            .preamble(preamble.clone())
            .tool(a_tool("read_file"))
            .max_tokens(8)
            .send()
            .await
            .expect("the gateway answers");
        read.push((
            answered.usage.cache_creation_input_tokens,
            answered.usage.cached_input_tokens,
        ));
    }

    assert!(
        read[0].0 > 1_024,
        "the first turn wrote the prefix to the cache: {read:?}"
    );
    assert!(
        read[1].1 >= read[0].0,
        "the second turn read back what the first wrote, so this gateway caches on this \
         route: {read:?}"
    );
}

#[test]
fn both_protocols_keep_the_providers_own_answer_on_output_and_tools() {
    for protocol in [Protocol::ChatCompletions, Protocol::Messages] {
        assert!(
            model(protocol, "http://127.0.0.1:9").composes_native_output_with_tools(),
            "{protocol:?}: the wrapper must forward the provider's answer, because rig's \
             default is false and changes how a typed prompt is sent"
        );
    }
}
