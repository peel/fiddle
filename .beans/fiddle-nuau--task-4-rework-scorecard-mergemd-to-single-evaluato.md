---
# fiddle-nuau
title: 'Task 4: Rework scorecard-merge.md to single-evaluator flow'
status: todo
type: task
tags:
    - branch
created_at: 2026-07-28T19:48:33Z
updated_at: 2026-07-28T19:48:33Z
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

- [ ] Append a pinning test to scripts/test-merge-scorecards.sh (existing assertions untouched): single-element scorecard array through merge-scorecards.sh preserves .domains.general.dimensions.correctness.score == 8 and .criteria[0].pass == true. Exact JSON fixture in plan Task 4 Step 1. If it already passes, keep it as a regression pin.
- [ ] Run: bash scripts/test-merge-scorecards.sh — all pass
- [ ] Rewrite the "Per-Domain Provider Merge (Step 1g)" section of skills/develop-loop/scorecard-merge.md as "Per-Domain Normalization (Step 1g)" per plan Task 4 Step 2: single-element jq -s pipe, note that min-merge/disagreements apply only to holistic, per-task eval-log passes no --disagreements. Update the Spec-Defect Check to scan the single scorecard-{domain}-{provider}.json instead of a multi-provider glob. Leave Cross-Domain Merge unchanged.
- [ ] Verify: grep -n "min\|disagreement" skills/develop-loop/scorecard-merge.md shows only holistic pointer and spec-defect text
- [ ] Commit

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
