use crate::identity::CapabilityId;

#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct EffectId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct PayloadHash(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EffectName(String);

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
#[error("`{0}` is not an effect name: use lowercase ASCII letters, digits, `_` and `.`")]
pub struct EffectNameError(String);

impl EffectName {
    pub fn parse(text: &str) -> Result<Self, EffectNameError> {
        let legal = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.';
        match !text.is_empty() && text.chars().all(legal) {
            true => Ok(EffectName(text.to_string())),
            false => Err(EffectNameError(text.to_string())),
        }
    }

    pub fn shipped(spelling: &'static str) -> Self {
        EffectName::parse(spelling)
            .expect("a spelling this build ships must parse under the grammar")
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EffectName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for EffectName {
    type Error = EffectNameError;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        EffectName::parse(&text)
    }
}

impl From<EffectName> for String {
    fn from(name: EffectName) -> String {
        name.0
    }
}

pub const ENSURE_BRANCH_PUBLISHED: &str = "ensure_branch_published";
pub const ENSURE_PULL_REQUEST: &str = "ensure_pull_request";
pub const ENSURE_CHECK_REQUESTED: &str = "ensure_check_requested";
pub const PUBLISH_DECISION_REQUEST: &str = "publish_decision_request";
pub const ENSURE_PULL_REQUEST_READY: &str = "ensure_pull_request_ready";
pub const ENSURE_PULL_REQUEST_BODY: &str = "ensure_pull_request_body";

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ProposedEffect {
    pub capability: CapabilityId,
    pub kind: EffectName,
    pub target: String,
    pub payload: String,
}

pub fn effect_id(project: &str, invocation_ref: &str, kind: &str, target: &str) -> EffectId {
    let material = length_prefixed([project, invocation_ref, kind, target]);
    EffectId(truncated_digest(&material))
}

pub fn payload_hash(payload: &str) -> PayloadHash {
    PayloadHash(truncated_digest(payload))
}

pub fn content_digest(content: &str) -> String {
    truncated_digest(content)
}

pub(crate) fn length_prefixed<const N: usize>(fields: [&str; N]) -> String {
    let mut material = String::new();
    for field in fields {
        material.push_str(&field.len().to_string());
        material.push(':');
        material.push_str(field);
    }
    material
}

pub(crate) fn truncated_digest(material: &str) -> String {
    blake3::hash(material.as_bytes()).to_hex()[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPELLINGS: [&str; 6] = [
        ENSURE_BRANCH_PUBLISHED,
        ENSURE_PULL_REQUEST,
        ENSURE_CHECK_REQUESTED,
        PUBLISH_DECISION_REQUEST,
        ENSURE_PULL_REQUEST_READY,
        ENSURE_PULL_REQUEST_BODY,
    ];

    #[test]
    fn an_effect_id_is_recomputable_from_canonical_inputs_alone() {
        let first = effect_id(
            "acme/widget",
            "beans:w-1",
            ENSURE_PULL_REQUEST,
            "main<-fiddle/abc",
        );
        let second = effect_id(
            "acme/widget",
            "beans:w-1",
            ENSURE_PULL_REQUEST,
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

    #[test]
    fn an_effect_id_is_pinned_to_an_independently_computed_digest() {
        assert_eq!(
            effect_id(
                "acme/widget",
                "beans:w-1",
                ENSURE_PULL_REQUEST,
                "main<-fiddle/abc"
            ),
            EffectId("39b2e77d1d17cb20".to_string())
        );
        assert_eq!(
            payload_hash(r#"{"title":"a"}"#),
            PayloadHash("7950cf4c9a0b76f2".to_string())
        );
    }

    #[test]
    fn the_hashed_name_and_the_serialized_name_are_the_same_spelling() {
        for spelling in SPELLINGS {
            let name = EffectName::parse(spelling).unwrap();
            assert_eq!(
                serde_json::to_value(&name).unwrap(),
                serde_json::json!(spelling),
                "serde and as_str must agree for {spelling}"
            );
            assert_eq!(name.as_str(), spelling);
        }
    }

    #[test]
    fn a_name_survives_a_round_trip_through_the_wire() {
        for spelling in SPELLINGS {
            let name = EffectName::parse(spelling).unwrap();
            let wire = serde_json::to_string(&name).unwrap();
            assert_eq!(serde_json::from_str::<EffectName>(&wire).unwrap(), name);
        }
        assert!(serde_json::from_str::<EffectName>("\"Ensure_Pull_Request\"").is_err());
    }

    #[test]
    fn every_name_has_a_distinct_wire_spelling() {
        let mut seen = std::collections::BTreeSet::new();
        for spelling in SPELLINGS {
            assert!(seen.insert(spelling), "{spelling} is spelled twice");
        }
        assert_eq!(seen.len(), SPELLINGS.len());
    }

    #[test]
    fn every_frozen_spelling_parses_under_the_grammar() {
        for spelling in SPELLINGS {
            assert!(
                EffectName::parse(spelling).is_ok(),
                "{spelling} ships in this build and no document could spell it"
            );
            assert_eq!(EffectName::shipped(spelling).as_str(), spelling);
        }
    }

    #[test]
    fn the_body_name_is_spelled_ensure_pull_request_body() {
        assert_eq!(ENSURE_PULL_REQUEST_BODY, "ensure_pull_request_body");
        assert!(SPELLINGS.contains(&ENSURE_PULL_REQUEST_BODY));
    }

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

    #[test]
    fn a_content_digest_is_pinned_to_the_same_published_width() {
        assert_eq!(content_digest(r#"{"title":"a"}"#), "7950cf4c9a0b76f2");
    }

    #[test]
    fn a_kind_participates_in_the_identity() {
        let a = effect_id("p", "beans:x", PUBLISH_DECISION_REQUEST, "acme/r#7");
        let b = effect_id("p", "beans:x", ENSURE_PULL_REQUEST_READY, "acme/r#7");
        assert_ne!(a, b);
    }

    #[test]
    fn every_canonical_input_changes_the_identity() {
        let base = effect_id("acme/widget", "beans:w-1", ENSURE_PULL_REQUEST, "t");
        assert_ne!(
            base,
            effect_id("acme/other", "beans:w-1", ENSURE_PULL_REQUEST, "t")
        );
        assert_ne!(
            base,
            effect_id("acme/widget", "beans:w-2", ENSURE_PULL_REQUEST, "t")
        );
        assert_ne!(
            base,
            effect_id("acme/widget", "beans:w-1", ENSURE_BRANCH_PUBLISHED, "t")
        );
        assert_ne!(
            base,
            effect_id("acme/widget", "beans:w-1", ENSURE_PULL_REQUEST, "u")
        );
    }

    #[test]
    fn an_embedded_nul_cannot_forge_a_shared_identity() {
        let kind = ENSURE_PULL_REQUEST;
        let collide_under_nul_joining = [
            (("a\0b", "c", "t"), ("a", "b\0c", "t")),
            (
                ("a", "b\0ensure_pull_request", "t"),
                ("a", "b", "ensure_pull_request\0t"),
            ),
        ];

        for ((p1, r1, t1), (p2, r2, t2)) in collide_under_nul_joining {
            assert_eq!(
                format!("{p1}\0{r1}\0{kind}\0{t1}"),
                format!("{p2}\0{r2}\0{kind}\0{t2}"),
                "fixture must actually collide under NUL joining, or it proves nothing"
            );
            assert_ne!(
                effect_id(p1, r1, kind, t1),
                effect_id(p2, r2, kind, t2),
                "distinct effects must never share an identity, whatever bytes they carry"
            );
        }
    }

    #[test]
    fn a_field_that_mimics_the_framing_is_still_only_content() {
        let kind = ENSURE_CHECK_REQUESTED;
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

    #[test]
    fn adjacent_fields_cannot_be_confused() {
        assert_ne!(
            effect_id("ab", "c", ENSURE_CHECK_REQUESTED, "t"),
            effect_id("a", "bc", ENSURE_CHECK_REQUESTED, "t"),
        );
    }

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

    const PINNED_PULL_REQUEST_IDENTITY: &str = "42138fc13dc17d78";

    #[test]
    fn a_name_outside_the_grammar_is_refused() {
        assert!(EffectName::parse("ensure_pull_request").is_ok());
        assert!(EffectName::parse("jira.transition").is_ok());
        for bad in [
            "Ensure_Pull_Request",
            "ensure pull request",
            "ensure-pull-request",
            "",
            "a\0b",
        ] {
            assert!(EffectName::parse(bad).is_err(), "{bad:?} parsed");
        }
    }

    #[test]
    fn the_six_wire_spellings_are_frozen() {
        assert_eq!(
            [
                ENSURE_BRANCH_PUBLISHED,
                ENSURE_PULL_REQUEST,
                ENSURE_CHECK_REQUESTED,
                PUBLISH_DECISION_REQUEST,
                ENSURE_PULL_REQUEST_READY,
                ENSURE_PULL_REQUEST_BODY,
            ],
            [
                "ensure_branch_published",
                "ensure_pull_request",
                "ensure_check_requested",
                "publish_decision_request",
                "ensure_pull_request_ready",
                "ensure_pull_request_body",
            ]
        );
    }

    #[test]
    fn an_identity_is_unchanged_by_the_move_to_names() {
        assert_eq!(
            effect_id("acme/r", "beans:x", ENSURE_PULL_REQUEST, "acme/r#7").0,
            PINNED_PULL_REQUEST_IDENTITY
        );
    }
}
