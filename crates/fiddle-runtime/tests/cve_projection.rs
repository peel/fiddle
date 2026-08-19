mod support;

use fiddle_core::{PackageType, ProjectedFinding, Severities, Severity};
use fiddle_runtime::cve::project::{project, Arm};
use support::cve::{
    document_of, every_fixture_grade, libraries, libraries_graded, os_packages, report_with,
    report_with_advisory_description, report_with_duplicate_cve_one_fixed_one_not,
    report_with_libraries_absent, report_with_os_absent, report_with_os_empty, scan_of, scanned,
    DEFAULT_LIBRARY_CVES, SENTINEL_PROSE,
};

#[test]
fn both_package_arrays_are_read() {
    let report = report_with(libraries(&["CVE-1"]), os_packages(&["CVE-2"]));
    let p =
        project(&scanned(&report), &every_fixture_grade()).expect("a fixture document projects");

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
    assert_eq!(
        project(&scanned(&report_with_os_absent()), &every_fixture_grade())
            .expect("a fixture document projects")
            .os_arm(),
        Arm::Absent
    );
    assert_eq!(
        project(&scanned(&report_with_os_empty()), &every_fixture_grade())
            .expect("a fixture document projects")
            .os_arm(),
        Arm::Empty
    );
    assert_eq!(
        project(
            &scanned(&report_with(libraries(&["CVE-1"]), os_packages(&["CVE-2"]))),
            &every_fixture_grade()
        )
        .expect("a fixture document projects")
        .os_arm(),
        Arm::Present
    );
}

#[test]
fn an_absent_library_array_is_reported_as_absent() {
    assert_eq!(
        project(
            &scanned(&report_with_libraries_absent()),
            &every_fixture_grade()
        )
        .expect("a fixture document projects")
        .library_arm(),
        Arm::Absent
    );
    assert_eq!(
        project(&scanned(&report_with_os_absent()), &every_fixture_grade())
            .expect("a fixture document projects")
            .library_arm(),
        Arm::Present
    );
    assert_eq!(
        project(
            &scanned(&report_with(libraries(&[]), os_packages(&[]))),
            &every_fixture_grade()
        )
        .expect("a fixture document projects")
        .library_arm(),
        Arm::Empty
    );
}

const DUPLICATED: &str = "CVE-2026-777";

const RENAMED: &str = "CVE-2026-778";

#[test]
fn a_cve_reported_both_with_and_without_a_fix_is_fixable_only() {
    let document = document_of(&report_with_duplicate_cve_one_fixed_one_not(DUPLICATED));
    let p = project(&scan_of(document.clone()), &every_fixture_grade())
        .expect("a fixture document projects");

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

    let mut renamed = document.clone();
    renamed["result"]["libraries"][0]["vulnerabilities"][0]["name"] =
        serde_json::Value::String(RENAMED.to_string());
    assert_ne!(
        renamed, document,
        "the mutation must reach the record that carries the fix"
    );

    let q =
        project(&scan_of(renamed), &every_fixture_grade()).expect("the mutated document projects");
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
    assert!(
        report.raw().contains(SENTINEL_PROSE),
        "the fixture has to put the prose in the document it is projected from"
    );

    let p =
        project(&scanned(&report), &every_fixture_grade()).expect("a fixture document projects");
    let projected: Vec<&ProjectedFinding> = p.all().collect();
    assert!(
        !projected.is_empty(),
        "an empty projection would satisfy the assertion below for the wrong \
         reason: nothing crossed the boundary because nothing was projected"
    );

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
    let document = document_of(&report_with(libraries(&["CVE-1"]), os_packages(&[])));
    assert_eq!(
        project(&scan_of(document.clone()), &every_fixture_grade())
            .expect("a fixture document projects")
            .all()
            .count(),
        1,
        "control: the unmutated document projects the finding this one is about"
    );

    let mut lower_cased = document.clone();
    lower_cased["result"]["libraries"][0]["vulnerabilities"][0]["severity"] =
        serde_json::Value::String("high".to_string());
    assert_ne!(
        lower_cased, document,
        "the mutation must reach the grade it is about"
    );

    let refused = project(&scan_of(lower_cased), &every_fixture_grade())
        .expect_err("a grade this build cannot rank is refused, not dropped");
    assert!(
        refused.to_string().contains("high"),
        "the refusal has to name the grade that was not admitted, got: {refused}"
    );
}

#[test]
fn a_deployment_that_names_a_lower_grade_projects_its_findings() {
    let report = report_with(libraries_graded(&["CVE-1"], "MEDIUM"), os_packages(&[]));

    let by_default =
        project(&scanned(&report), &every_fixture_grade()).expect("a fixture document projects");
    assert_eq!(
        by_default.all().count(),
        0,
        "a document naming no grades means HIGH and CRITICAL, so a MEDIUM \
         finding with no public exploit is not one this deployment acts on"
    );

    let acting_on_medium =
        Severities::of(&[Severity::Critical, Severity::High, Severity::Medium]).unwrap();
    let configured =
        project(&scanned(&report), &acting_on_medium).expect("a fixture document projects");
    assert_eq!(
        configured.all().count(),
        1,
        "the deployment named MEDIUM, so the MEDIUM finding is one it acts on"
    );
    assert!(
        configured.fixable().any(|f| f.cve.as_str() == "CVE-1"),
        "and it reaches the fixable set, which is what a run opens a group from"
    );
}
