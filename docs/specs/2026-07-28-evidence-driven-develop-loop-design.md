# Evidence-Driven Develop Loop — Design

Epic: fiddle-sip9. Date: 2026-07-28.

## Problem

Adversarial multi-provider code reviews in develop-loop rarely change outcomes within 1-2 runs. Judge-model errors correlate with generator errors ("if the model knew good code, it would write it" — Dex Horthy), so same-class adversarial review adds token cost without quality gain. Review iterations pay off only when they inject new evidence: test runs, runtime probes, invariant checks, plan-conformance diffs. Empirical trigger and research record live in the epic bean body.

## Evaluator Role

The evaluator's job narrows to interpreting gathered evidence: turning test results, invariant checks, and runtime observations into a scorecard against the bean's criteria. Pass/fail decisions belong to check-thresholds.sh and check-convergence.sh, not to the evaluator. The evaluator is not an open-ended quality judge. Maintainability and architecture judgment live upstream (DEFINE plan review, finish-branch human merge), not in the loop's scorecard.

## Design

### 1. Loop shape (skeleton unchanged, interior simplified)

Fresh implementer → gather evidence per domain (runtimes, tests, invariant checks) → one evaluator per domain → threshold/convergence verdict from the scripts → on FAIL, re-dispatch fresh implementer with failing evidence as guidance. Unchanged: dispatch budget (`max_dispatches_per_task`), eval-log protocol, hold-out criteria, and all escalation exits (SPEC_DEFECT via both paths, BLOCKED, NEEDS_CONTEXT, DISPATCHES_EXCEEDED).

### 2. One evaluator per domain per iteration

Replaces per-domain x per-provider dispatch. The bean eval block's `domains:` list still resolves via resolve-domains.sh to template, calibration, antipatterns, and runtime config; the domain keeps determining which evidence is gathered. Dispatch accounting: one implementer + one evaluator per domain per iteration.

### 3. Provider selection

`evaluators.domains.<d>.providers` is reinterpreted in place as an ordered preference list (no config migration, no new keys). Selection rule: the first available provider (per the session-start detection hook) that differs from the provider that ran the implementer. Fallback: the implementer's provider in a fresh context. Rationale: cross-provider protects against self-review leniency and gives a different failure trajectory; fresh context matters at least as much as a different provider, so the fallback loses little.

Because implementers are always Claude subagents, the rule reduces in practice to: prefer an external provider for evaluation whenever one is available. PASS_PENDING re-evaluation reuses the provider that produced the pass being confirmed, for comparability.

### 3a. Evidence pack

External providers run read-only (e.g. `codex exec -s read-only`) and cannot execute tests or probe runtimes. Evidence is therefore gathered before evaluator dispatch and materialized as an artifact: test output, invariant results, and runtime probe transcript per domain. Every evaluator, Claude or external, receives the evidence pack as files (`dispatch-provider.sh` gains `--evidence-file`). The evaluator interprets evidence; it does not gather it.

### 4. Merge simplification

Removed from the per-task path: provider merge, min-across-providers scoring, disagreement tracking. The spec-defect check runs directly on the single per-domain scorecard. Cross-domain union merge stays. merge-scorecards.sh is retained for holistic review and as a single-input format normalizer so scorecard shape stays uniform downstream.

### 5. Holistic review: unchanged

Multi-provider dispatch per `evaluators.holistic.providers`, coverage-matrix min-merge, remediation-bean union, own budget (`max_iterations`, default 3). Rationale: bounded cost per epic and requirement-grounded recall — a second reader catches missed requirements, which is a different mechanism than second opinions on code quality.

### 6. Convergence relaxation (evidence-first templates)

No new fields: the existing check-thresholds.sh split is the evidence/judgment split. Criteria (pass/fail, each backed by a cited evidence-pack artifact) are evidence; scored dimensions are judgment.

Domain templates are reworked so scored dimensions become optional per domain. A domain configured evidence-only converges on a single passing iteration; the two-consecutive-passes rule applies only to domains that keep judgment dimensions. check-convergence.sh distinguishes the cases from the verdict content (empty `dimensions` map means evidence-only). PASS_PENDING therefore only occurs for domains with judgment dimensions.

### 7. Plan cross-review (reinvesting the freed budget)

After write-plan produces the plan document, each define-phase external provider (from `providers.phases.define`) receives one critique dispatch with the plan and the design doc: coverage gaps, unverifiable steps, missing files, sizing problems. Claude folds accepted findings into the plan before bean creation. Single round, no debate, no scores. Seam: between write-plan's plan self-review and its "Create Beans from Plan" step; write-plan's existing `--epic` flag reuses the epic.

### 8. Harness enforcement (Claude Code; skill loop is the cross-harness fallback)

- **Stop hook** (shipped in hooks/hooks.json): reads the active bean's verdict state and blocks turn-end unless a terminal state is recorded (CONVERGED, or needs-attention via SPEC_DEFECT / BLOCKED / DISPATCHES_EXCEEDED). Deterministic; no judge model.
- **/goal**: documented manual equivalent; condition must be phrased against recorded verdicts and include the escalation exits, or it fights the dispatch budget.
- **/loop**: documented optional outer watchdog re-firing `fiddle:develop --epic <id>`; idempotent via restart recovery. Not a driver for the inner cycle (time-based, session-scoped).

Codex/Pi keep the skill-encoded loop via the using-fiddle harness mapping.

## Error Handling

- Preferred evaluator provider unavailable mid-run: fall through the preference list; last resort is the implementer's provider, fresh context. Record the substitution in the eval log entry.
- Evaluator returns an invalid scorecard: re-dispatch once; on second failure escalate the bean to needs-attention (existing behavior, unchanged).
- Runtime start failures: unchanged semantics (retry once on harness failure, include app/config errors in evaluator context).
- Stop hook cannot determine an active bean (no eval log in worktree): allow turn-end (fail-open) — the hook gates develop-loop turns only.

## Testing

- test-multi-provider.sh reworked: preference-order selection, differs-from-implementer rule, availability fallback.
- merge-scorecards tests: single-input normalization path; holistic multi-provider path unchanged.
- test-check-convergence.sh: evidence-only domains (empty dimensions map, single-pass), mixed domains (double-pass retained), regression detection unchanged.
- Evidence pack: assembly per domain, and dispatch-provider.sh --evidence-file passing.
- New test for the Stop-hook verdict gate (terminal vs non-terminal states, fail-open case).
- Portability check (check-portability.sh) still passes for skills referencing the new flow.

## Out of Scope (backlogged in docs/BACKLOG.md, 2026-07-28)

- PR-review feedback channel into calibration/antipattern memory (/iterate-style).
- Scheduled antipattern-eradication maintenance loop (scan, fix one, open PR, human merges).

## Decisions Log

- Holistic keeps multi-provider: user-confirmed 2026-07-28.
- Freed budget reinvested as single-round plan critique (not full panel, not challenge extension): user-confirmed 2026-07-28.
- `providers` arrays reinterpreted in place: user-confirmed 2026-07-28.
- Skip flags on orchestrate/define are user entry-point choices, not process shortcuts; nothing in this design changes them.
