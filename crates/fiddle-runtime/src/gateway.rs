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

pub fn completion_model(
    base_url: &str,
    api_key: String,
    variable: &str,
    model: &str,
) -> Result<GatewayModel, GatewayError> {
    let client = openai::Client::builder()
        .api_key(api_key)
        .base_url(base_url)
        .build()
        .map_err(|_| GatewayError {
            base_url: base_url.to_string(),
            variable: variable.to_string(),
        })?
        .completions_api();
    Ok(client.completion_model(model))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
