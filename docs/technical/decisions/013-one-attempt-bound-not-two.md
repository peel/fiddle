# 013 — M1 ships one bound, not two: `max_capability_attempts` is parsed and not consumed

**Date:** 2026-08-09
**Status:** accepted

## Context

Design §6.4 specifies **two independent limits**: an outer `max_capability_attempts` owned by `fiddle-runtime` and an inner `max_turns` enforced by Rig, plus a wall-clock deadline and a files-changed cap. The two are independent on purpose — Rig stops a looping conversation, the runtime stops a capability that keeps losing — and only their *product* bounds what a deployment pays.

M1 built one of them. `agent.max_capability_attempts` parses, defaults to 3, is pinned by two tests in `crates/fiddle-cli/src/config.rs`, and is **read by nothing**. It carries the one remaining `#[allow(dead_code)]` in `fiddle-cli`. `fiddle_runtime::attempt` runs a single attempt and reports `RunOutcome::Retryable` for a caller to repeat; nothing in this repository is that caller.

The gap was recorded in `docs/BACKLOG.md` (2026-08-09) and argued at the field. What was *not* recorded was the decision, and its absence had a cost: `docs/evaluator-calibration-general.md`'s `m1-bounded-behavior` anchor went on requiring, at its **Acceptable** level, "two independent bounds … each has a test that drives it past the limit" — a fourth nonexistent artifact in the very document a previous pass had rewritten to reconcile three others. An anchor that names something that does not exist scores every future bean against a fiction.

## Decision

**M1 ships one bound and says so.** `max_capability_attempts` stays in the schema — a document written against the reference configuration must load under `deny_unknown_fields`, and the pair of bounds is only meaningful written down together — and *no run consumes it*. Retry remains the caller's, expressed as `RunOutcome::Retryable` and exit 11.

"Says so" is now literal rather than a figure of speech. It was originally satisfied by this document and a field comment; the field carried a `#[allow(dead_code)]` whose stated reason was this ADR. That allowance is gone, because `config check` reads the value in order to report it as accepted-and-not-enforced — see **Consequences**. Nothing about the bound's *behaviour* changed: it is read to be reported, never to be applied.

The calibration anchor is corrected to require the bounds that exist: the inner turn limit, the wall-clock deadline, the files-changed cap, and the per-tool timeout, each with a test that drives it past its limit. The reconciled-claims preamble names this as the fourth claim it reconciles.

## What taking it up would cost

Written down because "deferred" without a price is indistinguishable from "forgotten", and because the next milestone should be able to start from the analysis rather than redo it.

**1. `Retryable` is not one thing, and only one of the things it is should be retried.** Four sites produce it, all in `crates/fiddle-runtime/src/orchestration.rs`: an intent the journal could not record, a capability that returned `Err`, a post-execution re-derivation that reached `Execute` again, and a bundle that could not be published. Only the second is "the capability tried and lost". A loop wrapped around the outcome would repeat an unwritable `<report.dir>` three times and reach exit 11 three times more slowly — and ADR 010 exists precisely because that path's *retryability* is a statement about the operator fixing permissions, not about trying again immediately. So an outer attempt bound needs a distinction the outcome type does not carry today: either a fifth thing on `RunOutcome`, or a retry decision taken inside `run` where the arms are still apart.

**2. Where the loop goes decides what an "attempt" is, and both placements move something load-bearing.** Inside `run`, a bundle's `capability_executions` and `progress` gain N entries where every consumer has only ever seen one, and the publication-failure arm is outside `run` anyway. Inside `attempt`, the attempt id is minted once — deliberately, so that no caller can hand in a duplicate and collide two bundles on one path — and the journal and the bundle are both filed under it; N tries therefore either share one id, making the journal unable to say which try it describes, or mint N ids and publish N bundles for one invocation. The second breaks the shape the stability proof reads: `fresh_invocation.rs` and `m0_skeleton.rs` both assert that *two fresh processes* produce two bundles with different `attempt_id`s, on the premise that one process produces one.

**3. It changes what committed M0 tests assert, and M0's lane is the baseline.** `orchestration::tests::a_capability_failure_is_retryable_and_recorded` asserts `executions.len() == 1` and the journal sequence `["intent", "effect:failed"]`; `an_unrecordable_intent_stops_the_run_before_the_capability` asserts the sequence is exactly `["intent"]`. Under a retry loop both become N-fold. These are not incidental assertions — they are how "the world never moves before the intent to move it is recorded" is stated as an *order* rather than as two counters that agree.

**4. Nobody has decided what `fixture_repair` retries.** `stub_mark` is idempotent, so a second try is free. A repair is not: `Workspace::create` branches a fresh detached worktree from the same HEAD, so a second attempt starts from the unrepaired tree and discards the first attempt's work — which may be the right thing (a clean slate for a model that went wrong) or exactly the wrong thing (throwing away a partial repair that was nearly right). "N fresh worktrees" and "N model attempts in one worktree" are different products with different costs, and the choice belongs with whoever is paying for the turns.

**5. The reference configuration files this key under `[execution]`, beside `run_timeout` and `max_parallel`.** Design §6.6 defers that table to the milestone with a durable lifecycle to bound. Building the loop here means implementing a third of that table's semantics in a crate that has no lifecycle, and then moving it.

## Consequences

**A deployment that writes `max_capability_attempts = 5` gets one attempt.** That is the sharp edge of this decision, and `config check` says so out loud. It reports the document valid — because it is — and reports the bound as *accepted and not enforced* beside the number of attempts that will actually be made:

```json
"max_capability_attempts": {
  "configured": 5,
  "enforced": 1,
  "status": "accepted-not-enforced",
  "decision": "013-one-attempt-bound-not-two"
}
```

Every bound that fires is a plain scalar in the same payload, so the shape alone tells the two kinds apart, and the `decision` key leads a reader here. The human rendering says the same thing in prose on the `agent.max_capability_attempts` line.

This corrects the position this section originally took, which was that the edge is discoverable in "exactly two places" — the field's documentation and this file — and "is not surfaced at runtime". That was true when this ADR was written and it is no longer. The correction matters beyond bookkeeping: design §6.6 promises that under `deny_unknown_fields` a deferred key is a loud error rather than a silently ignored one, and this key escaped that promise by a route §6.6 does not name — it is *known* rather than unknown, so strictness never looked at it. Deferring the retry loop is still right, and pricing it (above) is still the substance of this decision; what was wrong was treating non-surfacing as inherent to the deferral. It was a separate, cheap choice, and it has been made the other way.

Three commitments follow from the payload above, and breaking any of them silently is the failure this paragraph exists to prevent. `enforced` is a literal in `crates/fiddle-cli/src/render.rs` — `ENFORCED_CAPABILITY_ATTEMPTS`, currently `1` — not a value read from the document, because the whole point is that the document's number does not apply. The milestone that builds the loop has to change that constant, drop the object for a plain scalar, and delete this section. Until then, `crates/fiddle-acceptance/tests/config_check.rs::config_check_marks_the_attempt_bound_it_accepts_and_does_not_enforce` asserts all four keys from outside the process, so the surface cannot quietly stop existing.

**The direction of the error is the conservative one.** Nothing repeats, so an invocation's ceiling is one attempt's: one `max_turns` conversation, one `max_tokens` per completion, one `deadline`. A wrong outer bound would have been the other kind of mistake, and the gateway key carries a $100 hard cap (ADR 012).

**The bounds that do fire are the ones an evaluator can score.** `max_turns` (Rig's `MaxTurnsError`), `deadline`, `max_changed_files`, `tool_timeout` and `workspace.command_timeout` each fire and each has a test that drives it past its limit. What M1 does not have is a *second layer* above them.

**The backlog entry stays open, and this ADR is what it resolves to.** `docs/BACKLOG.md`'s 2026-08-09 entry records the gap; this records the decision and its price. Closing it means taking points 1–4 in order — the taxonomy first, because the placement question cannot be answered before it.

This supersedes no earlier ADR. It records a deviation from design §6.4, so the deviation survives in a committed document rather than only in a gitignored spec and a dead-code allowance.
