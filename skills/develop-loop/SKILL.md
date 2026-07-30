---
name: develop-loop
description: Use when a single task bean needs implementation and evaluation — called by fiddle:develop, not directly
---

# Develop Loop — Single Bean Evaluation


## Usage

Invoke as `fiddle:develop-loop --bean <id> --epic <id>`.

Implement and evaluate a single task bean through the full evaluation chain: dispatch implementer, gather evidence pack per domain, dispatch one evaluator per domain, merge scorecards, check convergence. Repeat until converged or budget exceeded.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`: `--bean <id>` (required, the bean to implement and evaluate) and `--epic <id>` (required, the parent epic for context and config).

Read `orchestrate.json` from project root and extract the `evaluators` block:

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

`max_dispatches_per_task` is the convergence budget; `domains` drives evaluator dispatch.

Every bean runs the whole chain whatever its domain, size, or apparent simplicity: the chain is what turns an implementer's claim into evidence, so a shortened chain returns no signal.

## 1a. Restart Check

If the bean is `in-progress`, follow: `skills/develop-loop/restart-recovery.md`

## 1b. Initialize Evaluation Log

For a fresh bean (not a restart), record the starting point:

```bash
BASE_SHA=$(git rev-parse HEAD)
scripts/append-eval-log.sh --bean-id {id} --init --base-sha "$BASE_SHA"
beans update {id} --status in-progress
mkdir -p .fiddle && echo "{id}" > .fiddle/active-bean
```

The `.fiddle/active-bean` marker arms the Stop-hook verdict gate: while it is
non-empty, turn-end is blocked until the bean reaches a terminal verdict. Every
terminal exit below clears it with `rm -f .fiddle/active-bean`.

Set `dispatch_count=0` and `iteration=0`.

## 1c. Resolve Domains

Read `domains` from the bean's eval block (e.g. `domains: [frontend, backend]`); default to `general` if absent. Resolve with the script rather than by hand, so config templates, runtimes, and fallbacks apply consistently:

```bash
scripts/resolve-domains.sh --domains "frontend,backend" --config orchestrate.json > resolved-domains.json
```

It returns one object per domain with `domain`, `template`, optional `runtime` and `ready_check`, and `resolved_via` (`config` or `fallback`). Store the list for steps 1e-2 and 1f.

Also read `runtime_order` from the eval block if present (e.g. `runtime_order: [backend, frontend]`); default to the `domains` order.

## 1d. Dispatch Implementer

Dispatch a subagent with the template at `skills/develop/implementer-prompt.md`, filling:

- `{ITERATION}` — iteration number (1 on first dispatch)
- `{TASK_TEXT}` — full bean body (title, description, acceptance criteria)
- `{CONTEXT}` — relevant file paths, architecture notes, codebase context
- `{EVAL_BLOCK}` — the task's Evaluation block criteria, excluding any criterion marked `holdout: true`
- `{ANTIPATTERNS}` — antipatterns to avoid (see below; empty if none configured)
- `{PRIOR_SCORECARD}` — previous evaluator scorecard with hold-out results removed (empty on first dispatch)
- `{PRIOR_GUIDANCE}` — evaluator fix instructions with hold-out-derived guidance removed (empty on first dispatch)
- `{WORK_DIR}` — worktree directory path

Increment `dispatch_count` after each dispatch.

### Hold-Out Criteria

A criterion marked `holdout: true` is evaluator-only: scored like any other, but never shown to the implementer in the prompt or in feedback. Exclude it from `{EVAL_BLOCK}`, and on re-dispatch (1m FAIL / PASS_REGRESSED) strip its `criteria[]` entry from `{PRIOR_SCORECARD}` and any guidance derived from it. Convergence has to come from the implementer generalizing to the spec; a criterion it can read is one it can rubric-match instead. Default: nothing is held out unless the eval block marks it.

### Antipattern Loading

For each resolved domain, read `evaluators.domains.<domain>.antipatterns` from `orchestrate.json` if the key is set, stopping at any `## Retired` heading (retired entries from deliver 5g are audit-only). Concatenate across domains into `{ANTIPATTERNS}`; leave it empty when no domain configures the key.

## 1e. Handle Implementer Status

- **DONE** / **DONE_WITH_CONCERNS** → evidence gathering (1e-2), then evaluation (1f). A DONE report is the implementer's claim about its own work, not an evaluation of it, so it never substitutes for the evaluator pass.
- **BLOCKED** → mark `needs-attention` with reason, `rm -f .fiddle/active-bean`, escalate to human, next bean.
- **NEEDS_CONTEXT** → supply the requested context and re-dispatch (1d).
- **SPEC_DEFECT** → mark `needs-attention`, `rm -f .fiddle/active-bean`, escalate to human, next bean. Do not re-dispatch: a faithful implementation of a defective spec cannot converge however many iterations run.
  - Record on the bean body what about the spec is defective (the implementer's evidence), a `fiddle:define` re-entry pointer, and the SPEC_DEFECT exit with the iteration number and the one implementer dispatch that occurred.
  - Decrement `dispatch_count` by 1, undoing the increment in 1d: a spec defect is DEFINE's failure, not the implementer's budget.
  - Write no eval-log entry: no evaluator ran, so no scorecard exists and `append-eval-log.sh` hard-requires `--scorecard`. The bean-body note is the record.

## 1e-2. Gather Evidence Pack (per domain)

For each resolved domain, before any evaluator dispatch:

1. Start runtimes if configured (below).
2. Run the project's test command, full output to `evidence-{domain}-tests.txt`.
3. Run invariant/validation scripts named in the bean's eval block, output to `evidence-{domain}-checks.txt`.
4. For runtime-configured domains, probe the running app per `skills/runtime-evidence/SKILL.md` into `evidence-{domain}-runtime.txt`.
5. Concatenate into `evidence-{domain}.txt` with one `### <source>` header per section.

The evaluator interprets this pack; it does not gather evidence itself.

### Runtime Start

If a resolved domain has a `runtime` array, start it before capturing runtime evidence or dispatching the evaluator — a runtime dimension scored without a live app is scored on nothing:

```bash
scripts/start-runtimes.sh --domains <resolved-domains-file>
```

Start in `runtime_order`: for `[backend, frontend]`, start backend, wait for ready, then frontend.

- Exit 0: runtime ready; proceed to evidence capture and evaluator dispatch.
- Exit 3 (harness failure): retry once; if the retry fails, escalate to human without counting against the dispatch budget.
- Exit 1 or 2: an app/config issue; include the error in the evidence pack and evaluator context.

Runtimes stay up through 1f so the evaluator can interact with the app, and stop at Runtime Stop.

## 1f. Dispatch Per-Domain Evaluator

One evaluator per resolved domain, processed in `runtime_order` if specified, otherwise the `domains` order. Select its provider:

    scripts/select-evaluator-provider.sh \
      --preference "<providers array joined with commas>" \
      --implementer claude > selected-provider.json

The domain's `providers` array is an ordered preference list. Implementers
are always claude subagents, so the first available external provider wins;
with none available the evaluator runs on claude in a fresh context.

**Provider `claude`:** dispatch an evaluator subagent on the `skills/evaluate/SKILL.md` protocol with the domain's template (the resolved domain's `template` field, e.g. `skills/evaluate/evaluator-general.md`, `evaluator-frontend.md`, `evaluator-backend.md`), passing `evidence-{domain}.txt` in context.

**External provider:** dispatch via the provider hook:

```bash
hooks/dispatch-provider.sh <provider> \
  --role evaluator \
  --topic "Evaluate domain: {domain}" \
  --instructions "$(cat skills/evaluate/{template}.md)" \
  --diff-file <diff-file> \
  --design-doc-file <design-doc-file> \
  --evidence-file evidence-{domain}.txt
```

External providers get the same context as a claude evaluator (protocol, domain template, calibration, diff, criteria, evidence pack) and return their JSON scorecard as the last content block — see `skills/develop/provider-context.md` for the schema.

Load evaluator context in the order specified by: `skills/develop-loop/context-loading-order.md`

Every evaluator returns one scorecard JSON with per-dimension scores under `.domains`, pass/fail criteria under `.criteria`, and a `"provider"` field naming its producer. Save it per domain and count the dispatch:

```bash
cat > scorecard-{domain}-{provider}.json   # ← evaluator output for this domain
dispatch_count=$((dispatch_count + 1))
```

Dispatch accounting: one implementer + one evaluator per domain per iteration (2 domains = 3 dispatches). PASS_PENDING re-evaluation reuses the provider recorded in selected-provider.json.

### Validate the Scorecard

Gate each scorecard before the merge:

```bash
scripts/validate-scorecard.sh --scorecard scorecard-{domain}-{provider}.json \
  --criteria-ids "<comma-separated criterion ids from the bean's eval block>"
```

On exit 2, stderr is a JSON array naming the gaps: re-dispatch that evaluator once with those errors in its context, and if its second scorecard also fails, mark the bean `needs-attention`, `rm -f .fiddle/active-bean`, and escalate. Merging a scorecard whose criteria ids or evidence do not hold up launders an unusable evaluation into a convergence decision. The re-dispatch counts against the budget like any other.

Dimension justifications may arrive under `evidence` or under `comment`; the validator accepts either, so field naming alone never triggers that re-dispatch.

### Runtime Stop

After all domain evaluators finish, run `scripts/stop-runtimes.sh --state <runtime-state-file>` so no processes outlive the evaluation.

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
