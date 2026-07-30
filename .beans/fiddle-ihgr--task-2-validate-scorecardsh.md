---
# fiddle-ihgr
title: 'Task 2: validate-scorecard.sh'
status: todo
type: task
tags:
    - branch
created_at: 2026-07-30T11:20:24Z
updated_at: 2026-07-30T11:20:24Z
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
