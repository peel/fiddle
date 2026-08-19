use crate::effect::EffectOutcome;
use crate::git::GitError;
use crate::process::{run_bounded, Bounded};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MINIMUM_PATH: &str = "/usr/bin:/bin";

#[derive(Debug)]
pub struct GhResponse {
    pub status: u16,
    pub body: serde_json::Value,
    pub retry_after: Option<Duration>,
    pub rate_limit_remaining: Option<u64>,
    pub link: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetryAdvice {
    pub retry_after: Option<Duration>,
    pub rate_limit_remaining: Option<u64>,
}

impl RetryAdvice {
    pub fn wants_a_wait(&self) -> bool {
        self.retry_after.is_some() || self.rate_limit_remaining == Some(0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GhError {
    #[error("gh could not authenticate (exit 4)")]
    Auth,
    #[error("gh was cancelled before it was started")]
    CancelledBeforeSpawn,
    #[error("gh was cancelled after it had already been started")]
    CancelledAfterSpawn,
    #[error("gh exceeded its {0:?} timeout and was killed")]
    Timeout(Duration),
    #[error("gh was killed before it answered (status {0})")]
    Killed(String),
    #[error("nothing was sent: {0}")]
    NotSent(String),
    #[error("HTTP {status}: {message}")]
    Http {
        status: u16,
        message: String,
        advice: RetryAdvice,
    },
    #[error("GraphQL {kind}: {message}")]
    GraphQl { kind: String, message: String },
    #[error("gh output could not be parsed: {0}")]
    Malformed(String),
    #[error("{count} objects matched where at most one was expected")]
    Duplicate { count: usize },
    #[error("the branch could not be pushed: {0}")]
    Push(#[from] GitError),
}

impl GhError {
    pub fn outcome(&self) -> EffectOutcome {
        match self {
            GhError::Timeout(_) | GhError::Killed(_) | GhError::CancelledAfterSpawn => {
                EffectOutcome::Unknown
            }
            GhError::Http { status, .. } if *status >= 500 => EffectOutcome::Unknown,
            GhError::Http { status: 422, .. } => EffectOutcome::Unknown,
            GhError::Http { .. } => EffectOutcome::NotCommitted,
            GhError::GraphQl { kind, .. } => match kind.as_str() {
                "NOT_FOUND" | "FORBIDDEN" => EffectOutcome::NotCommitted,
                _ => EffectOutcome::Unknown,
            },
            GhError::Duplicate { .. } => EffectOutcome::Unknown,
            GhError::Malformed(_) => EffectOutcome::Unknown,
            GhError::Auth | GhError::CancelledBeforeSpawn | GhError::NotSent(_) => {
                EffectOutcome::NotCommitted
            }
            GhError::Push(error) => error.outcome(),
        }
    }

    pub fn advice(&self) -> RetryAdvice {
        match self {
            GhError::Http { advice, .. } => *advice,
            _ => RetryAdvice::default(),
        }
    }

    pub fn is_worth_reading_again(&self) -> bool {
        match self {
            GhError::Timeout(_) | GhError::Killed(_) | GhError::CancelledAfterSpawn => true,
            GhError::Http { advice, .. } if advice.wants_a_wait() => true,
            GhError::Http { status, .. } => *status == 429 || *status >= 500,
            GhError::GraphQl { .. } => self.outcome() == EffectOutcome::Unknown,
            GhError::Auth
            | GhError::CancelledBeforeSpawn
            | GhError::NotSent(_)
            | GhError::Malformed(_)
            | GhError::Duplicate { .. }
            | GhError::Push(_) => false,
        }
    }
}

pub struct GhCli {
    program: PathBuf,
    args: Vec<String>,
    token: String,
    variable: String,
    config_dir: PathBuf,
    timeout: Duration,
}

impl std::fmt::Debug for GhCli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GhCli")
            .field("program", &self.program)
            .field("args", &self.args)
            .field("credential_from", &self.variable)
            .field("config_dir", &self.config_dir)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl GhCli {
    pub fn new(
        program: PathBuf,
        args: Vec<String>,
        token: String,
        variable: &str,
        config_dir: PathBuf,
        timeout: Duration,
    ) -> Self {
        Self {
            program,
            args,
            token,
            variable: variable.to_string(),
            config_dir,
            timeout,
        }
    }

    pub fn variable(&self) -> &str {
        &self.variable
    }

    pub async fn api(
        &self,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
        cancel: &CancellationToken,
    ) -> Result<GhResponse, GhError> {
        let mut command = self.command();
        command.arg("--method").arg(method).arg(path);
        let stdin = body.map(|body| {
            command.arg("--input").arg("-");
            body.to_string().into_bytes()
        });
        self.dispatch(&mut command, stdin, cancel).await
    }

    pub async fn graphql(
        &self,
        query: &str,
        variables: &[(&str, &str)],
        cancel: &CancellationToken,
    ) -> Result<serde_json::Value, GhError> {
        let mut command = self.command();
        command
            .arg("graphql")
            .arg("-f")
            .arg(format!("query={query}"));
        for (name, value) in variables {
            command.arg("-f").arg(format!("{name}={value}"));
        }

        let response = self.dispatch(&mut command, None, cancel).await?;

        if !response.body.is_object() {
            return Err(GhError::GraphQl {
                kind: UNKNOWN_ERROR_TYPE.to_string(),
                message: self.redact(&format!(
                    "the response body was {} rather than an object, so it said \
                     neither data nor errors",
                    shape(&response.body)
                )),
            });
        }

        match refusal(&response.body) {
            Some((kind, message)) => Err(GhError::GraphQl {
                kind,
                message: self.redact(&message),
            }),
            None => Ok(response.body["data"].clone()),
        }
    }

    fn command(&self) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.program);
        command.env_clear();
        command.env(
            "PATH",
            std::env::var_os("PATH")
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| MINIMUM_PATH.into()),
        );
        command.env("GH_TOKEN", &self.token);
        command.env("GH_CONFIG_DIR", &self.config_dir);
        command.env("GH_PROMPT_DISABLED", "1");
        command.env("NO_COLOR", "1");

        command.args(&self.args);
        command.arg("api").arg("-i");
        command
    }

    async fn dispatch(
        &self,
        command: &mut tokio::process::Command,
        stdin: Option<Vec<u8>>,
        cancel: &CancellationToken,
    ) -> Result<GhResponse, GhError> {
        if cancel.is_cancelled() {
            return Err(GhError::CancelledBeforeSpawn);
        }

        let bounded = run_bounded(command, stdin, self.timeout, cancel)
            .await
            .map_err(|source| {
                GhError::Malformed(self.redact(&format!(
                    "{} could not be run: {source}",
                    self.program.display()
                )))
            })?;

        match bounded {
            Bounded::CancelledAfterSpawn => Err(GhError::CancelledAfterSpawn),
            Bounded::TimedOut => Err(GhError::Timeout(self.timeout)),
            Bounded::Finished(output) => self.parse(&output),
        }
    }

    fn parse(&self, output: &std::process::Output) -> Result<GhResponse, GhError> {
        match output.status.code() {
            Some(0) => {}
            Some(2) => return Err(GhError::CancelledAfterSpawn),
            Some(4) => return Err(GhError::Auth),
            None => return Err(GhError::Killed("signal".to_string())),
            Some(code) if code >= 128 => return Err(GhError::Killed(code.to_string())),
            Some(_) => {}
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let (head, body) = match text.split_once("\r\n\r\n") {
            Some(split) => split,
            None => text.split_once("\n\n").unwrap_or((text.as_ref(), "")),
        };

        let mut lines = head.lines();
        let status_line = lines.next().unwrap_or_default();
        if !status_line.starts_with("HTTP/") {
            return Err(GhError::Malformed(self.redact(&format!(
                "no HTTP status line in {} (stderr: {})",
                snippet(&text),
                snippet(&String::from_utf8_lossy(&output.stderr)),
            ))));
        }
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| {
                GhError::Malformed(
                    self.redact(&format!("unreadable status line {}", snippet(status_line))),
                )
            })?;

        let mut retry_after = None;
        let mut rate_limit_remaining = None;
        let mut link = None;
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            match name.trim().to_ascii_lowercase().as_str() {
                "retry-after" => retry_after = value.parse().ok().map(Duration::from_secs),
                "x-ratelimit-remaining" => rate_limit_remaining = value.parse().ok(),
                "link" => link = Some(value.to_string()),
                _ => {}
            }
        }

        let body = parse_body(body).map_err(|reason| GhError::Malformed(self.redact(&reason)))?;

        let response = GhResponse {
            status,
            body,
            retry_after,
            rate_limit_remaining,
            link,
        };

        if response.status >= 400 {
            return Err(GhError::Http {
                status: response.status,
                message: self.redact(
                    response.body["message"]
                        .as_str()
                        .unwrap_or("no message in the response body"),
                ),
                advice: RetryAdvice {
                    retry_after: response.retry_after,
                    rate_limit_remaining: response.rate_limit_remaining,
                },
            });
        }

        Ok(response)
    }

    fn redact(&self, text: &str) -> String {
        match self.token.is_empty() {
            true => text.to_string(),
            false => text.replace(&self.token, "[redacted]"),
        }
    }
}

const UNKNOWN_ERROR_TYPE: &str = "UNKNOWN";

fn refusal(body: &serde_json::Value) -> Option<(String, String)> {
    let errors = match &body["errors"] {
        serde_json::Value::Null => return None,
        serde_json::Value::Array(errors) if errors.is_empty() => return None,
        serde_json::Value::Array(errors) => errors,
        _ => {
            return Some((
                UNKNOWN_ERROR_TYPE.to_string(),
                "the response carried an errors field that is not an array".to_string(),
            ))
        }
    };

    let first = &errors[0];
    Some((
        first["type"]
            .as_str()
            .unwrap_or(UNKNOWN_ERROR_TYPE)
            .to_string(),
        first["message"]
            .as_str()
            .unwrap_or("no message in the error")
            .to_string(),
    ))
}

fn shape(body: &serde_json::Value) -> &'static str {
    match body {
        serde_json::Value::Null => "empty or null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

fn parse_body(body: &str) -> Result<serde_json::Value, String> {
    let body = body.trim();
    if body.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    serde_json::from_str(body).map_err(|error| format!("body is not JSON ({error})"))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn answered(head: &str, body: &str) -> std::process::Output {
        std::process::Output {
            status: std::process::ExitStatus::from_raw(1 << 8),
            stdout: format!("{head}\r\n\r\n{body}").into_bytes(),
            stderr: Vec::new(),
        }
    }

    fn client() -> GhCli {
        GhCli::new(
            PathBuf::from("/nonexistent/gh"),
            Vec::new(),
            String::new(),
            "GH_TOKEN",
            PathBuf::from("/nonexistent"),
            Duration::from_secs(1),
        )
    }

    #[test]
    fn a_rate_limited_response_carries_its_headers_into_the_error() {
        let error = client()
            .parse(&answered(
                "HTTP/2.0 429 Too Many Requests\r\n\
                 Retry-After: 2\r\n\
                 X-RateLimit-Remaining: 0",
                r#"{"message":"API rate limit exceeded"}"#,
            ))
            .expect_err("a 429 is a failure");

        assert_eq!(
            error.advice(),
            RetryAdvice {
                retry_after: Some(Duration::from_secs(2)),
                rate_limit_remaining: Some(0),
            },
            "got {error:?}"
        );
        assert!(
            error.is_worth_reading_again(),
            "and the advice must be what makes it worth another look"
        );
    }

    #[test]
    fn a_refusal_with_no_headers_advises_nothing() {
        let error = client()
            .parse(&answered(
                "HTTP/2.0 403 Forbidden",
                r#"{"message":"Resource not accessible by integration"}"#,
            ))
            .expect_err("a 403 is a failure");

        assert_eq!(error.advice(), RetryAdvice::default(), "got {error:?}");
        assert!(!error.is_worth_reading_again());
    }

    #[test]
    fn a_successful_response_still_carries_its_headers() {
        let response = client()
            .parse(&answered(
                "HTTP/2.0 202 Accepted\r\nRetry-After: 5\r\nX-RateLimit-Remaining: 4999",
                "",
            ))
            .expect("a 202 is a response");

        assert_eq!(response.status, 202);
        assert_eq!(response.retry_after, Some(Duration::from_secs(5)));
        assert_eq!(response.rate_limit_remaining, Some(4999));
    }
}

fn snippet(text: &str) -> String {
    const LIMIT: usize = 120;
    let text = text.trim();
    match text.char_indices().nth(LIMIT) {
        Some((end, _)) => format!("{:?}…", &text[..end]),
        None => format!("{text:?}"),
    }
}
