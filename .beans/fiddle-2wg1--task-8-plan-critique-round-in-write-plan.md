---
# fiddle-2wg1
title: 'Task 8: Plan critique round in write-plan'
status: todo
type: task
tags:
    - branch
created_at: 2026-07-28T19:53:23Z
updated_at: 2026-07-28T19:53:23Z
parent: fiddle-sip9
---

Plan: docs/plans/2026-07-28-evidence-driven-develop-loop.md Task 8

## Context

Repo: /Users/peel/wrk/fiddle
Single external critique round for implementation plans: after write-plan's Self-Review and before Create Beans from Plan, each available provider from providers.phases.define reviews the plan against the design doc once; findings are folded inline, rejections noted, no re-dispatch.

## Files

- Modify: skills/write-plan/SKILL.md

## Steps

- [ ] Insert the "## Plan Critique (external providers)" section between "## Self-Review" and "## Create Beans from Plan", exact text in plan Task 8 Step 1: read providers.phases.define from orchestrate.json, skip when empty or none installed, dispatch each via hooks/dispatch-provider.sh --role plan-critic --design-doc-file <spec> --diff-file <plan> with instructions limited to coverage gaps, unverifiable steps, missing files, oversized tasks; fold accepted findings inline, note rejections, one round only.
- [ ] Verify: grep -n "## Plan Critique\|## Self-Review\|## Create Beans" skills/write-plan/SKILL.md shows Self-Review, Plan Critique, Create Beans in order; bash scripts/check-portability.sh exits 0
- [ ] Commit

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: critique-placement
      check: "Critique section sits between Self-Review and Create Beans from Plan; single round; skip rule for empty/unavailable providers"
    - id: critique-scope
      check: "Critique instructions ask only for coverage gaps, unverifiable steps, missing files, and oversized tasks; findings folded inline with rejections noted"
thresholds: {}
```
