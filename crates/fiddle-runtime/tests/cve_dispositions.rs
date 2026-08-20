mod support;

use fiddle_core::{AdvisoryId, ProjectedFinding, RunOutcome, Severity};
use fiddle_runtime::agent::{FindingDisposition, RepairReport};
use fiddle_runtime::capability::{ForbiddenShape, GroupStatus, MigrationAttempt, NeedsWork};
use fiddle_runtime::cve::project::{project, Projection};
use fiddle_runtime::cve::verdict::{
    disposition, report_of, Attempted, Budget, InProgress, Judgement, Landed, Run, Verdict,
    NO_PUBLISHED_FIX, REPORT_FILE,
};
use fiddle_runtime::evaluate::{evaluate, Evaluation, Reason};
use fiddle_runtime::scanner::Scanner;
use fiddle_runtime::workspace::WorkspacePath;
use std::collections::HashSet;
use std::mem::discriminant;
use support::cve::{
    absent_scanner, contract_for, every_fixture_grade, exit, image, libraries, os_packages,
    report_with, scan_of, scanner_with, stdout, tree_whose_rescan_reports, unfixed_libraries,
    GO_VET,
};

const FIXABLE_CVE: &str = "CVE-2026-3001";

const BLOCKED_CVE: &str = "CVE-2026-3002";

const SECOND_CVE: &str = "CVE-2026-3003";

fn projection_of(document: serde_json::Value) -> Projection {
    project(&scan_of(document), &every_fixture_grade())
        .expect("a fixture document this build can project")
}

fn clean_scan_no_findings() -> Run {
    let run = Run::scanned(projection_of(document_of(&report_with(
        libraries(&[]),
        os_packages(&[]),
    ))));
    let projection = run.projection().expect("a scan that produced a projection");
    assert_eq!(
        (
            projection.fixable().count(),
            projection.upstream_blocked().count()
        ),
        (0, 0),
        "this world's premise is both of the projection's sets being empty"
    );
    run
}

fn both_sets_empty() -> Run {
    clean_scan_no_findings()
}

fn verdicts_only() -> Run {
    let run = Run::scanned(projection_of(document_of(&report_with(
        unfixed_libraries(&[BLOCKED_CVE]),
        os_packages(&[]),
    ))));
    let projection = run.projection().expect("a scan that produced a projection");
    assert_eq!(
        (
            projection.fixable().count(),
            projection.upstream_blocked().count()
        ),
        (0, 1),
        "this world's premise is an empty fixable set and a non-empty blocked one"
    );
    run
}

fn open_pr_covers_it() -> Run {
    let mut run = one_fixable_finding();
    run.in_progress = Some(InProgress {
        number: 7,
        covers: vec![advisory(FIXABLE_CVE)],
    });
    run
}

fn fixed_in_the_tree() -> Run {
    let mut run = one_fixable_finding();
    run.already_fixed = vec![advisory(FIXABLE_CVE)];
    run
}

async fn one_group_clean() -> Run {
    let mut run = two_fixable_findings();
    run.attempted = vec![
        attempted_group(
            FIXABLE_CVE,
            clean_group(FIXABLE_CVE).await,
            true,
            Vec::new(),
        ),
        attempted_group(
            SECOND_CVE,
            needs_work_group(SECOND_CVE).await,
            true,
            Vec::new(),
        ),
    ];
    run.landed = Some(Landed {
        branch: "security/cve-remediation-20260817".to_string(),
        pull_request: 12,
    });
    assert!(
        run.attempted
            .iter()
            .any(|group| group.status == GroupStatus::Clean),
        "this world's premise is one clean group"
    );
    assert!(
        run.attempted
            .iter()
            .any(|group| group.status != GroupStatus::Clean),
        "and one that is not, so an `all` rule and an `any` rule disagree here"
    );
    run
}

async fn every_group_needs_work() -> Run {
    let mut run = two_fixable_findings();
    run.attempted = vec![
        attempted_group(
            FIXABLE_CVE,
            needs_work_group(FIXABLE_CVE).await,
            true,
            Vec::new(),
        ),
        attempted_group(
            SECOND_CVE,
            needs_work_group(SECOND_CVE).await,
            false,
            Vec::new(),
        ),
    ];
    assert!(
        run.attempted
            .iter()
            .all(|group| group.status != GroupStatus::Clean),
        "this world's premise is that nothing ended clean"
    );
    run
}

async fn scanner_unusable() -> Run {
    let scanner = scanner_with(absent_scanner());
    let why = scanner
        .scan(&image())
        .await
        .expect_err("a scanner that is not installed cannot produce a report")
        .to_string();
    assert!(
        why.contains("-which-is-not-installed") && why.contains("could not be started"),
        "the diagnostic should name the program that could not be started, got {why}"
    );
    Run::unusable(why)
}

async fn findings_beyond_budget(count: usize, bound: usize) -> Run {
    let cves: Vec<String> = (0..count).map(|at| format!("CVE-2026-40{at:02}")).collect();
    let borrowed: Vec<&str> = cves.iter().map(String::as_str).collect();
    let mut run = Run::scanned(projection_of(document_of(&report_with(
        libraries(&borrowed),
        os_packages(&[]),
    ))));

    let fixable: Vec<ProjectedFinding> = run
        .projection()
        .expect("a scan that produced a projection")
        .fixable()
        .cloned()
        .collect();
    assert_eq!(
        fixable.len(),
        count,
        "this world's premise is {count} fixable findings"
    );

    let (taken, deferred) = Budget::of(bound).apply(fixable);
    let mut attempted = Vec::new();
    for finding in &taken {
        attempted.push(attempted_group(
            finding.cve.as_str(),
            clean_group(finding.cve.as_str()).await,
            true,
            Vec::new(),
        ));
    }
    run.attempted = attempted;
    run.deferred = deferred;
    run.landed = Some(Landed {
        branch: "security/cve-remediation-20260817".to_string(),
        pull_request: 12,
    });
    run
}

fn one_fixable_finding() -> Run {
    let run = Run::scanned(projection_of(document_of(&report_with(
        libraries(&[FIXABLE_CVE]),
        os_packages(&[]),
    ))));
    assert_eq!(
        run.projection()
            .expect("a scan that produced a projection")
            .fixable()
            .count(),
        1,
        "this world's premise is one fixable finding"
    );
    run
}

fn two_fixable_findings() -> Run {
    Run::scanned(projection_of(document_of(&report_with(
        libraries(&[FIXABLE_CVE, SECOND_CVE]),
        os_packages(&[]),
    ))))
}

async fn clean_group(cve: &str) -> GroupStatus {
    let status = GroupStatus::of(&cleanly_evaluated(cve).await, &[], None);
    assert_eq!(
        status,
        GroupStatus::Clean,
        "this group's premise is an evaluation Task 14.b calls clean"
    );
    status
}

async fn needs_work_group(cve: &str) -> GroupStatus {
    let evaluation = evaluate(
        &contract_for(&[cve]),
        &tree_whose_rescan_reports(&[]).where_check(GO_VET, exit(1), stdout("")),
    )
    .await
    .expect("an evaluation that was not cancelled");
    let status = GroupStatus::of(&evaluation, &[], None);
    assert!(
        matches!(
            status,
            GroupStatus::NeedsWork {
                reason: NeedsWork::CheckFailed { .. }
            }
        ),
        "this group's premise is a failing check, got {status:?}"
    );
    status
}

async fn cleanly_evaluated(cve: &str) -> Evaluation {
    evaluate(&contract_for(&[cve]), &tree_whose_rescan_reports(&[]))
        .await
        .expect("an evaluation that was not cancelled")
}

fn attempted_group(
    cve: &str,
    status: GroupStatus,
    claimed_complete: bool,
    forbidden: Vec<ForbiddenShape>,
) -> Attempted {
    Attempted {
        findings: vec![finding_for(cve)],
        status,
        attempt: MigrationAttempt {
            report: RepairReport {
                changed_files: vec!["go.mod".to_string()],
                summary: format!("bumped the module {cve} is against"),
                claimed_complete,
                findings: vec![FindingDisposition {
                    cve: cve.to_string(),
                    attempted: true,
                    note: "bumped it".to_string(),
                }],
            },
            changed: vec![WorkspacePath::parse("go.mod").expect("a workspace-relative path")],
            forbidden,
            undeclared: None,
        },
    }
}

fn a_group_declining(cve: &str, status: GroupStatus) -> Attempted {
    let mut group = attempted_group(cve, status, false, Vec::new());
    group.attempt.report.findings = vec![FindingDisposition {
        cve: cve.to_string(),
        attempted: false,
        note: "no fix I can apply to this project without reading a registry".to_string(),
    }];
    group
}

fn unfixed_finding_for(cve: &str) -> ProjectedFinding {
    projection_of(document_of(&report_with(
        unfixed_libraries(&[cve]),
        os_packages(&[]),
    )))
    .upstream_blocked()
    .next()
    .cloned()
    .expect("a fixture document with one finding the scanner published no fix for")
}

fn finding_for(cve: &str) -> ProjectedFinding {
    projection_of(document_of(&report_with(
        libraries(&[cve]),
        os_packages(&[]),
    )))
    .fixable()
    .next()
    .cloned()
    .expect("a fixture document with one fixable finding")
}

fn advisory(cve: &str) -> AdvisoryId {
    AdvisoryId::parse(cve).expect("a fixture advisory id parses")
}

fn document_of(report: &support::cve::Report) -> serde_json::Value {
    support::cve::document_of(report)
}

#[tokio::test]
async fn seven_causes_reach_seven_distinguishable_results() {
    let cases: Vec<(&str, Run, RunOutcome, Reason)> = vec![
        (
            "both sets empty",
            both_sets_empty(),
            RunOutcome::Completed,
            Reason::NothingToDo,
        ),
        (
            "fixable empty, blocked non-empty",
            verdicts_only(),
            RunOutcome::Completed,
            Reason::VerdictsOnly,
        ),
        (
            "an open pull request covers it",
            open_pr_covers_it(),
            RunOutcome::Completed,
            Reason::AlreadyInProgress,
        ),
        (
            "already fixed in the tree",
            fixed_in_the_tree(),
            RunOutcome::Completed,
            Reason::AlreadyFixed,
        ),
        (
            "one group clean",
            one_group_clean().await,
            RunOutcome::Completed,
            Reason::PullRequest,
        ),
        (
            "every group needs work",
            every_group_needs_work().await,
            RunOutcome::Completed,
            Reason::UnsafeWithoutDirection,
        ),
        (
            "the scanner never ran",
            scanner_unusable().await,
            RunOutcome::Retryable {
                reason: fiddle_core::Published::of("ignored: the discriminant is the claim"),
            },
            Reason::ScanUnusable { why: String::new() },
        ),
    ];

    let mut seen = HashSet::new();
    for (cause, world, outcome, reason) in &cases {
        let reached = disposition(world);
        assert_eq!(
            discriminant(reached.outcome()),
            discriminant(outcome),
            "{cause} should reach {outcome:?}, got {:?}",
            reached.outcome()
        );
        assert_eq!(
            discriminant(reached.reason()),
            discriminant(reason),
            "{cause} should reach {reason:?}, got {:?}",
            reached.reason()
        );
        assert!(
            seen.insert((
                discriminant(reached.outcome()),
                discriminant(reached.reason())
            )),
            "two causes must not produce one result: {cause} reached a pair \
             an earlier cause already had"
        );
    }

    assert_eq!(
        seen.len(),
        cases.len(),
        "{} causes reached {} distinguishable outcome-and-reason pairs",
        cases.len(),
        seen.len()
    );
    assert_eq!(seen.len(), 7);
}

#[tokio::test]
async fn seven_causes_reach_seven_distinguishable_published_records() {
    let cases: Vec<(&str, Run, &str)> = vec![
        ("both sets empty", both_sets_empty(), "nothing_to_do"),
        (
            "fixable empty, blocked non-empty",
            verdicts_only(),
            "verdicts_only",
        ),
        (
            "an open pull request covers it",
            open_pr_covers_it(),
            "already_in_progress",
        ),
        (
            "already fixed in the tree",
            fixed_in_the_tree(),
            "already_fixed",
        ),
        ("one group clean", one_group_clean().await, "pull_request"),
        (
            "every group needs work",
            every_group_needs_work().await,
            "unsafe_without_direction",
        ),
        (
            "the scanner never ran",
            scanner_unusable().await,
            "scan_unusable",
        ),
    ];

    let mut published = HashSet::new();
    let mut without_reason = HashSet::new();
    for (cause, world, row) in &cases {
        let reached = disposition(world);
        let document =
            serde_json::to_value(reached.published()).expect("a published record serializes");
        assert_eq!(
            document["reason"], *row,
            "{cause} should publish the row it reached: {document}"
        );
        assert!(
            published.insert(document.to_string()),
            "two causes must not publish one document: {cause} published one an              earlier cause already had — {document}"
        );

        if matches!(reached.reason(), Reason::ScanUnusable { .. }) {
            assert!(
                matches!(reached.outcome(), RunOutcome::Retryable { .. }),
                "and it is the row the exit code separates: {:?}",
                reached.outcome()
            );
            continue;
        }
        let mut bare = document.clone();
        bare.as_object_mut().unwrap().remove("reason");
        assert!(
            without_reason.insert(bare.to_string()),
            "{cause}'s row must be checkable from its evidence and not from its              own name alone — with the reason removed it is a document another              row already published: {bare}"
        );
    }

    assert_eq!(
        published.len(),
        cases.len(),
        "{} causes published {} distinguishable documents",
        cases.len(),
        published.len()
    );
    assert_eq!(published.len(), 7);
    assert_eq!(without_reason.len(), 6);
}

#[tokio::test]
async fn an_unusable_scan_is_retryable_and_carries_the_scanner_s_own_diagnostic() {
    let reached = disposition(&scanner_unusable().await);

    let RunOutcome::Retryable { reason } = reached.outcome() else {
        panic!(
            "Design §3: retryable, never NoChange — got {:?}",
            reached.outcome()
        );
    };
    assert!(
        reason.as_str().contains("-which-is-not-installed"),
        "the outcome should carry the scanner's own diagnostic, got {reason:?}"
    );

    let Reason::ScanUnusable { why } = reached.reason() else {
        panic!("got {:?}", reached.reason());
    };
    assert!(why.contains("-which-is-not-installed"), "got {why}");
}

#[tokio::test]
async fn a_scanner_that_found_nothing_and_one_that_never_ran_are_not_the_same_result() {
    let found_nothing = disposition(&clean_scan_no_findings());
    let never_ran = disposition(&scanner_unusable().await);

    assert_ne!(found_nothing, never_ran);
    assert_ne!(
        discriminant(found_nothing.outcome()),
        discriminant(never_ran.outcome()),
        "the two differ in the half the exit code is derived from, not only in \
         the half a human reads"
    );
}

#[test]
fn an_empty_fixable_set_with_verdicts_touches_no_branch_and_no_pull_request() {
    let reached = disposition(&verdicts_only());

    assert!(!reached.verdicts().is_empty());
    assert!(reached.branch().is_none() && reached.pull_request().is_none());
    assert_ne!(disposition(&both_sets_empty()).reason(), reached.reason());
}

#[tokio::test]
async fn only_a_run_that_committed_something_names_the_branch_it_committed_to() {
    let landed = disposition(&one_group_clean().await);
    assert_eq!(landed.branch(), Some("security/cve-remediation-20260817"));
    assert_eq!(landed.pull_request(), Some(12));

    let in_progress = disposition(&open_pr_covers_it());
    assert_eq!(
        in_progress.pull_request(),
        Some(7),
        "the row's whole content is which pull request to go and look at"
    );
    assert_eq!(
        in_progress.branch(),
        None,
        "nothing was committed, so there is no branch this run put work on"
    );
}

#[test]
fn a_clean_image_is_not_an_image_somebody_had_already_fixed() {
    let reached = disposition(&clean_scan_no_findings());

    assert_eq!(reached.reason(), &Reason::NothingToDo);
    assert!(
        reached.already_fixed().is_empty(),
        "there was nothing to have been fixed"
    );
    assert_ne!(
        reached.reason(),
        disposition(&fixed_in_the_tree()).reason(),
        "and the world that really was already fixed reaches the other row, so \
         this is not an assertion two worlds could both satisfy"
    );
}

#[test]
fn a_fix_awaiting_review_and_a_fix_already_in_the_tree_are_not_one_row() {
    let awaiting = disposition(&open_pr_covers_it());
    let landed = disposition(&fixed_in_the_tree());

    assert_eq!(awaiting.reason(), &Reason::AlreadyInProgress);
    assert_eq!(landed.reason(), &Reason::AlreadyFixed);
    assert!(
        awaiting.already_fixed().is_empty(),
        "nothing is fixed in this tree; it is fixed on a branch nobody merged"
    );
    assert_eq!(landed.already_fixed(), &[advisory(FIXABLE_CVE)]);
    assert_eq!(
        landed.pull_request(),
        None,
        "there is no pull request to point a reader at"
    );
}

#[tokio::test]
async fn a_group_that_was_attempted_and_reverted_is_not_a_group_that_was_never_attempted() {
    let attempted = disposition(&every_group_needs_work().await);
    let never = disposition(&verdicts_only());

    assert_eq!(attempted.reason(), &Reason::UnsafeWithoutDirection);
    assert_eq!(never.reason(), &Reason::VerdictsOnly);
    assert_eq!(
        attempted.verdicts()[0].verdict,
        Judgement::NeedsWork,
        "something was tried and taken back"
    );
    assert_eq!(
        never.verdicts()[0].verdict,
        Judgement::UpstreamBlocked,
        "there was no move to make"
    );
}

#[tokio::test]
async fn one_clean_group_beside_one_that_needs_work_still_opens_the_pull_request() {
    let reached = disposition(&one_group_clean().await);

    assert_eq!(reached.reason(), &Reason::PullRequest);
    assert_eq!(
        reached.verdicts().len(),
        1,
        "the group that was reverted is still reported, and the clean one is not"
    );
    assert_eq!(reached.verdicts()[0].cve, advisory(SECOND_CVE));
}

#[tokio::test]
async fn a_finding_deferred_by_the_budget_is_reported_as_deferred() {
    let reached = disposition(&findings_beyond_budget(6, 5).await);

    assert_eq!(reached.deferred().len(), 1);
    assert_eq!(
        reached.deferred()[0].bound,
        5,
        "name the bound that deferred it"
    );
    assert!(
        !reached
            .verdicts()
            .iter()
            .any(|verdict| verdict.cve == reached.deferred()[0].cve),
        "deferred is not a verdict"
    );
    assert!(
        !reached.already_fixed().contains(&reached.deferred()[0].cve),
        "and not already-fixed either, so the next run may still take it"
    );
}

#[tokio::test]
async fn deferring_a_finding_does_not_change_what_the_run_came_to() {
    let reached = disposition(&findings_beyond_budget(6, 5).await);

    assert_eq!(reached.reason(), &Reason::PullRequest);
    assert_eq!(reached.outcome(), &RunOutcome::Completed);
    assert!(reached.branch().is_some());
}

#[tokio::test]
async fn a_run_inside_its_budget_defers_nothing() {
    let reached = disposition(&findings_beyond_budget(5, 5).await);
    assert!(reached.deferred().is_empty());
}

#[tokio::test]
async fn a_verdict_row_carries_what_the_attempt_said_about_that_finding() {
    let mut run = one_fixable_finding();
    run.attempted = vec![attempted_group(
        FIXABLE_CVE,
        needs_work_group(FIXABLE_CVE).await,
        true,
        Vec::new(),
    )];

    let json =
        serde_json::to_value(&disposition(&run).verdicts()[0]).expect("a verdict serializes");
    assert_eq!(
        json["attempted"],
        serde_json::json!(true),
        "the attempt worked on this finding and the row says so: {json}"
    );
    assert_eq!(
        json["note"],
        serde_json::json!("bumped it"),
        "verbatim, because it is the attempt's own account: {json}"
    );
}

#[tokio::test]
async fn a_declined_finding_reads_differently_from_one_the_attempt_worked_on() {
    let mut run = one_fixable_finding();
    run.attempted = vec![a_group_declining(
        FIXABLE_CVE,
        needs_work_group(FIXABLE_CVE).await,
    )];

    let json =
        serde_json::to_value(&disposition(&run).verdicts()[0]).expect("a verdict serializes");
    assert_eq!(
        json["attempted"],
        serde_json::json!(false),
        "nothing was tried for this one and the row has to say so: {json}"
    );
    assert!(
        json["note"]
            .as_str()
            .is_some_and(|it| !it.trim().is_empty()),
        "a declined finding carries the reason the attempt gave: {json}"
    );
    assert_eq!(
        json["verdict"],
        serde_json::json!("needs_work"),
        "declining is a verdict about the finding, not the model breaking its \
         contract: {json}"
    );
}

#[test]
fn a_verdict_for_a_finding_no_attempt_saw_carries_five_fields_and_a_verbatim_rationale() {
    let reached = disposition(&verdicts_only());
    let verdict = &reached.verdicts()[0];

    assert_eq!(
        verdict.rationale, NO_PUBLISHED_FIX,
        "verbatim: this is what a person reads in the ticket"
    );

    let json = serde_json::to_value(verdict).expect("a verdict serializes");
    assert_eq!(
        json.as_object()
            .expect("a verdict is a JSON object")
            .keys()
            .collect::<Vec<_>>(),
        ["cve", "package", "rationale", "severity", "verdict"]
    );
}

#[test]
fn each_of_the_five_fields_carries_what_it_names() {
    let reached = disposition(&verdicts_only());
    let json = serde_json::to_value(&reached.verdicts()[0]).expect("a verdict serializes");

    let finding = unfixed_finding_for(BLOCKED_CVE);
    assert_eq!(json["cve"], serde_json::json!(BLOCKED_CVE));
    assert_eq!(json["package"], serde_json::json!(finding.package));
    assert_eq!(json["rationale"], serde_json::json!(NO_PUBLISHED_FIX));
    assert_eq!(json["severity"], serde_json::json!("HIGH"));
    assert_eq!(json["verdict"], serde_json::json!("upstream_blocked"));
    assert_eq!(finding.severity, Severity::High, "the fixture's own grade");
}

#[tokio::test]
async fn a_needs_work_verdict_carries_the_status_s_own_words() {
    let world = every_group_needs_work().await;
    let expected: Vec<String> = world
        .attempted
        .iter()
        .map(|group| match &group.status {
            GroupStatus::NeedsWork { reason } => reason.to_string(),
            GroupStatus::Clean => panic!("this world's premise is that nothing ended clean"),
        })
        .collect();

    let reached = disposition(&world);
    let carried: Vec<String> = reached
        .verdicts()
        .iter()
        .map(|verdict| verdict.rationale.clone())
        .collect();

    assert_eq!(carried, expected);
    assert!(
        carried[0].contains("go vet ./..."),
        "the check's own command line is what an operator would type, got {}",
        carried[0]
    );
}

#[test]
fn the_report_is_written_even_when_empty() {
    assert_eq!(report_of(&both_sets_empty()), serde_json::json!([]));
}

#[test]
fn the_empty_report_reaches_the_disk() {
    let scratch = tempfile::tempdir().expect("a temporary directory");
    let path = disposition(&both_sets_empty())
        .write_report(scratch.path())
        .expect("the report is written");

    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some(REPORT_FILE)
    );
    let written = std::fs::read_to_string(&path).expect("the report is readable");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&written).expect("the report parses"),
        serde_json::json!([])
    );
}

#[test]
fn a_non_empty_report_reaches_the_disk_unchanged() {
    let scratch = tempfile::tempdir().expect("a temporary directory");
    let reached = disposition(&verdicts_only());
    let path = reached
        .write_report(scratch.path())
        .expect("the report is written");

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the report is readable"))
            .expect("the report parses");
    assert_eq!(written, reached.report());
    assert_eq!(written.as_array().expect("an array").len(), 1);
}

#[test]
fn a_written_verdict_deserializes_into_the_scanner_s_own_spellings() {
    let reached = disposition(&verdicts_only());
    let json = serde_json::to_value(&reached.verdicts()[0]).expect("a verdict serializes");

    assert_eq!(
        serde_json::from_value::<AdvisoryId>(json["cve"].clone()).expect("an advisory id"),
        advisory(BLOCKED_CVE)
    );
    assert_eq!(
        serde_json::from_value::<Severity>(json["severity"].clone()).expect("a severity"),
        Severity::High
    );
}

#[tokio::test]
async fn the_disposition_publishes_the_claim_beside_the_verdict_that_overruled_it() {
    let reached = disposition(&every_group_needs_work().await);

    assert_eq!(reached.attempts().len(), 2);
    assert!(
        reached.attempts()[0].claimed_complete,
        "the model said it had finished"
    );
    assert!(
        !reached.attempts()[1].claimed_complete,
        "and the second did not, so this lane is not reading a constant"
    );
    assert!(
        reached
            .attempts()
            .iter()
            .all(|record| record.status != GroupStatus::Clean),
        "and both were overruled, which is what makes the claim worth publishing"
    );
}

#[tokio::test]
async fn the_claim_changes_no_part_of_the_disposition_but_the_record_of_it() {
    let mut said_so = every_group_needs_work().await;
    let mut did_not = every_group_needs_work().await;
    for group in &mut said_so.attempted {
        group.attempt.report.claimed_complete = true;
    }
    for group in &mut did_not.attempted {
        group.attempt.report.claimed_complete = false;
    }

    let with = disposition(&said_so);
    let without = disposition(&did_not);

    assert_eq!(with.reason(), without.reason());
    assert_eq!(with.outcome(), without.outcome());
    assert_eq!(with.verdicts(), without.verdicts());
    assert_ne!(
        with.attempts(),
        without.attempts(),
        "the two worlds do differ, so the equalities above are not vacuous"
    );
}

#[tokio::test]
async fn every_forbidden_shape_reaches_the_record_in_path_order() {
    let shapes = vec![
        ForbiddenShape::ReplaceDirective {
            path: "a/go.mod".to_string(),
            directive: "replace example.com/x => ../x".to_string(),
        },
        ForbiddenShape::AddedSkip {
            path: "b/main_test.go".to_string(),
            line: "\tt.Skip(\"flaky\")".to_string(),
        },
        ForbiddenShape::NewControlFlow {
            path: "c/main.go".to_string(),
            keyword: "if",
            before: 1,
            after: 3,
        },
    ];

    let mut run = one_fixable_finding();
    run.attempted = vec![attempted_group(
        FIXABLE_CVE,
        GroupStatus::of(&cleanly_evaluated(FIXABLE_CVE).await, &shapes, None),
        true,
        shapes.clone(),
    )];

    let reached = disposition(&run);

    assert_eq!(
        reached.attempts()[0].forbidden,
        shapes,
        "all of them, in the order `classify` found them"
    );
    assert_eq!(
        reached.attempts()[0].status,
        GroupStatus::NeedsWork {
            reason: NeedsWork::OutOfScope(shapes[0].clone()),
        },
        "and the status still names only the first, which is Task 14.b's rule"
    );
    assert!(
        reached.verdicts()[0].rationale.contains("a/go.mod"),
        "the verdict reports the shape that decided the group, got {}",
        reached.verdicts()[0].rationale
    );
}

#[tokio::test]
async fn the_report_holds_only_what_this_run_could_not_patch() {
    let mut run = findings_beyond_budget(6, 5).await;
    run.already_fixed = vec![advisory("CVE-2026-9999")];
    let reached = disposition(&run);

    assert!(
        reached.verdicts().is_empty(),
        "every group this run attempted ended clean, so it could not patch nothing"
    );
    assert_eq!(reached.deferred().len(), 1);
    assert_eq!(reached.already_fixed().len(), 1);
    assert_eq!(report_of(&run), serde_json::json!([]));
}

#[test]
fn every_advisory_with_no_published_fix_gets_its_own_verdict() {
    let run = Run::scanned(projection_of(document_of(&report_with(
        unfixed_libraries(&[BLOCKED_CVE, SECOND_CVE]),
        os_packages(&[]),
    ))));
    assert_eq!(
        run.projection()
            .expect("a scan that produced a projection")
            .upstream_blocked()
            .count(),
        2,
        "this world's premise is two findings the scanner published no fix for"
    );

    let reached = disposition(&run);
    let reported: Vec<&AdvisoryId> = reached
        .verdicts()
        .iter()
        .map(|verdict: &Verdict| &verdict.cve)
        .collect();

    assert_eq!(
        reported,
        vec![&advisory(BLOCKED_CVE), &advisory(SECOND_CVE)]
    );
}
