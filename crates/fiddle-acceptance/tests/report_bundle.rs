//! The published evidence bundle, observed from outside the process.
//!
//! Everything asserted here is read back off the filesystem the way an operator
//! or a downstream tool would read it: the run is launched as a subprocess, the
//! bundle path is taken from the payload the run printed, and the bundle itself
//! is parsed as plain JSON. Nothing calls a library function.

mod support;

use support::{walkdir_dirs, walkdir_files, Scenario};

/// The bundle is what makes a run auditable after the fact, so it has to name
/// the build that produced it: a report that cannot be attributed to a version
/// and a revision is evidence about nothing in particular.
#[test]
fn run_publishes_a_bundle_carrying_package_version_and_source_revision() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");
    let v = s.run_json("beans:fiddle-m0-demo", 0);
    let path = s.report_dir().join(v["report"].as_str().unwrap());
    let b: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(b["schema"], "fiddle.report.v0");
    let ver = b["fiddle"]["package_version"].as_str().unwrap();
    assert!(
        ver.split('.').count() == 3,
        "package_version must be semver, got {ver}"
    );
    let rev = b["fiddle"]["source_revision"].as_str().unwrap();
    assert!(
        rev == "unknown" || (rev.len() == 40 && rev.chars().all(|c| c.is_ascii_hexdigit())),
        "source_revision must be a 40-hex sha or the literal `unknown`, got {rev:?}"
    );
    assert_eq!(b["outcome"], "completed");
    assert_eq!(b["next_action"], "complete");
    assert_eq!(b["capability_executions"][0]["capability_id"], "stub_mark");
    assert!(b["observations"]["work_item"]["available"].is_object());
    assert!(b["observations"]["changes"]["available"].is_object());
    // Design §4.7 requires `progress` alongside `capability_executions`.
    assert_eq!(b["progress"][0]["capability_id"], "stub_mark");
    assert_eq!(b["progress"][0]["stage"], "mark");
    assert_eq!(b["progress"][0]["status"], "completed");
    assert!(!b["progress"][0]["summary"].as_str().unwrap().is_empty());
    assert_eq!(
        b["mode"], "unattended",
        "the bundle records the mode it ran under"
    );
    assert_eq!(b["invocation_ref"], "beans:fiddle-m0-demo");
    assert_eq!(b["work_ref"], "beans:fiddle-m0-demo");
    assert!(
        !b["attempt_id"].as_str().unwrap().is_empty(),
        "the bundle must name the attempt that produced it"
    );
}

/// The atomicity property, injected at the publication boundary this task
/// introduces.
///
/// Three separate claims, all of which have to hold: the run reports `failed`
/// rather than `retryable` (repeating it would fail identically until someone
/// fixes the directory), the diagnostic names the directory so they know which
/// one, and nothing partial survives — no bundle, and no staging directory
/// either.
#[cfg(unix)]
#[test]
fn an_unwritable_report_dir_exits_20_and_leaves_no_partial_bundle() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");
    s.make_report_dir_unwritable();

    let out = s.run_raw("beans:fiddle-m0-demo");

    if out.status.code() == Some(0) {
        // Running with an identity that ignores the permission bits, so the
        // failure could not be injected at all.
        s.make_report_dir_writable();
        return;
    }
    assert_eq!(
        out.status.code(),
        Some(20),
        "a bundle that could not be published is failed, not retryable; stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains(s.report_dir().to_str().unwrap()),
        "diagnostic must name the path: {stderr}"
    );

    s.make_report_dir_writable();
    let leftovers: Vec<_> = walkdir_dirs(s.report_dir())
        .into_iter()
        .chain(walkdir_files(s.report_dir()))
        .filter(|p| p.to_string_lossy().contains(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "partial bundle left behind: {leftovers:?}"
    );
    assert!(
        walkdir_files(s.report_dir())
            .iter()
            .all(|p| p.file_name().unwrap() != "report.json"),
        "no report.json may exist when publication failed"
    );
}

/// A publication failure must stay distinguishable from a capability failure:
/// the capability writing the change set is retryable (exit 11), publishing the
/// bundle is not (exit 20). Same fixture, two different boundaries, two
/// different answers — asserted together so neither can drift into the other.
#[cfg(unix)]
#[test]
fn a_capability_failure_stays_retryable_and_distinct_from_a_publication_failure() {
    use std::os::unix::fs::PermissionsExt;

    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");
    let changes = s.stub_root().join("changes");
    std::fs::set_permissions(&changes, std::fs::Permissions::from_mode(0o500)).unwrap();

    let out = s.run_raw("beans:fiddle-m0-demo");

    std::fs::set_permissions(&changes, std::fs::Permissions::from_mode(0o755)).unwrap();
    if out.status.code() == Some(0) {
        // Running with an identity that ignores the permission bits.
        return;
    }
    assert_eq!(
        out.status.code(),
        Some(11),
        "a failed change-set write is retryable; stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The bundle is still published: the run failed at the capability, not at
    // publication, and the record of *that* is exactly what a reader needs.
    let bundles: Vec<_> = walkdir_files(s.report_dir())
        .into_iter()
        .filter(|p| p.file_name().unwrap() == "report.json")
        .collect();
    assert_eq!(bundles.len(), 1, "got {bundles:?}");
    let b: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&bundles[0]).unwrap()).unwrap();
    assert!(
        b["outcome"]["retryable"]["reason"]
            .as_str()
            .unwrap()
            .contains("change set"),
        "the bundle must record which boundary failed: {b}"
    );
    assert_eq!(b["capability_executions"][0]["status"], "failed");
}
