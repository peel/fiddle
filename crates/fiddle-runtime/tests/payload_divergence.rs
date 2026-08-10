//! The second half of the identity/payload split, under test.
//!
//! `fiddle-core`'s `effect` module states why an effect's identity and its
//! payload are hashed separately: an effect keeps its identity while its payload
//! changes, so "this is the same effect, already performed" can be told apart
//! from "this is the same effect, but the request is not the one that was
//! approved" — and a single hash over both would collapse the two, leaving the
//! second case to arrive looking like new work.
//!
//! Until this suite existed, nothing implemented that second half. The payload
//! hash was derived on every effect, carried into the envelope and into both
//! receipts, and read by no production code at all: the accessor on
//! `AuthorizedEffect` had zero callers and step 3 returned `Committed` on any
//! existing postcondition without consulting one.
//!
//! What the executor now checks is the one comparison this milestone can actually
//! make. The envelope is minted at step 6 for the payload the *proposal* carried;
//! the operation about to be applied at step 7 has a canonical payload of its
//! own; and the two must name the same request or nothing is applied. Approval is
//! minted and spent one step apart, and if the payload can change in between then
//! what policy allowed and what the adapter performs are two different things.
//!
//! **The identity does not move between these two cases, and that is the hazard.**
//! `every_canonical_input_changes_the_identity` in `fiddle-core` pins that the
//! identity is derived from the target and never from the payload — deliberately,
//! so that rewording a pull request does not open a second one — which is exactly
//! why a proposal and an operation that disagree about the request still agree
//! about the identity. Without the payload digest there is nothing left to notice
//! with.
//!
//! A separate binary rather than more cases in `effect_protocol.rs`, whose lane
//! is frozen at forty by this milestone's handoff. The world is the same scripted
//! one, reached through `support`, so this suite reaches no process, no credential
//! and no network either.

mod support;

use fiddle_core::{effect_id, payload_hash, EffectKind, ProposedEffect, FIXTURE_REPAIR};
use fiddle_runtime::effect::{EffectError, EffectOutcome};
use support::{Harness, Script, INVOCATION_REF, PAYLOAD, PROJECT, TARGET};

/// The effect this suite proposes, against the scripted operation's own target
/// but carrying `payload`.
///
/// The target is `support::TARGET` in every case, so every proposal here derives
/// the identity the scripted operation's envelope is checked against — the whole
/// point being that only the payload differs.
fn proposed_with(payload: &str) -> ProposedEffect {
    ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectKind::EnsureBranchPublished,
        target: TARGET.to_string(),
        payload: payload.to_string(),
    }
}

/// The order the executor is required to have walked when it refuses at step 6.
///
/// Spelled out rather than asserted piecewise, because the *position* of the
/// refusal is half of what makes it correct: after the capability check, after
/// the identity, after the postcondition inspection and after policy — and before
/// `apply`, which is the only line in the process that changes anything outside
/// it.
const UP_TO_AUTHORIZE: [&str; 5] = [
    "validate_capability",
    "derive_identity",
    "inspect_postcondition",
    "combine_policy",
    "authorize",
];

/// A payload the envelope was not minted for never reaches the world.
///
/// `Script::AbsentThenWritten` is chosen deliberately: this is the world that
/// *would* have accepted the write and recorded it, so a zero in
/// `mutation_requests` is the check refusing rather than the world declining.
/// Remove the comparison in `Executor::execute` and this test fails four times
/// over — the call returns a receipt, the mutation lands, the request is counted
/// and `apply` appears in the walk.
#[tokio::test]
async fn a_payload_the_envelope_was_not_minted_for_never_reaches_the_world() {
    let harness = Harness::new(Script::AbsentThenWritten);
    // The same commit the operation is about, plus a field it never agreed to.
    // A *widened* request rather than a different one, because widening is the
    // case the split was written for: it is the one that would otherwise look
    // like the work that was already approved.
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

/// The identity is the same for both requests, which is why the digest is needed.
///
/// This is the demonstration rather than the assertion — in the manner of
/// `an_embedded_nul_cannot_forge_a_shared_identity`, which proves its fixture
/// really does collide before asserting that the identity does not. If the
/// identity ever started moving with the payload, the case above would still
/// pass while having stopped testing the thing it is named for.
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

/// The inverse of the rule, so the guard is not merely "always refuse".
///
/// A proposal and an operation that name the same request walk the whole order,
/// dispatch exactly once, and produce a receipt carrying that request's digest.
/// Without this case the guard above would pass just as well on an executor that
/// refused every effect it was ever handed.
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

/// The comparison is over bytes, so an equivalent spelling is still a divergence.
///
/// `payload_hash` hashes what it is handed and says so: making the payload
/// canonical is the proposer's job, and this suite pins that the executor does
/// not quietly take on that job by parsing. A JSON-aware comparison here would be
/// a second definition of "the same request" living beside the digest's, and the
/// two would drift — the failure being silent in the worst direction, since a
/// widened request that happened to normalize the same would be waved through.
#[tokio::test]
async fn an_equivalent_spelling_is_not_the_same_payload() {
    let harness = Harness::new(Script::AbsentThenWritten);
    // The same object with one space in it. `PAYLOAD` is `{"sha":"deadbeef"}`.
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
