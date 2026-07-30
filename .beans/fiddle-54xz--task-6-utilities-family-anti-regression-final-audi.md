---
# fiddle-54xz
title: 'Task 6: Utilities family + anti-regression + final audit'
status: todo
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T11:20:24Z
parent: fiddle-85jh
blocked_by:
    - fiddle-ye4j
    - fiddle-802d
    - fiddle-n8d2
---

Plan: docs/plans/2026-07-30-claude5-slimdown.md Task 6

## Context

Repo: /Users/peel/wrk/fiddle. Part of epic fiddle-85jh (Claude-5 skill slim-down). See the epic body for shared Contracts and the spec docs/specs/2026-07-29-claude5-slimdown-design.md for the House Style and Prompt-Side Invariant Set that govern every rewrite.

## Files

- Modify: `skills/using-fiddle/SKILL.md` + its references/, `skills/adr/SKILL.md`, `skills/backlog/SKILL.md`, `skills/feedback/SKILL.md`, `skills/archive/SKILL.md`, `skills/init/SKILL.md`, `skills/insights/SKILL.md`, and every `skills/**/*.md` not covered by Tasks 3-5 (generate the list at task start with `find skills -name '*.md'` minus the enumerated files; record it in the bean summary so completion is checkable)
- Modify: `docs/technical/SYSTEM.md`

## Steps

- [ ] Rewrite remaining files to House Style; frontmatter `description` fields and every cross-skill handoff pointer preserved verbatim, as in all families.
- [ ] Anti-regression: add one invariant to SYSTEM.md ("Skills are written as judgment plus rationale; mechanical invariants live in scripts with exit-code contracts; no emphatic markup") and a three-line authoring note in using-fiddle stating the house style for contributors.
- [ ] Final audit across the whole tree: `grep -rn 'HARD-GATE\|<GATE>\|Rationalization Prevention\|## Red Flags\|\*\*Announce' skills/` → empty; `grep -rwn 'MUST\|NEVER' skills/` hits only frontmatter descriptions, JSON schema/interface text, or quoted external content (list every hit with its justification in the bean's Summary of Changes — durable on the bean, not only the commit body); record tree word-count before/after in the bean summary (target: substantial reduction).
- [ ] Full sweep + portability (run the main-checkout check-portability.sh against the tree if present); commit.

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: tree-audit-clean
      check: "Guardrail greps across skills/ are empty except justified frontmatter/schema/quoted hits, each listed in the commit body"
    - id: anti-regression-landed
      check: "SYSTEM.md carries the house-style invariant and using-fiddle carries the authoring note"
    - id: sweep-green
      check: "Full test sweep and available portability checks pass"
thresholds: {}
```
