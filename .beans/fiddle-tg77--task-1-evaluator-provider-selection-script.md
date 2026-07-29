---
# fiddle-tg77
title: 'Task 1: Evaluator provider selection script'
status: todo
type: task
tags:
    - branch
created_at: 2026-07-28T19:44:35Z
updated_at: 2026-07-28T19:44:35Z
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

- [ ] Write failing test scripts/test-select-evaluator-provider.sh using the repo's assert_exit/assert_json harness (copy header from scripts/test-check-convergence.sh). Cover: (1) external available and differing from implementer picked (fake codex binary in mktemp bin dir, PATH-restricted invocation, expect .provider == "codex", exit 0); (2) no external available falls back to implementer with reason matching "fallback" (PATH=/usr/bin:/bin, --preference "codex,claude" --implementer claude, expect .provider == "claude"); (3) preference order respected (fake gemini+codex, --preference "gemini,codex", expect gemini); (4) missing --preference exits 2; (5) blank preference list returns claude. Full test code in plan Task 1 Step 1.
- [ ] Run test, verify it fails: bash scripts/test-select-evaluator-provider.sh (script not found)
- [ ] Implement scripts/select-evaluator-provider.sh per plan Task 1 Step 3: parse --preference/--implementer, available() helper (claude always true, else command -v), iterate comma-split preference trimming whitespace, emit JSON via jq -n on first available differing provider, else fallback chain. chmod +x.
- [ ] Run tests, verify pass: bash scripts/test-select-evaluator-provider.sh — Results: 8 passed, 0 failed
- [ ] Commit

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
