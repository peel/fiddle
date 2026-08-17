//! What a run came to, and the report a person reads afterwards.
//!
//! The subject is [`fiddle_runtime::cve::verdict`]. Design §3 is a six-row table
//! whose closing paragraph is the whole reason this suite is separate from every
//! other CVE lane: *an outcome two different causes produce identically is not
//! an assertion about either of them.* A scanner that never ran, a scan that
//! found nothing, a repository whose findings were all fixed last week and one
//! whose only fixable group needs a major version bump are four situations with
//! four different remedies. A run reporting `NoChange` for all four has recorded
//! none of them, and the record is all anybody has a week later — the worktree,
//! the image and the scan are gone.
//!
//! # Seven rows, not six
//!
//! Design §3's table has six rows and this suite asserts **seven** dispositions.
//! The seventh is `AlreadyInProgress`, which the PRD's own sequence names in the
//! same breath as the other two `NoMitigation` causes — *not affected, already
//! fixed, or already in progress* — and which Design §4's shared-PR model is
//! entirely about. It is a row of its own because the action it implies is not
//! the action `AlreadyFixed` implies: one says *go and merge #7* and the other
//! says nothing at all.
//!
//! # Why the distinctness set is not built out of `format!("{:?}")`
//!
//! [`seven_causes_reach_seven_distinguishable_results`] keys its set on
//! [`std::mem::discriminant`] of each half of the pair. A `Debug` rendering is
//! not a contract: it is derived, it changes when a field is added, and — the
//! reason that matters here — it renders *field values*, so two rows that
//! reached the same variant with different payloads would count as two, and the
//! assertion would report distinctness the type does not have. Both halves of
//! this pair carry payloads today: `Reason::ScanUnusable` carries its
//! diagnostic and `RunOutcome::Retryable` carries a [`Published`] reason. A
//! discriminant is exactly *which variant*, which is exactly the claim.
//!
//! # Every row also fails under a mutation its neighbours survive
//!
//! Asserting that seven worlds reach seven pairs shows the table is injective.
//! It does not show that any particular row is *load-bearing* — a table with a
//! spurious row would pass it just as well. So each row below carries, in its
//! own doc comment, the mutation to `disposition` that makes it fail while the
//! other six stay green. Those mutations were applied one at a time and the
//! suite run under each; the results are in the lane's report. They are recorded
//! here rather than only in a report because the point of a mutation is that
//! somebody can repeat it.

// The support module is compiled per test binary and this suite reaches a small
// part of it; see `tests/support/mod.rs`.
mod support;

use fiddle_core::{AdvisoryId, ProjectedFinding, RunOutcome, Severity};
use fiddle_runtime::agent::RepairReport;
use fiddle_runtime::capability::{ForbiddenShape, GroupStatus, MigrationAttempt, NeedsWork};
use fiddle_runtime::cve::group::{select_target_version, GroupError};
use fiddle_runtime::cve::project::{project, Projection};
use fiddle_runtime::cve::verdict::{
    disposition, report_of, Attempted, Blocked, Budget, InProgress, Judgement, Landed, Run,
    Verdict, REPORT_FILE,
};
use fiddle_runtime::evaluate::{evaluate, Evaluation, Reason};
use fiddle_runtime::scanner::Scanner;
use fiddle_runtime::workspace::WorkspacePath;
use std::collections::HashSet;
use std::mem::discriminant;
use support::cve::{
    absent_scanner, available, contract_for, exit, image, libraries, os_packages, report_with,
    scan_of, scanner_with, stdout, tree_whose_rescan_reports, unfixed_libraries, GO_VET,
};

// ---------------------------------------------------------------------------
// The worlds
// ---------------------------------------------------------------------------
//
// These are local to this suite rather than in `tests/support/cve.rs`, which is
// the precedent Tasks 17.a, 17.b and 18 each set and each stated: the shared
// module exists so two lanes cannot build differently-named copies of one
// fixture, and a builder with exactly one caller is not that. Every world here
// is composed out of the shared module's own pieces — its scanner documents, its
// check contracts, its scripted trees — so nothing about the tree, the document
// or the evaluation is spelled a second time. The entry in that file's per-task
// list says so.

/// The advisory the fixable worlds are about.
const FIXABLE_CVE: &str = "CVE-2026-3001";

/// The advisory the upstream-blocked world is about — a finding whose report
/// names no fix anywhere.
///
/// A different id from [`FIXABLE_CVE`] so that a world holding both is a world
/// with two distinguishable findings rather than one the projection folded.
const BLOCKED_CVE: &str = "CVE-2026-3002";

/// The advisory the second group in the two-group worlds is about.
const SECOND_CVE: &str = "CVE-2026-3003";

/// The projection a document produces.
///
/// Through [`project`] and a real [`ScanReport`], never assembled field by
/// field: the fixable and upstream-blocked sets are that module's judgement, and
/// a fixture that set them directly would let this suite disagree with the
/// projection about what "fixable" means and never find out.
fn projection_of(document: serde_json::Value) -> Projection {
    project(&scan_of(document)).expect("a fixture document this build can project")
}

/// **Design §3 row 1.** The scanner ran, and reported nothing at all.
///
/// The empty arrays are written rather than the keys omitted, because an absent
/// array and an empty one are different claims — see
/// [`fiddle_runtime::cve::project`]. This world is the second: the scanner
/// looked at both halves of the image and found nothing in either.
///
/// **Mutation that fails this row and no other:** write the already-fixed test
/// as *every selected finding is in the already-fixed set* instead of *the set
/// is non-empty*. It reads better and it is vacuously true of a scan that
/// reported nothing, so a clean image reports as one whose vulnerabilities
/// somebody had already dealt with. Every other world has a finding outside the
/// set, so every other row keeps its answer — which is exactly why nobody would
/// chase this one.
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

/// The same world, under §3 row 1's own wording.
///
/// Two names for one value, deliberately. Row 1's cause is *both sets empty* and
/// the contrast that
/// [`a_scanner_that_found_nothing_and_one_that_never_ran_are_not_the_same_result`]
/// draws is about *a scan that found nothing*; they are the same world seen from
/// the two sides that must not be confused, and giving them two constructions
/// would let them drift into two worlds while the prose still claimed one.
fn both_sets_empty() -> Run {
    clean_scan_no_findings()
}

/// **Design §3 row 2.** The fixable set is empty and there is still something to
/// report.
///
/// **Mutation that fails this row and no other:** delete the blocked-verdicts
/// test from the table, so a run with nothing to attempt falls through to
/// `NothingToDo`. `both_sets_empty` still answers `NothingToDo` correctly, which
/// is precisely the confusion this row exists to prevent.
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

/// **The seventh row.** The shared pull request already covers everything this
/// run would have done.
///
/// The covered set is what the pull request's *branch commits* say, never its
/// body — see [`fiddle_runtime::cve::dedup`] for the 2026-08-12 incident that
/// settled that. Nothing in this suite can tell the difference, and the type
/// records which of the two it is so that the caller filling it in cannot.
///
/// **Mutation that fails this row and no other:** delete the in-progress test
/// from the table. The run then falls through to `NothingToDo`, and a nightly
/// job reports *nothing to do* on a repository with an open unmerged security
/// fix.
fn open_pr_covers_it() -> Run {
    let mut run = one_fixable_finding();
    run.in_progress = Some(InProgress {
        number: 7,
        covers: vec![advisory(FIXABLE_CVE)],
    });
    run
}

/// **Design §3 row 3.** Every finding the scan reported was already dealt with.
///
/// **Mutation that fails this row and no other:** delete the already-fixed test
/// from the table, so the run falls through to `NothingToDo`. This is §3's own
/// warning in its most plausible form: the answer *looks* right, and the
/// evidence that the scan reported anything at all is gone.
fn fixed_in_the_tree() -> Run {
    let mut run = one_fixable_finding();
    run.already_fixed = vec![advisory(FIXABLE_CVE)];
    run
}

/// **Design §3 row 4.** One group ended clean — and one did not.
///
/// Two groups and not one, and that is the point of the world. Design §2.7: *a
/// needs-work group does not stop the run. Remaining groups still process and
/// clean ones still land.* A world with a single clean group would be satisfied
/// by a table that required **every** group to be clean, which is the wrong rule
/// and the easy one to write.
///
/// **Mutation that fails this row and no other:** change the clean test from
/// *any* group to *all* groups. `every_group_needs_work` still answers
/// `UnsafeWithoutDirection`, and every row above is untouched.
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

/// **Design §3 row 5.** Attempts ran and not one could be shown safe.
///
/// **Mutation that fails this row and no other:** collapse rows 5 and 2 by
/// answering `VerdictsOnly` whenever no group is clean. `verdicts_only` still
/// answers `VerdictsOnly` — correctly — so five of the six neighbours notice
/// nothing, which is what makes a collapse the mutation worth guarding against.
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

/// **Design §3 row 6.** The scanner is not installed.
///
/// A real [`fiddle_runtime::scanner::ScanError`] and not a string this file
/// wrote: the adapter is pointed at a path holding nothing, which is the only
/// way an absent program can be reached — there is no process left to script an
/// arm in. What the disposition carries is that error's own diagnostic, so the
/// sentence an operator reads is the one the adapter composed.
///
/// **Mutation that fails this row and no other:** answer `RunOutcome::Completed`
/// for an unusable scan. Every `NoChange` row is already `Completed`, so nothing
/// else in the suite moves — and the run exits 0, the host's nightly job goes
/// green, and a scanner that has been broken for a week reports as a clean
/// image.
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

/// A run whose scan reported `count` fixable findings under a budget of `bound`.
///
/// The split is [`Budget`]'s, not this file's. A fixture that decided which
/// findings were deferred would be asserting the bound against itself; here the
/// production selection is what produces the [`Deferred`] rows and the suite
/// only supplies the world.
///
/// [`Deferred`]: fiddle_runtime::cve::verdict::Deferred
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

/// A run whose one group cannot be moved to its fix without crossing a major.
///
/// The rationale is **not** a literal in this file. It is
/// [`select_target_version`]'s own refusal, rendered by
/// [`GroupError`]'s `Display` — Task 9 wrote that wording precisely so that a
/// verdict lane would read it, and a fixture that spelled the sentence out again
/// would keep passing on the day somebody reworded the error.
fn blocked_by_a_major_bump() -> Run {
    let error = select_target_version(&["2.0.0"], &available(&["1.4.0", "2.0.0"]), "1.4.0")
        .expect_err("a fix in the next major is a refusal");
    assert!(
        matches!(error, GroupError::MajorBump { .. }),
        "this world's premise is Task 9's major-bump refusal, got {error:?}"
    );

    let mut run = one_fixable_finding();
    let findings = fixable_findings(&run);
    run.blocked = vec![Blocked { findings, error }];
    run
}

// ---------------------------------------------------------------------------
// The pieces the worlds are built from
// ---------------------------------------------------------------------------

/// A run whose scan reported exactly one fixable finding and nothing else.
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

/// A run whose scan reported two fixable findings, one per group.
fn two_fixable_findings() -> Run {
    Run::scanned(projection_of(document_of(&report_with(
        libraries(&[FIXABLE_CVE, SECOND_CVE]),
        os_packages(&[]),
    ))))
}

/// Every fixable finding in `run`, cloned out of its projection.
fn fixable_findings(run: &Run) -> Vec<ProjectedFinding> {
    run.projection()
        .expect("a scan that produced a projection")
        .fixable()
        .cloned()
        .collect()
}

/// A group whose five checks passed and whose rescan cleared it.
///
/// A real [`Evaluation`] over a real check contract and a real scripted tree,
/// put to the real [`GroupStatus::of`]. Nothing here constructs a `Clean`
/// directly: that value is Task 14.b's judgement and a fixture that asserted it
/// into existence would let this suite pass against a rule that never says
/// `Clean` at all.
async fn clean_group(cve: &str) -> GroupStatus {
    let status = GroupStatus::of(&cleanly_evaluated(cve).await, &[]);
    assert_eq!(
        status,
        GroupStatus::Clean,
        "this group's premise is an evaluation Task 14.b calls clean"
    );
    status
}

/// A group `go vet` refused.
///
/// The check fails and the rescan still clears, which keeps the world's one
/// variable the check: a tree that also failed its rescan would reach
/// `NeedsWork` by two routes and the lane could not say which.
async fn needs_work_group(cve: &str) -> GroupStatus {
    let evaluation = evaluate(
        &contract_for(&[cve]),
        &tree_whose_rescan_reports(&[]).where_check(GO_VET, exit(1), stdout("")),
    )
    .await
    .expect("an evaluation that was not cancelled");
    let status = GroupStatus::of(&evaluation, &[]);
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

/// Five green checks over a rescan reporting nothing.
async fn cleanly_evaluated(cve: &str) -> Evaluation {
    evaluate(&contract_for(&[cve]), &tree_whose_rescan_reports(&[]))
        .await
        .expect("an evaluation that was not cancelled")
}

/// One attempted group, with the attempt record behind it.
///
/// The report is the shape a model's reply deserializes into, built here rather
/// than driven through a bounded run: what this suite is about is what a
/// *disposition* does with `claimed_complete` and with the forbidden list, and a
/// real Rig attempt would be Task 14's lane over again with the same two values
/// arriving at the end of it.
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
            },
            changed: vec![WorkspacePath::parse("go.mod").expect("a workspace-relative path")],
            forbidden,
        },
    }
}

/// One projected finding under `cve`, produced by the projection rather than
/// assembled.
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

/// The document a fixture report renders to.
fn document_of(report: &support::cve::Report) -> serde_json::Value {
    support::cve::document_of(report)
}

// ---------------------------------------------------------------------------
// m4-dispositions-pairwise-distinct
// ---------------------------------------------------------------------------

/// **Seven causes, seven pairs**, asserted with a set and a printed count.
///
/// The set is keyed on [`discriminant`] of each half rather than on a `Debug`
/// rendering — see this file's header for why the difference is not cosmetic.
///
/// The count at the end is what makes the set an assertion rather than a
/// decoration: without it, a table answering one pair for every world would fail
/// on the `insert` for row 2 and the failure would read as a duplicate rather
/// than as a table that decides nothing. With it, the two failures are still
/// distinguishable, and a case list somebody shortened is caught as well.
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

/// The scanner row is the one that is **not** `Completed`, and it names its own
/// remedy.
///
/// The pairwise lane above compares discriminants, which is the right claim
/// there and deliberately blind to what the `Retryable` carries. This is the
/// other half: an operator told only *retryable* has been told to repeat a run
/// without being told what to fix first, and Design §3 gives that row exit 11
/// precisely so a host can tell it from a clean night.
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

// ---------------------------------------------------------------------------
// m4-found-nothing-is-not-did-not-scan
// ---------------------------------------------------------------------------

/// A clean scan and an unusable scanner are not the same result.
///
/// The comparison is over the whole [`Disposition`], not only the reason, so a
/// table that told them apart in the reason and then gave them the same outcome
/// — which is the shape of the defect, since the outcome is what the exit code
/// comes from — still fails here.
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

/// **Design §3 row 2.** The run has real output and must not cut a branch it
/// will not use.
#[test]
fn an_empty_fixable_set_with_verdicts_touches_no_branch_and_no_pull_request() {
    let reached = disposition(&verdicts_only());

    assert!(!reached.verdicts().is_empty());
    assert!(reached.branch().is_none() && reached.pull_request().is_none());
    assert_ne!(disposition(&both_sets_empty()).reason(), reached.reason());
}

/// The clean row is the only one that names a branch.
///
/// A run that *observed* a shared pull request has a branch in hand and still
/// has not put anything on it, so [`Disposition::branch`] answering from what a
/// run was handed rather than from what it did would report work that never
/// landed. `open_pr_covers_it` is the world where the two readings differ.
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

/// The two rows that are both *the fix already exists* are told apart by where
/// it is.
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

/// A needs-work run and a nothing-to-attempt run are two rows, and collapsing
/// them is the mutation this asserts against.
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

/// **Design §2.7.** A needs-work group does not stop the run.
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

// ---------------------------------------------------------------------------
// m4-deferred-is-its-own-state
// ---------------------------------------------------------------------------

/// **Design §2.5.** A finding the budget did not reach is reported as deferred,
/// with the bound that deferred it, and is in neither of the other two sets.
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

/// The bound is a **selection**, so the run still reaches its ordinary
/// disposition.
///
/// The half of §2.5 a lane asserting only the deferred list would miss: a run
/// that deferred a finding still opened a pull request for the five it took, and
/// a table that let a non-empty deferred list change the row would report a
/// successful night as something else.
#[tokio::test]
async fn deferring_a_finding_does_not_change_what_the_run_came_to() {
    let reached = disposition(&findings_beyond_budget(6, 5).await);

    assert_eq!(reached.reason(), &Reason::PullRequest);
    assert_eq!(reached.outcome(), &RunOutcome::Completed);
    assert!(reached.branch().is_some());
}

/// A run inside its budget defers nothing, which is the positive control the
/// lane above needs.
#[tokio::test]
async fn a_run_inside_its_budget_defers_nothing() {
    let reached = disposition(&findings_beyond_budget(5, 5).await);
    assert!(reached.deferred().is_empty());
}

// ---------------------------------------------------------------------------
// m4-verdict-contract
// ---------------------------------------------------------------------------

/// Five fields, in the serialized order, and the rationale verbatim.
///
/// The literal in this test and [`GroupError::MajorBump`]'s `Display` are two
/// spellings of one sentence, and that is the arrangement: Task 9 chose the
/// wording so that this lane would find it, so a reword there fails here rather
/// than quietly changing what a person reads in the ticket.
#[test]
fn the_verdict_report_carries_five_fields_and_a_verbatim_rationale() {
    let reached = disposition(&blocked_by_a_major_bump());
    let verdict = &reached.verdicts()[0];

    assert_eq!(
        verdict.rationale, "requires a major version bump from 1 to 2",
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

/// Every field carries the value its name claims, not merely a key.
///
/// The lane above asserts the *shape*. A report whose five keys were all present
/// and all holding the wrong package would satisfy it, which is the vacuous half
/// this closes.
#[test]
fn each_of_the_five_fields_carries_what_it_names() {
    let reached = disposition(&blocked_by_a_major_bump());
    let json = serde_json::to_value(&reached.verdicts()[0]).expect("a verdict serializes");

    let finding = finding_for(FIXABLE_CVE);
    assert_eq!(json["cve"], serde_json::json!(FIXABLE_CVE));
    assert_eq!(json["package"], serde_json::json!(finding.package));
    assert_eq!(
        json["rationale"],
        serde_json::json!("requires a major version bump from 1 to 2")
    );
    assert_eq!(json["severity"], serde_json::json!("HIGH"));
    assert_eq!(json["verdict"], serde_json::json!("upstream_blocked"));
    assert_eq!(finding.severity, Severity::High, "the fixture's own grade");
}

/// The rationale is carried, not composed.
///
/// The strongest form available: the whole of `GroupError`'s rendering has to
/// appear as the whole of the rationale. A module that prefixed it, wrapped it
/// in a sentence or trimmed it would fail — and a paraphrase, which is the
/// mutation this is aimed at, fails on the equality above as well as here.
#[test]
fn the_rationale_is_the_upstream_value_s_own_words_and_nothing_around_them() {
    let error = select_target_version(&["2.0.0"], &available(&["1.4.0", "2.0.0"]), "1.4.0")
        .expect_err("a fix in the next major is a refusal");
    let reached = disposition(&blocked_by_a_major_bump());

    assert_eq!(reached.verdicts()[0].rationale, error.to_string());
}

/// A needs-work verdict carries [`NeedsWork`]'s own words the same way.
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

/// The report is written even when it is empty.
#[test]
fn the_report_is_written_even_when_empty() {
    assert_eq!(report_of(&both_sets_empty()), serde_json::json!([]));
}

/// And *written*, to a file a downstream consumer can open.
///
/// The lane above asserts the value; this asserts the artefact. A consumer that
/// had to tell *the file is absent* from *there was nothing to report* would be
/// distinguishing a broken run from a clean one by a missing file, and the
/// direction it gets wrong is the one that reads as success.
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

/// A non-empty report reaches the disk as the same array the value holds.
#[test]
fn a_non_empty_report_reaches_the_disk_unchanged() {
    let scratch = tempfile::tempdir().expect("a temporary directory");
    let reached = disposition(&blocked_by_a_major_bump());
    let path = reached
        .write_report(scratch.path())
        .expect("the report is written");

    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("the report is readable"))
            .expect("the report parses");
    assert_eq!(written, reached.report());
    assert_eq!(written.as_array().expect("an array").len(), 1);
}

/// The report round-trips: what one run writes, the next can read.
///
/// This is what the `Serialize` derives added to `AdvisoryId` and `Severity` are
/// for, and the assertion that they were added under the *same* spelling their
/// deserializers use. A report a consumer could not feed back in is a report
/// whose field values are decoration.
#[test]
fn a_written_verdict_deserializes_into_the_scanner_s_own_spellings() {
    let reached = disposition(&blocked_by_a_major_bump());
    let json = serde_json::to_value(&reached.verdicts()[0]).expect("a verdict serializes");

    assert_eq!(
        serde_json::from_value::<AdvisoryId>(json["cve"].clone()).expect("an advisory id"),
        advisory(FIXABLE_CVE)
    );
    assert_eq!(
        serde_json::from_value::<Severity>(json["severity"].clone()).expect("a severity"),
        Severity::High
    );
}

// ---------------------------------------------------------------------------
// Task 14.b's two obligations on this lane
// ---------------------------------------------------------------------------

/// The model's claim is published beside the verdict that overruled it.
///
/// Design §2.5: *`claimed_complete` is evidence beside the exit code that
/// overruled it and is branched on nowhere.* This lane is the *beside* half —
/// that the claim survives to the record. The *branched on nowhere* half is a
/// negative about the whole workspace and no test of one path can establish it;
/// `cve_protocol::nothing_in_this_workspace_decides_on_claimed_complete` walks
/// every source file for it.
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

/// The claim changes nothing about the row the run reaches.
///
/// The positive control the lane above needs: two worlds identical except for
/// the claim reach the same disposition. A `disposition` that consulted it would
/// pass every assertion above and fail here.
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

/// Every forbidden shape reaches the record, in path order.
///
/// Task 14.b's second obligation. `GroupStatus::of` takes only the first shape
/// for its *reason*, which is all a refusal needs; an operator fixing the group
/// by hand wants the list, and by the time they read it the worktree the shapes
/// were computed in is gone. A record carrying one of three would make *how much
/// is wrong here* unanswerable without re-running the attempt.
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
        GroupStatus::of(&cleanly_evaluated(FIXABLE_CVE).await, &shapes),
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

// ---------------------------------------------------------------------------
// The report as a whole
// ---------------------------------------------------------------------------

/// Nothing that was fixed, deferred or left to a pull request appears in the
/// report.
///
/// The report is *the CVEs this run could not patch*. A clean group's advisories
/// are patched, a deferred one's were never looked at, and an already-fixed one's
/// were dealt with before the run began — three different ways of not belonging
/// in it, and a report that carried any of them would send somebody to a ticket
/// for a CVE that is not open.
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

/// A verdict per advisory, not per group.
#[test]
fn a_blocked_group_produces_one_verdict_for_every_advisory_in_it() {
    let error = select_target_version(&["2.0.0"], &available(&["1.4.0", "2.0.0"]), "1.4.0")
        .expect_err("a fix in the next major is a refusal");
    let mut run = two_fixable_findings();
    let findings = fixable_findings(&run);
    assert_eq!(findings.len(), 2, "this world's premise is a group of two");
    run.blocked = vec![Blocked { findings, error }];

    let reached = disposition(&run);
    let reported: Vec<&AdvisoryId> = reached
        .verdicts()
        .iter()
        .map(|verdict: &Verdict| &verdict.cve)
        .collect();

    assert_eq!(
        reported,
        vec![&advisory(FIXABLE_CVE), &advisory(SECOND_CVE)]
    );
}
