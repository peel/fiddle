---
# fiddle-802d
title: 'Task 5: Process family rewrite'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T13:13:17Z
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


## Evaluation Log
BASE_SHA: 7620da6bedd16ccef71e98da377fbe539d4d5967
total_dispatches: 4

### Iteration 2 (2026-07-30T13:12:53Z)
dispatches: 4
**general:**

## Summary of Changes

Rewrote the ten process-family files to house style (commits 3436514, 79d49db, 771585c). Word count 10713 to 8327, a 22 percent cut and the epic's first substantial reduction: this family carried the redundant prose (rationalization tables, red-flag lists, Common Mistakes sections, dot-graph flows, repeated restatements) that earlier interface-heavy families did not. All six ordering invariants survive once each with rationale; no distinct handoff pointer lost (verified by token-set diff, count drops were duplicate mentions).

Converged in 2 iterations, 4 dispatches. Iteration 1 failed emphasis-gone: REQUIRED SUB-SKILL and REQUIRED caps labels survived because the grep pattern only covered MUST/NEVER/ALWAYS. Iteration 2 de-capped them while keeping the handoff semantics.

Also fixed: the rewrite introduced em dashes into tdd, debug, verify, and worktrees, four files that previously had none; a follow-up commit replaced them with colons, semicolons, parentheses, and conjunctions.

Resolved contradiction: finish-branch said clean up the worktree for Options 1, 2, 4 in Step 5 while its Quick Reference and other sections said 1 and 4. Resolved toward keeping the worktree for Option 2 (PR review feedback lands on the branch), stated inline. Evaluator judged the direction defensible and the inline note sufficient. Behavioral change for anyone who followed the Step 5 wording.

Handed to bean fiddle-54xz: stale skill names in worktrees and finish-branch Integration sections (brainstorming, subagent-driven-development, executing-plans, finishing-a-development-branch, using-git-worktrees) are not current fiddle skill names and need a rename pass. Backlog candidate: skills/debug/SKILL.md points at three reference files that do not exist and never have (root-cause-tracing.md, defense-in-depth.md, condition-based-waiting.md); the pointers were preserved because handoffs-intact required it.
