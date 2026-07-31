# 007 — Evidence-driven single-evaluator develop loop

Status: accepted (2026-07-29)

## Context

Adversarial per-task code reviews across multiple providers rarely changed outcomes within one or two iterations: judge-model errors correlate with generator errors, so a second same-class opinion added token cost without quality gain. Running the redesign epic on itself supplied the decisive evidence: the codex evaluator, sandboxed read-only, could not execute the code under review and min-merged its own execution failures into FAIL verdicts until evidence was handed to it as an artifact.

## Decision

The per-task loop dispatches ONE evaluator per domain. Evidence (test output, checks, runtime probe transcripts) is gathered before dispatch into a per-domain pack and handed to every evaluator; external providers receive it via `dispatch-provider.sh --evidence-file`. The evaluator provider comes from the domain's `providers` array reinterpreted as an ordered preference list: first available provider differing from the always-claude implementer, falling back to the implementer's provider in a fresh context. Per-task min-merge and disagreement tracking are removed; `merge-scorecards.sh` remains as a single-input normalizer and rejects scorecards without a criteria array. Scored dimensions are optional per domain: an explicitly empty `"dimensions": {}` scorecard converges on a single pass, while scored dimensions keep the two-consecutive-passes rule. A Stop hook (`develop-verdict-gate.sh`) blocks turn-end while a bean lacks a terminal verdict. Holistic review keeps multi-provider dispatch, min-merge, and coverage-matrix union unchanged.

## Consequences

Typical documentation-shaped beans converge in 3 dispatches instead of 5 or more, and the empirical run showed six of eleven beans converging single-pass evidence-only. Maintainability and architecture judgment move upstream to plan review and human merge rather than living in evaluator scorecards. Read-only providers become first-class evaluators. Dispatch budgets need headroom for the confirming double-pass where judgment dimensions remain (per-task default raised 10 to 16, holistic 3 to 4 during the epic); the convergence script's budget-check counting convention at the boundary remains a known issue. Evaluator templates now enforce per-criterion evidence citation, so scorecards are auditable against the pack.
