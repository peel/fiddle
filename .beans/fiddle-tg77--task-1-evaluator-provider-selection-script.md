---
# fiddle-tg77
title: 'Task 1: Evaluator provider selection script'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-28T19:44:35Z
updated_at: 2026-07-29T08:58:21Z
parent: fiddle-sip9
---

Plan: docs/plans/2026-07-28-evidence-driven-develop-loop.md Task 1

## Context

Repo: /Users/peel/wrk/fiddle
New bash script picking the single evaluator provider for a domain from an ordered preference list: first available provider (command -v; claude always available) that differs from the implementer's provider wins; fallback to implementer provider, then claude. See parent epic contracts for the output shape.

## Files

- Create: scripts/select-evaluator-provider.sh
- Test: scripts/test-select-evaluator-provider.sh

## Steps

- [x] Write failing test scripts/test-select-evaluator-provider.sh using the repo's assert_exit/assert_json harness (copy header from scripts/test-check-convergence.sh). Cover: (1) external available and differing from implementer picked (fake codex binary in mktemp bin dir, PATH-restricted invocation, expect .provider == "codex", exit 0); (2) no external available falls back to implementer with reason matching "fallback" (PATH=/usr/bin:/bin, --preference "codex,claude" --implementer claude, expect .provider == "claude"); (3) preference order respected (fake gemini+codex, --preference "gemini,codex", expect gemini); (4) missing --preference exits 2; (5) blank preference list returns claude. Full test code in plan Task 1 Step 1.
- [x] Run test, verify it fails: bash scripts/test-select-evaluator-provider.sh (script not found)
- [x] Implement scripts/select-evaluator-provider.sh per plan Task 1 Step 3: parse --preference/--implementer, available() helper (claude always true, else command -v), iterate comma-split preference trimming whitespace, emit JSON via jq -n on first available differing provider, else fallback chain. chmod +x.
- [x] Run tests, verify pass: bash scripts/test-select-evaluator-provider.sh — Results: 8 passed, 0 failed
- [x] Commit

## Evaluation

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: preference-order-respected
      check: "select-evaluator-provider.sh returns the first available provider differing from --implementer, in list order"
    - id: fallback-chain
      check: "Unavailable externals fall back to the implementer provider, then to claude; reasons distinguish the cases"
    - id: invalid-input-exit-2
      check: "Missing --preference exits 2 with a JSON error on stderr"
thresholds: {}
```


## Evaluation Log
BASE_SHA: 59c3196ca6b0787e3e95f85d40d2d2fd89272c44
total_dispatches: 29

### Iteration 1 (2026-07-29T08:17:23Z)
dispatches: 3
**infrastructure:**
- correctness: 5/10 (FAIL, threshold 7)
- domain_spec_fidelity: 7/10 (FAIL, threshold 8)
- drift_resistance: 6/10
- idempotency: 7/10
- security_posture: 7/10
**Guidance:** "Fallback must target --implementer provider even when absent from the preference list, claude only as final fallback, with distinct reasons. Add arity checks so dangling --preference/--implementer exit 2 with JSON instead of a set -u bash error. Build unknown-arg error JSON via jq to survive embedded quotes."
**Disagreements:**
- infrastructure.correctness: spread 3 (claude: 8, codex: 5)

### Iteration 2 (2026-07-29T08:29:10Z)
dispatches: 6
**infrastructure:**
- correctness: 4/10 (FAIL, threshold 7)
- domain_spec_fidelity: 5/10 (FAIL, threshold 8)
- drift_resistance: 6/10
- idempotency: 6/10 (FAIL, threshold 7)
- security_posture: 7/10
**Guidance:** "Codex evaluator could not execute the script in its read-only sandbox (here-string needs writable TMPDIR) and scored the execution failure as product defects; claude execution verified all criteria. Replace here-string parsing with temp-free expansion to make the script sandbox-tolerant."
**Disagreements:**
- infrastructure.correctness: spread 4 (claude: 8, codex: 4)
- infrastructure.domain_spec_fidelity: spread 3 (claude: 8, codex: 5)

### Iteration 3 (2026-07-29T08:52:03Z)
dispatches: 9
**infrastructure:**
- correctness: 7/10
- domain_spec_fidelity: 8/10
- drift_resistance: 7/10
- idempotency: 8/10
- security_posture: 7/10

### Iteration 4 (2026-07-29T08:57:37Z)
dispatches: 11
**infrastructure:**
- correctness: 7/10
- domain_spec_fidelity: 8/10
- drift_resistance: 7/10
- idempotency: 8/10
- security_posture: 7/10

## Summary of Changes

Implemented scripts/select-evaluator-provider.sh (ordered preference, first available provider differing from implementer, implementer fresh-context fallback independent of list membership, claude last resort, jq-built JSON on all paths, exit 0/2 contract) with scripts/test-select-evaluator-provider.sh (23 assertions incl. deny-file-write sandbox regression test). Commits 87b8648, 96f3a4d, 41ec758. Converged after 4 iterations, 11 dispatches: iteration 2 feedback fixed fallback semantics and arity checks; iteration 3 replaced here-string parsing with temp-free expansion after the codex evaluator read-only sandbox could not execute here-strings (budget raised to 16 mid-bean, commit fdf9133, to fit the double-pass).
