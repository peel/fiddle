# Evaluator evolution

Run this after documentation and product artifacts are confirmed. It calibrates future evaluations from the current epic while keeping human judgment unanchored by evaluator output.

## 1. Blind spot-check

Follow [blind-spot-check.md](blind-spot-check.md) before revealing evaluator scorecards. Read `evaluators.spot_check.rate`: sample every Nth converged bean; absent means 5 and zero or less disables the check. Record per-dimension human/evaluator divergences in the bean Evaluation Log with `append-eval-log.sh --corrections`, then carry their summary into the final report.

## 2. Review scorecards and wait for corrections

Collect all scorecards from the epic's evaluation logs. Present each dimension, score, and evidence; ask where the evaluator was wrong. Wait for the user's corrections before modifying calibration, thresholds, or antipatterns.

## 3. Apply confirmed calibration and antipattern changes

For each score correction, append this block to the domain's configured calibration file, defaulting to `docs/evaluator-calibration-<domain>.md`, and set the corresponding config path if missing:

```markdown
## [dimension] — Correction (YYYY-MM-DD)
**Evaluator scored:** X/10 — "[evaluator evidence]"
**Human corrected to:** Y/10 — "[human reason]"
**Anchor:** For this project, score Y means: [human's description]
```

For each real post-delivery failure, append this block to the domain's antipattern file, defaulting to `docs/antipatterns-<domain>.md`, and set the corresponding config path if missing:

```markdown
## [antipattern-id] (YYYY-MM-DD)
**Pattern:** What the failure looks like
**Example:** Concrete code/behavior from this run
**Fix:** How to avoid it
```

Create a file with its matching top-level heading when necessary. Change thresholds only after user confirmation when several tasks show systematic strictness or leniency.

## 4. Review convergence and cross-epic trends

Flag tasks with more than five develop-evaluate cycles and target their repeat dimensions for calibration. Run `scripts/trend-eval-history.sh --beans-path <beans-path>` from a worktree, or omit the flag at the repository root. Present its reported values rather than reconstructing them:

```text
Longitudinal decay trends (oldest → newest epic):
- Dispatches-to-convergence: [from → to] ([direction])
- Iterations: [from → to] ([direction])
- Per-dimension scores: [dimension: from → to (direction), ...]
- Provider disagreements: [from → to] ([direction])
- Re-evaluations of an unchanged tree: [from → to] ([direction])

Decay alarm: [RAISED — <alarm_reasons> | none]
```

If the alarm is raised, fold affected dimensions into calibration work. If `trends` is null, report insufficient history.

Report `unchanged_tree_reevaluations` even when the alarm is quiet. It counts dispatches that re-judged a tree the previous dispatch had already judged, which is what a bean re-rolled until two evaluators agreed looks like from outside; the reader least likely to volunteer it is the one who did the re-rolling. A rising count is a calibration problem, not an implementation one — the work was not changing.

## 5. Age calibration anchors and antipatterns

Read `evaluators.aging.window_days` (default 90) and `evaluators.aging.quiet_epics` (default 3). Scan only content above `## Retired`.

- Flag calibration anchors older than the window. If their dimension has no human corrections in the last quiet epics, retire it; otherwise retain it for re-anchoring.
- For each domain's antipattern file, collapse any antipattern not detected in the last quiet epics.
- Never delete retired content. Move its complete block under `## Retired` in the same file and append `**Retired YYYY-MM-DD:** [reason]`.
- Do not load retired material into evaluator context; calibration loading and antipattern injection stop at `## Retired`.

Present the proposed aging changes and wait for confirmation before moving anything.

## 6. Close confirmation

Present the blind spot-check count and divergences, calibration anchors, antipatterns, threshold changes, high-iteration tasks, decay alarm, and aging actions. Wait for explicit confirmation before closing the epic.
