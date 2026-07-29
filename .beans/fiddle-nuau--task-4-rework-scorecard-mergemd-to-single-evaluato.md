---
# fiddle-nuau
title: 'Task 4: Rework scorecard-merge.md to single-evaluator flow'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-28T19:48:33Z
updated_at: 2026-07-29T09:57:42Z
parent: fiddle-sip9
---

Plan: docs/plans/2026-07-28-evidence-driven-develop-loop.md Task 4

## Context

Repo: /Users/peel/wrk/fiddle
The per-task path has one evaluator per domain, so provider min-merging and disagreement tracking move out of the per-task docs (they remain for holistic review). merge-scorecards.sh keeps running as a single-input normalizer for uniform scorecard shape.

## Files

- Modify: skills/develop-loop/scorecard-merge.md
- Test: scripts/test-merge-scorecards.sh

## Steps

- [x] Append a pinning test to scripts/test-merge-scorecards.sh (existing assertions untouched): single-element scorecard array through merge-scorecards.sh preserves .domains.general.dimensions.correctness.score == 8 and .criteria[0].pass == true. Exact JSON fixture in plan Task 4 Step 1. If it already passes, keep it as a regression pin.
- [x] Run: bash scripts/test-merge-scorecards.sh — all pass
- [x] Rewrite the "Per-Domain Provider Merge (Step 1g)" section of skills/develop-loop/scorecard-merge.md as "Per-Domain Normalization (Step 1g)" per plan Task 4 Step 2: single-element jq -s pipe, note that min-merge/disagreements apply only to holistic, per-task eval-log passes no --disagreements. Update the Spec-Defect Check to scan the single scorecard-{domain}-{provider}.json instead of a multi-provider glob. Leave Cross-Domain Merge unchanged.
- [x] Verify: grep -n "min\|disagreement" skills/develop-loop/scorecard-merge.md shows only holistic pointer and spec-defect text
- [x] Commit

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: single-input-pinned
      check: "test-merge-scorecards.sh pins single-element normalization preserving scores and criteria"
    - id: no-per-task-min-merge
      check: "scorecard-merge.md no longer instructs per-task provider min-merging or disagreement tracking; cross-domain merge and spec-defect check remain"
thresholds: {}
```


## Evaluation Log
BASE_SHA: c6c887d44aa62bb92c0069348fb88b56d9b65031
total_dispatches: 17

### Iteration 1 (2026-07-29T09:42:35Z)
dispatches: 3
**general:**
- code_quality: 6/10
- correctness: 6/10 (FAIL, threshold 7)
- domain_spec_fidelity: 8/10
**Guidance:** "Revert the unforced criteria[]? tolerance in merge-scorecards.sh (keep antipatterns_detected tolerance) or add explicit exit-2 validation for scorecards lacking a criteria array; add a test pinning rejection of criteria-less scorecards."

### Iteration 2 (2026-07-29T09:52:15Z)
dispatches: 6
**general:**
- code_quality: 7/10
- correctness: 8/10
- domain_spec_fidelity: 8/10

### Iteration 3 (2026-07-29T09:57:42Z)
dispatches: 8
**general:**
- code_quality: 7/10
- correctness: 8/10
- domain_spec_fidelity: 8/10

## Summary of Changes

Reworked scorecard-merge.md Step 1g to Per-Domain Normalization (single evaluator per domain), singularized the spec-defect check, kept cross-domain merge. Pin tests 10-12 added. merge-scorecards.sh gained tolerance for optional fields (forced by the plan fixture) plus explicit exit-2 validation rejecting scorecards without a criteria array — iteration 1 evaluation caught that the initial unforced criteria tolerance created a silent-convergence path (criteria-less scorecard normalizing to empty criteria that check-thresholds accepts). Commits 810678c, 0bbd69e. Converged in 3 iterations, 8 dispatches.
