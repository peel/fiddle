---
# fiddle-d2as
title: 'Fix: document /goal and /loop harness enforcement (spec section 8)'
status: completed
type: task
priority: normal
tags:
    - branch
    - remediation
created_at: 2026-07-29T12:10:24Z
updated_at: 2026-07-29T12:21:14Z
parent: fiddle-sip9
---

Remediation from holistic review iteration 1 (source: spec_coverage Missing, spec section 8).

## Context

Repo: /Users/peel/wrk/fiddle (work in worktree .worktrees/evidence-driven-develop-loop)
Spec docs/specs/2026-07-28-evidence-driven-develop-loop-design.md section 8 requires documenting, for the Claude harness: /goal as the manual equivalent of the Stop-hook verdict gate, and /loop as an optional outer watchdog. Neither is documented anywhere outside the spec. Natural home: a "Harness Enforcement (Claude Code)" section in skills/develop/SKILL.md near the Stop-hook material, with Codex/Pi fallback note per using-fiddle harness mapping.

## Files

- Modify: skills/develop/SKILL.md

## Steps

- [x] Add a "Harness Enforcement (Claude Code)" section to skills/develop/SKILL.md documenting: (a) the Stop-hook verdict gate ships in hooks/hooks.json (pointer to develop-loop marker lifecycle); (b) /goal as manual equivalent, condition phrased against recorded verdicts and INCLUDING the escalation exits (CONVERGED, or needs-attention via SPEC_DEFECT / BLOCKED / DISPATCHES_EXCEEDED) so the goal does not fight the dispatch budget; (c) /loop as optional outer watchdog re-firing fiddle:develop --epic <id>, idempotent via restart recovery, explicitly NOT a driver for the inner cycle (time-based, session-scoped); (d) Codex/Pi harnesses keep the skill-encoded loop via the using-fiddle mapping.
- [x] Verify: grep -n "goal\|/loop\|watchdog" skills/develop/SKILL.md shows the section; full sweep for t in scripts/test-*.sh clean
- [x] Commit

## Evaluation

```eval
domains: [general]
criteria:
  general:
    - id: goal-manual-equivalent-documented
      check: "Documentation states the /goal condition phrased against recorded verdicts and includes the escalation exits so it does not fight the dispatch budget"
    - id: loop-watchdog-documented
      check: "Documentation describes /loop as an optional outer watchdog re-firing fiddle:develop --epic, idempotent via restart recovery, not a driver for the inner cycle"
thresholds: {}
```


## Evaluation Log
BASE_SHA: 537e3ea1aea6b5a03121534e5a7f66d89deaed81
total_dispatches: 3

### Iteration 1 (2026-07-29T12:21:13Z)
dispatches: 3
**general:**

## Summary of Changes

Harness Enforcement section added to skills/develop/SKILL.md after Restart Resilience (commit a60d56e): Stop hook automatic/fail-open, /goal manual equivalent with verdict-phrased condition incl. escalation exits and budget rationale, /loop outer watchdog only, Codex/Pi skill-loop fallback. Converged single-pass evidence-only, 3 dispatches.
