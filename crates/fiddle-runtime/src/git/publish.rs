//! Pushing an attempt's work to a branch, without the credential ever reaching
//! a command line.
//!
//! `git push https://<token>@github.com/...` is the obvious form and the wrong
//! one: `argv` is world-readable through `/proc/<pid>/cmdline` on Linux, so the
//! token would be visible to every process on the host for the life of the
//! push. Git documents an environment channel for exactly this —
//! `GIT_CONFIG_COUNT` with numbered `GIT_CONFIG_KEY_<n>` / `GIT_CONFIG_VALUE_<n>`
//! pairs, applied to the process's runtime configuration and overriding
//! configuration files — and the environment is readable only by the same user.
//!
//! The second pair is not decoration. An empty `credential.helper` resets the
//! helper list accumulated from every configuration file, so the push cannot
//! fall back to the operator's keychain if the header is wrong. Without it a
//! green push would prove only that the machine happened to have a credential
//! lying around; with it, a green push proves the header supplied here actually
//! worked. `docs/technical/acceptance-repository.md` makes the same move to
//! prove a credential-free clone.
//!
//! # The environment, stated once
//!
//! **Seven names, and `HOME` is not among them.** `PATH`, inherited from this
//! process or [`MINIMUM_PATH`] when it has none; `GIT_TERMINAL_PROMPT`, because
//! a prompt in an unattended run is a hang; and the five that make up the config
//! channel. `git_publish::the_push_environment_is_exactly_seven_names_and_no_home`
//! asserts that set exactly, against what the child actually received.
//!
//! `HOME`'s absence is the load-bearing line, exactly as it is for
//! [`crate::github::cli`]. It is what `git` follows to `~/.gitconfig` and
//! `~/.git-credentials`, so with it gone the emptied helper is not the only
//! thing standing between this push and a stored credential — there is nothing
//! left to stand between. Adding `HOME` back, even pointed at a scratch
//! directory, would make the guarantee a guarantee about today's `git`.
//!
//! This is a *third* environment, beside the four names a workspace check runs
//! under and the five `gh` gets. They are deliberately not reconciled: they are
//! different spawn sites with different needs, and the one change that would be
//! genuinely wrong is widening the workspace's set to make publishing work. What
//! all three share is the *bound* — process group, deadline, cancellation —
//! which lives in [`crate::process`].
//!
//! No trust-store locator is inherited, and that is a decision rather than an
//! omission: `git`'s HTTPS transport carries its own default CA path, and a
//! cleared environment was verified to complete a TLS handshake against
//! `github.com` on this project's toolchain. If a live lane ever fails to verify
//! a certificate, this paragraph is the place to revisit — the fix would be to
//! inherit a CA *path*, which is a locator and not an authority, under the same
//! rule that lets `PATH` and `RUSTUP_HOME` through elsewhere.
//!
//! # Why nothing here forces
//!
//! `git push` to a named ref is already the idempotence the milestone needs: the
//! same commit twice is `Everything up-to-date`, and a different commit is
//! refused as a non-fast-forward. That property is the reason M2's design
//! dropped a bespoke identity scheme for branches, so a forced push would not be
//! a convenience — it would silently delete the thing the milestone is built on.
//! A diverged push is therefore reported and never retried with `--force`, and
//! [`GitCli::publish`] has no parameter that could ask for one.

use crate::effect::EffectOutcome;
use crate::process::{run_bounded, Bounded};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Where to look for `git` when this process was started without a `PATH`.
///
/// The same narrowing the other two spawn sites make, for the same reason: a
/// `PATH` says *where a program is*, not *who the process may act as*.
const MINIMUM_PATH: &str = "/usr/bin:/bin";

/// The remote a publish pushes to.
///
/// Fixed rather than a parameter: the worktree an attempt works in is created
/// from the repository under repair, whose `origin` is the thing being published
/// to. A caller-supplied remote would be a second string reaching the command
/// line with nothing to validate it against.
const REMOTE: &str = "origin";

/// Which host the injected header is scoped to.
///
/// Scoped rather than global — `http.extraHeader` with no URL would attach the
/// credential to *every* HTTP request `git` makes during the push, including one
/// to a redirect target. This is the only host M2 publishes to.
const CREDENTIAL_HOST: &str = "http.https://github.com/.extraHeader";

/// What a completed publish points at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedBranch {
    /// The branch as it now exists on the remote, without the `refs/heads/`
    /// prefix.
    pub branch: String,
    /// The commit the branch points at: the worktree's `HEAD` at the moment it
    /// was pushed. Read from the repository rather than parsed out of git's
    /// report, because a `push` that had nothing to do says nothing about what
    /// the ref points at.
    pub sha: String,
}

/// Everything a publish can fail as.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// The branch name was refused before anything was spawned. See
    /// [`validate_branch`] for why this is a boundary check rather than a
    /// convention.
    #[error("branch name {branch:?} was refused: {reason}")]
    InvalidBranch { branch: String, reason: String },

    /// The remote already has a ref of this name pointing at something this
    /// worktree's `HEAD` is not a descendant of.
    ///
    /// A distinct variant because it is a distinct *fact about the world*: the
    /// branch exists and holds work this attempt does not have. Every other push
    /// failure says the push did not happen; this one says it must not.
    #[error("{branch} exists on the remote and is not an ancestor of HEAD; not forced")]
    NonFastForward { branch: String },

    /// The remote refused the ref for a reason of its own — a protected branch,
    /// a hook, a quota. Carried verbatim because git's own wording is more
    /// specific than anything this layer could say.
    #[error("the remote rejected {branch}: {reason}")]
    Rejected { branch: String, reason: String },

    /// The push failed without rejecting a ref: an unreachable remote, a
    /// credential the far end would not take, a `git` that is not a `git`.
    #[error("git push failed: {stderr}")]
    Push { stderr: String },

    /// The worktree's `HEAD` could not be read, so there is nothing to publish
    /// and nothing to report a sha for.
    #[error("git rev-parse HEAD failed: {stderr}")]
    Head { stderr: String },

    /// The runtime's own deadline passed and the process group was killed.
    #[error("git exceeded its {0:?} timeout and was killed")]
    Timeout(Duration),

    /// The attempt was cancelled, so nothing was published.
    #[error("cancelled")]
    Cancelled,

    /// The child died without answering.
    ///
    /// Note what this is *not*: git exits **128** for any ordinary fatal — an
    /// unknown remote, a repository that is not one — so the "a code at or above
    /// 128 means a signal" reading that [`crate::github::cli`] applies to `gh`
    /// would misreport a plain error here as an ambiguous death. Only an absent
    /// exit code, which is what a signal leaves behind, reaches this variant.
    #[error("git was killed before it answered")]
    Killed,
}

impl GitError {
    /// What this failure says about whether the ref was written.
    ///
    /// The counterpart of [`crate::github::GhError::outcome`], and it exists for
    /// the same reason: the executor resolves an `Unknown` by reading the world
    /// and a `NotCommitted` by letting the refusal stand, so a landed push
    /// reported as `NotCommitted` is retried into a duplicate.
    ///
    /// **The porcelain report is the refusal channel, and its absence is not a
    /// refusal.** `git push --porcelain` writes one line per ref, and a `!` line
    /// is git telling us, per ref, that the update did not happen — that is
    /// where [`GitError::NonFastForward`] and [`GitError::Rejected`] come from,
    /// and it is the only evidence this adapter ever gets that the ref was left
    /// alone. Everything else about a push is a statement about the *process*,
    /// not about the ref.
    ///
    /// So the three groups below are: nothing was spawned that could write; the
    /// remote refused this ref by name; or the ref's fate is simply not known.
    /// The last group is where a killed `git` goes, because a push killed on the
    /// way back may have delivered its pack and moved the ref already — the case
    /// Task 4 left explicitly to whoever wired this up.
    pub fn outcome(&self) -> EffectOutcome {
        match self {
            // Nothing reached the remote. `publish` validates the branch name,
            // checks cancellation and reads the local `HEAD` *before* it builds
            // the pushing child, so each of these three is a failure on the near
            // side of the only spawn that can change anything.
            GitError::InvalidBranch { .. } | GitError::Cancelled | GitError::Head { .. } => {
                EffectOutcome::NotCommitted
            }
            // The remote named this ref and said no. A divergent ref is this
            // one, and it is the whole reason M2 needs no ownership trailer:
            // git refuses the non-fast-forward, so the branch is reported and
            // never overwritten.
            GitError::NonFastForward { .. } | GitError::Rejected { .. } => {
                EffectOutcome::NotCommitted
            }
            // The deadline passed, or a signal ended the child. Neither says
            // anything about the ref, and both are the ambiguous write this
            // milestone exists for.
            GitError::Timeout(_) | GitError::Killed => EffectOutcome::Unknown,
            // A push that failed without rejecting a ref: an unreachable
            // remote, a credential the far end would not take, a connection
            // that dropped. Deliberately `Unknown` rather than `NotCommitted`,
            // even though the commonest cause never reached the remote at all —
            // git expressed no verdict on the ref, and inventing one here is
            // exactly how a landed write gets reported as a failure. The
            // postcondition read settles it either way, and settling it by
            // looking costs one `GET`.
            GitError::Push { .. } => EffectOutcome::Unknown,
        }
    }
}

/// The one place a GitHub credential is turned into a running `git`.
///
/// `program` is the operator seam, the same one `[github] cli = { program, args }`
/// and `[workspace] check = { program, args }` already offer: someone may have
/// to pin a `git` version or put a wrapper in front of it, and the deterministic
/// suite substitutes a recording `git` there. Nothing fake enters the product to
/// make that possible.
pub struct GitCli {
    program: PathBuf,
    /// The resolved credential, held as a `String` and reaching exactly one
    /// child's environment — the same shape [`crate::github::cli::GhCli`] and
    /// [`crate::gateway`] use.
    token: String,
    /// What diagnostics name. An error can say which variable was empty without
    /// ever rendering what was in it, which is the whole reason the name is
    /// carried separately from the value.
    variable: String,
    timeout: Duration,
}

/// Hand-written rather than derived, because a derived one would print `token`.
///
/// The same reasoning as [`crate::github::cli::GhCli`]'s: `{:?}` is what a
/// `dbg!`, an `unwrap` on a `Result` containing one, or a tracing attribute
/// reaches for by default, and M1 shipped a defect in exactly this class.
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
    /// Build the one `git` this process will push with.
    ///
    /// `token` is taken by value and `variable` names where it came from, the
    /// same division [`crate::github::cli::GhCli::new`] makes: the caller owns
    /// resolving the credential because it owns the configuration, and this
    /// module owns everything that happens to it afterwards.
    pub fn new(program: PathBuf, token: String, variable: &str, timeout: Duration) -> Self {
        Self {
            program,
            token,
            variable: variable.to_string(),
            timeout,
        }
    }

    /// Which variable the credential came out of. A diagnostic may name this;
    /// nothing may name its value.
    pub fn variable(&self) -> &str {
        &self.variable
    }

    /// Push `worktree`'s `HEAD` to `branch` on the remote, and report what the
    /// branch now points at.
    ///
    /// Idempotent by construction rather than by bookkeeping: the ref is named,
    /// so the same commit pushed twice is a no-op on the remote and a second
    /// identical [`PublishedBranch`] here. A different commit is refused — see
    /// [`GitError::NonFastForward`] — and never forced.
    pub async fn publish(
        &self,
        worktree: &Path,
        branch: &str,
        cancel: &CancellationToken,
    ) -> Result<PublishedBranch, GitError> {
        // Before anything is spawned, and before the name can reach a command
        // line: see `validate_branch`.
        validate_branch(branch)?;
        // Checked before spawning, not only raced against: cancellation has to
        // prevent the effect, and this is the one moment where refusing is free.
        if cancel.is_cancelled() {
            return Err(GitError::Cancelled);
        }

        // Read first, so that the sha reported is the commit that was pushed
        // rather than whatever `HEAD` became afterwards, and so that a worktree
        // with no readable `HEAD` fails before it can change a remote.
        let sha = self.head_sha(worktree, cancel).await?;

        let mut command = tokio::process::Command::new(&self.program);
        command.current_dir(worktree);
        self.common_environment(&mut command);
        // The credential channel, and the only place it is built. Five names,
        // each of which git applies to this process's configuration alone.
        command.env("GIT_CONFIG_COUNT", "2");
        command.env("GIT_CONFIG_KEY_0", CREDENTIAL_HOST);
        command.env("GIT_CONFIG_VALUE_0", self.authorization());
        command.env("GIT_CONFIG_KEY_1", "credential.helper");
        command.env("GIT_CONFIG_VALUE_1", "");
        // `--porcelain` and not the human output: a rejection has to be read
        // from a stable field rather than from a sentence git is free to
        // reword, and the sentence it currently uses is not even the same one
        // in every case ("fetch first" and "non-fast-forward" are both this).
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
            Bounded::Cancelled => return Err(GitError::Cancelled),
            Bounded::TimedOut => return Err(GitError::Timeout(self.timeout)),
            Bounded::Finished(output) => output,
        };

        // A rejection is read before the exit code, because it is the more
        // specific answer: exit 1 covers a rejected ref and a network failure
        // alike, and only the porcelain block says which.
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

    /// The commit `worktree` is sitting on.
    ///
    /// A separate child with a separate environment, and deliberately not given
    /// the credential: reading the local `HEAD` needs no authority, so it is
    /// granted none. That is what keeps the credential-carrying environment to
    /// exactly one construction in this module.
    async fn head_sha(
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
            Bounded::Cancelled => return Err(GitError::Cancelled),
            Bounded::TimedOut => return Err(GitError::Timeout(self.timeout)),
            Bounded::Finished(output) => output,
        };
        if output.status.code().is_none() {
            return Err(GitError::Killed);
        }

        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Checked rather than trusted: this string is reported as the external
        // reference an effect receipt is identified by, and a `git` that printed
        // a warning or nothing at all would otherwise have that carried forward
        // as a commit that does not exist.
        match output.status.success() && is_object_name(&sha) {
            true => Ok(sha),
            false => Err(GitError::Head {
                stderr: self.redact(&String::from_utf8_lossy(&output.stderr)),
            }),
        }
    }

    /// The two names every `git` this module spawns gets, before the push adds
    /// the five that carry the credential.
    ///
    /// `env_clear` then an allowlist, rather than removing the names somebody
    /// remembered: a denylist protects the credentials that exist today, and an
    /// allowlist protects the ones nobody has added yet. This is the same rule
    /// [`crate::workspace::command`] states at length.
    fn common_environment(&self, command: &mut tokio::process::Command) {
        command.env_clear();
        command.env(
            "PATH",
            std::env::var_os("PATH")
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| MINIMUM_PATH.into()),
        );
        // A `git` that asks a terminal it does not have for a password hangs
        // until the deadline instead of failing.
        command.env("GIT_TERMINAL_PROMPT", "0");
    }

    /// The credential, in the form the wire needs it.
    ///
    /// GitHub accepts an installation or personal token as the *password* of
    /// HTTP basic auth with the fixed username `x-access-token`, which is what
    /// the `Authorization` header below encodes. The encoding is not
    /// obfuscation — base64 is not a secret — it is the header format, and
    /// [`redact`](Self::redact) removes both this form and the raw token from
    /// anything that could become a diagnostic.
    fn authorization(&self) -> String {
        format!(
            "Authorization: Basic {}",
            base64_standard(format!("x-access-token:{}", self.token).as_bytes())
        )
    }

    /// Remove the credential from anything about to become a diagnostic.
    ///
    /// Both forms, and the encoded one is the one that matters: nothing in this
    /// module puts the raw token anywhere near a message, but the header *is*
    /// carried by the child's configuration, and `GIT_TRACE_CURL`, a wrapper, or
    /// a future `git` are all free to echo a configured value into `stderr`. M1
    /// published a gateway response body that echoed the key it had received,
    /// which is the same defect one layer up.
    fn redact(&self, text: &str) -> String {
        if self.token.is_empty() {
            return text.to_string();
        }
        let encoded = base64_standard(format!("x-access-token:{}", self.token).as_bytes());
        text.replace(&encoded, "[redacted]")
            .replace(&self.token, "[redacted]")
    }
}

/// Refuse a branch name that could mean something other than a branch name.
///
/// This is not defence against a hostile caller; it is the difference between a
/// command line that is correct for the names anybody expects and one that is
/// correct for every name. Three shapes are the reason it exists at all: a
/// leading `+` on a refspec **is** `--force`, a leading `-` is parsed as an
/// option, and a `:` splits a refspec into source and destination. Each would
/// turn `HEAD:refs/heads/<branch>` into a different instruction, and the first
/// of them would destroy the idempotence the whole milestone rests on.
///
/// The rule is an allowlist of characters plus git's own `check-ref-format`
/// restrictions that survive it, so a name nobody thought of is refused rather
/// than passed through.
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

/// Whether `text` is a full object name, which is what `rev-parse HEAD` returns
/// and what a receipt may carry as an external reference.
fn is_object_name(text: &str) -> bool {
    text.len() == 40 && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// The reason field of the first rejected ref in a `--porcelain` report.
///
/// The format is one line per ref, tab-separated: a leading flag, then
/// `<from>:<to>`, then the summary. `!` is the flag for a rejection, and the
/// summary is where the reason lives.
fn rejected_reason(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find(|line| line.starts_with('!'))
        .and_then(|line| line.split('\t').nth(2))
        .map(|summary| summary.trim().to_string())
}

/// Whether a rejection means "the remote holds work `HEAD` is not a descendant
/// of".
///
/// Three spellings, because git uses different ones for the same fact: `fetch
/// first` when the local ref simply has not seen the remote's commits, `non-fast-forward`
/// when it has and has diverged anyway, and `stale info` when a
/// `--force-with-lease` expectation missed. All three are the same refusal, and
/// a reader that only knew one of them would report a diverged push as a
/// generic failure and invite a retry.
fn is_non_fast_forward(reason: &str) -> bool {
    let reason = reason.to_ascii_lowercase();
    ["fetch first", "non-fast-forward", "stale info"]
        .iter()
        .any(|spelling| reason.contains(spelling))
}

/// Standard base64, written out rather than pulled in.
///
/// One header needs encoding, in one place, and the alternative is a dependency
/// in the crate that carries the credential — which is a supply-chain surface
/// bought for sixteen lines. The unit tests below hold it to RFC 4648's own
/// vectors, including the two padding cases, because a hand-written encoder that
/// nothing checks is worse than a dependency.
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

    /// RFC 4648 §10's own vectors, covering both padding lengths. A hand-written
    /// encoder is only defensible if it is pinned to the standard rather than to
    /// itself.
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
        // The high bits and the last two alphabet entries, which an off-by-one
        // in the shifts would leave untested.
        assert_eq!(base64_standard(&[0xfb, 0xff, 0xfe]), "+//+");
    }

    /// Both forms of the credential are removed, not only the one this module
    /// writes down.
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

    /// An empty credential must not turn every diagnostic into `[redacted]`.
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

    /// The `Debug` that must not print a credential, asserted rather than
    /// remembered.
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

    /// Both spellings git uses for the same refusal, and one it does not.
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

    /// The names that would change what the command means, and the ordinary one
    /// that must still work.
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

    /// The classification, stated exhaustively.
    ///
    /// Written as a table over every variant rather than as a few interesting
    /// cases, because the failure this guards against is a *new* variant added
    /// later and defaulted into whichever arm was written with a wildcard. There
    /// is no wildcard in [`GitError::outcome`], and this is what keeps it that
    /// way: a variant added without a decision cannot compile past here.
    ///
    /// The two rows that carry the milestone are `Killed` and `NonFastForward`.
    /// A killed push may have delivered its pack and moved the ref, so reporting
    /// it `NotCommitted` would send a caller to retry a write that landed. A
    /// non-fast-forward is git naming this ref and refusing it, which is the
    /// verdict the design leans on in place of the ownership trailer it dropped.
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
            (GitError::Cancelled, EffectOutcome::NotCommitted),
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
        ] {
            assert_eq!(error.outcome(), expected, "{error:?}");
            // The verdict has to survive the trip into the executor's own
            // vocabulary, or the classification is decided twice and the second
            // decision is the one that counts.
            assert_eq!(
                crate::github::GhError::from(error).outcome(),
                expected,
                "the wrapped failure must classify the same way"
            );
        }
    }

    /// A full object name and the shapes a `git` that answered badly would
    /// produce.
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
