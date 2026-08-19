#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HumanDecisionRequirement {
    Automatic,
    Human,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DeploymentRule {
    Allow,
    RequireHuman,
    Deny,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow,
    Deny { reason: String },
    RequireHumanDecision { reason: String },
}

pub fn combine(capability: HumanDecisionRequirement, deployment: DeploymentRule) -> PolicyDecision {
    match (capability, deployment) {
        (_, DeploymentRule::Deny) => PolicyDecision::Deny {
            reason: "deployment policy denies this effect kind".to_string(),
        },
        (HumanDecisionRequirement::Human, _) => PolicyDecision::RequireHumanDecision {
            reason: "the capability's minimum requires human judgment for this effect".to_string(),
        },
        (HumanDecisionRequirement::Automatic, DeploymentRule::RequireHuman) => {
            PolicyDecision::RequireHumanDecision {
                reason: "deployment policy requires human judgment for this effect kind"
                    .to_string(),
            }
        }
        (HumanDecisionRequirement::Automatic, DeploymentRule::Allow) => PolicyDecision::Allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_may_strengthen_and_may_never_weaken() {
        use DeploymentRule::*;
        use HumanDecisionRequirement::*;

        assert!(matches!(combine(Automatic, Allow), PolicyDecision::Allow));
        assert!(matches!(
            combine(Automatic, RequireHuman),
            PolicyDecision::RequireHumanDecision { .. }
        ));
        assert!(matches!(
            combine(Automatic, Deny),
            PolicyDecision::Deny { .. }
        ));

        assert!(matches!(
            combine(Human, Allow),
            PolicyDecision::RequireHumanDecision { .. }
        ));
        assert!(matches!(
            combine(Human, RequireHuman),
            PolicyDecision::RequireHumanDecision { .. }
        ));
        assert!(matches!(combine(Human, Deny), PolicyDecision::Deny { .. }));
    }

    #[test]
    fn every_non_allow_decision_explains_itself() {
        use DeploymentRule::*;
        use HumanDecisionRequirement::*;
        for (cap, dep) in [
            (Automatic, RequireHuman),
            (Automatic, Deny),
            (Human, Allow),
            (Human, RequireHuman),
            (Human, Deny),
        ] {
            let reason = match combine(cap, dep) {
                PolicyDecision::Allow => panic!("{cap:?}/{dep:?} must not be Allow"),
                PolicyDecision::Deny { reason }
                | PolicyDecision::RequireHumanDecision { reason } => reason,
            };
            assert!(!reason.trim().is_empty(), "{cap:?}/{dep:?} must say why");
        }
    }

    #[test]
    fn a_denied_effect_is_never_downgraded_to_a_question() {
        assert!(matches!(
            combine(HumanDecisionRequirement::Human, DeploymentRule::Deny),
            PolicyDecision::Deny { .. }
        ));
    }
}
