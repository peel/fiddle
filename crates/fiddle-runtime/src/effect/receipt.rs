//! What an effect leaves behind: what was observed, and what was refused.
//!
//! A receipt is deliberately a record of an *observation* rather than of a
//! response. Its `postcondition` and `external_ref` are read back out of the
//! world after the mutation, because the case this whole milestone exists for is
//! the one where the response never arrived — and a receipt assembled from a
//! response would have nothing to say about it.
//!
//! The failures live here beside it for the same reason the outcome does: the
//! interesting thing about an effect that did not happen is *which kind* of not
//! happening it was, and that is a single taxonomy whether the answer is a
//! receipt or an error.

use crate::github::GhError;
use fiddle_core::{EffectId, EffectKind, PayloadHash};

/// Something the world was observed to contain.
///
/// Three methods rather than one, because a receipt needs three different
/// things from an observation and they are not interchangeable: a sentence a
/// person reads, an external reference a later run can look the object up by,
/// and the typed value the calling capability actually wanted.
///
/// `into_value` takes `self` so the typed value is moved out rather than
/// cloned, which is what lets an observation carry something a capability
/// consumes.
pub trait ObservedState {
    /// The typed result the proposing capability asked for.
    type Value;

    /// What is true out there, in terms a person reading a bundle understands.
    fn describe(&self) -> String;

    /// The external revision or reference: a commit sha, a PR number, a run id.
    /// `None` when the object has no stable external name.
    fn reference(&self) -> Option<String>;

    /// The typed value, consuming the observation.
    fn into_value(self) -> Self::Value;
}

/// The verified result of one effect.
///
/// **A receipt is not serialized, and `Serialize` is inert here.** It reaches a
/// published bundle as a *rendered* [`EvidenceRef`](fiddle_core::EvidenceRef) —
/// see `receipt_evidence` in `crate::capability::publish`, which states the
/// case: the bundle's evidence is a list of strings and giving a
/// structured receipt a home in the report schema would widen a published
/// contract. This comment used to claim the opposite, and the two could not both
/// be right.
///
/// The derive is unexercised rather than merely unused, and by more than
/// omission: none of the three `T`s a *run* instantiates it with —
/// `PublishedBranch`, `PullRequest`, `WorkflowRun` — is itself `Serialize`, so
/// the derived bound is unsatisfiable for every receipt production can produce.
/// It stays because the epic's `## Contracts` pins the derive;
/// `docs/BACKLOG.md` records removing it.
///
/// There is no `Deserialize`, and that part was always true: for the same reason
/// [`fiddle_core::PolicyDecision`] has none, nothing reads a receipt back in as
/// an authority. A later run re-derives the identity from canonical inputs and
/// looks at the world again.
#[derive(Clone, Debug, serde::Serialize)]
pub struct EffectReceipt<T> {
    pub effect_id: EffectId,
    pub payload_hash: PayloadHash,
    pub target: String,
    /// Always [`EffectOutcome::Committed`](super::EffectOutcome::Committed) on
    /// every path that builds one: a receipt is the *observed* result, and both
    /// construction sites in [`super`] reach it from a postcondition that was
    /// read back. The other two outcomes leave as an [`EffectError`] instead, and
    /// the field is still the three-valued type because that vocabulary is what a
    /// bundle consumer matches on.
    pub outcome: super::EffectOutcome,
    /// What was observed to be true after the operation, read back rather than
    /// assumed from the response.
    pub postcondition: String,
    /// The external revision or reference: a commit sha, a PR number, a run id.
    pub external_ref: Option<String>,
    pub value: T,
}

/// Every way an effect can fail to produce a receipt.
///
/// Each variant carries the [`EffectKind`] because a refusal reaches a person
/// long after the run, and "denied" with no antecedent sends its reader back to
/// the configuration to guess — the same argument [`fiddle_core::PolicyDecision`]
/// makes for carrying a reason.
///
/// [`EffectError::Unresolved`] is the variant that is worth the type. It is not
/// "it failed"; it is "nobody knows, and the read that was supposed to settle it
/// did not". Collapsing it into [`EffectError::Adapter`] would tell a caller a
/// write failed when it may well have landed, and the retry would perform it
/// twice.
#[derive(Debug, thiserror::Error)]
pub enum EffectError {
    #[error("policy denied {kind:?}: {reason}")]
    PolicyDenied { kind: EffectKind, reason: String },
    /// M2 has no decision channel. Fails closed and names what would satisfy it.
    #[error("{kind:?} requires a human decision, which M3 introduces: {reason}")]
    HumanDecisionRequired { kind: EffectKind, reason: String },
    /// The result was unknown and the postcondition read did not settle it.
    #[error("{kind:?} left an unresolved outcome: {reason}")]
    Unresolved { kind: EffectKind, reason: String },
    /// The envelope was minted for one payload and the operation would have
    /// applied another.
    ///
    /// Its own variant rather than a [`EffectError::PolicyDenied`], because no
    /// policy refused this: the proposal and the operation disagree about what
    /// the request *is*, which is a defect in the caller rather than a rule about
    /// what may be done. The identity is unchanged in this case — that is the
    /// whole reason the payload is hashed separately — so without the digest the
    /// mismatch would arrive looking like ordinary work.
    ///
    /// Both digests are carried because either one alone sends its reader to
    /// recompute the other, and a diagnostic that named only what was refused
    /// could not say what it was refused *against*.
    #[error(
        "{kind:?} was authorized for payload {} and would apply {}; nothing was performed",
        approved.0,
        applying.0
    )]
    PayloadDiverged {
        kind: EffectKind,
        approved: PayloadHash,
        applying: PayloadHash,
    },
    /// More than one object matched where exactly one was the postcondition.
    #[error("{kind:?} found {count} matching objects, expected at most one")]
    DuplicateState { kind: EffectKind, count: usize },
    #[error("adapter failure for {kind:?}: {source}")]
    Adapter { kind: EffectKind, source: GhError },
}
