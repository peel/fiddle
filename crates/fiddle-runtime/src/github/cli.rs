//! The one place a GitHub credential is turned into a running process.
//!
//! This module stands to the GitHub token exactly as [`crate::gateway`] stands
//! to the model key: one construction site, so "where could this credential
//! go?" has one answer. The environment below is the whole of what the child
//! sees.
//!
//! **The allowlist is five names, and this is the statement of it.** `PATH`,
//! inherited from this process or [`MINIMUM_PATH`] when it has none; `GH_TOKEN`,
//! the resolved credential; `GH_CONFIG_DIR`, pointed at a scratch directory so
//! the configuration source is pinned; `GH_PROMPT_DISABLED`, because a prompt in
//! an unattended run is a hang; and `NO_COLOR`, because ANSI escapes in output
//! that is going to be parsed are a defect waiting to be written.
//! `github_cli::the_gh_environment_is_exactly_five_names_and_no_home` asserts
//! that set exactly, against what the child actually received, so a sixth name
//! cannot arrive without an assertion changing.
//!
//! This is a *different* set from the four names a workspace check runs under
//! (`HOME`, `LANG`, `PATH`, `RUSTUP_HOME`), and the two are deliberately not
//! reconciled. They are different spawn sites with different needs, and the one
//! thing that would be genuinely wrong is widening the workspace's set to make
//! GitHub work. What the two share is the *bound* — the process group, the
//! deadline, the cancellation — which lives in [`crate::process`] and is
//! written once.
//!
//! # `HOME` is absent, and that is the load-bearing line
//!
//! With no `HOME` and a `GH_CONFIG_DIR` pointing at an empty directory, `gh`
//! answers "To get started with GitHub CLI, please run: gh auth login" rather
//! than reaching the operator's keyring. So "this adapter used the credential it
//! was given and no other" is a fact about the process rather than a promise in
//! a comment. Adding `HOME` back — even pointed at a scratch directory — would
//! reopen `~/.config/gh`, and the guarantee would quietly become a guarantee
//! about today's `gh`.
//!
//! # Why the status is parsed rather than inferred
//!
//! `gh help exit-codes` documents the whole set: **0** success, **1** any
//! failure, **2** cancelled, **4** authentication required. A 404, a 422 and a
//! 500 are all exit 1, so the HTTP status is simply not in the exit code. Every
//! call is therefore `gh api -i`, whose first line of stdout is the status line,
//! and [`GhCli::api`] reads the status from there. A branch that decided
//! anything about the response from exit 1 would have read the wrong surface.
//!
//! `gh` also has no timeout flag, so the runtime owns the deadline — which is
//! not only a cost. A `gh` killed after it has dispatched a request is a real
//! ambiguous write rather than a simulated one, and [`GhError::outcome`] is what
//! keeps it from being reported as a failure.

use crate::effect::EffectOutcome;
use crate::git::GitError;
use crate::process::{run_bounded, Bounded};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Where to look for `gh` when this process was started without a `PATH`.
///
/// The same narrowing the workspace runner makes, for the same reason: a `PATH`
/// says *where a program is*, not *who the process may act as*, so inheriting
/// one grants the child nothing it could not reach by absolute path. The
/// credential is the thing that stays under this module's control.
const MINIMUM_PATH: &str = "/usr/bin:/bin";

/// What `gh api -i` said, once the status line and the headers have been read
/// off the front of it.
#[derive(Debug)]
pub struct GhResponse {
    /// From the status line, never from the exit code.
    pub status: u16,
    /// The response body, parsed. `Null` when there was none — a 204 from a
    /// workflow dispatch is the ordinary case.
    pub body: serde_json::Value,
    /// `Retry-After`, when the response carried one.
    pub retry_after: Option<Duration>,
    /// `X-RateLimit-Remaining`, when the response carried one.
    pub rate_limit_remaining: Option<u64>,
}

/// What a response said about being asked the same question again.
///
/// The two headers were parsed off `gh api -i` from the first day of this
/// milestone and read by nobody, which is the defect ADR 013 had to price for
/// `agent.max_capability_attempts`: a value that parses, defaults and reaches
/// nothing looks like a feature and behaves like a comment. This type is the
/// channel that carries them somewhere — out of [`GhResponse`], into
/// [`GhError::Http`], and from there into the postcondition read's wait.
///
/// It is carried on the *error* as well as on the response because that is
/// where it matters: a 429, or a secondary-rate-limit 403, is a response the
/// client is being told to ask for again later, and the client only ever sees
/// those as a [`GhError`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RetryAdvice {
    /// `Retry-After`, when the response carried one. An instruction with a
    /// number attached, and the only wait in this system that GitHub chose
    /// rather than fiddle.
    pub retry_after: Option<Duration>,
    /// `X-RateLimit-Remaining`, when the response carried one.
    pub rate_limit_remaining: Option<u64>,
}

impl RetryAdvice {
    /// Whether the response asked to be left alone for a while.
    ///
    /// A `Retry-After` says so outright. A remaining budget of zero says the
    /// same thing without a number: the refusal is about the *credential's
    /// allowance*, which refills, rather than about the request, which does
    /// not. Both make a 403 that would otherwise be final into one worth
    /// looking past — and that distinction is this field's only job, so a
    /// deployment can tell "you may not" from "not just now".
    pub fn wants_a_wait(&self) -> bool {
        self.retry_after.is_some() || self.rate_limit_remaining == Some(0)
    }
}

/// Everything a `gh` invocation can fail as.
///
/// The variants exist to be *classified*, not merely reported: see
/// [`GhError::outcome`], which is where each one commits to whether the request
/// it describes may have changed the world.
#[derive(Debug, thiserror::Error)]
pub enum GhError {
    /// Exit 4. `gh` had no usable credential, so nothing was sent.
    #[error("gh could not authenticate (exit 4)")]
    Auth,
    /// Exit 2, or a cancellation this runtime raised before spawning.
    #[error("gh was cancelled (exit 2)")]
    Cancelled,
    /// The runtime's own deadline passed and the process group was killed.
    #[error("gh exceeded its {0:?} timeout and was killed")]
    Timeout(Duration),
    /// The child died without answering — a signal, or an exit code above 128.
    /// Distinct from [`GhError::Timeout`] because the runtime did not choose it,
    /// and classified `Unknown` for the same reason: the request may have
    /// landed.
    #[error("gh was killed before it answered (status {0})")]
    Killed(String),
    /// A response arrived and carried a status at or above 400.
    ///
    /// `advice` is what that response said about being asked again. It is not
    /// in the [`std::fmt::Display`] rendering on purpose: the message is what
    /// reaches an operator and a published bundle, and "wait two seconds" is an
    /// instruction to the client rather than a description of the failure.
    #[error("HTTP {status}: {message}")]
    Http {
        status: u16,
        message: String,
        advice: RetryAdvice,
    },
    /// Something came back that is not a response. This is the runner or the
    /// binary being wrong, not GitHub refusing — see [`GhError::outcome`] for
    /// why the difference matters more than it looks.
    #[error("gh output could not be parsed: {0}")]
    Malformed(String),
    /// More objects matched than the postcondition allows. Reported, never
    /// resolved by picking the first.
    #[error("{count} objects matched where at most one was expected")]
    Duplicate { count: usize },
    /// A push, which is `git` rather than `gh`.
    ///
    /// [`IntegrationOperation`](crate::effect::IntegrationOperation) fixes one
    /// error type for every operation's mutation, and `ensure_branch_published`
    /// is the one whose mutation is not an API call: a ref can only point at an
    /// object the remote already has, so the objects and the ref are published
    /// together by [`GitCli::publish`](crate::git::GitCli::publish) and there is
    /// no `POST /git/refs` to fail instead.
    ///
    /// The [`GitError`] is carried whole rather than flattened into a message,
    /// and that is the point of the variant. [`GhError::outcome`] delegates to
    /// [`GitError::outcome`], so the judgment about whether a failed push may
    /// have moved the ref is made by the type that knows git's refusal channel —
    /// rather than by mapping a push failure onto an HTTP status nobody sent.
    #[error("the branch could not be pushed: {0}")]
    Push(#[from] GitError),
}

impl GhError {
    /// What this failure says about whether the request changed anything.
    ///
    /// A lost answer is `Unknown`; an explicit refusal is `NotCommitted`. The
    /// milestone turns on getting these two apart, because `Unknown` sends the
    /// caller to read the world and `NotCommitted` sends it to retry — and a
    /// landed write reported as `NotCommitted` is retried into a duplicate.
    pub fn outcome(&self) -> EffectOutcome {
        match self {
            // The answer was lost. The write may or may not have landed.
            //
            // `Killed` is the state the exactly-once harness deliberately
            // creates: the scripted `gh` applies its mutation and *then* exits
            // as though killed. Classifying it with `Malformed` would report a
            // write that really landed as a write that never happened, and the
            // retry would perform it a second time — the exact duplicate this
            // milestone exists to prevent.
            GhError::Timeout(_) | GhError::Killed(_) => EffectOutcome::Unknown,
            // GitHub failed after receiving the request. Whether it got far
            // enough to act is not something a 5xx tells anyone.
            GhError::Http { status, .. } if *status >= 500 => EffectOutcome::Unknown,
            // 422 covers malformed input, invalid ref syntax, spam protection
            // and "already exists" — a refusal and a success wearing the same
            // number. It is never classified on its face; being `Unknown` is
            // what forces the caller into the postcondition read that can
            // actually tell those apart.
            GhError::Http { status: 422, .. } => EffectOutcome::Unknown,
            // Every other 4xx is GitHub saying it declined, in terms that leave
            // no room for it having acted anyway.
            GhError::Http { .. } => EffectOutcome::NotCommitted,
            // Two objects where one was expected means an earlier write is
            // unaccounted for; that is not a settled world.
            GhError::Duplicate { .. } => EffectOutcome::Unknown,
            // Nothing was sent (`Auth`, `Cancelled`), or something came back
            // that was never a response at all (`Malformed`) — which means the
            // process ran to a normal completion and produced garbage, rather
            // than dying mid-flight. That is a broken runner, not an ambiguous
            // write.
            GhError::Auth | GhError::Cancelled | GhError::Malformed(_) => {
                EffectOutcome::NotCommitted
            }
            // Delegated rather than restated: git's refusal channel is the
            // porcelain report, and only `GitError` knows which of its variants
            // came from one.
            GhError::Push(error) => error.outcome(),
        }
    }

    /// What the response said about being asked again, if it said anything.
    ///
    /// Total over the variants rather than a match at each call site: every
    /// failure has to answer "how long should I wait", and the ones that never
    /// carried a header answer it with [`RetryAdvice::default`] — which is not
    /// silence, it is "nothing was said", and the backoff supplies its own
    /// number for that case.
    pub fn advice(&self) -> RetryAdvice {
        match self {
            GhError::Http { advice, .. } => *advice,
            _ => RetryAdvice::default(),
        }
    }

    /// Whether asking the same question again could plausibly answer it
    /// differently.
    ///
    /// **This is asked only of a read.** Nothing consults it about a mutation,
    /// and the reason is the rule the whole milestone rests on: a read is
    /// idempotent, so looking again costs nothing, while re-dispatching a write
    /// whose answer was lost is how a duplicate external effect is born. See
    /// [`Executor::execute`](crate::effect::Executor::execute).
    pub fn is_worth_reading_again(&self) -> bool {
        match self {
            // The answer was lost on the way back. The next one may arrive.
            GhError::Timeout(_) | GhError::Killed(_) => true,
            // GitHub said so itself — either with a `Retry-After` or by
            // reporting the credential's allowance spent. This arm is ahead of
            // the status arms because it is the one that separates a
            // secondary-rate-limit 403, which passes, from a permissions 403,
            // which does not.
            GhError::Http { advice, .. } if advice.wants_a_wait() => true,
            // 429 is the rate limit saying so without a header; 5xx is GitHub
            // failing at something that is not about this request.
            GhError::Http { status, .. } => *status == 429 || *status >= 500,
            // Everything else is settled. A refusal stays refused, a cancelled
            // run must stop rather than wait, a broken runner does not repair
            // itself, and a second matching object does not become one object
            // by being looked at again.
            GhError::Auth
            | GhError::Cancelled
            | GhError::Malformed(_)
            | GhError::Duplicate { .. }
            | GhError::Push(_) => false,
        }
    }
}

/// A `gh` that carries one credential and runs under one environment.
///
/// `program` and `args` are the operator seam — `[github] cli = { program,
/// args }` exists because someone may have to pin a `gh` version or put a
/// wrapper in front of it, and it is the same seam `[workspace] check` already
/// offers. The deterministic suite substitutes a scripted `gh` there; nothing
/// fake enters the product to make that possible.
pub struct GhCli {
    program: PathBuf,
    args: Vec<String>,
    /// The resolved credential, held as a `String` and passed to one child's
    /// environment — the same shape [`crate::gateway`] uses for the model key,
    /// rather than a wrapper type this workspace does not have.
    token: String,
    /// What diagnostics name. An error can say which variable was empty without
    /// ever rendering what was in it, which is the whole reason the name is
    /// carried separately from the value.
    variable: String,
    config_dir: PathBuf,
    timeout: Duration,
}

/// Hand-written rather than derived, because a derived one would print
/// `token`.
///
/// This is not paranoia about a field nobody prints: `{:?}` on a struct is what
/// a `dbg!`, an `unwrap` on a `Result<_, _>` containing one, or a tracing
/// attribute reaches for by default, and M1 shipped a defect in exactly this
/// class — a response body that echoed the received key reached a published
/// bundle. The variable's *name* is here because that is the actionable half.
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
    /// Build the one `gh` this process will run.
    ///
    /// `token` is taken by value and `variable` names where it came from, the
    /// same division [`crate::gateway::completion_model`] makes: the caller owns
    /// resolving the credential because it owns the configuration, and this
    /// module owns everything that happens to it afterwards.
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

    /// Which variable the credential came out of. A diagnostic may name this;
    /// nothing may name its value.
    pub fn variable(&self) -> &str {
        &self.variable
    }

    /// One `gh api -i` call.
    ///
    /// `body`, when present, is written to the child's stdin behind `--input -`
    /// rather than passed as an argument. That is not only tidiness: `argv` is
    /// world-readable through `/proc/<pid>/cmdline` on Linux, and a request body
    /// is the kind of thing that grows a credential-shaped field later without
    /// anybody revisiting how it is passed.
    pub async fn api(
        &self,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
        cancel: &CancellationToken,
    ) -> Result<GhResponse, GhError> {
        // Checked before spawning, not only raced against: cancellation has to
        // prevent the effect, and this is the one moment where refusing is free.
        if cancel.is_cancelled() {
            return Err(GhError::Cancelled);
        }

        let mut command = tokio::process::Command::new(&self.program);
        command.env_clear();
        // A locator may be inherited, an authority may not — M1's rule, applied
        // at a second spawn site.
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
        // Nothing else. A sixth entry here is a change to the security boundary
        // and has to break `the_gh_environment_is_exactly_five_names_and_no_home`
        // before it can land.

        command.args(&self.args);
        command
            .arg("api")
            .arg("-i")
            .arg("--method")
            .arg(method)
            .arg(path);
        let stdin = body.map(|body| {
            command.arg("--input").arg("-");
            body.to_string().into_bytes()
        });

        // The deadline is the runtime's because `gh` has no flag for one. Own
        // process group and cancellation come with it — see [`crate::process`].
        let bounded = run_bounded(&mut command, stdin, self.timeout, cancel)
            .await
            .map_err(|source| {
                GhError::Malformed(self.redact(&format!(
                    "{} could not be run: {source}",
                    self.program.display()
                )))
            })?;

        match bounded {
            Bounded::Cancelled => Err(GhError::Cancelled),
            Bounded::TimedOut => Err(GhError::Timeout(self.timeout)),
            Bounded::Finished(output) => self.parse(&output),
        }
    }

    /// Turn a finished `gh` into a response or into a classified failure.
    ///
    /// The order of the arms is the argument. The exit code is consulted only
    /// for the three things it actually reports — authentication, cancellation,
    /// and the child having died rather than answered — and the HTTP status is
    /// read from the status line for everything else. Exit **1** deliberately
    /// falls through to the status line, because that single code covers a 404,
    /// a 422 and a 500 alike.
    fn parse(&self, output: &std::process::Output) -> Result<GhResponse, GhError> {
        match output.status.code() {
            Some(0) => {}
            Some(2) => return Err(GhError::Cancelled),
            Some(4) => return Err(GhError::Auth),
            // Nobody chose this one. `code()` is `None` when a signal ended the
            // process; a code at or above 128 is the shell's spelling of the
            // same thing, and a `gh` wrapper is exactly the sort of thing that
            // reports a killed child that way. Both reach `Killed`, and through
            // it `Unknown`, because a child that died on the way back tells us
            // nothing about whether the request landed.
            None => return Err(GhError::Killed("signal".to_string())),
            Some(code) if code >= 128 => return Err(GhError::Killed(code.to_string())),
            // Exit 1 and any other code: the response, if there is one, says
            // what happened.
            Some(_) => {}
        }

        let text = String::from_utf8_lossy(&output.stdout);
        let (head, body) = match text.split_once("\r\n\r\n") {
            Some(split) => split,
            // A `gh` that normalized its line endings is still answering the
            // question; refusing to read it would turn a cosmetic difference
            // into an unresolved outcome.
            None => text.split_once("\n\n").unwrap_or((text.as_ref(), "")),
        };

        let mut lines = head.lines();
        let status_line = lines.next().unwrap_or_default();
        // Checked rather than assumed: without this, a `gh` that printed a
        // warning first would have its second token parsed as a status, and the
        // adapter would report a number nobody sent.
        if !status_line.starts_with("HTTP/") {
            // `stderr` is quoted only here, and only because this is the one
            // failure an operator cannot diagnose without it: when `program` is
            // not the `gh` it was configured to be, stdout is usually empty and
            // the reason is on the other stream. It is redacted and bounded like
            // everything else that can reach a log.
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
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            // Header names are case-insensitive and HTTP/2 lowercases them,
            // so matching the spelling GitHub's documentation uses would work
            // over HTTP/1.1 and silently stop working over HTTP/2.
            match name.trim().to_ascii_lowercase().as_str() {
                "retry-after" => retry_after = value.parse().ok().map(Duration::from_secs),
                "x-ratelimit-remaining" => rate_limit_remaining = value.parse().ok(),
                _ => {}
            }
        }

        let body = parse_body(body).map_err(|reason| GhError::Malformed(self.redact(&reason)))?;

        // Assembled before the status is judged, so that both exits carry the
        // same parsed headers. The failure exit reads them straight back off
        // this value — which is the whole point: until this line the two header
        // fields were parsed and dropped, and a response a client is told to
        // retry is precisely the one that never reached the `Ok` arm where they
        // lived.
        let response = GhResponse {
            status,
            body,
            retry_after,
            rate_limit_remaining,
        };

        if response.status >= 400 {
            return Err(GhError::Http {
                status: response.status,
                // GitHub's error envelope, when there is one. The whole body is
                // deliberately not carried: it is the surface that reaches a
                // published bundle, and M1 already shipped one defect of that
                // shape.
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

    /// Remove the credential from anything about to become a diagnostic.
    ///
    /// Belt and braces: nothing in this module puts the token into a message on
    /// purpose, and this is what makes that true of the messages it did not
    /// write — a response body, a spawn error naming an environment. The failure
    /// this guards against is not hypothetical; M1 published a gateway response
    /// body that echoed the key it had received.
    fn redact(&self, text: &str) -> String {
        match self.token.is_empty() {
            true => text.to_string(),
            false => text.replace(&self.token, "[redacted]"),
        }
    }
}

/// A response body, or `Null` when there was none.
///
/// An empty body is ordinary — `POST .../dispatches` answers 204 with nothing —
/// so it is not a parse failure. A non-empty body that is not JSON is: this
/// client only ever asks for JSON, so anything else means the thing on the far
/// end of `program` is not the `gh` it was configured to be.
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

    /// A finished `gh`, as the parser sees one. Exit 1 rather than 0, because
    /// that is what `gh` really exits with for every HTTP failure and the
    /// status must come off the status line regardless.
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

    /// The two headers were parsed from the first day of this milestone and
    /// read by nobody. This is the half of their first consumer that can be
    /// asserted against a real response: a rate-limited read is precisely the
    /// one that never reaches the `Ok` arm the fields lived on, so a client
    /// that only carried them on success carried them nowhere.
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

    /// A refusal that said nothing about waiting says nothing about waiting.
    ///
    /// The contrast is the point: without it, an implementation that reported
    /// every failure as retryable would pass the case above.
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

    /// The headers still reach a successful response, unchanged. `Retry-After`
    /// on a 2xx is GitHub asking a client to come back for an answer it is
    /// still preparing, and dropping it here would move the defect rather than
    /// fix it.
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

/// A bounded quotation of something unparseable, so a diagnostic can be
/// specific without pasting an unbounded response into a log.
fn snippet(text: &str) -> String {
    const LIMIT: usize = 120;
    let text = text.trim();
    match text.char_indices().nth(LIMIT) {
        Some((end, _)) => format!("{:?}…", &text[..end]),
        None => format!("{text:?}"),
    }
}
