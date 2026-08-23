# 056 — A bound that a return spent names the rule and the budget

Status: accepted
Cites: crates/fiddle-runtime/src/agent/returns.rs, returns::exhausted, returns::Spent, returns::LastReturn, ReturnHook::spent, agent::classify, agent::attempt_briefed, AgentError::Bounded, crates/fiddle-runtime/tests/cve_protocol.rs, a_budget_that_affords_the_returns_ends_on_the_rule_and_not_on_the_budget, a_budget_one_turn_smaller_names_the_rule_and_the_budget, a_budget_spent_on_declaration_returns_names_the_declaration_rule, a_budget_exhausted_with_no_return_names_only_the_budget, a_budget_exhausted_after_no_return_names_the_budget_and_nothing_else, a_budget_a_return_spent_names_the_rule_the_count_and_the_budget, the_rule_the_sentence_names_is_the_rule_that_spent_the_last_turn

## Context

A return costs one model call. So a return can spend the last turn of the budget, and then rig raises `PromptError::MaxTurnsError` and `classify` reports:

```
the turn budget of 4 was exhausted
```

That names the wrong cause. The budget is how the attempt ended. The rule the report failed is why the turns went.

The `fiddle-57up` lane found this in its own work. A scenario budget of 4 turned a covered path from `unsafe_without_direction` into `retryable: the turn budget of 4 was exhausted`. Two returns had spent the turns, and the message said nothing about them.

This project has already paid for diagnostics that named a symptom. A tool call that did not name the tool. A provider failure that reported a status and discarded the message. A gate that printed a passing count from an incomplete run. This one is worse in one respect: **the message is true.** The budget was exhausted. Nothing in it looks wrong, so nobody checks it, and the operator raises a number that was never the problem.

## Decision

**A bound a return spent names the rule and the budget together.** `returns::exhausted` builds the reason from the turn budget and the hook's own record of its returns.

```
the turn budget of 6 was exhausted, and 2 of its turns were returns; the last
report failed the declaration rule: declared without changing: go.mod
```

**Both facts, or the sentence misleads.** The rule alone hides that the attempt could have continued. The budget alone hides why the turns went. The count is the third fact, and it is what says the returns spent the budget rather than the work.

**A run that returned nothing keeps the old sentence exactly.** `exhausted` reads `Spent::last`, and with no return it returns `the turn budget of N was exhausted` and nothing more. Naming a rule that never ran would send the reader to a check that never fired.

## The hook records, and `classify` reads a value

`ReturnHook` holds one lock over one `Spent`: the count and the last return's rule and reason. The count and the rule cannot disagree, because one mutex guards both and the hook writes them in one statement.

`classify` takes `&Spent`, and not the hook. `classify` is the one place a rig error becomes an `AgentError`, so the reason is composed once and two places cannot render it differently. Taking a value and not the hook keeps `exhausted` a pure function of the budget and the record, so it is proved without an agent, a model or a workspace.

`attempt_briefed` keeps the hook it registered and calls `ReturnHook::spent` after the run. The snapshot is taken once, after the run has ended, so no turn can change it between the read and the report.

## What this does not change

The rule, the bound and the sentence the model receives. ADR 053's accounting return, its bound of two and its `returned` record; ADR 055's declaration return, its two asks and the shared bound; ADR 026's declaration rule and its post-run check; ADR 052's `Redaction` path and both transcript bounds; ADR 054's retry. This record changes one diagnostic sentence.

**No budget was raised.** `fiddle-1z63` holds the budget question and has the measurements. A message that named the wrong cause is a defect in the message.

## Scope: the turn budget, and not the other three bounds

`AgentError::Bounded` has four sources: the turn budget, the deadline, the changed-file cap and the retry ceiling. Only the turn budget is counted in the same unit a return spends, so only it can be exhausted *by* returns in a way a count explains. A return also consumes wall time, so the deadline is the next candidate; it is left alone because a deadline reports elapsed time and not turns, and a count of returns does not explain it. If a deadline is seen to end an attempt a return was working through, that is a later change.

## Style

The reason is one sentence of about 30 words, which breaks the rule in `docs/technical/style.md` that a sentence uses 20 words or fewer. The rule is broken deliberately, and this is the note the rule asks for. Splitting the reason costs the reader the link between the budget and the rule, and that link is the whole of the fix.

## Consequences

- `ReturnHook::returned` becomes `ReturnHook::spent`, and returns the whole record rather than the count. Nothing outside the module called the old method.
- The internal per-turn struct is renamed from `Returned` to `Refusal`, so `Spent` and `LastReturn` name the record that outlives the turn.
- `classify` takes a third argument. Its four unit tests pass `Spent::default()`, which is the honest input: none of them is a turn-budget failure.
- `a_budget_exhausted_with_no_return_names_only_the_budget` asserts the whole reason with `assert_eq!` and not `contains`. A `contains` assertion would pass if the sentence grew a rule the run never applied.
