# 016 — A permanent refusal is `Failed`, not `Retryable`

Status: accepted; amended in M3 by the note below
Cites: EffectError::recurrence, CapabilityError::recurrence, Recurrence, exit_code_for, orchestration::concluded, fiddle_core::policy::combine, GhError::Duplicate, crates/fiddle-acceptance/tests/github_deployment.rs

## Context

`fiddle_runtime::orchestration::run` turned every capability `Err` into `RunOutcome::Retryable` and exit 11. That was correct for every way M1 could fail, because each was an obstacle in front of the request. M2 routed `EffectError` through the same arm without revisiting it, and three of its six variants fail that test.

## Decision

Decide the row per failure, in `CapabilityError::recurrence`, and delegate to `EffectError::recurrence` for the effect family. Write both as exhaustive matches with no wildcard arm, so nobody can add a variant without being asked the question. Add no exit code and no `RunOutcome` variant.

## Consequences

- `fiddle run --capability publish_change` against a document that denies an effect now exits 20 where it exited 11. That is a breaking change for anything scripting the exit code, and it is the change. The old number told a caller to retry a refusal forever.
- Exit 20 gains producers beside `assess → Blocked`. So the number alone no longer means "fiddle could not observe the world". They stay apart by their reason text.
- The project gave up one uniform row for a per-failure one, and closed no taxonomy. `RunOutcome` still carries none, which is the gap ADR 013 named from M1. This decision widens the set of failures sharing a row. `docs/BACKLOG.md` records it under the same date.
- The classification is observable. `github_deployment.rs` asserts 20 for a policy deny and 11 for a forge that is not there, against the same capability. A build that collapsed the two rows fails one of them.
- `EffectError::recurrence` is a public method on a public type, and that is intended. The classification is what a future caller most needs and most easily gets wrong.

## The three variants that failed the test

`Retryable`'s own test is one question. Would repeating this invocation succeed, once somebody fixed what the reason names?

**`PolicyDenied`.** A `[github.policy]` deny is a property of the deployment document. Repeating hands `fiddle_core::policy::combine` the same pair and gets the same `Deny` back, for as long as the document says so.

**`HumanDecisionRequired`.** M2 had no decision channel, so nothing a repeat could reach would answer it.

**`DuplicateState`.** Two objects matched where the postcondition allows one. Looking again does not make them one object, and picking the first is what `GhError::Duplicate` exists to have refused.

`PayloadDiverged` arrived during the same milestone and has the same shape: the proposal and the operation disagree about what the request is, and both are this build's own code.

The practical cost was that automation retrying on exit 11 looped forever on a denied effect, while exit 20 stayed unreachable outside `assess → Blocked`. `docs/technical/SYSTEM.md`'s exit table stated the discriminator correctly and was accurate about a code that contradicted it. The acceptance assertion that pinned the 11 reasoned it against exit 2, and nobody made the comparison against 20.

## The test is not "could a human intervene"

Read that way, everything is correctable and exit 20 becomes unreachable. The test is whether the failure is an obstacle in front of the request or a conclusion about it, and the codebase already draws the line there. A change set carrying a foreign correlation marker is fixable, in the sense that somebody can settle whose change set it is, and `orchestration::concluded` maps it to `Failed` anyway because repeating re-derives the same verdict from the same observation. `DuplicateState` has that shape exactly, and sits there by that precedent rather than by the refusal argument the other three share.

`CapabilityError::NotAuthorised` and `CapabilityError::Misbound` are classified for completeness of the match and change nothing observable. Neither is reachable through `orchestration::run`, because a grant is only issued for the capability the derivation named and an executor is only bound to the run that built it.

## Amendment (M3) — the decision channel exists, and `HumanDecisionRequired` moved

This decision's table gave `EffectError::HumanDecisionRequired` the row `Permanent`, `Failed`, exit 20, and argued the point at length: `Suspended` promises a run is waiting, M2 had nothing that could arrive and resume it, and exiting 10 would tell an operator to wait for something never coming. It said the arm would move when the channel existed, and that the move would be a behaviour change with a decision behind it.

**The arm has moved, and no ADR records the move.** `EffectError::recurrence` answers `Recurrence::Awaiting` for `HumanDecisionRequired`, `orchestration::run` maps `Awaiting` to `RunOutcome::Suspended`, and `exit_code_for` maps `Suspended` to 10. `Recurrence` carries three variants, not the two this decision's table implies. So the row this file states for that variant is false, and this note is the only committed record of the change.

Two other rows of the table have grown arms this decision did not name. `Correctable` now also covers `CapabilityError::NothingProposed` and several `DecisionError` variants; `Permanent` now also covers `CapabilityError::PublishesElsewhere`, `CapabilityError::DecisionRejected`, `CapabilityError::Projection` and the refused arms of `PlanError` and `DedupError`. Read the table below as the M2 classification it was written as, and `CapabilityError::recurrence` as the current one.

| Failure | Recurrence | Outcome | Exit |
| --- | --- | --- | --- |
| `EffectError::PolicyDenied` | `Permanent` | `Failed` | 20 |
| `EffectError::HumanDecisionRequired` | `Awaiting` since M3 | `Suspended` | 10 |
| `EffectError::DuplicateState` | `Permanent` | `Failed` | 20 |
| `EffectError::PayloadDiverged` | `Permanent` | `Failed` | 20 |
| `EffectError::Unresolved` | `Correctable` | `Retryable` | 11 |
| `EffectError::Adapter` | `Correctable` | `Retryable` | 11 |
| `CapabilityError::{Write, CheckFailed, Workspace, Agent}` | `Correctable` | `Retryable` | 11 |
| `CapabilityError::{NotAuthorised, Misbound}` | `Permanent` | `Failed` | 20 |

What this decision got right is the rule rather than the row: the classification belongs on the error, in an exhaustive match, and `exit_code_for` stays the single realisation of the table. Adding `Awaiting` cost one variant and one arm, which is what that shape bought.
