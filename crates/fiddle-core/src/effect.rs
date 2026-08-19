use crate::identity::CapabilityId;

#[derive(Clone, Debug, Eq, PartialEq, Hash, serde::Serialize)]
#[serde(transparent)]
pub struct EffectId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct PayloadHash(pub String);

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    EnsureBranchPublished,
    EnsurePullRequest,
    EnsureCheckRequested,
    PublishDecisionRequest,
    EnsurePullRequestReady,
    EnsurePullRequestBody,
}

const KINDS: usize = 6;

impl EffectKind {
    pub const ALL: [EffectKind; KINDS] = Self::chain();

    const FIRST: EffectKind = EffectKind::EnsureBranchPublished;

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

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct ProposedEffect {
    pub capability: CapabilityId,
    pub kind: EffectKind,
    pub target: String,
    pub payload: String,
}

pub fn effect_id(project: &str, invocation_ref: &str, kind: EffectKind, target: &str) -> EffectId {
    let material = length_prefixed([project, invocation_ref, kind.as_str(), target]);
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

        for pair in EffectKind::ALL.windows(2) {
            assert_eq!(pair[0].next(), Some(pair[1]));
        }
    }

    #[test]
    fn the_body_kind_is_spelled_ensure_pull_request_body() {
        assert_eq!(
            EffectKind::EnsurePullRequestBody.as_str(),
            "ensure_pull_request_body"
        );
        assert!(EffectKind::ALL.contains(&EffectKind::EnsurePullRequestBody));
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

    #[test]
    fn an_embedded_nul_cannot_forge_a_shared_identity() {
        let kind = EffectKind::EnsurePullRequest;
        let collide_under_nul_joining = [
            (("a\0b", "c", "t"), ("a", "b\0c", "t")),
            (
                ("a", "b\0ensure_pull_request", "t"),
                ("a", "b", "ensure_pull_request\0t"),
            ),
        ];

        for ((p1, r1, t1), (p2, r2, t2)) in collide_under_nul_joining {
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

    #[test]
    fn adjacent_fields_cannot_be_confused() {
        assert_ne!(
            effect_id("ab", "c", EffectKind::EnsureCheckRequested, "t"),
            effect_id("a", "bc", EffectKind::EnsureCheckRequested, "t"),
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
}
