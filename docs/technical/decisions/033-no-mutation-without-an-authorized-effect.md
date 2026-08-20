# 033 — No mutation runs without an authorized effect

Status: accepted

Cites: fiddle_runtime::effect::Executor::execute, effect::AuthorizedEffect,
IntegrationOperation::apply, fiddle_core::policy::combine, fiddle_core::effect_id,
fiddle_core::payload_hash, AttemptJournal::record_step

## Context

M2 gave fiddle its first capability that changes something outside this process. A
capability that could construct its own client, or skip identity derivation, or
read a permissive deployment document as permission to ignore its own minimum,
would make the authorization order a convention rather than a property.

## Decision

Route every external mutation through `Executor::execute`, which walks seven steps
in order: validate the capability, derive identity, inspect the postcondition,
combine policy, authorize, apply, observe the postcondition. Give
`AuthorizedEffect` — the token an adapter must hold — private fields and no
constructor outside `effect/mod.rs`. Let deployment policy strengthen a
capability's minimum and never weaken it.

## Consequences

**The compiler checks the property, not a paragraph.** `IntegrationOperation::apply`
takes an `AuthorizedEffect` by reference, and the doctest at its definition is
`compile_fail` on the struct literal.

**A capability holds no credential and builds no client.** `capability::publish`
receives an `Executor` already bound to its own `CapabilityId`, and step 1 refuses a
proposal made under any other.

**`combine` is total over the product, not sampled.** `Deny` in the document is
absolute, a capability's `Human` minimum survives a permissive document, and `Allow`
is the absence of an extra gate rather than the removal of an existing one. The
interesting cell is the one an author is least likely to pick — capability `Human`
against deployment `Allow`.

**The order is observable.** Each `ExecutionStep` is announced to an `EffectTrace`
*before* the work behind it, and `record_step` appends it to the journal as two
closed enums and nothing else, so no credential and no unbounded string can reach
that file through it.

**Identity is recomputed, never remembered.** `effect_id` is blake3 over a
**length-prefixed** encoding of `(project, invocation_ref, kind, target)`, so a
fresh process derives the same id from canonical inputs and no field's contents can
be mistaken for structure.

**What was given up: `RequireHumanDecision` fails closed.** M2's three operations
all declare `Automatic`, so the rule is reachable only from the document, and it
returns `EffectError::HumanDecisionRequired` because the channel that would answer
it is M3's.
