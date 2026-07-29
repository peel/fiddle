---
# fiddle-asf2
title: 'Fix: evidence-first rework of frontend/backend evaluator templates'
status: completed
type: task
priority: normal
tags:
    - branch
    - remediation
created_at: 2026-07-29T12:10:24Z
updated_at: 2026-07-29T12:24:38Z
parent: fiddle-sip9
---

Remediation from holistic review iteration 1 (source: dimension holistic_spec_fidelity 7/8; spec sections 3a and 6).

## Context

Repo: /Users/peel/wrk/fiddle (work in worktree .worktrees/evidence-driven-develop-loop)
evaluator-frontend.md and evaluator-backend.md retain Launch/Evidence Gathering sections instructing the evaluator to gather evidence itself and lack the Evidence Pack / Dimensions (optional) sections, contradicting the evaluator role in skills/evaluate/SKILL.md and develop-loop 1e-2. Mirror the Task 5 treatment of evaluator-general.md / evaluator-infrastructure.md (see commit a314ad6 for the exact section text and approach): insert the two sections after the title, keep dimension definitions below as when-configured definitions, reframe evidence-gathering instructions as what the pack should show (runtime domains: the runtime stays running through 1f, so interaction the pack already records is described, not re-performed). The explicit dimensions:{} never-omit contract must be stated.

## Files

- Modify: skills/evaluate/evaluator-frontend.md
- Modify: skills/evaluate/evaluator-backend.md

## Steps

- [x] Insert "## Evidence Pack" and "## Dimensions (optional)" sections after the title of both templates (copy from evaluator-general.md, commit a314ad6), preserving existing dimension definitions below.
- [x] Reframe self-gathering instructions (Launch/Evidence Gathering) as pack-interpretation, mirroring the evaluator-infrastructure.md "What the Evidence Should Show" treatment.
- [x] Verify: grep -L "Evidence Pack" skills/evaluate/evaluator-*.md returns nothing; grep -rn "dimensions" skills/evaluate/evaluator-frontend.md evaluator-backend.md | grep -iv "optional\|empty\|when.*threshold" shows no always-required assertions; full sweep clean.
- [x] Commit

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: frontend-backend-templates-evidence-first
      check: "Both templates carry Evidence Pack and Dimensions (optional) sections consistent with evaluator-general.md, with explicit dimensions:{} never-omit contract"
    - id: no-self-gathering-contradiction
      check: "No template instructs the evaluator to gather evidence in contradiction of the evaluator role in evaluate/SKILL.md"
thresholds: {}
```


## Evaluation Log
BASE_SHA: 537e3ea1aea6b5a03121534e5a7f66d89deaed81
total_dispatches: 3

### Iteration 1 (2026-07-29T12:24:38Z)
dispatches: 3
**general:**

## Summary of Changes

evaluator-frontend.md and evaluator-backend.md got the evidence-first treatment (commit 340cafc): Evidence Pack + Dimensions (optional) byte-identical to evaluator-general.md, Runtime Interaction replaced by pack-interpretation Verification Approach, not-running fallback scored from pack. Converged single-pass evidence-only, 3 dispatches. Noted for later: develop-loop 1f sentence about evaluator app interaction sits in mild tension with the interpret-only role.
