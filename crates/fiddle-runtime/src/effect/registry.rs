use crate::effect::{build, DynEffect, EffectError, Executor, StepParams};
use crate::github::{
    EnsureBranchPublished, EnsureCheckRequested, EnsurePullRequest, EnsurePullRequestBody,
    EnsurePullRequestReady,
};
use crate::human::PublishDecisionRequest;
use fiddle_core::{
    EffectName, HumanDecisionRequirement, ENSURE_CHECK_REQUESTED, ENSURE_PULL_REQUEST,
    ENSURE_PULL_REQUEST_BODY, ENSURE_PULL_REQUEST_READY, PUBLISH_DECISION_REQUEST,
};
use std::sync::OnceLock;

pub type Construct = fn(&Executor<'_>, &StepParams) -> Result<Box<dyn DynEffect>, EffectError>;

#[derive(Clone, Copy, Debug)]
pub struct EffectDescriptor {
    pub name: &'static str,
    pub minimum: HumanDecisionRequirement,
    pub construct: Construct,
}

impl PartialEq for EffectDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.minimum == other.minimum
            && std::ptr::fn_addr_eq(self.construct, other.construct)
    }
}

impl Eq for EffectDescriptor {}

#[derive(Clone, Debug, thiserror::Error, Eq, PartialEq)]
pub enum RegistryError {
    #[error("`{name}` is already an effect this build performs")]
    Duplicate { name: String },
    #[error("`{name}` is not an effect name, so no document could spell it")]
    Unspellable { name: String },
    #[error("the registry already holds {held} installed effects and is closed")]
    AlreadyInstalled { held: usize },
}

pub const BUILT_IN: &[EffectDescriptor] = &[
    EnsureBranchPublished::descriptor(),
    EffectDescriptor {
        name: ENSURE_PULL_REQUEST,
        minimum: HumanDecisionRequirement::Automatic,
        construct: build::<EnsurePullRequest>,
    },
    EffectDescriptor {
        name: ENSURE_CHECK_REQUESTED,
        minimum: HumanDecisionRequirement::Automatic,
        construct: build::<EnsureCheckRequested>,
    },
    EffectDescriptor {
        name: PUBLISH_DECISION_REQUEST,
        minimum: HumanDecisionRequirement::Automatic,
        construct: build::<PublishDecisionRequest>,
    },
    EffectDescriptor {
        name: ENSURE_PULL_REQUEST_READY,
        minimum: HumanDecisionRequirement::Human,
        construct: build::<EnsurePullRequestReady>,
    },
    EffectDescriptor {
        name: ENSURE_PULL_REQUEST_BODY,
        minimum: HumanDecisionRequirement::Automatic,
        construct: build::<EnsurePullRequestBody>,
    },
];

static EXTRA: OnceLock<&'static [EffectDescriptor]> = OnceLock::new();

pub fn install(extra: &'static [EffectDescriptor]) -> Result<(), RegistryError> {
    admissible(extra)?;
    EXTRA
        .set(extra)
        .map_err(|_| RegistryError::AlreadyInstalled {
            held: extension().len(),
        })
}

pub fn registered() -> Vec<&'static EffectDescriptor> {
    all(extension()).collect()
}

pub fn describe(name: &EffectName) -> Option<&'static EffectDescriptor> {
    find(name, extension())
}

pub fn resolve(name: &EffectName) -> Option<Construct> {
    describe(name).map(|descriptor| descriptor.construct)
}

fn extension() -> &'static [EffectDescriptor] {
    EXTRA.get().copied().unwrap_or(&[])
}

fn all(extra: &'static [EffectDescriptor]) -> impl Iterator<Item = &'static EffectDescriptor> {
    BUILT_IN.iter().chain(extra.iter())
}

fn find(
    name: &EffectName,
    extra: &'static [EffectDescriptor],
) -> Option<&'static EffectDescriptor> {
    all(extra).find(|descriptor| descriptor.name == name.as_str())
}

fn admissible(extra: &[EffectDescriptor]) -> Result<(), RegistryError> {
    let mut seen = std::collections::BTreeSet::new();
    for descriptor in extra {
        if EffectName::parse(descriptor.name).is_err() {
            return Err(RegistryError::Unspellable {
                name: descriptor.name.to_string(),
            });
        }
        let taken = BUILT_IN.iter().any(|held| held.name == descriptor.name);
        if taken || !seen.insert(descriptor.name) {
            return Err(RegistryError::Duplicate {
                name: descriptor.name.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::IntegrationOperation;
    use crate::github::{
        EnsureBranchPublished, EnsureCheckRequested, EnsurePullRequest, EnsurePullRequestBody,
        EnsurePullRequestReady,
    };
    use crate::human::PublishDecisionRequest;
    use fiddle_core::{
        DecisionBinding, DecisionRequestId, EffectId, EffectName, HumanDecisionRequest,
        HumanDecisionRequirement, PayloadHash, ENSURE_BRANCH_PUBLISHED, ENSURE_CHECK_REQUESTED,
        ENSURE_PULL_REQUEST, ENSURE_PULL_REQUEST_BODY, ENSURE_PULL_REQUEST_READY, PUBLISH_CHANGE,
        PUBLISH_DECISION_REQUEST,
    };

    fn unshipped(
        _executor: &Executor<'_>,
        _params: &StepParams,
    ) -> Result<Box<dyn DynEffect>, EffectError> {
        Err(EffectError::Unbuildable {
            kind: EffectName::shipped("jira.transition"),
            reason: "an extension in this test declares a name and ships no operation".to_string(),
        })
    }

    fn branch_op() -> EnsureBranchPublished {
        EnsureBranchPublished::new(
            "acme/widget".to_string(),
            "fiddle/abc".to_string(),
            "deadbeef".to_string(),
        )
    }

    fn pull_op() -> EnsurePullRequest {
        EnsurePullRequest::new(
            "acme/widget".to_string(),
            "acme".to_string(),
            "fiddle/abc".to_string(),
            "main".to_string(),
            "a title".to_string(),
            "a body".to_string(),
            false,
        )
    }

    fn check_op() -> EnsureCheckRequested {
        EnsureCheckRequested::new(
            "acme/widget".to_string(),
            "ci.yml".to_string(),
            "fiddle/abc".to_string(),
            "acme/widget",
            "beans:w-1",
        )
    }

    fn request_op() -> PublishDecisionRequest {
        PublishDecisionRequest::new(
            "acme/widget".to_string(),
            7,
            HumanDecisionRequest {
                invocation_ref: "beans:w-1".to_string(),
                work_ref: None,
                capability: PUBLISH_CHANGE,
                binding: DecisionBinding {
                    request: DecisionRequestId("0000000000000000".to_string()),
                    effect: EffectId("0000000000000000".to_string()),
                    payload: PayloadHash("0000000000000000".to_string()),
                    head_sha: "deadbeef".to_string(),
                },
                question: "may this proceed".to_string(),
                rationale: "because".to_string(),
                risks: Vec::new(),
                alternatives: Vec::new(),
                evidence: Vec::new(),
            },
        )
    }

    fn ready_op() -> EnsurePullRequestReady {
        EnsurePullRequestReady::new("acme/widget".to_string(), 7, "deadbeef".to_string())
    }

    fn body_op() -> EnsurePullRequestBody {
        EnsurePullRequestBody::new("acme/widget".to_string(), 7, "a body".to_string())
    }

    #[test]
    fn every_registered_name_is_unique_and_parses() {
        let mut seen = std::collections::BTreeSet::new();
        for descriptor in BUILT_IN {
            assert!(
                EffectName::parse(descriptor.name).is_ok(),
                "{} is unspellable",
                descriptor.name
            );
            assert!(
                seen.insert(descriptor.name),
                "{} is registered twice",
                descriptor.name
            );
        }
        assert_eq!(seen.len(), BUILT_IN.len());
    }

    #[test]
    fn the_registry_holds_exactly_the_six_this_build_ships() {
        let names: Vec<&str> = BUILT_IN.iter().map(|d| d.name).collect();
        assert_eq!(
            names,
            vec![
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
    fn lookup_refuses_a_name_no_descriptor_holds() {
        assert!(describe(&EffectName::parse("jira.transition").unwrap()).is_none());
        assert!(describe(&EffectName::parse(ENSURE_PULL_REQUEST).unwrap()).is_some());
    }

    #[test]
    fn with_nothing_installed_the_registry_is_exactly_the_built_ins() {
        assert_eq!(registered().len(), BUILT_IN.len());
        assert!(
            !BUILT_IN.is_empty(),
            "an empty built-in list satisfies the line above vacuously"
        );
    }

    #[test]
    fn an_extra_effect_reusing_a_built_in_name_is_refused() {
        static CLASH: &[EffectDescriptor] = &[EffectDescriptor {
            name: ENSURE_PULL_REQUEST,
            minimum: HumanDecisionRequirement::Automatic,
            construct: unshipped,
        }];
        assert_eq!(
            install(CLASH),
            Err(RegistryError::Duplicate {
                name: ENSURE_PULL_REQUEST.to_string()
            })
        );
    }

    static JIRA: &[EffectDescriptor] = &[
        EffectDescriptor {
            name: "jira.transition",
            minimum: HumanDecisionRequirement::Human,
            construct: unshipped,
        },
        EffectDescriptor {
            name: "jira.comment",
            minimum: HumanDecisionRequirement::Automatic,
            construct: unshipped,
        },
    ];

    #[test]
    fn an_admissible_extension_is_answered_beside_the_built_ins() {
        assert_eq!(admissible(JIRA), Ok(()));
        assert_eq!(all(JIRA).count(), BUILT_IN.len() + JIRA.len());
        let found = find(&EffectName::parse("jira.transition").unwrap(), JIRA)
            .expect("an admitted extra is answered");
        assert_eq!(found.minimum, HumanDecisionRequirement::Human);
        assert!(
            find(&EffectName::parse(ENSURE_PULL_REQUEST).unwrap(), JIRA).is_some(),
            "and an extension does not displace a built-in"
        );
        assert!(find(&EffectName::parse("jira.assign").unwrap(), JIRA).is_none());
    }

    #[test]
    fn an_extension_that_repeats_itself_is_refused() {
        static TWICE: &[EffectDescriptor] = &[
            EffectDescriptor {
                name: "jira.comment",
                minimum: HumanDecisionRequirement::Automatic,
                construct: unshipped,
            },
            EffectDescriptor {
                name: "jira.comment",
                minimum: HumanDecisionRequirement::Human,
                construct: unshipped,
            },
        ];
        assert_eq!(
            admissible(TWICE),
            Err(RegistryError::Duplicate {
                name: "jira.comment".to_string()
            })
        );
    }

    #[test]
    fn an_extension_no_document_could_spell_is_refused() {
        static SHOUTED: &[EffectDescriptor] = &[EffectDescriptor {
            name: "Jira.Transition",
            minimum: HumanDecisionRequirement::Automatic,
            construct: unshipped,
        }];
        assert_eq!(
            admissible(SHOUTED),
            Err(RegistryError::Unspellable {
                name: "Jira.Transition".to_string()
            })
        );
    }

    #[test]
    fn no_operation_declares_a_minimum_its_descriptor_does_not() {
        let cases: Vec<(&str, HumanDecisionRequirement)> = vec![
            (ENSURE_BRANCH_PUBLISHED, branch_op().minimum()),
            (ENSURE_PULL_REQUEST, pull_op().minimum()),
            (ENSURE_CHECK_REQUESTED, check_op().minimum()),
            (PUBLISH_DECISION_REQUEST, request_op().minimum()),
            (ENSURE_PULL_REQUEST_READY, ready_op().minimum()),
            (ENSURE_PULL_REQUEST_BODY, body_op().minimum()),
        ];
        assert_eq!(
            cases.len(),
            BUILT_IN.len(),
            "an effect was added without a case here"
        );
        for (name, declared) in cases {
            let descriptor =
                describe(&EffectName::parse(name).unwrap()).expect("a shipped name is registered");
            assert_eq!(
                descriptor.minimum, declared,
                "{name} disagrees with its descriptor"
            );
        }
    }
}
