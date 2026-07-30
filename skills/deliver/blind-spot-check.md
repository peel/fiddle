# Blind Spot-Check

Sampled blind review of converged beans. The human reads the raw diff cold — with no evaluator scorecard in view — scores the dimensions, and only then compares against the evaluator's scorecard. Divergence between the two is the measured drift between what evaluators score and what a human would flag. Because the human is not anchored on the evaluator's own evidence, this catches blind spots the attended gate and 5a review inherit.

## Sampling

Config: see `skills/orchestrate/SKILL.md` for the schema. This reference reads `evaluators.spot_check.rate` and `evaluators.domains.<domain>.calibration`.

`spot_check.rate` is an integer N — review every Nth converged bean. If the key is absent, default to `5`. If the value is `0` or less, the spot-check is disabled; return immediately.

1. Enumerate the epic's converged task beans (status `completed`, with an Evaluation Log) in stable order (bean creation order / ascending id).
2. Select every Nth bean (beans at index N, 2N, 3N, … counting from 1). If fewer than N beans converged, select none for this run.
3. For each selected bean, run the blind review below.

## Blind Review

For each sampled bean, the human's scores are recorded before any part of the evaluator scorecard is revealed. This ordering is the whole measurement: an anchored human measures nothing.

1. Show the human the raw diff for the bean only:
   ```bash
   git diff {BASE_SHA}...HEAD   # BASE_SHA from the bean's Evaluation Log
   ```
   Showing, summarizing, paraphrasing, or hinting at the evaluator's scores, evidence, guidance, or verdict all count as revealing it.
2. Present each scoring dimension for the bean's domain(s) with its threshold, and ask the human to score each 1–10 and give a one-line reason.
3. Record the human's scores.
4. Once the human has committed their scores, reveal the evaluator scorecard for the same bean.

The human scores the diff cold; do not score the dimensions yourself, or the run measures the evaluator against another evaluator.

## Divergence

For each dimension, compute the divergence: `human_score - evaluator_score`. A dimension diverges when the human and evaluator scores differ (treat any non-zero difference as a divergence; note especially crossings of the dimension threshold).

Build a divergences JSON array, one object per diverging dimension, using the same shape the attended gate uses for corrections:

```json
[
  {"domain": "general", "dimension": "correctness", "evaluator_score": 8, "human_score": 5, "reason": "blind spot-check: missed unhandled error path in diff"}
]
```

### Record in the eval log

Append the divergences to the sampled bean's Evaluation Log via `append-eval-log.sh` in `--spot-check` mode, reusing the existing `--corrections` mechanism:

```bash
scripts/append-eval-log.sh --bean-id {bean-id} \
  --spot-check \
  --scorecard {bean's final merged scorecard file} \
  --guidance "blind spot-check" \
  --corrections divergences.json
```

`--spot-check` writes the entry under a `### Spot-Check ({timestamp})` heading instead of a `### Iteration N` heading, so the post-convergence review does not inflate the bean's iteration count or overwrite its final-iteration dimension scores. Dispatches default to 0, recording the spot-check without inflating the dispatch budget. The divergences appear under a **Human Corrections** section of the spot-check entry.

## Calibration Anchors

Encode each divergence as a calibration anchor in the project's calibration file, using the exact format and file-location rules defined in `skills/develop-loop/attended-gate.md` (section "Calibration Anchor Encoding"). Do not invent a separate format — divergences feed the same anchors as attended-gate corrections so the evaluator receives them identically on future dispatches.

For each diverging dimension:
- **Locate the calibration file:** read `evaluators.domains.<domain>.calibration` from `orchestrate.json`; if absent, default to `docs/evaluator-calibration-<domain>.md`. Create the file if it does not exist.
- Append the anchor:

```markdown
## [dimension] — Correction (YYYY-MM-DD)
**Evaluator scored:** X/10 — "[evaluator evidence from scorecard]"
**Human corrected to:** Y/10 — "[human's blind reason]"
**Anchor:** For this project, score Y means: [human's description of what that score level looks like]
```

Ask the human for their reason and description as they score. After writing anchors, ensure `orchestrate.json` has `evaluators.domains.<domain>.calibration` set to the file path so the anchors reach evaluators on future runs.

Report the divergence summary back to Step 5: per sampled bean, which dimensions diverged and by how much. Track the divergence rate over time — a rising rate signals evaluator drift from human judgment.

Every divergence lands in both the eval log and the calibration file, not only in the summary — the summary is read once, while the anchors are what reach future evaluators.
