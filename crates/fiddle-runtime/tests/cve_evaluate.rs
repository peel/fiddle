//! What decides whether a repair is allowed to stand: five checks, each run as
//! its own command and judged by the criterion its own declaration names.
//!
//! The subject is [`fiddle_runtime::evaluate`]. A repair attempt hands back a
//! tree, and something has to say whether that tree is better than the one it
//! started from. The design's §2.6 answer is five commands — `go build`,
//! `go fmt`, `go vet`, `docker build`, and a `wizcli` rescan — and this suite is
//! about the two ways that answer is easy to get wrong.
//!
//! # One exit code standing for five checks
//!
//! The cheap implementation is a shell line: `go build ./... && go fmt ./... &&
//! go vet ./...`, one spawn, one status. It is wrong in a way that only shows up
//! when something fails, which is the only time anybody is reading. `&&` stops
//! at the first failure, so a tree with a build error is never vetted and the
//! operator is told one thing about five. Worse, the aggregate cannot express
//! the second half of this suite at all: two of the five are not judged by their
//! exit status, and a chain has nowhere to put that. So each check runs
//! separately and each records a result of its own, *including the ones after a
//! failure* — [`each_check_records_its_own_result`] and
//! [`each_check_ran_as_its_own_command`] are the two halves of that, one about
//! what came back and one about what was actually started.
//!
//! # A criterion is declared, never recognised
//!
//! `gofmt -l` exits **zero** and prints the names of the files it would rewrite.
//! A runner reading exit statuses reports a green `go fmt` over a tree that is
//! not formatted. The fix is not to teach the runner about `go fmt`: Task 11's
//! [`Success`] exists precisely so that no code anywhere derives a criterion
//! from a program name, because an operator who pins a version, adds a wrapper
//! or renames a check would then have silently changed what that check
//! *decides*, and the failure would arrive as a green run that should have been
//! red.
//!
//! Two tests pin that from both sides, and they only mean something together.
//! [`a_wrapped_go_fmt_still_fails_on_output`] moves the program to a path with
//! no `go` and no `fmt` in it and keeps the verdict; and
//! [`a_check_spelled_go_fmt_but_declared_exit_zero_passes_on_output`] keeps the
//! spelling, changes the declaration, and gets the other verdict. Either alone
//! is satisfied by an implementation that gets the criterion from the wrong
//! place — the first by one that always demands no output, the second by one
//! that never does.
//!
//! # Five green checks are not the same claim as "this repair worked"
//!
//! The third part of the suite is about the second judgement: the two conditions
//! the *rescan's own document* has to satisfy, which no check's criterion reads.
//!
//! Condition (a) is that every advisory the group set out to clear is gone from
//! **both** package arrays, and
//! [`condition_a_checks_both_package_arrays`] is the half that matters —
//! a reader walking only `libraries` calls an image with a surviving
//! `osPackages` finding repaired, which is the same collapse
//! `cve_projection`'s `both_package_arrays_are_read` exists to prevent.
//!
//! Condition (b) is that nothing appeared that the input scan did not report,
//! and it needs a lane of its own because **the happy path never reaches it**:
//! on a clean rescan (a) already answers, so (b) is only ever exercised by a
//! bump that clears its own advisory and brings a new one —
//! [`condition_b_catches_a_bump_that_trades_one_vulnerability_for_another`].
//! Its opposite number is
//! [`a_finding_the_input_already_reported_is_not_a_new_one`], without which the
//! condition is satisfied by "the rescan must be empty", and that would refuse
//! every repair of an image carrying more than one group's findings.
//!
//! And [`a_rescan_at_a_different_scanner_version_is_provisional_not_proof`] is
//! about what those two answers are *worth*. A finding leaves a scan for two
//! reasons — the tree changed, or the feed moved — so an absence observed
//! through a different scanner version is not evidence about the tree. It is
//! paired with
//! [`a_repaired_tree_whose_rescan_clears_the_group_is_accepted`], which is the
//! only lane in this file that asserts the affirmative: without it, every
//! assertion here is satisfied by a runner that accepts nothing.
//!
//! # What the next group does with all of that
//!
//! The last family is [`fiddle_runtime::cve::fold`], and it is here rather than
//! in a suite of its own because it reads nothing but an [`Evaluation`]: a
//! rescan reports on the whole image, so one group's clean result can show that
//! a *later* group's advisories have gone too, and the rule that decides whether
//! to believe it is a rule about the verdicts above.
//!
//! Every lane in it but one asserts `Proceed`, which is the shape of the risk:
//! folding wrongly records advisories as fixed that nothing fixed, refusing
//! wrongly costs one redundant attempt.
//! [`a_group_cleared_by_an_earlier_committed_bump_is_recorded_without_a_file_change`]
//! is the positive control the refusals need — it is
//! [`a_tree_that_passes_every_check_is_not_rejected`]'s argument again, and
//! without it a rule that never folds satisfies the whole family.
//!
//! The three refusals are three different ways an absence arrives without a tree
//! having been repaired — a bump that is not on the branch, a feed that moved,
//! and an array nobody reported on — and
//! [`a_partially_cleared_group_proceeds`] is the fourth thing, about the rule
//! being over *every* id rather than any.
//!
//! [`Evaluation`]: fiddle_runtime::evaluate::Evaluation
//! [`Success`]: fiddle_runtime::evaluate::Success

mod support;

use fiddle_core::Severity;
use fiddle_runtime::cve::dedup::FixedInCommits;
use fiddle_runtime::cve::fold::{fold, fold_commit_argv, Fold};
use fiddle_runtime::evaluate::{evaluate, Outcome, Reason, RescanVerdict, Success};
use support::cve::*;

/// `gofmt -l` exits zero and names the files it would rewrite, so the status
/// line says "fine" about a tree that is not formatted. The printed filename is
/// the complaint, and this is the check that reads it.
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
    // Named, not merely counted: `rejected()` alone would also be true if some
    // *other* check had failed, and the contract has four more.
    assert_eq!(r.first_failure().expect("a failure").name, GO_FMT);
}

/// Five results, and each of them the result of its own check.
///
/// The count on its own is weak — any runner returning five things satisfies it
/// — so what carries the assertion is that the five *differ*: exactly one failed
/// and it is the one the tree was scripted to break. A single aggregate status
/// cannot produce that shape.
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

/// The other half of the same property, and the half the result list cannot
/// show: five results could be five copies of one status, so this asks the tree
/// what was actually started.
///
/// `go vet` fails and it is the *third* of the five. A runner chaining the
/// commands with `&&`, or stopping at the first failure, runs three and reports
/// three; this asserts all five ran, in the order the contract declares them.
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

/// The third criterion, and the one whose whole point is that the status line is
/// not the answer.
///
/// This goes through the real [`Wizcli`] adapter over the scripted scanner's
/// `exit-nonzero-with-file` arm — a scanner that exits 3 and writes its report
/// anyway, which is what `wizcli` does when it reports findings. The check
/// passes because the artefact is there. A runner that read the status would
/// fail it, and the run would revert a repair over the scanner having found
/// something in an unrelated layer.
///
/// [`Wizcli`]: fiddle_runtime::Wizcli
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
    // The premise: an arm that exited zero would make the assertion above hold
    // for the wrong reason, and this fixture's own suite pins that exit at 3.
    assert_eq!(arm_exits_with("exit-nonzero-with-file"), 3);
    assert!(
        matches!(&rescan.outcome, Outcome::Scanned(report) if !report.scanner_version.is_empty()),
        "a passing artefact check carries the report it read, not just a boolean"
    );
    assert!(!r.rejected());
}

/// And the same criterion refusing: the scanner ran, exited the same way, and
/// left nothing behind. There is no artefact, so there is no evidence, so the
/// check does not pass.
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

/// An operator pins the formatter to an absolute path behind a wrapper script.
/// Nothing about the command line says `go` or `fmt` any more. The check still
/// fails on output, because the criterion travelled with the declaration.
///
/// This is the regression Task 11's [`Success`] exists to prevent, asserted
/// against the runner rather than against the parser.
///
/// [`Success`]: fiddle_runtime::evaluate::Success
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

/// The inverse, and it is the half that catches the opposite mistake. The
/// command line still reads `go fmt ./...`; the declaration now says exit zero
/// is enough. The check passes while printing a filename.
///
/// Nobody would configure this. It is here because it is the only shape that
/// distinguishes "the criterion is read from the declaration" from "the runner
/// happens to demand no output from anything that looks like a formatter" —
/// and the test above is satisfied by both.
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

/// `first_failure` is the *first*, and with one failing check that claim is
/// vacuous — every ordering agrees. Two failures are what make it a claim.
///
/// `go fmt` is second and `docker build` is fourth, so a runner returning the
/// last failure, or whichever finished first, gives the other answer.
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

/// A check that could not be started at all did not pass — and is not recorded
/// as though the tree had failed it.
///
/// The two are opposite remedies. An uninstalled `docker` is an operator's
/// machine to fix; a failing `docker build` is the repair to revert. A runner
/// that wrote both into `passed: false` with a status of `-1` would let the
/// second be reported as the first, and the loop would throw away a correct
/// repair because a laptop had no daemon.
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

/// The positive control. Without it every assertion above is satisfied by a
/// runner that rejects everything, and `rejected()` would be measuring nothing.
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

// ---------------------------------------------------------------------------
// The rescan conditions
// ---------------------------------------------------------------------------

/// A bump that clears its own advisory and brings a new one.
///
/// **The lane condition (b) exists for, and the one the happy path can never
/// reach.** The group set out to clear `CVE-2026-1` and it is gone, so condition
/// (a) is satisfied and a runner asking only that question calls this tree
/// repaired. What the rescan actually reports is a `HIGH` that was not in the
/// input — a dependency bumped past its vulnerability into a different one — and
/// only condition (b) sees it.
///
/// `first_failure()` is asserted to be `None`, and that is what stops the
/// assertion passing for two reasons: all five checks were green, so the
/// refusal cannot have come from a command and must have come from the document.
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
    // Named, not merely matched: a variant with the wrong advisory in it would
    // satisfy `matches!` and would be reporting the finding the group fixed as
    // the one that appeared.
    match r.reason() {
        Some(Reason::NewFindingAppeared { cve, severity }) => {
            assert_eq!(cve.as_str(), "CVE-2026-NEW-HIGH");
            assert_eq!(*severity, Severity::High);
        }
        other => panic!("expected the new finding to be named, found {other:?}"),
    }
}

/// The other half of condition (b), and the half that keeps it honest.
///
/// Without it, "no finding appeared that was not in the input" is satisfied by a
/// rule that demands an *empty* rescan — and that rule refuses every real
/// repair, because an image carries more than one group's findings and a bump
/// that fixes one group leaves the rest exactly where they were. Here
/// `CVE-2026-OTHER` is somebody else's, it was in the input, and it is still
/// there afterwards. That is a repair that worked.
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

/// An advisory surviving in `osPackages` is not gone.
///
/// Condition (a) is *both* arrays, and a reader that walked only `libraries`
/// would find this tree clean — the same collapse `cve_projection`'s
/// `both_package_arrays_are_read` exists to prevent, asserted here against the
/// rule rather than against the projection. Its sibling below is the ordinary
/// half; either alone is satisfied by a reader that looks in one place.
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

/// The ordinary half of the pair above: the same advisory, in the array a reader
/// would think to look in.
///
/// It is not redundant. With only the `osPackages` lane, a condition (a) that
/// read `osPackages` *instead of* `libraries` — the same one-array bug, mirrored
/// — passes everything this suite asks.
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

/// A rescan at a different scanner version has not proved anything.
///
/// The tree's rescan is clean: both conditions hold, and a runner comparing only
/// findings accepts it. But an advisory leaves a scan for two different reasons
/// — the tree changed, or the feed moved — and the two are indistinguishable
/// from a report whose scanner is not the one the input was scanned with. So the
/// result is provisional, and provisional is **not** accepted.
///
/// It is not rejected either, and the pair of assertions is the point: nothing
/// went wrong with the tree, so reporting this as a failed repair would throw
/// the work away over a scanner upgrade. Both versions are asserted, because a
/// `Provisional` carrying two equal strings would be a variant nobody could act
/// on.
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

/// The affirmative, and the only lane in this file that asserts it.
///
/// The same world as the one above with **one thing changed**: the rescan ran at
/// the version the input was scanned at. Every check passed, the group's
/// advisory is gone, nothing appeared, and the same scanner said so twice — so
/// this is a repair that is proved rather than merely unrefuted.
///
/// Without it, every assertion in this file holds for a runner that accepts
/// nothing, and `accepted()` measures nothing. It is also the fix-evidence
/// assertion: hand this lane a tree that was never repaired — a rescan still
/// reporting the advisory the group set out to clear — and `accepted()` is
/// false.
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

// ---------------------------------------------------------------------------
// What the conditions were answered over
// ---------------------------------------------------------------------------

/// A rescan that never reported on `osPackages` has not proved the group clear.
///
/// **Both conditions above are satisfied by an absence, and an array the
/// document does not carry supplies absences for free.** The scanner did not say
/// the OS findings were gone; it said nothing about OS packages at all, and
/// reading that silence as clearance is the misfire this milestone exists to
/// refuse — the same shape as reading a CVE *mentioned* in a merged pull
/// request's body as a CVE that pull request *fixed*.
///
/// Every other question is arranged to hold, so the missing array is the only
/// thing that can be deciding this: the group's advisory is not reported, every
/// finding that is reported was in the input, the scanner version matches, and
/// all five checks are green. See [`contract_for_a_partially_reported_rescan`].
///
/// [`contract_for_a_partially_reported_rescan`]: support::cve::contract_for_a_partially_reported_rescan
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

/// The positive half, and the reason the rule above is not simply "refuse
/// everything".
///
/// **The same document with one key changed.** `report_with_os_empty` is
/// `report_with_os_absent` with `osPackages` present and holding no packages,
/// which is the ordinary state of a distroless runtime — design §2.3 says so in
/// those words — and *is* an observation: the scanner looked and reported none.
/// A rule that collapsed absent into empty would refuse every distroless image
/// forever, and this lane is what makes that failure visible rather than
/// silently conservative.
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

/// The mirror on the other array, and the half that says what the rule is
/// really about.
///
/// With only the `osPackages` lane, the rule is satisfied by an implementation
/// that special-cases one key — and the defect is not about that key. It is that
/// a rescan can only prove a clean result over the parts of the image it
/// actually reported on, and a document with no `libraries` at all reports on no
/// more of it than one with no `osPackages`.
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

/// An unreported array does not excuse an advisory that is still there.
///
/// The tree's rescan omits `osPackages` **and** still reports the very advisory
/// the group set out to clear, in `libraries`. A surviving id is a positive
/// observation and no silence elsewhere qualifies it, so this is refused
/// outright rather than held as unproved — the same way round as the scanner
/// version comparison, and for the same reason.
///
/// Without this lane, the rule above is satisfied by one that answers
/// `NotObserved` before looking at anything, which would turn every refusal in
/// this file into a shrug the moment a scanner dropped an array.
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

/// A rescan whose document this build cannot read is not evidence of anything.
///
/// The scanner wrote its artefact, so the fifth check passes by its declared
/// criterion — and the document is still not a scan report. Refusing it is the
/// same direction the artefact criterion already takes for a scan that produced
/// nothing usable: a gate that excused an unreadable report would get weaker
/// exactly when the scanner started misbehaving.
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

/// A contract that claims no repair cannot be accepted, whatever the checks say.
///
/// Five green checks and a scanner that wrote a report — and nothing said what
/// this attempt set out to fix, so there is no advisory to look for and no
/// earlier scan to compare a version with. The safe direction is the one a
/// forgotten premise falls in: not refused, because nothing went wrong, and
/// never accepted, because nothing was proved.
#[tokio::test]
async fn a_contract_with_no_repair_premise_is_never_accepted() {
    let r = evaluate(&contract(), &green_tree())
        .await
        .expect("an evaluation that was not cancelled");

    assert!(!r.rejected());
    assert!(!r.accepted(), "there was no premise to prove");
    assert_eq!(r.rescan(), &RescanVerdict::NotCompared);
}

// ---------------------------------------------------------------------------
// The fold rule (Task 13)
// ---------------------------------------------------------------------------
//
// A rescan reports on the whole image, not on the group that caused it, so it
// can show that a *later* group's advisories have gone too. One base image bump
// clears a dozen OS findings filed against a dozen groups, and re-attempting
// each of them would open a repair against a tree that already has the fix.
//
// Every lane below except the first asserts `Proceed`, and that is the shape of
// the risk rather than an accident of what was easy to write: folding wrongly
// records advisories as fixed that nothing fixed, on a branch that gets merged
// and in a report a person reads, while refusing wrongly costs one redundant
// attempt. `a_group_cleared_by_an_earlier_committed_bump_…` is the positive
// control the other four need — without it a rule that never folds passes all of
// them and `AlreadyResolved` is unreachable.

/// The one lane that folds, and the reason the four refusals below mean
/// anything.
///
/// An earlier group ended clean and its bump was committed. Its rescan reported
/// nothing, so this group's advisory is gone from a document this rule is
/// willing to rest on, and the work is already done. Nothing is edited — the
/// commit that records it changes no file at all, which is what
/// `a_fold_is_recorded_without_rewriting_anything` is about.
#[tokio::test]
async fn a_group_cleared_by_an_earlier_committed_bump_is_recorded_without_a_file_change() {
    let prior = rescan_from_committed_clean_group(&[]).await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::AlreadyResolved
    );
}

/// The negative case, and the one the rule exists for.
///
/// The earlier group's bump was reverted, so it is not on the branch. Its rescan
/// is a perfectly accurate document about a tree that no longer exists — its own
/// verdict is `Cleared`, which the fixture asserts — and folding on it would
/// record this group's advisory as fixed by a change nobody will merge.
///
/// Its pair is `a_clean_group_whose_bump_was_not_committed_is_not_foldable`. The
/// two approach the same fact from opposite sides, which is what shows the rule
/// consults the branch rather than inferring it from the verdict.
#[tokio::test]
async fn a_needs_work_groups_rescan_is_not_foldable() {
    let prior = rescan_from_needs_work_group(&[]).await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::Proceed,
        "its bump was reverted, so nothing on the branch fixes this group"
    );
}

/// The same hazard reached from the other side: the group ended clean and the
/// commit did not happen.
///
/// Without this lane the rule is satisfied by one that reads only "did this end
/// clean", and a clean group whose commit failed would be folded on — a rescan
/// describing a tree the branch does not carry, exactly as above.
#[tokio::test]
async fn a_clean_group_whose_bump_was_not_committed_is_not_foldable() {
    let prior = rescan_from_a_clean_group_that_was_not_committed(&[]).await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::Proceed,
        "a clean verdict about a tree the branch does not carry is not a fix"
    );
}

/// Every id, not merely one.
///
/// The earlier rescan cleared `CVE-2026-5` and still reports `CVE-2026-6`, and
/// both belong to this group — one edit fixing two advisories. The edit is still
/// owed, and folding here would drop `CVE-2026-6` with nothing left to report it
/// missing.
///
/// The premise is the interesting half: the surviving id is *in this group*, so
/// a rule reading `any` rather than `all` folds and loses it.
#[tokio::test]
async fn a_partially_cleared_group_proceeds() {
    let prior = rescan_from_committed_clean_group(&["CVE-2026-6"]).await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5", "CVE-2026-6"]), Some(&prior)),
        Fold::Proceed,
        "every id must be absent, not merely one"
    );
}

/// The first group of a run has no earlier rescan, so there is nothing to fold
/// on.
#[tokio::test]
async fn the_first_group_of_a_run_proceeds() {
    assert_eq!(fold(&group_of(&["CVE-2026-5"]), None), Fold::Proceed);
}

/// An absence seen through a moved advisory feed is not evidence about the tree,
/// **even though the bump was committed**.
///
/// `Provisional` is not a refusal over in `evaluate` — nothing went wrong with
/// the tree — so a disposition may keep such a bump on the branch and flag it.
/// That makes committed-and-provisional a reachable state, and it is the lane
/// that shows the clean gate decides something the branch gate cannot: here the
/// branch does carry the tree, and the document still proves nothing.
#[tokio::test]
async fn a_provisional_rescan_is_not_foldable_even_though_its_bump_was_committed() {
    let prior = rescan_from_a_committed_group_at_another_scanner_version().await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::Proceed,
        "a finding leaves a scan because the tree changed or because the feed did"
    );
}

/// Silence about half the image is not a fold.
///
/// The earlier rescan's document carried no `osPackages` key, so it did not
/// report that the OS findings were gone — it said nothing about OS packages.
/// Every id is absent from such a document for free, which makes this the lane
/// where folding on absence is cheapest and most wrong. It is
/// `cve::project::Arm`'s absent-versus-empty distinction reaching the rule that
/// would otherwise be fooled by it.
#[tokio::test]
async fn an_array_the_rescan_never_reported_on_is_not_a_fold() {
    let prior = rescan_from_a_committed_group_that_reported_on_one_array().await;

    assert_eq!(
        fold(&group_of(&["CVE-2026-5"]), Some(&prior)),
        Fold::Proceed,
        "an array the scanner never wrote supplies absences for free"
    );
}

/// What recording a fold *is*, and the flag pair the whole hazard hides behind.
///
/// A fold changes no file, so `--allow-empty` is what makes it a commit at all.
/// `--amend` is the obvious alternative and is forbidden: on a branch this run is
/// reusing, the commit before this one may belong to a previous run and already
/// be pushed, so amending it would require a force-push this system does not do.
/// The defect is invisible on a fresh branch and only ever arrives on a reused
/// one, which is why it is pinned here rather than left to whoever wires the
/// committer.
///
/// The body names the advisories because `cve::dedup`'s log scan reads commit
/// bodies for ids: a fold whose body named none would be invisible to the next
/// run, which would then re-derive the whole decision.
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
