//! One bump per target, and how far that bump is allowed to move.
//!
//! [`crate::cve::attribute`] answers *which module fixes this finding*. Two
//! things follow from its answers and neither is attribution's business.
//!
//! The first is that findings **share** targets. A repository whose vulnerable
//! modules nearly all arrive through a handful of direct requirements produces
//! one target for several findings, and treating each finding as its own change
//! would open a pull request per finding against the same line of one `go.mod` —
//! branches that conflict with each other, reviews that each have to be told the
//! others exist, and a rescan per finding of an image that only changed once.
//! [`group`] collapses them: the unit of work is the edit, not the advisory.
//!
//! The second is that a target does not say **where to move it to**. A finding
//! names the version its own advisory is fixed in; a group holds several, the
//! module has published releases none of them mention, and the change this build
//! is allowed to write is bounded. [`select_target_version`] answers that, and
//! its refusals are the needs-work verdicts a person reads.
//!
//! # The key is the target alone, and that is load-bearing
//!
//! [`Target`] and nothing else. Not the target with the [`crate::cve::attribute::Rule`]
//! that produced it, and not the target with the package the scanner named.
//! Three of the four rules end in [`Target::Module`], and a key that carried the
//! rule would split one parent bump into as many groups as there are rules that
//! reached it — one branch bumping `gh.com/parent` under rule 2 and another
//! bumping it under rule 3, both editing the same line. That is why
//! [`crate::cve::attribute::Attribution`] carries its rule *beside* the target
//! rather than inside it, and this module is the reason the decision was made
//! that way.
//!
//! The same key is why the `Dockerfile` needs no special case. Every OS finding
//! is attributed to [`Target::DockerfileBaseImage`], which is one value, so a map
//! keyed on the target collects every OS finding into one group by construction —
//! not because anything here asks what kind of finding it is looking at. A
//! `if target == DockerfileBaseImage` branch would be a second implementation of
//! something equality already does, free to disagree with it.
//!
//! # Pure, and deliberately so
//!
//! Nothing in this module reads a tree, spawns a process or asks a proxy. What
//! it groups is findings something else already placed, and what it selects from
//! is a release list something else already fetched. That is what makes both
//! answers checkable: the whole difficulty of attribution is that it depends on
//! what a module proxy says on the day, and none of *this* does.
//!
//! # What the version bound is, and which target it is about
//!
//! [`select_target_version`] is asked about a target whose own fix versions are
//! named — attribution's rules 1 and 3, where the target *is* the module the
//! scanner reported, and the `Dockerfile`, where the fix is named as a tag. A
//! rule 2 group is not that case: its target is a parent, its findings' fix
//! versions belong to the child, and the version that parent moves to was
//! already resolved by the probe that measured it — see
//! [`crate::cve::attribute`]'s viability probe, which leaves its successful bump
//! on the tree precisely so it is not resolved a second time here.

use crate::cve::attribute::Target;
use crate::cve::version;
use fiddle_core::{AdvisoryId, ProjectedFinding};
use std::collections::BTreeMap;

/// A finding and the target attribution resolved for it.
///
/// The pair, rather than [`crate::cve::attribute::Attribution`] itself, because
/// grouping needs both halves and that type is only one of them: an attribution
/// carries no advisory id and no fix version, so a group built from attributions
/// alone could name neither the CVEs its commit body lists nor the version it
/// moves to. The other direction — carrying the whole attribution — would put a
/// resolver transcript per finding inside a value whose entire point is that it
/// is *one* edit; the rule and the transcript stay with the caller, which is
/// where a report is assembled from them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attributed {
    finding: ProjectedFinding,
    target: Target,
}

impl Attributed {
    /// Pair a finding with the target that fixes it.
    pub fn new(finding: ProjectedFinding, target: Target) -> Self {
        Attributed { finding, target }
    }

    /// The finding, as it was projected from the scan.
    pub fn finding(&self) -> &ProjectedFinding {
        &self.finding
    }

    /// What editing it means.
    pub fn target(&self) -> &Target {
        &self.target
    }
}

/// One edit, and every finding it fixes.
///
/// A branch, an attempt, a commit and a rescan are all per group, so this is the
/// unit the rest of the capability counts in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Group {
    target: Target,
    findings: Vec<Attributed>,
}

impl Group {
    /// What this group edits.
    pub fn target(&self) -> &Target {
        &self.target
    }

    /// Every finding in the group, in the order they arrived.
    pub fn findings(&self) -> &[Attributed] {
        &self.findings
    }

    /// The advisories this group's edit fixes, each named once.
    ///
    /// Deduplicated, because one advisory reaching a target through two packages
    /// is one advisory: a commit body that listed it twice would be read twice by
    /// the next run's log scan, and Task 13's fold — *is every id in this group
    /// absent from the rescan* — would ask the same question of the same id
    /// twice. Order-preserving and by equality rather than through a set, since
    /// [`AdvisoryId`] deliberately has neither `Hash` nor `Ord` and a group holds
    /// a handful of findings.
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

    /// Every version this group's findings name as fixing them.
    ///
    /// Handed to [`select_target_version`] rather than reduced here, because
    /// which of them bounds the move is that function's decision and a group that
    /// pre-reduced would have taken it — with none of the fail-closed reading
    /// that goes with it. Findings whose advisory has no published fix
    /// contribute nothing, which is how a group of only those reaches
    /// [`GroupError::NoFixedVersion`].
    pub fn fixed_versions(&self) -> Vec<&str> {
        self.findings
            .iter()
            .filter_map(|finding| finding.finding().fixed_version.as_deref())
            .filter(|fixed| !fixed.is_empty())
            .collect()
    }
}

/// Collect attributed findings into one group per target.
///
/// `BTreeMap` and not a hash map, for a reason that is about evidence rather
/// than taste: the run walks these groups and produces a commit, a rescan and a
/// section of a report for each, in order. Ordered by the key, that order is a
/// function of the findings; over a hash map it would be a function of the
/// hasher's seed, and two runs of one unchanged report would produce evidence
/// nobody could diff. It is also the whole implementation of *the `Dockerfile`
/// collects its findings by construction*: `Target` is `Ord` and `Eq`, and the
/// grouping is that and nothing else.
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

/// Why a group has no version to move to.
///
/// Five ways, and they are five because each is a different sentence in a
/// verdict a person acts on: *upstream has not published a fix*, *this needs a
/// major version bump*, *nothing inside that minor carries it*. A single
/// `None`, or one variant carrying a reason string, would put that distinction
/// where nothing can be matched on it — and the rationale a needs-work verdict
/// reports is this error's own [`std::fmt::Display`], so the wording is the
/// interface.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum GroupError {
    /// No finding in the group names a version its advisory is fixed in.
    ///
    /// Ordinary rather than exceptional: an advisory with no published fix is a
    /// normal state of the world, and the honest handling is to report it.
    #[error("no finding in this group names a fixed version")]
    NoFixedVersion,

    /// A fix version this build cannot compare, so the group is left alone.
    ///
    /// The fail-closed direction [`crate::cve::version`] argues for, applied to
    /// a *set*: taking the highest of the versions that can be read would move
    /// the group to a release below an unreadable one and report its advisory
    /// fixed by a bump that does not contain the fix.
    #[error("the fixed version `{version}` cannot be compared")]
    Unreadable {
        /// The version, as whoever produced it spelled it.
        version: String,
    },

    /// The fix is in a different major, so no move inside this line reaches it.
    ///
    /// Named rather than attempted. A major is where a project is entitled to
    /// break its callers, and the migration that follows is not a version bump —
    /// it is the work this build hands back to a person, with the span it would
    /// have crossed spelled out so the ticket says what it is asking for.
    #[error("requires a major version bump from {from} to {to}")]
    MajorBump {
        /// The major the tree is on.
        from: String,
        /// The major the fix is in.
        to: String,
    },

    /// Nothing published inside the fixed minor carries the fix.
    ///
    /// Reached two ways and they are one answer: a module whose fixed minor
    /// holds no release at or above the fix, and a base image on a floating tag
    /// with no pinned tag carrying it. Both mean *there is nothing this build
    /// may move to*, and the bound that made it so is named.
    #[error("no release in {minor} carries the fix at {fixed}")]
    NoRelease {
        /// The major and minor the move was bounded to.
        minor: String,
        /// The highest version the group's findings name as fixing them.
        fixed: String,
    },

    /// The tree is already at or above the fix, so there is no move to make.
    ///
    /// Dropping such a finding is the already-fixed set's job — this is the
    /// guard for when one reaches here anyway, which a run does the moment one
    /// group's bump clears a later group's finding. Without it the selection
    /// would answer with the latest patch inside a minor the tree has already
    /// left, and write a **downgrade** presented as a security fix.
    #[error("already at {current}, which is not below the fix at {fixed}")]
    AlreadyAtTheFix {
        /// What the tree resolves the target to now.
        current: String,
        /// The highest version the group's findings name as fixing them.
        fixed: String,
    },
}

/// The version a group moves to: the latest published release inside the minor
/// its highest fix lands in.
///
/// # The three bounds, in the order they are applied
///
/// 1. **The tree is not moved backwards.** A fix at or below `current` is one
///    something else already made, and selecting anyway would downgrade.
/// 2. **A major is never crossed.** Compared against `current`, because that is
///    the only operand that says where the tree *is*; the fix's own major says
///    where it would go.
/// 3. **The minor is a ceiling and a floor.** Candidates are the releases inside
///    the fixed major and minor that carry the fix, and the answer is the latest
///    of them. A release in a higher minor is refused even when it carries the
///    fix — a change whose claim is that it is the smallest one that fixes the
///    finding may not cross a minor on its own initiative.
///
/// # Why `current` may be unreadable, and what happens then
///
/// A base image on a floating tag — `latest`, or a bare distribution codename —
/// names no major, so bounds 1 and 2 have nothing to compare and are skipped
/// rather than failed. Bound 3 still applies and is what the design means by *a
/// floating tag with no newer pinned tag is needs-work*: a floating tag is not a
/// candidate, because a tag whose contents move is not a version this build can
/// say carries anything. Nothing in this function asks what the target is; the
/// `Dockerfile` case falls out of a tag list where a module has a release list.
///
/// # Both slices are `AsRef<str>`
///
/// The fix versions arrive as `&str` borrowed from findings and the releases as
/// `String` from whatever listed them. Generic over both rather than forcing one
/// side to allocate, which for the release list — every published version of a
/// module — is the side that is long.
///
/// The chosen version is handed back **in the spelling the release list used**.
/// A proxy prints `v0.54.10` and a `go get` has to be written with the `v`; a
/// value re-normalized here would be one no command could use.
pub fn select_target_version(
    fixed_versions: &[impl AsRef<str>],
    available: &[impl AsRef<str>],
    current: &str,
) -> Result<String, GroupError> {
    let fixed = highest_fix(fixed_versions)?;
    // Read once and reused, so "the minor the fix is in" is one fact rather than
    // one per use. Unwrapped rather than handled: `highest_fix` has already
    // refused every version this cannot read, so a fix without components here
    // would mean the two readings disagree.
    let (major, minor) = major_and_minor(fixed).expect("a readable fix has components");

    // Both of these need a `current` that names a version. A floating tag names
    // none, and the honest reading is that neither bound can be applied rather
    // than that both fail — see this function's header.
    if let Some((current_major, _)) = major_and_minor(current) {
        // Before the major comparison, deliberately: a tree already past the fix
        // is that, whether or not it also happens to be in another major, and
        // "requires a major version bump" would be the wrong sentence for a
        // group that requires nothing at all.
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
        // A release this cannot read is not a candidate — which is exactly what
        // makes a floating tag one that cannot be moved to, with no rule here
        // about tags.
        let Some(components) = major_and_minor(release) else {
            continue;
        };
        // The ceiling and the floor in one comparison, and `at_least` for the
        // rest: inside the fixed minor, and at or above the fix.
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

/// The highest version a group's findings name as fixing them.
///
/// Every version is checked readable **before** any of them is compared, and an
/// unreadable one refuses the whole set rather than being skipped. That order is
/// the point: [`version::at_least`] answers `false` in both directions for a
/// version it cannot read, so a maximum taken over a set with an incomparable
/// member is not a maximum — it is whichever member the loop happened to hold
/// when it met the unreadable one, and it can be below the fix that could not be
/// read.
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

/// A version's major and minor as numbers, or nothing if it names no version.
///
/// Reads through [`version::components`] rather than splitting again here, so
/// the leading `v` a module proxy prints comes off in the one place that knows
/// about it and a version this module accepts is one [`version::at_least`] can
/// also compare. A missing minor is `0`, which is the same reading `at_least`
/// gives it — `v3` and `3.0` are one line.
fn major_and_minor(version: &str) -> Option<(u64, u64)> {
    let components = version::components(version)?;
    Some((
        *components.first()?,
        components.get(1).copied().unwrap_or(0),
    ))
}
