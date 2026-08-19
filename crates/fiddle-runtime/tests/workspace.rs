mod fixture;

use fiddle_core::AttemptId;
use fiddle_runtime::capability::CapabilityError;
use fiddle_runtime::effect::Recurrence;
use fiddle_runtime::workspace::{Workspace, WorkspaceCommand, WorkspaceError, WorkspacePath};
use std::path::Path;
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

static ENV: RwLock<()> = RwLock::const_new(());

fn env_reader() -> RwLockReadGuard<'static, ()> {
    ENV.blocking_read()
}

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
    let repo = fixture::trivial_repo(dir.path());
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), cancel).unwrap();
    (ws, dir)
}

fn git_out(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|source| panic!("could not run git {args:?}: {source}"));
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_string();
    if output.status.success() {
        Ok(text(&output.stdout))
    } else {
        Err(text(&output.stderr))
    }
}

fn store_holds(repo: &Path, object: &str) -> bool {
    git_out(repo, &["cat-file", "-e", object]).is_ok()
}

fn commit_all(repo: &Path, message: &str) -> String {
    fixture::git(repo, &["add", "-A"]);
    fixture::git(
        repo,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            message,
        ],
    );
    git_out(repo, &["rev-parse", "HEAD"]).expect("a fresh commit has a sha")
}

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
    let repo = fixture::trivial_repo(dir.path());
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
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    let path = {
        let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();
        ws.root().to_path_buf()
    };
    assert!(!path.exists(), "a dropped workspace must not survive");
}

#[test]
fn removing_twice_is_not_an_error() {
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    let mut ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    ws.remove().unwrap();
    ws.remove()
        .expect("a second removal must be a no-op, not a git failure");
}

#[test]
fn a_worktree_branches_at_the_revision_it_was_given_and_not_at_head() {
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    let base = git_out(&repo, &["rev-parse", "HEAD"]).expect("the fixture has one commit");
    std::fs::write(repo.join("src/lib.rs"), "pub fn moved_on() {}\n").unwrap();
    let head = commit_all(&repo, "a commit a redirect must not branch from");
    assert_ne!(
        base, head,
        "the fixture's HEAD has to have moved, or this test cannot tell the two \
         branch points apart"
    );

    let at_base = Workspace::create_at(
        &repo,
        &dir.path().join("at-base"),
        &attempt(),
        &base,
        token(),
    )
    .expect("a revision the fixture's store holds is branchable");
    assert_eq!(
        git_out(at_base.root(), &["rev-parse", "HEAD"]).unwrap(),
        base,
        "the worktree's HEAD is the revision the caller named"
    );
    assert_eq!(
        at_base.read(&p("src/lib.rs")).unwrap(),
        "pub fn f() {}\n",
        "and the tree under it is that commit's, not the fixture's current one"
    );

    let at_head = Workspace::create(&repo, &dir.path().join("at-head"), &attempt(), token())
        .expect("the fixture's own HEAD is branchable");
    assert_eq!(
        at_head.read(&p("src/lib.rs")).unwrap(),
        "pub fn moved_on() {}\n",
        "`create` is the same call at HEAD, and HEAD is the other commit — \
         which is what makes the assertion above about the revision"
    );
}

#[test]
fn a_revision_the_fixture_can_only_fetch_is_refused_by_name_and_nothing_fetches() {
    let _env = env_reader();
    let elsewhere = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let origin = fixture::trivial_repo(elsewhere.path());
    fixture::git(
        dir.path(),
        &["clone", "-q", &origin.to_string_lossy(), "fixture"],
    );
    let fixture_repo = dir.path().join("fixture");
    std::fs::write(origin.join("src/lib.rs"), "pub fn redirected() {}\n").unwrap();
    let published = commit_all(&origin, "the commit the other machine published");

    let advertised = git_out(&fixture_repo, &["ls-remote", "origin"])
        .expect("the clone has its origin and can reach it");
    assert!(
        advertised.contains(&published),
        "the fixture's own remote must advertise the sha, so that the object is \
         one fetch away and the refusal is a choice rather than an impossibility: \
         ls-remote said {advertised:?}"
    );
    assert!(
        !store_holds(&fixture_repo, &published),
        "and the fixture's store must not hold it, or there is no limitation here \
         to pin"
    );

    let root = dir.path().join("ws");
    let refusal = match Workspace::create_at(&fixture_repo, &root, &attempt(), &published, token())
    {
        Err(error) => error,
        Ok(_) => {
            let diagnosis = if store_holds(&fixture_repo, &published) {
                "and the object is in the store now, so something resolves the \
                 revision — the documented limitation has been lifted and this \
                 test has to be rewritten, not deleted"
            } else {
                "and the object is still absent, so the refusal was swallowed and \
                 this workspace is branched from somewhere else"
            };
            panic!(
                "create_at returned a workspace for {published}, which the \
                 fixture's store did not hold, {diagnosis} — see the comment on \
                 this arm"
            );
        }
    };

    match &refusal {
        WorkspaceError::Git { command, stderr } => {
            assert!(
                command.contains("worktree add"),
                "the refusal names the git invocation that refused, not this layer's \
                 guess at it: {command}"
            );
            assert!(
                stderr.contains(&published),
                "and carries the revision, because a caller that cannot see which \
                 sha was unresolvable cannot correct it: {stderr}"
            );
        }
        other => panic!(
            "an unresolvable revision is a git failure carrying git's own \
             diagnostic, not {other}"
        ),
    }

    assert!(
        !root.join(attempt().0.as_str()).exists(),
        "a refused create_at leaves no worktree behind"
    );

    assert_eq!(
        CapabilityError::from(refusal).recurrence(),
        Recurrence::Correctable,
        "a fixture that could be given the object is an obstacle in front of the \
         run, not a verdict about it"
    );
}

#[test]
fn a_symlink_pointing_out_of_the_workspace_is_refused() {
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    let secret = dir.path().join("secret.txt");
    std::fs::write(&secret, "do not read me").unwrap();
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    std::os::unix::fs::symlink(&secret, ws.root().join("escape.txt")).unwrap();
    assert!(ws.read(&p("escape.txt")).is_err());
    let refusal = ws.write(&p("escape.txt"), "x");
    assert!(refusal.is_err());
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
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::trivial_repo(dir.path());
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
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    assert_eq!(ws.read(&p("src/lib.rs")).unwrap(), "pub fn f() {}\n");
    ws.write(&p("src/new.rs"), "pub fn n() {}\n").unwrap();
    assert_eq!(ws.read(&p("src/new.rs")).unwrap(), "pub fn n() {}\n");
}

#[test]
fn a_file_can_be_created_in_a_directory_that_does_not_exist_yet() {
    let _env = env_reader();
    let (ws, _dir) = workspace();

    ws.write(&p("src/newmod/deep/a.rs"), "pub fn a() {}\n")
        .unwrap();

    assert_eq!(
        ws.read(&p("src/newmod/deep/a.rs")).unwrap(),
        "pub fn a() {}\n"
    );
    assert!(
        ws.root().join("src/newmod/deep").is_dir(),
        "the intervening directories must exist inside the workspace"
    );
    assert_eq!(
        ws.changed_files().unwrap(),
        vec![p("src/newmod/deep/a.rs")],
        "and the file must be evidence like any other created file"
    );
}

#[test]
fn no_directory_created_on_the_models_behalf_lands_outside_the_workspace() {
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    std::os::unix::fs::symlink(&outside, ws.root().join("elsewhere")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("not-yet"), ws.root().join("dangle")).unwrap();

    for raw in ["elsewhere/newmod/a.rs", "dangle/newmod/a.rs"] {
        let refusal = ws.write(&p(raw), "pub fn a() {}\n");
        assert!(
            matches!(refusal, Err(WorkspaceError::Escape { .. })),
            "{raw} must be refused by containment, got {refusal:?}"
        );
    }
    assert!(
        !outside.join("newmod").exists(),
        "a refused write must not have created a directory outside the workspace"
    );
    assert!(
        !dir.path().join("not-yet").exists(),
        "and must not have created the far end of a dangling link either"
    );
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
fn an_ignore_rule_the_attempt_wrote_cannot_hide_what_it_created() {
    let _env = env_reader();
    let (ws, _dir) = workspace();

    ws.write(&p(".gitignore"), "*\n").unwrap();
    for i in 0..10 {
        ws.write(&p(&format!("evil{i}.rs")), "// smuggled\n")
            .unwrap();
    }

    let changed = ws.changed_files().unwrap();
    assert_eq!(
        changed.len(),
        11,
        "the rules the attempt wrote decided what the evidence says: {changed:?}"
    );
    for i in 0..10 {
        assert!(
            changed.contains(&p(&format!("evil{i}.rs"))),
            "evil{i}.rs was hidden by a rule the attempt authored: {changed:?}"
        );
    }
    assert!(
        changed.contains(&p(".gitignore")),
        "and the edit that tried to hide them is itself a change: {changed:?}"
    );
}

#[test]
fn an_ignore_rule_the_attempt_wrote_cannot_drag_build_output_in_either() {
    let _env = env_reader();
    let (ws, _dir) = workspace();
    std::fs::create_dir_all(ws.root().join("target/debug")).unwrap();
    std::fs::write(ws.root().join("target/debug/junk"), "x").unwrap();

    ws.write(&p(".gitignore"), "# nothing is ignored now\n")
        .unwrap();

    assert_eq!(
        ws.changed_files().unwrap(),
        vec![p(".gitignore")],
        "an attempt that un-ignored the build tree drowned its own evidence in it"
    );
}

#[test]
fn a_file_the_project_does_not_contain_is_not_readable() {
    let _env = env_reader();
    let (ws, _dir) = workspace();
    std::fs::create_dir_all(ws.root().join("target/debug")).unwrap();
    std::fs::write(
        ws.root().join("target/debug/fixture.d"),
        format!("{}/src/lib.rs:\n", ws.root().display()),
    )
    .unwrap();

    assert!(
        ws.read(&p("target/debug/fixture.d")).is_err(),
        "a build artefact is not part of the project and must not be served"
    );
    assert_eq!(ws.read(&p("src/lib.rs")).unwrap(), "pub fn f() {}\n");
    ws.write(&p("src/added.rs"), "pub fn a() {}\n").unwrap();
    assert_eq!(ws.read(&p("src/added.rs")).unwrap(), "pub fn a() {}\n");
}

#[test]
fn a_path_with_a_space_is_parsed_correctly() {
    let _env = env_reader();
    let (ws, _dir) = workspace();
    ws.write(&p("src/a file.rs"), "// new\n").unwrap();
    assert_eq!(ws.changed_files().unwrap(), vec![p("src/a file.rs")]);
}

#[test]
fn a_rename_reports_the_new_path_once_not_both() {
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
    let _env = env_reader();
    let (ws, _dir) = workspace();
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
    assert_eq!(
        lines.next(),
        Some("."),
        "a command result must carry no absolute path of the workspace it ran \
         in: stdout = {}",
        result.stdout
    );

    let failed = ws.run(&cmd("/bin/sh", &["-c", "exit 3"])).await.unwrap();
    assert_eq!(
        failed.exit_code, 3,
        "a non-zero exit is a result, not an error"
    );
}

#[tokio::test]
async fn neither_stream_of_a_command_result_names_the_workspace() {
    let _env = ENV.read().await;
    let (ws, _dir) = workspace();

    let result = ws
        .run(&cmd("/bin/sh", &["-c", "pwd; pwd >&2"]))
        .await
        .unwrap();

    let mut spellings = vec![ws.root().display().to_string()];
    if let Ok(canonical) = ws.root().canonicalize() {
        spellings.push(canonical.display().to_string());
    }
    for spelling in spellings {
        assert!(
            !result.stdout.contains(&spelling) && !result.stderr.contains(&spelling),
            "a command result named {spelling}: stdout = {} stderr = {}",
            result.stdout,
            result.stderr
        );
    }
    assert_eq!(
        (result.stdout.trim(), result.stderr.trim()),
        (".", "."),
        "the rewrite must be a relative path the reader can still use, not a \
         deletion"
    );
}

#[tokio::test]
async fn a_workspace_command_inherits_no_credential() {
    let (ws, _dir) = workspace();
    ws.run(&cmd("/usr/bin/true", &[])).await.unwrap();

    let guard = ENV.write().await;
    let inherited = std::env::var("RUSTUP_HOME").ok();
    unsafe {
        std::env::set_var("LITELLM_API_KEY", "sentinel-must-not-leak");
        std::env::remove_var("RUSTUP_HOME");
    }
    let without = ws.run(&cmd("/usr/bin/env", &[])).await.unwrap();
    unsafe { std::env::set_var("RUSTUP_HOME", "/nonexistent/rustup") };
    let with = ws.run(&cmd("/usr/bin/env", &[])).await.unwrap();
    unsafe {
        std::env::remove_var("LITELLM_API_KEY");
        match &inherited {
            Some(value) => std::env::set_var("RUSTUP_HOME", value),
            None => std::env::remove_var("RUSTUP_HOME"),
        }
    }
    drop(guard);

    for result in [&without, &with] {
        assert!(
            !result.stdout.contains("sentinel-must-not-leak"),
            "a workspace command must not observe the model credential: {}",
            result.stdout
        );
        assert!(!result.stdout.contains("LITELLM_API_KEY"));
    }
    assert_eq!(
        names(&without.stdout),
        ["HOME", "LANG", "PATH"],
        "with no toolchain locator to pass through, the allowlist is its three \
         unconditional names: {}",
        without.stdout
    );
    assert_eq!(
        names(&with.stdout),
        ["HOME", "LANG", "PATH", "RUSTUP_HOME"],
        "and exactly one more when the parent names one: {}",
        with.stdout
    );
}

fn names(stdout: &str) -> Vec<&str> {
    let mut seen: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('=').next())
        .collect();
    seen.sort_unstable();
    seen
}

#[tokio::test]
async fn a_toolchain_proxy_finds_its_toolchain_because_rustup_home_survives() {
    use std::os::unix::fs::PermissionsExt;

    let (ws, dir) = workspace();
    let shim = dir.path().join("cargo");
    std::fs::write(
        &shim,
        "#!/bin/sh\n\
         if [ -z \"$RUSTUP_HOME\" ]; then\n\
         \x20 echo 'error: no default toolchain configured' >&2\n\
         \x20 exit 1\n\
         fi\n\
         exit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();

    let guard = ENV.write().await;
    let inherited = std::env::var("RUSTUP_HOME").ok();
    unsafe { std::env::set_var("RUSTUP_HOME", "/nonexistent/rustup") };
    let result = ws.run(&cmd(&shim.to_string_lossy(), &[])).await.unwrap();
    unsafe {
        match &inherited {
            Some(value) => std::env::set_var("RUSTUP_HOME", value),
            None => std::env::remove_var("RUSTUP_HOME"),
        }
    }
    drop(guard);

    assert_eq!(
        result.exit_code, 0,
        "the toolchain locator did not survive the environment rebuild: {}",
        result.stderr
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
    assert!(err.to_string().contains("/bin/sh"), "got {err}");

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
async fn a_timed_out_command_takes_its_grandchildren_with_it() {
    let _env = ENV.read().await;
    let (ws, _dir) = workspace();
    let err = ws
        .run(&WorkspaceCommand {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "/bin/sh -c 'sleep 1; : > survived' & sleep 30".into(),
            ],
            timeout: Duration::from_millis(200),
        })
        .await
        .unwrap_err();

    assert!(matches!(err, WorkspaceError::Timeout { .. }), "got {err:?}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(
        !ws.root().join("survived").exists(),
        "a timed-out command must not leave a grandchild running"
    );
}

#[tokio::test]
async fn a_command_writes_its_caches_beside_the_worktree_and_not_into_it() {
    let _env = ENV.read().await;
    let (ws, _dir) = workspace();

    ws.run(&cmd(
        "/bin/sh",
        &["-c", "echo cached > \"$HOME/.toolcache\""],
    ))
    .await
    .unwrap();

    assert!(
        ws.changed_files().unwrap().is_empty(),
        "a tool writing to HOME must not appear as a change to the repository: {:?}",
        ws.changed_files().unwrap()
    );
    assert!(
        ws.home().join(".toolcache").exists(),
        "and it must still have landed somewhere the attempt owns"
    );
    assert!(
        !ws.home().starts_with(ws.root()),
        "which is only true while HOME is outside the worktree"
    );
}

#[test]
fn the_scratch_home_is_removed_with_the_worktree() {
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    let root = dir.path().join("ws");
    {
        let ws = Workspace::create(&repo, &root, &attempt(), token()).unwrap();
        std::fs::write(ws.home().join("cache"), "x").unwrap();
    }
    let leftovers: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    assert!(
        leftovers.is_empty(),
        "a dropped workspace must leave its scratch home behind no more than its \
         worktree: {leftovers:?}"
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
    assert!(!marker.exists(), "a cancelled command must not have run");
}
