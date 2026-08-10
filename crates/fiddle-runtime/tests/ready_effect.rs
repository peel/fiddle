//! `ensure_pull_request_ready`: the first operation in this build that a person
//! has to have agreed to, and the first whose identity carries a revision.
//!
//! Two questions are specific to this operation, and neither is the executor's
//! protocol — `effect_protocol.rs` owns that against a scripted operation.
//!
//! **Its minimum requires a person.** M2's three operations all declare
//! `Automatic`, so `combine`'s `(Human, Allow)` cell had been asserted in
//! `policy.rs`'s own table and produced by nothing that runs. Asserting it here,
//! through the operation, is what makes it a fact about the build rather than
//! about a table: a later edit that quietly relaxed this minimum to `Automatic`
//! would still pass `policy.rs` and would fail here.
//!
//! **Its identity carries the head sha.** The target is `{repo}#{pr}@{head_sha}`,
//! so a pull request whose head has moved is a different effect with a different
//! `EffectId` — and therefore a different `DecisionRequestId`, and therefore a
//! different question. That is what makes an approval given for an earlier
//! revision *unrecognisable* rather than merely rejected. Put the revision in
//! the payload alone and the identity would be unchanged, so the stale approval
//! would arrive looking like an answer to the current question and be refused as
//! a payload divergence, which reads as a caller misbehaving rather than as a
//! change having moved on since somebody looked at it.

use fiddle_core::{
    combine, effect_id, DeploymentRule, EffectKind, HumanDecisionRequirement, PolicyDecision,
};
use fiddle_runtime::effect::IntegrationOperation;
use fiddle_runtime::github::EnsurePullRequestReady;

/// The repository these cases are about, and the one the target names.
const REPO: &str = "acme/r";

/// The pull request's number. Seven rather than one, so an assertion on it
/// cannot pass by accident against an index or a count.
const PR: u64 = 7;

const PROJECT: &str = "p";
const INVOCATION_REF: &str = "beans:x";

fn op() -> EnsurePullRequestReady {
    op_at_head("aaaa")
}

fn op_at_head(head_sha: &str) -> EnsurePullRequestReady {
    EnsurePullRequestReady::new(REPO.to_string(), PR, head_sha.to_string())
}

/// The identity a fresh process would recompute for this operation.
fn identity_of(operation: &EnsurePullRequestReady) -> fiddle_core::EffectId {
    effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsurePullRequestReady,
        &operation.target(),
    )
}

/// The whole reason `combine` was written, reached by something that runs.
///
/// Both halves are asserted rather than only the declaration. The minimum on its
/// own is a value; what makes it a gate is that a deployment saying `Allow` —
/// the most permissive thing a document can say — still produces a decision that
/// has to go to a person.
#[test]
fn this_is_the_first_operation_whose_own_minimum_requires_a_person() {
    assert_eq!(
        op().minimum(),
        HumanDecisionRequirement::Human,
        "making a change reviewable is not fiddle's decision to take"
    );
    assert!(matches!(
        combine(op().minimum(), DeploymentRule::Allow),
        PolicyDecision::RequireHumanDecision { .. }
    ));
}

/// The revision is in the target, so a moved head is a different effect rather
/// than the same effect with a changed request.
///
/// The third assertion is about the *spelling* and not about tidiness: the
/// target is hashed into the identity, so two ways of writing one target are two
/// effects, and a process recomputing it from the same three facts has to arrive
/// at the same string.
#[test]
fn the_revision_is_part_of_the_identity_and_not_only_of_the_payload() {
    let a = op_at_head("aaaa");
    let b = op_at_head("bbbb");

    assert_ne!(a.target(), b.target());
    assert_ne!(identity_of(&a), identity_of(&b));
    assert!(a.target().contains("acme/r#7@"), "got {}", a.target());
    assert_eq!(
        identity_of(&a),
        identity_of(&op_at_head("aaaa")),
        "and the same revision recomputes the same identity, which is what lets \
         a fresh process recognise work it really did"
    );
}
