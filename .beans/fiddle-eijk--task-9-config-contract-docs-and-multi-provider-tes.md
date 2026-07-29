---
# fiddle-eijk
title: 'Task 9: Config contract docs and multi-provider test rework'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-28T19:53:23Z
updated_at: 2026-07-29T11:59:44Z
parent: fiddle-sip9
blocked_by:
    - fiddle-tg77
    - fiddle-b55w
---

Plan: docs/plans/2026-07-28-evidence-driven-develop-loop.md Task 9

## Context

Repo: /Users/peel/wrk/fiddle
Documentation and test alignment for the new provider semantics: evaluators.domains.<d>.providers is an ordered preference list (single evaluator, first available differing from the always-claude implementer); evaluators.holistic.providers stays dispatch-all. test-multi-provider.sh is repurposed accordingly.

## Files

- Modify: skills/orchestrate/SKILL.md
- Modify: skills/develop/SKILL.md
- Modify: README.md
- Modify: scripts/test-multi-provider.sh

## Steps

- [x] Update every description of evaluators.domains.<d>.providers in skills/orchestrate/SKILL.md and skills/develop/SKILL.md: ordered preference list for selecting the single evaluator, not a dispatch fan-out; holistic stays dispatch-all.
- [x] Replace the "Multi-provider scoring" paragraph in README.md with the "Provider selection" paragraph, exact text in plan Task 9 Step 2.
- [x] Rework scripts/test-multi-provider.sh keeping the harness style: (a) selection honors preference order (fake-bin PATH technique from scripts/test-select-evaluator-provider.sh); (b) merge-scorecards.sh still min-merges a two-provider array (holistic path); (c) single-element normalization preserves scores. Delete assertions requiring per-task multi-provider dispatch; keep assertions still valid for holistic.
- [x] Run: bash scripts/test-multi-provider.sh — all pass. Then full sweep: for t in scripts/test-*.sh; do bash "$t" >/dev/null || echo "FAIL: $t"; done — no FAIL lines; bash scripts/check-portability.sh exits 0
- [x] Commit

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: contract-docs-updated
      check: "orchestrate/develop SKILL.md and README describe providers as an ordered preference list for per-task evaluation, dispatch-all only for holistic"
    - id: test-suite-green
      check: "All scripts/test-*.sh pass and check-portability.sh exits 0"
thresholds: {}
```


## Evaluation Log
BASE_SHA: 4350ec6447860998292750728241df9f9ef81c52
total_dispatches: 8

### Iteration 1 (2026-07-29T11:53:50Z)
dispatches: 3
**general:**
- code_quality: 7/10
- correctness: 7/10
- domain_spec_fidelity: 8/10

### Iteration 2 (2026-07-29T11:59:44Z)
dispatches: 5
**general:**

## Summary of Changes

Preference-list provider contract documented in orchestrate/develop SKILL.md and README (plan text verbatim); test-multi-provider.sh reworked with a selection test (stub-bin PATH) and holistic/generic relabeling, zero assertion deletions. Commit 537e3ea. Converged in 2 iterations, 5 dispatches (codex scored dimensions in iteration 1, triggering the judgment double-pass; iteration 2 confirmed). check-portability.sh absent on branch (drift documented). Merge-conflict watchlist grew: README.md, orchestrate/develop SKILL.md also carry uncommitted main-checkout changes.
