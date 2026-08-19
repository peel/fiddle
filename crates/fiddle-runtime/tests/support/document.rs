pub const SENTINEL_PROSE: &str = "fiddle-prose-c47a06f9";

const LIBRARY_PACKAGES: [(&str, &str, &str); 3] = [
    ("golang.org/x/crypto", "v0.31.0", "v0.35.0"),
    ("golang.org/x/net", "v0.24.0", "v0.28.0"),
    ("github.com/docker/docker", "v24.0.7", "v24.0.9"),
];

const OS_PACKAGES: [(&str, &str, &str); 3] = [
    ("libssl3", "3.0.11-r0", "3.0.12-r0"),
    ("busybox", "1.36.1-r5", "1.36.1-r7"),
    ("zlib", "1.3-r0", "1.3.1-r0"),
];

const BENIGN_DESCRIPTION: &str = "a benign advisory summary";

pub const DEFAULT_LIBRARY_CVES: [&str; 1] = ["CVE-2026-0001"];

pub const DEFAULT_OS_CVES: [&str; 1] = ["CVE-2026-0002"];

pub const SECOND_OS_CVE: &str = "CVE-2026-0005";

pub const SECOND_LIBRARY_CVE: &str = "CVE-2026-0003";

#[derive(Debug, Clone)]
struct Package {
    name: String,
    version: String,
    vulnerabilities: Vec<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct Libraries(Vec<Package>);

#[derive(Debug, Clone)]
pub struct OsPackages(Vec<Package>);

pub fn libraries(cves: &[&str]) -> Libraries {
    Libraries(packages(cves, &LIBRARY_PACKAGES))
}

pub fn unfixed_libraries(cves: &[&str]) -> Libraries {
    Libraries(unfixed_packages(cves, &LIBRARY_PACKAGES))
}

pub fn libraries_graded(cves: &[&str], grade: &str) -> Libraries {
    Libraries(packages_graded(cves, &LIBRARY_PACKAGES, grade))
}

pub fn os_packages(cves: &[&str]) -> OsPackages {
    OsPackages(packages(cves, &OS_PACKAGES))
}

fn packages(cves: &[&str], table: &[(&str, &str, &str); 3]) -> Vec<Package> {
    packages_graded(cves, table, FIXTURE_GRADE)
}

fn packages_graded(cves: &[&str], table: &[(&str, &str, &str); 3], grade: &str) -> Vec<Package> {
    cves.iter()
        .enumerate()
        .map(|(at, cve)| {
            let (name, current, fixed) = table[at % table.len()];
            Package {
                name: name.to_string(),
                version: current.to_string(),
                vulnerabilities: vec![graded(cve, Some(fixed), BENIGN_DESCRIPTION, grade)],
            }
        })
        .collect()
}

fn unfixed_packages(cves: &[&str], table: &[(&str, &str, &str); 3]) -> Vec<Package> {
    cves.iter()
        .enumerate()
        .map(|(at, cve)| {
            let (name, current, _fixed) = table[at % table.len()];
            Package {
                name: name.to_string(),
                version: current.to_string(),
                vulnerabilities: vec![vulnerability(cve, None, BENIGN_DESCRIPTION)],
            }
        })
        .collect()
}

pub const FIXTURE_GRADE: &str = "HIGH";

fn vulnerability(cve: &str, fixed: Option<&str>, description: &str) -> serde_json::Value {
    graded(cve, fixed, description, FIXTURE_GRADE)
}

fn graded(cve: &str, fixed: Option<&str>, description: &str, severity: &str) -> serde_json::Value {
    let mut value = serde_json::json!({
        "name": cve,
        "severity": severity,
        "hasExploit": false,
        "description": description,
    });
    if let Some(fixed) = fixed {
        value["fixedVersion"] = serde_json::Value::String(fixed.to_string());
    }
    value
}

fn as_json(packages: &[Package]) -> serde_json::Value {
    serde_json::Value::Array(
        packages
            .iter()
            .map(|package| {
                serde_json::json!({
                    "name": package.name,
                    "version": package.version,
                    "vulnerabilities": package.vulnerabilities,
                })
            })
            .collect(),
    )
}

#[derive(Debug, Clone)]
pub enum ReportVariant {
    Plain(Libraries, OsPackages),
    OsAbsent,
    OsEmpty,
    LibrariesAbsent,
    DuplicateCve(String),
    AdvisoryDescription(String),
}

const REPORT_VARIANTS: usize = 6;

impl ReportVariant {
    pub fn index(&self) -> usize {
        match self {
            ReportVariant::Plain(_, _) => 0,
            ReportVariant::OsAbsent => 1,
            ReportVariant::OsEmpty => 2,
            ReportVariant::DuplicateCve(_) => 3,
            ReportVariant::AdvisoryDescription(_) => 4,
            ReportVariant::LibrariesAbsent => 5,
        }
    }

    pub fn label(&self) -> String {
        match self {
            ReportVariant::Plain(Libraries(l), OsPackages(o)) => {
                format!("plain({} libraries, {} os packages)", l.len(), o.len())
            }
            ReportVariant::OsAbsent => "os-absent".to_string(),
            ReportVariant::OsEmpty => "os-empty".to_string(),
            ReportVariant::DuplicateCve(cve) => format!("duplicate({cve})"),
            ReportVariant::AdvisoryDescription(_) => "advisory-description".to_string(),
            ReportVariant::LibrariesAbsent => "libraries-absent".to_string(),
        }
    }

    fn render(&self) -> Report {
        let mut result = serde_json::Map::new();
        match self {
            ReportVariant::Plain(Libraries(l), OsPackages(o)) => {
                result.insert("libraries".to_string(), as_json(l));
                result.insert("osPackages".to_string(), as_json(o));
            }
            ReportVariant::OsAbsent => {
                result.insert(
                    "libraries".to_string(),
                    as_json(&packages(&DEFAULT_LIBRARY_CVES, &LIBRARY_PACKAGES)),
                );
            }
            ReportVariant::OsEmpty => {
                result.insert(
                    "libraries".to_string(),
                    as_json(&packages(&DEFAULT_LIBRARY_CVES, &LIBRARY_PACKAGES)),
                );
                result.insert("osPackages".to_string(), serde_json::json!([]));
            }
            ReportVariant::LibrariesAbsent => {
                result.insert(
                    "osPackages".to_string(),
                    as_json(&packages(&DEFAULT_OS_CVES, &OS_PACKAGES)),
                );
            }
            ReportVariant::DuplicateCve(cve) => {
                let (fixable_name, fixable_version, fixed) = LIBRARY_PACKAGES[0];
                let (blocked_name, blocked_version, _) = LIBRARY_PACKAGES[1];
                result.insert(
                    "libraries".to_string(),
                    as_json(&[
                        Package {
                            name: fixable_name.to_string(),
                            version: fixable_version.to_string(),
                            vulnerabilities: vec![vulnerability(
                                cve,
                                Some(fixed),
                                BENIGN_DESCRIPTION,
                            )],
                        },
                        Package {
                            name: blocked_name.to_string(),
                            version: blocked_version.to_string(),
                            vulnerabilities: vec![vulnerability(cve, None, BENIGN_DESCRIPTION)],
                        },
                    ]),
                );
                result.insert("osPackages".to_string(), serde_json::json!([]));
            }
            ReportVariant::AdvisoryDescription(prose) => {
                let (name, version, fixed) = LIBRARY_PACKAGES[0];
                result.insert(
                    "libraries".to_string(),
                    as_json(&[Package {
                        name: name.to_string(),
                        version: version.to_string(),
                        vulnerabilities: vec![vulnerability(
                            DEFAULT_LIBRARY_CVES[0],
                            Some(fixed),
                            prose,
                        )],
                    }]),
                );
                result.insert("osPackages".to_string(), serde_json::json!([]));
            }
        }
        Report {
            raw: serde_json::to_string_pretty(&serde_json::json!({ "result": result }))
                .expect("a document built from json! values serializes"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Report {
    raw: String,
}

impl Report {
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

pub fn report_with(libraries: Libraries, os_packages: OsPackages) -> Report {
    ReportVariant::Plain(libraries, os_packages).render()
}

pub fn report_with_os_absent() -> Report {
    ReportVariant::OsAbsent.render()
}

pub fn report_with_os_empty() -> Report {
    ReportVariant::OsEmpty.render()
}

pub fn report_with_libraries_absent() -> Report {
    ReportVariant::LibrariesAbsent.render()
}

pub fn report_with_duplicate_cve_one_fixed_one_not(cve: &str) -> Report {
    ReportVariant::DuplicateCve(cve.to_string()).render()
}

pub fn report_with_advisory_description(text: &str) -> Report {
    ReportVariant::AdvisoryDescription(text.to_string()).render()
}

pub fn canonical_reports() -> [ReportVariant; REPORT_VARIANTS] {
    [
        ReportVariant::Plain(
            libraries(&DEFAULT_LIBRARY_CVES),
            os_packages(&DEFAULT_OS_CVES),
        ),
        ReportVariant::OsAbsent,
        ReportVariant::OsEmpty,
        ReportVariant::DuplicateCve("CVE-2026-0777".to_string()),
        ReportVariant::AdvisoryDescription(SENTINEL_PROSE.to_string()),
        ReportVariant::LibrariesAbsent,
    ]
}

pub fn distinct_reports() -> Vec<(String, Report)> {
    let mut variants: Vec<ReportVariant> = canonical_reports().into_iter().collect();
    variants.push(ReportVariant::Plain(
        libraries(&["CVE-1"]),
        os_packages(&[]),
    ));
    variants.push(ReportVariant::Plain(
        libraries(&[]),
        os_packages(&["CVE-1"]),
    ));
    variants
        .into_iter()
        .map(|variant| (variant.label(), variant.render()))
        .collect()
}
