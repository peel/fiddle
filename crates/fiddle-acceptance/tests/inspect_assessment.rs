mod support;

use support::Scenario;

#[test]
fn inspect_derives_execute_for_unstarted_work_and_stays_read_only() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let before = s.stub_snapshot();
    let v = s.inspect_json("beans:fiddle-m0-demo");

    assert!(
        v["assessment"]["not_started"].is_object(),
        "got {}",
        v["assessment"]
    );
    assert_eq!(v["next_action"]["execute"]["capability_id"], "stub_mark");
    assert_eq!(
        s.stub_snapshot(),
        before,
        "inspect must not mutate fixture state"
    );
    assert!(
        !s.report_dir().exists(),
        "inspect must not publish a report bundle"
    );
}

#[test]
fn inspect_derives_complete_when_marked_and_blocked_when_unobservable() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");
    s.write_change_marker("fiddle-m0-demo", &s.expected_marker("beans:fiddle-m0-demo"));

    let before = s.stub_snapshot();
    let v = s.inspect_json("beans:fiddle-m0-demo");

    assert!(
        v["assessment"]["satisfied"].is_object(),
        "got {}",
        v["assessment"]
    );
    assert_eq!(v["next_action"], serde_json::json!("complete"));
    assert_eq!(
        s.stub_snapshot(),
        before,
        "inspect must not mutate fixture state"
    );
    assert!(!s.report_dir().exists());

    let f = Scenario::new();
    f.write_work_item("fiddle-m0-demo", "open");
    f.write_change_marker("fiddle-m0-demo", "deadbeefdeadbeef");

    let fv = f.inspect_json("beans:fiddle-m0-demo");

    assert!(
        fv["assessment"]["blocked"].is_object(),
        "a foreign marker must be blocked, got {}",
        fv["assessment"]
    );
    let reason = fv["assessment"]["blocked"]["reason"].as_str().unwrap();
    assert!(
        reason.contains("deadbeefdeadbeef")
            && reason.contains(&f.expected_marker("beans:fiddle-m0-demo")),
        "the reason must name both the found and the expected marker: {reason}"
    );
    assert!(
        fv["next_action"]["blocked"].is_object(),
        "a foreign marker must never derive an execution or a completion, got {}",
        fv["next_action"]
    );

    let b = Scenario::new();
    b.remove_stub_root();

    let v = b.inspect_json("beans:fiddle-m0-demo");

    assert!(
        v["assessment"]["blocked"].is_object(),
        "got {}",
        v["assessment"]
    );
    assert!(v["next_action"]["blocked"]["reason"]
        .as_str()
        .unwrap()
        .contains("unavailable"));
}

#[test]
fn the_human_rendering_names_the_assessment_and_the_next_action() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let out = s.inspect_human("beans:fiddle-m0-demo");

    assert!(out.contains("assessment  = not started"), "got {out}");
    assert!(out.contains("next action = execute stub_mark"), "got {out}");
}
