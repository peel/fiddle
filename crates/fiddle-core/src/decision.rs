//! The identity of a question put to a person, and the marker that carries it
//! across a process boundary.
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
//! later process recomputes from canonical inputs and then compares. A marker
//! that was edited, or written by somebody else, fails that comparison; it is
//! never believed because it was read. What the marker actually supplies is
//! narrower and cannot be derived: which question this comment is, so the
//! continuation knows what to recompute in the first place.
//!
//! [`parse_marker`] is therefore strict to the point of refusing anything it
//! does not recognise exactly. The bodies it runs against are ordinary
//! conversation, where a person may well quote a marker while discussing it, and
//! a lenient parser would read that quotation as a request. A body that
//! half-matches is refused rather than repaired, and a body with no marker at
//! all is [`MarkerError::Absent`] rather than an error, because that is what
//! every other comment in the conversation looks like.

use crate::effect::{length_prefixed, truncated_digest, EffectId, PayloadHash};

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

    /// The round trip is the whole contract: what is written is what is read.
    #[test]
    fn a_marker_round_trips_through_a_comment_body() {
        let b = binding();
        let body = format!("Some prose a person reads.\n\n{}\n", render_marker(&b));
        assert_eq!(parse_marker(&body).unwrap(), b);
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

    /// Strictness, case by case. A body that half-matches is far more likely to
    /// be a person quoting the marker than a request comment, so each of these
    /// refuses rather than being interpreted.
    #[test]
    fn a_half_matching_marker_is_refused_and_not_interpreted() {
        let ok = render_marker(&binding());
        for (name, bad) in [
            (
                "truncated request",
                ok.replace("0123456789abcdef", "0123456789abcde"),
            ),
            (
                "uppercase hex",
                ok.replace("fedcba9876543210", "FEDCBA9876543210"),
            ),
            ("short head", ok.replace(&"a".repeat(40), &"a".repeat(39))),
            // Over-long as well as truncated. A width is checked with equality
            // rather than a minimum, so a field cannot be padded past its own
            // length and still be read as a digest this build produced.
            (
                "over-long request",
                ok.replace("0123456789abcdef", "0123456789abcdef0"),
            ),
            ("long head", ok.replace(&"a".repeat(40), &"a".repeat(41))),
            (
                "reordered keys",
                ok.replace("request=", "zzz=")
                    .replace("effect=", "request="),
            ),
            ("extra key", ok.replace(" -->", " extra=1 -->")),
            (
                "non-hex",
                ok.replace("00112233445566aa", "00112233445566zz"),
            ),
        ] {
            assert!(
                matches!(parse_marker(&bad), Err(MarkerError::Malformed(_))),
                "{name} was accepted: {bad}"
            );
        }
    }

    /// The version is compared, not skipped. A build meeting a marker it does
    /// not understand refuses rather than guessing — the argument
    /// [`DeploymentRule`](crate::DeploymentRule) makes for having no catch-all
    /// variant.
    #[test]
    fn an_unknown_marker_version_is_refused_by_version() {
        let bad = render_marker(&binding()).replace("v1", "v2");
        assert_eq!(parse_marker(&bad), Err(MarkerError::Version("v2".into())));
    }

    /// Two markers in one body is not a body to pick from.
    #[test]
    fn two_markers_in_one_body_are_malformed() {
        let m = render_marker(&binding());
        let bad = format!("{m}\n{m}");
        assert!(matches!(parse_marker(&bad), Err(MarkerError::Malformed(_))));
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
}
