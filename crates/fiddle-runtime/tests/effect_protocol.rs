mod support;

use fiddle_core::{
    decision_request_id, effect_id, payload_hash, DecisionBinding, DeploymentRule, EffectId,
    EffectName, HumanDecisionRequirement, InterpretedHumanDecision, PayloadHash, ProposedEffect,
    Published, ENSURE_BRANCH_PUBLISHED, ENSURE_CHECK_REQUESTED, ENSURE_PULL_REQUEST,
    ENSURE_PULL_REQUEST_BODY, ENSURE_PULL_REQUEST_READY, FIXTURE_REPAIR, PUBLISH_DECISION_REQUEST,
    STUB_MARK,
};
use fiddle_runtime::effect::{
    AdapterError, EffectContext, EffectError, EffectOutcome, EffectPhase, EffectReceipt,
    EffectTrace, ExecutionStep, Executor, IntegrationOperation, ObservedState, ReadRetry,
    Recurrence, ResolvedDecision,
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

#[test]
fn the_authorization_envelope_has_no_public_constructor() {
    let source = include_str!("../src/effect/mod.rs");
    assert!(
        !source.contains("pub fn authorize") && !source.contains("pub const fn authorize"),
        "AuthorizedEffect must not be constructible outside the executor"
    );
    assert!(source.contains("pub struct AuthorizedEffect<T> {\n    effect_id:"));
}

fn rustc_program() -> PathBuf {
    let cargo = PathBuf::from(env!("CARGO"));
    let sibling = cargo.with_file_name("rustc");
    if sibling.is_file() {
        sibling
    } else {
        PathBuf::from("rustc")
    }
}

fn compile_probe_against_this_crate(source: &str) -> std::process::Output {
    let dir = TempDir::new().unwrap();
    let probe = dir.path().join("probe.rs");
    std::fs::write(&probe, source).unwrap();
    let deps = std::env::current_exe()
        .unwrap()
        .parent()
        .expect("the test binary lives in cargo's deps directory")
        .to_path_buf();
    let mut rlibs: Vec<PathBuf> = std::fs::read_dir(&deps)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with("libfiddle_runtime-") && name.ends_with(".rlib")
                })
        })
        .collect();
    rlibs.sort_by_key(|path| {
        std::fs::metadata(path)
            .and_then(|meta| meta.modified())
            .unwrap()
    });
    let rlib = rlibs
        .pop()
        .expect("cargo builds fiddle_runtime's rlib beside this test binary");
    std::process::Command::new(rustc_program())
        .args([
            "--edition",
            "2021",
            "--crate-type",
            "lib",
            "--emit",
            "metadata",
        ])
        .arg("-o")
        .arg(dir.path().join("probe.meta"))
        .arg("-L")
        .arg(format!("dependency={}", deps.display()))
        .arg("--extern")
        .arg(format!("fiddle_runtime={}", rlib.display()))
        .arg(&probe)
        .output()
        .expect("the toolchain that built this test can compile a probe")
}

#[test]
fn the_authorization_envelope_type_is_nameable_from_another_crate() {
    let probe = compile_probe_against_this_crate(
        "use fiddle_runtime::effect::AuthorizedEffect;\n\
         pub fn takes_an_envelope<T>(_: &AuthorizedEffect<T>) {}\n",
    );
    assert!(
        probe.status.success(),
        "the envelope's public path must resolve outside its crate: {}",
        String::from_utf8_lossy(&probe.stderr)
    );
}

#[test]
fn a_struct_literal_cannot_forge_an_authorization_envelope_from_another_crate() {
    let probe = compile_probe_against_this_crate(
        "use fiddle_runtime::core::{EffectId, PayloadHash};\n\
         use fiddle_runtime::effect::AuthorizedEffect;\n\
         pub fn forge() -> AuthorizedEffect<()> {\n\
             AuthorizedEffect {\n\
                 effect_id: EffectId(\"0000000000000000\".to_string()),\n\
                 payload_hash: PayloadHash(\"0000000000000000\".to_string()),\n\
                 operation: (),\n\
             }\n\
         }\n",
    );
    let stderr = String::from_utf8_lossy(&probe.stderr);
    assert!(
        !probe.status.success(),
        "a struct literal must not build an envelope outside the executor"
    );
    assert!(
        stderr.contains("E0451"),
        "the refusal must be the private-field one, not some other error: {stderr}"
    );
}

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

#[tokio::test]
async fn an_already_satisfied_effect_is_never_put_to_policy() {
    let harness = Harness::new(Script::AlreadySatisfied)
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
    assert_eq!(harness.world.calls(), ["inspect", "apply", "inspect"]);
}

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
            error.adapter_source::<GhError>(),
            Some(GhError::Http { status: 403, .. })
        ),
        "expected the refusal to stand, got {error:?}"
    );
    assert_eq!(harness.world.mutations(), 0);
}

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

const BUDGET: (u32, Duration, Duration) = (5, Duration::from_millis(10), Duration::from_secs(30));

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
        1 + attempts,
        "the read is bounded by the budget it was given"
    );
    assert_eq!(harness.waits().len(), attempts - 1);
    assert!(
        error.to_string().contains("over 5 reads"),
        "the diagnostic must say that waiting was tried, got {error}"
    );
}

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

#[tokio::test]
async fn every_path_dispatches_at_most_one_mutation() {
    for script in Script::ALL {
        let harness = Harness::new(script).with_read_retry(BUDGET.0, BUDGET.1, BUDGET.2);
        let _ = harness
            .executor()
            .execute(branch_effect(), harness.operation())
            .await;

        let expected = match script {
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
    assert!(!GhError::Malformed(String::new()).is_worth_reading_again());
    assert!(
        !GhError::Duplicate { count: 2 }.is_worth_reading_again(),
        "a second object does not become one object by being looked at again"
    );
}

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

const DECIDED_HEAD: &str = "1f0e5d4c3b2a19876543210fedcba98765432100";

fn proposed_effect_id() -> EffectId {
    effect_id(PROJECT, INVOCATION_REF, ENSURE_BRANCH_PUBLISHED, TARGET)
}

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

fn gated_on_a_person() -> Harness {
    Harness::new(Script::AbsentThenWritten)
        .with_policy(HumanDecisionRequirement::Human, DeploymentRule::Allow)
}

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

#[tokio::test]
async fn a_decision_naming_another_effect_is_refused() {
    let harness = gated_on_a_person();
    let elsewhere = effect_id(
        PROJECT,
        INVOCATION_REF,
        ENSURE_BRANCH_PUBLISHED,
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
    let rendered = format!("{error}");
    assert!(
        rendered.contains(&elsewhere.0) && rendered.contains(&proposed_effect_id().0),
        "the refusal must name both identities: {rendered}"
    );
}

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

#[tokio::test]
async fn a_decision_changes_nothing_for_an_automatic_operation() {
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

#[tokio::test]
async fn a_decided_mutation_whose_answer_was_lost_is_settled_by_reading() {
    let harness = Harness::new(Script::WriteLandsAnswerLost)
        .with_policy(HumanDecisionRequirement::Human, DeploymentRule::Allow);
    let decision = approval(proposed_effect_id(), payload_hash(PAYLOAD));

    let receipt = harness
        .executor()
        .execute_decided(branch_effect(), harness.operation(), &decision)
        .await
        .expect("the answer was lost, not the write");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        harness.world.mutation_requests(),
        1,
        "one approval buys one dispatch, and a lost answer does not buy another"
    );
    assert_eq!(harness.world.mutations(), 1, "and it landed exactly once");
    assert!(
        harness.world.read_after_unknown(),
        "the executor went and looked rather than writing again"
    );
    assert_eq!(
        harness.world.calls(),
        ["inspect", "apply", "inspect"],
        "a read settled it; no second dispatch appears anywhere in the walk"
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
        "and the decision was resolved exactly once, before the single dispatch"
    );
}

#[tokio::test]
async fn every_decided_path_dispatches_at_most_one_mutation() {
    let decision = approval(proposed_effect_id(), payload_hash(PAYLOAD));
    for script in Script::ALL {
        let harness = Harness::new(script)
            .with_read_retry(BUDGET.0, BUDGET.1, BUDGET.2)
            .with_policy(HumanDecisionRequirement::Human, DeploymentRule::Allow);
        let _ = harness
            .executor()
            .execute_decided(branch_effect(), harness.operation(), &decision)
            .await;

        let expected = match script {
            Script::AlreadySatisfied | Script::TwoMatch => 0,
            _ => 1,
        };
        assert_eq!(
            harness.world.mutation_requests(),
            expected,
            "{script:?} dispatched the wrong number of mutations on the decided path"
        );
        assert!(
            harness.world.mutations() <= 1,
            "{script:?} changed the world {} times",
            harness.world.mutations()
        );
        if expected == 0 {
            assert!(
                !harness.world.steps().contains(&"resolve_decision"),
                "{script:?} settles before policy, so no decision may be resolved: {:?}",
                harness.world.steps()
            );
        }
    }
}

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

fn proposing_kind(kind: &'static str) -> ProposedEffect {
    ProposedEffect {
        kind: EffectName::shipped(kind),
        ..branch_effect()
    }
}

fn proposing_target(target: &str) -> ProposedEffect {
    ProposedEffect {
        target: target.to_string(),
        ..branch_effect()
    }
}

#[tokio::test]
async fn a_proposed_kind_the_operation_would_not_perform_is_refused() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let error = harness
        .executor()
        .execute(proposing_kind(ENSURE_PULL_REQUEST), harness.operation())
        .await
        .unwrap_err();

    assert_eq!(
        harness.world.calls(),
        Vec::<&str>::new(),
        "nothing reached the adapter"
    );
    assert_eq!(
        harness.world.steps(),
        ["validate_capability"],
        "and no identity was derived from a name the operation would not perform"
    );
    assert_eq!(harness.world.mutations(), 0, "and nothing was written");
    assert!(
        matches!(
            &error,
            EffectError::IdentityDiverged { part: "kind", proposed, performing, .. }
                if proposed == ENSURE_PULL_REQUEST && performing == ENSURE_BRANCH_PUBLISHED
        ),
        "expected IdentityDiverged naming both spellings, got {error:?}"
    );
    assert_eq!(error.recurrence(), Recurrence::Permanent);
}

#[tokio::test]
async fn a_proposed_target_the_operation_would_not_touch_is_refused() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let error = harness
        .executor()
        .execute(
            proposing_target("refs/heads/fiddle/elsewhere"),
            harness.operation(),
        )
        .await
        .unwrap_err();

    assert_eq!(
        harness.world.calls(),
        Vec::<&str>::new(),
        "nothing reached the adapter"
    );
    assert_eq!(
        harness.world.steps(),
        ["validate_capability"],
        "and no identity was derived for a target the operation would not touch"
    );
    assert_eq!(harness.world.mutations(), 0, "and nothing was written");
    assert!(
        matches!(
            &error,
            EffectError::IdentityDiverged { part: "target", proposed, performing, .. }
                if proposed == "refs/heads/fiddle/elsewhere" && performing == TARGET
        ),
        "expected IdentityDiverged naming both targets, got {error:?}"
    );
    assert_eq!(error.recurrence(), Recurrence::Permanent);
}

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
        effect_id(PROJECT, INVOCATION_REF, ENSURE_BRANCH_PUBLISHED, TARGET)
    );
    assert_eq!(receipt.payload_hash, payload_hash(PAYLOAD));
    assert_eq!(receipt.target, TARGET);
    assert_eq!(receipt.value, "deadbeef");
}

const REPO: &str = "o/r";

const PATIENT: Duration = Duration::from_secs(60);

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

fn git_says(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

struct Remote {
    dir: TempDir,
    remote: PathBuf,
    work: PathBuf,
    steps: Mutex<Vec<&'static str>>,
}

impl EffectTrace for Remote {
    fn step(&self, _kind: &EffectName, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl Remote {
    fn empty() -> Self {
        let dir = TempDir::new().unwrap();
        let remote = dir.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git_setup(&remote, &["init", "-q", "--bare", "."]);
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

    fn seed(&self, worktree: &Path, branch: &str) {
        git_setup(
            worktree,
            &["push", "-q", "origin", &format!("HEAD:refs/heads/{branch}")],
        );
    }

    fn head(&self) -> String {
        git_says(&self.work, &["rev-parse", "HEAD"])
    }

    fn branches(&self) -> Vec<String> {
        git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    fn branch_sha(&self, branch: &str) -> String {
        git_says(
            &self.remote,
            &["rev-parse", &format!("refs/heads/{branch}")],
        )
    }

    fn pushes(&self) -> usize {
        std::fs::read_to_string(self.work.join("pushes"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    fn context(&self) -> EffectContext {
        self.context_with(
            PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
            PathBuf::from("git"),
            PATIENT,
        )
    }

    fn context_reading_with(&self, gh: PathBuf) -> EffectContext {
        self.context_with(gh, PathBuf::from("git"), PATIENT)
    }

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

    fn applies(&self) -> usize {
        self.steps().iter().filter(|step| **step == "apply").count()
    }
}

fn published_branch() -> String {
    branch_name(PROJECT, INVOCATION_REF)
}

fn branch_operation(intended: &str) -> EnsureBranchPublished {
    EnsureBranchPublished::new(REPO.to_string(), published_branch(), intended.to_string())
}

async fn publish_the_branch<O>(
    remote: &Remote,
    ctx: &EffectContext,
    intended: &str,
    operation: O,
) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, EffectError>
where
    O: IntegrationOperation<Error = GhError>,
{
    let deployment = Deployment(DeploymentRule::Allow);
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectName::shipped(ENSURE_BRANCH_PUBLISHED),
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
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
}

#[test]
fn the_branch_name_is_derived_and_stable() {
    let first = branch_name("acme/widget", "beans:w-1");
    assert_eq!(first, branch_name("acme/widget", "beans:w-1"));
    assert!(
        first.starts_with("fiddle/"),
        "namespaced, so a human can see whose it is: {first}"
    );
    assert_ne!(first, branch_name("acme/widget", "beans:w-2"));
    assert_ne!(first, branch_name("acme/other", "beans:w-1"));

    assert_eq!(
        first,
        format!(
            "fiddle/{}",
            effect_id(
                "acme/widget",
                "beans:w-1",
                ENSURE_BRANCH_PUBLISHED,
                "acme/widget"
            )
            .0
        )
    );

    assert!(!first.contains(".."));
    assert!(!first.ends_with(".lock"));
    assert!(!first.split('/').any(|part| part.ends_with(".lock")));
    assert!(!first.starts_with(['-', '.', '/']));
    assert!(!first.ends_with(['.', '/']));
    assert!(first
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/-_".contains(c)));

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
            error.adapter_source::<GhError>(),
            Some(GhError::Push(GitError::NonFastForward { .. }))
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

const IMPATIENT: Duration = Duration::from_secs(3);

#[tokio::test]
async fn a_push_that_landed_before_its_answer_was_lost_is_resolved_by_reading() {
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
        lost.outcome(EffectPhase::Apply),
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

const HEAD_OWNER: &str = "o";

const BASE: &str = "main";

const WORKFLOW: &str = "fiddle-check.yml";

const REQUIRED_CHECK: &str = "build";

struct Denying(EffectName);

impl fiddle_runtime::effect::DeploymentPolicy for Denying {
    fn rule_for(&self, kind: &EffectName) -> DeploymentRule {
        match kind == &self.0 {
            true => DeploymentRule::Deny,
            false => DeploymentRule::Allow,
        }
    }
}

struct Local {
    dir: TempDir,
}

impl Local {
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

    fn forget(&self) {
        let _ = std::fs::remove_dir_all(self.dir.path().join("changes"));
        let _ = std::fs::remove_dir_all(self.reports());
        std::fs::create_dir_all(self.dir.path().join("changes")).unwrap();
    }
}

impl Remote {
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

fn publish_targets() -> (String, String, String) {
    let branch = branch_name(PROJECT, INVOCATION_REF);
    let pull = fiddle_runtime::github::pull_request_target(REPO, HEAD_OWNER, &branch, BASE);
    let check = fiddle_runtime::github::check_request_target(REPO, WORKFLOW, &branch);
    (branch, pull, check)
}

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
        trace: None,
        cancel: &tokio_util::sync::CancellationToken::new(),
    })
    .await;
    serde_json::to_value(&record.bundle).unwrap()
}

fn a_publishable_world() -> (Remote, Local) {
    let remote = Remote::empty();
    let local = Local::new("w-1");
    remote.check(REQUIRED_CHECK, "completed", Some("success"), &remote.head());
    (remote, local)
}

fn evidence_of(bundle: &serde_json::Value) -> Vec<String> {
    bundle["progress"][0]["evidence"]
        .as_array()
        .expect("a run that executed publishes one progress entry")
        .iter()
        .map(|entry| entry.as_str().unwrap().to_string())
        .collect()
}

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
    assert_eq!(
        remote.branches(),
        vec![branch_name(PROJECT, INVOCATION_REF)]
    );
    assert_eq!(remote.pull_request_creates(), 1);
    assert_eq!(remote.dispatch_requests(), 1);
}

#[tokio::test]
async fn progress_is_labelled_in_this_capabilitys_own_vocabulary() {
    let (remote, local) = a_publishable_world();
    let bundle = publish_attempt(&remote, &local, &Deployment(DeploymentRule::Allow)).await;

    assert_eq!(bundle["progress"][0]["stage"], "publish");
    assert_eq!(bundle["progress"][0]["capability_id"], "publish_change");
    assert_eq!(bundle["progress"][0]["status"], "completed");
}

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
                ENSURE_BRANCH_PUBLISHED,
                &fiddle_runtime::github::branch_target(&branch)
            )
        )
    );
    assert!(
        evidence[2].starts_with(&format!(
            "effect:ensure_pull_request:{}:committed:7:pull request #7 from {HEAD_OWNER}:{branch} \
             into {BASE}",
            identity(ENSURE_PULL_REQUEST, &pull)
        )),
        "{}",
        evidence[2]
    );
    assert!(
        evidence[3].starts_with(&format!(
            "effect:ensure_check_requested:{}:committed:4200:workflow run 4200 named",
            identity(ENSURE_CHECK_REQUESTED, &check)
        )),
        "{}",
        evidence[3]
    );
    let on_execution: Vec<String> = bundle["capability_executions"][0]["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_str().unwrap().to_string())
        .collect();
    assert_eq!(on_execution, evidence);
}

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

#[test]
fn the_registry_holds_every_capability_this_build_offers() {
    let ids: Vec<&str> = fiddle_runtime::CAPABILITIES
        .iter()
        .map(|capability| capability.0)
        .collect();
    assert_eq!(
        ids,
        [
            "stub_mark",
            "fixture_repair",
            "publish_change",
            "propose_change",
            "cve_mitigate"
        ]
    );
}

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

#[tokio::test]
async fn a_denied_effect_stops_the_sequence() {
    let (remote, local) = a_publishable_world();
    let bundle = publish_attempt(
        &remote,
        &local,
        &Denying(EffectName::shipped(ENSURE_PULL_REQUEST)),
    )
    .await;

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
    assert!(
        bundle["observations"]["changes"]["available"]["value"]["marker"].is_null(),
        "{}",
        bundle["observations"]["changes"]
    );
}

#[tokio::test]
async fn a_second_attempt_recognises_the_run_the_first_one_dispatched() {
    let (remote, local) = a_publishable_world();
    let (branch, _, check_target) = publish_targets();

    publish_attempt(&remote, &local, &Deployment(DeploymentRule::Allow)).await;
    assert_eq!(remote.dispatch_requests(), 1);

    let expected = fiddle_runtime::github::run_name(&effect_id(
        PROJECT,
        INVOCATION_REF,
        ENSURE_CHECK_REQUESTED,
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

    local.forget();
    let bundle = publish_attempt(&remote, &local, &Deployment(DeploymentRule::Allow)).await;
    assert_eq!(
        bundle["capability_executions"][0]["status"], "completed",
        "the capability really executed a second time: {}",
        bundle["capability_executions"]
    );
    assert_eq!(bundle["outcome"], "completed");

    assert_eq!(
        remote.dispatch_requests(),
        1,
        "exactly one run was ever asked for"
    );
    assert_eq!(remote.pull_request_creates(), 1);
    assert_eq!(remote.landed("dispatches"), 1);
    assert_eq!(remote.branches(), vec![branch]);

    let evidence = evidence_of(&bundle);
    assert_eq!(evidence.len(), 4, "{evidence:?}");
    assert!(
        evidence[3].contains(&expected),
        "the check receipt must name {expected}: {}",
        evidence[3]
    );
}

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
        trace: None,
        cancel: &tokio_util::sync::CancellationToken::new(),
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

#[tokio::test]
async fn an_unregistered_proposal_is_refused_before_an_identity_is_derived() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let error = harness
        .executor()
        .execute(unregistered_effect(FIXTURE_REPAIR), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::UnknownEffect { .. }),
        "expected UnknownEffect, got {error:?}"
    );
    assert_eq!(error.recurrence(), Recurrence::Permanent);
    assert!(
        format!("{error}").contains("jira.transition"),
        "the refusal must name the effect it refused: {error}"
    );
    assert_eq!(
        harness.world.steps(),
        Vec::<&str>::new(),
        "no execution step ran"
    );
    assert_eq!(
        harness.world.calls(),
        Vec::<&str>::new(),
        "nothing reached the adapter"
    );
}

#[tokio::test]
async fn an_unregistered_name_is_refused_ahead_of_the_capability_it_names() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let error = harness
        .executor()
        .execute(unregistered_effect(STUB_MARK), harness.operation())
        .await
        .unwrap_err();

    assert!(
        matches!(error, EffectError::UnknownEffect { .. }),
        "the registry decides before validate_capability, so this is not PolicyDenied: {error:?}"
    );
    assert_eq!(
        harness.world.steps(),
        Vec::<&str>::new(),
        "not even validate_capability ran"
    );
}

#[tokio::test]
async fn every_name_this_build_ships_survives_the_registry_check() {
    for shipped in [
        ENSURE_BRANCH_PUBLISHED,
        ENSURE_PULL_REQUEST,
        ENSURE_CHECK_REQUESTED,
        PUBLISH_DECISION_REQUEST,
        ENSURE_PULL_REQUEST_READY,
        ENSURE_PULL_REQUEST_BODY,
    ] {
        let harness = Harness::new(Script::AlreadySatisfied);
        let proposed = ProposedEffect {
            kind: EffectName::shipped(shipped),
            ..branch_effect()
        };
        harness
            .executor()
            .execute(proposed, harness.operation_performing(shipped))
            .await
            .unwrap_or_else(|error| panic!("{shipped} is registered and was refused: {error}"));
    }
}

fn unregistered_effect(capability: fiddle_core::CapabilityId) -> ProposedEffect {
    ProposedEffect {
        capability,
        kind: EffectName::parse("jira.transition").unwrap(),
        target: TARGET.to_string(),
        payload: PAYLOAD.to_string(),
    }
}
