//! What fiddle is permitted to do, and who has to be asked first.
//!
//! Two authorities meet here: the capability's own minimum, declared in Rust
//! by whoever wrote the capability, and the deployment's rule, written in a
//! configuration document by whoever runs it. The combination is ordered so
//! that the deployment can only ever make the answer *stricter*. A capability
//! that says an effect needs human judgment cannot be talked out of it by a
//! permissive document, and no agent can modify either input.
//!
//! The ordering matters more than it looks. Both authorities are trustworthy in
//! isolation, and the tempting reading is that the more specific one — the
//! deployment document, which knows the actual repository and the actual
//! blast radius — should win outright. It must not. A document is the input an
//! operator is most likely to relax under time pressure and least likely to
//! re-read afterwards, whereas a capability's minimum was written once by
//! someone reasoning about that effect in particular. Taking the stricter of
//! the two means a mistake in either direction fails safe: a document can add
//! a gate it did not have, and cannot remove one it never granted.
//!
//! [`combine`] is total over the product of the two inputs, so there is no
//! combination for which a caller must invent an answer, and no default that
//! could quietly become the permissive one.

/// The minimum a capability itself declares for an effect kind.
///
/// Deliberately two-valued and not a mirror of [`DeploymentRule`]. A capability
/// states a *floor* — whether it will ever act unattended — and has no standing
/// to deny an effect outright; refusing to offer the effect at all is how a
/// capability expresses that, and it does so by not proposing it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HumanDecisionRequirement {
    Automatic,
    Human,
}

/// What the deployment document says about one effect kind.
///
/// Three-valued because a deployment has one power a capability does not: it
/// can take an effect off the table entirely, for reasons — a protected branch,
/// an audit regime, a repository fiddle is only meant to read — that the
/// capability's author could not have known about.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeploymentRule {
    Allow,
    RequireHuman,
    Deny,
}

/// The single answer the two authorities produce together.
///
/// Every non-`Allow` variant carries a reason because this decision is the last
/// place that knows *why* an effect did not happen. Downstream it becomes a
/// typed refusal a person reads long after the run, and "denied" with no
/// antecedent is a message that sends its reader back to the configuration to
/// guess. Making the reason a field rather than a formatting concern means a
/// refusal cannot be constructed without one.
///
/// `Serialize` so a decision can be recorded in a published bundle; there is no
/// `Deserialize`, because nothing reads a decision back in — it is recomputed
/// from its two inputs, which is what keeps a recorded verdict from becoming an
/// authority of its own.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow,
    Deny { reason: String },
    RequireHumanDecision { reason: String },
}

/// Take the stricter of the capability's minimum and the deployment's rule.
///
/// The arms are ordered by strictness rather than by input, which is what makes
/// the "never weakens" property readable at the call site instead of being an
/// emergent consequence of six independent cells. `Deny` is matched first
/// because it is absolute; the capability's own `Human` minimum is matched next
/// so that it survives whatever the document said; and `Allow` is reachable
/// only when both authorities independently permit it.
///
/// A pure function of its two arguments. Nothing is read from outside, so the
/// same pair always yields the same decision and a reviewer can check the whole
/// policy by reading six lines rather than by reasoning about what state the
/// process was in.
pub fn combine(capability: HumanDecisionRequirement, deployment: DeploymentRule) -> PolicyDecision {
    match (capability, deployment) {
        // Deny is absolute: there is nothing left to ask a person about, and
        // routing it to a human would present a settled refusal as an open
        // question that somebody could answer the wrong way.
        (_, DeploymentRule::Deny) => PolicyDecision::Deny {
            reason: "deployment policy denies this effect kind".to_string(),
        },
        // The capability's own minimum survives a permissive deployment. This
        // is the cell the whole module exists for: `Allow` in the document is
        // an absence of an additional gate, never a removal of an existing one.
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

    /// The one rule: deployment may strengthen and may never weaken. Written
    /// out over the whole product rather than sampled, because the interesting
    /// cell is the one an author is least likely to pick — capability Human
    /// against deployment Allow, where a permissive deployment must not be
    /// able to buy away the capability's own minimum.
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

    /// A refusal a reader cannot act on is not much of a refusal.
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

    /// Deny wins over a human gate: there is nothing to ask about.
    #[test]
    fn a_denied_effect_is_never_downgraded_to_a_question() {
        assert!(matches!(
            combine(HumanDecisionRequirement::Human, DeploymentRule::Deny),
            PolicyDecision::Deny { .. }
        ));
    }
}
