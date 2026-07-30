---
# fiddle-n8d2
title: 'Task 3b: Evaluation family rewrite'
status: todo
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T11:20:24Z
parent: fiddle-85jh
blocked_by:
    - fiddle-e7op
---

Plan: docs/plans/2026-07-30-claude5-slimdown.md Task 3b

## Context

Repo: /Users/peel/wrk/fiddle. Part of epic fiddle-85jh (Claude-5 skill slim-down). See the epic body for shared Contracts and the spec docs/specs/2026-07-29-claude5-slimdown-design.md for the House Style and Prompt-Side Invariant Set that govern every rewrite.

## Files

- Modify: `skills/evaluate/SKILL.md`, `skills/evaluate/evaluator-general.md`, `skills/evaluate/evaluator-infrastructure.md`, `skills/evaluate/evaluator-frontend.md`, `skills/evaluate/evaluator-backend.md`, `skills/develop-holistic/SKILL.md`, `skills/develop/holistic-review.md`, `skills/develop/holistic-dimensions.md`, `skills/develop/holistic-scorecard-schema.md`, `skills/runtime-evidence/SKILL.md`

## Steps

- [ ] Rewrite to House Style (same removals/retentions as Task 3). Invariants surviving plainly here: evaluator distrust/evidence citation/output contract (the scorecard JSON schema stays verbatim — it is an interface), runtime-evidence-before-scoring, holistic-after-loop. Keep the explicit dimensions:{} contract text.
- [ ] Verify: emphasis greps empty for `skills/evaluate skills/develop-holistic skills/develop/holistic-*.md skills/runtime-evidence`; caps allowance as in Task 3; full sweep clean; word counts recorded.
- [ ] Commit.

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: emphasis-gone
      check: "No emphatic markup remains in the evaluation family"
    - id: contracts-survive
      check: "Scorecard JSON schema, dimensions:{} contract, distrust and evidence-citation invariants each appear once, plainly"
    - id: sweep-green
      check: "Full test sweep passes at the family checkpoint"
thresholds: {}
```
