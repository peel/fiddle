use crate::effect::EffectOutcome;
use crate::process::{run_bounded, Bounded};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MINIMUM_PATH: &str = "/usr/bin:/bin";

const REMOTE: &str = "origin";

const CREDENTIAL_HOST: &str = "http.https://github.com/.extraHeader";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedBranch {
    pub branch: String,
    pub sha: String,
}

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("branch name {branch:?} was refused: {reason}")]
    InvalidBranch { branch: String, reason: String },

    #[error("{branch} exists on the remote and is not an ancestor of HEAD; not forced")]
    NonFastForward { branch: String },

    #[error("the remote rejected {branch}: {reason}")]
    Rejected { branch: String, reason: String },

    #[error("git push failed: {stderr}")]
    Push { stderr: String },

    #[error("git fetch failed: {stderr}")]
    Fetch { stderr: String },

    #[error("the fetch was cancelled")]
    CancelledFetch,

    #[error("git rev-parse HEAD failed: {stderr}")]
    Head { stderr: String },

    #[error("git exceeded its {0:?} timeout and was killed")]
    Timeout(Duration),

    #[error("cancelled before anything was pushed")]
    CancelledBeforePush,

    #[error("cancelled after the push had already been started")]
    CancelledMidPush,

    #[error("git was killed before it answered")]
    Killed,
}

impl GitError {
    pub fn outcome(&self) -> EffectOutcome {
        match self {
            GitError::InvalidBranch { .. }
            | GitError::CancelledBeforePush
            | GitError::Head { .. } => EffectOutcome::NotCommitted,
            GitError::NonFastForward { .. } | GitError::Rejected { .. } => {
                EffectOutcome::NotCommitted
            }
            GitError::Timeout(_) | GitError::Killed | GitError::CancelledMidPush => {
                EffectOutcome::Unknown
            }
            GitError::Push { .. } => EffectOutcome::Unknown,
            GitError::Fetch { .. } | GitError::CancelledFetch => EffectOutcome::NotCommitted,
        }
    }
}

pub struct GitCli {
    program: PathBuf,
    token: String,
    variable: String,
    timeout: Duration,
}

impl std::fmt::Debug for GitCli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitCli")
            .field("program", &self.program)
            .field("credential_from", &self.variable)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl GitCli {
    pub fn new(program: PathBuf, token: String, variable: &str, timeout: Duration) -> Self {
        Self {
            program,
            token,
            variable: variable.to_string(),
            timeout,
        }
    }

    pub async fn publish(
        &self,
        worktree: &Path,
        branch: &str,
        cancel: &CancellationToken,
    ) -> Result<PublishedBranch, GitError> {
        validate_branch(branch)?;
        if cancel.is_cancelled() {
            return Err(GitError::CancelledBeforePush);
        }

        let sha = self.head_sha(worktree, cancel).await?;

        let mut command = tokio::process::Command::new(&self.program);
        command.current_dir(worktree);
        self.common_environment(&mut command);
        self.offer_credential(&mut command);
        command.args([
            "push",
            "--porcelain",
            REMOTE,
            &format!("HEAD:refs/heads/{branch}"),
        ]);

        let output = match run_bounded(&mut command, None, self.timeout, cancel)
            .await
            .map_err(|source| GitError::Push {
                stderr: self.redact(&format!(
                    "{} could not be run: {source}",
                    self.program.display()
                )),
            })? {
            Bounded::CancelledAfterSpawn => return Err(GitError::CancelledMidPush),
            Bounded::TimedOut => return Err(GitError::Timeout(self.timeout)),
            Bounded::Finished(output) => output,
        };

        if let Some(reason) = rejected_reason(&String::from_utf8_lossy(&output.stdout)) {
            return Err(match is_non_fast_forward(&reason) {
                true => GitError::NonFastForward {
                    branch: branch.to_string(),
                },
                false => GitError::Rejected {
                    branch: branch.to_string(),
                    reason: self.redact(&reason),
                },
            });
        }
        if output.status.code().is_none() {
            return Err(GitError::Killed);
        }
        if !output.status.success() {
            return Err(GitError::Push {
                stderr: self.redact(&String::from_utf8_lossy(&output.stderr)),
            });
        }

        Ok(PublishedBranch {
            branch: branch.to_string(),
            sha,
        })
    }

    pub async fn fetch(
        &self,
        repository: &Path,
        branch: &str,
        cancel: &CancellationToken,
    ) -> Result<(), GitError> {
        validate_branch(branch)?;
        if cancel.is_cancelled() {
            return Err(GitError::CancelledFetch);
        }

        let mut command = tokio::process::Command::new(&self.program);
        command.current_dir(repository);
        self.common_environment(&mut command);
        self.offer_credential(&mut command);
        command.args([
            "fetch",
            "--no-tags",
            "--quiet",
            REMOTE,
            &format!("+refs/heads/{branch}:refs/remotes/{REMOTE}/{branch}"),
        ]);

        let output = match run_bounded(&mut command, None, self.timeout, cancel)
            .await
            .map_err(|source| GitError::Fetch {
                stderr: self.redact(&format!(
                    "{} could not be run: {source}",
                    self.program.display()
                )),
            })? {
            Bounded::CancelledAfterSpawn => return Err(GitError::CancelledFetch),
            Bounded::TimedOut => return Err(GitError::Timeout(self.timeout)),
            Bounded::Finished(output) => output,
        };
        if output.status.code().is_none() {
            return Err(GitError::Killed);
        }
        match output.status.success() {
            true => Ok(()),
            false => Err(GitError::Fetch {
                stderr: self.redact(&String::from_utf8_lossy(&output.stderr)),
            }),
        }
    }

    pub async fn head_sha(
        &self,
        worktree: &Path,
        cancel: &CancellationToken,
    ) -> Result<String, GitError> {
        let mut command = tokio::process::Command::new(&self.program);
        command.current_dir(worktree);
        self.common_environment(&mut command);
        command.args(["rev-parse", "HEAD"]);

        let output = match run_bounded(&mut command, None, self.timeout, cancel)
            .await
            .map_err(|source| GitError::Head {
                stderr: self.redact(&format!(
                    "{} could not be run: {source}",
                    self.program.display()
                )),
            })? {
            Bounded::CancelledAfterSpawn => return Err(GitError::CancelledBeforePush),
            Bounded::TimedOut => return Err(GitError::Timeout(self.timeout)),
            Bounded::Finished(output) => output,
        };
        if output.status.code().is_none() {
            return Err(GitError::Killed);
        }

        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        match output.status.success() && is_object_name(&sha) {
            true => Ok(sha),
            false => Err(GitError::Head {
                stderr: self.redact(&String::from_utf8_lossy(&output.stderr)),
            }),
        }
    }

    fn common_environment(&self, command: &mut tokio::process::Command) {
        command.env_clear();
        command.env(
            "PATH",
            std::env::var_os("PATH")
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| MINIMUM_PATH.into()),
        );
        command.env("GIT_TERMINAL_PROMPT", "0");
    }

    fn offer_credential(&self, command: &mut tokio::process::Command) {
        command.env("GIT_CONFIG_COUNT", "2");
        command.env("GIT_CONFIG_KEY_0", CREDENTIAL_HOST);
        command.env("GIT_CONFIG_VALUE_0", self.authorization());
        command.env("GIT_CONFIG_KEY_1", "credential.helper");
        command.env("GIT_CONFIG_VALUE_1", "");
    }

    fn authorization(&self) -> String {
        format!(
            "Authorization: Basic {}",
            base64_standard(format!("x-access-token:{}", self.token).as_bytes())
        )
    }

    fn redact(&self, text: &str) -> String {
        if self.token.is_empty() {
            return text.to_string();
        }
        let encoded = base64_standard(format!("x-access-token:{}", self.token).as_bytes());
        text.replace(&encoded, "[redacted]")
            .replace(&self.token, "[redacted]")
    }
}

fn validate_branch(branch: &str) -> Result<(), GitError> {
    let refuse = |reason: &str| {
        Err(GitError::InvalidBranch {
            branch: branch.to_string(),
            reason: reason.to_string(),
        })
    };
    if branch.is_empty() {
        return refuse("it is empty");
    }
    if !branch
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
    {
        return refuse("only ASCII letters, digits and . _ - / are allowed");
    }
    if branch.starts_with(['-', '.', '/']) || branch.ends_with(['.', '/']) {
        return refuse("it must not begin with - or . or / nor end with . or /");
    }
    if branch.contains("..") || branch.contains("//") {
        return refuse("it must not contain .. or //");
    }
    if branch.ends_with(".lock") || branch.split('/').any(|part| part.ends_with(".lock")) {
        return refuse("no component may end with .lock");
    }
    Ok(())
}

fn is_object_name(text: &str) -> bool {
    text.len() == 40 && text.chars().all(|c| c.is_ascii_hexdigit())
}

fn rejected_reason(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find(|line| line.starts_with('!'))
        .and_then(|line| line.split('\t').nth(2))
        .map(|summary| summary.trim().to_string())
}

fn is_non_fast_forward(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    ["fetch first", "non-fast-forward", "stale info"]
        .iter()
        .any(|spelling| reason.contains(spelling))
}

fn base64_standard(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let bits = u32::from(chunk[0]) << 16
            | u32::from(chunk.get(1).copied().unwrap_or(0)) << 8
            | u32::from(chunk.get(2).copied().unwrap_or(0));
        let sextet = |shift: u32| ALPHABET[(bits >> shift & 0x3f) as usize] as char;
        out.push(sextet(18));
        out.push(sextet(12));
        out.push(match chunk.len() > 1 {
            true => sextet(6),
            false => '=',
        });
        out.push(match chunk.len() > 2 {
            true => sextet(0),
            false => '=',
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc_4648() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_standard(input.as_bytes()), expected, "{input:?}");
        }
        assert_eq!(base64_standard(&[0xfb, 0xff, 0xfe]), "+//+");
    }

    #[test]
    fn redaction_covers_the_token_and_the_header_that_carries_it() {
        let git = GitCli::new(
            PathBuf::from("git"),
            "ghp_sentinel".to_string(),
            "FIDDLE_GITHUB_TOKEN",
            Duration::from_secs(1),
        );
        let encoded = base64_standard(b"x-access-token:ghp_sentinel");
        let leaked = format!("fatal: Authorization: Basic {encoded} (from ghp_sentinel)");

        let redacted = git.redact(&leaked);
        assert!(!redacted.contains("ghp_sentinel"), "{redacted}");
        assert!(!redacted.contains(&encoded), "{redacted}");
        assert!(redacted.contains("[redacted]"), "{redacted}");
    }

    #[test]
    fn redaction_of_an_empty_credential_changes_nothing() {
        let git = GitCli::new(
            PathBuf::from("git"),
            String::new(),
            "FIDDLE_GITHUB_TOKEN",
            Duration::from_secs(1),
        );
        assert_eq!(
            git.redact("fatal: nothing to redact"),
            "fatal: nothing to redact"
        );
    }

    #[test]
    fn debug_names_the_variable_and_never_its_value() {
        let git = GitCli::new(
            PathBuf::from("git"),
            "ghp_sentinel".to_string(),
            "FIDDLE_GITHUB_TOKEN",
            Duration::from_secs(1),
        );
        let rendered = format!("{git:?}");
        assert!(!rendered.contains("ghp_sentinel"), "{rendered}");
        assert!(rendered.contains("FIDDLE_GITHUB_TOKEN"), "{rendered}");
    }

    #[test]
    fn a_rejection_is_read_from_the_porcelain_block() {
        let rejected = "To ../remote.git\n\
                        !\tHEAD:refs/heads/fiddle/abc\t[rejected] (fetch first)\n\
                        Done\n";
        let reason = rejected_reason(rejected).expect("a ! line is a rejection");
        assert_eq!(reason, "[rejected] (fetch first)");
        assert!(is_non_fast_forward(&reason));
        assert!(is_non_fast_forward("[rejected] (non-fast-forward)"));
        assert!(!is_non_fast_forward("[remote rejected] (pre-receive hook)"));

        assert_eq!(
            rejected_reason("To ../remote.git\n=\tHEAD:refs/heads/x\t[up to date]\nDone\n"),
            None,
            "an accepted push has no rejected ref"
        );
    }

    #[test]
    fn branch_validation_refuses_what_could_change_the_command() {
        for refused in [
            "",
            "+fiddle/abc",
            "--force",
            "-f",
            "fiddle/abc extra",
            "fiddle/abc:refs/heads/main",
            "fiddle/abc\n--force",
            "fiddle/../abc",
            "fiddle//abc",
            "/fiddle/abc",
            "fiddle/abc/",
            ".fiddle/abc",
            "fiddle/abc.",
            "fiddle/abc.lock",
            "fiddle/abc.lock/x",
            "fiddle/abc~1",
            "fiddle/abc^",
            "fiddle/ab c",
            "fiddle/\u{0}abc",
        ] {
            assert!(
                validate_branch(refused).is_err(),
                "{refused:?} must not reach a command line"
            );
        }
        for accepted in ["fiddle/abc", "main", "fiddle/repair-2024_01.v2", "a-b/c.d"] {
            assert!(validate_branch(accepted).is_ok(), "{accepted:?}");
        }
    }

    #[test]
    fn a_push_failure_says_what_it_knows_about_the_ref() {
        for (error, expected) in [
            (
                GitError::InvalidBranch {
                    branch: "+f".to_string(),
                    reason: "r".to_string(),
                },
                EffectOutcome::NotCommitted,
            ),
            (GitError::CancelledBeforePush, EffectOutcome::NotCommitted),
            (GitError::CancelledMidPush, EffectOutcome::Unknown),
            (
                GitError::Head {
                    stderr: "s".to_string(),
                },
                EffectOutcome::NotCommitted,
            ),
            (
                GitError::NonFastForward {
                    branch: "fiddle/abc".to_string(),
                },
                EffectOutcome::NotCommitted,
            ),
            (
                GitError::Rejected {
                    branch: "fiddle/abc".to_string(),
                    reason: "pre-receive hook".to_string(),
                },
                EffectOutcome::NotCommitted,
            ),
            (
                GitError::Timeout(Duration::from_secs(1)),
                EffectOutcome::Unknown,
            ),
            (GitError::Killed, EffectOutcome::Unknown),
            (
                GitError::Push {
                    stderr: "s".to_string(),
                },
                EffectOutcome::Unknown,
            ),
            (
                GitError::Fetch {
                    stderr: "s".to_string(),
                },
                EffectOutcome::NotCommitted,
            ),
            (GitError::CancelledFetch, EffectOutcome::NotCommitted),
        ] {
            assert_eq!(error.outcome(), expected, "{error:?}");
            assert_eq!(
                crate::github::GhError::from(error).outcome(),
                expected,
                "the wrapped failure must classify the same way"
            );
        }
    }

    #[test]
    fn only_a_full_object_name_is_reported_as_a_sha() {
        assert!(is_object_name("0123456789abcdef0123456789abcdef01234567"));
        assert!(!is_object_name(""));
        assert!(!is_object_name("0123456"));
        assert!(!is_object_name(
            "warning: something\n0123456789abcdef0123456789abcdef0123456"
        ));
        assert!(!is_object_name("z123456789abcdef0123456789abcdef01234567"));
    }
}
