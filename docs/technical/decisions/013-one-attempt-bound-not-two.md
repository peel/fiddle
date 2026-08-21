# 013 — M1 ships one bound, and reports the one it does not enforce

Date: 2026-08-09
Status: accepted; amended in M4b by the note below, which records that the deferral is over and names the decision that ended it
Cites: agent.max_capability_attempts, MitigateConfig::max_attempts, CveMitigate::bound_reached, crates/fiddle-acceptance/tests/config_check.rs::config_check_reports_the_attempt_bound_it_enforces_and_where_the_count_lives, fiddle_runtime::attempt, RunOutcome::Retryable, AgentBudget

`crates/fiddle-cli/src/render.rs` no longer holds this file's stem. It holds 037's; see the amendment.

## Context

Design §6.4 specifies two independent limits: an outer `max_capability_attempts` owned by the runtime, and an inner `max_turns` enforced by Rig. They are independent on purpose, and only their product bounds what a deployment pays. M1 built one of them.

## Decision

Ship one bound and say so. Keep `max_capability_attempts` in the schema, because the reference configuration must load under `deny_unknown_fields`, and consume it in no run. Leave retry to the caller, expressed as `RunOutcome::Retryable` and exit 11.

## Consequences

- A deployment that writes `max_capability_attempts = 5` gets one attempt. `config check` says so out loud, reporting the document valid and the bound accepted and not enforced.
- The project gave up the outer bound, and the direction of the error is the conservative one. An invocation's ceiling is one attempt's, and the gateway key carries a $100 hard cap.
- The bounds that do fire are the ones an evaluator can score. `max_turns`, `deadline`, `max_changed_files`, `tool_timeout` and `workspace.command_timeout` each fire and each has a test that drives it past its limit.
- What M1 lacks is a second layer above them. `docs/BACKLOG.md`'s 2026-08-09 entry stays open, and this decision is what it resolves to.
- The calibration anchor `m1-bounded-behavior` required two independent bounds. It now requires the four that exist, each with a test.

## What `config check` reported in M1

```json
"max_capability_attempts": {
  "configured": 5,
  "enforced": 1,
  "status": "accepted-not-enforced",
  "decision": "013-one-attempt-bound-not-two"
}
```

Every bound that fires is a plain scalar in the same payload, so the shape alone separates the two kinds, and the `decision` key leads a reader here. The human rendering says the same in prose.

Three commitments follow, and breaking any of them silently is what this section prevents. `enforced` is the literal `ENFORCED_CAPABILITY_ATTEMPTS`, not a value read from the document, because the document's number does not apply. The milestone that builds the loop has to change that constant, drop the object for a plain scalar and delete this section. Until then `config_check_marks_the_attempt_bound_it_accepts_and_does_not_enforce` asserts all four keys from outside the process.

This corrects the position this section first took, that the edge was discoverable in the field's documentation and this file alone and was not surfaced at runtime. Design §6.6 promises that a deferred key is a loud error under `deny_unknown_fields`, and this key escaped that promise by a route §6.6 does not name: it is known rather than unknown, so strictness never looked at it. Deferring the retry loop is still right. Treating non-surfacing as inherent to the deferral was wrong, and that was a separate cheap choice, now made the other way.

## What taking up the second bound would cost

The two bounds are independent because Rig stops a looping conversation and the runtime stops a capability that keeps losing. Written down because "deferred" without a price is indistinguishable from "forgotten".

**1. `Retryable` is not one thing, and only one of the things it is should be retried.** Four sites in `crates/fiddle-runtime/src/orchestration.rs` produce it: an intent the journal could not record, a capability that returned `Err`, a re-derivation that reached `Execute` again, and a bundle that could not be published. Only the second is "the capability tried and lost". A loop around the outcome would repeat an unwritable report directory three times and reach exit 11 three times more slowly. So an outer bound needs a distinction the outcome type does not carry: a fifth thing on `RunOutcome`, or a retry decision taken inside `run` where the arms are still apart.

**2. Where the loop goes decides what an attempt is, and both placements move something load-bearing.** Inside `run`, a bundle's `capability_executions` and `progress` gain N entries where every consumer has seen one, and the publication-failure arm sits outside `run` anyway. Inside `attempt`, the attempt id is minted once, deliberately, so that no caller can collide two bundles on one path. N tries would either share one id, leaving the journal unable to say which try it describes, or publish N bundles for one invocation. The second breaks the shape `fresh_invocation.rs` and `m0_skeleton.rs` read, that two fresh processes produce two bundles with different attempt ids.

**3. It changes what committed M0 tests assert.** `a_capability_failure_is_retryable_and_recorded` asserts one execution and the journal sequence `["intent", "effect:failed"]`. `an_unrecordable_intent_stops_the_run_before_the_capability` asserts the sequence is exactly `["intent"]`. A retry loop makes both N-fold, and these are how "the world never moves before the intent to move it is recorded" is stated as an order rather than as two counters that agree.

**4. Nobody has decided what a fixture repair retries.** `stub_mark` is idempotent, so a second try is free. A repair is not: `Workspace::create` branches a fresh detached worktree from the same HEAD, so a second attempt starts from the unrepaired tree and discards the first attempt's work. That may be right for a model that went wrong, or wrong for a partial repair that was nearly right. N fresh worktrees and N model attempts in one worktree are different products with different costs.

**5. The reference configuration files this key under `[execution]`.** Design §6.6 defers that table to the milestone with a durable lifecycle to bound. Building the loop here means implementing a third of that table's semantics in a crate with no lifecycle, and then moving it.

Closing the gap means taking points 1 to 4 in order, and the taxonomy first, because the placement question cannot be answered before it.

This supersedes no earlier ADR. It records a deviation from design §6.4, so the deviation survives in a committed document rather than in a gitignored spec and a dead-code allowance.

## Amendment (M4b) — the bound is enforced, and it cost none of the five things priced above

The deferral is over. `crates/fiddle-cli/src/main.rs` passes `agent.max_capability_attempts` into `MitigateConfig::max_attempts`, and `CveMitigate::bound_reached` compares it against the count `attempts::read` pulls from the pull request body. A run at the bound reaches `Row::AttemptBoundReached`, calls no model, and leaves the pull request for a person. [037](037-the-attempt-bound-is-per-pull-request.md) records how, and it is the decision the payload now names.

**None of the five costs above was paid, because no loop was built.** Each of them priced a retry loop inside one process. M4b counts attempts across processes instead. One invocation still makes one attempt, and the number already spent lives in the pull request's body, so a fresh process reads what an earlier one wrote. `RunOutcome::Retryable` therefore keeps its four producers and needs no taxonomy, `attempt` still mints one id per process, and the M0 sequences are exactly what they were. Point 4 is untouched: nobody has decided what a fixture repair retries, and this bound does not ask, because it bounds the rework of a pull request rather than of a worktree. Point 5 stands: the key is still under `[agent]`, and the PRD says so.

The expensive thing was the loop, and this record was right about that. What it did not foresee was that the bound could fire without one.

**What `config check` says now.** The object is kept, and every key in it is true.

```json
"max_capability_attempts": {
  "configured": 5,
  "status": "enforced-per-pull-request",
  "counted_in": "pull-request-body",
  "decision": "037-the-attempt-bound-is-per-pull-request"
}
```

`enforced` is gone. A document writing 5 gets 5, so there is no second number to report, and `ENFORCED_CAPABILITY_ATTEMPTS` is deleted with it. The object survives the scalar rule above for a reason the rule did not anticipate: this bound is spent across processes, so it has a place its count is held, and a plain scalar cannot name that place. `counted_in` names it, because an operator raising a bound has to know the body is what to edit. `config_check_reports_the_attempt_bound_it_enforces_and_where_the_count_lives` asserts the payload and the prose beside it.

A run stopped by the bound also publishes both numbers. `RunDisposition` carries `attempt_bound` as `{spent, bound}`, because `attempt_bound_reached` and a pull request number cannot tell 2 of 2 from 5 of 5, and the person reading them decides whether to raise the bound.
