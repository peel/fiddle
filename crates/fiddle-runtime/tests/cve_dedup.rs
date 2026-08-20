mod support;

use fiddle_runtime::cve::dedup::{commit_log_dedup, commit_log_dedup_with, FixedInCommits};
use support::cve::{forge_recording_calls, full_clone, log_of, shallow_clone};

const BASE: &str = "main";

#[test]
fn a_full_history_reads_the_advisories_this_branch_already_carries() {
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
fn one_commit_body_may_name_several_advisories_and_each_is_its_own_answer() {
    let log = log_of(&["fix(security): bump base image\n\nFixes CVE-2026-1, CVE-2026-2"]);
    let fixed = FixedInCommits::read(log.raw());

    assert!(fixed.names("CVE-2026-1"), "the first id the body names");
    assert!(fixed.names("CVE-2026-2"), "and the second");
    assert!(
        !fixed.names("CVE-2026-9"),
        "an advisory no commit names is not carried by this branch"
    );
}

#[test]
fn an_advisory_is_not_matched_by_a_longer_id_that_contains_it() {
    let log = log_of(&["fix(security): bump base image\n\nFixes CVE-2026-10"]);
    let fixed = FixedInCommits::read(log.raw());

    assert!(fixed.names("CVE-2026-10"), "the id the body actually names");
    assert!(
        !fixed.names("CVE-2026-1"),
        "CVE-2026-1 is not CVE-2026-10, and reading it as one would credit this \
         branch with work it does not carry"
    );
}

#[test]
fn a_shallow_history_fails_loudly_naming_fetch_depth() {
    let ws = shallow_clone();
    let err = commit_log_dedup(ws.path(), BASE).unwrap_err();
    assert!(
        err.to_string().contains("fetch-depth"),
        "under a shallow clone the log names nothing and the branch reads as \
         carrying no work — quiet output, which is exactly why the precondition \
         is asserted rather than relied on: {err}"
    );
}

#[test]
fn nothing_consults_pull_request_search_or_bodies() {
    let ws = full_clone(&["fix(security): bump base image\n\nFixes CVE-2026-1"]);
    let forge = forge_recording_calls();

    let fixed = commit_log_dedup_with(ws.path(), BASE, &forge).expect("a full clone answers");
    assert!(fixed.names("CVE-2026-1"), "the log is what was read");

    let calls = forge.calls();
    assert!(
        calls.iter().any(|call| call.starts_with("git log ")),
        "the recorder saw the log read, so it would have seen a forge call too: {calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|call| !call.contains("--search") && !call.contains("pr view")),
        "dedup reads the log, never a pull request's prose: {calls:?}"
    );
    assert!(
        calls.iter().all(|call| call.starts_with("git ")),
        "git is the only program the commit log is read with: {calls:?}"
    );
}
