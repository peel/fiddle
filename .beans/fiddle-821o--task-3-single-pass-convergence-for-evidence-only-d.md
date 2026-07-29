---
# fiddle-821o
title: 'Task 3: Single-pass convergence for evidence-only domains'
status: todo
type: task
tags:
    - branch
created_at: 2026-07-28T19:44:35Z
updated_at: 2026-07-28T19:44:35Z
parent: fiddle-sip9
---

Plan: docs/plans/2026-07-28-evidence-driven-develop-loop.md Task 3

## Context

Repo: /Users/peel/wrk/fiddle
Evidence-only verdicts (empty dimensions map) converge on the first PASS: re-running the same checks on unchanged code re-measures the same facts. Verdicts with scored dimensions keep the two-consecutive-passes rule. Do not modify existing test assertions.

## Files

- Modify: scripts/check-convergence.sh
- Test: scripts/test-check-convergence.sh

## Steps

- [ ] Append failing tests to scripts/test-check-convergence.sh (existing assertions untouched): (1) verdict PASS with "dimensions":{} and empty history expects exit 0, .status CONVERGED, .mode evidence-only; (2) verdict PASS with dimensions {"general.correctness":8} and empty history still expects exit 1 and PASS_PENDING; (3) verdict FAIL with empty dimensions expects exit 1. Exact test code in plan Task 3 Step 1.
- [ ] Run tests, verify new ones fail: bash scripts/test-check-convergence.sh
- [ ] Implement: in scripts/check-convergence.sh, after the VERDICT != PASS branch, insert the evidence-only short-circuit: DIM_COUNT=$(jq '.dimensions // {} | length' "$CURRENT"); if 0, echo '{"status":"CONVERGED","mode":"evidence-only"}' and exit 0.
- [ ] Run tests, verify all pass: bash scripts/test-check-convergence.sh
- [ ] Commit

## Evaluation

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: evidence-only-single-pass
      check: "PASS verdict with empty dimensions map returns CONVERGED (exit 0) with empty history"
    - id: judgment-double-pass-retained
      check: "PASS verdict with populated dimensions still returns PASS_PENDING on first pass"
    - id: existing-behavior-untouched
      check: "All pre-existing test-check-convergence.sh assertions pass unmodified"
thresholds: {}
```
