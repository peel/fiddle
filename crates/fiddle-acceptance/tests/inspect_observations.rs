mod support;

use support::{repo_root, Scenario};

#[test]
fn inspect_reports_available_observations_with_sources() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");

    let v = s.inspect_json("beans:fiddle-m0-demo");

    assert_eq!(
        v["observations"]["work_item"]["available"]["value"]["status"],
        "open"
    );
    assert_eq!(
        v["observations"]["work_item"]["available"]["source"],
        "stub:work/fiddle-m0-demo.json"
    );
    assert!(
        v["observations"]["changes"]["available"].is_object(),
        "changes = {}",
        v["observations"]["changes"]
    );
    assert!(v["observations"]["changes"]["available"]["value"]["marker"].is_null());
}

#[test]
fn missing_stub_root_is_unavailable_not_empty() {
    let s = Scenario::new();
    s.remove_stub_root();

    let v = s.inspect_json_expect_code("beans:fiddle-m0-demo", 0);

    for port in ["work_item", "changes"] {
        let observed = &v["observations"][port];
        assert!(
            observed["unavailable"].is_object(),
            "expected unavailable for {port}, got {observed}"
        );
        assert!(
            observed["unavailable"]["reason"]
                .as_str()
                .unwrap()
                .contains("unreadable"),
            "{port} must say why it could not be observed, got {observed}"
        );
        assert!(
            observed["unavailable"]["source"].is_string(),
            "{port} must still name the source it could not read, got {observed}"
        );
        assert!(
            observed["available"].is_null(),
            "must not degrade an unreadable source into an empty value; {port} = {observed}"
        );
    }
}

#[test]
fn malformed_work_item_is_unavailable_not_defaulted() {
    let s = Scenario::new();
    std::fs::write(s.stub_root().join("work/fiddle-m0-demo.json"), "{ not json").unwrap();

    let v = s.inspect_json("beans:fiddle-m0-demo");

    let observed = &v["observations"]["work_item"];
    assert!(
        observed["unavailable"]["reason"]
            .as_str()
            .unwrap()
            .contains("malformed"),
        "got {observed}"
    );
    assert!(observed["available"].is_null(), "got {observed}");
}

#[test]
fn the_tracked_demo_fixture_is_observable_from_the_repository_root() {
    let out = support::fiddle_command()
        .current_dir(repo_root())
        .args([
            "inspect",
            "beans:fiddle-m0-demo",
            "--config",
            "tests/fixtures/fiddle.toml",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["observations"]["work_item"]["available"]["value"]["status"],
        "open"
    );
    assert_eq!(
        v["observations"]["work_item"]["available"]["source"],
        "stub:work/fiddle-m0-demo.json"
    );
}
