//! The effect executor's protocol, and the first operation that composes it.
//!
//! Two halves, and the split between them is the point rather than an
//! arrangement of convenience.
//!
//! The **protocol half** is every *ambiguity* case: what the executor does when
//! it does not know whether a write landed. None of it reaches GitHub and none
//! of it spawns a process — the world is a scripted [`IntegrationOperation`], so
//! the properties the milestone turns on are decided by the executor rather than
//! by whatever a network happened to do that afternoon. The one rule underneath
//! all of it: **`Unknown` is resolved by reading the world, never by retrying
//! the mutation.** A retry there is how a duplicate external effect is born, so
//! the mutation dispatch count is asserted directly rather than inferred from an
//! outcome.
//!
//! The **branch half** is `ensure_branch_published` end to end, and it must be
//! asked of something real: a **bare repository on disk** pushed to by the
//! product's own `git`, and the **scripted `gh`** answering the ref read out of
//! that same repository. A fixture answering the read from its own idea of what
//! a push does would be asserting this file's assumptions about git rather than
//! git's behaviour — and "a divergent ref is refused as a non-fast-forward" is
//! precisely a claim about git's behaviour, since it is the claim that stands in
//! for the ownership trailer the design dropped. Still offline, still
//! credential-free: a path remote authenticates nobody.

// The scripted world, the deployment stub and the executor harness live in
// `tests/support/mod.rs`: they are shared with `pull_request_effect.rs`, and a
// fixture that exists twice is a fixture whose two copies eventually disagree
// about the protocol they are both supposed to be pinning.
mod support;

use fiddle_core::{
    decision_request_id, effect_id, payload_hash, DecisionBinding, DeploymentRule, EffectId,
    EffectKind, HumanDecisionRequirement, InterpretedHumanDecision, PayloadHash, ProposedEffect,
    Published, FIXTURE_REPAIR, STUB_MARK,
};
use fiddle_runtime::effect::{
    EffectContext, EffectError, EffectOutcome, EffectReceipt, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ObservedState, ReadRetry, ResolvedDecision,
};
use fiddle_runtime::git::{GitCli, GitError};
use fiddle_runtime::github::{branch_name, EnsureBranchPublished};
use fiddle_runtime::{GhCli, GhError, RetryAdvice};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use support::{
    branch_effect, proposed_by, Deployment, Harness, Script, INVOCATION_REF, PAYLOAD, PROJECT,
    TARGET,
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// The envelope
// ---------------------------------------------------------------------------

/// The envelope is only worth having if it cannot be forged. Receiving one must
/// prove identity, policy and payload were checked for this exact request.
///
/// The structural half of this proof is the `compile_fail` doctest on
/// [`AuthorizedEffect`] itself: this file is a *separate crate*, so if a struct
/// literal or a public constructor existed, that doctest would compile and fail
/// the suite. What is asserted here is the source-level half — that no
/// constructor is offered under a name a caller could reach for.
#[test]
fn the_authorization_envelope_has_no_public_constructor() {
    let source = include_str!("../src/effect/mod.rs");
    assert!(
        !source.contains("pub fn authorize") && !source.contains("pub const fn authorize"),
        "AuthorizedEffect must not be constructible outside the executor"
    );
    // Every field is private, so no struct literal works either.
    assert!(source.contains("pub struct AuthorizedEffect<T> {\n    effect_id:"));
}

// ---------------------------------------------------------------------------
// Step 3 before step 4, and both before the mutation
// ---------------------------------------------------------------------------

/// The postcondition is inspected *before* the mutation, so an effect that has
/// already happened is never performed a second time.
#[tokio::test]
async fn an_existing_postcondition_short_circuits_the_mutation() {
    let harness = Harness::new(Script::AlreadySatisfied);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.mutations(),
        0,
        "nothing was written; the world already agreed"
    );
    assert_eq!(
        harness.world.mutation_requests(),
        0,
        "and nothing was even dispatched"
    );
    assert_eq!(receipt.postcondition, "branch at deadbeef");
    assert_eq!(receipt.external_ref.as_deref(), Some("deadbeef"));
}

/// The stronger half of the same rule, and the one an endpoint-only test would
/// miss: an effect the world already satisfies is never *asked about*. Policy
/// is not consulted, so an effect that has already happened cannot be refused
/// for a rule it no longer needs.
#[tokio::test]
async fn an_already_satisfied_effect_is_never_put_to_policy() {
    let harness = Harness::new(Script::AlreadySatisfied)
        // The strictest policy there is. If the order were inverted, this would
        // deny an effect that has already happened.
        .with_policy(HumanDecisionRequirement::Human, DeploymentRule::Deny);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition"
        ],
        "the walk stops at the inspection; policy is never reached"
    );
}

/// The order is the contract, not an implementation detail: policy must be
/// consulted after the postcondition inspection (so an already-done effect is
/// never refused for a rule it no longer needs) and before the mutation (so a
/// refused effect never happens). A test that only checks the endpoints would
/// pass on an implementation that authorized first and asked afterwards.
#[tokio::test]
async fn the_nine_steps_happen_in_the_specified_order() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
            "authorize",
            "apply",
            "observe_postcondition",
        ]
    );
    // The steps are what the executor says it did; the calls are what the world
    // saw. Both are asserted, so a step emitted without the work behind it
    // would not pass.
    assert_eq!(harness.world.calls(), ["inspect", "apply", "inspect"]);
}

// ---------------------------------------------------------------------------
// What an unknown outcome resolves to
// ---------------------------------------------------------------------------

/// The rule the milestone turns on, in the executor rather than in an adapter.
#[tokio::test]
async fn an_unknown_outcome_is_resolved_by_reading_never_by_retrying() {
    let harness = Harness::new(Script::WriteLandsAnswerLost);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.mutation_requests(),
        1,
        "the mutation was dispatched exactly once"
    );
    assert_eq!(
        harness.world.mutations(),
        1,
        "and it landed exactly once, which is the property"
    );
    assert!(
        harness.world.read_after_unknown(),
        "the executor went and looked"
    );
    assert_eq!(
        harness.world.calls(),
        ["inspect", "apply", "inspect"],
        "a read settled it; no second dispatch appears anywhere in the walk"
    );
}

/// A read that itself fails leaves the effect unresolved and says so, rather
/// than degrading to one of the two confident answers.
#[tokio::test]
async fn an_unreadable_postcondition_leaves_the_effect_unresolved() {
    let harness = Harness::new(Script::WriteLostReadFails);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "expected Unresolved, got {error:?}"
    );
    assert_eq!(
        harness.world.mutation_requests(),
        1,
        "an unresolved outcome is still never retried"
    );
}

/// The mirror of the previous case: the adapter claimed success and the world
/// does not show it. Believing the response over the world is exactly what step
/// 8 exists to prevent, so this is unresolved too rather than committed.
#[tokio::test]
async fn a_dispatch_that_claimed_success_without_a_postcondition_is_unresolved() {
    let harness = Harness::new(Script::SuccessWithoutPostcondition);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "expected Unresolved, got {error:?}"
    );
}

/// A refusal that leaves no room for the write having happened, against a world
/// that agrees. Here the refusal stands as the answer — reporting this one
/// `Unresolved` would send a caller to investigate a settled failure.
#[tokio::test]
async fn a_confident_refusal_the_world_agrees_with_stays_a_failure() {
    let harness = Harness::new(Script::ConfidentRefusal);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(
            error,
            EffectError::Adapter {
                source: GhError::Http { status: 403, .. },
                ..
            }
        ),
        "expected the refusal to stand, got {error:?}"
    );
    assert_eq!(harness.world.mutations(), 0);
}

/// Two matching objects is a state to report, not a set to pick from.
#[tokio::test]
async fn more_than_one_matching_object_is_a_duplicate_state_error() {
    let harness = Harness::new(Script::TwoMatch);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::DuplicateState { count: 2, .. }),
        "expected DuplicateState with the count, got {error:?}"
    );
    assert_eq!(
        harness.world.mutation_requests(),
        0,
        "an unaccounted-for object is never written over"
    );
}

// ---------------------------------------------------------------------------
// Waiting for the read, and never for the write
// ---------------------------------------------------------------------------
//
// Real GitHub produces this milestone's central ambiguity unprompted, and it
// was measured rather than reasoned about: `GET .../actions/workflows/<f>/runs`
// reliably does not list a run immediately after the dispatch that created it,
// and `GET .../git/ref/heads/<b>` has answered 404 straight after the push that
// created the branch, with the branch and the sha verified correct moments
// later.
//
// Every case below is therefore a claim about the *read*. The one thing none of
// them may show is a second `apply`, which is why each asserts the dispatch
// count directly rather than inferring it from an outcome.

/// The scripted budget: enough attempts to settle, and waits that are recorded
/// rather than spent, so a case asserting a two-second wait costs no seconds.
const BUDGET: (u32, Duration, Duration) = (5, Duration::from_millis(10), Duration::from_secs(30));

/// A postcondition that arrives late is waited for, not re-dispatched.
///
/// The case the whole bean exists for: the write landed, the answer came back,
/// and the world took one more look to admit it. The dispatch count and the read
/// count move in opposite directions, which is the only way to tell "it waited"
/// from "it wrote again".
#[tokio::test]
async fn a_postcondition_that_arrives_late_is_waited_for_not_redispatched() {
    let harness = Harness::new(Script::PostconditionSurfacesLate)
        .with_read_retry(BUDGET.0, BUDGET.1, BUDGET.2);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.mutation_requests(),
        1,
        "the mutation was never re-sent"
    );
    assert_eq!(harness.world.mutations(), 1, "and it landed exactly once");
    assert_eq!(
        harness.world.calls(),
        ["inspect", "apply", "inspect", "inspect"],
        "the read was retried, and only the read"
    );
    assert_eq!(
        harness.waits().len(),
        1,
        "exactly one wait, between the two post-dispatch reads"
    );
}

/// The same lateness on top of a lost answer — both ambiguities at once, which
/// is the shape real GitHub hands over when a `gh` is killed mid-flight.
///
/// The criterion the bean turns on, asserted directly rather than inferred:
/// `apply` is dispatched exactly once, and the walk is counted whole so a second
/// one could not hide in it.
#[tokio::test]
async fn an_unknown_outcome_still_never_redispatches_the_mutation() {
    let harness = Harness::new(Script::WriteLandsAnswerLostAndSurfacesLate)
        .with_read_retry(BUDGET.0, BUDGET.1, BUDGET.2);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(harness.world.mutation_requests(), 1);
    assert_eq!(harness.world.mutations(), 1);
    assert_eq!(
        harness
            .world
            .calls()
            .iter()
            .filter(|call| **call == "apply")
            .count(),
        1,
        "a lost answer plus a late read is still exactly one dispatch"
    );
}

/// The budget is bounded, and exhausting it is still `Unresolved`.
///
/// A read-retry that waited indefinitely would turn "the write did not land"
/// into "wait longer, then claim success" — worse than the ambiguity it
/// replaces, because the ambiguity at least sends somebody to look.
#[tokio::test]
async fn a_read_that_never_settles_exhausts_its_budget_and_stays_unresolved() {
    let harness = Harness::new(Script::SuccessWithoutPostcondition)
        .with_read_retry(BUDGET.0, BUDGET.1, BUDGET.2);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "a spent budget never becomes a success, got {error:?}"
    );
    assert_eq!(harness.world.mutation_requests(), 1);
    let attempts = harness.read_retry().attempts() as usize;
    assert_eq!(
        harness.world.reads(),
        // One look before the mutation, and the whole budget after it.
        1 + attempts,
        "the read is bounded by the budget it was given"
    );
    assert_eq!(harness.waits().len(), attempts - 1);
    assert!(
        error.to_string().contains("over 5 reads"),
        "the diagnostic must say that waiting was tried, got {error}"
    );
}

/// A read that keeps *failing* is bounded by the same budget, and stays
/// unresolved for the same reason.
#[tokio::test]
async fn a_read_that_keeps_failing_exhausts_its_budget_and_stays_unresolved() {
    let harness =
        Harness::new(Script::WriteLostReadFails).with_read_retry(BUDGET.0, BUDGET.1, BUDGET.2);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "expected Unresolved, got {error:?}"
    );
    assert_eq!(
        harness.world.mutation_requests(),
        1,
        "an unreadable world is never answered by writing again"
    );
    assert_eq!(
        harness.world.reads(),
        1 + harness.read_retry().attempts() as usize
    );
}

/// `Retry-After` is honoured rather than parsed and dropped.
///
/// The header is the only wait in this system GitHub chose rather than fiddle,
/// and until this bean it reached nothing. The assertion is on the *first* wait
/// specifically: the backoff's own first step here is 10ms, so a two-second wait
/// cannot have come from anywhere but the header.
#[tokio::test]
async fn a_retry_after_header_sets_the_wait() {
    let harness =
        Harness::new(Script::RateLimitedThenSettles).with_read_retry(BUDGET.0, BUDGET.1, BUDGET.2);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(harness.waits(), [support::SCRIPTED_RETRY_AFTER]);
    assert_eq!(
        harness.world.mutation_requests(),
        1,
        "a rate limit is answered by waiting, never by writing again"
    );
}

/// A `Retry-After` longer than the deployment's ceiling does not sleep past it.
///
/// `max` is the operator's bound on how long one read may block a run. A server
/// asking for longer is answered by spending the budget and reporting
/// `Unresolved` — a caller who can decide — rather than by honouring a number
/// the document never agreed to.
#[tokio::test]
async fn a_retry_after_longer_than_the_ceiling_is_capped_at_it() {
    let ceiling = Duration::from_millis(250);
    assert!(
        support::SCRIPTED_RETRY_AFTER > ceiling,
        "this proves nothing unless the header really asks for longer"
    );
    let harness = Harness::new(Script::RateLimitedThenSettles).with_read_retry(
        BUDGET.0,
        Duration::from_millis(10),
        ceiling,
    );
    harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(harness.waits(), [ceiling]);
}

/// An absence *before* the mutation is knowledge, and is never waited on.
///
/// This is the asymmetry between the two read sites, and it is worth a case of
/// its own: `Ok(None)` at step 3 means the postcondition is not there yet and
/// the mutation is what fixes it. Waiting for it to change would put the whole
/// budget in front of every effect that has never run — a backoff that made the
/// ordinary path slower and nothing safer.
#[tokio::test]
async fn an_absence_before_the_mutation_is_not_waited_for() {
    let harness =
        Harness::new(Script::AbsentThenWritten).with_read_retry(BUDGET.0, BUDGET.1, BUDGET.2);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.calls(),
        ["inspect", "apply", "inspect"],
        "one look before, one after, and no waiting for either"
    );
    assert!(
        harness.waits().is_empty(),
        "an effect that has never run must not pay the budget, got {:?}",
        harness.waits()
    );
}

/// The schedule doubles, stays inside its ceiling, and never goes backwards.
///
/// Jitter is derived from the effect identity rather than from a random source —
/// it exists to decorrelate concurrent fiddle processes, and a backoff nobody
/// can reproduce is a backoff nobody can assert. The window is the lower half of
/// each step, which is what keeps the series non-decreasing.
#[tokio::test]
async fn the_backoff_doubles_within_its_ceiling() {
    let initial = Duration::from_millis(100);
    let ceiling = Duration::from_millis(400);
    let harness =
        Harness::new(Script::SuccessWithoutPostcondition).with_read_retry(6, initial, ceiling);
    harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    let waits = harness.waits();
    assert_eq!(waits.len(), 5, "one wait between each pair of reads");
    for (n, wait) in waits.iter().enumerate() {
        let step = (initial * 2u32.pow(n as u32)).min(ceiling);
        assert!(
            *wait >= step / 2 && *wait <= step,
            "wait {n} of {waits:?} must sit in the lower half of {step:?}"
        );
    }
    assert!(
        waits.windows(2).all(|pair| pair[1] >= pair[0]),
        "the series must never go backwards, got {waits:?}"
    );
    assert!(
        waits.iter().all(|wait| *wait <= ceiling),
        "nothing may exceed the ceiling, got {waits:?}"
    );
    assert!(
        *waits.last().unwrap() > waits[0],
        "and it must really have grown rather than stayed flat, got {waits:?}"
    );
}

/// `Script::ALL` really is all of them.
///
/// The sweep below is only worth as much as this: a world that escaped the array
/// would be a path nobody checked the dispatch count of. `Script::index` is an
/// exhaustive match, so a new variant cannot compile without a position, and
/// this asserts the positions are a bijection onto the array — which is what
/// stops a new variant being handed a number that already belongs to another.
#[test]
fn every_scripted_world_is_listed() {
    let mut seen = vec![false; Script::ALL.len()];
    for script in Script::ALL {
        let at = script.index();
        assert!(
            !std::mem::replace(&mut seen[at], true),
            "{script:?} shares position {at} with another world"
        );
        assert_eq!(
            Script::ALL[at].index(),
            at,
            "{script:?} is not at the position it claims"
        );
    }
    assert!(seen.into_iter().all(|listed| listed));
}

/// **The criterion the bean turns on, over every path there is.**
///
/// The cases above each assert the dispatch count for the situation they are
/// about. This one asserts it of *every* scripted world, with the budget
/// switched on for all of them, so a retry that slipped into an arm nobody wrote
/// a case for is caught by the sweep rather than by production.
///
/// The expected count is per-world rather than a blanket "at most one": two of
/// these worlds are settled before the mutation and must dispatch **zero**, so a
/// world that wrote when it should not have fails here rather than passing a
/// weaker bound.
#[tokio::test]
async fn every_path_dispatches_at_most_one_mutation() {
    for script in Script::ALL {
        let harness = Harness::new(script).with_read_retry(BUDGET.0, BUDGET.1, BUDGET.2);
        let _ = harness
            .executor()
            .execute(branch_effect(), harness.operation())
            .await;

        let expected = match script {
            // Nothing to do, or nothing that may be written over.
            Script::AlreadySatisfied | Script::TwoMatch => 0,
            _ => 1,
        };
        assert_eq!(
            harness.world.mutation_requests(),
            expected,
            "{script:?} dispatched the wrong number of mutations"
        );
        assert!(
            harness.world.mutations() <= 1,
            "{script:?} changed the world {} times",
            harness.world.mutations()
        );
    }

    // The two refusal paths, which no script reaches: a mutation policy stopped
    // must be dispatched zero times, and the read that runs before policy must
    // not have started a backoff on the way there either.
    for (minimum, rule) in [
        (HumanDecisionRequirement::Human, DeploymentRule::Allow),
        (HumanDecisionRequirement::Automatic, DeploymentRule::Deny),
    ] {
        let harness = Harness::new(Script::AbsentThenWritten)
            .with_read_retry(BUDGET.0, BUDGET.1, BUDGET.2)
            .with_policy(minimum, rule);
        harness
            .executor()
            .execute(branch_effect(), harness.operation())
            .await
            .expect_err("a refused effect must not succeed");
        assert_eq!(harness.world.mutation_requests(), 0);
        assert!(harness.waits().is_empty());
    }
}

/// Which failures are worth looking past, and which are the answer.
///
/// This is where `rate_limit_remaining` earns its keep. A permissions 403 and a
/// secondary-rate-limit 403 wear the same number, and the only thing that tells
/// them apart is what the response said about waiting: a spent allowance
/// refills, a missing permission does not.
#[test]
fn a_rate_limited_refusal_is_worth_reading_again_and_a_flat_refusal_is_not() {
    let http = |status, advice| GhError::Http {
        status,
        message: String::new(),
        advice,
    };
    let nothing_said = RetryAdvice::default();
    let allowance_spent = RetryAdvice {
        retry_after: None,
        rate_limit_remaining: Some(0),
    };
    let asked_to_wait = RetryAdvice {
        retry_after: Some(Duration::from_secs(1)),
        rate_limit_remaining: None,
    };

    assert!(
        !http(403, nothing_said).is_worth_reading_again(),
        "a permissions refusal is the answer"
    );
    assert!(
        http(403, allowance_spent).is_worth_reading_again(),
        "the same status with the allowance spent is `not just now`"
    );
    assert!(
        http(403, asked_to_wait).is_worth_reading_again(),
        "and so is one that named its own remedy"
    );
    assert!(http(429, nothing_said).is_worth_reading_again());
    assert!(http(500, nothing_said).is_worth_reading_again());
    assert!(!http(404, nothing_said).is_worth_reading_again());
    assert!(!http(422, nothing_said).is_worth_reading_again());

    assert!(GhError::Timeout(Duration::from_secs(1)).is_worth_reading_again());
    assert!(GhError::Killed("signal".to_string()).is_worth_reading_again());
    // The two cancellation provenances, and they answer this question
    // differently for the same reason they classify differently: one of them
    // has an answer that may exist to be read.
    assert!(
        !GhError::CancelledBeforeSpawn.is_worth_reading_again(),
        "nothing was started, so there is nothing to look for"
    );
    assert!(
        GhError::CancelledAfterSpawn.is_worth_reading_again(),
        "a request that may already have landed is settled by looking — and \
         `read_until_settled` still stops the run promptly, because it selects \
         on the token rather than on this answer"
    );
    assert!(!GhError::Auth.is_worth_reading_again());
    assert!(!GhError::NotSent(String::new()).is_worth_reading_again());
    // `Unknown` and still not worth another read, which is a pair no other
    // variant carries: a program that is not `gh` will not become one.
    assert!(!GhError::Malformed(String::new()).is_worth_reading_again());
    assert!(
        !GhError::Duplicate { count: 2 }.is_worth_reading_again(),
        "a second object does not become one object by being looked at again"
    );
}

// ---------------------------------------------------------------------------
// Policy, consumed
// ---------------------------------------------------------------------------

/// M2 has no decision channel, so a capability minimum demanding one fails
/// closed and names what would satisfy it. This is what stops the variant
/// shipping inert, the way `agent.max_capability_attempts` did.
#[tokio::test]
async fn a_human_decision_requirement_fails_closed_naming_m3() {
    let harness = Harness::new(Script::AbsentThenWritten)
        .with_policy(HumanDecisionRequirement::Human, DeploymentRule::Allow);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    let rendered = format!("{error}");
    assert!(
        matches!(error, EffectError::HumanDecisionRequired { .. }),
        "expected HumanDecisionRequired, got {error:?}"
    );
    assert!(
        rendered.contains("M3"),
        "a refusal must name what would satisfy it: {rendered}"
    );
    assert_eq!(
        harness.world.mutation_requests(),
        0,
        "a refused effect never happens"
    );
    assert_eq!(
        harness.world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
        ],
        "the walk stops at the combination; nothing is authorized"
    );
}

/// The deployment's own refusal is the other half of the same consumption.
#[tokio::test]
async fn a_denied_deployment_rule_refuses_before_the_mutation() {
    let harness = Harness::new(Script::AbsentThenWritten)
        .with_policy(HumanDecisionRequirement::Automatic, DeploymentRule::Deny);
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::PolicyDenied { .. }),
        "expected PolicyDenied, got {error:?}"
    );
    assert_eq!(harness.world.mutation_requests(), 0);
}

// ---------------------------------------------------------------------------
// Step 4's third input: a resolved human decision
// ---------------------------------------------------------------------------
//
// `combine` takes two inputs and the RFC's step 4 names three — "combine the
// capability's minimum effect rule with deployment policy *and, when needed,
// resolve a matching contextual human decision*". Until `execute_decided`
// existed this executor took two of them, so an operation whose `minimum()` is
// `Human` could not commit at all: step 4 refused, and `AuthorizedEffect` is
// unforgeable outside `crate::effect`, so `apply` had no other route to reach.
//
// Every case below asserts the mutation count as well as the outcome, for the
// reason the whole file does: what is being gated is a write, and a check that
// refused while the write happened anyway would satisfy an assertion about the
// error alone.

/// A revision, in the shape a marker carries one.
///
/// Filled in and never asserted on, because the executor does not compare it and
/// must not: the revision reaches the *identity* through the target —
/// `EnsurePullRequestReady`'s is `{repo}#{pr}@{head_sha}` — so a moved head is a
/// different `EffectId` and therefore a different question. Comparing the field
/// as well would be a second mechanism for one property, and the weaker one,
/// since an operation whose target omitted the revision would still pass it.
const DECIDED_HEAD: &str = "1f0e5d4c3b2a19876543210fedcba98765432100";

/// The effect `branch_effect()` proposes, recomputed the way a fresh process
/// would.
fn proposed_effect_id() -> EffectId {
    effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsureBranchPublished,
        TARGET,
    )
}

/// An approval addressed to one effect and one payload.
///
/// Built the way a continuation builds one: every value in the binding is
/// recomputed from canonical inputs rather than read out of a marker and
/// believed, and `ResolvedDecision::approved` is the only door past the verdict.
fn approval(effect: EffectId, payload: PayloadHash) -> ResolvedDecision {
    let request = decision_request_id(PROJECT, INVOCATION_REF, &effect);
    ResolvedDecision::approved(
        DecisionBinding {
            request,
            effect,
            payload,
            head_sha: DECIDED_HEAD.to_string(),
        },
        &InterpretedHumanDecision::Approve,
    )
    .expect("an approval is what a ResolvedDecision is made of")
}

/// A harness whose operation demands a person and whose document allows the
/// kind. `combine(Human, Allow)` is `RequireHumanDecision`, which is the only
/// cell that reaches step 4's third input at all.
fn gated_on_a_person() -> Harness {
    Harness::new(Script::AbsentThenWritten)
        .with_policy(HumanDecisionRequirement::Human, DeploymentRule::Allow)
}

/// A verdict that is not an approval never becomes a `ResolvedDecision`.
///
/// The gate in the type rather than in the executor, and worth its own case: a
/// function taking a bare `DecisionBinding` would accept the *question* as though
/// it were the answer, since a binding is what the marker in a request comment
/// carries and rendering one requires nobody's agreement.
#[test]
fn only_an_approval_becomes_a_resolved_decision() {
    let binding = || DecisionBinding {
        request: decision_request_id(PROJECT, INVOCATION_REF, &proposed_effect_id()),
        effect: proposed_effect_id(),
        payload: payload_hash(PAYLOAD),
        head_sha: DECIDED_HEAD.to_string(),
    };

    assert!(
        ResolvedDecision::approved(binding(), &InterpretedHumanDecision::Approve).is_some(),
        "an approval is the one verdict that does"
    );
    for refused in [
        InterpretedHumanDecision::Reject {
            reason: Published::of("not yet"),
        },
        InterpretedHumanDecision::Redirect {
            instruction: Published::of("do it differently"),
        },
        InterpretedHumanDecision::Unclear,
    ] {
        assert!(
            ResolvedDecision::approved(binding(), &refused).is_none(),
            "{refused:?} must not be convertible into something step 4 would spend"
        );
    }
}

/// The gate, unchanged: with no decision, a `Human` minimum still refuses.
///
/// M2's behaviour, and it must survive, because a run with no decision channel
/// must not silently acquire one. The step assertion is the new half — nothing
/// was resolved, so no step may announce a resolution.
#[tokio::test]
async fn a_human_minimum_with_no_decision_still_refuses() {
    let harness = gated_on_a_person();
    let error = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::HumanDecisionRequired { .. }),
        "expected HumanDecisionRequired, got {error:?}"
    );
    assert_eq!(harness.world.mutations(), 0);
    assert_eq!(harness.world.mutation_requests(), 0);
    assert!(
        !harness.world.steps().contains(&"resolve_decision"),
        "there was no decision to resolve, so no step may announce one: {:?}",
        harness.world.steps()
    );
}

/// A decision naming this exact effect satisfies step 4, and only then does the
/// mutation happen.
///
/// The decision is an input to the executor rather than a property of the
/// operation, so the check cannot be bypassed by however the operation was built:
/// the operation here is the same one the case above was refused with, and its
/// `minimum()` is still `Human`.
#[tokio::test]
async fn a_decision_naming_this_effect_permits_the_mutation() {
    let harness = gated_on_a_person();
    let decision = approval(proposed_effect_id(), payload_hash(PAYLOAD));

    let receipt = harness
        .executor()
        .execute_decided(branch_effect(), harness.operation(), &decision)
        .await
        .expect("a decision naming this exact effect satisfies step 4");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(harness.world.mutations(), 1);
    assert_eq!(
        harness.world.mutation_requests(),
        1,
        "and exactly once, as on every other path in this file"
    );
    assert_eq!(
        harness.world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
            "resolve_decision",
            "authorize",
            "apply",
            "observe_postcondition",
        ],
        "the resolution is announced after the combination that asked for it and \
         before the envelope it unlocks"
    );
}

/// A decision for a *different* effect buys nothing.
///
/// This is the property the revision-in-the-target design rests on: a moved head
/// derives a different `EffectId`, so an approval given for the old revision is
/// not an answer to the new question, and the executor is where that is enforced
/// rather than trusted.
///
/// The refusal is `HumanDecisionRequired` rather than a denial, and the
/// difference is what an operator does next: the current question genuinely has
/// not been answered, which is `Awaiting` and exit 10 — "go and answer it" —
/// rather than exit 20 and "this has concluded".
#[tokio::test]
async fn a_decision_naming_another_effect_is_refused() {
    let harness = gated_on_a_person();
    let elsewhere = effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsureBranchPublished,
        "refs/heads/fiddle/somewhere-else",
    );
    assert_ne!(
        elsewhere,
        proposed_effect_id(),
        "this proves nothing unless the two identities really differ"
    );
    let stale = approval(elsewhere.clone(), payload_hash(PAYLOAD));

    let error = harness
        .executor()
        .execute_decided(branch_effect(), harness.operation(), &stale)
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::HumanDecisionRequired { .. }),
        "expected HumanDecisionRequired, got {error:?}"
    );
    assert_eq!(harness.world.mutations(), 0);
    assert_eq!(harness.world.mutation_requests(), 0);
    // Which comparison failed, named: an operator reading this has to be able to
    // tell a stale approval from an absent one.
    let rendered = format!("{error}");
    assert!(
        rendered.contains(&elsewhere.0) && rendered.contains(&proposed_effect_id().0),
        "the refusal must name both identities: {rendered}"
    );
}

/// `Deny` is absolute and an approval cannot buy it.
///
/// `combine` already orders its arms this way — `(_, Deny)` is `Deny` whatever
/// the minimum — and this asserts the *executor* honours that ordering rather
/// than checking the decision first. The step assertion is what makes it an
/// ordering claim instead of an outcome claim: a build that read the decision,
/// found it good, and only then noticed the denial would reach the same error
/// while announcing a resolution it had no business performing.
#[tokio::test]
async fn an_approval_cannot_buy_a_denied_effect() {
    let harness = Harness::new(Script::AbsentThenWritten)
        .with_policy(HumanDecisionRequirement::Human, DeploymentRule::Deny);
    let decision = approval(proposed_effect_id(), payload_hash(PAYLOAD));

    let error = harness
        .executor()
        .execute_decided(branch_effect(), harness.operation(), &decision)
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::PolicyDenied { .. }),
        "expected PolicyDenied, got {error:?}"
    );
    assert_eq!(harness.world.mutations(), 0);
    assert_eq!(harness.world.mutation_requests(), 0);
    assert!(
        !harness.world.steps().contains(&"resolve_decision"),
        "a denied effect is refused before the decision is read: {:?}",
        harness.world.steps()
    );
}

/// The payload half: a decision resolved for one request does not license
/// another.
///
/// The cross-process reading of the identity/payload split that step 6 can only
/// check within one call. The identity is derived from the target and never from
/// the payload — deliberately, so that rewording a pull request does not open a
/// second one — so an approval and a request can agree about *which* effect this
/// is while disagreeing about *what is being done*.
///
/// **The proposal and the operation agree here, and that is the point of the
/// arrangement.** `payload_divergence.rs` is where those two are made to
/// disagree, and step 6 catches it there. Were this case to move the *request's*
/// payload instead of the decision's, step 6 would refuse it and the case would
/// still pass with step 4's comparison deleted — an assertion about a check that
/// was not running. Only the decision disagrees, so only step 4 can refuse it.
#[tokio::test]
async fn a_decision_does_not_license_a_widened_payload() {
    let harness = gated_on_a_person();
    let another_request = r#"{"sha":"cafebabe"}"#;
    assert_ne!(
        payload_hash(another_request),
        payload_hash(PAYLOAD),
        "the two requests must really differ"
    );
    let decision = approval(proposed_effect_id(), payload_hash(another_request));

    let error = harness
        .executor()
        .execute_decided(branch_effect(), harness.operation(), &decision)
        .await
        .unwrap_err();

    assert!(
        matches!(
            &error,
            EffectError::PayloadDiverged { approved, applying, .. }
                if approved == &payload_hash(another_request)
                    && applying == &payload_hash(PAYLOAD)
        ),
        "expected PayloadDiverged carrying the digest the person was shown and the \
         one this call would apply, got {error:?}"
    );
    assert_eq!(harness.world.mutations(), 0);
    assert_eq!(harness.world.mutation_requests(), 0);
}

/// An `Automatic` operation is unaffected by the new path.
///
/// Passing a decision to something that needed none changes nothing, so a caller
/// cannot make an ungated effect *look* approved — and, read the other way, the
/// decided path did not become a route around step 4. `combine` answered
/// `Allow`, nothing was gated, and the binding was never inspected, which the
/// absent step is what shows.
#[tokio::test]
async fn a_decision_changes_nothing_for_an_automatic_operation() {
    // The harness's default policy is `Automatic` against `Allow`.
    let harness = Harness::new(Script::AbsentThenWritten);
    let decision = approval(proposed_effect_id(), payload_hash(PAYLOAD));

    let receipt = harness
        .executor()
        .execute_decided(branch_effect(), harness.operation(), &decision)
        .await
        .expect("an ungated effect is unaffected by an approval it did not need");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(harness.world.mutations(), 1);
    assert!(
        !harness.world.steps().contains(&"resolve_decision"),
        "nothing was gated, so nothing was resolved: {:?}",
        harness.world.steps()
    );
}

/// The two entry points walk one order, and the decision is the only difference.
///
/// Asserted as a pair rather than left to the two cases above, because the claim
/// is about the *walk* and not about either call: the same operation under the
/// same document reaches `combine_policy` by an identical route, and what happens
/// after it is the only thing a third input may change. Two copies of the order —
/// the shape this deliberately did not take — would satisfy every case above
/// while being free to drift here.
#[tokio::test]
async fn the_decided_path_differs_from_the_undecided_one_only_at_step_four() {
    let undecided = gated_on_a_person();
    undecided
        .executor()
        .execute(branch_effect(), undecided.operation())
        .await
        .expect_err("no decision, so the requirement stands unmet");

    let decided = gated_on_a_person();
    let decision = approval(proposed_effect_id(), payload_hash(PAYLOAD));
    decided
        .executor()
        .execute_decided(branch_effect(), decided.operation(), &decision)
        .await
        .expect("the same walk, with the third input supplied");

    let shared = [
        "validate_capability",
        "derive_identity",
        "inspect_postcondition",
        "combine_policy",
    ];
    assert_eq!(
        undecided.world.steps(),
        shared,
        "the undecided walk stops at the combination"
    );
    assert_eq!(
        decided.world.steps()[..shared.len()],
        shared,
        "and the decided walk reaches it by exactly the same route"
    );
    assert_eq!(
        decided.world.steps()[shared.len()..],
        [
            "resolve_decision",
            "authorize",
            "apply",
            "observe_postcondition"
        ],
        "continuing only because a decision answered what the combination asked"
    );
}

/// A capability cannot claim another capability's identity when proposing.
#[tokio::test]
async fn an_executor_is_bound_to_one_capability() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let error = harness
        .executor()
        .execute(proposed_by(STUB_MARK), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::PolicyDenied { .. }),
        "expected PolicyDenied, got {error:?}"
    );
    let rendered = format!("{error}");
    assert!(
        rendered.contains("fixture_repair") && rendered.contains("stub_mark"),
        "the refusal must name both capabilities: {rendered}"
    );
    assert_eq!(
        harness.world.calls(),
        Vec::<&str>::new(),
        "validation precedes every look at the world"
    );
    assert_eq!(
        harness.world.steps(),
        ["validate_capability"],
        "and precedes every other step"
    );
}

// ---------------------------------------------------------------------------
// The receipt
// ---------------------------------------------------------------------------

/// The receipt carries the identity a *fresh* process would recompute, so a
/// later run can recognise this effect with nothing but its canonical inputs.
#[tokio::test]
async fn the_receipt_carries_the_recomputable_identity_and_payload_hash() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let receipt = harness
        .executor()
        .execute(branch_effect(), harness.operation())
        .await
        .unwrap();

    assert_eq!(
        receipt.effect_id,
        effect_id(
            PROJECT,
            INVOCATION_REF,
            EffectKind::EnsureBranchPublished,
            TARGET
        )
    );
    assert_eq!(receipt.payload_hash, payload_hash(PAYLOAD));
    assert_eq!(receipt.target, TARGET);
    assert_eq!(receipt.value, "deadbeef");
}

// ---------------------------------------------------------------------------
// One real branch
// ---------------------------------------------------------------------------

/// The repository the scripted `gh` answers for, and the one the API paths name.
const REPO: &str = "o/r";

/// A generous bound for children that answer immediately. Nothing in this half
/// is about the deadline; `github_cli` and `git_publish` own the process bounds
/// and this file inherits them rather than restating them.
const PATIENT: Duration = Duration::from_secs(60);

/// Run a setup `git` in `dir` and insist it succeeded.
///
/// Setup runs under the ambient environment on purpose — it is the test
/// arranging a world, not the code under test — but identity and the initial
/// branch are pinned with `-c` so that an operator's global configuration
/// cannot change what the fixture is.
fn git_setup(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.email=fiddle@example.invalid",
            "-c",
            "user.name=fiddle",
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git is on PATH for the test process");
    assert!(
        output.status.success(),
        "setup `git {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// One `git` question, answered as a trimmed string.
fn git_says(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The world one branch effect runs against: a bare repository standing in for
/// GitHub, a worktree holding the work, and the scripted `gh` that answers reads
/// out of the first of those.
///
/// The two adapters see the *same* remote through different doors — `git` writes
/// to it over a path, the scripted `gh` reads its ref files — which is what makes
/// "the postcondition was read back rather than assumed" a real claim here. A
/// stub answering from its own memory of what it had been asked would agree with
/// a push that never happened.
struct Remote {
    dir: TempDir,
    remote: PathBuf,
    work: PathBuf,
    steps: Mutex<Vec<&'static str>>,
}

impl EffectTrace for Remote {
    fn step(&self, _kind: EffectKind, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl Remote {
    /// An empty remote and a worktree with one commit pointing at it.
    fn empty() -> Self {
        let dir = TempDir::new().unwrap();
        // `remote.git` is the name the scripted `gh` looks for beside its own
        // scratch directory; see `tests/gh_stub/gh_stub.rs`.
        let remote = dir.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git_setup(&remote, &["init", "-q", "--bare", "."]);
        // Empty, and stays empty: it is what a real `gh` would be pinned to.
        std::fs::create_dir_all(dir.path().join("config")).unwrap();

        let this = Self {
            work: dir.path().join("work"),
            remote,
            dir,
            steps: Mutex::new(Vec::new()),
        };
        this.worktree("work", "one");
        this
    }

    /// A working repository with one commit whose content is `content`, and an
    /// `origin` pointing at the bare repository.
    fn worktree(&self, name: &str, content: &str) -> PathBuf {
        let work = self.dir.path().join(name);
        std::fs::create_dir_all(&work).unwrap();
        git_setup(&work, &["init", "-q", "."]);
        std::fs::write(work.join("file"), content).unwrap();
        git_setup(&work, &["add", "file"]);
        git_setup(&work, &["commit", "-q", "-m", name]);
        git_setup(
            &work,
            &[
                "remote",
                "add",
                "origin",
                &self.remote.display().to_string(),
            ],
        );
        work
    }

    /// Put `worktree`'s commit on `branch` before the effect runs.
    ///
    /// Arranged with the test's own `git` rather than with the adapter under
    /// test, so a world this file claims to have built is not built by the code
    /// the assertions are about.
    fn seed(&self, worktree: &Path, branch: &str) {
        git_setup(
            worktree,
            &["push", "-q", "origin", &format!("HEAD:refs/heads/{branch}")],
        );
    }

    /// The commit the work is sitting on: what a publish intends.
    fn head(&self) -> String {
        git_says(&self.work, &["rev-parse", "HEAD"])
    }

    /// Every branch the remote holds, in ref order.
    fn branches(&self) -> Vec<String> {
        git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    /// What one branch of the remote points at.
    fn branch_sha(&self, branch: &str) -> String {
        git_says(
            &self.remote,
            &["rev-parse", &format!("refs/heads/{branch}")],
        )
    }

    /// How many pushes were dispatched, counted from what the pushing `git`
    /// wrote down rather than inferred from the remote.
    ///
    /// The number a duplicate hides behind: an `Unknown` resolved by retrying
    /// the mutation instead of by reading the world shows up here as two, and
    /// leaves a remote that looks exactly the same either way.
    fn pushes(&self) -> usize {
        std::fs::read_to_string(self.work.join("pushes"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    /// A context whose `gh` is the scripted one and whose `git` is the real one.
    fn context(&self) -> EffectContext {
        self.context_with(
            PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
            PathBuf::from("git"),
            PATIENT,
        )
    }

    /// The same, with the `gh` program named — so a `gh` that cannot answer at
    /// all can be handed to the same operation.
    fn context_reading_with(&self, gh: PathBuf) -> EffectContext {
        self.context_with(gh, PathBuf::from("git"), PATIENT)
    }

    /// A context whose pushes go through the recording `git` in `mode`, under
    /// `timeout`.
    ///
    /// The mode is written into the worktree because that is the only channel
    /// the fixture has: the push environment is pinned to seven names and its
    /// argument vector is asserted exactly, so the working directory is what is
    /// left. See `tests/git_stub/git_stub.rs`.
    fn context_pushing_with(&self, mode: &str, timeout: Duration) -> EffectContext {
        std::fs::write(self.work.join("mode"), mode).unwrap();
        self.context_with(
            PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
            PathBuf::from(env!("CARGO_BIN_EXE_git_stub")),
            timeout,
        )
    }

    fn context_with(&self, gh: PathBuf, git: PathBuf, timeout: Duration) -> EffectContext {
        EffectContext::new(
            GhCli::new(
                gh,
                // The scratch directory arrives in `argv` because the adapter's
                // environment has room for exactly five names; see
                // `tests/gh_stub/gh_stub.rs`.
                vec![
                    "--stub-dir".to_string(),
                    self.dir.path().display().to_string(),
                ],
                "ghp_never_reaches_a_network".to_string(),
                "FIDDLE_GITHUB_TOKEN",
                self.dir.path().join("config"),
                PATIENT,
            ),
            GitCli::new(
                git,
                // Never used: a path remote authenticates nobody, which is what
                // keeps this lane credential-free while still running the exact
                // environment the product builds.
                "ghp_never_used_by_a_path_remote".to_string(),
                "FIDDLE_GITHUB_TOKEN",
                timeout,
            ),
            self.work.clone(),
            CancellationToken::new(),
        )
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    /// How many times the executor dispatched the mutation, from the step trace.
    fn applies(&self) -> usize {
        self.steps().iter().filter(|step| **step == "apply").count()
    }
}

/// The branch this run publishes, recomputed the way a fresh process would.
fn published_branch() -> String {
    branch_name(PROJECT, INVOCATION_REF)
}

/// The operation under test, aimed at `intended`.
fn branch_operation(intended: &str) -> EnsureBranchPublished {
    EnsureBranchPublished::new(REPO.to_string(), published_branch(), intended.to_string())
}

/// Walk the authorization order for one branch effect.
async fn publish_the_branch<O>(
    remote: &Remote,
    ctx: &EffectContext,
    intended: &str,
    operation: O,
) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, EffectError>
where
    O: IntegrationOperation,
{
    let deployment = Deployment(DeploymentRule::Allow);
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectKind::EnsureBranchPublished,
        target: fiddle_runtime::github::branch_target(&published_branch()),
        payload: serde_json::json!({ "repo": REPO, "sha": intended }).to_string(),
    };
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        ctx,
        remote,
        // One read and no waiting. The branch half's subject is the operation
        // against a real repository; the budget is the protocol half's, and it
        // says so at each of these three sites rather than inheriting one.
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
}

/// The name is the durable remote locator — the thing a fresh process has to
/// find a branch by after its own answer was lost — so it must fall out of
/// canonical inputs and nothing else.
///
/// The syntax assertions are not decoration. A name is rejected by git if it
/// contains `..`, ends a component in `.lock`, or begins with `-`, and a name
/// that reached the far end and failed there would be a failure this adapter
/// could have prevented. They hold *structurally* here — the digest is hex — but
/// they are asserted because the construction is what guarantees it, and a
/// construction can be changed. `an_absent_ref_is_published_and_then_read_back`
/// is the other half of the same claim: it pushes this exact name through
/// `GitCli::publish`, whose own boundary check is the one a real `git` would
/// have applied.
#[test]
fn the_branch_name_is_derived_and_stable() {
    let first = branch_name("acme/widget", "beans:w-1");
    assert_eq!(first, branch_name("acme/widget", "beans:w-1"));
    assert!(
        first.starts_with("fiddle/"),
        "namespaced, so a human can see whose it is: {first}"
    );
    // Both canonical inputs move the name, or two runs would publish over each
    // other's work under one ref.
    assert_ne!(first, branch_name("acme/widget", "beans:w-2"));
    assert_ne!(first, branch_name("acme/other", "beans:w-1"));

    // The identity's own derivation, reused rather than a second hash invented.
    assert_eq!(
        first,
        format!(
            "fiddle/{}",
            effect_id(
                "acme/widget",
                "beans:w-1",
                EffectKind::EnsureBranchPublished,
                "acme/widget"
            )
            .0
        )
    );

    // git's own ref rules.
    assert!(!first.contains(".."));
    assert!(!first.ends_with(".lock"));
    assert!(!first.split('/').any(|part| part.ends_with(".lock")));
    assert!(!first.starts_with(['-', '.', '/']));
    assert!(!first.ends_with(['.', '/']));
    assert!(first
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/-_".contains(c)));

    // Every input, not only the well-behaved ones: the encoding underneath is
    // length-prefixed, so a project carrying a separator, a NUL or a refspec
    // metacharacter still produces a name git will take.
    for project in [
        "",
        "a\0b",
        "+force:me",
        "../../etc",
        "x".repeat(500).as_str(),
    ] {
        let name = branch_name(project, "beans:w-1");
        assert!(
            name.strip_prefix("fiddle/")
                .is_some_and(|id| id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit())),
            "{project:?} produced {name}"
        );
    }
}

/// An absent ref is the only state that licenses a push — and a 404 is how the
/// remote says so.
///
/// This is `m2-branch-404-is-knowledge` in its ordinary form: the first
/// inspection of an empty remote is a 404, and it has to come back as "not
/// there" rather than as a failure to look, or the very first publish of every
/// run would fail closed.
#[tokio::test]
async fn an_absent_ref_is_published_and_then_read_back() {
    let remote = Remote::empty();
    let ctx = remote.context();
    let head = remote.head();

    let receipt = publish_the_branch(&remote, &ctx, &head, branch_operation(&head))
        .await
        .expect("an absent ref is published");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        remote.branches(),
        [published_branch()],
        "exactly one branch, at the deterministic name"
    );
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some(head.as_str()),
        "the observed sha, read back out of the remote rather than assumed"
    );
    assert_eq!(receipt.value.branch, published_branch());
    assert_eq!(receipt.value.sha, head);
    assert_eq!(
        remote.steps().last(),
        Some(&"observe_postcondition"),
        "the receipt is built from the read that follows the push"
    );
}

/// The same rule stated on its own, and its fail-closed edge beside it.
///
/// A 404 is a read that *succeeded* and returned an absence; a `gh` that could
/// not answer at all is a source that could not be read. M0's rule is that the
/// two are never equivalent, and this is that rule at the GitHub boundary: the
/// first is `Ok(None)` and licenses a push, the second is an error and stops
/// one. Collapsing them in either direction is a defect — one way the first
/// publish never happens, the other way an outage looks like an empty remote and
/// gets pushed over.
#[tokio::test]
async fn a_404_is_knowledge_and_an_unreadable_source_is_not() {
    let remote = Remote::empty();
    let operation = branch_operation(&remote.head());

    assert_eq!(
        operation.inspect(&remote.context()).await.unwrap(),
        None,
        "the remote answered 404: the ref is absent, and that is knowledge"
    );

    let unreadable = remote.context_reading_with(PathBuf::from("/nonexistent/gh"));
    let error = operation
        .inspect(&unreadable)
        .await
        .expect_err("a source that could not be read is never an absent ref");
    assert!(
        matches!(error, GhError::Malformed(_)),
        "expected the read to fail, got {error:?}"
    );
}

/// A ref already at the intended sha is the postcondition, not a conflict.
///
/// The steps are asserted rather than a push count, which is the stronger
/// claim: not merely that nothing landed, but that the executor never dispatched
/// a mutation at all. A fresh process meeting the world a previous one built is
/// exactly this case, and it is the whole of the recovery.
#[tokio::test]
async fn a_ref_already_at_the_intended_sha_is_already_satisfied() {
    let remote = Remote::empty();
    let head = remote.head();
    remote.seed(&remote.work, &published_branch());

    let ctx = remote.context();
    let receipt = publish_the_branch(&remote, &ctx, &head, branch_operation(&head))
        .await
        .expect("the postcondition already holds");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.external_ref.as_deref(), Some(head.as_str()));
    assert_eq!(
        remote.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition"
        ],
        "the walk stops at the inspection; nothing is pushed and nothing is even \
         put to policy"
    );
    assert_eq!(remote.branches(), [published_branch()]);
    assert_eq!(remote.branch_sha(&published_branch()), head);
}

/// A ref at the deterministic name pointing somewhere else is the case §5.5
/// deliberately left to git rather than to a commit trailer: the push is
/// attempted and refused as a non-fast-forward, so a divergent branch is
/// reported and never overwritten.
///
/// This is the assertion that the dropped ownership check left no hole, and it
/// has to be asked of a real `git` against a real remote — a fixture answering
/// "rejected" would be this file agreeing with its own belief about git, when
/// git's behaviour is the entire load-bearing claim.
///
/// Three things are asserted, and they are three different claims. The refusal
/// carries git's own verdict rather than a generic failure, so a caller can tell
/// a divergence from an outage. The ref still points where it did, so nothing
/// was forced. And no second branch appeared, so nothing routed around the
/// refusal by publishing elsewhere.
#[tokio::test]
async fn a_ref_at_our_name_pointing_elsewhere_is_refused_not_overwritten() {
    let remote = Remote::empty();
    let other = remote.worktree("other", "another");
    let theirs = git_says(&other, &["rev-parse", "HEAD"]);
    remote.seed(&other, &published_branch());
    let head = remote.head();
    assert_ne!(head, theirs, "the two worktrees must really diverge");

    let ctx = remote.context();
    let error = publish_the_branch(&remote, &ctx, &head, branch_operation(&head))
        .await
        .expect_err("a ref that is not an ancestor cannot fast-forward");

    assert!(
        matches!(
            error,
            EffectError::Adapter {
                source: GhError::Push(GitError::NonFastForward { .. }),
                ..
            }
        ),
        "expected git's own non-fast-forward verdict, got {error:?}"
    );
    assert_eq!(
        remote.branch_sha(&published_branch()),
        theirs,
        "the refused push must not have moved the ref"
    );
    assert_eq!(
        remote.branches(),
        [published_branch()],
        "and must not have added one beside it"
    );
    assert!(
        remote.steps().contains(&"apply"),
        "the judgment belongs to git, so the push has to actually be attempted"
    );
}

// ---------------------------------------------------------------------------
// An ambiguous push, resolved by looking
// ---------------------------------------------------------------------------

/// A deadline short enough that a `git` which never answers is ended by it, and
/// long enough that a `git` which does answer is not raced against it.
const IMPATIENT: Duration = Duration::from_secs(3);

/// The milestone's central rule, against a push that really happened.
///
/// This is the state the whole design turns on and the one no scripted world
/// can honestly stand in for: the pack reached the remote, the ref moved, and
/// then the child died before it could say so. `GitError::outcome` calls that
/// `Unknown` — not `NotCommitted` — precisely so the executor goes and looks
/// instead of concluding, because a landed write reported as failed is retried
/// into the duplicate this milestone exists to prevent.
///
/// The push is a real `git` against a real bare repository, interposed on only
/// to take the answer away afterwards. So the ref the postcondition read finds
/// is one that genuinely got there, and "resolved by reading" is a claim about
/// the system rather than about the harness.
#[tokio::test]
async fn a_push_that_landed_before_its_answer_was_lost_is_resolved_by_reading() {
    // The hazard is demonstrated before it is relied on. Both halves of an
    // ambiguous write have to be real or this test would pass on a push that
    // simply succeeded — the executor reaches `Committed` either way, and only
    // this witness says which route it took. A separate remote, because a push
    // into the one under test would satisfy the postcondition before the
    // executor ever ran.
    let witness = Remote::empty();
    let wctx = witness.context_pushing_with("push_then_killed", PATIENT);
    let lost = wctx
        .git
        .publish(&wctx.work, &published_branch(), &wctx.cancel)
        .await
        .expect_err("the fixture must really lose the answer, or it proves nothing");
    assert!(
        matches!(lost, GitError::Killed),
        "expected a child that died without answering, got {lost:?}"
    );
    assert_eq!(
        lost.outcome(),
        EffectOutcome::Unknown,
        "and it must classify Unknown, or the executor would never go and look"
    );
    assert_eq!(
        witness.branch_sha(&published_branch()),
        witness.head(),
        "and the write must really have landed, or the answer was all that was lost"
    );

    let remote = Remote::empty();
    let head = remote.head();
    let ctx = remote.context_pushing_with("push_then_killed", PATIENT);

    let receipt = publish_the_branch(&remote, &ctx, &head, branch_operation(&head))
        .await
        .expect("the answer was lost, not the write");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some(head.as_str()),
        "the sha comes from the read, since the push never reported one"
    );
    assert_eq!(
        remote.branches(),
        [published_branch()],
        "exactly one branch — the property, stated as an object count"
    );
    assert_eq!(remote.branch_sha(&published_branch()), head);
    assert_eq!(
        remote.pushes(),
        1,
        "the mutation was dispatched exactly once; an Unknown settled by \
         retrying instead of by reading would show up here as two"
    );
    assert_eq!(
        remote.applies(),
        1,
        "and the executor agrees it dispatched once"
    );
}

/// The other direction: the answer was lost and nothing is behind it.
///
/// A deadline the runtime imposed is the second failure that classifies
/// `Unknown`, and it reaches the same rule from the other side — the read finds
/// no ref, so nothing settles the question and the effect stays `Unresolved`.
/// Deliberately not "failed": the push may still be in flight, and a caller told
/// it failed would retry a write that could yet land. The push is still
/// dispatched exactly once, because an unresolved outcome is not a licence to
/// try again either.
#[tokio::test]
async fn a_push_whose_answer_was_lost_with_no_ref_behind_it_stays_unresolved() {
    let remote = Remote::empty();
    let head = remote.head();
    let ctx = remote.context_pushing_with("never_answers", IMPATIENT);

    let error = publish_the_branch(&remote, &ctx, &head, branch_operation(&head))
        .await
        .expect_err("nothing was observed, so nothing is confirmed");

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "expected Unresolved rather than a confident answer, got {error:?}"
    );
    let rendered = format!("{error}");
    assert!(
        rendered.contains("answer was lost"),
        "a caller has to be able to tell this from a settled failure: {rendered}"
    );
    // The hazard, demonstrated rather than assumed. A `git` that answered
    // promptly and did nothing would also leave the ref absent and also reach
    // `Unresolved` — by the arm that means "the adapter claimed success", which
    // is a different rule entirely. Naming the deadline in the message is what
    // distinguishes the two, so this test cannot pass on a fixture whose sleep
    // stopped working.
    assert!(
        rendered.contains("timeout"),
        "the answer has to have been really lost, to a deadline this runtime \
         imposed: {rendered}"
    );
    assert!(
        remote.branches().is_empty(),
        "no ref was created, and none was invented by reading"
    );
    assert_eq!(
        remote.pushes(),
        1,
        "an unresolved outcome is never resolved by dispatching again"
    );
    assert_eq!(remote.applies(), 1);
}

// ---------------------------------------------------------------------------
// The capability: where eight tasks of parts become one thing a run can do
// ---------------------------------------------------------------------------
//
// Everything above asks what the executor does with *one* effect. This half asks
// what a capability does with three of them, and it is the first place in the
// milestone with a product caller: `EnsureBranchPublished`, `EnsurePullRequest`,
// `EnsureCheckRequested` and `observe_checks` were all exercised only by tests
// until this section, and `ReviewState` was constructed nowhere at all.
//
// It runs the whole attempt — `fiddle_runtime::attempt`, not `PublishChange`
// alone — because four of the seven things this task has to be true about are
// properties of the *published bundle*, and a test that called the capability
// and inspected its return value would be asserting them of a value nobody
// publishes. Still offline and still credential-free: the remote is a bare
// repository on a path, the `gh` is the scripted one, and a path remote
// authenticates nobody.

/// The owner the head branch lives under, which is [`REPO`]'s own owner here.
const HEAD_OWNER: &str = "o";

/// The branch a publication is proposed into.
const BASE: &str = "main";

/// The workflow a check is requested from, spelled as the API path spells it.
const WORKFLOW: &str = "fiddle-check.yml";

/// The check a reader of this run's verification requires by name.
const REQUIRED_CHECK: &str = "build";

/// A deployment that refuses exactly one kind and allows the rest.
///
/// One kind rather than all of them, because the property under test is about
/// *ordering*: a policy that denied everything would stop the sequence at its
/// first step and say nothing about whether a refusal half way through stops
/// what comes after it.
struct Denying(EffectKind);

impl fiddle_runtime::effect::DeploymentPolicy for Denying {
    fn rule_for(&self, kind: EffectKind) -> DeploymentRule {
        match kind == self.0 {
            true => DeploymentRule::Deny,
            false => DeploymentRule::Allow,
        }
    }
}

/// Everything the ports and the publication need that the remote does not hold:
/// the fixture the work item is read from and the directory bundles land in.
struct Local {
    dir: TempDir,
}

impl Local {
    /// A fixture holding this invocation's work item and no change set.
    fn new(work_id: &str) -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("work")).unwrap();
        std::fs::create_dir_all(dir.path().join("changes")).unwrap();
        std::fs::write(
            dir.path().join(format!("work/{work_id}.json")),
            format!(r#"{{"id":"{work_id}","status":"open"}}"#),
        )
        .unwrap();
        Self { dir }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn reports(&self) -> PathBuf {
        self.dir.path().join("reports")
    }

    /// Remove every local artifact a previous attempt left: the change set, the
    /// bundles, the journal.
    ///
    /// What survives is the *world*, which is the only thing a fresh process is
    /// entitled to recognise its own work from. A retry that needed any of this
    /// would be remembering rather than recomputing, and would duplicate every
    /// effect the moment a machine was replaced.
    fn forget(&self) {
        let _ = std::fs::remove_dir_all(self.dir.path().join("changes"));
        let _ = std::fs::remove_dir_all(self.reports());
        std::fs::create_dir_all(self.dir.path().join("changes")).unwrap();
    }
}

/// What the scripted world holds and what it was asked for, read back out of the
/// stub's own files.
///
/// Read from the world the requests built rather than from anything the code
/// under test returned, so an assertion about how many objects exist is an
/// assertion about the world.
impl Remote {
    /// Seed one check run at one exact head, before the run under test starts.
    fn check(&self, name: &str, status: &str, conclusion: Option<&str>, head_sha: &str) {
        let path = self.dir.path().join("checks_seed");
        let mut seed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_default();
        seed.push(serde_json::json!({
            "name": name,
            "status": status,
            "conclusion": conclusion,
            "head_sha": head_sha,
        }));
        std::fs::write(&path, serde_json::Value::Array(seed).to_string()).unwrap();
    }

    /// Every request the scripted `gh` recorded, in arrival order.
    fn requests(&self) -> Vec<serde_json::Value> {
        let dir = self.dir.path().join("requests");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        files
            .iter()
            .filter_map(|file| serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok())
            .collect()
    }

    /// How many mutations of one shape were *asked for*, landed or not.
    ///
    /// Counted from the requests rather than from the objects, because that is
    /// the number a sequence that failed to stop would move and the object count
    /// might not.
    fn posts_to(&self, suffix: &str) -> usize {
        self.requests()
            .iter()
            .filter(|request| {
                let argv: Vec<String> = request["argv"]
                    .as_array()
                    .map(|argv| {
                        argv.iter()
                            .filter_map(|a| a.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                argv.iter().any(|a| a == "POST")
                    && argv.iter().any(|a| a.trim_end().ends_with(suffix))
            })
            .count()
    }

    fn pull_request_creates(&self) -> usize {
        self.posts_to("/pulls")
    }

    fn dispatch_requests(&self) -> usize {
        self.posts_to("/dispatches")
    }

    /// Every mutation that actually changed the world, of one shape.
    fn landed(&self, needle: &str) -> usize {
        std::fs::read_to_string(self.dir.path().join("world"))
            .unwrap_or_default()
            .lines()
            .filter(|line| {
                serde_json::from_str::<serde_json::Value>(line)
                    .ok()
                    .and_then(|w| w["key"].as_str().map(|key| key.contains(needle)))
                    .unwrap_or(false)
            })
            .count()
    }
}

/// The branch, the pull request target and the check target this run's identity
/// produces, recomputed the way a fresh process would.
fn publish_targets() -> (String, String, String) {
    let branch = branch_name(PROJECT, INVOCATION_REF);
    let pull = fiddle_runtime::github::pull_request_target(REPO, HEAD_OWNER, &branch, BASE);
    let check = fiddle_runtime::github::check_request_target(REPO, WORKFLOW, &branch);
    (branch, pull, check)
}

/// The configuration every publish scenario below runs under.
fn publish_config(remote: &Remote, local: &Local) -> fiddle_runtime::PublishConfig {
    fiddle_runtime::PublishConfig {
        repo: REPO.to_string(),
        head_owner: HEAD_OWNER.to_string(),
        base: BASE.to_string(),
        head_sha: remote.head(),
        title: "publish the work".to_string(),
        body: "opened by fiddle".to_string(),
        workflow: WORKFLOW.to_string(),
        required_checks: vec![REQUIRED_CHECK.to_string()],
        stub_root: local.root().to_path_buf(),
        project: PROJECT.to_string(),
    }
}

/// One whole attempt over the publish capability, and the bundle it published.
///
/// The bundle rather than the capability's return value: `capability_executions`,
/// `progress`, `observations` and the evidence under them are all published
/// documents, and a test that read them off an in-memory report would be
/// asserting about a value no consumer ever sees.
async fn publish_attempt(
    remote: &Remote,
    local: &Local,
    deployment: &dyn fiddle_runtime::effect::DeploymentPolicy,
) -> serde_json::Value {
    let ctx = remote.context();
    let reference: fiddle_core::InvocationRef = INVOCATION_REF.parse().unwrap();
    let executor = Executor::new(
        fiddle_core::PUBLISH_CHANGE,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        deployment,
        &ctx,
        remote,
        ReadRetry::none(),
    );
    let capability = fiddle_runtime::PublishChange::new(executor, publish_config(remote, local));

    let work_items = fiddle_runtime::StubWorkItemPort::new(local.root());
    let changes = fiddle_runtime::StubChangePort::new(local.root());
    let record = fiddle_runtime::attempt(&fiddle_runtime::AttemptContext {
        project: PROJECT,
        reference: &reference,
        mode: fiddle_core::Mode::Unattended,
        build: fiddle_core::FiddleBuild::new("0.1.0", fiddle_core::UNKNOWN_REVISION),
        report_dir: &local.reports(),
        work_items: &work_items,
        changes: &changes,
        capability: &capability,
        // The executor here reports to `Remote`, which is what these scenarios
        // assert the step *order* against. Sinking the same walk into the journal
        // as well is `attempt`'s and the acceptance lane's subject, not this
        // file's.
        trace: None,
    })
    .await;
    serde_json::to_value(&record.bundle).unwrap()
}

/// A world in which all three effects can succeed, with the required check
/// already green at the head this run is about to publish.
///
/// The check is seeded at the *worktree's* head, which is the commit the push
/// will put on the branch — so a capability that asked CI about any other head
/// would get an answer with the requirement missing, and the assertion below
/// would fail rather than quietly pass.
fn a_publishable_world() -> (Remote, Local) {
    let remote = Remote::empty();
    let local = Local::new("w-1");
    remote.check(REQUIRED_CHECK, "completed", Some("success"), &remote.head());
    (remote, local)
}

/// The evidence one progress entry carries, as strings.
fn evidence_of(bundle: &serde_json::Value) -> Vec<String> {
    bundle["progress"][0]["evidence"]
        .as_array()
        .expect("a run that executed publishes one progress entry")
        .iter()
        .map(|entry| entry.as_str().unwrap().to_string())
        .collect()
}

/// One capability per run, so the bundle shape every consumer has seen is
/// unchanged. ADR 013 priced the alternative and deferred it: three effects are
/// three receipts *inside* one execution, never three executions.
#[tokio::test]
async fn a_publish_run_records_exactly_one_capability_execution() {
    let (remote, local) = a_publishable_world();
    let bundle = publish_attempt(&remote, &local, &Deployment(DeploymentRule::Allow)).await;

    let executions = bundle["capability_executions"].as_array().unwrap();
    assert_eq!(executions.len(), 1, "{}", bundle["capability_executions"]);
    assert_eq!(executions[0]["capability_id"], "publish_change");
    assert_eq!(executions[0]["status"], "completed");
    assert_eq!(
        bundle["progress"].as_array().unwrap().len(),
        1,
        "one execution is one progress entry, which is the shape M0 published"
    );
    // The three effects really happened, so the single execution above is one
    // execution *of three effects* rather than one that did nothing.
    assert_eq!(
        remote.branches(),
        vec![branch_name(PROJECT, INVOCATION_REF)]
    );
    assert_eq!(remote.pull_request_creates(), 1);
    assert_eq!(remote.dispatch_requests(), 1);
}

/// Its progress stage is its own vocabulary, as M0's `mark` and M1's `repair`
/// are. There is no neutral stage name, which is why `Capability::stage` has no
/// default and why a third capability had to say its own word.
#[tokio::test]
async fn progress_is_labelled_in_this_capabilitys_own_vocabulary() {
    let (remote, local) = a_publishable_world();
    let bundle = publish_attempt(&remote, &local, &Deployment(DeploymentRule::Allow)).await;

    assert_eq!(bundle["progress"][0]["stage"], "publish");
    assert_eq!(bundle["progress"][0]["capability_id"], "publish_change");
    assert_eq!(bundle["progress"][0]["status"], "completed");
}

/// The three effects are proposed in order and each one's receipt is evidence a
/// reader can act on: which effect it was, the identity the object was created
/// under, the external reference to open, and the postcondition that was
/// observed to hold.
///
/// The identities are recomputed here from the canonical inputs rather than
/// copied out of the bundle, so this cannot pass on a capability that invented
/// three ids of its own — which is the failure mode that would make a fresh
/// process unable to recognise its own work.
#[tokio::test]
async fn all_three_receipts_reach_the_published_bundle() {
    let (remote, local) = a_publishable_world();
    let bundle = publish_attempt(&remote, &local, &Deployment(DeploymentRule::Allow)).await;

    let (branch, pull, check) = publish_targets();
    let sha = remote.branch_sha(&branch);
    let evidence = evidence_of(&bundle);

    assert_eq!(
        evidence.len(),
        4,
        "the reference the capability earned, then one per effect: {evidence:?}"
    );
    assert_eq!(evidence[0], format!("publish:{REPO}/pull/7"));

    let kinds: Vec<&str> = evidence[1..]
        .iter()
        .map(|entry| entry.split(':').nth(1).unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "ensure_branch_published",
            "ensure_pull_request",
            "ensure_check_requested"
        ],
        "{evidence:?}"
    );

    let identity = |kind, target: &str| {
        effect_id(PROJECT, INVOCATION_REF, kind, target)
            .0
            .to_string()
    };
    assert_eq!(
        evidence[1],
        format!(
            "effect:ensure_branch_published:{}:committed:{sha}:refs/heads/{branch} points at {sha}",
            identity(
                EffectKind::EnsureBranchPublished,
                &fiddle_runtime::github::branch_target(&branch)
            )
        )
    );
    assert!(
        evidence[2].starts_with(&format!(
            "effect:ensure_pull_request:{}:committed:7:pull request #7 from {HEAD_OWNER}:{branch} \
             into {BASE}",
            identity(EffectKind::EnsurePullRequest, &pull)
        )),
        "{}",
        evidence[2]
    );
    assert!(
        evidence[3].starts_with(&format!(
            "effect:ensure_check_requested:{}:committed:4200:workflow run 4200 named",
            identity(EffectKind::EnsureCheckRequested, &check)
        )),
        "{}",
        evidence[3]
    );
    // The same list is on the execution as well as on the progress entry, so a
    // consumer reading either finds the receipts.
    let on_execution: Vec<String> = bundle["capability_executions"][0]["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_str().unwrap().to_string())
        .collect();
    assert_eq!(on_execution, evidence);
}

/// A capability cannot reach the credential; it reaches an executor already
/// bound to its own identity, and the executor holds the client.
///
/// A source-level assertion because that is the level the property is at: the
/// type system cannot express "names no secret", and the alternative — asserting
/// that no token *reached* GitHub — would pass on a capability that held one and
/// happened not to use it this time.
#[test]
fn the_capability_never_receives_a_raw_token() {
    let source = include_str!("../src/capability/publish.rs");
    for named in ["GH_TOKEN", "FIDDLE_GITHUB_TOKEN", "token"] {
        assert!(
            !source.contains(named),
            "the capability names no credential, and it names `{named}`"
        );
    }
    for constructed in ["GhCli", "GitCli", "EffectContext::new"] {
        assert!(
            !source.contains(constructed),
            "the capability constructs no client, and it constructs `{constructed}`"
        );
    }
}

/// Registration: a build that can execute a capability names it in the one list
/// the CLI validates `--capability` against, so `run` and `inspect` both reach
/// it. `every_registered_capability_can_be_selected` in the binary is the other
/// half — it fails to build if a registered id has no selection.
#[test]
fn the_registry_holds_three_capabilities() {
    let ids: Vec<&str> = fiddle_runtime::CAPABILITIES
        .iter()
        .map(|capability| capability.0)
        .collect();
    assert_eq!(ids, ["stub_mark", "fixture_repair", "publish_change"]);
}

/// The two observations Task 8 added are filled by the run that can see them.
/// A type defined and filled by nobody would be the same defect as a
/// configuration key with no consumer.
///
/// Every value asserted here is checked against the world rather than against
/// another field of the same bundle: the pull request number is the one the
/// scripted forge really assigned, and the head is the sha the bare repository
/// really holds. The required check is green only because it was seeded at that
/// exact head — a capability that asked CI about any other commit would find the
/// requirement missing here rather than passing quietly.
#[tokio::test]
async fn a_publish_run_populates_the_review_and_verification_observations() {
    let (remote, local) = a_publishable_world();
    let bundle = publish_attempt(&remote, &local, &Deployment(DeploymentRule::Allow)).await;

    let (branch, _, _) = publish_targets();
    let sha = remote.branch_sha(&branch);

    let review = &bundle["observations"]["review"]["available"];
    assert!(review.is_object(), "{}", bundle["observations"]["review"]);
    assert_eq!(review["value"]["branch"], branch);
    assert_eq!(review["value"]["pull_request"], 7);
    assert_eq!(review["value"]["state"], "open");
    assert_eq!(review["revision"], sha);

    let verification = &bundle["observations"]["verification"]["available"];
    assert!(
        verification.is_object(),
        "{}",
        bundle["observations"]["verification"]
    );
    assert_eq!(verification["value"]["head_sha"], sha);
    assert_eq!(
        verification["value"]["required_missing"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "the required check was seeded at this exact head: {verification}"
    );
    assert_eq!(verification["value"]["failed"].as_array().unwrap().len(), 0);
    assert_eq!(
        verification["value"]["pending"].as_array().unwrap().len(),
        0
    );

    // And the two local observations M0 publishes are still exactly where they
    // were, still describing the state the run left behind.
    assert_eq!(
        bundle["observations"]["work_item"]["available"]["value"]["status"],
        "open"
    );
    assert_eq!(
        bundle["observations"]["changes"]["available"]["value"]["marker"],
        fiddle_core::correlation_key(PROJECT, INVOCATION_REF),
        "a publication accounts for the work, so it records the marker the next \
         invocation completes on"
    );
    assert_eq!(bundle["outcome"], "completed");
    assert_eq!(bundle["next_action"], "complete");
}

/// A refused effect stops the sequence, and the effects after it are not
/// attempted.
///
/// Asserted positively, against what reached the world: the pull request create
/// and the dispatch are counted from the requests the scripted `gh` recorded, so
/// this cannot pass by inferring "nothing ran" from an error type. What was
/// already published stands — a branch is not unpublished by a later refusal —
/// and the receipt for it still reaches the bundle.
#[tokio::test]
async fn a_denied_effect_stops_the_sequence() {
    let (remote, local) = a_publishable_world();
    let bundle = publish_attempt(&remote, &local, &Denying(EffectKind::EnsurePullRequest)).await;

    let branch = branch_name(PROJECT, INVOCATION_REF);
    assert_eq!(
        remote.branches(),
        vec![branch.clone()],
        "the branch it had already published stands"
    );
    assert_eq!(
        remote.pull_request_creates(),
        0,
        "the refused effect itself never reached the forge"
    );
    assert_eq!(
        remote.dispatch_requests(),
        0,
        "and nothing after the refusal ran"
    );
    assert_eq!(remote.landed("dispatches"), 0);

    // One execution, failed, carrying the receipt for the step that did happen.
    let executions = bundle["capability_executions"].as_array().unwrap();
    assert_eq!(executions.len(), 1);
    assert_eq!(executions[0]["status"], "failed");
    let evidence = evidence_of(&bundle);
    assert_eq!(evidence.len(), 1, "{evidence:?}");
    assert!(
        evidence[0].starts_with("effect:ensure_branch_published:"),
        "{}",
        evidence[0]
    );
    assert!(
        bundle["progress"][0]["summary"]
            .as_str()
            .unwrap()
            .contains("policy denied"),
        "the reason must name the refusal: {}",
        bundle["progress"][0]["summary"]
    );

    // What did land is still described, because a reader has to be able to find
    // the branch that is really out there.
    let review = &bundle["observations"]["review"]["available"];
    assert_eq!(review["value"]["branch"], branch);
    assert!(
        review["value"]["pull_request"].is_null(),
        "no pull request was opened, and none may be claimed: {review}"
    );
    assert!(
        review["value"]["state"].is_null(),
        "a state naming no object would be describing nothing: {review}"
    );
    // And the work is *not* accounted for: no marker was written, so the next
    // invocation does not complete on a publication that stopped half way.
    assert!(
        bundle["observations"]["changes"]["available"]["value"]["marker"].is_null(),
        "{}",
        bundle["observations"]["changes"]
    );
}

/// The check effect's identity comes from the executor's own pair, and from
/// nowhere else.
///
/// This is the hazard the whole arrangement exists for: the lookup happens at
/// step 3, before the envelope is minted at step 6, so a capability holding a
/// second copy of `(project, invocation_ref)` could name a run by one identity
/// and look it up by the other — and every attempt would then find nothing and
/// dispatch again, without bound. The name is recomputed here from the canonical
/// inputs, and the second attempt is what proves the lookup finds it.
#[tokio::test]
async fn a_second_attempt_recognises_the_run_the_first_one_dispatched() {
    let (remote, local) = a_publishable_world();
    let (branch, _, check_target) = publish_targets();

    publish_attempt(&remote, &local, &Deployment(DeploymentRule::Allow)).await;
    assert_eq!(remote.dispatch_requests(), 1);

    // The name the dispatched run carries, derived the way a fresh process
    // derives it rather than read back out of the request.
    let expected = fiddle_runtime::github::run_name(&effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsureCheckRequested,
        &check_target,
    ));
    let dispatched = remote
        .requests()
        .into_iter()
        .find_map(|request| {
            let body: serde_json::Value =
                serde_json::from_str(request["body"].as_str().unwrap_or("")).ok()?;
            body["inputs"]["fiddle_effect_id"]
                .as_str()
                .map(|id| format!("fiddle-{id}"))
        })
        .expect("the dispatch carries this effect's identity as an input");
    assert_eq!(dispatched, expected);

    // A second attempt over the same world. The change set now carries this
    // invocation's marker, so the derivation completes without executing — which
    // is itself the property, and the effects are proved unrepeated below.
    // The second attempt runs with **every local artifact removed**, which is
    // what makes this a test of the lookup rather than of the marker. Left in
    // place, the change set would satisfy the derivation and the capability
    // would never run at all — so the counts below would hold on a build whose
    // identity did not survive a fresh process, which is exactly the failure
    // they exist to catch.
    local.forget();
    let bundle = publish_attempt(&remote, &local, &Deployment(DeploymentRule::Allow)).await;
    assert_eq!(
        bundle["capability_executions"][0]["status"], "completed",
        "the capability really executed a second time: {}",
        bundle["capability_executions"]
    );
    assert_eq!(bundle["outcome"], "completed");

    // Nothing was asked for twice. Each of the three effects recomputed its
    // identity from the canonical inputs, found the object an earlier process
    // had created, and short-circuited before the mutation.
    assert_eq!(
        remote.dispatch_requests(),
        1,
        "exactly one run was ever asked for"
    );
    assert_eq!(remote.pull_request_creates(), 1);
    assert_eq!(remote.landed("dispatches"), 1);
    assert_eq!(remote.branches(), vec![branch]);

    // And the second attempt's own check receipt names the run the first one
    // dispatched, so the recognition is the identity's rather than the
    // fixture's.
    let evidence = evidence_of(&bundle);
    assert_eq!(evidence.len(), 4, "{evidence:?}");
    assert!(
        evidence[3].contains(&expected),
        "the check receipt must name {expected}: {}",
        evidence[3]
    );
}

/// An executor bound to another capability refuses the first proposal, so a
/// capability cannot propose an effect in a name that is not its own.
///
/// The binding is the executor's, not the proposal's, which is why this is
/// asserted of a real publish attempt rather than of a hand-built
/// `ProposedEffect`: the capability fills in its own id and has no parameter
/// through which a caller could supply another.
#[tokio::test]
async fn a_capability_cannot_publish_through_another_capabilitys_executor() {
    let (remote, local) = a_publishable_world();
    let ctx = remote.context();
    let deployment = Deployment(DeploymentRule::Allow);
    let executor = Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        &remote,
        ReadRetry::none(),
    );
    let capability = fiddle_runtime::PublishChange::new(executor, publish_config(&remote, &local));

    let reference: fiddle_core::InvocationRef = INVOCATION_REF.parse().unwrap();
    let work_items = fiddle_runtime::StubWorkItemPort::new(local.root());
    let changes = fiddle_runtime::StubChangePort::new(local.root());
    let record = fiddle_runtime::attempt(&fiddle_runtime::AttemptContext {
        project: PROJECT,
        reference: &reference,
        mode: fiddle_core::Mode::Unattended,
        build: fiddle_core::FiddleBuild::new("0.1.0", fiddle_core::UNKNOWN_REVISION),
        report_dir: &local.reports(),
        work_items: &work_items,
        changes: &changes,
        capability: &capability,
        // The executor here reports to `Remote`, which is what these scenarios
        // assert the step *order* against. Sinking the same walk into the journal
        // as well is `attempt`'s and the acceptance lane's subject, not this
        // file's.
        trace: None,
    })
    .await;

    let bundle = serde_json::to_value(&record.bundle).unwrap();
    assert_eq!(bundle["capability_executions"][0]["status"], "failed");
    assert!(
        bundle["progress"][0]["summary"]
            .as_str()
            .unwrap()
            .contains("cannot propose for"),
        "{}",
        bundle["progress"][0]["summary"]
    );
    assert!(
        remote.branches().is_empty(),
        "a proposal made under another capability's name reaches nothing"
    );
    assert_eq!(remote.pull_request_creates(), 0);
    assert_eq!(remote.dispatch_requests(), 0);
}
