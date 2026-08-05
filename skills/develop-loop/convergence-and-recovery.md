# Convergence and recovery

## 1g–1h. Merge Scorecards

Normalize each domain's scorecard and merge across domains following: `skills/develop-loop/scorecard-merge.md`

That protocol runs a pre-merge Spec-Defect Check, so its result is known once the merge completes. If any domain's evaluator flagged `spec_defect.detected == true`, the bean takes the spec-defect exit rather than the threshold path:

1. Run 1l now with the merged scorecard, so `append-eval-log.sh` records the iteration (the merged scorecard satisfies its `--scorecard` requirement).
2. Route to `needs-attention` per the scorecard-merge Spec-Defect Check: record the defect reason and `fiddle:define` re-entry pointer, escalate to human, do not re-dispatch.
3. `rm -f .fiddle/active-bean`
4. Skip 1i, 1j, 1k, and 1m for this bean.
5. Return to the orchestrator for the next bean.

Otherwise continue to 1i.

## 1i. Attended Scorecard Gate

If `evaluators.attended` is true in orchestrate.json, follow: `skills/develop-loop/attended-gate.md`. When it is false, go straight to 1j.

## 1j–1k. Check Thresholds and Convergence

Run both scripts and act on their verdicts rather than judging thresholds or convergence yourself:

```bash
scripts/check-thresholds.sh --scorecard {scorecard_file} --criteria {criteria_file}
scripts/check-convergence.sh --current {verdict_file} --history {history_file} --max-dispatches N --current-dispatches M
```

`check-thresholds.sh` takes the merged scorecard (from 1h, as corrected in 1i) and returns `PASS` (exit 0) or `FAIL` (exit 1), naming the failing domain(s) and including a `dimensions` flat map (`{"frontend.correctness": 8, ...}`). Pass that output to `check-convergence.sh` as `--current`, and append it to the `--history` array for later checks. `check-convergence.sh` returns:

- **CONVERGED** (exit 0) — two consecutive passes with no regressions
- **FAIL** (exit 1) — thresholds not met
- **PASS_PENDING** (exit 1) — passed once, needs a consecutive pass
- **PASS_REGRESSED** (exit 1) — passed but regressed on previously-passing dimensions
- **DISPATCHES_EXCEEDED** (exit 2) — budget exhausted

On DISPATCHES_EXCEEDED, stop and ask the human. The budget is the only protection against iterating forever on a bean that is not converging, so spending past it — or lowering thresholds to fit — hides exactly the problem it just surfaced.

## 1l. Log Evaluation

After every evaluation cycle:

```bash
scripts/append-eval-log.sh --bean-id {id} --iteration {N} --scorecard {scorecard_file} --dispatches {count} --guidance {text} --antipatterns antipatterns.json
```

- `--dispatches` counts actual dispatches, not iterations.
- No `--disagreements` on the per-task path: one evaluator per domain produces no disagreements file, and that tracking is holistic-only.
- Record the provider and reason from selected-provider.json, which is what captures fallback substitutions.
- `--antipatterns` is optional: `jq -c '.antipatterns_detected // []' {scorecard_file} > antipatterns.json`. A non-empty array appends an **Antipatterns detected:** section, the durable per-epic record deliver 5g ages against.
- Pass `--corrections {corrections_json}` (array of `{domain, dimension, evaluator_score, human_score, reason}`) when the attended gate produced corrections.

The log is the loop's only state that survives a restart, so let the script write it.

## 1m. Act on Convergence Result

| Result | Action |
|---|---|
| **CONVERGED** | Mark bean `completed`. `rm -f .fiddle/active-bean`. Return to orchestrator. |
| **FAIL** | Re-dispatch implementer with the failing dimensions, their domains, and fix guidance. → 1d |
| **PASS_PENDING** | Re-evaluate without re-implementing; reuse the provider in selected-provider.json. → 1e-2 |
| **PASS_REGRESSED** | Re-dispatch implementer with regression details (which dimensions in which domains, by how much). → 1d |
| **DISPATCHES_EXCEEDED** | Mark bean `needs-attention`. `rm -f .fiddle/active-bean`. Escalate to human. Return to orchestrator. |

FAIL and PASS_REGRESSED go back through 1d, so their feedback omits hold-out criterion results and hold-out-derived guidance.

SPEC_DEFECT never reaches this table: the implementer-reported path exits at 1e and the evaluator-flagged path exits at the end of 1g–1h, both routing the bean to `needs-attention` before convergence is checked.
