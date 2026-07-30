---
# fiddle-ye4j
title: 'Task 4: Lifecycle family rewrite + config dedup'
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

Plan: docs/plans/2026-07-30-claude5-slimdown.md Task 4

## Context

Repo: /Users/peel/wrk/fiddle. Part of epic fiddle-85jh (Claude-5 skill slim-down). See the epic body for shared Contracts and the spec docs/specs/2026-07-29-claude5-slimdown-design.md for the House Style and Prompt-Side Invariant Set that govern every rewrite.

## Files

- Modify: `skills/orchestrate/SKILL.md`, `skills/discover/SKILL.md`, `skills/discover-docs/SKILL.md`, `skills/define/SKILL.md`, `skills/deliver/SKILL.md`, `skills/deliver/blind-spot-check.md`, `skills/deliver-docs/SKILL.md`, `skills/quickfix/SKILL.md`

## Steps

- [ ] Rewrite to House Style (same removals/retentions as Task 3; blind-before-reveal invariant survives in blind-spot-check.md as one plain statement with its anchoring rationale).
- [ ] Config dedup: orchestrate/SKILL.md keeps the single full orchestrate.json schema; develop and develop-loop (already rewritten in Task 3 — touch only their config sections if the pointer wasn't placed then) and the lifecycle skills replace their embedded config JSON blocks with one line: "Config schema: see skills/orchestrate/SKILL.md; this skill reads <keys>." Resolve any contradictions found between the copies (record them in the commit body).
- [ ] Verify: `grep -rln 'max_dispatches_per_task\|"providers": {\|"evaluators": {' skills/` → schema blocks in skills/orchestrate/SKILL.md only (key-name prose mentions elsewhere allowed, embedded JSON schema blocks not); emphasis greps empty for the family; full sweep clean.
- [ ] Commit.

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: emphasis-gone
      check: "No emphatic markup remains in the lifecycle family"
    - id: schema-single-home
      check: "The orchestrate.json schema block exists only in orchestrate/SKILL.md; other skills name the keys they read and point there"
    - id: sweep-green
      check: "Full test sweep passes at the family checkpoint"
thresholds: {}
```
