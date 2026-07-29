---
# fiddle-qxyk
title: 'Task 5: Evidence-first evaluator templates'
status: todo
type: task
tags:
    - branch
created_at: 2026-07-28T19:48:33Z
updated_at: 2026-07-28T19:48:33Z
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

- [ ] Insert "## Evidence Pack" and "## Dimensions (optional)" sections after the title of evaluator-general.md, exact text in plan Task 5 Step 1: evidence pack interpretation role, mandatory per-criterion evidence citation (file + excerpt; no evidence = fail with reason "no evidence"), dimensions emitted only when thresholds configured, else "dimensions": {}.
- [ ] Apply the same two sections to evaluator-infrastructure.md, preserving its existing dimension definitions below.
- [ ] Update the scorecard contract in skills/evaluate/SKILL.md: dimensions may be an empty object for evidence-only evaluation; criteria entries gain a required "evidence" field citing the artifact; "provider" stays required.
- [ ] Verify: grep -rn "dimensions" skills/evaluate/*.md | grep -iv "optional\|empty\|when.*threshold" shows no text asserting dimensions are always required
- [ ] Commit

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
