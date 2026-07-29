---
# fiddle-eijk
title: 'Task 9: Config contract docs and multi-provider test rework'
status: todo
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-28T19:53:23Z
updated_at: 2026-07-28T19:53:34Z
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

- [ ] Update every description of evaluators.domains.<d>.providers in skills/orchestrate/SKILL.md and skills/develop/SKILL.md: ordered preference list for selecting the single evaluator, not a dispatch fan-out; holistic stays dispatch-all.
- [ ] Replace the "Multi-provider scoring" paragraph in README.md with the "Provider selection" paragraph, exact text in plan Task 9 Step 2.
- [ ] Rework scripts/test-multi-provider.sh keeping the harness style: (a) selection honors preference order (fake-bin PATH technique from scripts/test-select-evaluator-provider.sh); (b) merge-scorecards.sh still min-merges a two-provider array (holistic path); (c) single-element normalization preserves scores. Delete assertions requiring per-task multi-provider dispatch; keep assertions still valid for holistic.
- [ ] Run: bash scripts/test-multi-provider.sh — all pass. Then full sweep: for t in scripts/test-*.sh; do bash "$t" >/dev/null || echo "FAIL: $t"; done — no FAIL lines; bash scripts/check-portability.sh exits 0
- [ ] Commit

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
