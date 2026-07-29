---
# fiddle-6c1s
title: 'Task 7: Stop-hook verdict gate'
status: todo
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-28T19:53:23Z
updated_at: 2026-07-28T19:53:34Z
parent: fiddle-sip9
blocked_by:
    - fiddle-b55w
---

Plan: docs/plans/2026-07-28-evidence-driven-develop-loop.md Task 7

## Context

Repo: /Users/peel/wrk/fiddle
A Claude Code Stop hook blocks turn-end while a develop-loop bean is active without a terminal verdict. develop-loop writes .fiddle/active-bean (bean id) at 1b and clears it on every terminal exit; the hook emits {"decision":"block","reason":...} when the marker is non-empty and fails open otherwise.

## Files

- Create: hooks/develop-verdict-gate.sh
- Modify: hooks/hooks.json
- Modify: skills/develop-loop/SKILL.md
- Modify: .gitignore
- Test: scripts/test-develop-verdict-gate.sh

## Steps

- [ ] Write failing test scripts/test-develop-verdict-gate.sh (assert_exit/assert_json harness): (1) no marker under CLAUDE_PROJECT_DIR → exit 0, empty output; (2) marker containing "fiddle-sip9" → exit 0, .decision == "block", .reason mentions the bean id; (3) empty marker file → exit 0, empty output. Exact test code in plan Task 7 Step 1.
- [ ] Run test, verify it fails: bash scripts/test-develop-verdict-gate.sh (hook missing)
- [ ] Implement hooks/develop-verdict-gate.sh per plan Task 7 Step 3: read ${CLAUDE_PROJECT_DIR:-.}/.fiddle/active-bean, fail-open (exit 0, no output) when missing/empty/jq unavailable, else jq -n block decision with reason naming the bean and the required terminal states. chmod +x.
- [ ] Run tests, verify pass: bash scripts/test-develop-verdict-gate.sh — Results: 7 passed, 0 failed
- [ ] Register in hooks/hooks.json: top-level "Stop" entry running develop-verdict-gate.sh with timeout 5 (JSON block in plan Task 7 Step 5); validate with jq . hooks/hooks.json
- [ ] Wire marker lifecycle in skills/develop-loop/SKILL.md: 'mkdir -p .fiddle && echo "{id}" > .fiddle/active-bean' at 1b; 'rm -f .fiddle/active-bean' at 1m CONVERGED and DISPATCHES_EXCEEDED rows, 1e BLOCKED and SPEC_DEFECT exits, and the 1g-1h spec-defect gate. Add .fiddle/ to .gitignore if not present.
- [ ] Commit

## Evaluation

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: block-when-active
      check: "Hook emits {decision: block} naming the bean when .fiddle/active-bean is non-empty"
    - id: fail-open
      check: "Hook exits 0 with empty output when marker is missing, empty, or jq is unavailable"
    - id: marker-lifecycle
      check: "develop-loop SKILL.md writes the marker at 1b and clears it on every terminal exit path"
thresholds: {}
```
