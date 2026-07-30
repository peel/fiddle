---
# fiddle-e7op
title: 'Task 3: Develop core rewrite + validator wiring'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T12:10:47Z
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

## Dogfood Finding from Task 2 (must handle in validator wiring)

Codex evaluators sometimes write dimension justifications under a `comment` field instead of the schema's `evidence` field; validate-scorecard.sh then exits 2. When wiring validate-scorecard.sh into develop-loop 1f, prevent this from making every dimension-scoring codex scorecard spuriously fail: normalize codex dimension output (comment -> evidence) before validation, OR accept `comment` as an evidence alias in validate-scorecard.sh (adjust its test too), OR scope the validator to criteria+shape and not dimension-evidence. Pick one and note it; do not let the re-dispatch-once policy fire on a field-name quirk.


## Evaluation Log
BASE_SHA: 5798bdf46be3e6d7afba3c27b02f849235f97e5a
total_dispatches: 2

### Iteration 1 (2026-07-30T12:10:19Z)
dispatches: 2
**general:**

## Summary of Changes

Rewrote the eight develop-core files to house style and deleted iron-laws.md with both references (commit e9e7481). Zero emphasis markup and zero caps hits across the nine owned files; all 14 script invocations and cross-skill pointers preserved; frontmatter descriptions byte-identical. Both validators wired: develop Step 1 runs validate-bean-body.sh per bean (exit 2 stops), develop-loop 1f runs validate-scorecard.sh per evaluator return with the re-dispatch-once-then-needs-attention policy now written down for the first time. Word count 5829 to 4744 (-18.6%). Converged single-pass evidence-only, 2 dispatches.

Dogfood fix chosen: validate-scorecard.sh accepts `comment` as an evidence alias (plus two new tests, 22 passing), because provider-context.md documents `comment` to external evaluators while evaluate/SKILL.md uses `evidence` — the validator was the piece out of step. Fixed once for all callers rather than adding a pipeline stage to a skill being slimmed.

Carried forward: word reduction is 18.6% not the one-third the plan step suggested; residue is preserve-verbatim contract text (script invocations, schema, context-loading order). Task 4 config dedup removes ~120 more words from these files. Also flagged: the main checkout has an uncommitted frontmatter restructure of develop-loop/SKILL.md (name field and argument-hint) that will conflict at merge and cuts against the frontmatter-verbatim constraint.
