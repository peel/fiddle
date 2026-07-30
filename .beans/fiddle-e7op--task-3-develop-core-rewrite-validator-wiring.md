---
# fiddle-e7op
title: 'Task 3: Develop core rewrite + validator wiring'
status: todo
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T11:20:24Z
parent: fiddle-85jh
blocked_by:
    - fiddle-oz6z
    - fiddle-ihgr
---

Plan: docs/plans/2026-07-30-claude5-slimdown.md Task 3

## Context

Repo: /Users/peel/wrk/fiddle. Part of epic fiddle-85jh (Claude-5 skill slim-down). See the epic body for shared Contracts and the spec docs/specs/2026-07-29-claude5-slimdown-design.md for the House Style and Prompt-Side Invariant Set that govern every rewrite.

## Files

- Modify: `skills/develop/SKILL.md`, `skills/develop-loop/SKILL.md`, `skills/develop-loop/scorecard-merge.md`, `skills/develop-loop/restart-recovery.md`, `skills/develop-loop/attended-gate.md`, `skills/develop-loop/context-loading-order.md`, `skills/develop/implementer-prompt.md`, `skills/develop/provider-context.md`
- Delete: `skills/develop/iron-laws.md`

## Steps

- [ ] Rewrite each file to the spec's House Style: strip HARD-GATE/GATE markup, caps emphasis, rationalization tables, Red Flags, Announce lines; keep flow, plain script invocations, cross-skill pointers, frontmatter descriptions verbatim. Retain as single plain statements with one-line rationale (spec's Invariant Set): implementer-DONE-is-a-claim, budget-exceeded-ask-human, spec-defect routing, hold-out redaction, evaluator distrust/evidence/output contract, runtime-evidence-before-scoring, attended and blind ordering, holistic-after-loop-before-finish. Keep the Stop-hook marker lifecycle lines and all script invocations exactly.
- [ ] Wire validators: develop Step 1 becomes "For each task bean run scripts/validate-bean-body.sh --body <body-file> (--container for pure container features); stop on exit 2 and report the JSON errors." develop-loop 1f gains, after each evaluator returns: "Run scripts/validate-scorecard.sh --scorecard <file> --criteria-ids <ids from the eval block>; on exit 2 re-dispatch that evaluator once, then mark the bean needs-attention." Delete iron-laws.md and both references to it.
- [ ] Verify: `grep -rn 'HARD-GATE\|Rationalization\|## Red Flags\|\*\*Announce\|iron-laws' skills/develop skills/develop-loop` → empty; caps greps (`grep -rw 'MUST\|NEVER'`) only inside frontmatter descriptions, JSON schema/interface text, or quoted external content; full sweep clean; word count of the covered files reduced by at least a third (record before/after in the bean summary).
- [ ] Commit.

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: emphasis-gone
      check: "No HARD-GATE, rationalization table, Red Flags, Announce, or iron-laws reference remains in the develop core files"
    - id: invariants-survive
      check: "Each Invariant Set statement owned by these files appears exactly once, plainly, with rationale"
    - id: validators-wired
      check: "develop runs validate-bean-body.sh per bean; develop-loop runs validate-scorecard.sh per evaluator return with the re-dispatch-once policy"
    - id: sweep-green
      check: "Full test sweep passes at the family checkpoint"
thresholds: {}
```
