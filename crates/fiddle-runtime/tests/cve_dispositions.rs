mod support;

use fiddle_core::{AdvisoryId, ProjectedFinding, RunOutcome, Severity};
use fiddle_runtime::agent::{FindingDisposition, RepairReport};
use fiddle_runtime::capability::{GroupStatus, MigrationAttempt, NeedsWork};
use fiddle_runtime::cve::project::{project, Projection};
use fiddle_runtime::cve::verdict::{
    disposition, report_of, Attempted, BoundReached, Budget, InProgress, Judgement, Landed, Row,
    Run, Verdict, FINDINGS_FILE, REPORT_FILE,
};
use fiddle_runtime::evaluate::{evaluate, Evaluation, RescanVerdict};
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

const DECLINED: &str = "no fix I can apply to this project without reading a registry";

const SETTLED: &str = "the requirement already resolves to the fixed release";

const CHECKS_REFUSED: &str =
    "github:acme/r/commits/c0ffee/check-runs: the check runs could not be read: HTTP 403";

fn still_reported(cve: &str) -> String {
    format!("still reported after the bump: {cve}")
}

fn projection_of(document: serde_json::Value) -> Projection {
    project(&scan_of(document), &every_fixture_grade())
        .expect("a fixture document this build can project")
}

fn clean_scan_no_findings() -> Run {
    let run = Run::scanned(projection_of(document_of(&report_with(
        libraries(&[]),
        os_packages(&[]),
    ))));
    assert_eq!(
        run.projection()
            .expect("a scan that produced a projection")
            .all()
            .count(),
        0,
        "this world's premise is a projection that holds nothing"
    );
    run
}

fn one_finding_with_no_published_fix() -> Run {
    let run = Run::scanned(projection_of(document_of(&report_with(
        unfixed_libraries(&[BLOCKED_CVE]),
        os_packages(&[]),
    ))));
    let projected: Vec<&ProjectedFinding> = run
        .projection()
        .expect("a scan that produced a projection")
        .all()
        .collect();
    assert_eq!(projected.len(), 1);
    assert_eq!(
        projected[0].fixed_version, None,
        "this world's premise is one finding the scanner published no fix for, \
         and the projection carries it to the attempt rather than deciding it"
    );
    run
}

async fn the_attempt_declined_a_finding_with_no_published_fix() -> Run {
    let mut run = one_finding_with_no_published_fix();
    run.attempted = vec![a_group_declining_an_unfixed_finding(
        BLOCKED_CVE,
        still_reported_group(BLOCKED_CVE).await,
    )];
    run
}

async fn the_attempt_said_nothing_about_a_finding_with_no_published_fix() -> Run {
    let mut run = one_finding_with_no_published_fix();
    let mut group =
        a_group_declining_an_unfixed_finding(BLOCKED_CVE, still_reported_group(BLOCKED_CVE).await);
    group.attempt.report.findings = Vec::new();
    run.attempted = vec![group];
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

fn every_row() -> Vec<Row> {
    vec![
        Row::NothingToDo,
        Row::AlreadyInProgress,
        Row::AlreadyFixed,
        Row::PullRequest,
        Row::UnsafeWithoutDirection,
        Row::AttemptBoundReached,
        Row::ScanUnusable { why: String::new() },
        Row::ChecksUnreadable { why: String::new() },
    ]
}

fn label_the_host_closes(row: &Row) -> Option<&'static str> {
    match row {
        Row::PullRequest => Some("needs-work"),
        Row::UnsafeWithoutDirection => Some("upstream-blocked"),
        Row::NothingToDo
        | Row::AlreadyInProgress
        | Row::AlreadyFixed
        | Row::AttemptBoundReached
        | Row::ScanUnusable { .. }
        | Row::ChecksUnreadable { .. } => None,
    }
}

fn the_check_read_was_refused() -> Run {
    let mut run = one_fixable_finding();
    run.checks_unreadable = Some(CHECKS_REFUSED.to_string());
    run
}

fn a_reused_pull_request_at_the_bound() -> Run {
    let mut run = one_fixable_finding();
    run.bound_reached = Some(BoundReached {
        number: 9,
        spent: 4,
        bound: 4,
    });
    run
}

fn fixed_in_the_tree() -> Run {
    let mut run = one_fixable_finding();
    run.already_fixed = vec![advisory(FIXABLE_CVE)];
    run
}

async fn the_attempt_found_the_tree_already_at_the_fix() -> Run {
    let mut run = one_fixable_finding();
    run.already_fixed = vec![advisory(FIXABLE_CVE)];
    run.attempted = vec![a_settled_group(FIXABLE_CVE).await];
    run
}

async fn a_settled_group(cve: &str) -> Attempted {
    let mut group = attempted_group(cve, clean_group(cve).await, true);
    group.attempt.report.changed_files = Vec::new();
    group.attempt.report.summary = format!("{cve} needed nothing");
    group.attempt.report.findings = vec![FindingDisposition {
        cve: cve.to_string(),
        attempted: true,
        note: SETTLED.to_string(),
    }];
    group.attempt.changed = Vec::new();
    assert!(
        group.settled(),
        "this group's premise is a clean evaluation over a tree the attempt did \
         not change"
    );
    group
}

async fn one_group_clean() -> Run {
    let mut run = two_fixable_findings();
    run.attempted = vec![
        attempted_group(FIXABLE_CVE, clean_group(FIXABLE_CVE).await, true),
        attempted_group(SECOND_CVE, needs_work_group(SECOND_CVE).await, true),
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
        attempted_group(FIXABLE_CVE, needs_work_group(FIXABLE_CVE).await, true),
        attempted_group(SECOND_CVE, needs_work_group(SECOND_CVE).await, false),
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

    let projected: Vec<ProjectedFinding> = run
        .projection()
        .expect("a scan that produced a projection")
        .all()
        .cloned()
        .collect();
    assert_eq!(
        projected.len(),
        count,
        "this world's premise is {count} projected findings"
    );

    let (taken, deferred) = Budget::of(bound).apply(projected);
    let mut attempted = Vec::new();
    for finding in &taken {
        attempted.push(attempted_group(
            finding.cve.as_str(),
            clean_group(finding.cve.as_str()).await,
            true,
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
            .all()
            .count(),
        1,
        "this world's premise is one finding that names a fix"
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
    let status = GroupStatus::of(&cleanly_evaluated(cve).await, None, None);
    assert_eq!(
        status,
        GroupStatus::Clean,
        "this group's premise is an evaluation Task 14.b calls clean"
    );
    status
}

async fn still_reported_group(cve: &str) -> GroupStatus {
    let evaluation = evaluate(&contract_for(&[cve]), &tree_whose_rescan_reports(&[cve]))
        .await
        .expect("an evaluation that was not cancelled");
    let status = GroupStatus::of(&evaluation, None, None);
    assert!(
        matches!(
            &status,
            GroupStatus::NeedsWork {
                reason: NeedsWork::Unproved(RescanVerdict::StillReported(cves)),
            } if cves.iter().any(|reported| reported.as_str() == cve)
        ),
        "this group's premise is a rescan that still names {cve}, got {status:?}"
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
    let status = GroupStatus::of(&evaluation, None, None);
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

fn attempted_group(cve: &str, status: GroupStatus, claimed_complete: bool) -> Attempted {
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
                direction: None,
            },
            changed: vec![WorkspacePath::parse("go.mod").expect("a workspace-relative path")],
            undeclared: None,
        },
    }
}

fn a_group_declining(cve: &str, status: GroupStatus) -> Attempted {
    let mut group = attempted_group(cve, status, false);
    group.attempt.report.changed_files = Vec::new();
    group.attempt.changed = Vec::new();
    group.attempt.report.findings = vec![FindingDisposition {
        cve: cve.to_string(),
        attempted: false,
        note: DECLINED.to_string(),
    }];
    group
}

fn a_group_declining_an_unfixed_finding(cve: &str, status: GroupStatus) -> Attempted {
    let mut group = a_group_declining(cve, status);
    group.findings = vec![unfixed_finding_for(cve)];
    group
}

fn unfixed_finding_for(cve: &str) -> ProjectedFinding {
    projection_of(document_of(&report_with(
        unfixed_libraries(&[cve]),
        os_packages(&[]),
    )))
    .all()
    .next()
    .cloned()
    .expect("a fixture document with one finding the scanner published no fix for")
}

fn finding_for(cve: &str) -> ProjectedFinding {
    projection_of(document_of(&report_with(
        libraries(&[cve]),
        os_packages(&[]),
    )))
    .all()
    .next()
    .cloned()
    .expect("a fixture document with one finding that names a fix")
}

fn advisory(cve: &str) -> AdvisoryId {
    AdvisoryId::parse(cve).expect("a fixture advisory id parses")
}

fn document_of(report: &support::cve::Report) -> serde_json::Value {
    support::cve::document_of(report)
}

#[tokio::test]
async fn every_cause_reaches_a_distinguishable_result() {
    let cases: Vec<(&str, Run, RunOutcome, Row)> = vec![
        (
            "nothing projected",
            clean_scan_no_findings(),
            RunOutcome::Completed,
            Row::NothingToDo,
        ),
        (
            "an open pull request covers it",
            open_pr_covers_it(),
            RunOutcome::Completed,
            Row::AlreadyInProgress,
        ),
        (
            "already fixed in the tree",
            fixed_in_the_tree(),
            RunOutcome::Completed,
            Row::AlreadyFixed,
        ),
        (
            "one group clean",
            one_group_clean().await,
            RunOutcome::Completed,
            Row::PullRequest,
        ),
        (
            "every group needs work",
            every_group_needs_work().await,
            RunOutcome::Completed,
            Row::UnsafeWithoutDirection,
        ),
        (
            "the scanner never ran",
            scanner_unusable().await,
            RunOutcome::Retryable {
                reason: fiddle_core::Published::of("ignored: the discriminant is the claim"),
            },
            Row::ScanUnusable { why: String::new() },
        ),
        (
            "the reused pull request is at the bound",
            a_reused_pull_request_at_the_bound(),
            RunOutcome::Completed,
            Row::AttemptBoundReached,
        ),
        (
            "the check runs could not be read",
            the_check_read_was_refused(),
            RunOutcome::Retryable {
                reason: fiddle_core::Published::of("ignored: the discriminant is the claim"),
            },
            Row::ChecksUnreadable { why: String::new() },
        ),
    ];
    assert_eq!(
        cases.len(),
        every_row().len(),
        "every row a run can reach needs a cause here, or a row reaches nothing"
    );

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
    assert_eq!(seen.len(), 8);
}

#[tokio::test]
async fn every_cause_publishes_a_distinguishable_record() {
    let cases: Vec<(&str, Run, &str)> = vec![
        (
            "nothing projected",
            clean_scan_no_findings(),
            "nothing_to_do",
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
        (
            "the reused pull request is at the bound",
            a_reused_pull_request_at_the_bound(),
            "attempt_bound_reached",
        ),
        (
            "the check runs could not be read",
            the_check_read_was_refused(),
            "checks_unreadable",
        ),
    ];
    assert_eq!(
        cases.len(),
        every_row().len(),
        "every row a run can reach needs a cause here, or a row publishes nothing"
    );

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

        if matches!(
            reached.reason(),
            Row::ScanUnusable { .. } | Row::ChecksUnreadable { .. }
        ) {
            assert!(
                matches!(reached.outcome(), RunOutcome::Retryable { .. }),
                "and it is a row the exit code separates: {:?}",
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
    assert_eq!(published.len(), 8);
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

    let Row::ScanUnusable { why } = reached.reason() else {
        panic!("got {:?}", reached.reason());
    };
    assert!(why.contains("-which-is-not-installed"), "got {why}");
}

#[test]
fn a_refused_check_read_is_retryable_and_names_the_endpoint_it_could_not_read() {
    let reached = disposition(&the_check_read_was_refused());

    let RunOutcome::Retryable { reason } = reached.outcome() else {
        panic!(
            "a run that could not look has not completed — got {:?}",
            reached.outcome()
        );
    };
    assert!(
        reason.as_str().contains("check-runs"),
        "the outcome should name the read that was refused, got {reason:?}"
    );

    let Row::ChecksUnreadable { why } = reached.reason() else {
        panic!(
            "a refused read is its own row, not one of the rows a clean sweep \
             reaches — got {:?}",
            reached.reason()
        );
    };
    assert!(why.contains("403"), "got {why}");
}

#[test]
fn a_run_nothing_blamed_and_a_run_that_could_not_look_are_not_the_same_result() {
    let looked = disposition(&one_fixable_finding());
    let could_not = disposition(&the_check_read_was_refused());

    assert_eq!(
        looked.reason(),
        &Row::NothingToDo,
        "these two worlds differ only in whether the check read succeeded, and \
         this one read it"
    );
    assert_eq!(looked.outcome(), &RunOutcome::Completed);

    assert_ne!(
        discriminant(looked.reason()),
        discriminant(could_not.reason()),
        "a refused read must not reach the row a run that looked and found no \
         blame reaches"
    );
    assert_ne!(
        discriminant(looked.outcome()),
        discriminant(could_not.outcome()),
        "and it must not exit as that run exits, or nothing reports the refusal"
    );
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

#[tokio::test]
async fn an_attempt_that_declined_everything_touches_no_branch_and_no_pull_request() {
    let reached = disposition(&the_attempt_declined_a_finding_with_no_published_fix().await);

    assert!(!reached.verdicts().is_empty());
    assert!(
        reached.branch().is_none() && reached.pull_request().is_none(),
        "the attempt changed no file, so there is no change for a person to \
         judge and nothing to publish it on"
    );
    assert_ne!(
        disposition(&clean_scan_no_findings()).reason(),
        reached.reason()
    );
}

const UNPROVED_BRANCH: &str = "security/cve-unproved-20260817";

#[tokio::test]
async fn an_unproved_attempt_names_the_draft_a_person_has_to_judge() {
    let mut run = every_group_needs_work().await;
    run.judged = Some(Landed {
        branch: UNPROVED_BRANCH.to_string(),
        pull_request: 19,
    });

    let reached = disposition(&run);

    assert_eq!(
        reached.reason(),
        &Row::UnsafeWithoutDirection,
        "publishing the change does not make it a repair fiddle stands behind"
    );
    assert_eq!(reached.branch(), Some(UNPROVED_BRANCH));
    assert_eq!(
        reached.pull_request(),
        Some(19),
        "an operator reads the disposition and has to reach the diff from it"
    );
    assert_eq!(
        reached.verdicts()[0].legacy_label,
        Some("upstream-blocked"),
        "and the label the host's query closes is the one it was"
    );
    assert_eq!(
        disposition(&every_group_needs_work().await).pull_request(),
        None,
        "the same world that published nothing points at nothing, so the row \
         reads the publication and does not assume it"
    );
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

    assert_eq!(reached.reason(), &Row::NothingToDo);
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

    assert_eq!(awaiting.reason(), &Row::AlreadyInProgress);
    assert_eq!(landed.reason(), &Row::AlreadyFixed);
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
async fn a_settled_attempt_is_counted_and_still_reaches_already_fixed() {
    let reached = disposition(&the_attempt_found_the_tree_already_at_the_fix().await);

    assert_eq!(
        reached.reason(),
        &Row::AlreadyFixed,
        "an attempt that changed nothing is not a pull request and is not a run \
         needing direction"
    );
    assert_eq!(reached.already_fixed(), &[advisory(FIXABLE_CVE)]);
    assert_eq!(
        reached.attempts().len(),
        1,
        "the run consumed a completion and ran the checks, so the count of \
         attempts is one"
    );
    assert!(
        reached.attempts()[0].settled,
        "and the record says the tree needed nothing"
    );
    assert!(
        reached.verdicts().is_empty(),
        "nothing is unfixed, so nothing is reported unfixed"
    );
    assert_eq!(
        reached.branch(),
        None,
        "a settled attempt cuts no branch, so the row names none"
    );
    assert_eq!(reached.pull_request(), None, "and opens no pull request");

    let published = reached.published();
    assert_eq!(published.attempts[0].status, "settled");
    assert_eq!(
        published.attempts[0].dispositions,
        vec![fiddle_core::DisposedFinding {
            cve: advisory(FIXABLE_CVE),
            attempted: true,
            note: SETTLED.to_string(),
        }],
        "the note the agent wrote reaches the published surface on this path \
         too, and it is the only surface that carries it"
    );
}

#[tokio::test]
async fn a_finding_with_no_published_fix_reaches_the_attempt_and_carries_its_own_reason() {
    let reached = disposition(&the_attempt_declined_a_finding_with_no_published_fix().await);

    assert_eq!(
        reached.attempts().len(),
        1,
        "the finding reached an attempt rather than a verdict written before \
         one was opened"
    );
    assert_eq!(reached.attempts()[0].cves, vec![advisory(BLOCKED_CVE)]);

    let verdict = &reached.verdicts()[0];
    assert_eq!(
        verdict.verdict,
        Judgement::NeedsWork,
        "the judgement is the rescan's, and no projection wrote one before the \
         attempt opened"
    );

    let json = serde_json::to_value(verdict).expect("a verdict serializes");
    assert_eq!(
        json["attempted"],
        serde_json::json!(false),
        "nothing was tried for it, and the row says so from the attempt's own \
         report rather than from the projection: {json}"
    );
    assert_eq!(
        json["note"],
        serde_json::json!(DECLINED),
        "and the reason is the agent's, verbatim: {json}"
    );
    assert_eq!(
        json["rationale"],
        serde_json::json!(still_reported(BLOCKED_CVE)),
        "while the rationale is the rescan's own account of why the run is not \
         done: {json}"
    );
}

#[tokio::test]
async fn one_clean_group_beside_one_that_needs_work_still_opens_the_pull_request() {
    let reached = disposition(&one_group_clean().await);

    assert_eq!(reached.reason(), &Row::PullRequest);
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

    assert_eq!(reached.reason(), &Row::PullRequest);
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

#[tokio::test]
async fn a_verdict_the_attempt_said_nothing_about_carries_six_fields_and_a_verbatim_rationale() {
    let reached =
        disposition(&the_attempt_said_nothing_about_a_finding_with_no_published_fix().await);
    let verdict = &reached.verdicts()[0];

    assert_eq!(
        verdict.rationale, "still reported after the bump: CVE-2026-3002",
        "verbatim: this is what a person reads in the ticket"
    );

    let json = serde_json::to_value(verdict).expect("a verdict serializes");
    assert_eq!(
        json.as_object()
            .expect("a verdict is a JSON object")
            .keys()
            .collect::<Vec<_>>(),
        [
            "cve",
            "legacy_label",
            "package",
            "rationale",
            "severity",
            "verdict"
        ],
        "the attempt reported no disposition for it, so the row is the five \
         projection fields and the compatibility label, and nothing invented \
         beside them: {json}"
    );
}

#[tokio::test]
async fn each_field_of_a_verdict_carries_what_it_names() {
    let reached =
        disposition(&the_attempt_said_nothing_about_a_finding_with_no_published_fix().await);
    let json = serde_json::to_value(&reached.verdicts()[0]).expect("a verdict serializes");

    let finding = unfixed_finding_for(BLOCKED_CVE);
    assert_eq!(json["cve"], serde_json::json!(BLOCKED_CVE));
    assert_eq!(json["package"], serde_json::json!(finding.package));
    assert_eq!(
        json["rationale"],
        serde_json::json!("still reported after the bump: CVE-2026-3002")
    );
    assert_eq!(json["severity"], serde_json::json!("HIGH"));
    assert_eq!(json["verdict"], serde_json::json!("needs_work"));
    assert_eq!(json["legacy_label"], serde_json::json!("upstream-blocked"));
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
            GroupStatus::Clean | GroupStatus::Directed { .. } => {
                panic!("this world's premise is that nothing ended clean")
            }
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
    assert_eq!(report_of(&clean_scan_no_findings()), serde_json::json!([]));
}

#[test]
fn the_empty_report_reaches_the_disk() {
    let scratch = tempfile::tempdir().expect("a temporary directory");
    let path = disposition(&clean_scan_no_findings())
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

#[tokio::test]
async fn a_non_empty_report_reaches_the_disk_unchanged() {
    let scratch = tempfile::tempdir().expect("a temporary directory");
    let reached = disposition(&the_attempt_declined_a_finding_with_no_published_fix().await);
    let path = reached
        .write_report(scratch.path())
        .expect("the report is written");

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the report is readable"))
            .expect("the report parses");
    assert_eq!(written, reached.report());
    assert_eq!(written.as_array().expect("an array").len(), 1);
}

#[tokio::test]
async fn a_written_verdict_deserializes_into_the_scanner_s_own_spellings() {
    let reached = disposition(&the_attempt_declined_a_finding_with_no_published_fix().await);
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

#[tokio::test]
async fn every_advisory_with_no_published_fix_gets_its_own_verdict() {
    let mut run = Run::scanned(projection_of(document_of(&report_with(
        unfixed_libraries(&[BLOCKED_CVE, SECOND_CVE]),
        os_packages(&[]),
    ))));
    assert_eq!(
        run.projection()
            .expect("a scan that produced a projection")
            .all()
            .count(),
        2,
        "this world's premise is two findings the scanner published no fix for"
    );
    run.attempted = vec![
        a_group_declining_an_unfixed_finding(BLOCKED_CVE, still_reported_group(BLOCKED_CVE).await),
        a_group_declining_an_unfixed_finding(SECOND_CVE, still_reported_group(SECOND_CVE).await),
    ];

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
    for verdict in reached.verdicts() {
        let disposed = verdict
            .disposed
            .as_ref()
            .expect("the attempt reported a disposition for every finding it saw");
        assert!(
            !disposed.attempted,
            "each row is the attempt's own decline rather than one row standing \
             for both: {verdict:?}"
        );
    }
}

#[test]
fn every_row_answers_whether_it_carries_a_legacy_label() {
    let rows = every_row();
    assert_eq!(
        rows.len(),
        8,
        "the eight rows, and every one of them answers"
    );

    for row in &rows {
        let label = label_the_host_closes(row);
        assert_eq!(
            row.legacy_label(),
            label,
            "{} should carry {label:?}, got {:?}",
            row.row(),
            row.legacy_label()
        );
    }

    let carried: HashSet<&str> = rows.iter().filter_map(Row::legacy_label).collect();
    assert_eq!(
        carried,
        HashSet::from(["needs-work", "upstream-blocked"]),
        "the host's query closes these two labels and no other, so a row that \
         carried a third would file a ticket nothing closes"
    );
}

#[tokio::test]
async fn a_declined_finding_carries_fiddle_s_disposition_and_the_label_beside_it() {
    let reached = disposition(&the_attempt_declined_a_finding_with_no_published_fix().await);
    assert_eq!(
        reached.reason(),
        &Row::UnsafeWithoutDirection,
        "this world's premise is the row a finding with no published fix now \
         travels"
    );

    let json = serde_json::to_value(&reached.verdicts()[0]).expect("a verdict serializes");
    assert_eq!(
        json["verdict"],
        serde_json::json!("needs_work"),
        "fiddle has no upstream-blocked judgement, and the disposition is its \
         own: {json}"
    );
    assert_eq!(
        json["legacy_label"],
        serde_json::json!("upstream-blocked"),
        "while the label beside it is the one the host's query closes: {json}"
    );
    assert_ne!(
        json["verdict"], json["legacy_label"],
        "two fields, so dropping the label when M5 owns Jira removes a field \
         rather than unpicks a string: {json}"
    );
}

#[tokio::test]
async fn a_group_needing_work_beside_a_landed_one_carries_the_other_label() {
    let reached = disposition(&one_group_clean().await);
    assert_eq!(reached.reason(), &Row::PullRequest);

    let json = serde_json::to_value(&reached.verdicts()[0]).expect("a verdict serializes");
    assert_eq!(
        json["legacy_label"],
        serde_json::json!("needs-work"),
        "the run opened a pull request for the other group, and this finding \
         still needs work: {json}"
    );
    assert_ne!(
        json["legacy_label"],
        serde_json::to_value(&disposition(&every_group_needs_work().await).verdicts()[0])
            .expect("a verdict serializes")["legacy_label"],
        "the two rows that carry verdicts carry two labels, so the label is \
         read from the row and not written once"
    );
}

#[tokio::test]
async fn the_label_reaches_the_report_on_the_disk() {
    let scratch = tempfile::tempdir().expect("a temporary directory");
    let path = disposition(&the_attempt_declined_a_finding_with_no_published_fix().await)
        .write_report(scratch.path())
        .expect("the report is written");

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the report is readable"))
            .expect("the report parses");
    assert_eq!(
        written[0]["legacy_label"],
        serde_json::json!("upstream-blocked"),
        "write_report is the only writer, so the label a host reads has to be \
         in the file it wrote: {written}"
    );
}

#[tokio::test]
async fn the_sentence_a_ticket_shows_for_a_declined_finding_is_the_agent_s_own() {
    let notes = [
        "this ecosystem pins the version in a lockfile I will not resolve here",
        "the maintainer withdrew the release and I will not pin a withdrawn one",
    ];

    let mut carried = Vec::new();
    for note in &notes {
        let mut run = one_finding_with_no_published_fix();
        let mut group = a_group_declining_an_unfixed_finding(
            BLOCKED_CVE,
            still_reported_group(BLOCKED_CVE).await,
        );
        group.attempt.report.findings = vec![FindingDisposition {
            cve: BLOCKED_CVE.to_string(),
            attempted: false,
            note: (*note).to_string(),
        }];
        run.attempted = vec![group];

        let json =
            serde_json::to_value(&disposition(&run).verdicts()[0]).expect("a verdict serializes");
        assert!(
            !json.to_string().contains("no fixed version"),
            "the deleted constant must not return as the sentence an operator \
             reads: {json}"
        );
        carried.push(json["note"].clone());
    }

    assert_eq!(
        carried,
        notes
            .iter()
            .map(|note| serde_json::json!(note))
            .collect::<Vec<_>>(),
        "each note is the agent's, verbatim"
    );
    assert_ne!(
        carried[0], carried[1],
        "two agents wrote two sentences, so no fixed string in this build can \
         be the source of either"
    );
}

fn published_findings(reached: &fiddle_runtime::cve::verdict::Disposition) -> serde_json::Value {
    serde_json::to_value(reached.findings()).expect("the complete findings serialize")
}

fn findings_on_the_disk(reached: &fiddle_runtime::cve::verdict::Disposition) -> serde_json::Value {
    let scratch = tempfile::tempdir().expect("a temporary directory");
    let path = reached
        .write_findings(scratch.path())
        .expect("the findings are written");
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some(FINDINGS_FILE),
        "the complete findings sit beside {REPORT_FILE}, not inside it"
    );
    serde_json::from_str(&std::fs::read_to_string(&path).expect("the findings are readable"))
        .expect("the findings parse")
}

#[tokio::test]
async fn a_scan_the_bound_cut_down_still_publishes_every_finding_it_projected() {
    let reached = disposition(&findings_beyond_budget(3, 1).await);

    assert_eq!(
        reached.deferred().len(),
        2,
        "this world's premise is a bound that cut the scan down"
    );
    assert_eq!(reached.projected(), Some(3));

    let published = published_findings(&reached);
    assert_eq!(published["scanned"]["projected"], 3);
    assert_eq!(
        published["scanned"]["findings"].as_array().map(Vec::len),
        Some(3),
        "the host's feed reports every finding still in the build, so the \
         bounded set the agent worked on cannot be the whole answer: {published}"
    );
    assert_eq!(
        findings_on_the_disk(&reached),
        published,
        "the file a host reads is the document this build published"
    );
}

#[tokio::test]
async fn a_scan_within_the_bound_publishes_the_same_findings_and_defers_none() {
    let reached = disposition(&findings_beyond_budget(3, 3).await);

    assert_eq!(
        reached.deferred(),
        &[],
        "this world differs from the one above in the bound alone, and this \
         bound cut nothing"
    );
    assert_eq!(reached.projected(), Some(3));

    let published = published_findings(&reached);
    assert_eq!(published["scanned"]["projected"], 3);
    assert_eq!(
        published["scanned"]["findings"].as_array().map(Vec::len),
        Some(3),
        "publishing the deferred remainder alone would answer 0 here: {published}"
    );
    assert_eq!(findings_on_the_disk(&reached), published);
}

#[tokio::test]
async fn the_published_count_is_the_length_of_the_published_list() {
    for run in [
        findings_beyond_budget(3, 1).await,
        findings_beyond_budget(3, 3).await,
        clean_scan_no_findings(),
    ] {
        let published = published_findings(&disposition(&run));
        assert_eq!(
            published["scanned"]["projected"],
            serde_json::json!(published["scanned"]["findings"]
                .as_array()
                .map(Vec::len)
                .expect("a scanned document carries a list")),
            "a count that disagrees with the list beside it is worse than no \
             count: {published}"
        );
    }
}

#[test]
fn a_scan_that_found_nothing_is_not_a_scan_that_did_not_run() {
    let empty = published_findings(&disposition(&clean_scan_no_findings()));

    assert_eq!(empty["scanned"]["projected"], 0);
    assert_eq!(empty["scanned"]["findings"], serde_json::json!([]));
    assert!(
        empty.get("unusable").is_none(),
        "a scan that ran and found nothing names no failure: {empty}"
    );
    assert_eq!(
        disposition(&clean_scan_no_findings()).projected(),
        Some(0),
        "zero is an answer this run can stand behind"
    );
}

#[tokio::test]
async fn a_scan_that_did_not_run_publishes_no_list_and_no_count() {
    let reached = disposition(&scanner_unusable().await);
    let unusable = published_findings(&reached);

    assert_eq!(reached.projected(), None);
    assert!(
        unusable.get("scanned").is_none(),
        "a run that never read a document must not publish a list of what it \
         holds: {unusable}"
    );
    assert!(
        unusable["unusable"]["why"]
            .as_str()
            .is_some_and(|why| why.contains("-which-is-not-installed")),
        "and it says why the scan produced no answer: {unusable}"
    );
    assert_ne!(
        unusable,
        published_findings(&disposition(&clean_scan_no_findings())),
        "an absent list and an empty list are two answers, and a feed that \
         reads them as one reports a clean image for a scan that never ran"
    );
    assert_eq!(findings_on_the_disk(&reached), unusable);
}

#[test]
fn the_published_findings_carry_the_array_arms_the_document_used() {
    let absent = published_findings(&disposition(&Run::scanned(projection_of(document_of(
        &support::cve::report_with_os_absent(),
    )))));
    let empty = published_findings(&disposition(&Run::scanned(projection_of(document_of(
        &support::cve::report_with_os_empty(),
    )))));

    assert_eq!(absent["scanned"]["os_packages"], "absent");
    assert_eq!(empty["scanned"]["os_packages"], "empty");
    assert_eq!(
        absent["scanned"]["libraries"], "present",
        "both documents report libraries, so the arm read is the OS one: {absent}"
    );
    assert_eq!(empty["scanned"]["libraries"], "present", "{empty}");
}

#[tokio::test]
async fn every_published_finding_reads_back_as_the_finding_the_projection_made() {
    let run = findings_beyond_budget(3, 1).await;
    let projected: Vec<ProjectedFinding> = run
        .projection()
        .expect("a scan that produced a projection")
        .all()
        .cloned()
        .collect();

    let published = findings_on_the_disk(&disposition(&run));
    let read: Vec<ProjectedFinding> =
        serde_json::from_value(published["scanned"]["findings"].clone())
            .expect("the published findings read back into the type that made them");

    assert_eq!(read, projected);
}

#[tokio::test]
async fn the_bundle_states_the_denominator_beside_the_bounded_count() {
    let over_the_bound = disposition(&findings_beyond_budget(3, 1).await).published();
    assert_eq!(over_the_bound.verdicts, 0);
    assert_eq!(
        over_the_bound.projected,
        Some(3),
        "a reader given the verdict count alone concludes the image has that \
         many problems"
    );

    let never_scanned = disposition(&scanner_unusable().await).published();
    assert_eq!(
        never_scanned.projected, None,
        "and a run that scanned nothing must not report a count of zero"
    );
}

#[test]
fn the_published_findings_are_the_grades_the_deployment_acts_on() {
    let acted_on = published_findings(&disposition(&Run::scanned(projection_of(document_of(
        &report_with(
            support::cve::libraries_graded(&[FIXABLE_CVE], "HIGH"),
            os_packages(&[]),
        ),
    )))));
    let below = published_findings(&disposition(&Run::scanned(projection_of(document_of(
        &report_with(
            support::cve::libraries_graded(&[FIXABLE_CVE], "MEDIUM"),
            os_packages(&[]),
        ),
    )))));

    assert_eq!(acted_on["scanned"]["projected"], 1);
    assert_eq!(
        below["scanned"]["projected"], 0,
        "this file publishes the projection whole, and the projection is the \
         grades the document named. A deployment that wants MEDIUM names it in \
         severities: {below}"
    );
}
