# Convergence and recovery

Normalize and merge domain scorecards with [scorecard-merge.md](scorecard-merge.md). An evaluator-detected spec defect is logged immediately, recorded on the bean with its DEFINE re-entry pointer, marked `needs-attention`, and never re-dispatched.

When `evaluators.attended` is true, follow [attended-gate.md](attended-gate.md); otherwise proceed to thresholds. Run `scripts/check-thresholds.sh` and `scripts/check-convergence.sh` rather than deciding by inspection. The only convergence verdicts are:

- `CONVERGED`: two consecutive passes without regressions; mark completed and clear the active marker.
- `FAIL`: re-dispatch the implementer with failing domains and guidance.
- `PASS_PENDING`: re-evaluate without re-implementing and reuse the recorded provider.
- `PASS_REGRESSED`: re-dispatch the implementer with regression details.
- `DISPATCHES_EXCEEDED`: mark `needs-attention`, clear the active marker, and escalate.

Log every evaluator cycle with `scripts/append-eval-log.sh`, including actual dispatch count, selected provider/fallback reason, attended corrections, and detected antipatterns. Per-task evaluation has no disagreements file because it uses one evaluator per domain. Keep hold-out results and guidance out of any implementer re-dispatch. The Evaluation Log is the only restart-surviving state.
