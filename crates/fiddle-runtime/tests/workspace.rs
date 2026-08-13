//! The per-attempt workspace, exercised against a real git repository.
//!
//! These are integration tests rather than unit tests because every property
//! here is about the filesystem and about `git` — an isolated checkout, a
//! teardown that survives the ways a caller can leave early, and a containment
//! check that only means something once a symlink actually exists on disk. None
//! of that can be faked from inside the crate without testing the fake instead.

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
    let repo = fixture::trivial_repo(dir.path());
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), cancel).unwrap();
    (ws, dir)
}

/// Run git in `dir` and hand back its stdout, or its stderr if it refused.
///
/// [`fixture::git`] panics on a non-zero exit, which is right for a fixture step
/// that must have worked and wrong for the questions asked below. "Does this
/// store hold that object" is *asked* by a git command that is expected to fail
/// on the interesting answer, and a helper that panicked on it could not report
/// it.
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

/// Whether `repo`'s own object store holds `object`, asked of git.
///
/// The question the whole of
/// [`a_revision_the_fixture_can_only_fetch_is_refused_by_name_and_nothing_fetches`]
/// turns on, so it is asked rather than inferred from how the fixture was built.
fn store_holds(repo: &Path, object: &str) -> bool {
    git_out(repo, &["cat-file", "-e", object]).is_ok()
}

/// Commit everything in `repo` and hand back the sha it produced.
///
/// The identity is passed per invocation for the reason
/// [`fixture::trivial_repo`] gives: a CI runner has no `user.email` and `git
/// commit` refuses outright without one.
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
    // Teardown must survive an early return, a `?`, and a panic. M0 learned this
    // for bundle publication with a Drop guard; the same applies here.
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
    // The explicit call and the guard both run on the happy path, so the second
    // removal has to be a no-op rather than a failed `git worktree remove` on a
    // path git no longer knows about.
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
    // The redirect path's premise, and the only test that has ever asked for it:
    // `create_at`'s `revision` reached git through no assertion at this tier, so
    // a version that dropped it and branched from `HEAD` like
    // `Workspace::create` would have produced a working worktree, a readable
    // file and a resolvable sha — everything except the *right* commit.
    //
    // Both branch points are taken in one test because only the contrast is the
    // property. The two commits differ in the one file that is read, so "the
    // revision was honoured" and "the fixture happened to be there already" do
    // not render identically.
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
    // **The limitation `Workspace::create_at` documents, reproduced rather than
    // described.** A redirect's second attempt names the commit its published
    // branch is at, and that commit is in the fixture's store only because the
    // first attempt was made in a worktree *of this fixture*. A process that did
    // not make it — the next machine, a rebuilt runner, a fresh clone — has the
    // sha and not the object, and nothing here fetches.
    //
    // Two repositories stand in for the two machines: `origin` makes the commit,
    // and the fixture is a clone taken *before* it existed. That is the honest
    // shape of the failure. A syntactically invalid revision would fail here too,
    // and it would prove something else — that git rejects nonsense — because the
    // refusal and the store would both be out of the picture.
    //
    // **The direction this pins, and the direction it cannot.** A fetch that
    // *worked* would change the outcome, and the tripwire below is what notices;
    // measured, by adding `git fetch -q origin` ahead of the `worktree add`. A
    // fetch that was added and *failed* would leave the store exactly as it is and
    // this same refusal behind it, so it is invisible from out here — the
    // credential boundary is pinned by outcome, not by counting git children, and
    // nothing at this tier can count them. Closing that half means asserting on
    // the processes the workspace spawns, which nothing in this crate does yet.
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

    // The denominators. Without the first, "nothing fetched" cannot be told from
    // "there was nothing to fetch"; without the second, the refusal below could
    // be about anything.
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
        // TRIPWIRE. Two causes reach this arm and they want opposite responses,
        // so the store is *asked* which one it was rather than the next reader
        // being left to work it out. Both were measured: a `git fetch -q origin`
        // added ahead of the `worktree add` takes the first branch, and a failed
        // `worktree add` that silently retries at `HEAD` takes the second.
        Ok(_) => {
            let diagnosis = if store_holds(&fixture_repo, &published) {
                // The object arrived, so something resolves the revision now — a
                // fetch, an alternate, a caller contract that ensures it first.
                // The limitation this test exists to pin has been lifted, and the
                // test has to be rewritten rather than deleted: assert *how*.
                // Which credential the resolution carries and where it comes
                // from, whether `git::publish` is still the only
                // credential-carrying git child or deliberately is not, and that
                // a resolution which fails is still a correctable
                // `WorkspaceError` naming the revision. Then say the same at
                // `ProposeChange::produce_from`, the caller the constraint was
                // documented for.
                "and the object is in the store now, so something resolves the \
                 revision — the documented limitation has been lifted and this \
                 test has to be rewritten, not deleted"
            } else {
                // Nothing arrived, so the refusal was swallowed and this is a
                // worktree branched from somewhere else — the one thing the
                // documentation says must not happen quietly. A defect, not a
                // lifted limitation, and the test stays exactly as it is.
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

    // Not swallowed: nothing was branched from somewhere else and reported as a
    // success.
    assert!(
        !root.join(attempt().0.as_str()).exists(),
        "a refused create_at leaves no worktree behind"
    );
    // There is deliberately no `!store_holds` assertion here. It would fail on no
    // mutation the arm above does not already fail on first — a fetch that worked
    // never reaches this line, and a fetch that failed leaves the store exactly as
    // this refusal found it. The store question belongs where it discriminates,
    // which is the tripwire.

    // Not permanent. `CapabilityError::Workspace` is `Correctable`, and this is
    // the reason that arm has to stay that way: the operator fetches, or points
    // the run at the fixture the branch was made in, and the same invocation
    // succeeds. Asserted through the real error rather than a hand-built one, and
    // it is this crate's only assertion over that arm.
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
    // The counterpart to the refusals: resolution has to still admit a path
    // whose leaf does not exist yet — the walk stops at the last component that
    // is there and joins the rest on, and that is the branch the dangling-link
    // check above sits in front of.
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
    // **A repair that cannot add a module is a repair capability with a hole in
    // it.** Adding a file under a new directory is the ordinary shape of "extract
    // this into its own module", and until resolution walked to the deepest
    // *existing* ancestor it was not merely refused — it could not be expressed:
    // `canonicalize` on an absent parent fails with ENOENT, which surfaced as
    // `WorkspaceError::Io` and reached the model as "writing the file did not
    // succeed", with nothing anywhere telling it that a directory was what was
    // missing.
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
    // The containment guarantee has to hold for the directories resolution
    // creates, not only for the leaf it opens — a `create_dir_all` that followed
    // a link would build a tree on the operator's filesystem before any
    // containment check saw the leaf at all. Every rung is checked by the same
    // rule: an existing component is canonicalized and must resolve inside, and
    // only once a component is genuinely absent does the rest of the path become
    // plain names joined onto a directory already proven inside.
    let _env = env_reader();
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::trivial_repo(dir.path());
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    // A link to a real directory outside, and a link to nothing at all: the
    // first would have `create_dir_all` build a tree out there, the second is
    // the shape whose leaf check already existed and which must keep firing when
    // it is an *interior* component rather than the leaf.
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
    // **The changed-file set is evidence, so it may not be an output the thing
    // being judged can shape.** `.gitignore` is an ordinary versioned file: it
    // parses, it resolves, and `write_file` will write it. A derivation that
    // asked git *with the worktree's own ignore rules applied* would therefore
    // be asking a question the agent had already answered for it — ten created
    // files reported as one, the changed-file cap bypassed, and a published
    // evidence reference naming a count that is not true.
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
    // The other direction of the same rule, and the one that says the fix is
    // not simply `--ignored`. Excluding build output is what makes the evidence
    // readable at all — one `cargo test` writes thousands of files nobody
    // edited. So the exclusion has to survive an attempt that *removes* the
    // rule producing it: the rules that shape the evidence are the project's,
    // as committed, and an attempt cannot add to them or take from them.
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
    // A build tree is inside the workspace and syntactically innocent, and its
    // files routinely carry absolute host paths — cargo writes them into every
    // `.d`. "The project you are repairing" is what git says the project is,
    // and what is not in it is not readable.
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
    // Not a rule that refuses everything: a file the project does contain, and
    // one the attempt created itself, both still read.
    assert_eq!(ws.read(&p("src/lib.rs")).unwrap(), "pub fn f() {}\n");
    ws.write(&p("src/added.rs"), "pub fn a() {}\n").unwrap();
    assert_eq!(ws.read(&p("src/added.rs")).unwrap(), "pub fn a() {}\n");
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
    // `git status` collapses a wholly-new directory into one entry,
    // `?? src/newmod/` — a directory, which is not a changed *file* and hides
    // how many there are. Created files are therefore derived from
    // `git ls-files --others` instead, which names files and never directories,
    // and which cannot be silenced by a `status.showUntrackedFiles=no` in an
    // operator's config either.
    //
    // The directories are the workspace's to make. This test used to call
    // `create_dir_all` on the agent's behalf, which was the defect in
    // miniature: what it was proving about the evidence was true, and the model
    // could not have got into that position at all.
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
    // **The command's working directory is the workspace, and the result says
    // so relatively.** Both halves in one assertion, because one implies the
    // other here: `pwd` prints `getcwd()`, and the only reason it can come back
    // as `.` is that `getcwd()` was the workspace root and `Workspace::run`
    // rewrote it. A command that had run somewhere else would print that
    // somewhere else, unrewritten, and this would fail.
    //
    // Which spelling `getcwd()` reports is not something this has to know any
    // more — a macOS temporary directory is reached through a symlink, so it is
    // the far side of it, and both spellings are rewritten.
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

/// **Neither stream of a command result names the workspace, in any spelling.**
///
/// Asserted against a command that prints the path on *both* streams, and
/// against both spellings of it, because the consumer that matters most reads
/// `stderr`: `CapabilityError::CheckFailed` embeds it, and the orchestration
/// publishes that error's rendering as a run's `reason` and as a progress
/// summary. Before this guarantee lived in `Workspace::run` it lived at two
/// call sites in the `run_check` tool, so the model was protected from the
/// worktree path and the published bundle was not.
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
    // Resolve the tool PATH before the environment is mutated, so that nothing
    // inside `run` reads `environ` while the sentinel is being set below.
    ws.run(&cmd("/usr/bin/true", &[])).await.unwrap();

    let guard = ENV.write().await;
    let inherited = std::env::var("RUSTUP_HOME").ok();
    // SAFETY: `ENV` is held for writing, so no other test in this binary is
    // reading or writing the environment for the duration of the mutations
    // below. `env_clear` means the child's environment is built from the
    // explicit allowlist rather than captured from this process; `run` reads
    // exactly one variable of its own — `RUSTUP_HOME` — so it is the write lock,
    // rather than the absence of any reader, that makes this sound.
    unsafe {
        std::env::set_var("LITELLM_API_KEY", "sentinel-must-not-leak");
        std::env::remove_var("RUSTUP_HOME");
    }
    // Both shapes of the allowlist, decided here rather than inherited from
    // whatever the runner happens to export: a test whose expected set depended
    // on the ambient environment would assert one thing locally and another on
    // CI, which is the class of failure this scenario exists to catch.
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
    // The sentinel proves this one credential does not survive; the exhaustive
    // checks prove *nothing* does. A denylist would have to be extended for
    // every credential added later, and these are the assertions that fail if
    // `env_clear` is ever dropped for a selective `env_remove`.
    //
    // Two exact sets rather than one loosened to a `contains`, because
    // `RUSTUP_HOME` is the allowlist's one conditional entry and "conditional"
    // has to mean *exactly* present-when-the-parent-has-one. A `contains` would
    // admit any number of further names nobody ever argued for.
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

/// Every variable name a child saw, sorted.
fn names(stdout: &str) -> Vec<&str> {
    let mut seen: Vec<&str> = stdout
        .lines()
        .filter_map(|line| line.split('=').next())
        .collect();
    seen.sort_unstable();
    seen
}

/// A toolchain proxy inside a workspace command can still find its toolchain.
///
/// The allowlist assertion above says `RUSTUP_HOME` reaches the child. This says
/// why anybody should care, and it is the assertion that would have caught the
/// defect: rustup's `cargo` is not a compiler but a proxy, which resolves which
/// toolchain to exec through `RUSTUP_HOME` and refuses outright without one —
/// and `run` points `HOME` at a per-attempt scratch directory precisely so that
/// nothing is found under `$HOME/.rustup`. So on every machine whose Rust came
/// from rustup, which is every machine this project's merge gate runs on, a
/// nested `cargo test --offline` failed before it compiled anything, and the
/// capability reported `CheckFailed` over repairs that were correct.
///
/// Proven against a shim rather than against rustup itself, because a test that
/// needed rustup installed would be skipped in exactly the dev shell where this
/// is written. The shim asserts the one behaviour that defines the proxy: no
/// `RUSTUP_HOME`, no toolchain, non-zero exit.
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
    // SAFETY: as above — `ENV` is held for writing, so this is the only thread
    // in this binary touching the environment.
    unsafe { std::env::set_var("RUSTUP_HOME", "/nonexistent/rustup") };
    // Named by absolute path, so what is proven is that the *environment*
    // reaches the child. Going through `PATH` would prove nothing extra and
    // could not be arranged anyway: `TOOL_PATH` is resolved once per process.
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
async fn a_timed_out_command_takes_its_grandchildren_with_it() {
    // The gap the previous test cannot see. `kill_on_drop` reaps the process
    // this runtime holds a handle to and nothing underneath it, and the command
    // this runner exists to run — `cargo test` — spawns test binaries. A check
    // that hung would be reported as `Timeout` perfectly correctly while its
    // children carried on.
    //
    // The shell below is that shape made small: it backgrounds a grandchild that
    // outlives it and then waits past the deadline. Killing only the direct
    // child leaves `survived` to appear a second later; killing the process
    // group does not. Verified by removing `process_group(0)` and watching this
    // assertion, and only this one, fail.
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
    // `HOME` is where a tool that insists on a cache will put one, and the
    // worktree is the tree whose diff is this attempt's evidence. Pointing one
    // at the other means the first `cargo test` inside a workspace reports
    // `.cargo/.package-cache` and its siblings as files the agent changed —
    // fabricated paths in published evidence, and three of the changed-file cap
    // spent on nothing. The scratch home is still inside the per-attempt
    // directory, so it is still thrown away; it is just not inside the diff.
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
    // It is an ordinary directory git knows nothing about, so nothing removes it
    // unless this does. A workspace root left holding one `.home` per attempt is
    // the leak the Drop guard exists to prevent, one indirection along.
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
    // Cancellation has to prevent the effect, not merely end the future.
    assert!(!marker.exists(), "a cancelled command must not have run");
}
