---
# fiddle-ye4j
title: 'Task 4: Lifecycle family rewrite + config dedup'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T12:52:46Z
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


## Evaluation Log
BASE_SHA: 5b830c27388235c7a955f376e795016d636ae28d
total_dispatches: 2

### Iteration 1 (2026-07-30T12:52:46Z)
dispatches: 2
**general:**

## Summary of Changes

Rewrote the eight lifecycle files to house style and centralized the orchestrate.json schema (commit 787b16e, rebased as part of the portability reconciliation). Zero markup and zero caps across the family; schema blocks now exist only in orchestrate/SKILL.md with seven pointer lines elsewhere naming just the keys each skill reads. Blind-before-reveal ordering survives in blind-spot-check.md with its anchoring rationale. Converged single-pass evidence-only, 2 dispatches.

Contradictions resolved: dispatch budget was 60 in docs, 10 in README, 16 in config; holistic max_iterations 3 in docs, 4 in config. Docs now state fallbacks-when-absent and point at orchestrate.json as the live value, because no script implements a fallback for either key — the model reads them straight from the file, which is why they drifted freely. README's copy of 10 is out of scope; backlog it.

Word count went UP for this family (6935 to 7196): orchestrate/SKILL.md absorbed schema keys that were previously undocumented anywhere, and caps directives became prose-with-rationale. Recorded rather than dressed up — tree-size reduction has to come from families with genuinely redundant prose.
