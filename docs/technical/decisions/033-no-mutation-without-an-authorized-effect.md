# 033 — No mutation runs without an authorized effect

Status: accepted; amended in M3 by the note below
Cites: fiddle_runtime::effect::Executor::execute, effect::AuthorizedEffect, effect::ExecutionStep, IntegrationOperation::apply, IntegrationOperation::minimum, fiddle_core::policy::combine, fiddle_core::effect_id, fiddle_core::payload_hash, AttemptJournal::record_step, github/ready.rs::minimum, effect_protocol::a_struct_literal_cannot_forge_an_authorization_envelope_from_another_crate

## Context

M2 gave fiddle its first capability that changes something outside this process. A capability could construct its own client, skip identity derivation, or ignore its own minimum. Any of those makes the authorization order a convention rather than a property.

## Decision

Route every external mutation through `Executor::execute`, which walks `ExecutionStep`'s eight steps in order. Give `AuthorizedEffect`, the token an adapter must hold, private fields and no constructor outside `effect/mod.rs`. Let deployment policy strengthen a capability's minimum, and never weaken it.

## Consequences

- The compiler checks the property, not a paragraph. `IntegrationOperation::apply` takes an `AuthorizedEffect` by reference, and `a_struct_literal_cannot_forge_an_authorization_envelope_from_another_crate` drives `rustc` to prove a forgery does not build.
- A capability holds no credential and builds no client. `capability::publish` receives an `Executor` already bound to its own `CapabilityId`, and step 1 refuses a proposal made under any other.
- `combine` is total over the product, not sampled. `Deny` in the document is absolute, a capability's `Human` minimum survives a permissive document, and `Allow` removes no existing gate.
- The order is observable. Each `ExecutionStep` is announced before the work behind it. `record_step` appends two closed enums and nothing else, so no credential reaches the journal.
- What was given up: an operation that declares `Human` cannot run unattended. `github/ready.rs` declares it, so the ready transition always waits for an answer.

## The eight steps

`ValidateCapability`, `DeriveIdentity`, `InspectPostcondition`, `CombinePolicy`, `ResolveDecision`, `Authorize`, `Apply`, `ObservePostcondition`.

`ResolveDecision` runs only on the `RequireHumanDecision` arm, and only when a decision was supplied. It checks that the decision's binding names this `effect_id`, so an approval for one effect cannot authorise another.

## Identity is recomputed, never remembered

`effect_id` is blake3 over a length-prefixed encoding of `(project, invocation_ref, kind, target)`. A fresh process derives the same id from canonical inputs, and no field's contents can be mistaken for structure. `payload_hash` is the same digest over the payload alone.

## Amendment (M3) — the interesting cell is reachable from a capability

This decision's last consequence read: "`RequireHumanDecision` fails closed. M2's three operations all declare `Automatic`, so the rule is reachable only from the document, and it returns `EffectError::HumanDecisionRequired` because the channel that would answer it is M3's."

Three parts of that no longer hold. `EffectKind` carries six variants, not three. `crates/fiddle-runtime/src/github/ready.rs::minimum` declares `HumanDecisionRequirement::Human`, so the rule is reachable from a capability and not only from the document. And the channel exists: `Executor::execute` resolves a supplied decision through `ExecutionStep::ResolveDecision`, and returns `EffectError::HumanDecisionRequired` only when none was supplied.

The bullet above contradicted this decision's own `combine` bullet, which calls capability `Human` against deployment `Allow` "the interesting cell". That cell is now the ready transition's ordinary path.

ADR 016's table is the other half of this correction. `EffectError::HumanDecisionRequired` now classifies as `Recurrence::Awaiting`, which `orchestration::run` maps to `RunOutcome::Suspended` and exit 10, rather than to `Failed` and exit 20.
