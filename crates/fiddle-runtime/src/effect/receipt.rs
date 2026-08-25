use super::AdapterError;
use fiddle_core::{EffectId, EffectName, PayloadHash};

pub trait ObservedState {
    type Value;

    fn describe(&self) -> String;

    fn reference(&self) -> Option<String>;

    fn into_value(self) -> Self::Value;
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct EffectReceipt<T> {
    pub effect_id: EffectId,
    pub payload_hash: PayloadHash,
    pub target: String,
    pub outcome: super::EffectOutcome,
    pub postcondition: String,
    pub external_ref: Option<String>,
    pub value: T,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Recurrence {
    Correctable,

    Permanent,

    Awaiting,
}

#[derive(Debug, thiserror::Error)]
pub enum EffectError {
    #[error("`{kind}` is not an effect this build performs")]
    UnknownEffect { kind: EffectName },
    #[error("`{kind}` could not be built from this step: {reason}")]
    Unbuildable { kind: EffectName, reason: String },
    #[error("policy denied {kind}: {reason}")]
    PolicyDenied { kind: EffectName, reason: String },
    #[error("{kind} is awaiting a human decision on the channel M3 introduced: {reason}")]
    HumanDecisionRequired { kind: EffectName, reason: String },
    #[error("{kind} left an unresolved outcome: {reason}")]
    Unresolved { kind: EffectName, reason: String },
    #[error(
        "{kind} was authorized for payload {} and would apply {}; nothing was performed",
        approved.0,
        applying.0
    )]
    PayloadDiverged {
        kind: EffectName,
        approved: PayloadHash,
        applying: PayloadHash,
    },
    #[error(
        "{kind} was proposed with {part} {proposed} and the operation carries {part} \
         {performing}; nothing was performed"
    )]
    IdentityDiverged {
        kind: EffectName,
        part: &'static str,
        proposed: String,
        performing: String,
    },
    #[error("{kind} found {count} matching objects, expected at most one")]
    DuplicateState { kind: EffectName, count: usize },
    #[error("adapter failure for {kind}: {source}")]
    Adapter {
        kind: EffectName,
        source: Box<dyn AdapterError>,
    },
}

impl EffectError {
    pub fn adapter_source<E: AdapterError>(&self) -> Option<&E> {
        match self {
            EffectError::Adapter { source, .. } => {
                (source.as_ref() as &dyn std::any::Any).downcast_ref::<E>()
            }
            _ => None,
        }
    }

    pub fn recurrence(&self) -> Recurrence {
        match self {
            EffectError::UnknownEffect { .. } => Recurrence::Permanent,

            EffectError::Unbuildable { .. } => Recurrence::Permanent,

            EffectError::PolicyDenied { .. } => Recurrence::Permanent,

            EffectError::HumanDecisionRequired { .. } => Recurrence::Awaiting,

            EffectError::PayloadDiverged { .. } => Recurrence::Permanent,

            EffectError::IdentityDiverged { .. } => Recurrence::Permanent,

            EffectError::DuplicateState { .. } => Recurrence::Permanent,

            EffectError::Unresolved { .. } => Recurrence::Correctable,

            EffectError::Adapter { .. } => Recurrence::Correctable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::GhError;

    fn kind() -> EffectName {
        EffectName::shipped(fiddle_core::ENSURE_PULL_REQUEST)
    }

    fn reason() -> String {
        "because".to_string()
    }

    #[test]
    fn every_effect_failure_declares_which_exit_row_it_belongs_in() {
        let cases: [(&str, EffectError, Recurrence); 9] = [
            (
                "no descriptor in this build holds the name",
                EffectError::UnknownEffect {
                    kind: EffectName::parse("jira.transition").unwrap(),
                },
                Recurrence::Permanent,
            ),
            (
                "the step named none of what the operation is made of",
                EffectError::Unbuildable {
                    kind: kind(),
                    reason: reason(),
                },
                Recurrence::Permanent,
            ),
            (
                "a deployment rule denies the kind",
                EffectError::PolicyDenied {
                    kind: kind(),
                    reason: reason(),
                },
                Recurrence::Permanent,
            ),
            (
                "a decision channel that now exists, and has not answered yet",
                EffectError::HumanDecisionRequired {
                    kind: kind(),
                    reason: reason(),
                },
                Recurrence::Awaiting,
            ),
            (
                "the caller's own two halves disagree",
                EffectError::PayloadDiverged {
                    kind: kind(),
                    approved: PayloadHash("a".into()),
                    applying: PayloadHash("b".into()),
                },
                Recurrence::Permanent,
            ),
            (
                "the proposal names work the operation would not do",
                EffectError::IdentityDiverged {
                    kind: kind(),
                    part: "target",
                    proposed: "a".into(),
                    performing: "b".into(),
                },
                Recurrence::Permanent,
            ),
            (
                "the world holds an ambiguity fiddle may not resolve",
                EffectError::DuplicateState {
                    kind: kind(),
                    count: 2,
                },
                Recurrence::Permanent,
            ),
            (
                "nobody knows, and a read settles it",
                EffectError::Unresolved {
                    kind: kind(),
                    reason: reason(),
                },
                Recurrence::Correctable,
            ),
            (
                "the forge would not answer",
                EffectError::Adapter {
                    kind: kind(),
                    source: Box::new(GhError::Auth),
                },
                Recurrence::Correctable,
            ),
        ];

        for (what, error, expected) in cases {
            assert_eq!(
                error.recurrence(),
                expected,
                "{what}: {error} was classified {:?}",
                error.recurrence()
            );
        }
    }

    #[test]
    fn a_diverged_identity_names_the_part_and_both_spellings() {
        let error = EffectError::IdentityDiverged {
            kind: kind(),
            part: "target",
            proposed: "refs/heads/one".into(),
            performing: "refs/heads/two".into(),
        };
        assert_eq!(
            format!("{error}"),
            "ensure_pull_request was proposed with target refs/heads/one and the \
             operation carries target refs/heads/two; nothing was performed"
        );
    }

    #[test]
    fn a_required_human_decision_is_now_awaiting_rather_than_permanent() {
        let error = EffectError::HumanDecisionRequired {
            kind: EffectName::shipped(fiddle_core::ENSURE_PULL_REQUEST_READY),
            reason: "the capability's minimum requires human judgment".into(),
        };
        assert_eq!(error.recurrence(), Recurrence::Awaiting);
        assert_ne!(
            error.recurrence(),
            Recurrence::Permanent,
            "a run that can be resumed by an answer is not a run that has concluded"
        );
    }

    #[test]
    fn no_other_permanent_refusal_became_a_wait() {
        let refusals = [
            EffectError::UnknownEffect {
                kind: EffectName::parse("jira.transition").unwrap(),
            },
            EffectError::Unbuildable {
                kind: kind(),
                reason: reason(),
            },
            EffectError::PolicyDenied {
                kind: kind(),
                reason: reason(),
            },
            EffectError::DuplicateState {
                kind: kind(),
                count: 2,
            },
            EffectError::PayloadDiverged {
                kind: kind(),
                approved: PayloadHash("a".into()),
                applying: PayloadHash("b".into()),
            },
            EffectError::IdentityDiverged {
                kind: kind(),
                part: "kind",
                proposed: "a".into(),
                performing: "b".into(),
            },
        ];
        for error in refusals {
            assert_eq!(error.recurrence(), Recurrence::Permanent, "{error}");
        }
    }

    #[test]
    fn the_two_families_are_both_inhabited() {
        assert_ne!(
            EffectError::PolicyDenied {
                kind: kind(),
                reason: reason(),
            }
            .recurrence(),
            EffectError::Unresolved {
                kind: kind(),
                reason: reason(),
            }
            .recurrence(),
            "a refused effect and an unsettled one must not share an exit row"
        );
    }
}
