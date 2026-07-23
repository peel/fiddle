# 006 — Keep code_quality threshold at 6, control drift with the decay alarm

**Date:** 2026-07-23
**Status:** accepted

## Context

Evaluator thresholds act as equilibria, not floors: an unattended loop converges
to the lowest passing score, so a codebase trends toward uniform threshold-level
quality over time. code_quality has the lowest bar (6). The obvious fix is to
raise it to 7 for long-lived code, but raising the threshold only moves the
equilibrium point without removing the dynamic. The actual failure mode is
quality drift across epics, which is now observable: `scripts/trend-eval-history.sh`
aggregates per-dimension mean scores across epics and DELIVER step 5f raises an
`alarm` when a dimension declines across the two most recent epics.

## Decision

Keep the per-task code_quality threshold at 6. Do not raise it to 7. Control
long-term quality drift with the longitudinal decay alarm (DELIVER 5f over
`scripts/trend-eval-history.sh`) instead of a higher per-task bar.

Rationale: raising the threshold to 7 reproduces the same equilibrium dynamic at
a higher number and forces every small config-tweak task to clear a "Good" bar
designed for long-lived code. The decay alarm addresses drift over time directly,
where the failure actually lives, and folds affected dimensions into the
calibration updates from DELIVER 5b.

## Consequences

- Small and short-lived tasks are not forced over a bar meant for long-lived code.
- Quality regression is caught at the epic boundary, not per task; a single task
  can still land at exactly 6. The alarm is the backstop, so DELIVER 5f must be
  run each epic for this control to function.
- The policy depends on eval-log history existing; with fewer than two epics of
  data `trends` is null and no drift signal is available yet.
- If the decay alarm proves insufficient in practice, revisit raising the
  threshold as a follow-up decision that supersedes this one.
