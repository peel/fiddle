//! Turning a scanner's document into the findings this build acts on.
//!
//! [`fiddle_core::finding`] states what a finding is allowed to be — six fields,
//! and a seventh refused rather than dropped. This module is the only place that
//! *produces* one, and it is therefore where that contract is either kept or
//! quietly lost.
//!
//! # Nothing here deserializes a scanner record
//!
//! A real vulnerability record carries dozens of keys, most of them prose an
//! advisory's author wrote: a description, remediation text, links. Nothing can
//! deserialize such a record into a [`ProjectedFinding`], because that type
//! declares `deny_unknown_fields` over six names — and that refusal *is* the
//! injection boundary rather than an inconvenience on the way to one.
//!
//! So [`record`] reads six values out of the document **by name** and builds the
//! typed value from them. There is deliberately no intermediate struct mirroring
//! the scanner's shape: a permissive one would be a second, laxer boundary a few
//! lines above the strict one, and every field it carried would be a field
//! somebody later has to remember not to pass on. The six reads below are the
//! whole of what crosses, and `description` is not among them because there is
//! no line here that could name it.
//!
//! # Absent is not empty
//!
//! [`packages`] answers an [`Arm`] beside the array it read, because *the
//! scanner did not report on OS packages* and *the scanner reported on OS
//! packages and found none* are different facts about the world and only the
//! second is evidence that a base image is clean. A distroless runtime
//! legitimately has an empty `osPackages`, which is what makes the collapse so
//! easy to ship: it looks right until a base image changes, and then it drops
//! every OS finding without saying so. [`crate::scanner::ScanReport`] keeps the
//! document verbatim precisely so this distinction still exists by the time it
//! reaches here.
//!
//! # Subtraction, not a filter
//!
//! One advisory can be reported twice, against two packages, once with a fix and
//! once without — the same CVE in a module a project depends on directly and in
//! one it gets through a parent. Splitting the two lists by *filtering* on
//! `fixedVersion` puts such an advisory in both, and the upstream-blocked list is
//! read as "there is nothing we can do about these": an advisory that is fixable
//! somewhere does not belong in it. So [`project`] computes the fixable set first
//! and takes the blocked set as the unfixed records **minus** the advisories that
//! are fixable anywhere.
//!
//! # What this refuses
//!
//! A record whose grade this build cannot rank, whose advisory names nothing, or
//! whose package has no name is a document defect, and [`project`] answers
//! [`ProjectionError`] for it rather than skipping the record. Skipping is the
//! failure `fiddle_core::finding`'s header rules out in so many words: a drop
//! taken here is a drop every later reader has to be trusted to repeat, and it
//! presents to an operator as the scan having found nothing.

use fiddle_core::{selected, AdvisoryId, PackageType, ProjectedFinding, Severity};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

use crate::scanner::ScanReport;

/// What a scanner said about one of the two package arrays.
///
/// Three answers rather than a `bool` or an emptiness test on the array,
/// because the two unsuccessful-looking ones are not the same claim — see this
/// module's header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Arm {
    /// The key is not in the document, or is `null`: the scanner made no claim
    /// about this half of the image at all.
    Absent,
    /// The key is there and holds no packages: the scanner looked and reported
    /// none.
    Empty,
    /// The scanner reported packages. Whether any of their findings survive
    /// selection is a different question, and one the projection answers.
    Present,
}

/// Every finding one scan produced, split the two ways this capability acts on.
///
/// The three views are *indices* into a single list rather than three lists of
/// their own. Cloning a finding into each would make it possible for the same
/// advisory to be described differently depending on which accessor a caller
/// reached for, and that divergence is the kind nothing fails on — it just
/// renders one thing into a pull request body and another into a report.
#[derive(Debug)]
pub struct Projection {
    findings: Vec<ProjectedFinding>,
    fixable: Vec<usize>,
    upstream_blocked: Vec<usize>,
    os_arm: Arm,
}

impl Projection {
    /// Every selected finding, from both package arrays, in document order.
    pub fn all(&self) -> impl Iterator<Item = &ProjectedFinding> + '_ {
        self.findings.iter()
    }

    /// The findings naming a version an upgrade could be written to.
    pub fn fixable(&self) -> impl Iterator<Item = &ProjectedFinding> + '_ {
        self.fixable.iter().map(|&at| &self.findings[at])
    }

    /// The findings with no fix anywhere in this report.
    ///
    /// Not "the findings without a `fixedVersion`" — see the header on why the
    /// difference between those two is the point of this module.
    pub fn upstream_blocked(&self) -> impl Iterator<Item = &ProjectedFinding> + '_ {
        self.upstream_blocked.iter().map(|&at| &self.findings[at])
    }

    /// What the scanner said about `osPackages`.
    ///
    /// Exposed for the OS array and not for `libraries`, because they are not
    /// symmetric in practice: a Go project always declares dependencies, so an
    /// absent `libraries` is a broken scan, while an empty `osPackages` is the
    /// ordinary state of a distroless runtime and is the answer a caller has to
    /// be able to tell from silence. A caller that needs the same distinction
    /// for `libraries` should expose it here in the change that needs it —
    /// [`packages`] already computes it.
    pub fn os_arm(&self) -> Arm {
        self.os_arm
    }
}

/// Why a document produced no projection.
///
/// Both variants are the document being a shape this build does not read, and
/// they are separate because they are found in different places and fixed by
/// different people: one is a package array that is not an array, the other is a
/// single record inside one. The fields are diagnostics; what discriminates an
/// arm is the variant.
#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    /// `result.<array>` is present and is not a list of packages.
    #[error("{array} is present in the report and is not an array of packages")]
    NotAnArray { array: &'static str },

    /// A package, or a vulnerability inside one, is not a record this build can
    /// project.
    ///
    /// Positional rather than named, because the names are exactly what may be
    /// missing: a record refused for having no package name cannot be reported
    /// by its package name.
    #[error("{array}[{package}] vulnerability {vulnerability}: {reason}")]
    Unreadable {
        array: &'static str,
        package: usize,
        vulnerability: usize,
        reason: String,
    },
}

/// The findings `report` carries, selected and split.
///
/// Both package arrays are read. Selection is [`selected`] and nothing else, so
/// the rule lives in the pure crate where it is argued for rather than being
/// restated here in terms this file could get wrong.
pub fn project(report: &ScanReport) -> Result<Projection, ProjectionError> {
    // The library arm is computed and not kept — see [`Projection::os_arm`] for
    // why only one of the two is exposed. It goes through the same reader so
    // that a `libraries` key of the wrong shape is refused rather than read as
    // no libraries at all.
    let (_, library_packages) = packages(&report.document, "libraries")?;
    let (os_arm, os_packages) = packages(&report.document, "osPackages")?;

    let mut findings = Vec::new();
    // The two arrays differ in exactly one thing this build cares about: the
    // `packageType` a record inherits from the array it was in. Iterating over
    // the pair rather than calling a reader twice is what keeps that the only
    // difference — two call sites would be free to grow a second one.
    for (array, from, package_type) in [
        ("libraries", library_packages, PackageType::Library),
        ("osPackages", os_packages, PackageType::Os),
    ] {
        select_into(array, from, package_type, &mut findings)?;
    }

    let fixable: Vec<usize> = findings
        .iter()
        .enumerate()
        .filter(|(_, finding)| names_a_fix(finding))
        .map(|(at, _)| at)
        .collect();

    // Scoped, so the borrow of `findings` this set holds is over before the
    // findings are moved into the value below — and so that the order is
    // structural rather than conventional: the blocked set cannot be computed
    // until the fixable one exists.
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
        os_arm,
    })
}

/// One package array, and what the scanner claimed by writing it.
///
/// A missing key and an explicit `null` are one answer: both are the scanner
/// declining to report on that half of the image. Anything that is neither
/// absent nor an array is a document this build cannot read, and is refused
/// rather than treated as absent — treating it as absent would turn a malformed
/// report into a clean one.
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

/// Project every selected finding in one package array onto `into`.
fn select_into(
    array: &'static str,
    from: &[Value],
    package_type: PackageType,
    into: &mut Vec<ProjectedFinding>,
) -> Result<(), ProjectionError> {
    for (at, package) in from.iter().enumerate() {
        let refuse = |vulnerability: usize, reason: String| ProjectionError::Unreadable {
            array,
            package: at,
            vulnerability,
            reason,
        };

        // The package's own two fields. A finding whose package has no name or
        // no version names nothing anybody could upgrade, so it is refused here
        // rather than projected with a placeholder.
        let name = package["name"]
            .as_str()
            .ok_or_else(|| refuse(0, "the package has no name".to_string()))?;
        let current = package["version"]
            .as_str()
            .ok_or_else(|| refuse(0, format!("the package {name} has no version")))?;

        // A package with no `vulnerabilities` key reported no vulnerabilities.
        // That is not a drop: there is no finding to lose. A key holding
        // something other than an array is a different matter and is refused.
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
            if let Some(finding) = record(vulnerability, name, current, package_type)
                .map_err(|reason| refuse(nth, reason))?
            {
                into.push(finding);
            }
        }
    }
    Ok(())
}

/// One vulnerability record, projected — or nothing, if selection declines it.
///
/// This is the boundary, and it is six reads wide. Each names a key; nothing
/// iterates the record, and nothing deserializes it as a whole. A key the
/// scanner writes and this build has no line for — `description`, `remediation`,
/// `link` — cannot reach the returned value, because there is no expression here
/// that could carry it.
fn record(
    vulnerability: &Value,
    package: &str,
    current: &str,
    package_type: PackageType,
) -> Result<Option<ProjectedFinding>, String> {
    // Through `AdvisoryId`'s own `Deserialize`, so the canonical spelling and
    // the refusal of a blank id are the ones the pure crate defines. Reading the
    // string and upper-casing it here would be a second normalization to drift
    // from the first.
    let cve = AdvisoryId::deserialize(&vulnerability["name"])
        .map_err(|reason| format!("the advisory id is unreadable: {reason}"))?;
    // Likewise the closed grade set. A grade this build cannot rank refuses the
    // report; it does not sort itself into "not selected", which would be the
    // silent half of the same decision.
    let severity = Severity::deserialize(&vulnerability["severity"])
        .map_err(|reason| format!("{cve:?} carries a grade this build cannot rank: {reason}"))?;

    // Absent and `null` both mean the scanner reported no public exploit, which
    // is what every reference document says explicitly. A value that is neither
    // absent nor a boolean is a defect, and defaulting it to `false` would
    // silently decline every finding on the exploit arm.
    let has_exploit = match vulnerability.get("hasExploit") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(known)) => *known,
        Some(other) => return Err(format!("{cve:?} reports hasExploit as {other}")),
    };

    // The three spellings of "no fix published yet" that `ProjectedFinding`
    // documents: absent, `null`, and — through `names_a_fix` and `selected` — an
    // empty string.
    let fixed_version = match vulnerability.get("fixedVersion") {
        None | Some(Value::Null) => None,
        Some(Value::String(version)) => Some(version.clone()),
        Some(other) => return Err(format!("{cve:?} reports fixedVersion as {other}")),
    };

    if !selected(severity, has_exploit, fixed_version.as_deref()) {
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

/// Does this finding name a version an upgrade could be written to?
///
/// The same reading of a blank version that [`selected`] gives it, and for the
/// same reason: on either side, a `fixedVersion` of `""` names no release, and
/// an upgrade written to it upgrades a package to nothing.
fn names_a_fix(finding: &ProjectedFinding) -> bool {
    finding
        .fixed_version
        .as_deref()
        .is_some_and(|version| !version.trim().is_empty())
}
