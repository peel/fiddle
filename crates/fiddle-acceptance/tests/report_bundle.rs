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

/// The atomicity property, injected at the publication boundary this milestone
/// introduces — and, with it, the fail-closed rule that governs it.
///
/// An unwritable `<report.dir>` is where an attempt records *both* what it is
/// about to do and what it did, so a wholly unwritable one stops the attempt
/// before it touches anything. Four claims, all of which have to hold: the run
/// exits `11` rather than `20` (repeating it after an operator fixes the
/// directory succeeds, which is what `retryable` promises and `failed` denies),
/// the diagnostic names the directory so they know which one, the fixture is
/// untouched, and nothing partial survives — no bundle, and no staging directory
/// either.
#[cfg(unix)]
#[test]
fn an_unwritable_report_dir_exits_11_and_changes_nothing() {
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
        Some(11),
        "an attempt that could not record itself is retryable — fixing the directory and \
         repeating it succeeds; stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains(s.report_dir().to_str().unwrap()),
        "diagnostic must name the path: {stderr}"
    );
    assert!(
        s.read_change_marker("fiddle-m0-demo").is_none(),
        "an attempt that could not record what it was about to do must not do it"
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

    // And the promise the exit code makes is kept: the same invocation, repeated
    // after the operator fixed the directory, succeeds.
    let repeated = s.run_json("beans:fiddle-m0-demo", 0);
    assert_eq!(repeated["outcome"], "completed");
}

/// The other half of the publication boundary, which the fail-closed case above
/// cannot reach: an attempt whose capability *succeeded* and whose bundle could
/// not be published.
///
/// Injected by leaving the journal directory writable while `<report.dir>` itself
/// is not, which is the state a `<report.dir>` sealed after an earlier attempt
/// would be in. The claim is that the world moving is never unrecorded: the
/// marker is on disk, no bundle exists, and the journal says which capability ran
/// under which attempt.
#[cfg(unix)]
#[test]
fn a_publication_failure_after_a_successful_execution_still_records_it() {
    let s = Scenario::new();
    s.write_work_item("fiddle-m0-demo", "open");
    s.prepare_journal_dir();
    s.make_report_dir_unwritable();

    let out = s.run_raw_with(&["--json"], "beans:fiddle-m0-demo");

    s.make_report_dir_writable();
    if out.status.code() == Some(0) {
        return; // an identity that ignores the permission bits
    }
    assert_eq!(
        out.status.code(),
        Some(11),
        "stderr = {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["report"].is_null(),
        "no bundle was published, so the payload must name none: {v}"
    );
    assert_eq!(
        v["capability_executions"][0]["status"], "completed",
        "the capability succeeded; got {}",
        v["capability_executions"]
    );
    assert!(
        s.read_change_marker("fiddle-m0-demo").is_some(),
        "the world moved — this case is about what records that"
    );
    assert!(
        walkdir_files(s.report_dir())
            .iter()
            .all(|p| p.file_name().unwrap() != "report.json"),
        "publication failed, so no bundle may exist"
    );

    // The durable record of the execution, read the way an operator would find
    // it: a file under `<report.dir>` naming the attempt and the capability.
    let records = s.journal_records();
    assert_eq!(records.len(), 1, "got {records:?}");
    let text = std::fs::read_to_string(&records[0]).unwrap();
    assert!(
        text.contains("stub_mark") && text.contains("completed"),
        "the journal must record that the capability executed: {text}"
    );
}

/// A publication failure must stay distinguishable from a capability failure.
/// Both are retryable and both exit `11`, so the reason is what tells them
/// apart — the capability names the change set, publication names the report
/// bundle, and an intent that could not be recorded names the attempt journal.
/// Same fixture, two different boundaries, asserted together so neither reason
/// can drift into the other.
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
