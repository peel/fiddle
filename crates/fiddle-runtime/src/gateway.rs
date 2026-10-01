use rig_core::client::CompletionClient;
use rig_core::completion::{CompletionError, CompletionRequest, CompletionResponse};
use rig_core::providers::{anthropic, openai};
use rig_core::streaming::StreamingCompletionResponse;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    #[default]
    ChatCompletions,
    Messages,
}

impl Protocol {
    pub fn name(self) -> &'static str {
        match self {
            Protocol::ChatCompletions => "chat-completions",
            Protocol::Messages => "messages",
        }
    }

    pub fn caches(self) -> bool {
        matches!(self, Protocol::Messages)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Thinking {
    #[default]
    Default,
    Disabled,
}

impl Thinking {
    pub fn name(self) -> &'static str {
        match self {
            Thinking::Default => "default",
            Thinking::Disabled => "disabled",
        }
    }
}

#[derive(Clone)]
pub enum GatewayModel {
    ChatCompletions(openai::completion::CompletionModel),
    Messages(anthropic::completion::CompletionModel, Thinking),
}

fn without_thinking(mut request: CompletionRequest) -> CompletionRequest {
    let disabled = serde_json::json!({ "type": "disabled" });
    request.additional_params = match request.additional_params.take() {
        Some(serde_json::Value::Object(mut held)) => {
            held.insert("thinking".to_string(), disabled);
            Some(serde_json::Value::Object(held))
        }
        _ => Some(serde_json::json!({ "thinking": disabled })),
    };
    request
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum GatewayResponse {
    Messages(anthropic::completion::CompletionResponse),
    ChatCompletions(openai::completion::CompletionResponse),
}

fn carried<T>(
    answered: CompletionResponse<T>,
    wrap: impl FnOnce(T) -> GatewayResponse,
) -> CompletionResponse<GatewayResponse> {
    CompletionResponse {
        choice: answered.choice,
        usage: answered.usage,
        raw_response: wrap(answered.raw_response),
        message_id: answered.message_id,
    }
}

pub const STREAMING_UNSUPPORTED: &str =
    "this build does not stream a completion, so a streamed request is refused rather than \
     answered in part";

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct NeverStreamed;

impl rig_core::completion::GetTokenUsage for NeverStreamed {
    fn token_usage(&self) -> rig_core::completion::Usage {
        rig_core::completion::Usage::new()
    }
}

impl rig_core::completion::CompletionModel for GatewayModel {
    type Response = GatewayResponse;
    type StreamingResponse = NeverStreamed;
    type Client = ();

    fn make(_client: &Self::Client, _model: impl Into<String>) -> Self {
        unreachable!(
            "a gateway model is built by `completion_model`, which reads the credential once"
        )
    }

    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse<GatewayResponse>, CompletionError> {
        match self {
            GatewayModel::ChatCompletions(model) => {
                let answered = model.completion(request).await?;
                Ok(carried(answered, GatewayResponse::ChatCompletions))
            }
            GatewayModel::Messages(model, thinking) => {
                let request = match thinking {
                    Thinking::Default => request,
                    Thinking::Disabled => without_thinking(request),
                };
                let answered = model.completion(request).await?;
                Ok(carried(answered, GatewayResponse::Messages))
            }
        }
    }

    fn composes_native_output_with_tools(&self) -> bool {
        match self {
            GatewayModel::ChatCompletions(model) => model.composes_native_output_with_tools(),
            GatewayModel::Messages(model, _) => model.composes_native_output_with_tools(),
        }
    }

    async fn stream(
        &self,
        _request: CompletionRequest,
    ) -> Result<StreamingCompletionResponse<NeverStreamed>, CompletionError> {
        Err(CompletionError::ProviderError(
            STREAMING_UNSUPPORTED.to_string(),
        ))
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "a model client for {base_url} could not be built from the credential in \
     {variable}"
)]
pub struct GatewayError {
    pub base_url: String,
    pub variable: String,
}

pub const REDACTED: &str = "[redacted]";

const EXCERPT_LIMIT: usize = 240;

#[derive(Clone, Default)]
pub struct Redaction {
    credential: Option<String>,
}

impl Redaction {
    pub fn of(credential: &str) -> Self {
        match credential.is_empty() {
            true => Redaction::unknown(),
            false => Redaction {
                credential: Some(credential.to_string()),
            },
        }
    }

    pub fn unknown() -> Self {
        Redaction { credential: None }
    }

    pub fn excerpt(&self, text: &str) -> Option<String> {
        let held = self.redacted(text, EXCERPT_LIMIT)?;
        Some(match held.cut {
            true => format!("{:?}\u{2026}", held.text),
            false => format!("{:?}", held.text),
        })
    }

    pub fn redacted(&self, text: &str, limit: usize) -> Option<Redacted> {
        let credential = self.credential.as_deref()?;
        let replaced = text.replace(credential, REDACTED);
        Some(cut(replaced.trim(), limit))
    }
}

pub struct Redacted {
    pub text: String,

    pub cut: bool,
}

impl std::fmt::Debug for Redaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let held = match self.credential {
            Some(_) => "a credential",
            None => "no credential",
        };
        write!(f, "Redaction({held})")
    }
}

fn cut(text: &str, limit: usize) -> Redacted {
    match text.char_indices().nth(limit) {
        Some((end, _)) => Redacted {
            text: text[..end].to_string(),
            cut: true,
        },
        None => Redacted {
            text: text.to_string(),
            cut: false,
        },
    }
}

pub struct Gateway {
    pub model: GatewayModel,
    pub redaction: Redaction,
}

pub fn completion_model(
    protocol: Protocol,
    thinking: Thinking,
    base_url: &str,
    api_key: String,
    variable: &str,
    model: &str,
) -> Result<Gateway, GatewayError> {
    let redaction = Redaction::of(&api_key);
    let unbuilt = || GatewayError {
        base_url: base_url.to_string(),
        variable: variable.to_string(),
    };
    let model = match protocol {
        Protocol::ChatCompletions => GatewayModel::ChatCompletions(
            openai::Client::builder()
                .api_key(api_key)
                .base_url(base_url)
                .build()
                .map_err(|_| unbuilt())?
                .completions_api()
                .completion_model(model),
        ),
        Protocol::Messages => GatewayModel::Messages(
            anthropic::Client::builder()
                .api_key(api_key)
                .base_url(base_url)
                .build()
                .map_err(|_| unbuilt())?
                .completion_model(model)
                .with_prompt_caching(),
            thinking,
        ),
    };
    Ok(Gateway { model, redaction })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "sk-unit-must-not-appear-2b71";

    #[test]
    fn a_model_is_built_without_reaching_the_endpoint() {
        assert!(
            completion_model(
                Protocol::ChatCompletions,
                Thinking::Default,
                "http://127.0.0.1:9/v1",
                "not-a-real-credential".to_string(),
                "LITELLM_API_KEY",
                "a-model",
            )
            .is_ok(),
            "a well-formed endpoint and credential build a model with nothing \
             listening at the far end"
        );
    }

    #[test]
    fn a_credential_that_cannot_be_a_header_is_refused_without_being_quoted() {
        let secret = "sk-secret\nvalue";
        let Err(error) = completion_model(
            Protocol::ChatCompletions,
            Thinking::Default,
            "http://127.0.0.1:9/v1",
            secret.to_string(),
            "LITELLM_API_KEY",
            "a-model",
        ) else {
            panic!("a header value cannot carry a newline, so no client can be built")
        };

        let rendered = format!("{error}\n{error:?}");
        assert!(
            !rendered.contains("sk-secret"),
            "the refusal repeated the credential: {rendered}"
        );
        assert!(
            rendered.contains("LITELLM_API_KEY"),
            "the refusal must name the variable to fix: {rendered}"
        );
    }

    #[test]
    fn the_model_and_the_redaction_come_from_one_read_of_the_credential() {
        let gateway = completion_model(
            Protocol::ChatCompletions,
            Thinking::Default,
            "http://127.0.0.1:9/v1",
            SECRET.to_string(),
            "LITELLM_API_KEY",
            "a-model",
        )
        .expect("a well-formed endpoint and credential build a model");

        let excerpt = gateway
            .redaction
            .excerpt(&format!("Incorrect API key provided: {SECRET}"))
            .expect("the redaction holds the credential the client was given");
        assert!(
            !excerpt.contains(SECRET),
            "the excerpt kept the credential the client authenticates with: {excerpt}"
        );
        assert!(
            excerpt.contains(REDACTED),
            "the excerpt must mark where the credential was: {excerpt}"
        );
    }

    #[test]
    fn an_unknown_credential_yields_no_excerpt() {
        assert_eq!(
            Redaction::unknown().excerpt("Incorrect API key provided: sk-anything"),
            None,
            "a redaction that holds no credential cannot promise a safe excerpt"
        );
        assert_eq!(
            Redaction::of("").excerpt("Incorrect API key provided: sk-anything"),
            None,
            "an empty credential matches every position, so it redacts nothing"
        );
    }

    #[test]
    fn an_excerpt_replaces_every_copy_of_the_credential() {
        let redaction = Redaction::of(SECRET);
        let excerpt = redaction
            .excerpt(&format!("{SECRET} was sent and {SECRET} was refused"))
            .expect("a known credential yields an excerpt");
        assert!(
            !excerpt.contains(SECRET),
            "one copy of the credential survived: {excerpt}"
        );
        assert_eq!(
            excerpt.matches(REDACTED).count(),
            2,
            "both copies must be marked: {excerpt}"
        );
    }

    #[test]
    fn an_excerpt_is_bounded_and_quoted() {
        let redaction = Redaction::of(SECRET);
        let long = "x".repeat(EXCERPT_LIMIT * 2);
        let excerpt = redaction
            .excerpt(&long)
            .expect("a known credential yields an excerpt");
        assert!(
            excerpt.ends_with('…'),
            "a body past the bound is cut and marked as cut: {excerpt}"
        );
        assert!(
            excerpt.matches('x').count() == EXCERPT_LIMIT,
            "the bound is {EXCERPT_LIMIT} characters: {excerpt}"
        );

        let newline = redaction
            .excerpt("first\nsecond")
            .expect("a known credential yields an excerpt");
        assert_eq!(
            newline, "\"first\\nsecond\"",
            "an excerpt is escaped, so a body cannot forge a second line: {newline}"
        );
    }

    #[test]
    fn a_redaction_never_renders_the_credential_it_holds() {
        let rendered = format!("{:?}", Redaction::of(SECRET));
        assert!(
            !rendered.contains(SECRET),
            "the redaction printed the credential it exists to hide: {rendered}"
        );
    }
}
