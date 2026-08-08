//! What a run can point at as proof of what it did.
//!
//! A later M0 task grows this module into the whole published bundle. What
//! lives here now is the reference an assessment cites for the conclusion it
//! reached, and the two records a run keeps of what it actually executed.

/// A pointer to something a reader can go and check.
///
/// Deliberately the same opaque `<origin>:<locator>` shape as
/// [`crate::SourceRef`], and deliberately a distinct type: a source is where an
/// observation *came from*, while evidence is what a conclusion *rests on*.
/// They frequently coincide in M0 — an assessment cites the sources it read —
/// but the two roles diverge as soon as a capability produces an artefact that
/// was never observed.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct EvidenceRef(pub String);

impl std::fmt::Display for EvidenceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One capability a run actually executed, and how that execution ended.
///
/// The list of these is what makes "the capability was never executed"
/// checkable from outside the process: a run that derived `Blocked` publishes
/// an empty list, and no amount of reading the outcome alone could establish
/// that.
///
/// `status` is a free string rather than an enum because it describes the
/// execution, not the run: M0 records `completed` and `failed`, and a
/// capability that grows richer stages should not have to widen a core enum to
/// say so.
#[derive(Clone, Debug, serde::Serialize)]
pub struct CapabilityExecution {
    pub capability_id: crate::identity::CapabilityId,
    pub status: String,
    pub evidence: Vec<EvidenceRef>,
}

/// One observable stage within a capability execution.
///
/// Design §4.7 requires the published bundle to carry `progress` alongside
/// `capability_executions`: the executions say *what ran*, progress says *what
/// happened while it ran*, in the words a reader can act on. M0's single
/// capability emits exactly one entry per execution, so an empty
/// `capability_executions` implies an empty `progress`.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ProgressEntry {
    pub capability_id: crate::identity::CapabilityId,
    pub stage: String,
    pub status: String,
    pub summary: String,
    pub evidence: Vec<EvidenceRef>,
}
