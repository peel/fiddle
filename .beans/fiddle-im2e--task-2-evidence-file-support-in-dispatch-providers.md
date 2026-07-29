---
# fiddle-im2e
title: 'Task 2: Evidence file support in dispatch-provider.sh'
status: completed
type: task
priority: normal
tags:
    - branch
created_at: 2026-07-28T19:44:35Z
updated_at: 2026-07-29T09:14:27Z
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

- [x] Write failing test scripts/test-dispatch-evidence.sh: fake provider binary that cats stdin back, invoke dispatch-provider.sh with --role evaluator --topic t --instructions i --evidence-file <tmp file containing "TestOutput: 12 passed, 0 failed">, assert payload contains "## Evidence" and the file content. Resolve the provider command the same way the hook already does (read how dispatch-provider.sh maps provider name to command; if from orchestrate.json, write a minimal one in the tmp dir and run from there). Full test skeleton in plan Task 2 Step 1.
- [x] Run test, verify it fails: bash scripts/test-dispatch-evidence.sh (no "## Evidence" in payload)
- [x] Implement: in hooks/dispatch-provider.sh add EVIDENCE="" to declarations, add parser case '--evidence-file) EVIDENCE="$(cat "$2")"; shift 2 ;;' next to --diff-file, and append '## Evidence' + content to the assembled payload exactly where the diff section is appended (follow that pattern verbatim, same variable).
- [x] Run tests, verify pass: bash scripts/test-dispatch-evidence.sh — Results: 2 passed, 0 failed
- [x] Commit

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


## Evaluation Log
BASE_SHA: 41ec758fb5f148e60f6e34ff8ea80f26f66f8fd7
total_dispatches: 8

### Iteration 1 (2026-07-29T09:08:36Z)
dispatches: 3
**infrastructure:**
- correctness: 7/10
- domain_spec_fidelity: 8/10
- drift_resistance: 6/10
- idempotency: 7/10
- security_posture: 7/10

### Iteration 2 (2026-07-29T09:14:26Z)
dispatches: 5
**infrastructure:**
- correctness: 7/10
- domain_spec_fidelity: 8/10
- drift_resistance: 6/10
- idempotency: 7/10
- security_posture: 7/10

## Summary of Changes

Added --evidence-file to hooks/dispatch-provider.sh (parser case, {EVIDENCE} substitution) with the ## Evidence section in skills/develop/provider-context.md between Diff and Previous Feedback, plus scripts/test-dispatch-evidence.sh (10 assertions, fake cat-back provider in an isolated temp project). Commit eeb16a5. Converged in 2 iterations, 5 dispatches, zero provider disagreements. Known inherited quirk noted by evaluation: bash 5.2+ patsub_replacement mangles ampersands in substituted content (affects pre-existing --diff-file equally); candidate for a later fix.
