---
name: fiddle:develop-loop
description: Use when a single task bean needs implementation and evaluation — called by fiddle:develop, not directly
argument-hint: --bean <id> --epic <id>
---

# Develop Loop — Single Bean Evaluation

Implement and evaluate a single task bean through the full evaluation chain: dispatch implementer, dispatch evaluators, merge scorecards, check convergence. Repeat until converged or budget exceeded.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--bean <id>` | **required** | The task bean to implement and evaluate |
| `--epic <id>` | **required** | The parent epic (for context and config) |

Read `orchestrate.json` from project root. Extract the `evaluators` block:

```json
{
  "evaluators": {
    "attended": false,
    "max_dispatches_per_task": 60,
    "domains": {
      "general": { "template": "evaluator-general", "providers": ["claude"] }
    }
  }
}
```

Store `max_dispatches_per_task` for the convergence budget. Store `domains` for evaluator dispatch.

## Iron Laws

Read and internalize: `skills/develop/iron-laws.md`

## Rationalization Prevention

| Rationalization | Reality |
|---|---|
| "Implementer said DONE, skip evaluation" | DONE is a claim. Evaluation is evidence. |
| "General domain only, lightweight eval" | General domain gets the full chain. No shortcuts. |
| "Simple task, one iteration enough" | Convergence requires two consecutive passes. Run the scripts. |
| "Runtime not configured, skip runtime start" | No runtime ≠ no evaluation. General domain still applies. |
| "Scorecard looks good, skip merge scripts" | You cannot eyeball conservative min scoring. Run merge-scorecards.sh. |
| "Budget is high, no need to track dispatches" | Budget exists to prevent infinite loops. Track every dispatch. |

## 1a. Restart Check

If bean is `in-progress`, follow: `skills/develop-loop/restart-recovery.md`

## 1b. Initialize Evaluation Log

For a fresh task bean (not a restart), record the starting point:

```bash
BASE_SHA=$(git rev-parse HEAD)
scripts/append-eval-log.sh --bean-id {id} --init --base-sha "$BASE_SHA"
beans update {id} --status in-progress
```

Set `dispatch_count=0` and `iteration=0`.

## 1c. Resolve Domains

<HARD-GATE>
Before dispatching any evaluator, you MUST run:
  scripts/resolve-domains.sh --domains "{task domains}" --config orchestrate.json
Use the script's output to configure evaluators. Do NOT resolve domains manually.
</HARD-GATE>

Read the task bean's eval block to extract `domains` (e.g., `domains: [frontend, backend]`). If no `domains` are specified in the eval block, default to `"general"`. **Defaulting to general does not reduce evaluation requirements. The full evaluation chain applies.**

Run:
```bash
scripts/resolve-domains.sh --domains "frontend,backend" --config orchestrate.json > resolved-domains.json
```

The script outputs a JSON array of resolved domain objects:
```json
[
  {"domain": "frontend", "template": "evaluator-frontend", "runtime": ["flutter run ..."], "ready_check": {...}, "resolved_via": "config"},
  {"domain": "backend", "template": "evaluator-backend", "runtime": ["mix phx.server"], "ready_check": {...}, "resolved_via": "config"},
  {"domain": "general", "template": "evaluator-general", "resolved_via": "fallback"}
]
```

Store the resolved domains list for use in step 1f (per-domain evaluator dispatch).

Also extract `runtime_order` from the task's eval block if present (e.g., `runtime_order: [backend, frontend]`). If not specified, default to the order listed in the `domains` array.

## 1d. Dispatch Implementer

Dispatch a subagent using the template at `skills/develop/implementer-prompt.md`. Fill placeholders:
- `{ITERATION}` — iteration number (1 on first dispatch)
- `{TASK_TEXT}` — full bean body (title, description, acceptance criteria)
- `{CONTEXT}` — relevant file paths, architecture notes, codebase context
- `{EVAL_BLOCK}` — the task's Evaluation block criteria, EXCLUDING any criterion marked `holdout: true` (see Hold-Out Criteria below)
- `{ANTIPATTERNS}` — known antipatterns to avoid (see antipattern loading below; empty if none configured)
- `{PRIOR_SCORECARD}` — previous evaluator scorecard (empty on first dispatch), with hold-out criterion results removed (see Hold-Out Criteria below)
- `{PRIOR_GUIDANCE}` — specific fix instructions from evaluator (empty on first dispatch), with any guidance derived from hold-out criteria removed
- `{WORK_DIR}` — worktree directory path

### Hold-Out Criteria

A task's eval block may mark a criterion `holdout: true`. Hold-out criteria are
evaluator-only: they are scored like any other criterion but are NEVER shown to
the implementer. When filling `{EVAL_BLOCK}`, exclude every criterion with
`holdout: true`. When filling `{PRIOR_SCORECARD}` and `{PRIOR_GUIDANCE}` for a
re-dispatch (from step 1m FAIL / PASS_REGRESSED), also strip the `criteria[]`
entries for hold-out IDs and any guidance sentence derived from them.

Why: convergence must come from the implementer generalizing to the spec, not
from rubric-matching a criterion it can read. A criterion held out of the prompt
can only be satisfied by getting the underlying quality right.

Default: no criteria are held out unless a bean's eval block marks them.

### Antipattern Loading

For each resolved domain from step 1c, check `evaluators.domains.<domain>.antipatterns` in `orchestrate.json`. If the key exists, read the file at that path (relative to project root), stopping at the `## Retired` heading if present (collapsed antipatterns from deliver 5g are audit-only and must not be injected). Concatenate antipattern content from all resolved domains into a single block and inject into the `{ANTIPATTERNS}` placeholder. If no domain has an `antipatterns` key configured, leave `{ANTIPATTERNS}` empty (the section header still appears but with no content).

Increment `dispatch_count` after each dispatch.

## 1e. Handle Implementer Status

The implementer returns one of:
- **DONE** or **DONE_WITH_CONCERNS** → proceed to evaluation (step 1f)
- **BLOCKED** → mark bean `needs-attention` with reason, escalate to human, move to next bean
- **NEEDS_CONTEXT** → provide the requested context and re-dispatch (back to step 1d)
- **SPEC_DEFECT** → mark bean `needs-attention`, escalate to human, move to next bean. Do NOT re-dispatch — re-implementing a defective spec cannot converge.
  - **Record on the bean body:** WHAT about the spec is defective (the implementer's evidence), a `fiddle:define` re-entry pointer, and a line noting the SPEC_DEFECT exit, the current iteration number, and that one implementer dispatch occurred.
  - **Budget:** a spec defect is DEFINE's failure, not the implementer's budget, so decrement `dispatch_count` by 1 (undoing the increment from step 1d) so this dispatch does not count against `max_dispatches_per_task`.
  - **No eval-log entry:** this path short-circuits before any evaluator runs, so no scorecard exists and `append-eval-log.sh` (which hard-requires a `--scorecard` file) cannot run. The bean-body note above is the record of the dispatch; do not call the eval-log script for this path.

<GATE>Proceed to evaluator dispatch (1f). Implementer DONE is not evaluation.</GATE>

## 1f. Dispatch Per-Domain, Per-Provider Evaluators

For EACH resolved domain (from `resolved-domains.json` produced in step 1c), dispatch evaluators for EACH provider. Process domains in `runtime_order` if specified, otherwise in the order listed in the `domains` array.

### Runtime Start (per domain)

<HARD-GATE>
If a domain has runtime configured (i.e., the resolved domain entry has a "runtime" array):
  1. Run: scripts/start-runtimes.sh --domains <resolved-domains-file>
     Start runtimes in runtime_order sequence. If runtime_order is [backend, frontend], start backend first, wait for ready, then start frontend.
  2. If exit 0: runtime is ready. Proceed to evaluator dispatch.
  3. If exit 3 (harness failure): retry once. If retry fails, escalate to human without counting against dispatch budget.
  4. If exit 1 or 2: this is an app/config issue. Include the error in evaluator context.
Do NOT skip runtime start. Do NOT proceed to evaluator dispatch if runtime is required but not started.
</HARD-GATE>

### Per-Domain, Per-Provider Evaluator Dispatch

For each resolved domain, read the `providers` array from the resolved domain config (e.g., `{"domain": "general", "providers": ["claude", "codex"], ...}`). If no `providers` array is present, default to `["claude"]`.

For each provider in the domain's `providers` array, dispatch an evaluator:

**If provider is `claude`:** Dispatch an evaluator subagent using `skills/evaluate/SKILL.md` protocol with that domain's template (e.g., `skills/evaluate/evaluator-general.md`, `evaluator-frontend.md`, or `evaluator-backend.md` — as specified in the resolved domain's `template` field).

**If provider is external (not `claude`):** Dispatch via the provider hook:

```bash
hooks/dispatch-provider.sh <provider> \
  --role evaluator \
  --topic "Evaluate domain: {domain}" \
  --instructions "$(cat skills/evaluate/{template}.md)" \
  --diff-file <diff-file> \
  --design-doc-file <design-doc-file>
```

The external provider receives the same context as the claude evaluator: evaluation protocol, domain template, calibration data, diff, and criteria. The external provider MUST return a valid JSON scorecard as its last content block (see `skills/develop/provider-context.md` for schema requirements).

**Each provider dispatch counts as 1 toward the dispatch budget**, regardless of provider type. For example, 2 providers x 2 domains = 4 dispatches per iteration.

Load evaluator context in order specified by: `skills/develop-loop/context-loading-order.md`

Each evaluator (regardless of provider) returns a single scorecard JSON containing both per-dimension scores (under `.domains`) and pass/fail criteria (under `.criteria`). The scorecard MUST include a `"provider"` field identifying which provider produced it. Save each provider's scorecard per domain separately:

```bash
# Save per-provider, per-domain scorecard
cat > scorecard-{domain}-{provider}.json   # ← evaluator output for this domain+provider

# Increment dispatch_count for EACH provider dispatch
dispatch_count=$((dispatch_count + 1))
```

### Runtime Stop (after all domains evaluated)

<HARD-GATE>
After ALL domain evaluators (all providers, all domains) have completed:
  Run: scripts/stop-runtimes.sh --state <runtime-state-file>
Do NOT leave processes running after evaluation.
</HARD-GATE>

## 1g–1h. Merge Scorecards

Merge provider and cross-domain scorecards following: `skills/develop-loop/scorecard-merge.md`

The scorecard-merge protocol includes a pre-merge Spec-Defect Check. Once the merge (1g/1h) completes, the result of that check is known.

<HARD-GATE>
If the pre-merge Spec-Defect Check detected `spec_defect.detected == true` on ANY per-provider scorecard, this bean takes the spec-defect exit and does NOT flow through the normal threshold/convergence path:
  1. Run the eval-log step (1l) NOW with the merged scorecard, so `append-eval-log.sh` records the iteration (its `--scorecard` requirement is satisfied by the merged scorecard).
  2. Route the bean to `needs-attention` per the scorecard-merge Spec-Defect Check (mark `needs-attention`, record the defect reason and `fiddle:define` re-entry pointer, escalate to human, do NOT re-dispatch).
  3. SKIP 1i (attended gate), 1j (thresholds), 1k (convergence), and 1m entirely for this bean.
  4. Return to the orchestrator for the next bean.
</HARD-GATE>

<GATE>If no spec defect was detected, proceed to threshold checks (1j). If a spec defect was detected, take the spec-defect exit above instead. Do not skip to next task.</GATE>

## 1i. Attended Scorecard Gate

If `evaluators.attended` is true in orchestrate.json, follow: `skills/develop-loop/attended-gate.md`

When `evaluators.attended` is false, skip directly to threshold checks (step 1j).

## 1j. Check Thresholds

<HARD-GATE>
After the attended gate (step 1i) or directly after scorecard merge (when unattended), you MUST run:
  scripts/check-thresholds.sh --scorecard {scorecard_file} --criteria {criteria_file}
  scripts/check-convergence.sh --current {verdict_file} --history {history_file} --max-dispatches N --current-dispatches M
Act on the scripts' verdicts. Do NOT compute thresholds or convergence yourself.
</HARD-GATE>

Run `check-thresholds.sh` with the merged scorecard (from step 1h, potentially corrected in step 1i). It produces a verdict: `PASS` (exit 0) or `FAIL` (exit 1). The output includes a `dimensions` flat map (`{"frontend.correctness": 8, "backend.api_quality": 7, ...}`) with all per-domain dimension scores. On `FAIL`, the output identifies which domain(s) did not meet thresholds. This output can be passed directly to `check-convergence.sh` as the `--current` file and appended to the history array for future convergence checks.

## 1k. Check Convergence

Run `check-convergence.sh` with the `--current` file (the check-thresholds.sh output, which includes the `dimensions` flat map), the `--history` file (a JSON array of prior check-thresholds.sh outputs), and dispatch budget.

Possible outcomes:
- **CONVERGED** (exit 0) — two consecutive passes with no regressions
- **FAIL** (exit 1, status FAIL) — thresholds not met
- **PASS_PENDING** (exit 1, status PASS_PENDING) — passed but only once, need consecutive pass
- **PASS_REGRESSED** (exit 1, status PASS_REGRESSED) — passed but regressed on previously-passing dimensions
- **DISPATCHES_EXCEEDED** (exit 2) — budget exhausted

<HARD-GATE>
If check-convergence.sh returns DISPATCHES_EXCEEDED (exit 2), you MUST stop and ask the human.
Do NOT continue iterating. Do NOT lower thresholds. Do NOT rationalize.
</HARD-GATE>

## 1l. Log Evaluation

<HARD-GATE>
After every evaluation cycle, you MUST run:
  scripts/append-eval-log.sh --bean-id {id} --iteration {N} --scorecard {scorecard_file} --dispatches {count} --guidance {text} --disagreements disagreements.json --antipatterns antipatterns.json
The --dispatches {count} MUST reflect actual provider dispatches (each provider dispatch = 1), not just iterations.
For example, 2 providers x 2 domains = 4 dispatches per iteration.
The --disagreements parameter is optional. Pass the combined disagreements file from step 1g. If the file contains a non-empty array, disagreement details are appended to the iteration entry.
The --antipatterns parameter is optional. Write the merged scorecard's `.antipatterns_detected` array to `antipatterns.json` (`jq -c '.antipatterns_detected // []' {scorecard_file} > antipatterns.json`) and pass it. If the file contains a non-empty array, an **Antipatterns detected:** section is appended to the iteration entry, giving deliver 5g the durable per-epic detection record it ages against.
If the attended gate (step 1i) produced human corrections, include them in the log entry by passing --corrections {corrections_json} with the array of {domain, dimension, evaluator_score, human_score, reason} objects.
Do NOT skip logging. Do NOT write the log entry manually.
</HARD-GATE>

## 1m. Act on Convergence Result

| Result | Action |
|---|---|
| **CONVERGED** | Mark bean `completed`. Return to orchestrator. |
| **FAIL** | Dispatch fresh implementer with scorecard feedback showing failing dimensions (including which domain(s) failed) and fix guidance. → Back to 1d. |
| **PASS_PENDING** | Re-evaluate without re-implementing — scorecard may stabilize. → Back to 1f. |
| **PASS_REGRESSED** | Dispatch fresh implementer with regression details (which dimensions in which domains regressed and by how much). → Back to 1d. |
| **DISPATCHES_EXCEEDED** | Mark bean `needs-attention`. Escalate to human. Return to orchestrator. |

> FAIL and PASS_REGRESSED re-dispatch through step 1d, so their `{PRIOR_SCORECARD}` and `{PRIOR_GUIDANCE}` feedback MUST omit hold-out criterion results and any guidance derived from them (see Hold-Out Criteria in step 1d).

> SPEC_DEFECT never reaches this table: the implementer-reported path exits at step 1e, and the evaluator-flagged path exits via the spec-defect HARD-GATE at the end of 1g–1h — it runs 1l logging with the merged scorecard, then routes to `needs-attention` and skips 1i/1j/1k/1m. Both route the bean to `needs-attention` before convergence is checked.

## Red Flags

- **Never** dispatch an implementer without the full bean body and acceptance criteria
- **Never** dispatch without injecting relevant codebase context
- **Never** discard human score corrections — corrected scores must update the scorecard AND encode calibration anchors
- **Never** skip calibration file loading when `evaluators.domains.<domain>.calibration` is configured OR when the default path `docs/evaluator-calibration-<domain>.md` exists — the calibration file must be loaded at position 3 in the context loading order, immediately after the domain template
