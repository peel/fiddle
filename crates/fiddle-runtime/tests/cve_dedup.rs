//! Which findings this run has already dealt with, and where that is read from.
//!
//! The subject is [`fiddle_runtime::cve::dedup`]. A scan is a photograph of an
//! image that was built some time before the branch reached its current state,
//! so a report routinely names findings the tree has since moved past and
//! findings an earlier commit on this very branch already fixed. Proposing those
//! again is not merely noise: Task 9's [`GroupError::AlreadyAtTheFix`] exists
//! because a group whose tree is already above the fix selects *the latest patch
//! inside the fixed minor*, which is a **downgrade** wearing a security fix's
//! commit message. This suite covers the stage that should have dropped such a
//! finding before grouping ever saw it.
//!
//! # Two package types, two authorities, and neither one may borrow the other's
//!
//! A **library** finding is settled by comparing versions, because a Go module
//! version is a number and the tree is the thing that holds the current one. The
//! scanner's `current` field is not consulted: it records what the *image* held
//! when it was scanned, and the entire point of this stage is that the branch
//! has moved since.
//!
//! An **OS** finding cannot be settled that way at all. What fixes one is moving
//! a base image tag, and base image tags do not sort — `3.20`, `3.20-slim`,
//! `bookworm`, a digest. So the authority is the branch's own record of what it
//! did: the commit bodies between `origin/<base>` and `HEAD`.
//!
//! # What is *not* an authority, and the incident that settled it
//!
//! A pull request's body is not evidence. It is written when the pull request is
//! opened, it lists what a scan found, and a rescan after the fix lands leaves
//! the ones that were not fixed sitting in that same prose. On 2026-08-12 a
//! merged grpc pull request's body named `CVE-2026-45045` as unrelated leftover
//! still present, dedup read the mention as a fix, and the finding was dropped
//! while it was still open. **A mention is evidence a CVE was seen, not that it
//! was fixed.** [`nothing_consults_pull_request_search_or_bodies`] is the
//! standing proof that no code path here reaches a forge.
//!
//! # Why a truncated history is a refusal rather than an empty set
//!
//! Under a `--depth 1` clone the log names nothing, so every OS finding reads as
//! *unfixed*. That output is safe — the run proposes fixes that are already
//! applied, which wastes a reviewer's time and endangers nothing — and that is
//! exactly why it would stay broken indefinitely if it were silent. Nobody
//! chases a wasteful-but-correct report. So the precondition is asserted, and
//! the diagnostic names the knob: `fetch-depth`.
//!
//! [`GroupError::AlreadyAtTheFix`]: fiddle_runtime::cve::group::GroupError::AlreadyAtTheFix

mod support;

use fiddle_runtime::cve::dedup::{
    already_fixed, commit_log_dedup, commit_log_dedup_with, FixedInCommits,
};
use support::cve::{
    direct, finding_fixed_at, forge_recording_calls, full_clone, go, go_with_shipped, log_of,
    os_finding, shallow_clone,
};

/// The branch every fixture clone is forked from.
const BASE: &str = "main";

/// The module the library lanes are about, spelled as both producers spell it:
/// the tree writes `v0.54.1` and a scanner writes `0.54.0`.
const MODULE: &str = "golang.org/x/crypto";

// ---------------------------------------------------------------------------
// A library, settled against the tree
// ---------------------------------------------------------------------------

/// The tree is past the fix, so the finding is gone.
///
/// `v0.54.1` against a fix at `0.54.0` is deliberately the mixed-`v` pair
/// `version::at_least` exists for — a comparison that stripped one operand and
/// not the other would order these the wrong way round.
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

/// The same world one release lower, which is what stops the row above passing
/// against an `already_fixed` that answers `true` for every library.
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

/// The pair that separates a numeric comparison from a lexical one.
///
/// `0.54.10` is above `0.54.3` and below it as text, and the lexical reading
/// fails in the dangerous direction: it calls the older tree the newer one and
/// drops an open finding. Both directions are asserted, because a comparison
/// that simply always answered `false` would satisfy the first row alone.
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

/// A module the tree does not require at all leaves the finding open.
///
/// The fail-closed direction, and the one a `unwrap_or_default()` on the
/// resolver's answer would get wrong: an absent record read as an empty version
/// would compare as `0`, which is below every fix, so this row would still pass
/// — and the row above it would still pass — while a *main module* record, which
/// also carries no version, would too. The assertion that matters is that
/// nothing here reports a fix for a module it never found.
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

// ---------------------------------------------------------------------------
// An OS package, settled against the branch's own commits
// ---------------------------------------------------------------------------

/// One body, two advisories, and each is matched on its own.
///
/// The world is `go(direct())`: a tree with no OS package in it whatsoever. So
/// the positive row cannot have been produced by a version comparison — there is
/// nothing in that tree to compare — and it is the commit body or nothing.
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
    // The second id in the same body, which a scan that stopped at the first
    // match — or matched the body as one string against one id — would miss.
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

/// An id that is a prefix of another id is not dropped by it.
///
/// `CVE-2026-1` inside `CVE-2026-10` is what a `contains` reading would call a
/// match, and the mistake is silent in the direction that closes an open
/// finding. Both rows are here because the boundary has two sides, and a matcher
/// anchored only at the start would pass the first.
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

/// The two arms do not lend each other their evidence.
///
/// A commit body naming a library's advisory does not drop that finding: the
/// tree is the authority for a library, and a body outlives the change that
/// wrote it — a revert leaves the sentence and takes the fix. Without this row
/// an implementation that consulted the commit log for *every* finding would
/// pass every other test in this file.
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

// ---------------------------------------------------------------------------
// Where the commit bodies come from
// ---------------------------------------------------------------------------

/// A real repository, a real range, and the set that comes out of it.
///
/// This is the row that stops [`a_shallow_history_fails_loudly_naming_fetch_depth`]
/// being vacuous: on its own, a test that asserts a refusal cannot tell a
/// working reader that refuses one bad input from a reader that refuses every
/// input. Two commits ahead of `origin/main`, each naming its own advisory, and
/// a third advisory named by neither.
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

/// A `--depth 1` clone refuses, and the refusal names the knob that fixes it.
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

// ---------------------------------------------------------------------------
// The forge nothing here talks to
// ---------------------------------------------------------------------------

/// No code path in this module consults pull request search, or a pull request.
///
/// A PR body lists CVEs still present after a rescan, so a mention is evidence a
/// CVE was SEEN, not fixed. This dropped CVE-2026-45045 on 2026-08-12 when a
/// merged grpc PR's body named it as unrelated leftover.
///
/// **Why this is not an assertion over an empty list.** The recorder is not a
/// forge stand-in that sits unused waiting to not be called — it is the
/// `Spawn` seam, the single way `dedup` starts any program at all, so the list
/// it holds is *everything* the module ran. The `git log` row is what proves the
/// recorder was wired in and would have caught a `gh`; the remaining rows are
/// then a statement about the module rather than about the fixture. Swap the log
/// read for a `gh pr list --search` and this test fails on the second assertion.
///
/// [`already_fixed`] itself takes no `Spawn` and so cannot start a program by
/// construction; it is called here anyway, after the read, so that the recorder
/// covers the whole of the path an OS finding takes.
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
