---
# fiddle-b55w
title: 'Task 6: Rework develop-loop SKILL.md to the simplified flow'
status: todo
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-28T19:48:33Z
updated_at: 2026-07-28T19:53:34Z
parent: fiddle-sip9
blocked_by:
    - fiddle-tg77
    - fiddle-nuau
---

Plan: docs/plans/2026-07-28-evidence-driven-develop-loop.md Task 6

## Context

Repo: /Users/peel/wrk/fiddle
develop-loop's evaluation chain becomes: gather evidence pack per domain (new step 1e-2), select one evaluator provider per domain via scripts/select-evaluator-provider.sh, dispatch one evaluator per domain (claude subagent or dispatch-provider.sh with --evidence-file), normalize via single-input merge. See parent epic contracts for file shapes.

## Files

- Modify: skills/develop-loop/SKILL.md

## Steps

- [ ] Insert step "1e-2. Gather Evidence Pack (per domain)" between 1e and 1f, exact text in plan Task 6 Step 1: move the Runtime Start gate here; run project test command into evidence-{domain}-tests.txt; run eval-block-named validation scripts into evidence-{domain}-checks.txt; runtime probes into evidence-{domain}-runtime.txt; concatenate to evidence-{domain}.txt with "### <source>" headers.
- [ ] Replace the "Per-Domain, Per-Provider Evaluator Dispatch" content of 1f with the single-evaluator dispatch, exact text in plan Task 6 Step 2: select via scripts/select-evaluator-provider.sh --preference "<providers joined with commas>" --implementer claude > selected-provider.json; one evaluator per domain (claude subagent with evidence pack in context, or dispatch-provider.sh --evidence-file evidence-{domain}.txt); accounting "one implementer + one evaluator per domain per iteration"; PASS_PENDING re-eval reuses selected-provider.json.
- [ ] Update 1g to point at "Per-Domain Normalization" in scorecard-merge.md; remove "2 providers x 2 domains = 4 dispatches" examples from 1f and 1l; 1l passes no --disagreements on the per-task path and records the selected provider and reason from selected-provider.json in the log entry.
- [ ] Verify: grep -n "per-provider\|each provider\|min scoring" skills/develop-loop/SKILL.md leaves only preference-list semantics; bash scripts/check-portability.sh exits 0
- [ ] Commit

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: evidence-pack-step
      check: "SKILL.md has a per-domain evidence-pack step (tests, checks, runtime probes) before evaluator dispatch"
    - id: single-dispatch
      check: "Exactly one evaluator per domain per iteration, provider chosen via select-evaluator-provider.sh; accounting examples updated"
    - id: no-stale-fanout
      check: "No remaining instruction dispatches evaluators for every provider on the per-task path"
thresholds: {}
```
