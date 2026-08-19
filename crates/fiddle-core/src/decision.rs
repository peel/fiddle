use crate::effect::{length_prefixed, truncated_digest, EffectId, PayloadHash};
use crate::identity::{CapabilityId, WorkRef};
use crate::published::Published;
use crate::report::EvidenceRef;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct DecisionRequestId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct DecisionBinding {
    pub request: DecisionRequestId,
    pub effect: EffectId,
    pub payload: PayloadHash,
    pub head_sha: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ActorRef {
    pub id: u64,
    pub login: String,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum InterpretedHumanDecision {
    Approve,
    Reject { reason: Published },
    Redirect { instruction: Published },
    Unclear,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct HumanDecisionRequest {
    pub invocation_ref: String,
    pub work_ref: Option<WorkRef>,
    pub capability: CapabilityId,
    pub binding: DecisionBinding,
    pub question: String,
    pub rationale: String,
    pub risks: Vec<String>,
    pub alternatives: Vec<String>,
    pub evidence: Vec<EvidenceRef>,
}

pub const MARKER_VERSION: &str = "v1";

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum MarkerError {
    #[error("no fiddle decision marker in this body")]
    Absent,
    #[error("marker version {0} is not {MARKER_VERSION}")]
    Version(String),
    #[error("marker is malformed: {0}")]
    Malformed(String),
}

const OPENING: &str = "<!-- fiddle:decision ";

const CLOSING: &str = " -->";

const FIELDS: [(&str, usize); 4] = [
    ("request", 16),
    ("effect", 16),
    ("payload", 16),
    ("head", 40),
];

fn is_version_token(token: &str) -> bool {
    token
        .strip_prefix('v')
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

pub fn decision_request_id(
    project: &str,
    invocation_ref: &str,
    effect: &EffectId,
) -> DecisionRequestId {
    let material = length_prefixed([project, invocation_ref, effect.0.as_str()]);
    DecisionRequestId(truncated_digest(&material))
}

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

    #[test]
    fn a_marker_round_trips_through_a_comment_body() {
        let b = binding();
        let body = format!("Some prose a person reads.\n\n{}\n", render_marker(&b));
        assert_eq!(parse_marker(&body).unwrap(), b);
    }

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

    #[test]
    fn a_marker_is_an_html_comment() {
        let m = render_marker(&binding());
        assert!(m.starts_with("<!-- fiddle:decision v1 "), "got {m}");
        assert!(m.ends_with(" -->"), "got {m}");
    }

    #[test]
    fn an_ordinary_comment_is_absent_rather_than_malformed() {
        assert_eq!(parse_marker("looks good to me"), Err(MarkerError::Absent));
    }

    type Case = (&'static str, String, String);

    type Refusals = Vec<(&'static str, String)>;

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
                "an opening never closed",
                ok.replace(" -->", ""),
                r#"a marker opens and is never closed by " -->""#.to_string(),
            ),
        ]);
        assert_eq!(observed, wanted);
    }

    #[test]
    fn an_unknown_marker_version_is_refused_by_version() {
        let bad = render_marker(&binding()).replace("v1", "v2");
        assert_eq!(parse_marker(&bad), Err(MarkerError::Version("v2".into())));
        assert_eq!(
            parse_marker("<!-- fiddle:decision v10 whatever-a-later-build-writes -->"),
            Err(MarkerError::Version("v10".into())),
        );
    }

    #[test]
    fn an_edited_quotation_parses_and_binds_somewhere_else() {
        let edited = render_marker(&binding()).replace("fedcba9876543210", "0000000000000001");
        let parsed = parse_marker(&edited).expect("a well-formed marker parses, whoever wrote it");
        assert_eq!(parsed.effect, EffectId("0000000000000001".into()));
        assert_ne!(parsed, binding());
        assert_eq!(
            parse_marker(&format!("> {}", render_marker(&binding()))),
            Ok(binding()),
        );
    }

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
        assert_ne!(
            a,
            decision_request_id("proj", "beans:x", &EffectId("0".repeat(16)))
        );
        assert_ne!(a, decision_request_id("proj", "beans:y", &e));
        assert_ne!(a, decision_request_id("other", "beans:x", &e));
    }

    #[test]
    fn a_request_id_is_pinned_to_an_independently_computed_digest() {
        assert_eq!(
            decision_request_id("proj", "beans:x", &EffectId("fedcba9876543210".into())),
            DecisionRequestId("4ff7bf7b0d873a41".to_string())
        );
    }

    #[test]
    fn no_field_boundary_can_be_forged_by_a_fields_contents() {
        let e = EffectId("f".repeat(16));
        assert_ne!(
            decision_request_id("a", "bc", &e),
            decision_request_id("ab", "c", &e),
        );
    }

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

    #[test]
    fn every_object_in_a_serialized_request_holds_exactly_the_fields_it_declares() {
        let request = request();
        let document = serde_json::to_value(&request).expect("the type derives Serialize");

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
