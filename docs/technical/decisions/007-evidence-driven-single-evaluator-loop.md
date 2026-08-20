# 007 — Dispatch one evaluator per domain and hand it the evidence

Date: 2026-07-29
Status: accepted
Cites: hooks/dispatch-provider.sh, scripts/merge-scorecards.sh, hooks/develop-verdict-gate.sh, orchestrate.json

## Context

An adversarial per-task review rarely changed a verdict, because a judge model's errors correlate with a generator's. Running the redesign epic on itself settled it: the codex evaluator was sandboxed read-only and could not execute the code. It merged its own execution failures into FAIL verdicts, and stopped once a run handed it the evidence.

## Decision

Dispatch one evaluator per domain. Gather the evidence before dispatch into a per-domain pack. `hooks/dispatch-provider.sh` hands that pack to every evaluator through `--evidence-file`. Read each domain's `providers` array under `evaluators.domains` in `orchestrate.json` as an ordered preference list. Pick the first provider that differs from the implementer.

## Consequences

- A documentation-shaped bean converges in three dispatches rather than five or more. Six of eleven beans converged on evidence alone in the measured run.
- The project gives up per-task min-merge and disagreement tracking. Judgement about maintainability and architecture moves upstream to plan review and human merge.
- A read-only provider is now a first-class evaluator, because it never has to run the code.
- A dispatch budget needs headroom for the confirming second pass wherever a scored dimension remains. The per-task default rose from 10 to 16.
- An evaluator template requires evidence per criterion, so a reviewer can audit a scorecard against the pack.

`merge-scorecards.sh` now normalises a single input and refuses a scorecard without a criteria array. It answers "every scorecard must contain a criteria array". A domain may declare `"dimensions": {}` and converge on one pass; a scored dimension keeps the two-consecutive-passes rule. The `develop-verdict-gate.sh` Stop hook blocks turn-end while a bean carries no terminal verdict.

This ADR raised the holistic budget "3 to 4". `orchestrate.json` now sets `evaluators.holistic.max_iterations` to 8, and lists one holistic provider rather than several.
