use crate::cve::attribute::Target;
use crate::cve::version;
use fiddle_core::{AdvisoryId, ProjectedFinding};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attributed {
    finding: ProjectedFinding,
    target: Target,
}

impl Attributed {
    pub fn new(finding: ProjectedFinding, target: Target) -> Self {
        Attributed { finding, target }
    }

    pub fn finding(&self) -> &ProjectedFinding {
        &self.finding
    }

    pub fn target(&self) -> &Target {
        &self.target
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Group {
    target: Target,
    findings: Vec<Attributed>,
}

impl Group {
    pub fn target(&self) -> &Target {
        &self.target
    }

    pub fn findings(&self) -> &[Attributed] {
        &self.findings
    }

    pub fn cves(&self) -> Vec<&AdvisoryId> {
        let mut cves: Vec<&AdvisoryId> = Vec::new();
        for finding in &self.findings {
            let cve = &finding.finding().cve;
            if !cves.contains(&cve) {
                cves.push(cve);
            }
        }
        cves
    }

    pub fn fixed_versions(&self) -> Vec<&str> {
        self.findings
            .iter()
            .filter_map(|finding| finding.finding().fixed_version.as_deref())
            .filter(|fixed| !fixed.is_empty())
            .collect()
    }
}

pub fn group(attributed: &[Attributed]) -> Vec<Group> {
    let mut by_target: BTreeMap<&Target, Vec<Attributed>> = BTreeMap::new();
    for finding in attributed {
        by_target
            .entry(finding.target())
            .or_default()
            .push(finding.clone());
    }
    by_target
        .into_iter()
        .map(|(target, findings)| Group {
            target: target.clone(),
            findings,
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum GroupError {
    #[error("no finding in this group names a fixed version")]
    NoFixedVersion,

    #[error("the fixed version `{version}` cannot be compared")]
    Unreadable { version: String },

    #[error("requires a major version bump from {from} to {to}")]
    MajorBump { from: String, to: String },

    #[error("no release in {minor} carries the fix at {fixed}")]
    NoRelease { minor: String, fixed: String },

    #[error("this build could not select a move: {why}")]
    Unselectable { why: String },

    #[error("already at {current}, which is not below the fix at {fixed}")]
    AlreadyAtTheFix { current: String, fixed: String },
}

pub fn select_target_version(
    fixed_versions: &[impl AsRef<str>],
    available: &[impl AsRef<str>],
    current: &str,
) -> Result<String, GroupError> {
    let fixed = highest_fix(fixed_versions)?;
    let (major, minor) = major_and_minor(fixed).expect("a readable fix has components");

    if let Some((current_major, _)) = major_and_minor(current) {
        if version::at_least(current, fixed) {
            return Err(GroupError::AlreadyAtTheFix {
                current: current.to_string(),
                fixed: fixed.to_string(),
            });
        }
        if current_major != major {
            return Err(GroupError::MajorBump {
                from: current_major.to_string(),
                to: major.to_string(),
            });
        }
    }

    let mut best: Option<&str> = None;
    for release in available {
        let release = release.as_ref();
        let Some(components) = major_and_minor(release) else {
            continue;
        };
        if components != (major, minor) || !version::at_least(release, fixed) {
            continue;
        }
        if best.is_none_or(|best| version::at_least(release, best)) {
            best = Some(release);
        }
    }

    best.map(str::to_string).ok_or(GroupError::NoRelease {
        minor: format!("{major}.{minor}"),
        fixed: fixed.to_string(),
    })
}

fn highest_fix(fixed_versions: &[impl AsRef<str>]) -> Result<&str, GroupError> {
    let mut highest: Option<&str> = None;
    for fixed in fixed_versions {
        let fixed = fixed.as_ref();
        if version::components(fixed).is_none() {
            return Err(GroupError::Unreadable {
                version: fixed.to_string(),
            });
        }
        if highest.is_none_or(|highest| version::at_least(fixed, highest)) {
            highest = Some(fixed);
        }
    }
    highest.ok_or(GroupError::NoFixedVersion)
}

fn major_and_minor(version: &str) -> Option<(u64, u64)> {
    let components = version::components(version)?;
    Some((
        *components.first()?,
        components.get(1).copied().unwrap_or(0),
    ))
}
