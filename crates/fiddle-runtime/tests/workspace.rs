//! The per-attempt workspace, exercised against a real git repository.
//!
//! These are integration tests rather than unit tests because every property
//! here is about the filesystem and about `git` — an isolated checkout, a
//! teardown that survives the ways a caller can leave early, and a containment
//! check that only means something once a symlink actually exists on disk. None
//! of that can be faked from inside the crate without testing the fake instead.

mod fixture;

use fiddle_core::AttemptId;
use fiddle_runtime::workspace::{Workspace, WorkspaceCommand, WorkspaceError, WorkspacePath};
use std::time::Duration;
use tokio::sync::{RwLock, RwLockReadGuard};
use tokio_util::sync::CancellationToken;

fn attempt() -> AttemptId {
    AttemptId("01JQZX0000000000000000000".to_string())
}

fn token() -> CancellationToken {
    CancellationToken::new()
}

fn p(raw: &str) -> WorkspacePath {
    WorkspacePath::parse(raw).expect("the test's own path must be workspace-relative")
}

/// Serializes the one test that mutates this process's environment against
/// every test that reads it.
///
/// `setenv` is not thread-safe against a concurrent `getenv`, and libtest runs
/// these tests as threads of a single process. Every test here spawns `git`
/// with an *inherited* environment, which reads `environ`, so the sentinel test
/// takes the write side and the rest take the read side. Readers never block
/// each other, so this costs nothing except when the sentinel is actually set.
/// `--test-threads=1` would do the same job, but only for whoever remembers the
/// flag; this holds for anyone who runs the suite the ordinary way.
///
/// Tokio's lock rather than `std`'s because the tests that run a command hold
/// the guard across an `await`, which a blocking guard must not be.
static ENV: RwLock<()> = RwLock::const_new(());

/// Claim the right to read this process's environment, from a blocking test.
fn env_reader() -> RwLockReadGuard<'static, ()> {
    ENV.blocking_read()
}

/// A workspace over a throwaway fixture repository.
///
/// The [`tempfile::TempDir`] comes back with it because it owns the fixture the
/// worktree was branched from; dropping it early would delete the repository
/// out from under the workspace.
fn workspace() -> (Workspace, tempfile::TempDir) {
    workspace_with(token())
}

fn workspace_with_cancelled_token() -> (Workspace, tempfile::TempDir) {
    let cancel = token();
    cancel.cancel();
    workspace_with(cancel)
}

fn workspace_with(cancel: CancellationToken) -> (Workspace, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::broken_crate(dir.path());
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), cancel).unwrap();
    (ws, dir)
}

/// A command with a timeout generous enough that only a hang could reach it.
fn cmd(program: &str, args: &[&str]) -> WorkspaceCommand {
    WorkspaceCommand {
        program: program.into(),
        args: args.iter().map(|a| (*a).into()).collect(),
        timeout: Duration::from_secs(30),
    }
}

#[test]
fn a_workspace_is_an_isolated_checkout_that_disappears() {
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::broken_crate(dir.path());
    let mut ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();
    let path = ws.root().to_path_buf();

    ws.write(&p("src/lib.rs"), "pub fn g() {}\n").unwrap();
    assert_ne!(
        std::fs::read_to_string(repo.join("src/lib.rs")).unwrap(),
        "pub fn g() {}\n",
        "mutating the workspace must not touch the fixture it came from"
    );

    ws.remove().unwrap();
    assert!(!path.exists());
}

#[test]
fn the_worktree_is_removed_even_when_nobody_calls_remove() {
    // Teardown must survive an early return, a `?`, and a panic. M0 learned this
    // for bundle publication with a Drop guard; the same applies here.
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::broken_crate(dir.path());
    let path = {
        let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();
        ws.root().to_path_buf()
    };
    assert!(!path.exists(), "a dropped workspace must not survive");
}

#[test]
fn removing_twice_is_not_an_error() {
    // The explicit call and the guard both run on the happy path, so the second
    // removal has to be a no-op rather than a failed `git worktree remove` on a
    // path git no longer knows about.
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::broken_crate(dir.path());
    let mut ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    ws.remove().unwrap();
    ws.remove()
        .expect("a second removal must be a no-op, not a git failure");
}

#[test]
fn a_symlink_pointing_out_of_the_workspace_is_refused() {
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::broken_crate(dir.path());
    let secret = dir.path().join("secret.txt");
    std::fs::write(&secret, "do not read me").unwrap();
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    // Syntactically innocent; its *resolution* leaves the workspace.
    std::os::unix::fs::symlink(&secret, ws.root().join("escape.txt")).unwrap();
    assert!(ws.read(&p("escape.txt")).is_err());
    let refusal = ws.write(&p("escape.txt"), "x");
    assert!(refusal.is_err());
    // Named, not merely failed: an `Escape` proves the containment check refused
    // it before opening anything. An `Io` here would mean the write was attempted
    // and something unrelated stopped it, which is not the guarantee.
    assert!(
        matches!(refusal, Err(WorkspaceError::Escape { .. })),
        "the write must be refused by containment, got {refusal:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&secret).unwrap(),
        "do not read me",
        "a refused write must not have happened anyway"
    );
}

#[test]
fn a_dangling_symlink_out_of_the_workspace_is_refused() {
    // The nastier cousin of the test above, and the reason resolution cannot
    // treat "canonicalize failed" as "the leaf does not exist yet": for a link
    // whose target is missing, canonicalize fails, the parent resolves *inside*,
    // and `std::fs::write` then follows the link and creates the file outside.
    // Only looking at the link itself distinguishes the two cases.
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::broken_crate(dir.path());
    let target = dir.path().join("not-yet.txt");
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    std::os::unix::fs::symlink(&target, ws.root().join("dangle.txt")).unwrap();
    assert!(ws.write(&p("dangle.txt"), "x").is_err());
    assert!(
        !target.exists(),
        "a refused write must not have created the file it was aimed at"
    );
}

#[test]
fn an_ordinary_file_round_trips_and_a_new_one_can_be_created() {
    // The counterpart to the refusals: resolution has to still admit a path
    // whose leaf does not exist yet, which is the branch the symlink check
    // reaches through the parent.
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::broken_crate(dir.path());
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    assert_eq!(ws.read(&p("src/lib.rs")).unwrap(), "pub fn f() {}\n");
    ws.write(&p("src/new.rs"), "pub fn n() {}\n").unwrap();
    assert_eq!(ws.read(&p("src/new.rs")).unwrap(), "pub fn n() {}\n");
}

#[test]
fn changed_files_come_from_git_not_from_anyones_claim() {
    let _env = env_reader();
    let (ws, _dir) = workspace();
    assert!(
        ws.changed_files().unwrap().is_empty(),
        "an untouched workspace changed nothing"
    );
    ws.write(&p("src/lib.rs"), "pub fn g() {}\n").unwrap();
    assert_eq!(ws.changed_files().unwrap(), vec![p("src/lib.rs")]);
}

#[test]
fn build_artefacts_never_appear() {
    let _env = env_reader();
    let (ws, _dir) = workspace();
    std::fs::create_dir_all(ws.root().join("target/debug")).unwrap();
    std::fs::write(ws.root().join("target/debug/junk"), "x").unwrap();
    assert!(
        ws.changed_files().unwrap().is_empty(),
        "the fixture gitignores target/, or the changed-file evidence is worthless"
    );
}

#[test]
fn a_path_with_a_space_is_parsed_correctly() {
    // --porcelain QUOTES paths containing spaces or non-ASCII bytes, and renders a
    // rename as `R  old -> new`. `-z` emits NUL-separated, never-quoted entries
    // instead, so neither shape needs unquoting and a fixed byte slice cannot
    // mis-parse. Prove it with a path the quoting form would mangle.
    let _env = env_reader();
    let (ws, _dir) = workspace();
    ws.write(&p("src/a file.rs"), "// new\n").unwrap();
    assert_eq!(ws.changed_files().unwrap(), vec![p("src/a file.rs")]);
}

#[test]
fn a_rename_reports_the_new_path_once_not_both() {
    // Observed from git 2.51: `git mv src/lib.rs src/renamed.rs` produces
    // `R  src/renamed.rs\0src/lib.rs\0` — the NEW path in the status record and
    // the origin as its own bare record. Consuming that second record is what
    // stops the origin being mistaken for a changed file of its own.
    let _env = env_reader();
    let (ws, _dir) = workspace();
    fixture::git(ws.root(), &["mv", "src/lib.rs", "src/renamed.rs"]);

    let changed = ws.changed_files().unwrap();
    assert_eq!(
        changed,
        vec![p("src/renamed.rs")],
        "the rename's origin record is data about the same change, not a second one"
    );
}

#[test]
fn a_file_created_in_a_new_directory_is_named_not_just_its_directory() {
    // git's default untracked mode collapses a wholly-new directory into one
    // entry, `?? src/newmod/` — a directory, which is not a changed *file* and
    // hides how many there are. `-uall` is what makes the evidence name the files
    // an agent actually wrote. The same flag also overrides a
    // `status.showUntrackedFiles=no` in an operator's config, which would
    // otherwise drop every created file from the evidence silently.
    let _env = env_reader();
    let (ws, _dir) = workspace();
    std::fs::create_dir_all(ws.root().join("src/newmod")).unwrap();
    ws.write(&p("src/newmod/a.rs"), "pub fn a() {}\n").unwrap();
    ws.write(&p("src/newmod/b.rs"), "pub fn b() {}\n").unwrap();

    assert_eq!(
        ws.changed_files().unwrap(),
        vec![p("src/newmod/a.rs"), p("src/newmod/b.rs")],
        "a new directory's files must be named individually, not collapsed"
    );
}

#[tokio::test]
async fn a_command_runs_in_the_workspace_and_reports_what_it_did() {
    // Without this the isolation tests below would all pass on a runner that
    // never actually runs anything: a command that produces no output cannot
    // leak a credential either.
    let _env = ENV.read().await;
    let (ws, _dir) = workspace();

    let result = ws
        .run(&cmd("/bin/sh", &["-c", "echo out; echo err >&2; pwd"]))
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stderr.trim(), "err");
    let mut lines = result.stdout.lines();
    assert_eq!(lines.next(), Some("out"));
    // The command's working directory is the workspace, not this process's.
    // Compared canonicalized because a macOS temp directory is reached through
    // a symlink and `getcwd` reports the far side of it.
    assert_eq!(
        lines.next().map(std::path::PathBuf::from),
        Some(ws.root().canonicalize().unwrap())
    );

    let failed = ws.run(&cmd("/bin/sh", &["-c", "exit 3"])).await.unwrap();
    assert_eq!(
        failed.exit_code, 3,
        "a non-zero exit is a result, not an error"
    );
}

#[tokio::test]
async fn a_workspace_command_inherits_no_credential() {
    let (ws, _dir) = workspace();
    // Resolve the tool PATH before the environment is mutated, so that nothing
    // inside `run` reads `environ` while the sentinel is being set below.
    ws.run(&cmd("/usr/bin/true", &[])).await.unwrap();

    let guard = ENV.write().await;
    // SAFETY: `ENV` is held for writing, so no other test in this binary is
    // reading the environment for the duration of the mutation. `run` itself
    // does not read it — `env_clear` means the child's environment is built
    // from the explicit allowlist rather than captured from this process.
    unsafe { std::env::set_var("LITELLM_API_KEY", "sentinel-must-not-leak") };
    let result = ws.run(&cmd("/usr/bin/env", &[])).await.unwrap();
    unsafe { std::env::remove_var("LITELLM_API_KEY") };
    drop(guard);

    assert!(
        !result.stdout.contains("sentinel-must-not-leak"),
        "a workspace command must not observe the model credential: {}",
        result.stdout
    );
    assert!(!result.stdout.contains("LITELLM_API_KEY"));
    // The sentinel proves this one credential does not survive; the exhaustive
    // check proves *nothing* does. A denylist would have to be extended for
    // every credential added later, and this is the assertion that fails if
    // `env_clear` is ever dropped for a selective `env_remove`.
    let mut seen: Vec<&str> = result
        .stdout
        .lines()
        .filter_map(|line| line.split('=').next())
        .collect();
    seen.sort_unstable();
    assert_eq!(
        seen,
        ["HOME", "LANG", "PATH"],
        "the child's environment must be exactly the allowlist: {}",
        result.stdout
    );
}

#[tokio::test]
async fn a_command_that_overruns_its_timeout_is_killed() {
    let _env = ENV.read().await;
    let (ws, _dir) = workspace();
    let err = ws
        .run(&WorkspaceCommand {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "sleep 30".into()],
            timeout: Duration::from_millis(300),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, WorkspaceError::Timeout { .. }), "got {err:?}");
    // Naming the program is the point: an operator reading the diagnostic has to
    // learn *what* hung, not only that something did.
    assert!(err.to_string().contains("/bin/sh"), "got {err}");

    // The deadline must *kill* the child, not merely stop waiting for it.
    // Asserting the error alone would pass just as happily on a runner that
    // leaves a `sleep 30` running with nobody holding its handle — verified by
    // deleting `kill_on_drop` and watching this assertion, and only this one,
    // fail. So the command is one that would leave a trace if it outlived its
    // deadline, and the trace must not appear.
    let err = ws
        .run(&WorkspaceCommand {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "sleep 1; : > outlived".into()],
            timeout: Duration::from_millis(200),
        })
        .await
        .unwrap_err();
    assert!(matches!(err, WorkspaceError::Timeout { .. }), "got {err:?}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        !ws.root().join("outlived").exists(),
        "a timed-out command must not survive its deadline"
    );
}

#[tokio::test]
async fn a_cancelled_token_stops_a_command_before_it_runs() {
    let _env = ENV.read().await;
    let (ws, _dir) = workspace_with_cancelled_token();
    let marker = ws.root().join("ran");
    assert!(matches!(
        ws.run(&cmd("/bin/sh", &["-c", "echo ran > ran"])).await,
        Err(WorkspaceError::Cancelled)
    ));
    // Cancellation has to prevent the effect, not merely end the future.
    assert!(!marker.exists(), "a cancelled command must not have run");
}
