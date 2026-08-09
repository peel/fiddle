//! Identity for one proposed external effect.
//!
//! The rule this module exists to hold: an identity is derived from canonical
//! inputs and from nothing else, because the process that needs to recompute it
//! is a *different* process that has already lost the first one's workspace,
//! journal and memory. A clock, a counter or a random source here would make
//! the milestone's central property — recognising work a previous process
//! performed against the outside world — unprovable rather than merely
//! untested.
//!
//! Identity and payload are hashed separately, and that split is the point. An
//! effect keeps its identity while its payload changes, so the executor can
//! tell "this is the same effect, already performed" apart from "this is the
//! same effect, but the request has been widened since it was approved". A
//! single hash over both would collapse the two, and the second case would
//! arrive looking like new work.

use crate::identity::CapabilityId;

/// The identity of one proposed effect: 16 hex characters of a blake3 digest
/// over the canonical inputs.
///
/// Serialized transparently, so it appears on the wire as the bare string a
/// consumer matches on. `Hash` because the executor indexes proposed effects by
/// identity when it reads back what the world already contains.
#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct EffectId(pub String);

/// A digest of the canonical payload, carried beside — never folded into — the
/// identity, so a changed request is visible against an unchanged effect.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct PayloadHash(pub String);

/// The external results this milestone supports, as a closed set.
///
/// Closed rather than a free string for the same reason [`crate::InvocationScheme`]
/// is: an unrecognised kind is a rejected effect, not one fiddle will attempt on
/// the strength of a name it does not know. It also makes the identity's kind
/// component a spelling this build controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    EnsureBranchPublished,
    EnsurePullRequest,
    EnsureCheckRequested,
}

impl EffectKind {
    /// A stable wire name, and the single source of the kind's spelling: the
    /// identity hashes it and serde renders it, so the two cannot drift.
    ///
    /// Deliberately not `Debug`, which is a diagnostic aid rather than a
    /// contract and which a derive is free to change.
    pub fn as_str(&self) -> &'static str {
        match self {
            EffectKind::EnsureBranchPublished => "ensure_branch_published",
            EffectKind::EnsurePullRequest => "ensure_pull_request",
            EffectKind::EnsureCheckRequested => "ensure_check_requested",
        }
    }
}

/// An effect a capability proposes to perform against the outside world.
///
/// `target` and `payload` are separate fields rather than one request document
/// because only `target` enters the identity: what is being acted on is what
/// makes two proposals the same effect, and how it is being described is not.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ProposedEffect {
    pub capability: CapabilityId,
    pub kind: EffectKind,
    /// Canonical target identity: what is being acted on, independent of payload.
    pub target: String,
    /// Canonical payload: the full request, normalized and order-stable.
    pub payload: String,
}

/// The identity a fresh process recomputes for an effect it may already have
/// performed.
///
/// `blake3(project + NUL + invocation_ref + NUL + kind + NUL + target)`,
/// rendered as the first 16 hex characters — deliberately the same construction
/// as [`crate::correlation_key`], because it answers the same question one level
/// out: which writer does this belong to.
///
/// The separator is a NUL byte because it cannot occur inside any of the four
/// inputs, so no pair of adjacent fields can be re-split into another pair and
/// collide: `("ab", "c")` and `("a", "bc")` hash differently. Without it two
/// distinct effects could share an identity, and the executor would read one as
/// evidence that the other had already been performed.
///
/// Every input is an argument. Nothing is read from the environment, and
/// hashing is arithmetic over the bytes it was handed, which is exactly what
/// makes the recomputation checkable rather than merely plausible: the identity
/// derived on one machine before the answer was lost is the identity derived on
/// another afterwards.
pub fn effect_id(project: &str, invocation_ref: &str, kind: EffectKind, target: &str) -> EffectId {
    let material = format!("{project}\0{invocation_ref}\0{}\0{target}", kind.as_str());
    EffectId(blake3::hash(material.as_bytes()).to_hex()[..16].to_string())
}

/// The digest of a canonical payload, in the same 16-hex-character shape as an
/// identity.
///
/// The caller is responsible for handing over a payload that is already
/// normalized and order-stable; this function hashes bytes and does not know
/// what would count as an equivalent spelling of the same request.
pub fn payload_hash(payload: &str) -> PayloadHash {
    PayloadHash(blake3::hash(payload.as_bytes()).to_hex()[..16].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole milestone rests on: a process with no memory of
    /// the first attempt derives the same identity from the same canonical
    /// inputs. Nothing here may read a clock, a counter or a random source.
    #[test]
    fn an_effect_id_is_recomputable_from_canonical_inputs_alone() {
        let first = effect_id(
            "acme/widget",
            "beans:w-1",
            EffectKind::EnsurePullRequest,
            "main<-fiddle/abc",
        );
        let second = effect_id(
            "acme/widget",
            "beans:w-1",
            EffectKind::EnsurePullRequest,
            "main<-fiddle/abc",
        );
        assert_eq!(first, second);
        assert_eq!(
            first.0.len(),
            16,
            "16 hex characters, as correlation_key is"
        );
        assert!(first.0.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// Equality between two calls in one process only proves the function is
    /// not obviously stateful; it would still hold if the construction changed
    /// under a refactor. The digest below was produced outside this crate —
    ///
    /// ```text
    /// printf 'acme/widget\0beans:w-1\0ensure_pull_request\0main<-fiddle/abc' | b3sum
    /// ```
    ///
    /// — so it pins the *definition* rather than whatever this implementation
    /// happens to compute. That is what the recomputation actually rests on: a
    /// later process is a later build too, and an identity that moved between
    /// builds would fail to recognise an effect that had really been performed,
    /// and perform it a second time.
    #[test]
    fn an_effect_id_is_pinned_to_an_independently_computed_digest() {
        assert_eq!(
            effect_id(
                "acme/widget",
                "beans:w-1",
                EffectKind::EnsurePullRequest,
                "main<-fiddle/abc"
            ),
            EffectId("ea1b316ceb483813".to_string())
        );
        assert_eq!(
            payload_hash(r#"{"title":"a"}"#),
            PayloadHash("7950cf4c9a0b76f2".to_string())
        );
    }

    /// The kind's spelling is hashed into every identity *and* rendered onto
    /// the wire, through two separate mechanisms. If serde's rename and
    /// [`EffectKind::as_str`] ever disagree, an identity would stop matching
    /// the record that describes it, so the two are pinned against each other
    /// here rather than trusted to stay in step.
    #[test]
    fn the_hashed_kind_and_the_serialized_kind_are_the_same_spelling() {
        for kind in [
            EffectKind::EnsureBranchPublished,
            EffectKind::EnsurePullRequest,
            EffectKind::EnsureCheckRequested,
        ] {
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str()),
                "serde and as_str must agree for {kind:?}"
            );
        }
    }

    /// Each of the four inputs must move the identity, or two different effects
    /// collide and the executor treats one as evidence for the other.
    #[test]
    fn every_canonical_input_changes_the_identity() {
        let base = effect_id(
            "acme/widget",
            "beans:w-1",
            EffectKind::EnsurePullRequest,
            "t",
        );
        assert_ne!(
            base,
            effect_id(
                "acme/other",
                "beans:w-1",
                EffectKind::EnsurePullRequest,
                "t"
            )
        );
        assert_ne!(
            base,
            effect_id(
                "acme/widget",
                "beans:w-2",
                EffectKind::EnsurePullRequest,
                "t"
            )
        );
        assert_ne!(
            base,
            effect_id(
                "acme/widget",
                "beans:w-1",
                EffectKind::EnsureBranchPublished,
                "t"
            )
        );
        assert_ne!(
            base,
            effect_id(
                "acme/widget",
                "beans:w-1",
                EffectKind::EnsurePullRequest,
                "u"
            )
        );
    }

    /// Field boundaries are separated, so ("ab","c") and ("a","bc") differ.
    #[test]
    fn adjacent_fields_cannot_be_confused() {
        assert_ne!(
            effect_id("ab", "c", EffectKind::EnsureCheckRequested, "t"),
            effect_id("a", "bc", EffectKind::EnsureCheckRequested, "t"),
        );
    }

    /// A payload change must be detectable against an unchanged identity —
    /// that is what stops an approved effect from being widened later.
    #[test]
    fn a_payload_hash_moves_independently_of_the_identity() {
        assert_ne!(
            payload_hash(r#"{"title":"a"}"#),
            payload_hash(r#"{"title":"b"}"#)
        );
        assert_eq!(
            payload_hash(r#"{"title":"a"}"#),
            payload_hash(r#"{"title":"a"}"#)
        );
    }
}
