//! The identity of a question put to a person, the marker that carries it across
//! a process boundary, and the four values an answer can amount to.
//!
//! A run that needs a human decision publishes the question and stops. The run
//! that acts on the answer is a *different* process, started later, holding no
//! workspace, no journal and no memory of the first. The only thing the two
//! share is the conversation, so the request comment is where the binding
//! between question and effect has to live.
//!
//! The marker in that comment is not evidence, and this module's shape is what
//! keeps it from becoming evidence. Every field it carries — the request id, the
//! gated [`EffectId`], the [`PayloadHash`] and the head sha — is a value the
//! later process obtains for itself, from its own canonical inputs or from the
//! forge, and then compares; none of them is believed because it was read. What
//! the marker actually supplies is narrower and cannot be derived: which question
//! this comment is, so the continuation knows what to recompute in the first
//! place.
//!
//! [`parse_marker`] is therefore strict to the point of refusing anything it
//! does not recognise exactly. The bodies it runs against are ordinary
//! conversation, where a person may well quote a marker while discussing it, and
//! a lenient parser would read that quotation as a request. A body that
//! half-matches is refused rather than repaired, and a body with no marker at
//! all is [`MarkerError::Absent`] rather than an error, because that is what
//! every other comment in the conversation looks like.
//!
//! # A parse that succeeds has authenticated nothing
//!
//! Strictness is worth having and is worth not overreading. A successful
//! [`parse_marker`] establishes one fact: this body contains a well-formed
//! marker. It does not establish that the marker is *this* run's request, that
//! fiddle wrote it, or that a person did. A comment quoting a request comment
//! parses, because a verbatim quote is byte-identical. A quotation somebody
//! *edited* parses too, and yields a **different** binding — well-formedness is
//! a property of the four fields' shapes, never of their values, so any four
//! plausible digests satisfy it.
//!
//! This module cannot close that gap and is deliberately not asked to. It has no
//! run to compare against and no forge to ask, and a rule invented here to guess
//! at authorship would be a rule that could be satisfied by whoever knew it. The
//! authentication happens in the continuation, and its shape matters here because
//! it is what a successful parse is not: three of the marker's four fields are
//! compared against values recomputed from the run's own canonical inputs — the
//! request id as a *sieve*, answering which comment this is and authenticating
//! nothing, since it can be copied off the visible conversation; the gated
//! [`EffectId`] as the authentication itself — not because the conversation
//! cannot carry it, since a verbatim quotation of the request comment carries it
//! exactly and is refused one step earlier, as a second comment naming this
//! request id, but because nobody without the run's canonical inputs can
//! *derive* it, so a marker somebody composed names an effect this run does not
//! derive; the [`PayloadHash`] as the separate claim that the work has not moved
//! under an approval. The fourth, the head sha, is compared against the head
//! observed from the forge, and that is a recomputation on neither side.
//! Authorship is a check of its own: an allowlist of numeric
//! [`ActorRef::id`], reached only for comments that are not bot- or
//! app-attributed, so a bot carrying an allowlisted id is refused before the
//! allowlist is consulted. A caller that branched on
//! `parse_marker(body).is_ok()` would have skipped every part of that, however
//! strict the parser it called.
//!
//! The paragraph above describes another crate — `fiddle-runtime`'s
//! `human::validate`, where `resolve` holds the request-id sieve and the effect
//! and payload comparisons, `observe` — which `resolve` calls — holds the head
//! sha comparison against the forge, and `select_candidates` holds the
//! authorship rule — and it is stated knowing that nothing in this file can keep
//! it true. `fiddle-core` does not depend on `fiddle-runtime` and must not, so no
//! test here fails when that walk changes. An earlier version of this text called
//! all four fields recomputed and presented the request-id sieve as an
//! authentication, and it survived because there was nothing here able to
//! contradict it. Read those functions rather than this paragraph if the
//! difference matters.
//!
//! The consequence for [`render_marker`] is that its output is a wire format:
//! one build writes those bytes and a later build reads them, so the bytes are
//! pinned by a test against a literal rather than by a round trip, which moves
//! both halves together and would agree with any format at all.
//!
//! [`InterpretedHumanDecision`] is the far end of the same exchange, and it lives
//! beside the marker for a reason the two share: both are what one process is
//! willing to conclude from text another party wrote. The marker's answer to that
//! is to carry only values its reader compares against ones it obtained for
//! itself, rather than values a reader has to take on trust. This enum's answer
//! is to have nowhere to put anything else — see its own documentation.

use crate::effect::{length_prefixed, truncated_digest, EffectId, PayloadHash};
use crate::identity::{CapabilityId, WorkRef};
use crate::published::Published;
use crate::report::EvidenceRef;

/// The identity of one question put to a person: 16 hex characters of a blake3
/// digest over the run and the effect the question gates.
///
/// Serialized transparently, so it appears on the wire as the bare string a
/// consumer matches on, as [`EffectId`] does.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct DecisionRequestId(pub String);

/// What the marker in a request comment says, and the only thing a continuation
/// trusts it for: knowing what to recompute and compare against.
///
/// # Its serialized field set is pinned, and by a different test than you expect
///
/// A fifth field here is the same hazard
/// [`HumanDecisionRequest`]'s own documentation describes — a second copy of the
/// request id, a marker naming one question while the consumer looks for another,
/// and a run that posts forever — one level below where that type's key-set
/// assertion can look. `the_request_id_is_held_in_exactly_one_place` walks the
/// document for the id's *value*, so it catches a nested copy that **agrees** with
/// `request` and cannot see one that disagrees, which is the more dangerous of the
/// two. `every_object_in_a_serialized_request_holds_exactly_the_fields_it_declares`
/// is what refuses the field whatever value it carries.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct DecisionBinding {
    pub request: DecisionRequestId,
    pub effect: EffectId,
    pub payload: PayloadHash,
    /// The revision the question was asked about, 40 lowercase hex characters.
    ///
    /// Carried separately because the payload does not name it: a payload can be
    /// word-for-word what it was while the branch beneath it has moved, and an
    /// approval given for one revision is not an approval of another.
    pub head_sha: String,
}

/// Who did something, by the two names a forge gives them.
///
/// Here rather than in the adapter that reads one, because an authorization
/// check is domain logic: the validation order compares an allowlist against
/// this value, and a pure module that had to reach into the GitHub client for
/// the type it compares would invert the boundary this workspace enforces
/// mechanically.
///
/// **The `id` is what an allowlist matches, and the `login` is what a
/// diagnostic prints.** They are not interchangeable, and that is the reason
/// both are carried. A login can be changed by the person holding it,
/// released, and then taken by somebody else; a numeric id cannot, and is
/// never reissued. An allowlist checked against logins therefore grants the
/// decision to whoever holds the name on the day the check runs, who may be a
/// different person from the one that was authorized. The login stays because
/// a refusal naming only a number is unreadable by the operator who has to act
/// on it.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ActorRef {
    /// Immutable and never reissued. The half an authorization decision is made
    /// against.
    pub id: u64,
    /// What the person is called today. For reading, never for deciding.
    pub login: String,
}

/// What a person's reply amounts to, and the whole of what reading one may
/// conclude.
///
/// # Why this type is the milestone's blast radius
///
/// A reply is ordinary language and is read by a model, so the reading can be
/// wrong. What this enum guarantees is that being wrong cannot *widen* anything:
/// the effect's identity, the payload digest, the actor and the target all come
/// from the deterministic shell, which read them from the world and compares them
/// against it. A wrong reading picks the wrong one of four branches. There is no
/// field here for it to pick a different effect through.
///
/// That is a property of the shape rather than of any rule, which is what makes
/// it hold against a hostile model as well as a mistaken one. A variant added
/// later that carried an [`EffectId`], an actor or a policy would give a reading
/// somewhere to put one, so the guarantee is worth stating as the reason the
/// shape is closed: two of the four variants carry nothing at all, and the other
/// two carry text.
///
/// # Why the text is [`Published`] and not [`String`]
///
/// Because it is authored outside this process — by a person, relayed by a model
/// reading that person — and it lands on
/// [`RunOutcome`](crate::RunOutcome)'s reason, which is a field somebody reads.
/// [`Published::of`] is the only way to fill such a field, so a caller cannot
/// forward an unbounded reply by writing the assignment correctly. A `String`
/// here would accept anything.
///
/// # Why [`Unclear`](InterpretedHumanDecision::Unclear) rather than an error
///
/// Because everything that is not an answer has to arrive as the same value, and
/// an error is a value callers treat differently. A timeout, a refusal, empty
/// output, malformed JSON, an unknown spelling — each of those is a reply nobody
/// gave, and the right response to all of them is the follow-up
/// `Unclear` produces. Returning a `Result` would offer a caller an `unwrap_or`,
/// and the only default an approval-shaped API has is an approval.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum InterpretedHumanDecision {
    /// The person approved this request, unconditionally.
    ///
    /// It carries nothing, and that is the point: an approval is an approval of
    /// the question that was asked, which the shell already holds. Anything this
    /// variant could carry would be something a reading got to choose about an
    /// effect it did not author.
    Approve,
    /// The person declined. The reason is theirs, quoted.
    Reject { reason: Published },
    /// The person asked for something else. The instruction reaches a later
    /// prompt, so whatever produces one bounds it before it gets here.
    Redirect { instruction: Published },
    /// No answer was read — including because none was given, and including
    /// because reading failed.
    Unclear,
}

/// The question itself: what a person is being asked, and everything they need
/// in order to answer it.
///
/// A description and not a channel. *Where* the question is put — a pull request
/// conversation today, something else later — is `fiddle-runtime`'s
/// `PublishDecisionRequest`, which renders one of these into a body and publishes
/// it. What lives here is the content, because the content is what the run
/// derived and is the same question whichever conversation ends up carrying it.
///
/// # The fields a person reads, and why they are all required
///
/// [`question`](HumanDecisionRequest::question),
/// [`rationale`](HumanDecisionRequest::rationale),
/// [`risks`](HumanDecisionRequest::risks),
/// [`alternatives`](HumanDecisionRequest::alternatives) and
/// [`evidence`](HumanDecisionRequest::evidence) are one set rather than five
/// options, because the person answering has *only* what this type carries: they
/// are reading a comment, not sitting beside the run. A request that named its
/// question and left out what it rests on would be asking for an approval of
/// something the approver cannot see, and the three [`Vec`]s being empty is the
/// honest way to say a run found no risk, weighed no alternative or cited
/// nothing — which is a statement, and different from the field not being there.
///
/// # Why the free text is [`String`] and not [`Published`]
///
/// Because this text is authored *inside* this process: it is what the run
/// concluded about its own work, from its own canonical inputs. [`Published`] is
/// the bound on text that arrived from somewhere else — a person, or a model
/// relaying one — and [`InterpretedHumanDecision`] is where it is applied, on the
/// way back. Bounding the outbound half as well would read as symmetry while
/// actually being a bound against the wrong party.
///
/// [`invocation_ref`](HumanDecisionRequest::invocation_ref) is a [`String`] for a
/// narrower reason: it is the same spelling [`decision_request_id`] hashes, and a
/// value of this type is built by a run that already parsed its own reference at
/// the boundary where a defect in one is reportable.
///
/// # Why the binding is carried rather than derived here
///
/// Not one of [`DecisionBinding`]'s four fields is this type's to compute. They
/// belong to the effect the question gates — its identity, its payload digest,
/// the revision it was asked about — which the caller holds and this type only
/// describes. Carrying it is also what makes the rendered body self-sufficient:
/// [`render_marker`] over this one field is the whole of what a later process
/// needs to recognise this question again.
///
/// # Which question this is lives in the binding, and nowhere else
///
/// [`binding`](HumanDecisionRequest::binding)`.request` is the only place this type
/// holds the request id, and that is a property rather than an omission. The id is
/// what a later process recognises the question by, and it can only recognise it
/// through the marker — which [`render_marker`] renders from the binding. A second
/// copy of the id beside the binding would therefore be a value nothing on the wire
/// could ever carry, and a producer that filled the two from two derivations, or a
/// consumer that matched on the copy, would publish a marker naming one question
/// and then look for another: it would find nothing, conclude it had not asked yet,
/// and **post again on every attempt, forever**. That copy existed until
/// `fiddle-11vj` deleted it. It was read by nothing, so no test could have noticed
/// the disagreement; the fix is that the disagreement is no longer expressible.
///
/// Re-adding it is a failing test and not a review comment:
/// `the_request_id_is_held_in_exactly_one_place` asserts this type's serialized
/// top-level key set, which is what makes a *shape* the derive already exposes into
/// something a gate can refuse. Anything that needs the id at the top level takes a
/// method over [`binding`](HumanDecisionRequest::binding), never a stored field —
/// storing a derived value beside what it derives from is what created a hazard no
/// behavioural test could see.
#[derive(Clone, Debug, serde::Serialize)]
pub struct HumanDecisionRequest {
    /// The request that reached the asking run. For a reader, and for
    /// [`binding`](HumanDecisionRequest::binding)'s request id, which is derived
    /// over it.
    pub invocation_ref: String,
    /// The work the question is about, when the run is addressed to work at all.
    pub work_ref: Option<WorkRef>,
    /// Which capability is asking.
    pub capability: CapabilityId,
    /// What an answer to this question would be an answer *to*.
    pub binding: DecisionBinding,
    /// The one thing being asked, phrased so that yes and no both mean
    /// something.
    pub question: String,
    /// Why the run wants to do it.
    pub rationale: String,
    /// What could go wrong if the answer is yes.
    pub risks: Vec<String>,
    /// What else the run considered and did not propose.
    pub alternatives: Vec<String>,
    /// What the rationale rests on, so the approver can check it rather than
    /// take it.
    pub evidence: Vec<EvidenceRef>,
}

/// The only marker version this build writes, and the only one it reads.
pub const MARKER_VERSION: &str = "v1";

/// Why a body is not a request comment this build can act on.
///
/// Three variants rather than one, because the three are acted on differently.
/// [`Absent`](MarkerError::Absent) is the ordinary case and means "keep
/// looking"; [`Version`](MarkerError::Version) means a marker fiddle wrote in
/// some other build, which a reader diagnoses by upgrading; and
/// [`Malformed`](MarkerError::Malformed) means a body shaped like a marker that
/// is not one, which is the case worth printing.
///
/// Because those three prescribe three different actions, which one a body earns
/// is part of the contract and not a detail. In particular [`Version`] is
/// reserved for a well-formed version token this build does not know: it tells
/// an operator to upgrade, and a body that was merely reflowed, respaced or
/// truncated must not tell them that, because there is no build to upgrade to.
/// [`parse_marker`] therefore checks that the token is shaped like a version
/// before it compares it.
///
/// [`Version`]: MarkerError::Version
#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum MarkerError {
    #[error("no fiddle decision marker in this body")]
    Absent,
    #[error("marker version {0} is not {MARKER_VERSION}")]
    Version(String),
    #[error("marker is malformed: {0}")]
    Malformed(String),
}

/// Everything before the version token, and the string a body is searched for.
const OPENING: &str = "<!-- fiddle:decision ";

/// The closing delimiter, including the space that separates it from the last
/// field, so the fields are exactly what lies between the two.
const CLOSING: &str = " -->";

/// The four fields, in the one order a marker may spell them, each with the
/// number of hex characters its value must have.
///
/// A slice rather than four open-coded branches so that the order and the widths
/// are one statement a reader checks against the format above, rather than four
/// that could disagree.
const FIELDS: [(&str, usize); 4] = [
    ("request", 16),
    ("effect", 16),
    ("payload", 16),
    ("head", 40),
];

/// Whether a token could be a marker version at all: `v` and then at least one
/// decimal digit.
///
/// The version's own spelling is the one part of the grammar that cannot change
/// between versions, because every build has to read it *before* it knows
/// whether it can read anything else. So a token of this shape which is not
/// [`MARKER_VERSION`] really is a marker from another build, and
/// [`MarkerError::Version`] tells its reader the true and actionable thing.
///
/// A token of any other shape is not a version. It is what a body that was
/// reflowed, respaced or truncated leaves in the first position — an empty
/// string from a doubled space, a whole field from a dropped token, a run of
/// fields from a newline swallowing the separators — and none of those is
/// diagnosed by upgrading. Checking the shape here is what keeps the version
/// comparison from becoming the catch-all that the first token of a mangled
/// body falls into.
fn is_version_token(token: &str) -> bool {
    token
        .strip_prefix('v')
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

/// The identity of the question that gates one effect.
///
/// `blake3` over a length-prefixed encoding of `(project, invocation_ref,
/// effect)`, rendered as the first 16 hex characters — the same framing and the
/// same width [`effect_id`](crate::effect_id) uses, and reusing its encoder
/// rather than restating it.
///
/// **Why it derives from the effect rather than standing alone.** A random or
/// counted id would have to be persisted to be recognised again, and there is
/// nowhere to persist it: the process that asked the question is gone by the
/// time the answer arrives. Deriving it means the continuation recomputes the
/// id from inputs it can reconstruct — which repository, which work item, which
/// effect — and finds its own earlier question in the conversation with nothing
/// carried between the two.
///
/// It also makes staleness free rather than a mechanism of its own. The gated
/// [`EffectId`] covers the effect's target, so a moved branch head derives a
/// different effect, which derives a different request id, which is a different
/// question. An approval given to the earlier question is not addressed to the
/// later one, and no rule had to be written to say so.
///
/// Length-prefixed for [`effect_id`](crate::effect_id)'s reason. A separator
/// byte would be a separator the arguments could contain — `project` is
/// unconstrained and this function takes `&str` rather than the parsed types —
/// and `("a", "bc")` and `("ab", "c")` would then name one question for two
/// different effects.
///
/// Every input is an argument, and hashing is arithmetic over the bytes it was
/// handed, so the id derived before the run stopped is the id derived after it.
pub fn decision_request_id(
    project: &str,
    invocation_ref: &str,
    effect: &EffectId,
) -> DecisionRequestId {
    let material = length_prefixed([project, invocation_ref, effect.0.as_str()]);
    DecisionRequestId(truncated_digest(&material))
}

/// Render a binding as the marker line a request comment carries.
///
/// An HTML comment, so a person reading the conversation sees the question and
/// not the bookkeeping, while the API body a later process reads carries it
/// exactly. Written in [`FIELDS`]' order, which is the order [`parse_marker`]
/// requires, from the one declaration.
///
/// This does not validate what it is handed. A binding is built from values this
/// crate derived, and a marker is checked when it is read back rather than when
/// it is written — the reader is the side facing text it did not author.
pub fn render_marker(binding: &DecisionBinding) -> String {
    let values = [
        binding.request.0.as_str(),
        binding.effect.0.as_str(),
        binding.payload.0.as_str(),
        binding.head_sha.as_str(),
    ];
    let mut out = String::from(OPENING);
    out.push_str(MARKER_VERSION);
    for ((key, _), value) in FIELDS.iter().zip(values) {
        out.push(' ');
        out.push_str(key);
        out.push('=');
        out.push_str(value);
    }
    out.push_str(CLOSING);
    out
}

/// Read the one marker a body carries, or say why it has none this build can
/// act on.
///
/// Strict in every dimension the format has: exactly one marker, exactly the
/// four keys, in exactly that order, each value exactly its width in lowercase
/// hex, and nothing else between the delimiters. Each of those is a way a body
/// can resemble a request comment without being one, and reading such a body as
/// a request would attach an approval to a question nobody asked.
///
/// Two markers is a refusal rather than a choice between them. There is no
/// principled way to pick — first is not more authoritative than last — and a
/// body carrying two is a body somebody assembled, which is precisely the case
/// not to interpret.
///
/// The version is compared before the rest of the grammar is applied, not after.
/// A marker from a later build is entitled to a shape this one does not know, so
/// counting its fields would report a malformed marker when the truth is a
/// version this build cannot read. Refusing by version says the thing a reader
/// can act on.
///
/// What *is* checked before that comparison is the version token's own shape,
/// because the comparison is otherwise a catch-all: whatever a mangled body
/// leaves in the first position is not `v1`, so a doubled space, an embedded
/// newline or a dropped token would each be reported as a foreign version and
/// send an operator to upgrade a build that is not the problem. See
/// `is_version_token` for why that one token's spelling is fixed across
/// versions while the rest of the grammar is not.
pub fn parse_marker(body: &str) -> Result<DecisionBinding, MarkerError> {
    let mut openings = body.match_indices(OPENING);
    let Some((start, _)) = openings.next() else {
        return Err(MarkerError::Absent);
    };
    if openings.next().is_some() {
        return Err(MarkerError::Malformed(
            "a body carrying two markers is not a body to choose between".to_string(),
        ));
    }

    let rest = &body[start + OPENING.len()..];
    let Some(end) = rest.find(CLOSING) else {
        return Err(MarkerError::Malformed(format!(
            "a marker opens and is never closed by {CLOSING:?}"
        )));
    };
    let inside = &rest[..end];

    // Split on a single space rather than on whitespace: the format writes one,
    // so a run of spaces or a newline inside the marker is a body that was
    // reflowed or assembled rather than one fiddle wrote.
    let mut tokens = inside.split(' ');
    let version = tokens.next().unwrap_or_default();
    if !is_version_token(version) {
        return Err(MarkerError::Malformed(format!(
            "a marker opens with a version token like {MARKER_VERSION:?}, not {version:?}"
        )));
    }
    if version != MARKER_VERSION {
        return Err(MarkerError::Version(version.to_string()));
    }

    let mut values = Vec::with_capacity(FIELDS.len());
    for (key, width) in FIELDS {
        let Some(token) = tokens.next() else {
            return Err(MarkerError::Malformed(format!("no {key} field")));
        };
        let Some(value) = token.strip_prefix(key).and_then(|v| v.strip_prefix('=')) else {
            return Err(MarkerError::Malformed(format!(
                "expected {key}= where the marker has {token:?}"
            )));
        };
        // Lowercase spelled out as an alphabet rather than as
        // `is_ascii_hexdigit`, which accepts both cases: one digest has one
        // spelling, and a marker offering another was not written by this build.
        let lowercase_hex = value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if value.len() != width || !lowercase_hex {
            return Err(MarkerError::Malformed(format!(
                "{key} must be {width} lowercase hex characters, not {value:?}"
            )));
        }
        values.push(value);
    }
    if let Some(extra) = tokens.next() {
        return Err(MarkerError::Malformed(format!(
            "a marker carries the four fields and nothing else, but this one has {extra:?}"
        )));
    }

    Ok(DecisionBinding {
        request: DecisionRequestId(values[0].to_string()),
        effect: EffectId(values[1].to_string()),
        payload: PayloadHash(values[2].to_string()),
        head_sha: values[3].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> DecisionBinding {
        DecisionBinding {
            request: DecisionRequestId("0123456789abcdef".into()),
            effect: EffectId("fedcba9876543210".into()),
            payload: PayloadHash("00112233445566aa".into()),
            head_sha: "a".repeat(40),
        }
    }

    /// One whole request, for the two tests that assert over its serialized form.
    ///
    /// Every container is deliberately **non-empty** — `work_ref` is `Some`, and
    /// the three vectors each hold one element. Both tests walk the document, and
    /// a walk can only look inside a container that has something in it, so a
    /// fixture emptied here would silently stop covering those arms. The shape test
    /// asserts that it is populated rather than trusting this comment.
    fn request() -> HumanDecisionRequest {
        HumanDecisionRequest {
            invocation_ref: "beans:w-1".to_string(),
            work_ref: Some(WorkRef("w-1".to_string())),
            capability: crate::PUBLISH_CHANGE,
            binding: binding(),
            question: "May fiddle mark this ready for review?".to_string(),
            rationale: "The check passed at this revision.".to_string(),
            risks: vec!["review notifications reach the team".to_string()],
            alternatives: vec!["leave it a draft and revisit".to_string()],
            evidence: vec![EvidenceRef("check=pass".to_string())],
        }
    }

    /// The round trip is half the contract: what this build writes, this build
    /// reads. Only half, because both halves move together — see
    /// [`the_rendered_marker_is_pinned_byte_for_byte`].
    #[test]
    fn a_marker_round_trips_through_a_comment_body() {
        let b = binding();
        let body = format!("Some prose a person reads.\n\n{}\n", render_marker(&b));
        assert_eq!(parse_marker(&body).unwrap(), b);
    }

    /// The marker is a wire format read across builds as well as across a process
    /// death: an earlier build writes it into a GitHub conversation and a later
    /// one has to find it there. So the bytes are pinned against a literal. A
    /// round trip cannot do this job — permute the field order or rename a key
    /// and render and parse move together, leaving every marker already posted
    /// unreadable with the suite still green.
    ///
    /// If this test fails, the format changed. That is allowed, and it costs a
    /// [`MARKER_VERSION`] bump plus a decision about the markers already out
    /// there. What is not allowed is for it to change without anybody noticing.
    #[test]
    fn the_rendered_marker_is_pinned_byte_for_byte() {
        assert_eq!(
            render_marker(&binding()),
            "<!-- fiddle:decision v1 \
             request=0123456789abcdef \
             effect=fedcba9876543210 \
             payload=00112233445566aa \
             head=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -->",
        );
    }

    /// The other half of the same contract: a marker an earlier build wrote is a
    /// marker this one still reads, out of a body with prose around it.
    ///
    /// The literal below is a fixture typed by hand, not `render_marker`'s output
    /// pasted in. Pasting would make this the round trip again, and it would then
    /// agree with whatever format the renderer had drifted to.
    #[test]
    fn a_marker_from_an_earlier_build_still_parses() {
        let body = "Looks fine to me, go ahead.\n\n\
                    <!-- fiddle:decision v1 \
                    request=0123456789abcdef \
                    effect=fedcba9876543210 \
                    payload=00112233445566aa \
                    head=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa -->\n";
        assert_eq!(parse_marker(body), Ok(binding()));
    }

    /// It is an HTML comment so a reader of the conversation never sees it.
    #[test]
    fn a_marker_is_an_html_comment() {
        let m = render_marker(&binding());
        assert!(m.starts_with("<!-- fiddle:decision v1 "), "got {m}");
        assert!(m.ends_with(" -->"), "got {m}");
    }

    /// A body with no marker is not an error worth a diagnostic — it is every
    /// other comment in the conversation.
    #[test]
    fn an_ordinary_comment_is_absent_rather_than_malformed() {
        assert_eq!(parse_marker("looks good to me"), Err(MarkerError::Absent));
    }

    /// One row of a refusal table: the case's name, the body to parse, and the
    /// message the refusal should carry.
    type Case = (&'static str, String, String);

    /// A whole table's outcome, named per case so a diff says which row moved
    /// rather than only that something did.
    type Refusals = Vec<(&'static str, String)>;

    /// Every case's refusal, gathered before any of them is asserted, paired with
    /// what it should have been.
    ///
    /// A `for` loop of `assert_eq!` over a table stops at the earliest
    /// divergence. It *catches* any single case that regresses — either form
    /// does — but it cannot **report** a second one in the same run, so a claim
    /// quantified over three bodies was never something one run could evidence.
    /// That is not hypothetical here: the claim that three mangled bodies each
    /// used to refuse as [`MarkerError::Version`] had to be checked by hand in a
    /// scratch tree, one body at a time, because the loop could name only one of
    /// them. And the run that mattered was the one where all three regressed
    /// together, which is the shape of a removed guard.
    ///
    /// So the whole table is compared in one [`assert_eq!`]: a run prints every
    /// case, named, so a reader can see which ones moved rather than only the
    /// earliest one to move. The refusal is reduced to the reason a
    /// [`MarkerError::Malformed`] carries, and anything that is not one is
    /// spelled out with `{:?}` instead, so a body that started being accepted —
    /// or refused by a different variant — shows up as itself in the diff rather
    /// than as a missing reason.
    fn refusals(cases: &[Case]) -> (Refusals, Refusals) {
        let observed = cases
            .iter()
            .map(|(name, body, _)| {
                let seen = match parse_marker(body) {
                    Err(MarkerError::Malformed(why)) => why,
                    other => format!("{other:?}"),
                };
                (*name, seen)
            })
            .collect();
        let wanted = cases
            .iter()
            .map(|(name, _, want)| (*name, want.clone()))
            .collect();
        (observed, wanted)
    }

    /// Strictness, case by case. A body that half-matches is far more likely to
    /// be a person quoting the marker than a request comment, so each of these
    /// refuses rather than being interpreted.
    ///
    /// Each case asserts the message it earns rather than `Malformed(_)`.
    /// `Malformed(_)` cannot say *which* check refused, so it passes just as
    /// happily when some neighbouring check catches the case first and the one
    /// under test is dead — and this module's refusals are a diagnosis an
    /// operator reads and acts on, not an opaque no. All eight are observed on
    /// every run; see [`refusals`].
    #[test]
    fn a_half_matching_marker_is_refused_and_not_interpreted() {
        let ok = render_marker(&binding());
        let (observed, wanted) = refusals(&[
            (
                "truncated request",
                ok.replace("0123456789abcdef", "0123456789abcde"),
                r#"request must be 16 lowercase hex characters, not "0123456789abcde""#.to_string(),
            ),
            (
                "uppercase hex",
                ok.replace("fedcba9876543210", "FEDCBA9876543210"),
                r#"effect must be 16 lowercase hex characters, not "FEDCBA9876543210""#.to_string(),
            ),
            (
                "short head",
                ok.replace(&"a".repeat(40), &"a".repeat(39)),
                format!(
                    "head must be 40 lowercase hex characters, not {:?}",
                    "a".repeat(39)
                ),
            ),
            // Over-long as well as truncated. A width is checked with equality
            // rather than a minimum, so a field cannot be padded past its own
            // length and still be read as a digest this build produced.
            (
                "over-long request",
                ok.replace("0123456789abcdef", "0123456789abcdef0"),
                r#"request must be 16 lowercase hex characters, not "0123456789abcdef0""#
                    .to_string(),
            ),
            (
                "long head",
                ok.replace(&"a".repeat(40), &"a".repeat(41)),
                format!(
                    "head must be 40 lowercase hex characters, not {:?}",
                    "a".repeat(41)
                ),
            ),
            (
                "reordered keys",
                ok.replace("request=", "zzz=")
                    .replace("effect=", "request="),
                r#"expected request= where the marker has "zzz=0123456789abcdef""#.to_string(),
            ),
            (
                "extra key",
                ok.replace(" -->", " extra=1 -->"),
                r#"a marker carries the four fields and nothing else, but this one has "extra=1""#
                    .to_string(),
            ),
            (
                "non-hex",
                ok.replace("00112233445566aa", "00112233445566zz"),
                r#"payload must be 16 lowercase hex characters, not "00112233445566zz""#
                    .to_string(),
            ),
        ]);
        assert_eq!(observed, wanted);
    }

    /// The ways a body gets damaged between being written and being read again —
    /// respaced, reflowed by an editor, its version token dropped, truncated, its
    /// closing lost — and the refusal each one earns. Five kinds, listed in the
    /// order the table below runs them.
    ///
    /// All five are [`MarkerError::Malformed`] and none is
    /// [`MarkerError::Version`]. **Three** were `Version` before this test
    /// existed: the respacing, the reflow and the dropped token — the three that
    /// corrupt whatever lands in the version token's position — because such a
    /// body's first token is not `v1` either, and the version comparison stood
    /// where anything unrecognised fell through. The other two were already
    /// `Malformed`, because they damage the marker somewhere the version check
    /// never looks: the truncation leaves `v1` intact and runs out of fields, and
    /// the lost closing refuses before there is anything to tokenise. Nothing
    /// unsafe was accepted either way; the refusal simply told an operator whose
    /// comment had been reflowed to upgrade their build, which is a day spent on
    /// the wrong thing. The message is asserted per case for that exact reason:
    /// the bug was never *whether* these refuse.
    ///
    /// That "three" is a claim about three separate bodies, so all three are
    /// observed on every run rather than the first one shielding the others — see
    /// [`refusals`] for why a `for` loop of assertions could not carry that claim.
    #[test]
    fn a_mangled_body_is_malformed_and_says_how() {
        let ok = render_marker(&binding());
        let (observed, wanted) = refusals(&[
            (
                "a doubled space before the version",
                ok.replace("fiddle:decision v1", "fiddle:decision  v1"),
                r#"a marker opens with a version token like "v1", not """#.to_string(),
            ),
            (
                "a newline reflowed into the marker",
                ok.replace("v1 request=", "v1\nrequest="),
                r#"a marker opens with a version token like "v1", not "v1\nrequest=0123456789abcdef""#
                    .to_string(),
            ),
            (
                "the version token dropped",
                ok.replace("v1 ", ""),
                r#"a marker opens with a version token like "v1", not "request=0123456789abcdef""#
                    .to_string(),
            ),
            (
                "a trailing field truncated away",
                ok.replace(&format!(" head={}", "a".repeat(40)), ""),
                "no head field".to_string(),
            ),
            (
                // The strictness argument cites this case and nothing exercised
                // it: an opening with no closing is a fragment, and reading to
                // the end of the body instead would let the four fields be
                // gathered from arbitrary prose.
                "an opening never closed",
                ok.replace(" -->", ""),
                r#"a marker opens and is never closed by " -->""#.to_string(),
            ),
        ]);
        assert_eq!(observed, wanted);
    }

    /// The version is compared, not skipped. A build meeting a marker it does
    /// not understand refuses rather than guessing — the argument
    /// [`DeploymentRule`](crate::DeploymentRule) makes for having no catch-all
    /// variant.
    ///
    /// The second case is the one that fixes the meaning of the first. A later
    /// build's marker is entitled to a grammar this build cannot parse, so the
    /// version is compared *before* the fields are counted, and a body carrying
    /// nothing else this build understands still refuses by version rather than
    /// as malformed. That is why the token's shape is what admits it to this
    /// comparison, and why the mangled bodies above are not admitted to it.
    #[test]
    fn an_unknown_marker_version_is_refused_by_version() {
        let bad = render_marker(&binding()).replace("v1", "v2");
        assert_eq!(parse_marker(&bad), Err(MarkerError::Version("v2".into())));
        assert_eq!(
            parse_marker("<!-- fiddle:decision v10 whatever-a-later-build-writes -->"),
            Err(MarkerError::Version("v10".into())),
        );
    }

    /// What a successful parse does not mean, asserted rather than only written
    /// down: an *edited* quotation of a request comment parses, and binds
    /// somewhere else. Well-formedness is a property of the four fields' shapes
    /// and never of their values, so any four plausible digests satisfy it.
    ///
    /// This is not a defect to fix here. A pure module has no run to compare
    /// against and no forge to ask who wrote a comment, and a rule invented here
    /// to guess at authorship would be satisfiable by whoever knew the rule. It
    /// is recorded as a test because the safety of the continuation rests on
    /// comparing every field against a value it obtained for itself — and, for
    /// the effect id, on that comparison alone — never on a body having parsed. A
    /// caller who read `is_ok()` as "this is my request" would be handed whatever
    /// the editor typed.
    #[test]
    fn an_edited_quotation_parses_and_binds_somewhere_else() {
        let edited = render_marker(&binding()).replace("fedcba9876543210", "0000000000000001");
        let parsed = parse_marker(&edited).expect("a well-formed marker parses, whoever wrote it");
        assert_eq!(parsed.effect, EffectId("0000000000000001".into()));
        assert_ne!(parsed, binding());
        // And the verbatim quotation is not even distinguishable in principle:
        // byte-identical input, identical binding.
        assert_eq!(
            parse_marker(&format!("> {}", render_marker(&binding()))),
            Ok(binding()),
        );
    }

    /// Two markers in one body is not a body to pick from.
    #[test]
    fn two_markers_in_one_body_are_malformed() {
        let m = render_marker(&binding());
        let bad = format!("{m}\n{m}");
        assert_eq!(
            parse_marker(&bad),
            Err(MarkerError::Malformed(
                "a body carrying two markers is not a body to choose between".to_string()
            )),
        );
    }

    /// The request id derives from the effect it gates, which is what makes it
    /// recomputable by a process that kept nothing.
    #[test]
    fn a_request_id_derives_from_the_effect_it_gates() {
        let e = EffectId("fedcba9876543210".into());
        let a = decision_request_id("proj", "beans:x", &e);
        assert_eq!(a, decision_request_id("proj", "beans:x", &e));
        assert_eq!(a.0.len(), 16);
        assert!(a
            .0
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
        // A different effect is a different question. This is the property §5.8
        // rests on: a moved head derives a new effect, so a new request.
        assert_ne!(
            a,
            decision_request_id("proj", "beans:x", &EffectId("0".repeat(16)))
        );
        assert_ne!(a, decision_request_id("proj", "beans:y", &e));
        assert_ne!(a, decision_request_id("other", "beans:x", &e));
    }

    /// Two calls agreeing inside one process only shows the function is not
    /// obviously stateful; it would still agree after a refactor that moved the
    /// construction. The digest below was produced outside this crate —
    ///
    /// ```text
    /// printf '4:proj7:beans:x16:fedcba9876543210' | b3sum
    /// ```
    ///
    /// — so it pins the *definition* rather than whatever this implementation
    /// happens to compute, which is what the recomputation actually rests on. A
    /// continuation is a later build as well as a later process, and a request
    /// id that moved between builds would fail to find the question the earlier
    /// build had already asked, and ask it again.
    #[test]
    fn a_request_id_is_pinned_to_an_independently_computed_digest() {
        assert_eq!(
            decision_request_id("proj", "beans:x", &EffectId("fedcba9876543210".into())),
            DecisionRequestId("4ff7bf7b0d873a41".to_string())
        );
    }

    /// Length-prefixed framing, for [`effect_id`](crate::effect_id)'s reason: no
    /// field's contents can be mistaken for structure. Injectivity, checked
    /// rather than argued.
    #[test]
    fn no_field_boundary_can_be_forged_by_a_fields_contents() {
        let e = EffectId("f".repeat(16));
        assert_ne!(
            decision_request_id("a", "bc", &e),
            decision_request_id("ab", "c", &e),
        );
    }

    /// Every path in `value` whose string leaf is `needle`, dotted through objects
    /// and indexed through arrays.
    ///
    /// The paths and not the count, so a failure says *where* the duplicate is, and
    /// so that a copy which moves the id somewhere else fails rather than counting
    /// to one from the wrong place.
    fn paths_holding(value: &serde_json::Value, needle: &str, at: &str, found: &mut Vec<String>) {
        let below = |key: &str| {
            if at.is_empty() {
                key.to_string()
            } else {
                format!("{at}.{key}")
            }
        };
        match value {
            serde_json::Value::String(leaf) if leaf == needle => found.push(at.to_string()),
            serde_json::Value::Object(fields) => {
                for (key, nested) in fields {
                    paths_holding(nested, needle, &below(key), found);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, nested) in items.iter().enumerate() {
                    paths_holding(nested, needle, &below(&index.to_string()), found);
                }
            }
            _ => {}
        }
    }

    /// The request id is held in **exactly one place**, and that is asserted rather
    /// than described.
    ///
    /// [`HumanDecisionRequest`] carried the id twice until `fiddle-11vj` — once as
    /// its own field and once in [`DecisionBinding`] — with only the binding
    /// reaching the marker, because [`render_marker`] takes the binding. A producer
    /// filling the two from two derivations, or a consumer matching on the copy,
    /// publishes a marker naming one question and then searches for another: it
    /// finds nothing, concludes it has not asked yet, and **posts again on every
    /// attempt, forever**.
    ///
    /// # Why this test is about the type's shape and not its behaviour
    ///
    /// Deleting the field made that bug unwritable, and no *behavioural* test can
    /// fail without the deletion — after it, the divergence cannot be constructed,
    /// so there is no run to observe going wrong. It does not follow that no test
    /// can fail without it: the type derives [`serde::Serialize`], which makes its
    /// **shape** observable from outside with no behaviour involved. So the guard
    /// against somebody re-adding the field is a red test rather than a review
    /// comment, and it lives here in `fiddle-core` beside the type it constrains.
    ///
    /// # The key set, the nested walk, and deliberately not an occurrence count
    ///
    /// Counting how often the id appears in the serialized document and requiring
    /// one **passes a second copy that disagrees** — which is the dangerous case
    /// rather than the harmless one, since a copy equal to the binding's id posts
    /// nothing wrong and a copy that differs is what posts forever. That is what a
    /// count cannot do, and it is the whole of it: an earlier version of this
    /// comment drew a wider conclusion — *"a count is strictly weaker than the key
    /// set"* — and that is false. The key set inspects **top-level fields only**, so
    /// it is blind to the case the count was written for, a second copy carried by a
    /// nested value. Two assertions follow, and neither covers the other:
    ///
    /// * the key set fails on any second **top-level** field whatever its value,
    ///   including one that disagrees with the binding, which is what a count would
    ///   wave through;
    /// * the walk fails on a copy nested at any depth **whose value agrees** with
    ///   `binding.request`, and it asserts the path rather than the number, so
    ///   moving the id out of the binding fails too instead of counting to one from
    ///   the wrong place.
    ///
    /// What neither refuses is a copy that is *both* nested *and* disagreeing: a
    /// value-equality walk cannot see a value it is not equal to, and nothing here
    /// gates [`DecisionBinding`]'s own serialized key set. That was recorded as this
    /// test's residual rather than its gap, measured — the mutation passes here,
    /// 64/0 — and it is the reason the count was not simply restored, since a count
    /// sees that case no better and the disagreeing top-level copy worse.
    ///
    /// **It is now closed, and not here.**
    /// `every_object_in_a_serialized_request_holds_exactly_the_fields_it_declares`
    /// pins every object in the document to its declared field set, which refuses
    /// the fifth field whatever value it carries. That test overlaps this one's key
    /// set deliberately and is kept separate for a reason worth stating: folded in
    /// above these two assertions it would fail first on both of their cases and
    /// take the credit, and a later reader deleting one of them would see nothing go
    /// red. Two tests, three cases, and each assertion still fails on its own —
    /// which is measured rather than asserted here, by re-running the nested-equal
    /// and top-level-disagreeing mutations after the shape guard was added.
    ///
    /// All of this is worth the words because the assertions would look
    /// interchangeable to somebody tidying this test, and the person most likely to
    /// edit a guard against re-adding the field is the person re-adding it. Keeping
    /// either one alone would leave the guard green for a change it exists to refuse.
    #[test]
    fn the_request_id_is_held_in_exactly_one_place() {
        let request = request();

        let document = serde_json::to_value(&request).expect("the type derives Serialize");
        let mut fields: Vec<&str> = document
            .as_object()
            .expect("a struct serializes as an object")
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "alternatives",
                "binding",
                "capability",
                "evidence",
                "invocation_ref",
                "question",
                "rationale",
                "risks",
                "work_ref",
            ],
            "the request id belongs to the binding alone: a top-level copy of it is \
             what published a marker naming one question and then looked for another"
        );

        let mut carriers: Vec<String> = Vec::new();
        paths_holding(
            &document,
            request.binding.request.0.as_str(),
            "",
            &mut carriers,
        );
        carriers.sort_unstable();
        assert_eq!(
            carriers,
            ["binding.request"],
            "and nothing nested repeats it either: the key set above sees top-level \
             fields only, so a copy carried by a nested value is the same \
             forever-posting hazard one level below where that assertion can look"
        );
    }

    /// Every object in `value`, as its dotted path and its own sorted key set.
    ///
    /// Objects rather than leaves, because the question is what a document may
    /// *contain* and not what it happens to hold — which is the difference between
    /// this and [`paths_holding`], and the reason a value that disagrees is visible
    /// here and invisible there.
    ///
    /// Arrays are indexed and descended into, so a struct that appeared inside
    /// `risks` or `evidence` would be named rather than skipped. The root's path is
    /// the empty string; `root` would be indistinguishable from a field called
    /// `root`. Keys are sorted explicitly even though `serde_json`'s map is ordered
    /// today, because that ordering is a crate feature away from being insertion
    /// order and this assertion should not depend on which.
    fn object_shapes(value: &serde_json::Value, at: &str, found: &mut Vec<(String, Vec<String>)>) {
        let below = |key: &str| {
            if at.is_empty() {
                key.to_string()
            } else {
                format!("{at}.{key}")
            }
        };
        match value {
            serde_json::Value::Object(fields) => {
                let mut keys: Vec<String> = fields.keys().cloned().collect();
                keys.sort_unstable();
                found.push((at.to_string(), keys));
                for (key, nested) in fields {
                    object_shapes(nested, &below(key), found);
                }
            }
            serde_json::Value::Array(items) => {
                for (index, nested) in items.iter().enumerate() {
                    object_shapes(nested, &below(&index.to_string()), found);
                }
            }
            _ => {}
        }
    }

    /// A field nobody declared has nowhere to appear, at any depth and whatever it
    /// carries.
    ///
    /// # The case this exists for
    ///
    /// `the_request_id_is_held_in_exactly_one_place` holds two assertions and they
    /// leave one gap between them, which that test records as its residual: a second
    /// copy of the request id that is **both nested and disagreeing**. The key set
    /// there reads top-level fields only, so it cannot see inside
    /// [`DecisionBinding`]; the value walk there compares against
    /// `binding.request`'s value, so it cannot see a copy that differs from it. A
    /// disagreeing copy is the dangerous one — an equal copy publishes nothing wrong,
    /// and a copy that differs is what makes a producer publish a marker naming one
    /// question while a consumer searches for another, find nothing, conclude it has
    /// not asked, and post again on every attempt forever.
    ///
    /// So the guard here is not about the id at all: it is about **shape**, which is
    /// what makes it blind to the value and therefore able to refuse every value.
    /// Both types' field sets are pinned, `HumanDecisionRequest`'s nine and
    /// `DecisionBinding`'s four, and the walk that produces them is total — a struct
    /// appearing anywhere in the document, including inside one of the vectors, is a
    /// new object with a new path and fails this.
    ///
    /// # Why it is its own test
    ///
    /// It overlaps the key-set assertion in the other test on purpose and must not
    /// be folded into it. An inversion is caught by whichever assertion fires first,
    /// so a total shape guard placed above those two would fail on their cases as
    /// well as on this one, and somebody deleting either of them afterwards would see
    /// nothing turn red. Separate tests mean all three cases are attributed, and that
    /// was measured after this was written rather than argued: the nested-equal and
    /// top-level-disagreeing mutations still fail their own assertions over there.
    #[test]
    fn every_object_in_a_serialized_request_holds_exactly_the_fields_it_declares() {
        let request = request();
        let document = serde_json::to_value(&request).expect("the type derives Serialize");

        // The walk's denominator. It descends into containers, so a container with
        // nothing in it is a place it cannot look — and a fixture emptied here would
        // narrow this assertion without failing it, which is the shape of null this
        // test exists to close in the first place.
        assert!(
            request.work_ref.is_some()
                && !request.risks.is_empty()
                && !request.alternatives.is_empty()
                && !request.evidence.is_empty(),
            "the fixture must populate every container, or the walk below stops \
             covering the paths that run through them and says nothing about it"
        );

        let mut shapes: Vec<(String, Vec<String>)> = Vec::new();
        object_shapes(&document, "", &mut shapes);
        shapes.sort();
        let observed: Vec<(&str, Vec<&str>)> = shapes
            .iter()
            .map(|(at, keys)| (at.as_str(), keys.iter().map(String::as_str).collect()))
            .collect();

        assert_eq!(
            observed,
            vec![
                (
                    "",
                    vec![
                        "alternatives",
                        "binding",
                        "capability",
                        "evidence",
                        "invocation_ref",
                        "question",
                        "rationale",
                        "risks",
                        "work_ref",
                    ]
                ),
                ("binding", vec!["effect", "head_sha", "payload", "request"]),
            ],
            "these two objects are the whole of a serialized request, and their field \
             sets are the whole of what it may carry. A path that is not here is a \
             type that grew a field or a newtype that stopped being transparent; a \
             key that is not here is somewhere to put a second copy of the request \
             id, which is the hazard `the_request_id_is_held_in_exactly_one_place` \
             describes and the one value it cannot see"
        );
    }
}
