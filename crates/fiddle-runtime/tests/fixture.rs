//! A throwaway git repository for runtime integration tests.
//!
//! Shared by `mod fixture;` from the test files that need a real repository to
//! branch a worktree from. Cargo also compiles this file as a test target of its
//! own — it contains no tests, and the `dead_code` allow below is what keeps that
//! empty target, and any future test file that uses only part of this module,
//! from failing the `-D warnings` gate.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// A one-commit repository holding a trivial crate.
///
/// The identity is passed per-invocation rather than assumed: a CI runner has no
/// `user.email` configured and `git commit` refuses outright without one, so a
/// fixture that relied on the ambient config would pass locally and fail there.
///
/// `target/` is gitignored so that `git status --porcelain` over a worktree of
/// this repository reports what a build *changed*, not what it *produced*.
pub fn broken_crate(dir: &Path) -> PathBuf {
    let repo = dir.join("fixture");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    git(&repo, &["init", "-q", "."]);
    git(&repo, &["add", "-A"]);
    git(
        &repo,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    repo
}

/// Run git in `dir`, panicking with its stderr if it fails.
///
/// A fixture that failed silently would surface as an unrelated assertion
/// failure further down the test, so the panic carries what git actually said.
pub fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|source| panic!("could not run git {args:?}: {source}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
