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
/// `blake3` over a length-prefixed encoding of the four canonical inputs,
/// rendered as the first 16 hex characters.
///
/// **Why length-prefixing rather than a separator byte.** The obvious
/// construction joins the fields with NUL and argues that NUL cannot occur
/// inside them. That argument does not hold here. NUL is valid UTF-8, so a
/// `&str` may carry one; only [`InvocationRef`](crate::InvocationRef) is
/// constrained at its parse boundary (ADR 011), `project` and `target` are
/// not, and this function accepts `&str` rather than the parsed types, so even
/// that one guarantee does not reach it through the signature. A separator
/// whose exclusion rests on convention is a separator that can be violated by
/// input, and `("a\0b", "c")` and `("a", "b\0c")` would then name one identity
/// for two different effects — the executor would read a performed effect as
/// evidence for one that never happened.
///
/// Prefixing each field with its byte length makes the encoding injective for
/// *every* input rather than for a well-behaved subset: a reader takes exactly
/// `n` bytes, so a field's contents can never be mistaken for structure. That
/// is a stronger guarantee than validation, and it needs no rejection path —
/// there is no such thing as an input this function must refuse, so
/// [`EffectId`] stays a total function of its arguments and the milestone's
/// central property holds unconditionally.
///
/// This deliberately diverges from [`crate::correlation_key`], which still
/// joins with NUL. That value is written into fixture state on disk and
/// compared by later runs, it is pinned by test to a published digest, and
/// M0's acceptance lane depends on it; re-basing it would break the very
/// cross-process recognition it exists to provide, for an exposure M0 does not
/// have. The divergence is the considered choice, not drift.
///
/// Every input is an argument. Nothing is read from outside, and hashing is
/// arithmetic over the bytes it was handed, which is what makes the
/// recomputation checkable rather than merely plausible: the identity derived
/// on one machine before the answer was lost is the identity derived on another
/// afterwards.
pub fn effect_id(project: &str, invocation_ref: &str, kind: EffectKind, target: &str) -> EffectId {
    let material = length_prefixed([project, invocation_ref, kind.as_str(), target]);
    EffectId(truncated_digest(&material))
}

/// The digest of a canonical payload, in the same 16-hex-character shape as an
/// identity.
///
/// A single field, so it needs no framing: there is no boundary to confuse.
///
/// The caller is responsible for handing over a payload that is already
/// normalized and order-stable; this function hashes bytes and does not know
/// what would count as an equivalent spelling of the same request.
pub fn payload_hash(payload: &str) -> PayloadHash {
    PayloadHash(truncated_digest(payload))
}

/// Encode fields so that distinct tuples always yield distinct bytes.
///
/// Each field becomes its **byte** length, a colon, then the field itself —
/// `["ab", "c"]` is `2:ab1:c`, while `["a", "bc"]` is `1:a2:bc`. The length is
/// read up to the first colon and then exactly that many bytes are consumed, so
/// a colon or a digit *inside* a field is ordinary content and cannot be read
/// as framing. Byte length rather than character count because the digest is
/// taken over bytes; the two differ for any non-ASCII field.
fn length_prefixed<const N: usize>(fields: [&str; N]) -> String {
    let mut material = String::new();
    for field in fields {
        material.push_str(&field.len().to_string());
        material.push(':');
        material.push_str(field);
    }
    material
}

/// The 16-hex-character rendering both identities use.
///
/// One definition rather than two copies of the truncation, so an identity and
/// a payload digest cannot drift into different widths.
fn truncated_digest(material: &str) -> String {
    blake3::hash(material.as_bytes()).to_hex()[..16].to_string()
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
    /// under a refactor. The digests below were produced outside this crate —
    ///
    /// ```text
    /// printf '11:acme/widget9:beans:w-119:ensure_pull_request16:main<-fiddle/abc' | b3sum
    /// printf '{"title":"a"}' | b3sum
    /// ```
    ///
    /// — so they pin the *definition* rather than whatever this implementation
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
            EffectId("39b2e77d1d17cb20".to_string())
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

    /// A separator only delivers unambiguity if the byte genuinely cannot occur
    /// inside a field — and NUL is valid UTF-8, so a `&str` may carry one.
    /// [`InvocationRef`](crate::InvocationRef) is constrained at its parse
    /// boundary (ADR 011) but `project` and `target` are not, and this function
    /// takes `&str` rather than the parsed types, so no guarantee reaches it
    /// through the signature either.
    ///
    /// Each pair below is two genuinely different effects whose NUL-joined
    /// material is byte-identical. Under a separator-only encoding they would
    /// share one identity, and the executor would read a performed effect as
    /// evidence for one that had never happened — the precise failure this
    /// milestone exists to prevent.
    #[test]
    fn an_embedded_nul_cannot_forge_a_shared_identity() {
        let kind = EffectKind::EnsurePullRequest;
        let collide_under_nul_joining = [
            // One field boundary moved: the plain adjacent-field case, but with
            // the NUL supplied by the input instead of by the encoding.
            (("a\0b", "c", "t"), ("a", "b\0c", "t")),
            // The same trick reaching across the kind field: on one side the
            // reference absorbs the kind's own spelling, on the other the
            // target donates it back.
            (
                ("a", "b\0ensure_pull_request", "t"),
                ("a", "b", "ensure_pull_request\0t"),
            ),
        ];

        for ((p1, r1, t1), (p2, r2, t2)) in collide_under_nul_joining {
            // The hazard is demonstrated, not asserted: these two tuples really
            // do produce one byte string under NUL joining. If this ever stops
            // holding, the case below has quietly stopped testing anything.
            assert_eq!(
                format!("{p1}\0{r1}\0{}\0{t1}", kind.as_str()),
                format!("{p2}\0{r2}\0{}\0{t2}", kind.as_str()),
                "fixture must actually collide under NUL joining, or it proves nothing"
            );
            assert_ne!(
                effect_id(p1, r1, kind, t1),
                effect_id(p2, r2, kind, t2),
                "distinct effects must never share an identity, whatever bytes they carry"
            );
        }
    }

    /// Length-prefixing introduces a `:` and a digit run of its own, so a field
    /// that *looks* like framing is the natural next thing to try. It cannot
    /// work: a length is read first and then exactly that many bytes are taken,
    /// so content is never scanned for structure. Empty fields are included
    /// because a zero length is the case an ad-hoc encoding most often drops.
    #[test]
    fn a_field_that_mimics_the_framing_is_still_only_content() {
        let kind = EffectKind::EnsureCheckRequested;
        let tuples = [
            ("2:ab", "c", "t"),
            ("2", "ab1:c", "t"),
            ("1:x", "1:y", "t"),
            ("", "1:x1:y", "t"),
            ("1:x1:y", "", "t"),
            ("a", "b", "3:xyz"),
        ];

        let ids: Vec<_> = tuples
            .iter()
            .map(|(p, r, t)| effect_id(p, r, kind, t))
            .collect();
        for (i, left) in ids.iter().enumerate() {
            for (j, right) in ids.iter().enumerate().skip(i + 1) {
                assert_ne!(
                    left, right,
                    "{:?} and {:?} are different effects and must not share an identity",
                    tuples[i], tuples[j]
                );
            }
        }
    }

    /// Field boundaries are framed, so ("ab","c") and ("a","bc") differ.
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
