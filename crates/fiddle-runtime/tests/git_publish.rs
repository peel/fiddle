//! Publishing a branch: where the credential goes, and what a second push does.
//!
//! Everything here is offline and credential-free. Two fixtures carry it, and
//! the split between them is the point rather than an implementation detail.
//!
//! A **recording `git`** — the fixture in `tests/git_stub/`, reached through
//! the same `program` seam an operator would use to pin or wrap `git` — answers
//! the questions that are about *what the child was given*: which arguments,
//! which environment. Those cannot be asked of a real `git`, because a real
//! `git` does not report what it received; it reports what it did.
//!
//! A **real `git` against a bare repository on disk** answers the questions that
//! are about *what actually happened to a remote*: that a second push of the
//! same commit leaves one branch and not two, and that a diverged push is
//! refused rather than forced. Those cannot be asked of a fixture, because a
//! fixture would be answering from this test's own assumptions about git's
//! behaviour — which is exactly the thing under test.
//!
//! Nothing reaches GitHub, no credential is needed, and no network is used. The
//! `extraHeader` the push carries is inert against a path remote, which is what
//! makes it honest to send the real one: the same environment the product builds
//! is the one these pushes run under.

use fiddle_runtime::effect::EffectOutcome;
use fiddle_runtime::git::{GitCli, GitError};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// A generous bound for children that answer immediately. Nothing here is about
/// the deadline; the process-level bounds are `github_cli`'s subject and this
/// module inherits them rather than restating them.
const PATIENT: Duration = Duration::from_secs(60);

/// What the recording `git` answers `rev-parse HEAD` with, so that a fixture
/// which never touches a repository can still complete a publish. Kept in step
/// with `tests/git_stub/git_stub.rs` by the assertions that read it back.
const STUB_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

// ---------------------------------------------------------------------------
// The recording `git`
// ---------------------------------------------------------------------------

/// What one invocation of the recording `git` received.
struct Recorded {
    argv: Vec<String>,
    env: BTreeMap<String, String>,
}

/// A `GitCli` pointed at the recording `git`, publishing out of `dir`.
///
/// `dir` is both the worktree the adapter is asked to publish and the place the
/// fixture records into, because the working directory is the only channel left:
/// the environment is pinned to seven names and the argument vector is asserted
/// exactly. See `tests/git_stub/git_stub.rs`.
fn recording_git(dir: &Path, token: &str, mode: &str) -> GitCli {
    std::fs::write(dir.join("mode"), mode).unwrap();
    GitCli::new(
        PathBuf::from(env!("CARGO_BIN_EXE_git_stub")),
        token.to_string(),
        "FIDDLE_GITHUB_TOKEN",
        PATIENT,
    )
}

/// Read back what one invocation of the recording `git` was given.
fn recorded(dir: &Path, subcommand: &str) -> Recorded {
    let record = std::fs::read_to_string(dir.join(format!("{subcommand}.json")))
        .unwrap_or_else(|_| panic!("the recording git was never asked to {subcommand}"));
    let record: serde_json::Value = serde_json::from_str(&record).unwrap();
    Recorded {
        argv: record["argv"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| arg.as_str().unwrap().to_string())
            .collect(),
        env: record["env"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| {
                let (name, value) = entry.as_str().unwrap().split_once('=').unwrap();
                (name.to_string(), value.to_string())
            })
            .collect(),
    }
}

/// Run one publish against a recording `git` and return what the push received.
async fn push_against_recording_git(token: &str) -> (Recorded, Result<(), GitError>) {
    let dir = TempDir::new().unwrap();
    let outcome = recording_git(dir.path(), token, "accepted")
        .publish(dir.path(), "fiddle/abc", &CancellationToken::new())
        .await;
    (recorded(dir.path(), "push"), outcome.map(|_| ()))
}

/// Decode standard base64, independently of the encoder under test.
///
/// Written out rather than compared against a re-encoding of the expected
/// value, because a test that encoded with the same function it is checking
/// would agree with any encoder, correct or not. Decoding asserts the thing the
/// criterion is actually about: the sentinel is *in* the environment.
fn base64_decode(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut bits = 0u32;
    let mut have = 0u32;
    let mut out = Vec::new();
    for byte in text.bytes().filter(|byte| *byte != b'=') {
        let value = ALPHABET
            .iter()
            .position(|candidate| *candidate == byte)
            .unwrap_or_else(|| panic!("{byte:?} is not standard base64"))
            as u32;
        bits = (bits << 6) | value;
        have += 6;
        if have >= 8 {
            have -= 8;
            out.push((bits >> have) as u8);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Real git, against a bare repository on disk
// ---------------------------------------------------------------------------

/// Run a setup `git` in `dir` and insist it succeeded.
///
/// Setup runs under the ambient environment on purpose — it is the test
/// arranging a world, not the code under test — but identity and the initial
/// branch are pinned with `-c` so that an operator's global configuration
/// cannot change what the fixture is.
fn git_setup(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.email=fiddle@example.invalid",
            "-c",
            "user.name=fiddle",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git is on PATH for the test process");
    assert!(
        output.status.success(),
        "setup `git {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// An empty bare repository to push into. This is the whole of the "remote".
fn bare_repository(dir: &Path) -> PathBuf {
    let remote = dir.join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    git_setup(&remote, &["init", "-q", "--bare", "."]);
    remote
}

/// A working repository with one commit whose content is `content`, and an
/// `origin` pointing at `remote`.
fn worktree_with_one_commit(dir: &Path, name: &str, remote: &Path, content: &str) -> PathBuf {
    let work = dir.join(name);
    std::fs::create_dir_all(&work).unwrap();
    git_setup(&work, &["init", "-q", "."]);
    std::fs::write(work.join("file"), content).unwrap();
    git_setup(&work, &["add", "file"]);
    git_setup(&work, &["commit", "-q", "-m", "one"]);
    git_setup(
        &work,
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    work
}

/// Every branch the bare repository holds, in ref order.
fn branches(remote: &Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["for-each-ref", "--format=%(refname:short)", "refs/heads/"])
        .current_dir(remote)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// What one branch of the bare repository points at.
fn remote_sha(remote: &Path, branch: &str) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", &format!("refs/heads/{branch}")])
        .current_dir(remote)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The product's own publisher, pointed at the real `git` and carrying a token
/// that is never used, because a path remote authenticates nobody.
fn real_git() -> GitCli {
    GitCli::new(
        PathBuf::from("git"),
        "ghp_never_used_by_a_path_remote".to_string(),
        "FIDDLE_GITHUB_TOKEN",
        PATIENT,
    )
}

// ---------------------------------------------------------------------------
// What the child was given
// ---------------------------------------------------------------------------

/// `/proc/<pid>/cmdline` is world-readable on Linux. The environment is not.
/// This is the whole reason the config channel is used instead of a URL.
///
/// The second half of the assertion is not the sentinel appearing verbatim
/// among the environment's values: HTTP basic auth is base64, so what the
/// environment carries is `x-access-token:<sentinel>` encoded. The test decodes
/// it rather than searching for a substring, which is the stronger claim — the
/// sentinel reached `git`, through the environment, in the exact form the wire
/// needs.
#[tokio::test]
async fn the_token_never_appears_in_the_pushed_command_line() {
    const SENTINEL: &str = "ghp_argv_sentinel";
    let (push, outcome) = push_against_recording_git(SENTINEL).await;
    outcome.expect("an accepted push reports the published branch");

    assert!(
        !push.argv.iter().any(|arg| arg.contains(SENTINEL)),
        "a credential in argv is readable by every process on the host: {:?}",
        push.argv
    );
    let header = push
        .env
        .get("GIT_CONFIG_VALUE_0")
        .expect("it must have reached git somehow — through the environment");
    let encoded = header
        .strip_prefix("Authorization: Basic ")
        .expect("the header is HTTP basic auth");
    assert_eq!(
        String::from_utf8(base64_decode(encoded)).unwrap(),
        format!("x-access-token:{SENTINEL}"),
        "the environment carries the sentinel, encoded as the wire needs it"
    );
}

/// The argument vector, stated exactly.
///
/// This is the anti-forcing property made structural rather than argued. A
/// forced push is spelled `--force`, `-f`, `--force-with-lease`, or a `+` on the
/// front of the refspec; none of them can appear without this assertion
/// changing, so "the diverged case is refused rather than forced" is a fact
/// about the command line and not only about the branch that handles the
/// rejection.
#[tokio::test]
async fn the_push_command_line_is_exactly_this_and_carries_no_force() {
    let (push, _) = push_against_recording_git("tok").await;
    assert_eq!(
        push.argv,
        [
            "push",
            "--porcelain",
            "origin",
            "HEAD:refs/heads/fiddle/abc"
        ],
        "a force flag or a leading + on the refspec would destroy the \
         idempotence the milestone rests on"
    );
}

/// The header is delivered through git's documented environment config channel.
#[tokio::test]
async fn the_credential_arrives_as_an_env_injected_extra_header() {
    let (push, _) = push_against_recording_git("tok").await;
    assert_eq!(
        push.env.get("GIT_CONFIG_COUNT").map(String::as_str),
        Some("2")
    );
    assert_eq!(
        push.env.get("GIT_CONFIG_KEY_0").map(String::as_str),
        Some("http.https://github.com/.extraHeader")
    );
    assert!(push.env["GIT_CONFIG_VALUE_0"].starts_with("Authorization: Basic "));
}

/// The half that is worth a test: with the helper emptied, the push cannot
/// fall back to the operator's keychain, so "it used the credential it was
/// given and no other" is a fact about the process.
#[tokio::test]
async fn the_credential_helper_is_emptied_so_no_keychain_is_reachable() {
    let (push, _) = push_against_recording_git("tok").await;
    assert_eq!(
        push.env.get("GIT_CONFIG_KEY_1").map(String::as_str),
        Some("credential.helper")
    );
    assert_eq!(
        push.env.get("GIT_CONFIG_VALUE_1").map(String::as_str),
        Some("")
    );
    assert_eq!(
        push.env.get("GIT_TERMINAL_PROMPT").map(String::as_str),
        Some("0")
    );
}

/// The previous test asserts the pair is *sent*. This one asserts it *works*,
/// which is a different claim and the one the design actually leans on: an
/// empty `credential.helper` does not append an empty helper to the list, it
/// **resets** the list, so a helper configured anywhere else is gone.
///
/// It is asked of a real `git`, under the environment the product actually
/// built — read back verbatim from what the recording `git` received rather
/// than reconstructed here, so the test cannot pass by agreeing with itself. And
/// it is asked with `git credential fill`, which *consults* the helper list,
/// rather than `git config --get-all`, which merely prints it: the reset is a
/// property of how the credential machinery reads the list, and a test that
/// listed the values would see the empty string sitting in front of the helper
/// and conclude nothing.
///
/// This is not a hypothetical on a developer's machine. Probing this project's
/// own toolchain found a `credential.helper` in *system* configuration, which
/// `HOME`'s absence does nothing about: without the reset, a push here would
/// have answered from the operator's keychain, and every green test would have
/// been proving that the machine had a credential rather than that the header
/// worked.
#[tokio::test]
async fn the_emptied_helper_clears_a_helper_that_is_configured_elsewhere() {
    let dir = TempDir::new().unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    git_setup(&work, &["init", "-q", "."]);

    // A helper in the repository's own configuration, standing in for the
    // operator's keychain: same list, same mechanism, and it answers offline.
    // It leaves a marker, so "was a helper consulted?" is a question about the
    // filesystem rather than about output nobody should be capturing.
    let marker = dir.path().join("a-helper-ran");
    git_setup(
        &work,
        &[
            "config",
            "credential.helper",
            &format!(
                "!f() {{ touch '{}'; echo username=u; echo password=p; }}; f",
                marker.display()
            ),
        ],
    );

    recording_git(&work, "tok", "accepted")
        .publish(&work, "fiddle/abc", &CancellationToken::new())
        .await
        .unwrap();
    let delivered = recorded(&work, "push").env;

    // Deliberately discards stdout: a `git credential fill` that *did* reach a
    // helper would print a password, and a test that captured one into an
    // assertion message would be the same defect it is here to prevent.
    let a_helper_answers = |env: &BTreeMap<String, String>| {
        let _ = std::fs::remove_file(&marker);
        let mut child = std::process::Command::new("git")
            .args(["credential", "fill"])
            .current_dir(&work)
            .env_clear()
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"protocol=https\nhost=github.com\n\n")
            .unwrap();
        let answered = child.wait().unwrap().success();
        (answered, marker.exists())
    };

    assert_eq!(
        a_helper_answers(&delivered),
        (false, false),
        "under the environment the push actually runs in, no helper is reachable \
         and no credential can be produced — which is what makes a green push \
         proof that the supplied header worked"
    );

    // The control, and the reason the assertion above is evidence rather than a
    // tautology: with the config channel removed the same repository's helper
    // answers immediately. `GIT_CONFIG_NOSYSTEM` is added here and *only* here,
    // so that the machine's own system-configured helper cannot answer first —
    // which would make this control depend on the machine and, worse, hand a
    // real credential to a test process.
    let without_the_channel: BTreeMap<String, String> = delivered
        .iter()
        .filter(|(name, _)| !name.starts_with("GIT_CONFIG_"))
        .map(|(name, value)| (name.clone(), value.clone()))
        .chain([("GIT_CONFIG_NOSYSTEM".to_string(), "1".to_string())])
        .collect();
    assert_eq!(
        a_helper_answers(&without_the_channel),
        (true, true),
        "the control: without the config channel this repository's helper is \
         reached, so the silence above is caused by the reset and not by there \
         being nothing to reset"
    );
}

/// The environment is the security boundary, so it is pinned exactly — the same
/// move `workspace::a_workspace_command_inherits_no_credential` makes for the
/// four-name workspace set and `github_cli::the_gh_environment_is_exactly_five_names_and_no_home`
/// makes for `gh`. This is the third spawn site and it has its own contract:
/// seven names, and `HOME` is not among them.
///
/// `HOME`'s absence is the load-bearing line, for the same reason it is in the
/// `gh` adapter. It is what `git` follows to `~/.gitconfig` and
/// `~/.git-credentials`, so without it the emptied helper is not the only thing
/// standing between this push and a stored credential — there is nothing to
/// stand between.
#[tokio::test]
async fn the_push_environment_is_exactly_seven_names_and_no_home() {
    let (push, _) = push_against_recording_git("tok").await;
    let names: Vec<&str> = push.env.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_KEY_0",
            "GIT_CONFIG_KEY_1",
            "GIT_CONFIG_VALUE_0",
            "GIT_CONFIG_VALUE_1",
            "GIT_TERMINAL_PROMPT",
            "PATH",
        ],
        "an eighth name here is a change to the security boundary, and HOME \
         is the one that would undo the whole of it"
    );
}

/// The local read that resolves the commit is a separate child with a separate
/// environment, and it is not given the credential at all.
///
/// Reading `HEAD` needs no authority, so it is not granted any: the
/// credential-carrying environment exists at exactly one spawn site in this
/// module, which is what makes "where could this token go?" answerable by
/// reading one function.
#[tokio::test]
async fn the_local_read_carries_no_credential_at_all() {
    const SENTINEL: &str = "ghp_local_read_sentinel";
    let dir = TempDir::new().unwrap();
    let git = recording_git(dir.path(), SENTINEL, "accepted");
    let published = git
        .publish(dir.path(), "fiddle/abc", &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        published.sha, STUB_SHA,
        "the reported commit comes from the local read, not from what the push said"
    );

    let read = recorded(dir.path(), "rev-parse");
    let names: Vec<&str> = read.env.keys().map(String::as_str).collect();
    assert_eq!(names, ["GIT_TERMINAL_PROMPT", "PATH"]);
    assert!(
        !read
            .argv
            .iter()
            .chain(read.env.values())
            .any(|text| text.contains(SENTINEL) || text.contains("Authorization")),
        "a read of the local HEAD has no business holding the credential"
    );
}

/// A failure surfaced from a push is redacted of the credential material.
///
/// The recording `git` echoes the configured header, which is what a
/// `GIT_TRACE_CURL` or a wrapper would do. What must not survive into a
/// `GitError` is the credential in any of the forms it exists in — the token
/// and the base64 that carries it.
#[tokio::test]
async fn a_failed_push_reports_stderr_with_the_credential_removed() {
    const SENTINEL: &str = "ghp_stderr_sentinel";
    let dir = TempDir::new().unwrap();
    let git = recording_git(dir.path(), SENTINEL, "leaks_the_header");
    let error = git
        .publish(dir.path(), "fiddle/abc", &CancellationToken::new())
        .await
        .expect_err("exit 128 with no rejected ref is a failed push");

    let rendered = format!("{error} / {error:?}");
    assert!(
        rendered.contains("unable to access remote"),
        "git's own diagnostic is what makes the failure actionable: {rendered}"
    );
    assert!(
        !rendered.contains(SENTINEL),
        "the token survived into a diagnostic: {rendered}"
    );
    let encoded = recorded(dir.path(), "push").env["GIT_CONFIG_VALUE_0"]
        .strip_prefix("Authorization: Basic ")
        .unwrap()
        .to_string();
    assert!(
        !rendered.contains(&encoded),
        "the encoded credential is the form that actually leaks: {rendered}"
    );
}

/// A branch name is validated before it can become part of a command line.
///
/// Not because any caller sends these, but because "it holds by convention" is
/// not an argument: a leading `+` on a refspec *is* a force, and a leading `-`
/// is an option. Refusing at the boundary makes the argument-vector assertion
/// above true for every input rather than for the well-behaved ones.
#[tokio::test]
async fn a_branch_name_that_could_change_the_command_is_refused() {
    let dir = TempDir::new().unwrap();
    let git = recording_git(dir.path(), "tok", "accepted");
    for branch in [
        "",
        "+fiddle/abc",
        "--force",
        "fiddle/abc extra",
        "fiddle/../abc",
        "fiddle/abc:refs/heads/main",
        "fiddle/abc\n--force",
        "/fiddle/abc",
        "fiddle/abc/",
        "fiddle//abc",
        "fiddle/abc.lock",
        ".fiddle/abc",
        "fiddle/abc~1",
    ] {
        let error = git
            .publish(dir.path(), branch, &CancellationToken::new())
            .await
            .expect_err("a branch name that could change the command must be refused");
        assert!(
            matches!(error, GitError::InvalidBranch { .. }),
            "{branch:?} produced {error:?}"
        );
    }
    assert!(
        !dir.path().join("push.json").exists() && !dir.path().join("rev-parse.json").exists(),
        "a refused branch name must not have spawned anything"
    );
}

// ---------------------------------------------------------------------------
// What actually happened to the remote
// ---------------------------------------------------------------------------

/// Pushing the same commit to the same ref twice is a no-op. This is why the
/// branch half of the milestone needs no bespoke identity machinery.
#[tokio::test]
async fn pushing_the_same_commit_twice_is_idempotent() {
    let dir = TempDir::new().unwrap();
    let remote = bare_repository(dir.path());
    let work = worktree_with_one_commit(dir.path(), "work", &remote, "one");

    let git = real_git();
    let cancel = CancellationToken::new();
    let first = git.publish(&work, "fiddle/abc", &cancel).await.unwrap();
    let second = git.publish(&work, "fiddle/abc", &cancel).await.unwrap();

    assert_eq!(first.sha, second.sha);
    assert_eq!(first.branch, "fiddle/abc");
    assert_eq!(branches(&remote), ["fiddle/abc"], "exactly one branch");
    assert_eq!(
        remote_sha(&remote, "fiddle/abc"),
        first.sha,
        "the second push left the ref where the first one put it"
    );
}

/// A different commit to an existing ref is refused rather than forced.
///
/// The remote's ref is asserted afterwards as well as the error, because those
/// are two different claims: an implementation that forced the push and then
/// reported a rejection would satisfy the first and destroy the property the
/// milestone rests on.
#[tokio::test]
async fn a_diverged_push_is_refused_not_forced() {
    let dir = TempDir::new().unwrap();
    let remote = bare_repository(dir.path());
    let first = worktree_with_one_commit(dir.path(), "first", &remote, "one");
    let other = worktree_with_one_commit(dir.path(), "other", &remote, "another");

    let git = real_git();
    let cancel = CancellationToken::new();
    let published = git.publish(&first, "fiddle/abc", &cancel).await.unwrap();
    let error = git
        .publish(&other, "fiddle/abc", &cancel)
        .await
        .expect_err("a ref that is not an ancestor cannot fast-forward");

    assert!(
        matches!(error, GitError::NonFastForward { .. }),
        "{error:?}"
    );
    assert_eq!(
        remote_sha(&remote, "fiddle/abc"),
        published.sha,
        "the refused push must not have moved the ref"
    );
    assert_eq!(branches(&remote), ["fiddle/abc"], "and not have added one");
}

/// A cancelled publish does not spawn anything.
///
/// Cancellation has to *prevent* the effect, not merely stop waiting for it,
/// which is the same check `Workspace::run` and `GhCli::api` make before their
/// own spawns.
///
/// This is the **first** of the push's two cancellation provenances, and the
/// classification is asserted here rather than only in the table beside
/// [`GitError::outcome`]: no pushing child existed, so `NotCommitted` is
/// knowledge about the ref rather than an assumption about it. The second
/// provenance is the test after this one.
#[tokio::test]
async fn a_cancelled_publish_changes_nothing() {
    let dir = TempDir::new().unwrap();
    let remote = bare_repository(dir.path());
    let work = worktree_with_one_commit(dir.path(), "work", &remote, "one");

    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = real_git()
        .publish(&work, "fiddle/abc", &cancel)
        .await
        .expect_err("a cancelled attempt publishes nothing");

    assert!(matches!(error, GitError::CancelledBeforePush), "{error:?}");
    assert!(branches(&remote).is_empty(), "no ref was created");
    assert_eq!(
        error.outcome(),
        EffectOutcome::NotCommitted,
        "nothing was pushed, and that is knowledge: {error:?}"
    );
}

/// The **second** provenance, and the push's half of the defect M2's holistic
/// review found: a cancellation that reaches a `git push` which is *already
/// running*.
///
/// A push killed on the way back may have delivered its pack and moved the ref
/// already — the identical reasoning [`GitError::Killed`] and
/// [`GitError::Timeout`] are `Unknown` for. Cancellation was the one interruption
/// left out of that group, and it is the only one of the three a `^C` can
/// actually produce: `crate::process` gives every bounded child a process group
/// of its own, so a terminal interrupt reaches it *only* through the token.
///
/// The premise is observed rather than assumed: the recording `git` writes down
/// the invocation it received, so `push.json` existing is proof the pushing child
/// really ran before the token cancelled. Without it this test would pass against
/// the pre-spawn refusal above.
#[tokio::test]
async fn a_cancellation_after_the_push_was_spawned_is_an_ambiguous_write() {
    let dir = TempDir::new().unwrap();
    let remote = bare_repository(dir.path());
    let work = worktree_with_one_commit(dir.path(), "work", &remote, "one");
    // A `git push` that never answers, so the cancellation is what ends it
    // rather than a child that had already finished.
    let git = recording_git(&work, "ghp_whatever", "never_answers");

    let cancel = CancellationToken::new();
    let canceller = cancel.clone();
    // Wait for the premise rather than sleeping past it. A fixed
    // `sleep(250ms)` stood here and raced the child it depended on: it failed
    // once in a full-workspace run under load — never in isolation, where this
    // test passes 6/6 — and what it reported was the premise assertion below,
    // so a scheduling delay read as a product defect (`fiddle-vicv`).
    //
    // `push.json` is the recording `git`'s own note that the pushing child ran,
    // which is exactly what the premise asserts, so waiting for it puts the
    // cancellation after the spawn by construction rather than by arithmetic
    // about machine speed. The bound is a ceiling, not a timing assumption: on
    // expiry it cancels anyway and lets the premise assertion report with its
    // own message, instead of hanging the suite.
    let evidence = work.join("push.json");
    tokio::spawn(async move {
        for _ in 0..6_000 {
            if evidence.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        canceller.cancel();
    });

    let error = git
        .publish(&work, "fiddle/abc", &cancel)
        .await
        .expect_err("a cancelled push is a failure");

    assert!(
        work.join("push.json").exists(),
        "the pushing child must have run before the token cancelled, or this is \
         the pre-spawn case wearing the other name"
    );
    assert!(matches!(error, GitError::CancelledMidPush), "{error:?}");
    assert_eq!(
        error.outcome(),
        EffectOutcome::Unknown,
        "a ref that may already have moved is not a refusal: {error:?}"
    );
    assert_ne!(
        error.outcome(),
        GitError::CancelledBeforePush.outcome(),
        "the two provenances of one cancellation must not classify alike"
    );
    // And the verdict has to survive the trip into the executor's vocabulary,
    // for the reason the exhaustive table beside `GitError::outcome` gives: a
    // classification decided twice is decided by the second one.
    assert_eq!(
        fiddle_runtime::github::GhError::from(error).outcome(),
        EffectOutcome::Unknown
    );
}
