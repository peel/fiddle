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
//! same effect, but the request is not the one that was approved". A single hash
//! over both would collapse the two, and the second case would arrive looking
//! like new work.
//!
//! **What consumes the second half, and what does not.** The comparison is made
//! at the executor's step 6, in `fiddle-runtime`: the envelope is minted for the
//! payload the proposal carried, and the executor refuses before the mutation
//! when the operation it was handed is about a different one. That is the
//! "widened since it was approved" case as this milestone can actually observe
//! it — approval is minted at step 6 and spent at step 7, and the two must be
//! about the same request.
//!
//! What is **not** here is a comparison across processes. Nothing persists a
//! prior payload hash — not the journal, not the bundle, not the forge, which
//! carries the identity in a branch name and a run title but never the payload —
//! so a second attempt cannot ask what payload the first was approved for. That
//! version needs a durable record whose absence then has to mean something, and
//! a policy for the answer, and the design states the failure rather than the
//! response. It is recorded in `docs/BACKLOG.md` rather than guessed at here.

use crate::identity::CapabilityId;

/// The identity of one proposed effect: 16 hex characters of a blake3 digest
/// over the canonical inputs.
///
/// Serialized transparently, so it appears on the wire as the bare string a
/// consumer matches on.
///
/// `Hash` is derived and **nothing in this milestone keys a collection on an
/// identity**; there is no `HashMap`, `HashSet` or `BTreeMap` of effects
/// anywhere. It is here because the value is a 16-character digest that is the
/// natural key for one, and a consumer of this crate that wanted to index by it
/// would otherwise have to wrap it. The claim this comment used to make — that
/// the executor indexes proposed effects by identity when it reads back what the
/// world already contains — was never true of the tree: the executor recognises
/// an effect by *reading the world for that one effect*, one operation at a time,
/// and never by looking one up in a set it built first. See `docs/BACKLOG.md`.
#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct EffectId(pub String);

/// A digest of the canonical payload, carried beside — never folded into — the
/// identity, so a changed request is visible against an unchanged effect.
///
/// The executor's step 6 is what looks: it compares the digest the envelope was
/// minted for against the digest of the request the operation would actually
/// apply, and refuses the mutation when they differ. No `Hash`, deliberately —
/// unlike [`EffectId`] this value is only ever *compared*, and a derive nothing
/// needs is what this type's neighbours are being corrected for.
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
    /// Publishing a contextual question. `Automatic` wherever it is proposed,
    /// and it must be: a question that needed a question would not terminate.
    PublishDecisionRequest,
    /// Marking a draft pull request ready for review. The first effect in this
    /// build whose capability minimum is `Human`.
    EnsurePullRequestReady,
    /// Rewriting the body of a pull request that already exists.
    ///
    /// The first kind whose *target* carries a digest of what is being written,
    /// and the reason is the one this enum's doc gives from the other side. Every
    /// other kind acts on an object that a repeat run names identically — a
    /// branch, a head-and-base pair, a pull request at a revision — so the
    /// identity is stable across runs by construction and the payload is where a
    /// change becomes visible.
    ///
    /// A body update has no such object. The pull request it rewrites is the same
    /// pull request on every run, so a target of repository and number alone would
    /// derive one identity for "say it covers one advisory" and for "say it covers
    /// three" — and the second run would be indistinguishable from the first. That
    /// is why [`crate::EffectKind::EnsurePullRequestBody`]'s target is built by
    /// `fiddle_runtime::github::pull_request_body_target`, which folds
    /// [`content_digest`] of the intended body in beside the repository and the
    /// number.
    ///
    /// **Narrow to a pull request's body, and to nothing else.** There is no
    /// comment-editing counterpart and there must not be:
    /// `DecisionError::RequestEdited` refuses a request comment whose timestamps
    /// disagree *because* nothing in this build can edit a comment, so a kind that
    /// could would remove the ground that refusal stands on. That variant's own
    /// documentation in `fiddle_runtime::human::validate` states it — *"fiddle's
    /// own question has been edited, which fiddle has no path that does"* — and
    /// `cve_shared_pr::no_comment_edit_path_exists` walks the workspace for it.
    EnsurePullRequestBody,
}

/// How many kinds [`EffectKind`] has, and the size [`EffectKind::ALL`] is
/// declared at.
///
/// Written once and then *checked* rather than trusted: [`EffectKind::chain`]
/// refuses a successor chain that runs out before the array is full and refuses
/// one that continues past its end, and both refusals are const-evaluated, so
/// this number and the chain cannot disagree without the crate failing to build.
const KINDS: usize = 6;

impl EffectKind {
    /// Every kind this build has, in declaration order.
    ///
    /// Derived from [`EffectKind::next`] rather than written out, and that is the
    /// whole point of it existing. A hand-written array is a second list of the
    /// variants, maintained by memory: a new variant compiles perfectly well
    /// without a line in it, and every lane that iterates the array then silently
    /// stops covering the new kind. `next` is an exhaustive match with **no
    /// wildcard**, so a new variant does not compile until its author has placed
    /// it in the chain, and [`EffectKind::chain`]'s two const assertions then
    /// refuse a chain whose length has stopped agreeing with [`KINDS`].
    ///
    /// **What that does not close**, stated rather than left to look complete: an
    /// author who adds a variant, answers `next` with `None` for it and points
    /// nothing at it has written a variant the chain never reaches, and both
    /// assertions still hold. Stable Rust has no way to enumerate an enum's
    /// variants, so nothing short of generating the enum from a single list closes
    /// that — and generating it would take the documentation above off each
    /// variant and put the closed set behind a macro, which this crate's
    /// legibility is worth more than the remaining gap. What the construction does
    /// buy is that the author is *made to answer the question*, in the same place
    /// and by the same mechanism the plan requires of `as_str`.
    pub const ALL: [EffectKind; KINDS] = Self::chain();

    /// Where the chain starts. Declaration order, so [`EffectKind::ALL`] reads as
    /// the enum reads.
    const FIRST: EffectKind = EffectKind::EnsureBranchPublished;

    /// The kind after this one in declaration order, or `None` at the end.
    ///
    /// An exhaustive match with no wildcard, which is the forcing function
    /// [`EffectKind::ALL`] rests on. A wildcard arm here would let a new variant
    /// fall through to `None`, which is exactly the silence this construction
    /// exists to prevent.
    const fn next(self) -> Option<EffectKind> {
        match self {
            EffectKind::EnsureBranchPublished => Some(EffectKind::EnsurePullRequest),
            EffectKind::EnsurePullRequest => Some(EffectKind::EnsureCheckRequested),
            EffectKind::EnsureCheckRequested => Some(EffectKind::PublishDecisionRequest),
            EffectKind::PublishDecisionRequest => Some(EffectKind::EnsurePullRequestReady),
            EffectKind::EnsurePullRequestReady => Some(EffectKind::EnsurePullRequestBody),
            EffectKind::EnsurePullRequestBody => None,
        }
    }

    /// Walk the chain from [`EffectKind::FIRST`] into the array
    /// [`EffectKind::ALL`] publishes.
    ///
    /// Both refusals are load-bearing and they fail in opposite directions. A
    /// chain that ends early means [`KINDS`] claims more kinds than the chain
    /// reaches, so the array would have to be padded with a repeat of `FIRST` — a
    /// list with a duplicate in it, which every lane that iterates it would then
    /// be quietly testing twice. A chain that continues past the end means a kind
    /// exists that the array omits, which is the silence this whole construction
    /// is about.
    const fn chain() -> [EffectKind; KINDS] {
        let mut all = [Self::FIRST; KINDS];
        let mut i = 1;
        while i < KINDS {
            all[i] = match all[i - 1].next() {
                Some(kind) => kind,
                None => panic!("the successor chain ends before ALL is full"),
            };
            i += 1;
        }
        assert!(
            all[KINDS - 1].next().is_none(),
            "the successor chain continues past the end of ALL, so a kind is missing from it"
        );
        all
    }

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
            EffectKind::PublishDecisionRequest => "publish_decision_request",
            EffectKind::EnsurePullRequestReady => "ensure_pull_request_ready",
            EffectKind::EnsurePullRequestBody => "ensure_pull_request_body",
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

/// A digest of content that is *part of a target*, in the same
/// 16-hex-character shape as an identity.
///
/// This exists for the one case [`EffectKind::EnsurePullRequestBody`] describes:
/// an effect whose object is the same object on every run, so that what is being
/// written has to enter the identity or two different writes become one effect.
/// Folding the digest into the target is what makes them two.
///
/// **Not [`payload_hash`], and the distinction is deliberate rather than
/// cosmetic.** [`PayloadHash`]'s own documentation is that it is *"carried
/// beside — never folded into — the identity"*, because it is the value step 6
/// compares to catch a request that widened after it was approved. A value
/// serving both roles would mean an approval and the request it was given for
/// could never disagree — the divergence check would compare a number against
/// itself and always pass. So this returns a bare [`String`], which composes into
/// a target and cannot be mistaken at a call site for the thing that is compared
/// against one.
///
/// A digest rather than the content itself, for two reasons that both matter. A
/// target is hashed into an identity and is also carried in a receipt and read by
/// people, so an unbounded target is an unbounded record of something already
/// recorded elsewhere; and a body is prose somebody may have written, which has
/// no business appearing verbatim in an identity string.
///
/// Pure, like everything else here: arithmetic over the bytes it was handed, no
/// clock and no local state, so a later process on another machine derives the
/// same digest for the same content.
pub fn content_digest(content: &str) -> String {
    truncated_digest(content)
}

/// Encode fields so that distinct tuples always yield distinct bytes.
///
/// Each field becomes its **byte** length, a colon, then the field itself —
/// `["ab", "c"]` is `2:ab1:c`, while `["a", "bc"]` is `1:a2:bc`. The length is
/// read up to the first colon and then exactly that many bytes are consumed, so
/// a colon or a digit *inside* a field is ordinary content and cannot be read
/// as framing. Byte length rather than character count because the digest is
/// taken over bytes; the two differ for any non-ASCII field.
///
/// Visible to the crate rather than to this module alone because
/// [`decision_request_id`](crate::decision_request_id) frames its own three
/// fields the same way. Sharing the function rather than the argument is what
/// keeps the two framings from drifting apart: a second copy could acquire a
/// different separator or a character count under a later edit, and nothing
/// would fail until an identity stopped matching across builds.
pub(crate) fn length_prefixed<const N: usize>(fields: [&str; N]) -> String {
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
/// One definition rather than two copies of the truncation, so an identity, a
/// payload digest and a decision request id cannot drift into different widths.
/// The marker's parser checks each field against a fixed length, so a width
/// that moved here would be a marker this build could no longer read.
pub(crate) fn truncated_digest(material: &str) -> String {
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
    ///
    /// Over [`EffectKind::ALL`] rather than over a list written out here. The
    /// list this used to hold was the second copy of the variants that
    /// `ALL`'s documentation is about: a kind added without a line in it was a
    /// kind whose two spellings nothing compared, and neither this test nor its
    /// neighbour below would have said so.
    #[test]
    fn the_hashed_kind_and_the_serialized_kind_are_the_same_spelling() {
        for kind in EffectKind::ALL {
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                serde_json::json!(kind.as_str()),
                "serde and as_str must agree for {kind:?}"
            );
        }
    }

    /// The wire spelling is what a later process matches on, so two kinds
    /// sharing one spelling would make two different effects indistinguishable
    /// on the wire and identical in the identity.
    #[test]
    fn every_kind_has_a_distinct_wire_spelling() {
        let mut seen = std::collections::BTreeSet::new();
        for kind in EffectKind::ALL {
            assert!(
                seen.insert(kind.as_str()),
                "{} is spelled twice",
                kind.as_str()
            );
        }
        assert_eq!(seen.len(), EffectKind::ALL.len());
    }

    /// [`EffectKind::ALL`] is the chain, walked — so it holds every kind once,
    /// in declaration order, and holds no duplicate.
    ///
    /// The duplicate half is the one worth stating. `chain` seeds the array with
    /// [`EffectKind::FIRST`] and overwrites from index one, so a chain that ran
    /// out early would leave a repeat of the first kind sitting in the tail; the
    /// const assertion refuses that at build time and this is its runtime
    /// witness. Without it, every lane that iterates `ALL` would be silently
    /// testing one kind twice and another not at all.
    #[test]
    fn all_holds_every_kind_once_in_declaration_order() {
        assert_eq!(EffectKind::ALL[0], EffectKind::FIRST);
        assert_eq!(EffectKind::ALL.last().unwrap().next(), None);

        let mut seen = std::collections::BTreeSet::new();
        for kind in EffectKind::ALL {
            assert!(
                seen.insert(kind.as_str()),
                "{} appears twice",
                kind.as_str()
            );
        }
        assert_eq!(seen.len(), KINDS);

        // Adjacent entries really are successors, which is what makes the array
        // the chain rather than a list that happens to be the same length.
        for pair in EffectKind::ALL.windows(2) {
            assert_eq!(pair[0].next(), Some(pair[1]));
        }
    }

    /// The new kind is on the wire under the spelling the plan names, and the
    /// spelling is pinned rather than derived from the variant's name — serde's
    /// `rename_all` and [`EffectKind::as_str`] are two mechanisms that could
    /// agree with each other while both drifting from what an earlier build
    /// wrote into a record.
    #[test]
    fn the_body_kind_is_spelled_ensure_pull_request_body() {
        assert_eq!(
            EffectKind::EnsurePullRequestBody.as_str(),
            "ensure_pull_request_body"
        );
        assert!(EffectKind::ALL.contains(&EffectKind::EnsurePullRequestBody));
    }

    /// A digest folded into a target has to be a digest: bounded, moving with
    /// its content, and never the content itself.
    ///
    /// The last of the three is the one an implementation could lose by
    /// accident. `format!("{repo}#{pr}@{body}")` also gives two bodies two
    /// targets, so "the target moved" alone does not distinguish a digest from
    /// the prose spliced in whole — and the prose in an identity string is both
    /// unbounded and a copy of something already recorded in the payload.
    #[test]
    fn a_content_digest_is_bounded_and_moves_with_its_content() {
        let short = content_digest("covers 1 CVE");
        let long = content_digest(&"covers 3 CVEs. ".repeat(500));

        assert_eq!(short.len(), 16);
        assert_eq!(long.len(), short.len(), "a digest does not grow with input");
        assert!(short.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(
            !short.contains("covers"),
            "the content is not in the digest"
        );

        assert_ne!(short, content_digest("covers 3 CVEs"));
        assert_eq!(
            short,
            content_digest("covers 1 CVE"),
            "and it is recomputable, which is what a later process depends on"
        );
    }

    /// Every 16-hex value in this module is the same width, pinned against an
    /// independently computed digest rather than against each other.
    ///
    /// ```text
    /// printf '{"title":"a"}' | b3sum
    /// ```
    ///
    /// The width is a contract: the marker's parser checks each field against a
    /// fixed length, so a [`content_digest`] that drifted would put a target
    /// into an identity a later build could no longer read back. Pinning it to
    /// the published digest is what makes that checkable — comparing it to
    /// [`payload_hash`] would only prove the two moved together.
    #[test]
    fn a_content_digest_is_pinned_to_the_same_published_width() {
        assert_eq!(content_digest(r#"{"title":"a"}"#), "7950cf4c9a0b76f2");
    }

    /// The kind is one of the four framed inputs, so two kinds against the same
    /// target are different effects. Without this, making a pull request ready
    /// and asking about it would collide.
    #[test]
    fn a_kind_participates_in_the_identity() {
        let a = effect_id(
            "p",
            "beans:x",
            EffectKind::PublishDecisionRequest,
            "acme/r#7",
        );
        let b = effect_id(
            "p",
            "beans:x",
            EffectKind::EnsurePullRequestReady,
            "acme/r#7",
        );
        assert_ne!(a, b);
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
