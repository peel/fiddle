---
# fiddle-802d
title: 'Task 5: Process family rewrite'
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

Plan: docs/plans/2026-07-30-claude5-slimdown.md Task 5

## Context

Repo: /Users/peel/wrk/fiddle. Part of epic fiddle-85jh (Claude-5 skill slim-down). See the epic body for shared Contracts and the spec docs/specs/2026-07-29-claude5-slimdown-design.md for the House Style and Prompt-Side Invariant Set that govern every rewrite.

## Files

- Modify: `skills/brainstorm/SKILL.md`, `skills/write-plan/SKILL.md`, `skills/define-beans/SKILL.md`, `skills/challenge/SKILL.md`, `skills/panel/SKILL.md`, `skills/tdd/SKILL.md`, `skills/debug/SKILL.md`, `skills/debug/root-cause-tracing.md`, `skills/debug/defense-in-depth.md`, `skills/debug/condition-based-waiting.md`, `skills/verify/SKILL.md`, `skills/worktrees/SKILL.md`, `skills/finish-branch/SKILL.md`

## Steps

- [ ] Rewrite to House Style. Invariants surviving plainly here: approval-before-implementation (brainstorm), red-first ordering (tdd), root-cause-before-fix and stop-after-repeated-failures (debug), verification-before-claiming (verify), typed-confirmation-before-discard (finish-branch). Tables of rationalizations/red flags collapse to at most one sentence each where the insight is real.
- [ ] Verify: emphasis greps empty for the family; cross-skill pointers intact (grep each "Use the fiddle:" reference still present); full sweep clean.
- [ ] Commit.

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: emphasis-gone
      check: "No emphatic markup remains in the process family"
    - id: ordering-invariants-survive
      check: "Approval-first, red-first, root-cause-first, verify-first, and typed-discard each appear once, plainly, with rationale"
    - id: handoffs-intact
      check: "Every cross-skill invocation pointer present before the rewrite is still present"
thresholds: {}
```
