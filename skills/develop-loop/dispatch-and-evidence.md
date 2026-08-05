# Dispatch and evidence

For a fresh bean, capture `BASE_SHA`, initialize its evaluation log, set it `in-progress`, and arm `.fiddle/active-bean`; reset `dispatch_count` and `iteration`. On restart, follow [restart-recovery.md](restart-recovery.md).

Resolve the `implementer` model for phase `develop` before dispatching the implementer. Pass the result only when the resolver emits `model`; otherwise inherit the session. Dispatch the implementer with `skills/develop/implementer-prompt.md`, complete task context, the eval block excluding `holdout: true` criteria, current-domain antipatterns truncated at `## Retired`, and prior non-holdout feedback. Count the dispatch.

A DONE report is only a claim: gather evidence before evaluation. BLOCKED and SPEC_DEFECT become `needs-attention` and clear the active marker; a spec defect records its evidence and DEFINE re-entry pointer, decrements the implementer dispatch, and writes no evaluator log. NEEDS_CONTEXT receives the requested context and re-dispatches.

For each domain, start configured runtimes in order; retry a harness-failure exit once and escalate a second failure without charging the budget. Capture test output, configured validation scripts, and runtime probes into one evidence pack with source headings. Evaluators interpret this pack and never gather their own evidence.

Resolve the `evaluator` model for phase `develop` before dispatching an internal evaluator; omit a model when the resolver returns session inheritance. Select one evaluator per domain with `scripts/select-evaluator-provider.sh`; the ordered provider preference chooses the first available provider distinct from the Claude implementer, otherwise a fresh Claude evaluator. Assemble static context through `scripts/assemble-evaluator-context.sh`; append runtime state, task criteria, and prior diff/scorecard/guidance only after it. Dispatch the selected provider, save its scorecard, and count the dispatch.

Validate every scorecard with `scripts/validate-scorecard.sh`. On a schema failure, re-dispatch that evaluator once with the JSON errors; a second invalid scorecard escalates. Stop all runtimes after evaluators complete.

External providers return their final scorecard JSON according to `skills/develop/provider-context.md`; that output contract applies after run-state sections are appended.
