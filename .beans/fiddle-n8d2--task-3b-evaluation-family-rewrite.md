---
# fiddle-n8d2
title: 'Task 3b: Evaluation family rewrite'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T12:30:17Z
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


## Evaluation Log
BASE_SHA: e9e7481bcb56c4d73946097c5ad09b34566e66fc
total_dispatches: 5

## Scope Addendum (user-authorized 2026-07-30)

Two pre-existing assertions assert the literal HARD-GATE markup this epic removes, making emphasis-gone and sweep-green mutually exclusive. User authorized retargeting them to assert the surviving invariant text instead, preserving each test's intent. Files added to this bean's scope:
- Modify: scripts/test-multi-domain-holistic.sh (line ~249: "<HARD-GATE>" -> "before scoring any holistic dimension")
- Modify: scripts/test-runtime-e2e.sh (line ~116: "HARD-GATE" -> "before scoring any dimension")

### Iteration 1 (2026-07-30T12:27:29Z)
dispatches: 2
**general:**
- code_quality: 7/10
- correctness: 8/10
- domain_spec_fidelity: 8/10

### Iteration 2 (2026-07-30T12:30:16Z)
dispatches: 3
**general:**
- code_quality: 7/10
- correctness: 8/10
- domain_spec_fidelity: 8/10

## Summary of Changes

Rewrote the ten evaluation-family files to house style (commit 8639945): zero markup and zero caps across all ten; all four evaluator templates byte-identical on the shared Evidence Pack and Dimensions-optional sections (md5 850829da); scorecard JSON schema, dimensions:{} never-omit contract, distrust and evidence-citation invariants all intact. Word count 9188 to 8750 (-4.8%; the family is mostly 1-10 rubric scales, which are interface content preserved verbatim). holistic-dimensions.md left unchanged as already-house-style pure rubrics.

Spec defect found and resolved with user authorization (commit 5b830c2): test-multi-domain-holistic.sh and test-runtime-e2e.sh asserted the literal HARD-GATE markup this epic removes, making emphasis-gone and sweep-green mutually exclusive. Both assertions retargeted to the invariant the markup guarded (interaction before scoring a holistic dimension; recording before scoring any dimension), preserving test intent. Sweep restored to 17/17.

Converged in 2 iterations, 3 dispatches.
