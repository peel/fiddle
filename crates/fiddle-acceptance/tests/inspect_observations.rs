//! Black-box coverage of the observations `fiddle inspect --json` reports.
//!
//! Two halves of one contract are asserted here from outside the process: an
//! observable source is reported `available` with a source reference naming
//! where it came from, and an *un*observable source is reported `unavailable`
//! with a reason — never degraded into an empty or absent value.

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
    // The change set has not been marked yet, and an unmarked change set over a
    // readable fixture root is a real observation — not an unobservable one.
    assert!(
        v["observations"]["changes"]["available"].is_object(),
        "changes = {}",
        v["observations"]["changes"]
    );
    assert!(v["observations"]["changes"]["available"]["value"]["marker"].is_null());
}

/// RFC line 796: `Unavailable` is not equivalent to empty or absent. A missing
/// fixture root must surface as an explicit unobservable state carrying a
/// reason, and must not leave an `available` observation behind for a consumer
/// to mistake for "there is nothing there".
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

/// A fixture root that exists but whose work item is not JSON is unobservable
/// too — a parse failure must never be silently defaulted into a work item.
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

/// The tracked fixture in `tests/fixtures/` is the documented demo, and
/// `stub.root` in it is repository-relative — so this runs the binary from the
/// repository root, exactly as the documentation tells a reader to.
#[test]
fn the_tracked_demo_fixture_is_observable_from_the_repository_root() {
    let out = assert_cmd::Command::cargo_bin("fiddle")
        .unwrap()
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
