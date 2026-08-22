use fiddle_core::AttemptId;
use fiddle_runtime::capability::{Git, InRepository, InWorktree};
use fiddle_runtime::git::GitCli;
use fiddle_runtime::workspace::Workspace;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const PATIENT: Duration = Duration::from_secs(60);

const TOKEN: &str = "ghp_adapter_sentinel";

fn recording_git(dir: &Path) -> GitCli {
    std::fs::write(dir.join("mode"), "accepted").unwrap();
    GitCli::new(
        PathBuf::from(env!("CARGO_BIN_EXE_git_stub")),
        TOKEN.to_string(),
        "FIDDLE_GITHUB_TOKEN",
        PATIENT,
    )
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

fn repository_with_one_commit(dir: &Path) -> PathBuf {
    git_setup(dir, &["init", "-q", "."]);
    std::fs::write(dir.join("file"), "one\n").unwrap();
    git_setup(dir, &["add", "file"]);
    git_setup(dir, &["commit", "-q", "-m", "one"]);
    dir.to_path_buf()
}

fn offered_credential(dir: &Path) -> Option<String> {
    let record = std::fs::read_to_string(dir.join("fetch.json")).ok()?;
    let record: serde_json::Value = serde_json::from_str(&record).unwrap();
    record["env"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_str().unwrap().to_string())
        .find(|entry| entry.starts_with("GIT_CONFIG_VALUE_0="))
}

#[tokio::test]
async fn the_repository_adapter_offers_the_credential_when_it_fetches() {
    let dir = TempDir::new().unwrap();
    let repository = repository_with_one_commit(dir.path());
    let network = recording_git(&repository);

    InRepository::new(&repository, &network, CancellationToken::new())
        .fetch("main")
        .await
        .expect("the recording git accepts the fetch");

    let offered = offered_credential(&repository).expect("the fetch carried no credential at all");
    assert!(
        offered.contains("Authorization: Basic "),
        "the fetch has to authenticate the way the push does: {offered}"
    );
    assert!(
        !offered.contains(TOKEN),
        "the header carries the encoded credential, never the raw token: {offered}"
    );
}

#[tokio::test]
async fn the_worktree_adapter_offers_the_credential_when_it_fetches() {
    let dir = TempDir::new().unwrap();
    let fixture = dir.path().join("fixture");
    std::fs::create_dir_all(&fixture).unwrap();
    repository_with_one_commit(&fixture);

    let root = dir.path().join("attempts");
    let attempt = AttemptId("01JCVEADAPTER00000000000000".to_string());
    let workspace = Workspace::create(&fixture, &root, &attempt, CancellationToken::new())
        .expect("a worktree of the fixture");
    let network = recording_git(workspace.root());

    InWorktree::new(&workspace, PATIENT, &network)
        .fetch("main")
        .await
        .expect("the recording git accepts the fetch");

    let offered =
        offered_credential(workspace.root()).expect("the fetch carried no credential at all");
    assert!(offered.contains("Authorization: Basic "), "{offered}");
}

#[tokio::test]
async fn a_local_subcommand_never_reaches_the_credentialed_adapter() {
    let dir = TempDir::new().unwrap();
    let repository = repository_with_one_commit(dir.path());
    let network = recording_git(&repository);
    let git = InRepository::new(&repository, &network, CancellationToken::new());

    let head = git
        .run(&["rev-parse", "--verify", "--quiet", "HEAD"])
        .await
        .expect("a local read needs no credential and still runs");
    assert_eq!(head.trim().len(), 40, "{head:?}");

    let committed = git
        .run(&[
            "-c",
            "user.email=fiddle@example.invalid",
            "-c",
            "user.name=fiddle",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "a local commit",
        ])
        .await;
    assert!(
        committed.is_ok(),
        "the guard reads the subcommand and not the first argument: {committed:?}"
    );

    assert_eq!(
        offered_credential(&repository),
        None,
        "no local subcommand may offer a credential, and none may spawn the \
         credentialed adapter to do it"
    );
}

#[tokio::test]
async fn the_local_runner_refuses_every_subcommand_that_reaches_a_remote() {
    let dir = TempDir::new().unwrap();
    let repository = repository_with_one_commit(dir.path());
    let network = recording_git(&repository);
    let git = InRepository::new(&repository, &network, CancellationToken::new());

    for refused in [
        vec!["fetch", "--no-tags", "origin", "main"],
        vec!["push", "origin", "HEAD:refs/heads/main"],
        vec!["pull", "origin", "main"],
        vec!["ls-remote", "origin"],
        vec!["clone", "https://github.com/o/r.git"],
        vec!["-c", "protocol.version=2", "fetch", "origin"],
    ] {
        let outcome = git.run(&refused).await;
        let why = outcome
            .expect_err(&format!("{refused:?} reached a remote with no credential"))
            .to_string();
        assert!(
            why.contains("reaches a remote"),
            "the refusal has to say why: {why}"
        );
    }

    assert_eq!(
        offered_credential(&repository),
        None,
        "a refusal must happen before anything is spawned"
    );
}
