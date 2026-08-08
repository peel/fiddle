//! Black-box coverage of `fiddle run` — the first command that changes the
//! world.
//!
//! Everything here is asserted from outside the process: an exit code, a
//! `--json` payload, and the fixture file the run left behind. Each scenario
//! builds its own temporary project, so no test depends on another's state and
//! none of them touches the tracked fixtures.

mod support;

use support::Scenario;

/// The happy path of design §4.4: unstarted work derives `Execute`, the
/// capability writes the correlation key, and the run completes.
#[test]
fn run_executes_the_stub_capability_and_completes() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let v = s.run_json("beans:fiddle-m0-demo", 0);

    assert_eq!(v["outcome"], "completed");
    assert_eq!(
        v["capability_executions"][0]["capability_id"], "stub_mark",
        "got {}",
        v["capability_executions"]
    );
    assert_eq!(v["capability_executions"][0]["status"], "completed");

    let marker = s
        .read_change_marker("fiddle-m0-demo")
        .expect("capability must write the marker");
    assert_eq!(
        marker.len(),
        16,
        "correlation key must be 16 hex chars, got {marker:?}"
    );
    assert!(marker.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(
        marker,
        s.expected_marker("beans:fiddle-m0-demo"),
        "the marker written must be this project and invocation's correlation key"
    );
}

/// The fail-closed arm of design §4.3, asserted where it matters most: a world
/// fiddle cannot observe must not be acted on. Exit 20 is the numeric row of
/// the exit-code table, read from outside the process.
#[test]
fn run_on_an_unobservable_source_fails_closed_with_exit_20() {
    let s = Scenario::new();
    s.remove_stub_root();

    let v = s.run_json("beans:fiddle-m0-demo", 20);

    assert!(v["outcome"]["failed"].is_object(), "got {}", v["outcome"]);
    assert!(
        v["capability_executions"].as_array().unwrap().is_empty(),
        "a blocked derivation must never execute the capability"
    );
    assert!(
        v["progress"].as_array().unwrap().is_empty(),
        "nothing ran, so nothing can have made progress"
    );
    assert!(
        s.read_change_marker("fiddle-m0-demo").is_none(),
        "a blocked run must leave no marker behind"
    );
}

/// A completed run must describe the state it left behind. The `execute` it
/// derived on entry is no longer true by the time it reports, and echoing it
/// would send the caller round the loop for work that is already done.
/// Design §4.7.
#[test]
fn run_reports_complete_after_a_successful_execution() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let v = s.run_json("beans:fiddle-m0-demo", 0);

    assert_eq!(
        v["next_action"],
        serde_json::json!("complete"),
        "a completed run must not advertise work still to do, got {}",
        v["next_action"]
    );
    // The observations reported must be the post-execution ones the action was
    // derived from — otherwise `complete` would rest on a view that does not
    // show the marker.
    assert_eq!(
        v["observations"]["changes"]["available"]["value"]["marker"],
        serde_json::json!(s.expected_marker("beans:fiddle-m0-demo")),
        "got {}",
        v["observations"]["changes"]
    );
}

/// The whole `run` surface design §4.5 documents, not a subset of it.
#[test]
fn run_accepts_the_documented_mode_and_capability_flags() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let v = s.run_json_with(
        &["--mode", "attended", "--capability", "stub_mark"],
        "beans:fiddle-m0-demo",
        0,
    );

    assert_eq!(v["outcome"], "completed");
    assert_eq!(
        v["mode"], "attended",
        "the bundle must record the mode it ran under"
    );

    // The other mode is accepted too, and is what a run defaults to.
    let d = Scenario::new();
    d.write_work_item("fiddle-m0-demo", "open");
    assert_eq!(
        d.run_json_with(&["--mode", "unattended"], "beans:fiddle-m0-demo", 0)["mode"],
        "unattended"
    );

    let o = Scenario::new();
    o.write_work_item("fiddle-m0-demo", "open");
    assert_eq!(
        o.run_json("beans:fiddle-m0-demo", 0)["mode"],
        "unattended",
        "omitting --mode must be the same as naming the default"
    );
}

/// An unknown capability id is a usage error, not a silent no-op: a run asked
/// to do something this build has never heard of, and that exited 0 having done
/// nothing, would be indistinguishable from a run that did the work.
#[test]
fn run_rejects_an_unknown_capability_id_rather_than_no_opping() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let out = s.run_raw_with(
        &["--capability", "not-a-capability"],
        "beans:fiddle-m0-demo",
    );

    assert_eq!(
        out.status.code(),
        Some(2),
        "an unknown capability id is a usage error"
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("not-a-capability") && stderr.contains("stub_mark"),
        "diagnostic must name the bad value and the known ids: {stderr}"
    );
    assert!(
        s.read_change_marker("fiddle-m0-demo").is_none(),
        "a rejected invocation must not have executed anything"
    );
}

/// A `--mode` fiddle does not know is rejected by the same row of the table,
/// and the diagnostic names the alternatives.
#[test]
fn run_rejects_an_unknown_mode() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let out = s.run_raw_with(&["--mode", "supervised"], "beans:fiddle-m0-demo");

    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("supervised") && stderr.contains("attended"),
        "got {stderr}"
    );
    assert!(s.read_change_marker("fiddle-m0-demo").is_none());
}

/// Row `11` of the exit-code table, asserted numerically from outside the
/// process like the rest of them.
///
/// The world stays observable — the derivation has to reach `Execute` for the
/// capability to fail at all — so the failure is injected as a change directory
/// that can be listed but not written to. That is a Unix permission, hence the
/// gate; an identity that ignores permission bits makes the case unbuildable,
/// hence the early return.
#[cfg(unix)]
#[test]
fn a_capability_that_cannot_write_exits_11_and_records_the_failed_execution() {
    use std::os::unix::fs::PermissionsExt;

    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");
    let changes = s.stub_root().join("changes");

    std::fs::set_permissions(&changes, std::fs::Permissions::from_mode(0o500)).unwrap();
    let out = s.run_raw_with(&["--json"], "beans:fiddle-m0-demo");
    std::fs::set_permissions(&changes, std::fs::Permissions::from_mode(0o755)).unwrap();

    if out.status.code() == Some(0) {
        return; // running with an identity that ignores the permission bits
    }

    assert_eq!(
        out.status.code(),
        Some(11),
        "stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["outcome"]["retryable"].is_object(),
        "got {}",
        v["outcome"]
    );
    assert_eq!(v["capability_executions"][0]["status"], "failed");
    assert!(
        s.read_change_marker("fiddle-m0-demo").is_none(),
        "a failed execution must leave no marker and no debris"
    );
}

/// A reader at a terminal is entitled to the same conclusions the payload
/// carries, not only to the exit code.
#[test]
fn the_human_rendering_names_the_outcome_the_mode_and_what_ran() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let out = s.run_raw_with(&[], "beans:fiddle-m0-demo");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8(out.stdout).unwrap();

    assert!(stdout.contains("outcome     = completed"), "got {stdout}");
    assert!(stdout.contains("mode        = unattended"), "got {stdout}");
    assert!(stdout.contains("next action = complete"), "got {stdout}");
    assert!(stdout.contains("stub_mark completed"), "got {stdout}");
}
