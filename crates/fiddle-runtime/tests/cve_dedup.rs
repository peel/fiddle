mod support;

use fiddle_runtime::cve::dedup::{
    already_fixed, commit_log_dedup, commit_log_dedup_with, FixedInCommits,
};
use support::cve::{
    direct, finding_fixed_at, forge_recording_calls, full_clone, go, go_with_shipped, log_of,
    os_finding, shallow_clone,
};

const BASE: &str = "main";

const MODULE: &str = "golang.org/x/crypto";

#[tokio::test]
async fn a_library_already_at_or_above_the_fix_is_dropped_reading_the_tree() {
    let ws = go_with_shipped(MODULE, "v0.54.1");
    assert!(
        already_fixed(
            &finding_fixed_at(MODULE, "0.54.0"),
            &ws,
            &FixedInCommits::read("")
        )
        .await
        .expect("a tree on disk answers"),
        "the tree ships v0.54.1 and the fix landed in 0.54.0"
    );
}

#[tokio::test]
async fn a_library_below_the_fix_is_kept() {
    let ws = go_with_shipped(MODULE, "v0.53.9");
    assert!(
        !already_fixed(
            &finding_fixed_at(MODULE, "0.54.0"),
            &ws,
            &FixedInCommits::read("")
        )
        .await
        .expect("a tree on disk answers"),
        "v0.53.9 is below the fix at 0.54.0"
    );
}

#[tokio::test]
async fn a_library_version_is_compared_numerically_and_not_lexically() {
    let past = go_with_shipped(MODULE, "v0.54.10");
    assert!(
        already_fixed(
            &finding_fixed_at(MODULE, "0.54.3"),
            &past,
            &FixedInCommits::read("")
        )
        .await
        .expect("a tree on disk answers"),
        "0.54.10 is above 0.54.3 numerically, and below it as text"
    );

    let behind = go_with_shipped(MODULE, "v0.54.3");
    assert!(
        !already_fixed(
            &finding_fixed_at(MODULE, "0.54.10"),
            &behind,
            &FixedInCommits::read("")
        )
        .await
        .expect("a tree on disk answers"),
        "and 0.54.3 is below 0.54.10, which the text reading gets backwards"
    );
}

#[tokio::test]
async fn a_module_the_tree_does_not_require_is_not_already_fixed() {
    let ws = go(direct());
    assert!(
        !already_fixed(
            &finding_fixed_at("gh.com/never-required", "0.0.1"),
            &ws,
            &FixedInCommits::read("")
        )
        .await
        .expect("a tree on disk answers"),
        "the tree cannot say a module it does not have is fixed"
    );
}

#[tokio::test]
async fn an_os_finding_is_dropped_only_from_a_commit_body() {
    let log = log_of(&["fix(security): bump base image\n\nFixes CVE-2026-1, CVE-2026-2"]);
    let fixed = FixedInCommits::read(log.raw());
    let ws = go(direct());

    assert!(
        already_fixed(&os_finding("CVE-2026-2"), &ws, &fixed)
            .await
            .expect("a tree on disk answers"),
        "base image tags do not sort; the branch's own record is the authority"
    );
    assert!(
        already_fixed(&os_finding("CVE-2026-1"), &ws, &fixed)
            .await
            .expect("a tree on disk answers"),
        "one body may name several advisories and each is its own answer"
    );
    assert!(
        !already_fixed(&os_finding("CVE-2026-9"), &ws, &fixed)
            .await
            .expect("a tree on disk answers"),
        "an advisory no commit names is still open"
    );
}

#[tokio::test]
async fn an_advisory_is_not_matched_by_a_longer_id_that_contains_it() {
    let log = log_of(&["fix(security): bump base image\n\nFixes CVE-2026-10"]);
    let fixed = FixedInCommits::read(log.raw());
    let ws = go(direct());

    assert!(
        already_fixed(&os_finding("CVE-2026-10"), &ws, &fixed)
            .await
            .expect("a tree on disk answers"),
        "the id the body actually names"
    );
    assert!(
        !already_fixed(&os_finding("CVE-2026-1"), &ws, &fixed)
            .await
            .expect("a tree on disk answers"),
        "CVE-2026-1 is not CVE-2026-10, and reading it as one closes an open finding"
    );
}

#[tokio::test]
async fn a_commit_body_does_not_drop_a_library_finding() {
    let log = log_of(&["fix(deps): bump crypto\n\nFixes CVE-2026-0008"]);
    let ws = go_with_shipped(MODULE, "v0.53.9");
    assert!(
        !already_fixed(
            &finding_fixed_at(MODULE, "0.54.0"),
            &ws,
            &FixedInCommits::read(log.raw())
        )
        .await
        .expect("a tree on disk answers"),
        "the tree still ships v0.53.9, whatever a commit body says about it"
    );
}

#[test]
fn a_full_history_reads_what_the_branch_already_fixed() {
    let ws = full_clone(&[
        "fix(security): bump base image\n\nFixes CVE-2026-1",
        "fix(security): and again\n\nFixes CVE-2026-2",
    ]);
    let fixed = commit_log_dedup(ws.path(), BASE).expect("a full clone answers");

    assert!(fixed.names("CVE-2026-1"), "the older commit's advisory");
    assert!(fixed.names("CVE-2026-2"), "and the newer one's");
    assert!(!fixed.names("CVE-2026-9"), "and nothing else");
}

#[test]
fn a_shallow_history_fails_loudly_naming_fetch_depth() {
    let ws = shallow_clone();
    let err = commit_log_dedup(ws.path(), BASE).unwrap_err();
    assert!(
        err.to_string().contains("fetch-depth"),
        "under a shallow clone every OS finding reads as unfixed — safe output, \
         which is exactly why the precondition is asserted rather than relied on: {err}"
    );
}

#[tokio::test]
async fn nothing_consults_pull_request_search_or_bodies() {
    let ws = full_clone(&["fix(security): bump base image\n\nFixes CVE-2026-1"]);
    let forge = forge_recording_calls();

    let fixed = commit_log_dedup_with(ws.path(), BASE, &forge).expect("a full clone answers");
    let _ = already_fixed(&os_finding("CVE-2026-1"), &ws, &fixed)
        .await
        .expect("a tree on disk answers");

    let calls = forge.calls();
    assert!(
        calls.iter().any(|call| call.starts_with("git log ")),
        "the recorder saw the log read, so it would have seen a forge call too: {calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|call| !call.contains("--search") && !call.contains("pr view")),
        "dedup reads the tree and the log, never a pull request's prose: {calls:?}"
    );
    assert!(
        calls.iter().all(|call| call.starts_with("git ")),
        "git is the only program the already-fixed set is read with: {calls:?}"
    );
}
