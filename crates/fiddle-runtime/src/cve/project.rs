use fiddle_core::{selected, AdvisoryId, PackageType, ProjectedFinding, Severities, Severity};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

use crate::scanner::ScanReport;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Arm {
    Absent,
    Empty,
    Present,
}

#[derive(Debug)]
pub struct Projection {
    findings: Vec<ProjectedFinding>,
    fixable: Vec<usize>,
    upstream_blocked: Vec<usize>,
    library_arm: Arm,
    os_arm: Arm,
}

impl Projection {
    pub fn all(&self) -> impl Iterator<Item = &ProjectedFinding> + '_ {
        self.findings.iter()
    }

    pub fn fixable(&self) -> impl Iterator<Item = &ProjectedFinding> + '_ {
        self.fixable.iter().map(|&at| &self.findings[at])
    }

    pub fn upstream_blocked(&self) -> impl Iterator<Item = &ProjectedFinding> + '_ {
        self.upstream_blocked.iter().map(|&at| &self.findings[at])
    }

    pub fn os_arm(&self) -> Arm {
        self.os_arm
    }

    pub fn library_arm(&self) -> Arm {
        self.library_arm
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("{array} is present in the report and is not an array of packages")]
    NotAnArray { array: &'static str },

    #[error("{array}[{package}] vulnerability {vulnerability}: {reason}")]
    Unreadable {
        array: &'static str,
        package: usize,
        vulnerability: usize,
        reason: String,
    },
}

pub fn project(report: &ScanReport, acted_on: &Severities) -> Result<Projection, ProjectionError> {
    let (library_arm, library_packages) = packages(&report.document, "libraries")?;
    let (os_arm, os_packages) = packages(&report.document, "osPackages")?;

    let mut findings = Vec::new();
    for (array, from, package_type) in [
        ("libraries", library_packages, PackageType::Library),
        ("osPackages", os_packages, PackageType::Os),
    ] {
        select_into(array, from, package_type, acted_on, &mut findings)?;
    }

    let fixable: Vec<usize> = findings
        .iter()
        .enumerate()
        .filter(|(_, finding)| names_a_fix(finding))
        .map(|(at, _)| at)
        .collect();

    let upstream_blocked: Vec<usize> = {
        let fixable_advisories: HashSet<&str> = fixable
            .iter()
            .map(|&at| findings[at].cve.as_str())
            .collect();
        findings
            .iter()
            .enumerate()
            .filter(|(_, finding)| {
                !names_a_fix(finding) && !fixable_advisories.contains(finding.cve.as_str())
            })
            .map(|(at, _)| at)
            .collect()
    };

    Ok(Projection {
        findings,
        fixable,
        upstream_blocked,
        library_arm,
        os_arm,
    })
}

fn packages<'a>(
    document: &'a Value,
    array: &'static str,
) -> Result<(Arm, &'a [Value]), ProjectionError> {
    match document["result"].get(array) {
        None | Some(Value::Null) => Ok((Arm::Absent, &[])),
        Some(Value::Array(packages)) if packages.is_empty() => Ok((Arm::Empty, packages)),
        Some(Value::Array(packages)) => Ok((Arm::Present, packages)),
        Some(_) => Err(ProjectionError::NotAnArray { array }),
    }
}

fn select_into(
    array: &'static str,
    from: &[Value],
    package_type: PackageType,
    acted_on: &Severities,
    into: &mut Vec<ProjectedFinding>,
) -> Result<(), ProjectionError> {
    for (at, package) in from.iter().enumerate() {
        let refuse = |vulnerability: usize, reason: String| ProjectionError::Unreadable {
            array,
            package: at,
            vulnerability,
            reason,
        };

        let name = package["name"]
            .as_str()
            .ok_or_else(|| refuse(0, "the package has no name".to_string()))?;
        let current = package["version"]
            .as_str()
            .ok_or_else(|| refuse(0, format!("the package {name} has no version")))?;

        let vulnerabilities = match package.get("vulnerabilities") {
            None | Some(Value::Null) => continue,
            Some(Value::Array(vulnerabilities)) => vulnerabilities,
            Some(_) => {
                return Err(refuse(
                    0,
                    format!("{name} reports vulnerabilities that are not an array"),
                ))
            }
        };

        for (nth, vulnerability) in vulnerabilities.iter().enumerate() {
            if let Some(finding) = record(vulnerability, name, current, package_type, acted_on)
                .map_err(|reason| refuse(nth, reason))?
            {
                into.push(finding);
            }
        }
    }
    Ok(())
}

fn record(
    vulnerability: &Value,
    package: &str,
    current: &str,
    package_type: PackageType,
    acted_on: &Severities,
) -> Result<Option<ProjectedFinding>, String> {
    let cve = AdvisoryId::deserialize(&vulnerability["name"])
        .map_err(|reason| format!("the advisory id is unreadable: {reason}"))?;
    let severity = Severity::deserialize(&vulnerability["severity"])
        .map_err(|reason| format!("{cve:?} carries a grade this build cannot rank: {reason}"))?;

    let has_exploit = match vulnerability.get("hasExploit") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(known)) => *known,
        Some(other) => return Err(format!("{cve:?} reports hasExploit as {other}")),
    };

    let fixed_version = match vulnerability.get("fixedVersion") {
        None | Some(Value::Null) => None,
        Some(Value::String(version)) => Some(version.clone()),
        Some(other) => return Err(format!("{cve:?} reports fixedVersion as {other}")),
    };

    if !selected(acted_on, severity, has_exploit, fixed_version.as_deref()) {
        return Ok(None);
    }

    Ok(Some(ProjectedFinding {
        cve,
        package: package.to_string(),
        current: current.to_string(),
        fixed_version,
        severity,
        package_type,
    }))
}

fn names_a_fix(finding: &ProjectedFinding) -> bool {
    finding
        .fixed_version
        .as_deref()
        .is_some_and(|version| !version.trim().is_empty())
}
