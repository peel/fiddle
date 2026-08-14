//! What a scanner document is allowed to become once it is inside fiddle.
//!
//! The subject is [`fiddle_runtime::cve::project`], and the four properties this
//! suite exists for are the four ways a projection fails *quietly*:
//!
//! 1. **Reading one package array.** `osPackages` is empty for a distroless
//!    runtime, so a projection that only reads `libraries` looks correct for as
//!    long as nobody changes the base image, and then drops every OS finding
//!    without reporting anything.
//! 2. **Collapsing an absent array into an empty one.** *The scanner said
//!    nothing about OS packages* and *the scanner looked and found none* are
//!    different facts, and only the second one is evidence.
//! 3. **Filtering where subtraction is meant.** One advisory can be reported
//!    twice — once against a package with a fix and once against a package
//!    without — and a filter on `fixedVersion` puts it in *both* the fixable and
//!    the upstream-blocked list.
//! 4. **Letting the scanner's prose through.** The record a scanner writes
//!    carries free text authored outside this build, and the projection is the
//!    boundary that text stops at.
//!
//! Every world here is one of `support::cve`'s documents. Nothing runs a
//! scanner: the projection is a function of bytes, so the suite is offline by
//! construction and has no arm that needs a credential.

mod support;

use fiddle_core::{PackageType, ProjectedFinding};
use fiddle_runtime::cve::project::{project, Arm};
use fiddle_runtime::scanner::ScanReport;
use support::cve::{
    libraries, os_packages, report_with, report_with_advisory_description,
    report_with_duplicate_cve_one_fixed_one_not, report_with_os_absent, report_with_os_empty,
    Report, DEFAULT_LIBRARY_CVES, SENTINEL_PROSE,
};

// ---------------------------------------------------------------------------
// Putting a document where a scan would have left one
// ---------------------------------------------------------------------------
//
// These two are plumbing rather than worlds, which is why they are here and not
// in `support::cve` under that module's extension convention: they build no
// fixture and decide nothing on a lane's behalf — they only carry the bytes a
// fixture already produced across the one type boundary between the builders
// and [`project`], which takes a [`ScanReport`] because that is what a real
// capability will be holding. The moment a second suite needs them they belong
// in the shared module, and the convention there says so.

/// The document a fixture wrote, parsed.
fn document_of(report: &Report) -> serde_json::Value {
    serde_json::from_str(report.raw()).expect("a fixture document is JSON")
}

/// A scan that produced `document`.
///
/// The provenance is fixed and uninteresting: no assertion in this suite reads
/// it, because the projection is a function of the document alone. It is spelled
/// implausibly on purpose, so that a value from here turning up in an assertion
/// about a real scan would be visible rather than plausible.
fn scan_of(document: serde_json::Value) -> ScanReport {
    ScanReport {
        document,
        scanner_version: "wizcli 0.0.0-fixture".to_string(),
        image_digest: "sha256:fixture".to_string(),
    }
}

/// The two above, for the ordinary case where a test wants a fixture scanned.
fn scanned(report: &Report) -> ScanReport {
    scan_of(document_of(report))
}

// ---------------------------------------------------------------------------
// The four properties
// ---------------------------------------------------------------------------

#[test]
fn both_package_arrays_are_read() {
    // The two ids are in different arrays, so an implementation that reads
    // either array alone fails on one of the first two assertions. A single
    // assertion here would not: with only the OS row, a projection that read
    // `osPackages` and dropped `libraries` would pass.
    let report = report_with(libraries(&["CVE-1"]), os_packages(&["CVE-2"]));
    let p = project(&scanned(&report)).expect("a fixture document projects");

    assert!(
        p.all().any(|f| f.cve.as_str() == "CVE-2"),
        "osPackages is empty for a distroless runtime; reading only libraries \
         drops every OS finding the moment the base image changes"
    );
    assert!(
        p.all().any(|f| f.cve.as_str() == "CVE-1"),
        "and the same in the other direction: libraries is the array a Go \
         project's own dependencies are reported in"
    );

    // `packageType` is in neither record — it is a fact about *which array* the
    // record was in, and this build's only chance to record it is here. Without
    // these two rows a projection that labelled every finding `library` would
    // satisfy everything above, and a base-image finding would be attributed to
    // a module upgrade that cannot fix it.
    let os = p
        .all()
        .find(|f| f.cve.as_str() == "CVE-2")
        .expect("asserted present above");
    assert_eq!(os.package_type, PackageType::Os);
    let library = p
        .all()
        .find(|f| f.cve.as_str() == "CVE-1")
        .expect("asserted present above");
    assert_eq!(library.package_type, PackageType::Library);
}

#[test]
fn an_empty_os_array_differs_from_an_absent_one() {
    // Two documents that differ in one key's presence, and the arm has to differ
    // with it: absent means the scanner did not report on OS packages at all,
    // empty means it did and found none. Only the second is evidence a base
    // image is clean.
    assert_eq!(
        project(&scanned(&report_with_os_absent()))
            .expect("a fixture document projects")
            .os_arm(),
        Arm::Absent
    );
    assert_eq!(
        project(&scanned(&report_with_os_empty()))
            .expect("a fixture document projects")
            .os_arm(),
        Arm::Empty
    );
    // The control, and it is not decoration: the two rows above are both
    // satisfied by an `os_arm` that never answers `Present`, and such an
    // implementation would report every scanned base image as unexamined.
    assert_eq!(
        project(&scanned(&report_with(
            libraries(&["CVE-1"]),
            os_packages(&["CVE-2"])
        )))
        .expect("a fixture document projects")
        .os_arm(),
        Arm::Present
    );
}

/// The advisory the duplicate fixture reports twice.
const DUPLICATED: &str = "CVE-2026-777";

/// The advisory the control below renames the *fixed* record to, so that
/// [`DUPLICATED`] is left unfixed everywhere.
const RENAMED: &str = "CVE-2026-778";

#[test]
fn a_cve_reported_both_with_and_without_a_fix_is_fixable_only() {
    // Filtering on fixedVersion puts it in BOTH lists. Subtraction is the fix.
    let document = document_of(&report_with_duplicate_cve_one_fixed_one_not(DUPLICATED));
    let p = project(&scan_of(document.clone())).expect("a fixture document projects");

    // The premise, asserted rather than assumed. If the fixture ever stopped
    // carrying the advisory twice, everything below would still pass and would
    // be about a document in which a filter and a subtraction agree.
    assert_eq!(
        p.all().filter(|f| f.cve.as_str() == DUPLICATED).count(),
        2,
        "the fixture has to report {DUPLICATED} twice, or this test is about \
         nothing"
    );

    assert!(p.fixable().any(|f| f.cve.as_str() == DUPLICATED));
    assert!(
        !p.upstream_blocked().any(|f| f.cve.as_str() == DUPLICATED),
        "subtract, never filter"
    );

    // The control. The assertion above is satisfied by an `upstream_blocked`
    // that is always empty, which is the one implementation that would make this
    // suite claim a property it does not have — so the *same* document with one
    // word changed has to put the same advisory in that list. Renaming the fixed
    // record leaves the unfixed one blocked by nothing, and nothing else about
    // the document moves.
    let mut renamed = document.clone();
    renamed["result"]["libraries"][0]["vulnerabilities"][0]["name"] =
        serde_json::Value::String(RENAMED.to_string());
    assert_ne!(
        renamed, document,
        "the mutation must reach the record that carries the fix"
    );

    let q = project(&scan_of(renamed)).expect("the mutated document projects");
    assert!(
        q.upstream_blocked().any(|f| f.cve.as_str() == DUPLICATED),
        "with no fix anywhere for {DUPLICATED}, it is upstream-blocked — this is \
         the row that stops the assertion above passing vacuously"
    );
    assert!(
        q.fixable().any(|f| f.cve.as_str() == RENAMED),
        "and the renamed record is the fixable one it was before"
    );
}

#[test]
fn no_scanner_prose_crosses_the_boundary() {
    let report = report_with_advisory_description(SENTINEL_PROSE);
    // A sentinel is only evidence if something planted it. Without this row the
    // assertion at the end holds for a fixture that never carried the prose.
    assert!(
        report.raw().contains(SENTINEL_PROSE),
        "the fixture has to put the prose in the document it is projected from"
    );

    let p = project(&scanned(&report)).expect("a fixture document projects");
    let projected: Vec<&ProjectedFinding> = p.all().collect();
    assert!(
        !projected.is_empty(),
        "an empty projection would satisfy the assertion below for the wrong \
         reason: nothing crossed the boundary because nothing was projected"
    );

    // Serialized whole and searched as text, rather than field by field: a
    // per-field check can only look at the fields somebody remembered to look
    // at, and the property is about everything the value carries.
    //
    // The rendering is `Debug` and not `serde_json`, because `ProjectedFinding`
    // has no `Serialize` — see this suite's report on the adaptation. It is the
    // stronger of the two here: `Debug` is derived over every field, including
    // the private `String` inside an `AdvisoryId`, and no serde attribute can
    // exclude anything from it.
    let serialized = format!("{projected:#?}");
    assert!(
        serialized.contains(DEFAULT_LIBRARY_CVES[0]),
        "the rendering has to hold the projected value's own fields, or its \
         silence about the prose is the silence of an empty string"
    );
    assert!(
        !serialized.contains(SENTINEL_PROSE),
        "the projection is the injection boundary; prose stays on the \
         deterministic side"
    );
}

#[test]
fn a_grade_this_build_cannot_rank_refuses_the_report_rather_than_dropping_it() {
    // Not in the bean, and the reason it is here is the signature: `project`
    // answers a `Result`, and without this row nothing would ever produce the
    // unsuccessful arm. The alternative shape — skipping a record this build
    // cannot read — is the failure `fiddle_core::finding`'s header rules out in
    // so many words: a drop taken here is a drop every later reader has to be
    // trusted to repeat, and it presents as the scanner having found nothing.
    let document = document_of(&report_with(libraries(&["CVE-1"]), os_packages(&[])));
    assert_eq!(
        project(&scan_of(document.clone()))
            .expect("a fixture document projects")
            .all()
            .count(),
        1,
        "control: the unmutated document projects the finding this one is about"
    );

    // Lower case, because that is the habit that produced the advisory-id defect
    // this milestone exists to fix — the plausible way a real scanner's spelling
    // stops matching the closed set.
    let mut lower_cased = document.clone();
    lower_cased["result"]["libraries"][0]["vulnerabilities"][0]["severity"] =
        serde_json::Value::String("high".to_string());
    assert_ne!(
        lower_cased, document,
        "the mutation must reach the grade it is about"
    );

    let refused = project(&scan_of(lower_cased))
        .expect_err("a grade this build cannot rank is refused, not dropped");
    assert!(
        refused.to_string().contains("high"),
        "the refusal has to name the grade that was not admitted, got: {refused}"
    );
}
