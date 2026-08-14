//! The support module's tests about itself.
//!
//! # Why these are not in the file they are about
//!
//! `support/mod.rs` and `support/cve.rs` are compiled *into* every suite that
//! writes `mod support;` — eight of them today. A `#[test]` in either file would
//! therefore be registered once per includer and run nine times, which inflates
//! the suite's count with copies of one assertion and makes every future suite
//! pay for them. Cargo offers no per-target `cfg` to gate them with, so the tests
//! live in this target's own root instead, where they are compiled exactly once.
//!
//! That is also why `tests/fixture.rs` — the other shared fixture module in this
//! crate — contains no tests at all. This file is what that module does not have:
//! somewhere for a shared fixture to be asserted *about*, so a helper the whole
//! milestone is built on is not first exercised by the lane that trusts it.
//!
//! The path `tests/support/support.rs` follows `tests/gh_stub/gh_stub.rs`: a
//! target whose root sits in a directory is named after the target, because
//! cargo auto-discovers `tests/*/main.rs` and a file claimed by two targets
//! builds twice and warns on every invocation.

#[path = "mod.rs"]
mod support;

use std::path::Path;
use support::cve::*;

#[test]
fn the_support_module_builds_distinguishable_worlds() {
    // A support module that compiles but produces indistinguishable worlds is
    // worse than none: every lane built on it would pass for the wrong reason.
    assert_ne!(
        go(direct()).go_mod(),
        go(indirect_via("gh.com/parent")).go_mod()
    );
    assert_ne!(go(direct()).go_mod(), go(stdlib()).go_mod());
    assert_ne!(
        report_with(libraries(&["CVE-1"]), os_packages(&[])).raw(),
        report_with(libraries(&[]), os_packages(&["CVE-1"])).raw()
    );
    // An ABSENT array and an EMPTY one are different worlds, and Task 6 asserts
    // the projection tells them apart. If this module cannot build both, that
    // assertion cannot exist.
    assert_ne!(report_with_os_absent().raw(), report_with_os_empty().raw());
    assert_ne!(log_of(&["Fixes CVE-2026-1"]).raw(), log_of(&[]).raw());
}

#[test]
fn every_world_is_real_files_on_disk() {
    // Not an in-memory double: a lane's evidence must be something a reader can
    // go and inspect after the fact.
    let w = go(direct());
    assert!(w.path().join("go.mod").is_file());
    assert!(w.path().is_absolute());
}

#[test]
fn the_stub_path_is_named_without_requiring_the_stub_to_exist_yet() {
    // Task 4 builds the wizcli stub binary. This only fixes WHERE it will be,
    // so Task 4 has one place to satisfy rather than inventing its own.
    assert!(support::wiz_stub("ok").program.ends_with("wiz_stub"));
    assert_eq!(support::wiz_stub("empty-file").args, vec!["empty-file"]);
}

/// The two spot checks above name three shapes; this one covers the rest.
///
/// Written as a walk over [`all_shapes`] rather than as more `assert_ne!` pairs
/// because a shape added later is then covered without anybody remembering to
/// add a line — which is the failure the pairs would have.
#[test]
fn no_two_go_shapes_build_the_same_tree() {
    let built: Vec<(usize, String)> = all_shapes()
        .into_iter()
        .map(|shape| (shape.index(), go(shape).go_mod()))
        .collect();
    for (i, (left, left_mod)) in built.iter().enumerate() {
        for (right, right_mod) in built.iter().skip(i + 1) {
            assert_ne!(
                left_mod, right_mod,
                "shapes {left} and {right} build the same go.mod, so a test that \
                 tells them apart is passing for some other reason"
            );
        }
    }
}

/// Every shape reaches [`all_shapes`], so the walk above is over all of them.
///
/// [`Shape::index`]'s match is exhaustive, so a new shape cannot be added
/// without being given a position; this is what makes the positions a bijection
/// onto the list, and therefore what makes the list complete.
#[test]
fn every_go_shape_is_listed() {
    let mut listed: Vec<usize> = all_shapes().iter().map(Shape::index).collect();
    let count = listed.len();
    listed.sort_unstable();
    listed.dedup();
    assert_eq!(
        listed,
        (0..count).collect::<Vec<_>>(),
        "a shape whose index is missing from all_shapes is a world no test reaches"
    );
}

#[test]
fn no_two_reports_the_lanes_need_are_the_same_bytes() {
    let reports = distinct_reports();
    for (i, (left, left_raw)) in reports.iter().enumerate() {
        for (right, right_raw) in reports.iter().skip(i + 1) {
            assert_ne!(
                left_raw.raw(),
                right_raw.raw(),
                "{left} and {right} are the same document"
            );
        }
    }
}

/// The same completeness guard as [`every_go_shape_is_listed`], for the reports.
#[test]
fn every_report_variant_is_listed() {
    let mut listed: Vec<usize> = canonical_reports()
        .iter()
        .map(ReportVariant::index)
        .collect();
    let count = listed.len();
    listed.sort_unstable();
    listed.dedup();
    assert_eq!(
        listed,
        (0..count).collect::<Vec<_>>(),
        "a variant missing from canonical_reports is a report no test reaches"
    );
}

#[test]
fn a_workspace_is_a_committed_repository_whose_record_excludes_its_own_arrangement() {
    // `git_calls` answers what the code under test did to this repository. The
    // arrangement is git too — a fresh workspace is initialised and committed —
    // and recording that would put the fixture's own `add` into the answer, so
    // Task 15's "no call staged everything" would be asserting this module.
    let w = go(direct());
    assert!(
        !w.git(&["rev-parse", "--verify", "HEAD"]).is_empty(),
        "construction really did run git, so the empty record below is a choice"
    );
    assert_eq!(
        w.git_calls(),
        ["rev-parse --verify HEAD"],
        "the record holds what went through the handle and nothing from before it"
    );
}

#[test]
fn a_workspace_reports_a_stray_file_as_unclean() {
    // `is_clean` is read by Task 8 only to assert a probing edit was reverted, and
    // a version answering `true` unconditionally would satisfy that forever. The
    // positive case is what makes the negative one an assertion.
    let w = go(direct());
    assert!(w.is_clean(), "nothing has touched this tree yet");
    std::fs::write(w.path().join("go.mod"), "module example.com/edited\n").unwrap();
    assert!(
        !w.is_clean(),
        "an edited go.mod is exactly what must not pass"
    );
}

#[test]
fn a_shallow_clone_is_shallow_and_an_ordinary_workspace_is_not() {
    // Task 13 refuses to read a fixed set out of a truncated history, and that
    // refusal is only about truncation if the two worlds really differ in it.
    assert_eq!(
        shallow_clone()
            .git(&["rev-parse", "--is-shallow-repository"])
            .trim(),
        "true"
    );
    assert_eq!(
        go(direct())
            .git(&["rev-parse", "--is-shallow-repository"])
            .trim(),
        "false"
    );
}

#[test]
fn a_commit_log_carries_the_bodies_it_was_given() {
    // `log_of(&[]).raw()` is empty, so a reader answering "" for everything would
    // satisfy the distinguishability test above. This is the positive case beside
    // that negative one.
    let log = log_of(&[
        "fix(security): bump\n\nFixes CVE-2026-1",
        "chore: unrelated",
    ]);
    assert!(log.raw().contains("Fixes CVE-2026-1"));
    assert!(log.raw().contains("chore: unrelated"));
    assert!(log.path().join(".git").is_dir(), "a real history on disk");
}

#[test]
fn a_world_is_removed_from_the_disk_when_its_handle_drops() {
    // A fixture that leaked its temporary directory would leave one behind per
    // test per run, which is the failure `Workspace`'s own Drop guard exists for.
    let path = {
        let w = go(direct());
        w.path().to_path_buf()
    };
    assert!(!path.exists(), "{} outlived its handle", path.display());
}

#[test]
fn each_sentinel_is_unmistakable_for_another() {
    // Every sentinel is read by an assertion of the form "this string is not in
    // that output". Two sentinels where one contains the other would collapse two
    // such assertions into one, and the weaker would pass on the other's evidence.
    for (i, left) in ALL_SENTINELS.iter().enumerate() {
        for right in ALL_SENTINELS.iter().skip(i + 1) {
            assert!(
                !left.contains(right) && !right.contains(left),
                "{left} and {right} cannot be searched for independently"
            );
        }
    }
}

#[test]
fn the_derived_stub_path_is_a_placeholder_until_task_4_declares_the_binary() {
    // A tripwire rather than a comment, so it fails on the day the blocker is
    // removed instead of being noticed some time after. See `wiz_stub`.
    assert!(
        !Path::new(&support::wiz_stub("ok").program).exists(),
        "the wiz_stub binary now exists, so Task 4 has landed: replace the sibling \
         derivation in `wiz_stub` with env!(\"CARGO_BIN_EXE_wiz_stub\"), which cargo \
         guarantees, and delete this test"
    );
}
