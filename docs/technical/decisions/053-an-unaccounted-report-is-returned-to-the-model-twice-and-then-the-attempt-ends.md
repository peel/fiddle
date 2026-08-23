# 053 — An unaccounted report is returned to the model twice, and then the attempt ends

Status: accepted
Cites: crates/fiddle-runtime/src/agent/returns.rs, ReturnHook, ReturnHook::holding, ReturnHook::failure, returns::RETURNS, returns::returned_to_the_model, returns::report_in, agent::accounting, agent::unaccounted, agent::attempt_briefed, transcript::RETURNED, crates/fiddle-runtime/tests/cve_protocol.rs, a_report_that_accounts_for_everything_ends_the_attempt_in_one_report, a_report_that_accounts_for_nothing_is_returned_and_the_run_continues, a_model_that_never_accounts_for_the_advisory_ends_after_the_stated_returns, an_unfinished_claim_that_accounts_for_everything_ends_the_attempt, the_sentence_the_model_receives_names_the_advisory_it_left_out, the_plan_that_ended_run_32634427291_carries_an_accounting_failure, a_run_shown_no_advisory_has_no_accounting_rule_to_fail, a_turn_calling_a_tool_is_never_returned, a_report_failing_both_rules_is_returned_on_the_accounting_rule_first, nothing_in_this_workspace_decides_on_claimed_complete

## Context

Run 32634427291 spent three turns of a budget of forty. It ended in 3.4 seconds.

```
turn 1  reason=tool_calls  out=22   list_files
turn 2  reason=tool_calls  out=21   read_file go.mod
turn 3  reason=stop        out=261  content
```

Turn 3 was a plan written in the report's shape. It ended `Let me first check if there are any direct usages of the jwt package`, and it carried `claimed_complete: false` and `findings: []`.

Rig read that content as the structured answer and finished the run. fiddle then refused it with `the report does not account for what it was shown; shown and not reported: CVE-2025-30204`, and failed the attempt with 37 turns unspent.

The refusal is right. The report disposed of no advisory, so it is not a report.

The response is what is wrong. fiddle knew the report was incomplete. The model said the same. The deployment paid for 37 more turns, and fiddle spent none of them. An earlier run reached `Fixed by updating golang-jwt/jwt/v4 to v4.5.2`, so the model can do this work.

## Decision

**An unaccounted report is returned to the model as a turn.** `ReturnHook` runs the accounting rule inside the agent loop, on `on_model_turn_finished`. A turn whose report fails the rule is rejected, and the model is sent the failure and asked to continue.

**The bound is two returns.** `RETURNS` is 2. After two returns fiddle accepts the third unaccounted report into the run. The post-run check in `GroupMigration::migrate` then refuses it, exactly as it does today.

ADR 055 adds the declaration rule to the same hook and the same bound, and renames `AccountingHook` to `ReturnHook`. Nothing this record decides changes. Read the two together: the bound below is two returns over the attempt, and not two per rule.

**The accounting rule does not change.** `agent::unaccounted` still refuses a report that leaves an advisory out, invents one, disposes of one twice, or declines one without a reason. The hook calls the same `agent::accounting` function that builds the refusal, so a return and a refusal can never disagree about what is wrong.

## Why two

One return answers the observed failure. The transcript shows a model that had read `go.mod`, had named the fix, and needed one more turn. One return gives it that turn.

Two returns cover the model that misreads the first return. Three cover nothing more. Two returns name the advisory twice. A model that still leaves it out will leave it out again. Each further return is a paid call, and its answer is already known.

The bound counts returns, and it is not a share of the budget. A budget of forty turns and a budget of eight end the same way. The failure is in the model's reading of the report, and not in the turns it has left.

The turn budget bounds the returns a second time. A return spends one model call from the run's own budget, so a run that reaches `max_turns` ends on `max_turns` and not on this bound.

## What the model receives

The sentence carries fiddle's own accounting failure and then says what to do:

```
fiddle refused that report: the report does not account for what it was shown;
shown and not reported: CVE-2025-30204. Continue the work, then send one report
that accounts for every advisory this task showed you.
```

`returned_to_the_model` builds it from the reason `agent::accounting` returned. A generic instruction to try again names nothing the model can act on, and the model that produced run 32634427291 had already read the brief.

## `claimed_complete: false` does not end an attempt, and it does not extend one

This decision never reads `claimed_complete`. The field is the model's own claim. `nothing_in_this_workspace_decides_on_claimed_complete` already requires that no source under `crates` reads it except to record it. That rule stands.

So a report that accounts for every advisory ends the attempt whatever the claim says. ADR 026 and `unexplained_decline` already hold that a decline with a reason is an answer. A model that disposed of every advisory answered the question fiddle asked. A return would press it to claim more than it did.

What ended run 32634427291 was not the claim. It was a report that disposed of nothing. The accounting rule sees that, and the claim adds nothing the rule does not already know.

## What is returned, and what cannot be

`retry_model_turn` refuses a turn that carries a tool call, so that provider-visible history never holds an unanswered call. `ReturnHook` returns only a tool-free turn.

The run uses `OutputMode::Tool`, so a report can arrive by two channels. Run 32634427291 used the text channel: the gateway answered `finish: stop` with content, and rig accepted the text because it parses and carries every required field. That channel is the one this decision fixes.

A report that arrives as a call to the synthetic output tool is a tool-bearing turn. Rig intercepts that call before tool execution and finalizes the run, and it exposes no hook between the two. Such a report is refused after the run, as it is today. This is a real gap and not a choice: it closes when rig offers a hook on output-tool finalization.

## The transcript

Each return writes one `returned` record: the turn it refused, which return it was, the bound, and the reason. `RETURNED` is the sixth record kind.

The turn numbers show the return without the record. `AgentRun` increments its turn before every model call. So a returned turn 3 is followed by turn 4, and two `sent` records never share one number. ADR 052 said no run fiddle makes retries a turn; one does now, and the `returned` record is what says so.

Nothing in ADR 052 changes. `Redaction` is the one path to the file, and the reason is text that passes through it. The record adds three numbers and one short reason to a run, so neither bound moves. The transcript is still off unless `FIDDLE_TRANSCRIPT=1`.

## Consequences

- `attempt_briefed` takes one more argument: the advisories the run was shown. `attempt` passes an empty slice, because the repair path shows none.
- A run shown no advisory has no accounting rule to fail, so it installs a hook that returns nothing. The repair path is unchanged.
- `agent::accounting` is public and returns the reason. `agent::unaccounted` wraps it in `AgentError::Protocol` and keeps its signature, so every caller and every existing test is unchanged.
- `ReturnHook` is registered after `TranscriptHook`. A `Retry` stops the hooks after it on that event. The transcript writes its own records on `on_completion_response`, and that event fires first. So a refused turn is recorded whole before it is refused.
- Three scripted models in `cve_protocol.rs` now repeat their bad report `RETURNS + 1` times. A script that ends on one bad report no longer ends the run, and `MockCompletionModel` answers a turn it has no script for with a provider error.
- The bound and the accounting rule are proved by different tests. `a_model_that_never_accounts_for_the_advisory_ends_after_the_stated_returns` asserts the failure is still `AgentError::Protocol` naming the advisory, so reaching the bound changes when the attempt ends and not how.
