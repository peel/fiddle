use fiddle_runtime::effect::EffectOutcome;
use fiddle_runtime::git::{GitCli, GitError};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const PATIENT: Duration = Duration::from_secs(60);

const STUB_SHA: &str = "0123456789abcdef0123456789abcdef01234567";

struct Recorded {
    argv: Vec<String>,
    env: BTreeMap<String, String>,
}

fn recording_git(dir: &Path, token: &str, mode: &str) -> GitCli {
    std::fs::write(dir.join("mode"), mode).unwrap();
    GitCli::new(
        PathBuf::from(env!("CARGO_BIN_EXE_git_stub")),
        token.to_string(),
        "FIDDLE_GITHUB_TOKEN",
        PATIENT,
    )
}

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

async fn push_against_recording_git(token: &str) -> (Recorded, Result<(), GitError>) {
    let dir = TempDir::new().unwrap();
    let outcome = recording_git(dir.path(), token, "accepted")
        .publish(dir.path(), "fiddle/abc", &CancellationToken::new())
        .await;
    (recorded(dir.path(), "push"), outcome.map(|_| ()))
}

async fn fetch_against_recording_git(token: &str) -> (Recorded, Result<(), GitError>) {
    let dir = TempDir::new().unwrap();
    let outcome = recording_git(dir.path(), token, "accepted")
        .fetch(dir.path(), "main", &CancellationToken::new())
        .await;
    (recorded(dir.path(), "fetch"), outcome)
}

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

fn bare_repository(dir: &Path) -> PathBuf {
    let remote = dir.join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    git_setup(&remote, &["init", "-q", "--bare", "."]);
    remote
}

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

fn remote_sha(remote: &Path, branch: &str) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", &format!("refs/heads/{branch}")])
        .current_dir(remote)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn rev_parse(dir: &Path, revision: &str) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--verify", revision])
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git rev-parse {revision} in {} failed: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn real_git() -> GitCli {
    GitCli::new(
        PathBuf::from("git"),
        "ghp_never_used_by_a_path_remote".to_string(),
        "FIDDLE_GITHUB_TOKEN",
        PATIENT,
    )
}

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

#[tokio::test]
async fn the_emptied_helper_clears_a_helper_that_is_configured_elsewhere() {
    let dir = TempDir::new().unwrap();
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    git_setup(&work, &["init", "-q", "."]);

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

#[tokio::test]
async fn the_fetch_offers_the_credential_the_push_offers() {
    let (fetch, outcome) = fetch_against_recording_git("tok").await;
    outcome.expect("an accepted fetch reports nothing and fails at nothing");
    let (push, _) = push_against_recording_git("tok").await;

    assert_eq!(
        fetch.env, push.env,
        "every name and every value, not the names alone. A fetch and a push \
         that differ in one of either cannot both be satisfied by one host \
         configuration, which is the defect this pins. A header keyed to \
         another host would pass a comparison of names."
    );

    let header = fetch
        .env
        .get("GIT_CONFIG_VALUE_0")
        .expect("it must have reached git somehow — through the environment");
    let encoded = header
        .strip_prefix("Authorization: Basic ")
        .expect("the header is HTTP basic auth");
    assert_eq!(
        String::from_utf8(base64_decode(encoded)).unwrap(),
        "x-access-token:tok",
        "equality above passes if both sides are equally wrong, so the fetch's \
         header is decoded here and not only compared"
    );
}

#[tokio::test]
async fn the_fetch_command_line_names_one_branch_and_takes_no_tags() {
    let (fetch, _) = fetch_against_recording_git("tok").await;
    assert_eq!(
        fetch.argv,
        [
            "fetch",
            "--no-tags",
            "--quiet",
            "origin",
            "+refs/heads/main:refs/remotes/origin/main"
        ],
        "a wider refspec would move refs this run did not name"
    );
}

#[tokio::test]
async fn the_token_never_appears_in_the_fetched_command_line() {
    const SENTINEL: &str = "ghp_fetch_argv_sentinel";
    let (fetch, _) = fetch_against_recording_git(SENTINEL).await;
    assert!(
        !fetch.argv.iter().any(|arg| arg.contains(SENTINEL)),
        "a credential in argv is readable by every process on the host: {:?}",
        fetch.argv
    );
}

#[tokio::test]
async fn a_fetch_that_offers_the_credential_still_reads_a_path_remote() {
    let dir = TempDir::new().unwrap();
    let remote = bare_repository(dir.path());
    let author = worktree_with_one_commit(dir.path(), "author", &remote, "one");
    real_git()
        .publish(&author, "main", &CancellationToken::new())
        .await
        .expect("a path remote accepts the push");

    let reader = dir.path().join("reader");
    std::fs::create_dir_all(&reader).unwrap();
    git_setup(&reader, &["init", "-q", "."]);
    git_setup(
        &reader,
        &["remote", "add", "origin", &remote.display().to_string()],
    );

    real_git()
        .fetch(&reader, "main", &CancellationToken::new())
        .await
        .expect("the header names github.com, so a path remote ignores it");

    assert_eq!(
        rev_parse(&reader, "refs/remotes/origin/main"),
        remote_sha(&remote, "main"),
        "the acceptance harness fetches from a path remote, and the credential \
         must not change what it reads"
    );
}

#[tokio::test]
async fn a_refused_branch_name_never_reaches_a_fetch() {
    let dir = TempDir::new().unwrap();
    let refusal = recording_git(dir.path(), "tok", "accepted")
        .fetch(dir.path(), "--upload-pack=touch", &CancellationToken::new())
        .await;
    assert!(
        matches!(refusal, Err(GitError::InvalidBranch { .. })),
        "{refusal:?}"
    );
    assert!(
        !dir.path().join("fetch.json").exists(),
        "the refusal has to happen before git is spawned"
    );
}

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

#[tokio::test]
async fn a_cancellation_after_the_push_was_spawned_is_an_ambiguous_write() {
    let dir = TempDir::new().unwrap();
    let remote = bare_repository(dir.path());
    let work = worktree_with_one_commit(dir.path(), "work", &remote, "one");
    let git = recording_git(&work, "ghp_whatever", "never_answers");

    let cancel = CancellationToken::new();
    let canceller = cancel.clone();
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
    assert_eq!(
        fiddle_runtime::github::GhError::from(error).outcome(),
        EffectOutcome::Unknown
    );
}
