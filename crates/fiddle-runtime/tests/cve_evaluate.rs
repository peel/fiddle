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
//! [`Success`]: fiddle_runtime::evaluate::Success

mod support;

use fiddle_runtime::evaluate::{evaluate, Outcome, Success};
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
