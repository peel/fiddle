//! The scanner port, gated against the scripted `wizcli`.
//!
//! Every test here drives a compiled fixture binary through the same seam an
//! operator would pin a real `wizcli` at, so what is asserted is a *subprocess
//! contract*: what the adapter does with an artefact, an exit code and a stream
//! of diagnostics it did not write. Nothing imports the stub as a library, and
//! nothing here reaches a network, a credential or a Wiz tenant — the gate is
//! offline by construction, and the scripted scanner is permanent rather than a
//! placeholder for one.

mod support;

// `Scanner` is imported for its method and not named again: the suite drives a
// scan through the port, which is the seam a capability will hold, rather than
// through whatever concrete adapter `scanner_with` happened to build.
use fiddle_runtime::scanner::ScanError;
use fiddle_runtime::Scanner as _;
use support::cve::{
    absent_scanner, arm_exits_with, arm_was_exercised, image, observed_exit, scanner_with, ARMS,
};

#[tokio::test]
async fn the_stub_can_produce_each_unsuccessful_arm() {
    // The stub is the gate's scanner. It must be able to produce every arm the
    // adapter has to discriminate, or the failure tests built on it cannot
    // exist: a suite whose fixture can only ever succeed proves that the
    // successful path works and says nothing at all about the other five.
    //
    // Iterated over `ARMS` rather than over a list written here, so the arms
    // this asserts about and the arms `arm_was_exercised` knows how to check
    // are one list. `ARMS` is a fixed-length array for the reason `all_shapes`
    // is: dropping an entry has to be a compile error rather than a quietly
    // shorter loop.
    for arm in ARMS {
        let out = scanner_with(support::wiz_stub(arm)).scan(&image()).await;
        assert!(
            arm_was_exercised(arm, &out),
            "arm {arm} is producible, but the scan came back {out:?}"
        );
        // The outcome above is not enough on its own. `ok` and
        // `exit-nonzero-with-file` are both a successful report, and that is
        // the adapter behaving correctly rather than an oversight — so an arm
        // that stopped exiting non-zero would satisfy every assertion above it
        // while no longer being the situation it is named for. The status is
        // the only thing that separates those two, and `empty-file` and
        // `unparseable-file` likewise mean nothing unless they end cleanly, so
        // it is pinned for all six. See `arm_exits_with` for each arm's.
        assert_eq!(
            observed_exit(arm),
            arm_exits_with(arm),
            "arm {arm} no longer ends on the status line that defines it"
        );
    }
}

#[tokio::test]
async fn a_scanner_that_is_not_installed_is_its_own_classification() {
    // The sixth `ScanError`, and the only one with no arm above it: the loop
    // covers what a running scanner can do, and this covers there not being one.
    // Left untested it would be the one variant whose remedy — install the
    // scanner, or fix the path it was pinned to — is reachable only through a
    // classification nothing has ever seen the adapter produce.
    let out = scanner_with(absent_scanner()).scan(&image()).await;
    // The program is asserted as well as the variant. `Missing` is reached from
    // a `NotFound` raised somewhere inside spawning, and a build that resolved
    // the seam against the wrong path — a working directory, an inherited
    // `PATH` — would raise exactly the same error about a different program and
    // pass a test that only matched the variant.
    match out {
        Err(ScanError::Missing { program, .. }) => {
            assert_eq!(program, std::path::PathBuf::from(absent_scanner().program));
        }
        other => panic!("a scanner that is not on disk came back as {other:?}"),
    }
}
