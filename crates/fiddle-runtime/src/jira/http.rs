use crate::gateway::REDACTED;
use crate::jira::JiraError;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::header::{HeaderValue, ACCEPT, AUTHORIZATION};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const CLAMP: usize = 2048;

#[derive(Debug)]
pub struct JiraResponse {
    pub status: u16,

    pub body: serde_json::Value,
}

pub struct JiraHttp {
    client: reqwest::Client,
    base_url: String,
    credential: Credential,
    timeout: Duration,
}

impl JiraHttp {
    pub fn new(
        base_url: &str,
        user: &str,
        token: &str,
        timeout: Duration,
    ) -> Result<Self, JiraError> {
        Ok(Self {
            client: reqwest::Client::builder().build().map_err(|_| {
                JiraError::Unreachable("no http client could be built for this site".to_string())
            })?,
            base_url: base_url.trim_end_matches('/').to_string(),
            credential: Credential::basic(user, token)?,
            timeout,
        })
    }

    pub async fn api(
        &self,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
        cancel: &CancellationToken,
    ) -> Result<JiraResponse, JiraError> {
        if cancel.is_cancelled() {
            return Err(JiraError::Unreachable("cancelled".to_string()));
        }
        let sending = method.parse::<reqwest::Method>().map_err(|_| {
            JiraError::Malformed(format!("`{method}` is not a method this client sends"))
        })?;
        let request = self
            .client
            .request(sending, format!("{}{path}", self.base_url))
            .header(AUTHORIZATION, self.credential.header.clone())
            .header(ACCEPT, "application/json")
            .timeout(self.timeout);
        let request = match body {
            Some(value) => request.json(value),
            None => request,
        };
        let round_trip = async {
            let answered = request.send().await.map_err(|e| self.unreachable(&e))?;
            let status = answered.status().as_u16();
            let text = answered.text().await.map_err(|e| self.unreachable(&e))?;
            Ok::<(u16, String), JiraError>((status, text))
        };
        let (status, text) = tokio::select! {
            _ = cancel.cancelled() => return Err(JiraError::Unreachable("cancelled".to_string())),
            answered = round_trip => answered?,
        };
        let parsed = match text.trim().is_empty() {
            true => Some(serde_json::Value::Null),
            false => serde_json::from_str(&text).ok(),
        };
        let body = match parsed {
            Some(body) => self.credential.scrubbed(body),
            None if (200..300).contains(&status) => return Err(self.malformed(status, &text)),
            None => serde_json::Value::Null,
        };
        Ok(JiraResponse { status, body })
    }

    pub fn quoted(&self, body: &serde_json::Value) -> Option<String> {
        let spoken: Vec<&str> = body["errorMessages"]
            .as_array()?
            .iter()
            .filter_map(|held| held.as_str())
            .filter(|held| !held.trim().is_empty())
            .collect();
        match spoken.is_empty() {
            true => None,
            false => Some(self.quotable(&spoken.join("; "))),
        }
    }

    fn unreachable(&self, error: &reqwest::Error) -> JiraError {
        JiraError::Unreachable(self.quotable(&error.to_string()))
    }

    fn malformed(&self, status: u16, text: &str) -> JiraError {
        JiraError::Malformed(format!("HTTP {status}: {}", self.quotable(text)))
    }

    pub fn quotable(&self, text: &str) -> String {
        clamp(&self.credential.redacted(text))
    }
}

struct Credential {
    header: HeaderValue,
    encoded: String,
    token: String,
}

impl Credential {
    fn basic(user: &str, token: &str) -> Result<Self, JiraError> {
        let encoded = BASE64.encode(format!("{user}:{token}"));
        let mut header = HeaderValue::from_str(&format!("Basic {encoded}")).map_err(|_| {
            JiraError::Unreachable("the credential could not become a header".to_string())
        })?;
        header.set_sensitive(true);
        Ok(Self {
            header,
            encoded,
            token: token.to_string(),
        })
    }

    fn redacted(&self, text: &str) -> String {
        [self.encoded.as_str(), self.token.as_str()]
            .into_iter()
            .filter(|held| !held.is_empty())
            .fold(text.to_string(), |text, held| text.replace(held, REDACTED))
    }

    fn scrubbed(&self, body: serde_json::Value) -> serde_json::Value {
        match body {
            serde_json::Value::String(held) => serde_json::Value::String(self.redacted(&held)),
            serde_json::Value::Array(held) => {
                serde_json::Value::Array(held.into_iter().map(|each| self.scrubbed(each)).collect())
            }
            serde_json::Value::Object(held) => serde_json::Value::Object(
                held.into_iter()
                    .map(|(name, value)| (self.redacted(&name), self.scrubbed(value)))
                    .collect(),
            ),
            held => held,
        }
    }
}

fn clamp(text: &str) -> String {
    match text.len() <= CLAMP {
        true => text.to_string(),
        false => {
            let mut at = CLAMP;
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            format!("{}\u{2026} ({} bytes elided)", &text[..at], text.len() - at)
        }
    }
}
