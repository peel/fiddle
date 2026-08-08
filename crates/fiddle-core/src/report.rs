//! What a run can point at as proof of what it did.
//!
//! Later M0 tasks grow this module into the whole published bundle. What lives
//! here now is the one type an assessment already needs: the reference it cites
//! for the conclusion it reached.

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
