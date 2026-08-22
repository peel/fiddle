use rig_core::client::CompletionClient;
use rig_core::providers::openai;

pub type GatewayModel = openai::completion::CompletionModel;

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
        let credential = self.credential.as_deref()?;
        Some(bounded(&text.replace(credential, REDACTED)))
    }
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

fn bounded(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(EXCERPT_LIMIT) {
        Some((end, _)) => format!("{:?}…", &text[..end]),
        None => format!("{text:?}"),
    }
}

pub struct Gateway {
    pub model: GatewayModel,
    pub redaction: Redaction,
}

pub fn completion_model(
    base_url: &str,
    api_key: String,
    variable: &str,
    model: &str,
) -> Result<Gateway, GatewayError> {
    let redaction = Redaction::of(&api_key);
    let client = openai::Client::builder()
        .api_key(api_key)
        .base_url(base_url)
        .build()
        .map_err(|_| GatewayError {
            base_url: base_url.to_string(),
            variable: variable.to_string(),
        })?
        .completions_api();
    Ok(Gateway {
        model: client.completion_model(model),
        redaction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "sk-unit-must-not-appear-2b71";

    #[test]
    fn a_model_is_built_without_reaching_the_endpoint() {
        assert!(
            completion_model(
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
