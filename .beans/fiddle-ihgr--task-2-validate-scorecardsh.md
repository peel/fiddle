---
# fiddle-ihgr
title: 'Task 2: validate-scorecard.sh'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T11:37:43Z
parent: fiddle-85jh
---

Plan: docs/plans/2026-07-30-claude5-slimdown.md Task 2

## Context

Repo: /Users/peel/wrk/fiddle. Part of epic fiddle-85jh (Claude-5 skill slim-down). See the epic body for shared Contracts and the spec docs/specs/2026-07-29-claude5-slimdown-design.md for the House Style and Prompt-Side Invariant Set that govern every rewrite.

## Files

- Create: `scripts/validate-scorecard.sh`
- Test: `scripts/test-validate-scorecard.sh`

## Steps

- [ ] Write failing test: (1) valid scorecard (provider, dimensions object with non-empty evidence per scored dimension, criteria matching --criteria-ids "a,b" each with non-empty evidence, no spec_defect) → exit 0; (2) missing provider → exit 2; (3) criteria id not in --criteria-ids, or an expected id missing → exit 2 naming the id; (4) empty evidence on a criterion or scored dimension → exit 2; (5) dimensions present but not an object → exit 2; (6) explicit empty dimensions `{}` → exit 0 (evidence-only is valid); (7) spec_defect present with detected true but no reason → exit 2; (8) malformed JSON → exit 2.
- [ ] Run test, verify failure.
- [ ] Implement: `validate-scorecard.sh --scorecard <file> --criteria-ids <comma-list>` (ids extracted by the orchestrator from the bean's eval block, mirroring how resolve-domains.sh receives --domains). jq checks per the test matrix; all failures in one JSON error array on stderr, exit 2.
- [ ] Run test green; full sweep clean.
- [ ] Commit.

## Evaluation

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: valid-passes-invalid-named
      check: "Valid scorecards exit 0; each malformation (provider, criteria ids, empty evidence, dimensions type, spec_defect shape, bad JSON) exits 2 with a JSON error naming it"
    - id: evidence-only-valid
      check: "Explicit empty dimensions object exits 0"
    - id: interface-convention
      check: "Criteria ids arrive via --criteria-ids argument; the script parses no YAML"
thresholds: {}
```


## Evaluation Log
BASE_SHA: 3294d9801fe882d91fc48b2d9694c6b3788aa769
total_dispatches: 5

### Iteration 1 (2026-07-30T11:35:32Z)
dispatches: 2
**infrastructure:**
- correctness: 9/10
- domain_spec_fidelity: 9/10
- drift_resistance: 7/10
- idempotency: 8/10
- security_posture: 7/10

### Iteration 2 (2026-07-30T11:37:22Z)
dispatches: 3
**infrastructure:**
- correctness: 9/10
- domain_spec_fidelity: 9/10
- drift_resistance: 7/10
- idempotency: 8/10
- security_posture: 7/10

## Summary of Changes

Added scripts/validate-scorecard.sh (--scorecard FILE --criteria-ids LIST; exit 0 valid, exit 2 JSON error array; provider/criteria-id-set/evidence/dimensions-type/spec_defect checks; no YAML parsing) and scripts/test-validate-scorecard.sh (19 assertions, 8+ cases). Commit 5798bdf. Converged in 2 iterations, 3 dispatches.

## Dogfood Finding (for Task 3)

When validate-scorecard.sh was run on a real codex evaluator scorecard, codex had written dimension justifications under a `comment` field instead of the schema's `evidence` field, so the validator exited 2. Instructing codex to use `evidence` fixed it on re-dispatch. Task 3 wiring must handle this: either normalize codex dimension output (comment→evidence) before validation, or have validate-scorecard.sh accept `comment` as an evidence alias, or the re-dispatch-once policy will fire on every codex scorecard that scores dimensions. Recorded so the validator wiring does not make codex evaluators spuriously fail.
