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
use fiddle_runtime::Scanner as _;
use support::cve::{arm_was_exercised, image, scanner_with, ARMS};

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
    }
}
