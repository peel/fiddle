#[path = "mod.rs"]
mod support;

use std::path::Path;
use support::cve::*;

#[test]
fn the_support_module_builds_distinguishable_worlds() {
    assert_ne!(
        go(direct()).go_mod(),
        go(indirect_via("gh.com/parent")).go_mod()
    );
    assert_ne!(go(direct()).go_mod(), go(stdlib()).go_mod());
    assert_ne!(
        report_with(libraries(&["CVE-1"]), os_packages(&[])).raw(),
        report_with(libraries(&[]), os_packages(&["CVE-1"])).raw()
    );
    assert_ne!(report_with_os_absent().raw(), report_with_os_empty().raw());
    assert_ne!(log_of(&["Fixes CVE-2026-1"]).raw(), log_of(&[]).raw());
}

#[test]
fn every_world_is_real_files_on_disk() {
    let w = go(direct());
    assert!(w.path().join("go.mod").is_file());
    assert!(w.path().is_absolute());
    assert!(
        w.path().join("go.sum").is_file(),
        "a tree with requirements"
    );
    assert!(
        !go(stdlib()).path().join("go.sum").exists(),
        "a tree with no requirements has nothing to sum"
    );
    assert_eq!(w.path(), w.path().canonicalize().unwrap());
}

#[test]
fn a_workspace_reads_its_go_mod_out_of_the_tree() {
    let w = go(direct());
    let edited = "module example.com/probed\n";
    std::fs::write(w.path().join("go.mod"), edited).unwrap();
    assert_eq!(w.go_mod(), edited);
}

#[test]
fn the_stub_path_is_named_without_requiring_the_stub_to_exist_yet() {
    assert!(support::wiz_stub("ok").program.ends_with("wiz_stub"));
    assert_eq!(support::wiz_stub("empty-file").args, vec!["empty-file"]);
}

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

#[test]
fn no_position_in_all_shapes_is_claimed_twice() {
    let mut listed: Vec<usize> = all_shapes().iter().map(Shape::index).collect();
    listed.sort_unstable();
    listed.dedup();
    assert_eq!(
        listed,
        (0..all_shapes().len()).collect::<Vec<_>>(),
        "two entries of all_shapes are the same shape, so one shape is unreached"
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

#[test]
fn no_position_in_canonical_reports_is_claimed_twice() {
    let mut listed: Vec<usize> = canonical_reports()
        .iter()
        .map(ReportVariant::index)
        .collect();
    listed.sort_unstable();
    listed.dedup();
    assert_eq!(
        listed,
        (0..canonical_reports().len()).collect::<Vec<_>>(),
        "two entries of canonical_reports are the same variant"
    );
}

#[test]
fn a_workspace_is_a_committed_repository_whose_record_excludes_its_own_arrangement() {
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
    assert!(w.is_clean());
    assert_eq!(w.git_calls().len(), 1, "asking is not doing");
}

#[test]
fn a_workspace_reports_a_stray_file_as_unclean() {
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
    let path = {
        let w = go(direct());
        w.path().to_path_buf()
    };
    assert!(!path.exists(), "{} outlived its handle", path.display());
}

#[test]
fn each_sentinel_is_unmistakable_for_another() {
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
fn the_scripted_scanner_is_a_binary_that_exists() {
    let stub = support::wiz_stub("ok");
    assert!(
        Path::new(&stub.program).exists(),
        "{} is not on disk, so no suite can drive a scan",
        stub.program
    );
    assert_eq!(stub.args, vec!["ok".to_string()]);
}

#[test]
fn the_offline_go_publishes_the_releases_a_sweep_asks_it_about() {
    assert_eq!(
        offline_go(
            Path::new("/nowhere"),
            &["list", "-m", "-versions", SWEEP_MODULE]
        )
        .text()
        .trim(),
        format!("{SWEEP_MODULE} {SWEEP_VULNERABLE} {SWEEP_FIXED}")
    );

    let unknown = offline_go(
        Path::new("/nowhere"),
        &["list", "-m", "-versions", "example.com/never-released"],
    );
    assert_eq!(unknown.text().trim(), "example.com/never-released");
    assert_eq!(
        unknown.code, 0,
        "having no releases is an answer, not a refusal"
    );

    assert!(!Path::new("/nowhere").exists());
}
