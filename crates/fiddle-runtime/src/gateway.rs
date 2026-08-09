//! The one construction of a model that talks to a real provider.
//!
//! Everything else in this crate is generic over Rig's own
//! [`CompletionModel`](rig_core::completion::CompletionModel), which is what
//! lets the whole of M1's central property be proven offline against a scripted
//! model. This module is the other end of that: the single place where a
//! concrete, credential-carrying model is built, so the seam has exactly one
//! production implementation and the tests need none of it.
//!
//! It lives in `fiddle-runtime` rather than in the binary because design §6.1
//! states that every Rig import lives here. The CLI resolves the credential —
//! it owns the configuration, so it owns the environment lookup — and hands the
//! resolved value straight in. Nothing in this module stores it: it goes into
//! the client's `Authorization` header and the `String` is consumed.

use rig_core::client::CompletionClient;
use rig_core::providers::openai;

/// The model this build talks to a gateway through.
///
/// Named so a caller can hold one without spelling Rig's generic client types,
/// and so the whole of "which provider integration is in use" is one line that
/// a future change has to edit deliberately.
pub type GatewayModel = openai::completion::CompletionModel;

/// The gateway client could not be built from what the configuration named.
///
/// One variant, and it is deliberately terse about *why*. The only ways
/// building can fail are a base URL or a credential that cannot become an HTTP
/// header, and the credential is the second of those — so the underlying error
/// is not carried onto this type at all. Rig's own error does not quote the
/// header value it rejected, but "does not today" is not the guarantee this
/// wants: the caller renders whatever it is given, and the caller is rendering
/// it for an operator who may be reading a CI log.
///
/// What survives is what an operator can act on: which endpoint was configured
/// and which variable held the credential. Both are names, and neither is the
/// secret.
#[derive(Debug, thiserror::Error)]
#[error(
    "a model client for {base_url} could not be built from the credential in \
     {variable}"
)]
pub struct GatewayError {
    pub base_url: String,
    pub variable: String,
}

/// A completion model for `model`, served by the OpenAI-compatible endpoint at
/// `base_url`, authenticated with `api_key`.
///
/// `variable` names the environment variable `api_key` came out of and is used
/// for nothing but the diagnostic — it is what tells an operator which of their
/// variables to go and fix, without the failure having to quote the value to be
/// specific about it.
///
/// # Why the chat-completions API
///
/// The gateway routes both `/v1/chat/completions` and `/v1/responses`; Rig's
/// default OpenAI client targets the latter. Chat-completions is chosen because
/// it is the more exercised translation path from an OpenAI-shaped request to a
/// non-OpenAI upstream — which is exactly what a gateway fronting Anthropic
/// models is doing — and this milestone's tool-calling and structured-output
/// behaviour has to survive that translation.
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

    /// Building a client is offline: nothing is dialled until a completion is
    /// requested, which is what lets the CLI refuse a misconfigured deployment
    /// before it does any work.
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

    /// A credential that cannot become a header is refused, and the refusal
    /// carries the variable's *name* and nothing of its value.
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
