---
# fiddle-im2e
title: 'Task 2: Evidence file support in dispatch-provider.sh'
status: todo
type: task
tags:
    - branch
created_at: 2026-07-28T19:44:35Z
updated_at: 2026-07-28T19:44:35Z
parent: fiddle-sip9
---

Plan: docs/plans/2026-07-28-evidence-driven-develop-loop.md Task 2

## Context

Repo: /Users/peel/wrk/fiddle
External providers run read-only and cannot execute tests, so evidence is gathered before dispatch and passed as a file. dispatch-provider.sh gains --evidence-file, appended to the payload as an "## Evidence" section, mirroring the existing --diff-file handling.

## Files

- Modify: hooks/dispatch-provider.sh
- Test: scripts/test-dispatch-evidence.sh

## Steps

- [ ] Write failing test scripts/test-dispatch-evidence.sh: fake provider binary that cats stdin back, invoke dispatch-provider.sh with --role evaluator --topic t --instructions i --evidence-file <tmp file containing "TestOutput: 12 passed, 0 failed">, assert payload contains "## Evidence" and the file content. Resolve the provider command the same way the hook already does (read how dispatch-provider.sh maps provider name to command; if from orchestrate.json, write a minimal one in the tmp dir and run from there). Full test skeleton in plan Task 2 Step 1.
- [ ] Run test, verify it fails: bash scripts/test-dispatch-evidence.sh (no "## Evidence" in payload)
- [ ] Implement: in hooks/dispatch-provider.sh add EVIDENCE="" to declarations, add parser case '--evidence-file) EVIDENCE="$(cat "$2")"; shift 2 ;;' next to --diff-file, and append '## Evidence' + content to the assembled payload exactly where the diff section is appended (follow that pattern verbatim, same variable).
- [ ] Run tests, verify pass: bash scripts/test-dispatch-evidence.sh — Results: 2 passed, 0 failed
- [ ] Commit

## Evaluation

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: evidence-in-payload
      check: "dispatch-provider.sh --evidence-file appends an '## Evidence' section with the file content to the provider payload"
    - id: no-regression-existing-args
      check: "--diff-file and --design-doc-file behavior is unchanged (existing dispatch tests still pass)"
thresholds: {}
```
