//! The per-attempt workspace, exercised against a real git repository.
//!
//! These are integration tests rather than unit tests because every property
//! here is about the filesystem and about `git` — an isolated checkout, a
//! teardown that survives the ways a caller can leave early, and a containment
//! check that only means something once a symlink actually exists on disk. None
//! of that can be faked from inside the crate without testing the fake instead.

mod fixture;

use fiddle_core::AttemptId;
use fiddle_runtime::workspace::{Workspace, WorkspaceError, WorkspacePath};
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

#[test]
fn a_workspace_is_an_isolated_checkout_that_disappears() {
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
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::broken_crate(dir.path());
    let mut ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    ws.remove().unwrap();
    ws.remove()
        .expect("a second removal must be a no-op, not a git failure");
}

#[test]
fn a_symlink_pointing_out_of_the_workspace_is_refused() {
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
    let dir = tempfile::tempdir().unwrap();
    let repo = fixture::broken_crate(dir.path());
    let ws = Workspace::create(&repo, &dir.path().join("ws"), &attempt(), token()).unwrap();

    assert_eq!(ws.read(&p("src/lib.rs")).unwrap(), "pub fn f() {}\n");
    ws.write(&p("src/new.rs"), "pub fn n() {}\n").unwrap();
    assert_eq!(ws.read(&p("src/new.rs")).unwrap(), "pub fn n() {}\n");
}
