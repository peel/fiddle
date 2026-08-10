# 016 — A permanent refusal is `Failed`, not `Retryable`

Status: accepted

## Context

`fiddle_runtime::orchestration::run` turned every capability `Err` into `RunOutcome::Retryable`, and exit **11**. That was correct for every way M1 could fail: an unwritable change-set directory, a check that did not pass, a workspace that could not be prepared, a bounded attempt that produced no report. Each is an obstacle in front of the request, and each satisfies `RunOutcome::Retryable`'s own documented test — *would repeating this invocation, once someone has fixed what the reason names, succeed?*

M2 routed `EffectError` through the same arm without revisiting it, and three of its six variants do not satisfy that test:

- **`PolicyDenied`.** A `[github.policy]` deny is a property of the deployment document. Repeating hands `fiddle_core::policy::combine` the same pair and gets the same `Deny` back, for as long as the document says so.
- **`HumanDecisionRequired`.** M2 has no decision channel. Nothing a repeat can reach will answer it.
- **`DuplicateState`.** Two objects matched where the postcondition allows one. Looking again does not make them one object, and picking the first is precisely what `GhError::Duplicate` exists to have refused.

A fourth, `PayloadDiverged`, was added by remediation R4 during the same milestone and has the same shape: the proposal and the operation disagree about what the request is, and they are both this build's own code.

The practical cost was that automation retrying on exit 11 looped indefinitely on a denied effect, while exit **20** stayed unreachable outside `assess → Blocked` and the condition it was reserved for was live. `docs/technical/SYSTEM.md`'s exit table stated the discriminator correctly — *the discriminator is `Failed`'s own promise: this will not succeed by being repeated as invoked* — and was accurate about a code that contradicted it. The acceptance assertion that pinned the 11 (`github_deployment.rs`) reasoned it against exit **2**, the row a document fiddle declined to act on; the comparison against 20 was never made.

## Decision

The row is decided per failure, by `CapabilityError::recurrence`, which delegates to `EffectError::recurrence` for the one variant with two families inside it. Both are exhaustive `match`es with no wildcard arm, so a new variant cannot be added without its author being asked the question.

| Failure | Recurrence | Outcome | Exit |
| --- | --- | --- | --- |
| `EffectError::PolicyDenied` | `Permanent` | `Failed` | 20 |
| `EffectError::HumanDecisionRequired` | `Permanent` | `Failed` | 20 |
| `EffectError::DuplicateState` | `Permanent` | `Failed` | 20 |
| `EffectError::PayloadDiverged` | `Permanent` | `Failed` | 20 |
| `EffectError::Unresolved` | `Correctable` | `Retryable` | 11 |
| `EffectError::Adapter` | `Correctable` | `Retryable` | 11 |
| `CapabilityError::{Write, CheckFailed, Workspace, Agent}` | `Correctable` | `Retryable` | 11 |
| `CapabilityError::{NotAuthorised, Misbound}` | `Permanent` | `Failed` | 20 |

**The test is not "could a human intervene".** Read that way, everything is correctable and exit 20 becomes unreachable. The test is whether the failure is an *obstacle in front of* the request or a *conclusion about* it — and the codebase already draws the line there. A change set carrying a foreign correlation marker is fixable, in the sense that somebody can settle whose change set it is, and `orchestration::concluded` maps it to `Failed` anyway because repeating re-derives the same verdict from the same observation. `DuplicateState` is that shape exactly, and is placed by that precedent rather than by the refusal argument the other three share.

**`HumanDecisionRequired` is `Failed` and not `Suspended`,** although `Suspended`'s wording — *stopped short of a decision it is not entitled to make* — describes it. `Suspended` says a run is **waiting**: it promises something can arrive and resume it. In M2 nothing can, and exiting 10 would tell an operator to wait for something that is never coming. M2's epic contract reserves that row for M3 for the same reason. When the decision channel exists, this arm moves to `Suspended`, and the move is a behaviour change with a decision behind it rather than an exit code quietly meaning something new.

`CapabilityError::{NotAuthorised, Misbound}` are classified for completeness of the `match` and change nothing observable: neither is reachable through `orchestration::run`, because a grant is only ever issued for the capability the derivation named and an executor is only ever bound to the run that built it.

No new exit code, and no new `RunOutcome` variant. `exit_code_for` is unchanged and is still the single realisation of the table.

## Consequences

`fiddle run --capability publish_change` against a document that denies an effect kind now exits **20** where it exited 11. That is a breaking change for anything scripting the exit code, and it is the change: the previous number told a caller to retry a refusal forever.

Exit 20 gains four producers beside `assess → Blocked`, so the number alone no longer means "fiddle could not observe the world". They stay distinguishable by their reason text, which is the same mechanism exit 11's four producers already rely on — and the same gap ADR 013 named from M1: `RunOutcome` carries no taxonomy, and this decision widens the set of things sharing a row rather than closing it. Recorded in `docs/BACKLOG.md` under the same date.

The classification is observable, and deliberately: `github_deployment.rs` asserts 20 for a policy deny and 11 for a forge that is not there, in the same file against the same capability, so a build that collapsed the two rows fails one of them. `orchestration.rs`'s own tests assert the same pair one layer down, where everything except the row — the execution status, the progress status, the journal sequence — is identical between them.

`EffectError::recurrence` is a public method on a public type, so it is now part of `fiddle-runtime`'s surface. That is intended: the classification is the thing a future caller most needs and most easily gets wrong.
