# Attended Scorecard Gate

When `evaluators.attended` is true in orchestrate.json, the human reviews the merged cross-domain scorecard after the merge and before threshold checks. The ordering matters: once a threshold verdict exists, the review becomes a rubber stamp on a decision already made.

1. Show the full merged scorecard with all dimension scores across all domains.
2. Highlight any dimension scoring below its threshold (show score and threshold).
3. Highlight any provider disagreements from disagreements.json (show dimension, provider scores, spread).
4. Ask: "Do you agree with these scores? Correct any you disagree with, or confirm to proceed."

If the human corrects a score:

a. Record the correction: `{domain, dimension, evaluator_score, human_score, reason}`
b. Update the merged scorecard (scorecard.json) with the corrected score for that dimension.
c. Encode the correction as a calibration anchor in the project's calibration file (see below).
d. Use the corrected scorecard for all subsequent threshold and convergence checks.

A correction that is not written into both the scorecard and the calibration file is lost: the current verdict ignores it and the next evaluator repeats the same misjudgment.

If the human confirms: proceed with evaluator scores unchanged.

## Calibration Anchor Encoding

Read `evaluators.domains.<domain>.calibration` from `orchestrate.json` for the calibration file path, defaulting to `docs/evaluator-calibration-<domain>.md`. Create the file if it does not exist, and append the anchor in this format:

```markdown
## [dimension] — Correction (YYYY-MM-DD)
**Evaluator scored:** X/10 — "[evaluator evidence from scorecard]"
**Human corrected to:** Y/10 — "[human's stated reason]"
**Anchor:** For this project, score Y means: [human's description of what that score level looks like]
```

Ask the human for their reason and description when they correct a score. The anchor becomes part of the evaluator's context on future dispatches (loaded at position 3 in the context loading order — see `skills/develop-loop/context-loading-order.md`).

When `evaluators.attended` is false, skip the attended gate entirely — proceed directly to threshold checks.
