---
# fiddle-qxyk
title: 'Task 5: Evidence-first evaluator templates'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-28T19:48:33Z
updated_at: 2026-07-29T10:40:50Z
parent: fiddle-sip9
---

Plan: docs/plans/2026-07-28-evidence-driven-develop-loop.md Task 5

## Context

Repo: /Users/peel/wrk/fiddle
Evaluator templates become evidence-first: the evaluator interprets a pre-gathered evidence pack, cites an artifact per criterion verdict, and emits scored dimensions only when the task's eval block sets thresholds for the domain. Existing dimension definitions stay as the when-configured definitions.

## Files

- Modify: skills/evaluate/evaluator-general.md
- Modify: skills/evaluate/evaluator-infrastructure.md
- Modify: skills/evaluate/SKILL.md

## Steps

- [x] Insert "## Evidence Pack" and "## Dimensions (optional)" sections after the title of evaluator-general.md, exact text in plan Task 5 Step 1: evidence pack interpretation role, mandatory per-criterion evidence citation (file + excerpt; no evidence = fail with reason "no evidence"), dimensions emitted only when thresholds configured, else "dimensions": {}.
- [x] Apply the same two sections to evaluator-infrastructure.md, preserving its existing dimension definitions below.
- [x] Update the scorecard contract in skills/evaluate/SKILL.md: dimensions may be an empty object for evidence-only evaluation; criteria entries gain a required "evidence" field citing the artifact; "provider" stays required.
- [x] Verify: grep -rn "dimensions" skills/evaluate/*.md | grep -iv "optional\|empty\|when.*threshold" shows no text asserting dimensions are always required
- [x] Commit

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: evidence-citation-required
      check: "Templates require each criterion verdict to cite its evidence artifact; missing evidence scores fail"
    - id: dimensions-optional
      check: "Templates and evaluate SKILL.md state dimensions are emitted only when the eval block sets thresholds; empty dimensions object otherwise"
    - id: role-boundary
      check: "Templates say the evaluator interprets pre-gathered evidence and does not gather it or judge beyond it"
thresholds: {}
```


## Evaluation Log
BASE_SHA: 0bbd69e2f2088a6ece1cd57315fee06e5af981de
total_dispatches: 8

### Iteration 1 (2026-07-29T10:37:41Z)
dispatches: 3
**general:**
- code_quality: 8/10
- correctness: 9/10
- domain_spec_fidelity: 9/10

### Iteration 2 (2026-07-29T10:40:49Z)
dispatches: 5
**general:**
- code_quality: 8/10
- correctness: 9/10
- domain_spec_fidelity: 9/10

## Summary of Changes

Evidence-first rework of evaluator-general.md, evaluator-infrastructure.md, and evaluate/SKILL.md (commit a314ad6): Evidence Pack + Dimensions (optional) sections, HARD-GATE conditional on thresholds, explicit dimensions:{} never-omit contract (matches check-convergence.sh single-pass), per-criterion artifact citation with no-evidence fail rule, infrastructure Verification Approach reframed interpret-only. Converged in 2 iterations, 5 dispatches. During evaluation both providers spontaneously followed the new contract (each emitted dimensions:{} in one iteration since the bean sets no thresholds) — live validation of the evidence-only shape. Out of scope, noted for later: evaluator-frontend.md/backend.md still carry gather-style framing; main checkout has uncommitted WIP on evaluate/SKILL.md (merge-conflict risk at finish-branch).
