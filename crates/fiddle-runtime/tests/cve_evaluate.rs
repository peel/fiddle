mod support;

use fiddle_core::Severity;
use fiddle_runtime::cve::dedup::FixedInCommits;
use fiddle_runtime::cve::fold::{fold, fold_commit_argv, plan_group, Fold, GroupPlan};
use fiddle_runtime::cve::group::GroupError;
use fiddle_runtime::evaluate::{evaluate, Outcome, Reason, RescanVerdict, Success};
use support::cve::*;

#[tokio::test]
async fn go_fmt_fails_on_output_even_at_exit_zero() {
    let r = evaluate(
        &contract(),
        &tree_where(GO_FMT, exit(0), stdout("main.go\n")),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(
        r.rejected(),
        "a printed filename means a file needed formatting"
    );
    assert_eq!(r.first_failure().expect("a failure").name, GO_FMT);
}

#[tokio::test]
async fn each_check_records_its_own_result() {
    let r = evaluate(&contract(), &tree_where(GO_VET, exit(1), stdout("")))
        .await
        .expect("an evaluation that was not cancelled");

    assert_eq!(
        r.checks().len(),
        5,
        "five results, not one aggregate exit code"
    );
    assert_eq!(r.checks().iter().filter(|c| !c.passed).count(), 1);
    assert_eq!(r.first_failure().expect("a failure").name, GO_VET);
}

#[tokio::test]
async fn each_check_ran_as_its_own_command() {
    let tree = tree_where(GO_VET, exit(1), stdout(""));
    evaluate(&contract(), &tree)
        .await
        .expect("an evaluation that was not cancelled");

    assert_eq!(
        tree.ran(),
        vec![GO_BUILD, GO_FMT, GO_VET, DOCKER_BUILD, WIZCLI_RESCAN],
        "five commands in the declared order, and the two after the failure too"
    );
}

#[tokio::test]
async fn an_artefact_check_passes_at_a_non_zero_exit() {
    let tree = green_tree().scanned_by("exit-nonzero-with-file");
    let r = evaluate(&contract(), &tree)
        .await
        .expect("an evaluation that was not cancelled");

    let rescan = r
        .checks()
        .iter()
        .find(|c| c.name == WIZCLI_RESCAN)
        .expect("the rescan among the results");
    assert!(rescan.passed, "the artefact decides, not the status line");
    assert_eq!(arm_exits_with("exit-nonzero-with-file"), 3);
    assert!(
        matches!(&rescan.outcome, Outcome::Scanned(report) if !report.scanner_version.is_empty()),
        "a passing artefact check carries the report it read, not just a boolean"
    );
    assert!(!r.rejected());
}

#[tokio::test]
async fn an_artefact_check_fails_when_the_scanner_wrote_nothing() {
    let tree = green_tree().scanned_by("exit-nonzero-no-file");
    let r = evaluate(&contract(), &tree)
        .await
        .expect("an evaluation that was not cancelled");

    assert!(r.rejected());
    assert_eq!(r.first_failure().expect("a failure").name, WIZCLI_RESCAN);
    assert_eq!(
        r.checks().iter().filter(|c| !c.passed).count(),
        1,
        "the four commands before it still passed"
    );
}

#[tokio::test]
async fn a_wrapped_go_fmt_still_fails_on_output() {
    let contract = contract_with(GO_FMT, WRAPPER, Success::ExitZeroAndNoOutput);
    let r = evaluate(
        &contract,
        &tree_where(WRAPPER, exit(0), stdout("main.go\n")),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(
        r.rejected(),
        "a rename is a rename, not a change of meaning"
    );
    assert_eq!(r.first_failure().expect("a failure").name, WRAPPER);
}

#[tokio::test]
async fn a_check_spelled_go_fmt_but_declared_exit_zero_passes_on_output() {
    let contract = contract_with(GO_FMT, GO_FMT, Success::ExitZero);
    let r = evaluate(&contract, &tree_where(GO_FMT, exit(0), stdout("main.go\n")))
        .await
        .expect("an evaluation that was not cancelled");

    assert!(
        !r.rejected(),
        "the declaration decides; no code may read `go fmt` out of a program name"
    );
}

#[tokio::test]
async fn first_failure_is_the_earliest_in_declared_order() {
    let tree = green_tree()
        .where_check(DOCKER_BUILD, exit(1), stdout(""))
        .where_check(GO_FMT, exit(0), stdout("main.go\n"));
    let r = evaluate(&contract(), &tree)
        .await
        .expect("an evaluation that was not cancelled");

    assert_eq!(r.checks().iter().filter(|c| !c.passed).count(), 2);
    assert_eq!(r.first_failure().expect("a failure").name, GO_FMT);
}

#[tokio::test]
async fn a_check_that_could_not_be_started_is_not_run_rather_than_failed() {
    let tree = green_tree().where_check_cannot_start(DOCKER_BUILD);
    let r = evaluate(&contract(), &tree)
        .await
        .expect("an evaluation that was not cancelled");

    assert!(r.rejected(), "an unanswered check is not an answered one");
    assert_eq!(r.checks().len(), 5, "the check after it still ran");
    let docker = r
        .checks()
        .iter()
        .find(|c| c.name == DOCKER_BUILD)
        .expect("docker build among the results");
    assert!(!docker.passed);
    assert!(
        matches!(&docker.outcome, Outcome::NotRun(why) if why.contains("no such program")),
        "recorded as unanswered and saying why, not as an exit status the tree chose"
    );
}

#[tokio::test]
async fn a_tree_that_passes_every_check_is_not_rejected() {
    let r = evaluate(&contract(), &green_tree())
        .await
        .expect("an evaluation that was not cancelled");

    assert_eq!(r.checks().len(), 5);
    assert!(r.checks().iter().all(|c| c.passed));
    assert!(r.first_failure().is_none());
    assert!(!r.rejected());
}

#[tokio::test]
async fn condition_b_catches_a_bump_that_trades_one_vulnerability_for_another() {
    let r = evaluate(
        &contract_for(&["CVE-2026-1"]),
        &tree_whose_rescan_reports(&["CVE-2026-NEW-HIGH"]),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(r.rejected());
    assert!(!r.accepted());
    assert!(
        r.first_failure().is_none(),
        "every check was green, so only the rescan's document can have refused this"
    );
    match r.reason() {
        Some(Reason::NewFindingAppeared { cve, severity }) => {
            assert_eq!(cve.as_str(), "CVE-2026-NEW-HIGH");
            assert_eq!(*severity, Severity::High);
        }
        other => panic!("expected the new finding to be named, found {other:?}"),
    }
}

#[tokio::test]
async fn a_finding_the_input_already_reported_is_not_a_new_one() {
    let contract = and_the_input_also_reported(contract_for(&["CVE-2026-1"]), &["CVE-2026-OTHER"]);
    let r = evaluate(&contract, &tree_whose_rescan_reports(&["CVE-2026-OTHER"]))
        .await
        .expect("an evaluation that was not cancelled");

    assert!(
        r.accepted(),
        "the group's own advisory is gone and nothing appeared that was not already there"
    );
    assert!(!r.rejected());
    assert_eq!(r.rescan(), &RescanVerdict::Cleared);
}

#[tokio::test]
async fn condition_a_checks_both_package_arrays() {
    let r = evaluate(
        &contract_for(&["CVE-2026-2"]),
        &tree_whose_rescan_reports_in_os_array(&["CVE-2026-2"]),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(r.rejected(), "an id surviving in osPackages is not gone");
    assert!(!r.accepted());
    assert!(
        r.first_failure().is_none(),
        "the scanner wrote its artefact, so no check failed; the document is what refused this"
    );
    assert_eq!(
        r.rescan(),
        &RescanVerdict::StillReported(vec![
            fiddle_core::AdvisoryId::parse("CVE-2026-2").expect("a fixture advisory id parses")
        ]),
        "and it says which advisory survived, not merely that one did"
    );
}

#[tokio::test]
async fn condition_a_catches_an_id_surviving_in_libraries() {
    let r = evaluate(
        &contract_for(&["CVE-2026-2"]),
        &tree_whose_rescan_reports(&["CVE-2026-2"]),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(r.rejected());
    assert!(r.first_failure().is_none());
    assert!(matches!(r.rescan(), RescanVerdict::StillReported(ids) if ids.len() == 1));
}

#[tokio::test]
async fn a_rescan_at_a_different_scanner_version_is_provisional_not_proof() {
    let r = evaluate(&contract_scanned_by("1.2.3"), &tree_rescanned_by("1.3.0"))
        .await
        .expect("an evaluation that was not cancelled");

    assert!(
        !r.accepted(),
        "the finding may have left the scan because the feed moved, not the tree"
    );
    assert!(!r.rejected(), "and nothing went wrong with the tree either");
    match r.reason() {
        Some(Reason::Provisional {
            scanned_at,
            rescanned_at,
        }) => {
            assert_eq!(scanned_at, "1.2.3");
            assert_eq!(rescanned_at, "1.3.0");
        }
        other => panic!("expected a provisional rescan naming both versions, found {other:?}"),
    }
}

#[tokio::test]
async fn a_repaired_tree_whose_rescan_clears_the_group_is_accepted() {
    let r = evaluate(&contract_scanned_by("1.2.3"), &tree_rescanned_by("1.2.3"))
        .await
        .expect("an evaluation that was not cancelled");

    assert!(r.checks().iter().all(|c| c.passed));
    assert!(r.accepted());
    assert!(!r.rejected());
    assert_eq!(r.rescan(), &RescanVerdict::Cleared);
    assert!(
        r.reason().is_none(),
        "a proved repair has nothing to explain"
    );
}

#[tokio::test]
async fn an_absent_os_array_in_a_rescan_is_not_proof() {
    let r = evaluate(
        &contract_for_a_partially_reported_rescan(),
        &tree_whose_rescan_omits_the_os_array(),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(
        r.first_failure().is_none(),
        "the scanner wrote its artefact, so no check failed"
    );
    assert!(
        !r.accepted(),
        "half the image was not looked at, so nothing about it was proved"
    );
    assert!(
        !r.rejected(),
        "and nothing went wrong with the tree either — what is missing is proof"
    );
    assert_eq!(
        r.rescan(),
        &RescanVerdict::NotObserved {
            array: "osPackages"
        },
        "and the record says which half of the image went unreported"
    );
}

#[tokio::test]
async fn a_rescan_reporting_no_os_packages_is_still_proof() {
    let r = evaluate(
        &contract_for_a_partially_reported_rescan(),
        &tree_whose_rescan_reports_no_os_packages(),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(
        r.accepted(),
        "an empty osPackages is what the scanner observed, not what it declined to say"
    );
    assert_eq!(r.rescan(), &RescanVerdict::Cleared);
}

#[tokio::test]
async fn an_absent_library_array_in_a_rescan_is_not_proof_either() {
    let r = evaluate(
        &contract_for_a_partially_reported_rescan(),
        &tree_whose_rescan_omits_the_library_array(),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(r.first_failure().is_none());
    assert!(!r.accepted());
    assert!(!r.rejected());
    assert_eq!(
        r.rescan(),
        &RescanVerdict::NotObserved { array: "libraries" }
    );
}

#[tokio::test]
async fn an_advisory_still_reported_is_refused_even_where_an_array_is_missing() {
    let r = evaluate(
        &contract_for(&["CVE-2026-2"]),
        &tree_whose_rescan_omits_the_os_array_and_reports(&["CVE-2026-2"]),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(r.rejected(), "an id that is there is there");
    assert!(!r.accepted());
    assert!(matches!(
        r.rescan(),
        RescanVerdict::StillReported(ids) if ids.len() == 1
    ));
}

#[tokio::test]
async fn a_rescan_this_build_cannot_read_is_not_evidence() {
    let r = evaluate(
        &contract_for(&["CVE-2026-1"]),
        &tree_whose_rescan_is_unreadable(),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(r.rejected());
    assert!(!r.accepted());
    assert!(
        r.first_failure().is_none(),
        "the artefact was written, so the check itself passed"
    );
    assert!(matches!(r.rescan(), RescanVerdict::Unreadable(_)));
}

#[tokio::test]
async fn a_contract_with_no_repair_premise_is_never_accepted() {
    let r = evaluate(&contract(), &green_tree())
        .await
        .expect("an evaluation that was not cancelled");

    assert!(!r.rejected());
    assert!(!r.accepted(), "there was no premise to prove");
    assert_eq!(r.rescan(), &RescanVerdict::NotCompared);
}

#[tokio::test]
async fn a_group_cleared_by_an_earlier_committed_bump_is_recorded_without_a_file_change() {
    let prior = rescan_from_committed_clean_group(&[]).await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::AlreadyResolved
    );
}

#[tokio::test]
async fn a_needs_work_groups_rescan_is_not_foldable() {
    let prior = rescan_from_needs_work_group(&[]).await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::Proceed,
        "its bump was reverted, so nothing on the branch fixes this group"
    );
}

#[tokio::test]
async fn a_clean_group_whose_bump_was_not_committed_is_not_foldable() {
    let prior = rescan_from_a_clean_group_that_was_not_committed(&[]).await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::Proceed,
        "a clean verdict about a tree the branch does not carry is not a fix"
    );
}

#[tokio::test]
async fn a_partially_cleared_group_proceeds() {
    let prior = rescan_from_committed_clean_group(&["CVE-2026-6"]).await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5", "CVE-2026-6"]), Some(&prior)),
        Fold::Proceed,
        "every id must be absent, not merely one"
    );
}

#[tokio::test]
async fn the_first_group_of_a_run_proceeds() {
    assert_eq!(fold(&group_of(&["CVE-2026-5"]), None), Fold::Proceed);
}

#[tokio::test]
async fn a_provisional_rescan_is_not_foldable_even_though_its_bump_was_committed() {
    let prior = rescan_from_a_committed_group_at_another_scanner_version().await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::Proceed,
        "a finding leaves a scan because the tree changed or because the feed did"
    );
}

#[tokio::test]
async fn an_array_the_rescan_never_reported_on_is_not_a_fold() {
    let prior = rescan_from_a_committed_group_that_reported_on_one_array().await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::Proceed,
        "an array the scanner never wrote supplies absences for free"
    );
}

#[test]
fn a_group_the_tree_already_carries_the_fix_for_is_recorded_as_resolved() {
    let plan = plan_group(
        &group_of(&["CVE-2026-5"]),
        Err(GroupError::AlreadyAtTheFix {
            current: "v0.28.0".to_string(),
            fixed: "v0.28.0".to_string(),
        }),
        None,
    );

    assert_eq!(
        plan,
        GroupPlan::AlreadyResolved,
        "the run moved this requirement itself; a verdict row would contradict \
         its own commit"
    );
}

#[tokio::test]
async fn a_refusal_this_build_cannot_move_past_is_blocked_even_where_a_fold_would_have_folded() {
    let prior = rescan_from_committed_clean_group(&[]).await;
    let why = "selecting a base-image tag needs a registry this build does not read";

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::AlreadyResolved,
        "the premise: this rescan is one the fold rule rests on, so a plan that \
         consulted it would fold"
    );
    assert_eq!(
        plan_group(
            &group_of(&["CVE-2026-5"]),
            Err(GroupError::Unselectable {
                why: why.to_string()
            }),
            Some(&prior),
        ),
        GroupPlan::Blocked(GroupError::Unselectable {
            why: why.to_string()
        }),
        "there is a move owed and this build may not make it, which is a verdict \
         and never a commit"
    );
}

#[tokio::test]
async fn a_target_whose_group_an_earlier_rescan_cleared_is_still_folded() {
    let prior = rescan_from_committed_clean_group(&[]).await;

    assert_eq!(
        plan_group(
            &group_of(&["CVE-2026-5"]),
            Ok("v0.35.0".to_string()),
            Some(&prior),
        ),
        GroupPlan::AlreadyResolved,
    );
}

#[test]
fn a_selected_target_no_clearance_covers_is_attempted() {
    assert_eq!(
        plan_group(&group_of(&["CVE-2026-5"]), Ok("v0.35.0".to_string()), None),
        GroupPlan::Attempt("v0.35.0".to_string()),
    );
}

#[test]
fn a_fold_is_recorded_without_rewriting_anything() {
    let argv = fold_commit_argv(&group_of(&["CVE-2026-5", "CVE-2026-6"]));

    assert!(
        argv.contains(&"--allow-empty".to_string()),
        "a fold changes no file, so without this there is no commit: {argv:?}"
    );
    assert!(
        !argv.iter().any(|argument| argument == "--amend"),
        "amending could rewrite a previous run's pushed commit: {argv:?}"
    );
    let body = argv.last().expect("a commit message");
    for cve in ["CVE-2026-5", "CVE-2026-6"] {
        assert!(
            FixedInCommits::read(body).names(cve),
            "the next run's log scan reads this body for ids, and {cve} is not in {body:?}"
        );
    }
}
