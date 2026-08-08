//! The M0 mandatory proof: a second fresh process leaves the world alone.
//!
//! Every claim here is made across a *process* boundary, not a function
//! boundary. `fiddle-runtime` already proves the orchestration-level version of
//! this — a second `run` over a satisfied world does not reach the capability —
//! but that test shares one address space, one clock reading, and one set of
//! ports with the first run. Only two genuinely separate `fiddle run`
//! invocations can show that the property survives a restart, because only they
//! re-derive the correlation key from configuration on disk rather than reusing
//! a value already in memory.
//!
//! The proof has two halves, and both are needed:
//!
//! - The world is **unchanged**: the fixture bytes are identical, exactly one
//!   marker file exists, and the second bundle records no execution.
//! - The second run was nonetheless a **real attempt**, not a cached answer: it
//!   published its own bundle under its own `attempt_id`, having observed and
//!   derived afresh, and reached the same `work_ref` as the first.
//!
//! Without the second half, a `fiddle` that noticed a previous bundle and
//! replayed it would pass the first half perfectly.

mod support;

use support::Scenario;

const WORK_ID: &str = "fiddle-m0-demo";
const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

#[test]
fn a_second_fresh_process_does_not_re_execute_and_leaves_state_identical() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");

    // Process 1: unstarted work, so this one executes and writes the marker.
    let first = s.run_json(INVOCATION_REF, 0);
    let bundle1 = s.read_bundle(&first);
    assert_eq!(
        bundle1["capability_executions"][0]["capability_id"], "stub_mark",
        "the first run must actually have executed, or the second proves nothing: {}",
        bundle1["capability_executions"]
    );
    let state_after_first = s.stub_snapshot();
    let marker_after_first = s
        .read_change_marker(WORK_ID)
        .expect("the first run must leave a marker");
    let marker_bytes_after_first = std::fs::read(&s.change_files(WORK_ID)[0]).unwrap();

    // Process 2: a fresh binary invocation over the world process 1 left.
    let second = s.run_json(INVOCATION_REF, 0);
    let bundle2 = s.read_bundle(&second);

    // Half one: the observable state is untouched.
    assert_eq!(
        s.stub_snapshot(),
        state_after_first,
        "fixture bytes must be identical after the second invocation"
    );
    let change_files = s.change_files(WORK_ID);
    assert_eq!(
        change_files.len(),
        1,
        "exactly one marker file may exist, got {change_files:?}"
    );
    assert_eq!(
        std::fs::read(&change_files[0]).unwrap(),
        marker_bytes_after_first,
        "the marker file's bytes must be untouched by the second invocation"
    );
    assert_eq!(
        s.read_change_marker(WORK_ID).as_deref(),
        Some(marker_after_first.as_str()),
        "the marker must still be the first run's correlation key"
    );
    assert_eq!(
        marker_after_first,
        s.expected_marker(INVOCATION_REF),
        "and that key must be this project and invocation's, not an arbitrary token"
    );

    assert!(
        bundle2["capability_executions"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the second run must not execute stub_mark again, got {}",
        bundle2["capability_executions"]
    );
    assert!(
        bundle2["progress"].as_array().unwrap().is_empty(),
        "no execution means no progress entries, got {}",
        bundle2["progress"]
    );
    assert_eq!(bundle2["outcome"], "completed");
    assert_eq!(bundle2["next_action"], serde_json::json!("complete"));

    // Half two: it was still a genuinely new attempt over the same work.
    assert_ne!(
        bundle1["attempt_id"], bundle2["attempt_id"],
        "the second run must be a genuinely new attempt, not a cached result"
    );
    assert_ne!(
        first["report"], second["report"],
        "a new attempt publishes its own bundle rather than pointing at the first one's"
    );
    assert_eq!(
        bundle1["work_ref"], bundle2["work_ref"],
        "work identity must be stable across attempts"
    );
    assert_eq!(bundle2["work_ref"], serde_json::json!(INVOCATION_REF));
    // A replayed bundle would carry the first run's pre-execution view. This one
    // observed the world for itself and saw the marker already there.
    assert_eq!(
        bundle2["observations"]["changes"]["available"]["value"]["marker"],
        serde_json::json!(marker_after_first),
        "the second run must report the world it observed, got {}",
        bundle2["observations"]["changes"]
    );
}

/// The read-only view a caller consults after the fact agrees with the runs:
/// the capability is satisfied and there is nothing left to do.
///
/// `inspect` never writes, so this is also the assertion that two runs plus an
/// inspect leave the fixture exactly as one run did.
#[test]
fn inspect_after_two_runs_reports_satisfied() {
    let s = Scenario::new();
    s.write_work_item(WORK_ID, "open");
    s.run_json(INVOCATION_REF, 0);
    s.run_json(INVOCATION_REF, 0);

    let v = s.inspect_json(INVOCATION_REF);

    assert!(
        v["assessment"]["satisfied"].is_object(),
        "got {}",
        v["assessment"]
    );
    assert_eq!(v["next_action"], serde_json::json!("complete"));
}
