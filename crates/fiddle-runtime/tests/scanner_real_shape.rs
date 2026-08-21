mod support;

use fiddle_core::{PackageType, ProjectedFinding, Severity};
use fiddle_runtime::cve::project::{project, Arm};
use fiddle_runtime::Scanner as _;
use serde_json::{json, Value};
use support::cve::{every_fixture_grade, image, scan_of, scanner_with};

const ADVISORY: &str = "CVE-2026-45001";

const PACKAGE: &str = "github.com/acme/widget/v2";

const CURRENT: &str = "2.52.13";

const FIXED: &str = "2.52.14";

const CLIENT_VERSION: &str = "1.66.0-a29c961";

const VULNERABILITY_KEYS: usize = 27;

const READ_BY_THE_PROJECTION: [&str; 4] = ["name", "severity", "fixedVersion", "hasExploit"];

const UNREAD_KEY: &str = "aFieldTheScannerHasNotShippedYet";

fn real_document() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/wiz-real/wiz.json"))
        .expect("the real scan document is JSON")
}

fn host_rows() -> Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/wiz-real/all-findings.json"
    ))
    .expect("the host's own projection of the same document is JSON")
}

fn host_projection() -> Vec<ProjectedFinding> {
    serde_json::from_value(host_rows()).expect("the host writes the six fields fiddle projects")
}

fn projected(document: Value) -> Vec<ProjectedFinding> {
    project(&scan_of(document), &every_fixture_grade())
        .expect("the real scan document projects")
        .all()
        .cloned()
        .collect()
}

#[test]
fn the_real_scan_document_projects_to_six_typed_fields() {
    let findings = projected(real_document());
    assert_eq!(
        findings.len(),
        1,
        "the real document reports one library finding: {findings:?}"
    );

    let finding = &findings[0];
    assert_eq!(
        finding.cve.as_str(),
        ADVISORY,
        "the advisory id arrives as `name`, so the six fields are a restructuring \
         of the scanner's record rather than a renaming of it"
    );
    assert_eq!(
        finding.package, PACKAGE,
        "the package name belongs to the parent library, not to the vulnerability"
    );
    assert_eq!(
        finding.current, CURRENT,
        "and so does the version the image carries"
    );
    assert_eq!(finding.fixed_version.as_deref(), Some(FIXED));
    assert_eq!(finding.severity, Severity::Medium);
    assert_eq!(finding.package_type, PackageType::Library);
}

#[test]
fn fiddle_and_the_host_project_the_same_document_the_same_way() {
    assert_eq!(
        projected(real_document()),
        host_projection(),
        "the host runs its own jq over this document and fiddle runs its own \
         projection; one image scanned twice must not produce two different \
         findings, or an operator reading both files reports a defect against \
         the difference"
    );
}

#[test]
fn the_one_real_finding_is_selected_by_its_exploit_and_not_by_its_grade() {
    let acted_on = every_fixture_grade();
    assert!(
        !acted_on.grades().any(|grade| grade == Severity::Medium),
        "the default grades are the two this build acts on, and MEDIUM is not \
         one of them"
    );
    assert_eq!(
        projected(real_document()).len(),
        1,
        "the only finding measured against a real scan is a MEDIUM with a \
         published exploit, so the exploit clause is the clause that carries it"
    );
}

#[test]
fn the_fixture_keeps_the_keys_the_projection_never_reads() {
    let document = real_document();
    let vulnerability = document["result"]["libraries"][0]["vulnerabilities"][0]
        .as_object()
        .expect("a vulnerability is an object");

    assert_eq!(
        vulnerability.len(),
        VULNERABILITY_KEYS,
        "the scanner writes {VULNERABILITY_KEYS} keys and fiddle reads four of \
         them; a fixture trimmed to the four would prove nothing about the \
         document the scanner writes"
    );
    for key in READ_BY_THE_PROJECTION {
        assert!(
            vulnerability.contains_key(key),
            "the projection reads {key} by name: {vulnerability:?}"
        );
    }
}

#[test]
fn the_scanner_document_tolerates_a_key_the_six_field_record_refuses() {
    let mut document = real_document();
    document["result"]["libraries"][0]["vulnerabilities"][0][UNREAD_KEY] = json!(true);
    assert_eq!(
        projected(document),
        host_projection(),
        "the projection reads four fields by name, so a fifth the scanner adds \
         in a later release cannot change what it produces"
    );

    let mut row = host_rows()[0].clone();
    row[UNREAD_KEY] = json!(true);
    assert!(
        serde_json::from_value::<ProjectedFinding>(row).is_err(),
        "deny_unknown_fields guards the record fiddle writes and reads back, \
         which is fiddle's own contract, and never reaches the scanner's document"
    );
}

#[test]
fn the_real_document_reports_no_os_array_at_all() {
    let projection = project(&scan_of(real_document()), &every_fixture_grade())
        .expect("the real scan document projects");

    assert_eq!(
        projection.library_arm(),
        Arm::Present,
        "the library arm is the arm a real scan has exercised"
    );
    assert_eq!(
        projection.os_arm(),
        Arm::Absent,
        "the real document reports osPackages as null rather than as an empty \
         array, so the distroless runtime the host scans reaches the absent arm; \
         no real OS finding has ever reached either fiddle's OS arm or the \
         host's"
    );
}

#[tokio::test]
async fn the_scan_reads_its_version_from_the_document_and_not_from_the_output() {
    let report = scanner_with(support::wiz_stub("real-shape"))
        .scan(&image())
        .await
        .expect("a real-shaped document is a scan that succeeded");

    assert_eq!(
        report.scanner_version, CLIENT_VERSION,
        "the scanner prints one version on its output and records another in \
         the document it wrote; the recorded one is the one that produced the \
         findings, and a version read from anywhere else can disagree with the \
         scan it is filed against"
    );
    assert_eq!(
        report.findings().len(),
        1,
        "and the document the version came from is the one fiddle projects"
    );
}
