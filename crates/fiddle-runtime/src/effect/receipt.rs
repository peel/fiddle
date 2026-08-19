use crate::github::GhError;
use fiddle_core::{EffectId, EffectKind, PayloadHash};

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
    #[error("policy denied {kind:?}: {reason}")]
    PolicyDenied { kind: EffectKind, reason: String },
    #[error("{kind:?} is awaiting a human decision on the channel M3 introduced: {reason}")]
    HumanDecisionRequired { kind: EffectKind, reason: String },
    #[error("{kind:?} left an unresolved outcome: {reason}")]
    Unresolved { kind: EffectKind, reason: String },
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
    #[error("{kind:?} found {count} matching objects, expected at most one")]
    DuplicateState { kind: EffectKind, count: usize },
    #[error("adapter failure for {kind:?}: {source}")]
    Adapter { kind: EffectKind, source: GhError },
}

impl EffectError {
    pub fn recurrence(&self) -> Recurrence {
        match self {
            EffectError::PolicyDenied { .. } => Recurrence::Permanent,

            EffectError::HumanDecisionRequired { .. } => Recurrence::Awaiting,

            EffectError::PayloadDiverged { .. } => Recurrence::Permanent,

            EffectError::DuplicateState { .. } => Recurrence::Permanent,

            EffectError::Unresolved { .. } => Recurrence::Correctable,

            EffectError::Adapter { .. } => Recurrence::Correctable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIND: EffectKind = EffectKind::EnsurePullRequest;

    fn reason() -> String {
        "because".to_string()
    }

    #[test]
    fn every_effect_failure_declares_which_exit_row_it_belongs_in() {
        let cases: [(&str, EffectError, Recurrence); 6] = [
            (
                "a deployment rule denies the kind",
                EffectError::PolicyDenied {
                    kind: KIND,
                    reason: reason(),
                },
                Recurrence::Permanent,
            ),
            (
                "a decision channel that now exists, and has not answered yet",
                EffectError::HumanDecisionRequired {
                    kind: KIND,
                    reason: reason(),
                },
                Recurrence::Awaiting,
            ),
            (
                "the caller's own two halves disagree",
                EffectError::PayloadDiverged {
                    kind: KIND,
                    approved: PayloadHash("a".into()),
                    applying: PayloadHash("b".into()),
                },
                Recurrence::Permanent,
            ),
            (
                "the world holds an ambiguity fiddle may not resolve",
                EffectError::DuplicateState {
                    kind: KIND,
                    count: 2,
                },
                Recurrence::Permanent,
            ),
            (
                "nobody knows, and a read settles it",
                EffectError::Unresolved {
                    kind: KIND,
                    reason: reason(),
                },
                Recurrence::Correctable,
            ),
            (
                "the forge would not answer",
                EffectError::Adapter {
                    kind: KIND,
                    source: GhError::Auth,
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
    fn a_required_human_decision_is_now_awaiting_rather_than_permanent() {
        let error = EffectError::HumanDecisionRequired {
            kind: EffectKind::EnsurePullRequestReady,
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
            EffectError::PolicyDenied {
                kind: KIND,
                reason: reason(),
            },
            EffectError::DuplicateState {
                kind: KIND,
                count: 2,
            },
            EffectError::PayloadDiverged {
                kind: KIND,
                approved: PayloadHash("a".into()),
                applying: PayloadHash("b".into()),
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
                kind: KIND,
                reason: reason(),
            }
            .recurrence(),
            EffectError::Unresolved {
                kind: KIND,
                reason: reason(),
            }
            .recurrence(),
            "a refused effect and an unsettled one must not share an exit row"
        );
    }
}
