---
# fiddle-54xz
title: 'Task 6: Utilities family + anti-regression + final audit'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T13:28:09Z
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

## Handed over from bean fiddle-802d

1. Stale skill names to rename in the Integration sections of skills/worktrees/SKILL.md (lines ~128-137) and skills/finish-branch/SKILL.md: brainstorming -> brainstorm, subagent-driven-development -> develop, executing-plans -> develop, finishing-a-development-branch -> finish-branch, using-git-worktrees -> worktrees. These are superpowers-era names, not current fiddle skill names.
2. skills/debug/SKILL.md points at three reference files that do not exist and never have: root-cause-tracing.md, defense-in-depth.md, condition-based-waiting.md (referenced in Phase 1 step 5 and the Supporting Techniques list). Either drop the pointers or record them in docs/BACKLOG.md; do not leave them dangling silently.


## Evaluation Log
BASE_SHA: 771585caa36923a38a576fc22b7b6784d6c63a3e
total_dispatches: 2

### Iteration 1 (2026-07-30T13:28:08Z)
dispatches: 2
**general:**

## Summary of Changes

Rewrote the thirteen remaining skill files, applied both handovers, landed anti-regression, and completed the tree-wide audit (commits de064ab and the follow-up caps sweep). Converged single-pass evidence-only, 2 dispatches.

Final audit: markup grep across skills/ returns zero hits. The caps grep returns exactly one hit, brainstorm/SKILL.md:3, the frontmatter description that is the trigger selector and which the spec preserves verbatim because slimming it risks non-activation. Tree word count 38549 to 34270, down 11.1 percent. Per family: develop core 5829 to 4744, evaluation 9188 to 8750, lifecycle 6935 to 7196 (rose: absorbed previously undocumented schema keys), process 10713 to 8327, utilities down 373 including the anti-regression additions.

Handovers applied: stale skill names renamed to current fiddle names in worktrees and finish-branch Integration sections, with the two aliases for develop deduplicated; the three dangling debug reference pointers dropped with their substance folded inline and a dated BACKLOG entry recording that the techniques were referenced but never written.

Anti-regression: SYSTEM.md Invariants gained the house-style invariant naming frontmatter, schemas, and quoted content as the exceptions and cross-referencing the authoring note; using-fiddle gained an Authoring Skills section. Last reviewed set to 2026-07-30.

Extra scope taken deliberately: the caps sweep left ANY/ALL/NOT in orchestrate quick-path criteria (lifecycle family, already converged) and a capitalized ALL in develop-holistic that a test asserted. Both were fixed here because this is the final-audit bean and leaving them meant knowingly shipping an incomplete success measure. The test assertion was updated to the new casing, which preserves its strength since it checks the invariant text rather than markup.

Recorded: check-portability.sh does not exist in this repo (drift carried from the previous epic), so the 17-suite sweep is the operative gate.
