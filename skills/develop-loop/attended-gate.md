# Attended Scorecard Gate

<HARD-GATE>
IF evaluators.attended is true in orchestrate.json:
  After merging cross-domain scorecards, before threshold checks, you MUST present the merged scorecard to the human for review.

  1. Show the full merged scorecard with ALL dimension scores across ALL domains.
  2. Highlight any dimension scoring BELOW its threshold (show score and threshold).
  3. Highlight any provider disagreements from disagreements.json (show dimension, provider scores, spread).
  4. Ask: "Do you agree with these scores? Correct any you disagree with, or confirm to proceed."

  If the human corrects a score:
    a. Record the correction: {domain, dimension, evaluator_score, human_score, reason}
    b. Update the merged scorecard (scorecard.json) with the human's corrected score for that dimension.
    c. Encode the correction as a calibration anchor in the project's calibration file (see below).
    d. Use the corrected scorecard for ALL subsequent threshold and convergence checks.

  If the human confirms: proceed with evaluator scores unchanged.

Do NOT skip the attended gate when evaluators.attended is true.
Do NOT proceed to threshold checks without human confirmation when attended mode is active.
</HARD-GATE>

## Calibration Anchor Encoding

When the human corrects a score during attended review, append a calibration anchor to the project's calibration file for that domain.

**Locate the calibration file:** Read `evaluators.domains.<domain>.calibration` from `orchestrate.json`. If the key is present, use that path. If absent, default to `docs/evaluator-calibration-<domain>.md`. Create the file if it does not exist.

**Append the anchor in this format:**

```markdown
## [dimension] — Correction (YYYY-MM-DD)
**Evaluator scored:** X/10 — "[evaluator evidence from scorecard]"
**Human corrected to:** Y/10 — "[human's stated reason]"
**Anchor:** For this project, score Y means: [human's description of what that score level looks like]
```

Ask the human for their reason and description when they correct a score. The anchor becomes part of the evaluator's context on future dispatches (loaded at position 3 in the context loading order — see `skills/develop-loop/context-loading-order.md`).

When `evaluators.attended` is false, skip the attended gate entirely — proceed directly to threshold checks.
