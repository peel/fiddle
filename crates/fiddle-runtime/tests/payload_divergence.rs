mod support;

use fiddle_core::{effect_id, payload_hash, EffectKind, ProposedEffect, FIXTURE_REPAIR};
use fiddle_runtime::effect::{EffectError, EffectOutcome};
use support::{Harness, Script, INVOCATION_REF, PAYLOAD, PROJECT, TARGET};

fn proposed_with(payload: &str) -> ProposedEffect {
    ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectKind::EnsureBranchPublished,
        target: TARGET.to_string(),
        payload: payload.to_string(),
    }
}

const UP_TO_AUTHORIZE: [&str; 5] = [
    "validate_capability",
    "derive_identity",
    "inspect_postcondition",
    "combine_policy",
    "authorize",
];

#[tokio::test]
async fn a_payload_the_envelope_was_not_minted_for_never_reaches_the_world() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let widened = r#"{"force":true,"sha":"deadbeef"}"#;

    let error = harness
        .executor()
        .execute(proposed_with(widened), harness.operation())
        .await
        .expect_err("an operation about another payload must not be applied");

    match error {
        EffectError::PayloadDiverged {
            kind,
            approved,
            applying,
        } => {
            assert_eq!(kind, EffectKind::EnsureBranchPublished);
            assert_eq!(
                approved,
                payload_hash(widened),
                "the digest the envelope was minted for is the proposal's"
            );
            assert_eq!(
                applying,
                payload_hash(PAYLOAD),
                "and the one it is refused against is the operation's"
            );
            assert_ne!(approved, applying, "or there was nothing to refuse");
        }
        other => panic!("a diverged payload must be its own refusal, not {other:?}"),
    }

    assert_eq!(
        harness.world.mutation_requests(),
        0,
        "nothing may be dispatched under an approval minted for another request"
    );
    assert_eq!(
        harness.world.mutations(),
        0,
        "and nothing may have changed out there: {:?}",
        harness.world.calls()
    );
    assert_eq!(
        harness.world.steps(),
        UP_TO_AUTHORIZE,
        "the refusal belongs after policy and before the mutation"
    );
}

#[test]
fn the_two_requests_are_one_identity() {
    let widened = r#"{"force":true,"sha":"deadbeef"}"#;
    let of = |payload: &str| {
        let proposed = proposed_with(payload);
        effect_id(PROJECT, INVOCATION_REF, proposed.kind, &proposed.target)
    };

    assert_eq!(
        of(PAYLOAD),
        of(widened),
        "a changed payload is the same effect, and that is the hazard"
    );
    assert_ne!(
        payload_hash(PAYLOAD),
        payload_hash(widened),
        "so the payload digest is the only thing left that can tell them apart"
    );
}

#[tokio::test]
async fn the_payload_the_envelope_was_minted_for_reaches_the_mutation() {
    let harness = Harness::new(Script::AbsentThenWritten);

    let receipt = harness
        .executor()
        .execute(proposed_with(PAYLOAD), harness.operation())
        .await
        .expect("a proposal and an operation about one request are one request");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.payload_hash,
        payload_hash(PAYLOAD),
        "the receipt records the payload that was actually applied"
    );
    assert_eq!(
        harness.world.mutations(),
        1,
        "and the write really happened: {:?}",
        harness.world.calls()
    );
    assert!(
        harness.world.steps().contains(&"apply"),
        "the walk reaches the mutation: {:?}",
        harness.world.steps()
    );
}

#[tokio::test]
async fn an_equivalent_spelling_is_not_the_same_payload() {
    let harness = Harness::new(Script::AbsentThenWritten);
    let respelled = r#"{"sha": "deadbeef"}"#;
    assert_ne!(respelled, PAYLOAD, "or this case proves nothing");

    let error = harness
        .executor()
        .execute(proposed_with(respelled), harness.operation())
        .await
        .expect_err("the digest is over bytes, and these are different bytes");

    assert!(
        matches!(error, EffectError::PayloadDiverged { .. }),
        "a respelled payload is refused as a diverged one: {error:?}"
    );
    assert_eq!(
        harness.world.mutations(),
        0,
        "and nothing was written while it was being decided"
    );
}
