# 055 — A declaration breach is returned to the model, on the same bound as an unaccounted report

Status: accepted
Cites: crates/fiddle-runtime/src/agent/returns.rs, ReturnHook, ReturnHook::declaration_failure, ReturnHook::accounting_failure, returns::declaration, returns::declaration_returned, returns::Declarations, returns::Held, returns::ACCOUNTING, returns::DECLARATION, cve::breached, cve::undeclared, cve::DeclarationBreach, agent::attempt_briefed, transcript::RETURNED, crates/fiddle-runtime/tests/cve_protocol.rs, a_report_whose_declaration_matches_the_diff_ends_the_attempt_in_one_report, a_report_declaring_a_file_it_did_not_change_is_returned_and_asked_for_the_work, a_report_omitting_a_file_it_changed_is_returned_and_asked_for_a_corrected_report, a_model_alternating_between_the_two_rules_is_returned_the_stated_total_and_no_more, a_path_declared_and_not_changed_asks_the_model_for_the_work, a_path_changed_and_not_declared_asks_the_model_for_a_corrected_report, a_report_wrong_in_both_directions_is_asked_for_the_work_it_declared, the_two_asks_are_different_sentences, a_path_the_run_changed_before_briefing_is_excused, a_run_whose_declarations_are_unchecked_returns_no_breach, a_derived_file_a_declared_command_wrote_is_a_breach_when_the_attempt_omits_it

## Context

Run 32642423186 spent three turns of a budget of forty.

```
[647]   turn 1  reason=tool_calls  list_files        dur=3ms
[1222]  turn 2  reason=tool_calls  read_file go.mod  dur=3ms
[73541] turn 3  reason=stop        content
```

`rationale: declared without changing: go.mod`. The note read "I need to update the version to 4.5.2 by replacing the vulnerable version in the direct dependency line". The model read `go.mod`, spent 72 seconds, called no writing tool, and reported having changed the file.

ADR 053 did not fire, and correctly. The report named the advisory, so the accounting rule passed. What failed is ADR 026's declaration rule, which is checked after the run and ends the attempt with 37 turns unspent.

This is the same shape ADR 053 fixed, one rule over. fiddle knows what is wrong, the sentence already exists, and the attempt has turns left.

## Decision

**A declaration breach is returned to the model as a turn.** `ReturnHook` runs the declaration rule beside the accounting rule, on `on_model_turn_finished`. It reads the workspace's changed-file set at that turn, compares it against the report's `changed_files` chained onto the paths the run changed before briefing, and returns the breach the comparison names.

**One hook, one bound, one counter.** `RETURNS` is 2 over the attempt and not per rule. A model alternating between an accounting failure and a declaration breach earns two returns in total.

**The accounting rule is read first.** `GroupMigration::migrate` refuses on accounting before it reads the diff. The hook does the same, so a return and a refusal can never disagree about which rule a report failed.

**The rule itself does not change.** `cve::breached` is the set comparison ADR 026 decided, and `cve::undeclared` now calls it. The hook, the post-run check and the operator-facing refusal read one function and render one `DeclarationBreach`.

## The two halves are different requests

`declared without changing` means the model reported work it did not do. The return asks for the work.

```
fiddle refused that report: declared without changing: go.mod. Change every
file you declared, then send one report whose changed_files names every file
you changed.
```

`changed without declaring` means the model did work it did not report. The return asks for a corrected report, and never for more edits.

```
fiddle refused that report: changed without declaring: main_test.go. Send one
report whose changed_files names every file you changed.
```

A report wrong in both directions gets the first sentence. The undone work is the larger failure, and asking for it already asks for a corrected report. `the_two_asks_are_different_sentences` fails if the two collapse into one.

## Where the rule is held, and where it is not

`Declarations` says so by name. `Declarations::Held` carries the workspace and the excused paths; `Declarations::Unchecked` is the repair path, which has no post-run declaration check. A return on a path where nothing afterwards refuses the same report would invent a rule, so the hook returns nothing there.

`Held` bundles the two things a run is held to, so `attempt_briefed` takes one argument for both and its arity does not grow.

## What did not change

ADR 026's rule is exactly as strict, and the post-run check in `GroupMigration::migrate` is still what ends the attempt when the bound is spent. `MigrationAttempt::undeclared` and `GroupStatus::of` are untouched, so an attempt whose declaration is still wrong on the third report is still published as a draft for a person to judge.

ADR 052's rules hold: `Redaction` is the one path to the transcript, the file is off unless `FIDDLE_TRANSCRIPT=1`, and both bounds are unmoved. ADR 053's accounting behaviour and its bound of two hold. ADR 054's retry is a different layer and is unaffected.

## Consequences

- The `returned` record gains a `rule` field, `accounting` or `declaration`. Without it a shared bound is unreadable in the transcript: two returns on one line each carrying a reason do not say that two rules shared one budget.
- A return costs one model call, so a tight turn budget can be spent on returns before the post-run check is reached. `cve_mitigation.rs` raises its deployment's `max_turns` from 4 to 6 for that reason, and `a_derived_file_a_declared_command_wrote_is_a_breach_when_the_attempt_omits_it` still reaches the draft it asserts. A deployment whose budget cannot afford two returns ends on the budget, as ADR 053 already said.
- A tree the hook cannot read returns nothing. The post-run check runs against the same tree and refuses the attempt, so an unreadable tree cannot pass as a clean declaration.
- `crates/fiddle-runtime/src/agent/accounting.rs` becomes `returns.rs`, and `AccountingHook` becomes `ReturnHook`. A module holding two rules cannot be named after one of them. ADR 053's `Cites:` line moves with it.
- Three scripted models repeat a breaching report `RETURNS + 1` times, in `cve_protocol.rs` and in `cve_mitigation.rs`. A script that ends on one breaching report no longer ends the run.
- The declaration rule reads paths and not contents. `cve::breached` takes `&[&str]`, so the hook calls `Workspace::changed_files` once a turn and never `Workspace::edits`, which reads every changed file whole out of git.
