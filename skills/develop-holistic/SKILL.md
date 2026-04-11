---
name: fiddle:develop-holistic
description: Use after all per-task evaluations complete — assesses cross-domain integration and creates remediation beans
argument-hint: --epic <id>
---

# Develop Holistic — Cross-Domain Integration Review

Assess the full system as an integrated whole. Catches cross-domain issues that per-task evaluation cannot see. Creates remediation beans and re-evaluates until converged or budget exceeded.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--epic <id>` | **required** | The epic to holistically review |

Read `orchestrate.json` from project root. Extract `evaluators.holistic` block:

```json
{
  "evaluators": {
    "holistic": {
      "providers": ["claude"],
      "max_iterations": 3
    }
  }
}
```

Store `providers` for holistic reviewer dispatch (default: `["claude"]`). Store `max_iterations` for the holistic dispatch budget (default: 3).

## 2a. Pre-flight

All task beans must be `completed` or `needs-attention` (escalated). Check:

```bash
beans list --parent <epic-id> --json
```

If any task bean is still `todo` or `in-progress`, return to the orchestrator — those beans must be processed first.

Start ALL domain runtimes for the full set of domains across all tasks. Collect every unique domain from all task beans' eval blocks and start them:

```bash
scripts/start-runtimes.sh --domains <all-domains-resolved.json>
```

<HARD-GATE>
ALL domain runtimes must be running before holistic review begins.
If start-runtimes.sh fails (exit 3, harness failure): retry once. If retry fails, escalate to human.
If start-runtimes.sh fails (exit 1 or 2, app/config issue): include the error in holistic reviewer context and proceed — Runtime Health will reflect the failure.
Do NOT skip runtime start. Do NOT dispatch holistic review without attempting to start all runtimes.
</HARD-GATE>

## 2b. Dispatch Holistic Reviewer (Per-Provider)

Read `evaluators.holistic.providers` from `orchestrate.json`. If the key is absent, default to `["claude"]`.

For each provider in the holistic providers list, dispatch a holistic reviewer:

**If provider is `claude`:** Dispatch a subagent using `skills/develop/holistic-review.md`.

**If provider is external (not `claude`):** Dispatch via the provider hook:

```bash
hooks/dispatch-provider.sh <provider> \
  --role holistic-reviewer \
  --topic "Holistic review: cross-domain integration assessment" \
  --instructions "$(cat skills/develop/holistic-review.md)" \
  --diff-file <diff-file> \
  --design-doc-file <design-doc-file>
```

Provide to ALL holistic reviewers (claude and external):

- The full diff since the epic's base SHA (before any task started): `git diff {epic-base-sha}...HEAD`
- The design spec / plan document
- All task bean bodies (for spec requirements): `beans list --parent <epic-id> --json`
- Runtime state for ALL domains (ports, domain names, ready status)

Each holistic reviewer produces a single JSON scorecard with:
- `domain: "holistic"` with dimensions: `integration`, `coherence`, `holistic_spec_fidelity`, `polish`, `runtime_health`
- `spec_coverage_matrix` — JSON array classifying every spec requirement as Full/Weak/Missing
- `remediation_beans` — JSON array of remediation tasks for gaps

The scorecard MUST include a `"provider"` field identifying which provider produced it.

**Each holistic provider dispatch counts as 1 toward the holistic dispatch budget.** Track `holistic_dispatch_count` across iterations. For example, 2 providers = 2 dispatches per holistic iteration. The `--current-dispatches` passed to `check-convergence.sh` in step 2c reflects total holistic provider dispatches (not just iteration count).

Save each provider's scorecard separately:

```bash
cat > scorecard-holistic-{provider}.json   # ← holistic reviewer output for this provider
```

## 2b-2. Merge Holistic Provider Scorecards

After all holistic provider scorecards are collected, merge them before threshold checks.

<HARD-GATE>
After receiving ALL holistic provider scorecards, you MUST run:
  scripts/merge-scorecards.sh < scorecards-array.json > scorecard-holistic.json 2> disagreements-holistic.json
Use the merged scorecard for threshold checks. Do NOT merge scores yourself.
</HARD-GATE>

Build a JSON array of all holistic provider scorecards and pipe to `merge-scorecards.sh`:

```bash
# Collect all holistic provider scorecards into a JSON array
jq -s '.' scorecard-holistic-*.json | \
  scripts/merge-scorecards.sh > scorecard-holistic.json 2> disagreements-holistic.json
```

The merged scorecard uses conservative (min) scoring: for each dimension, the final score is the minimum across providers. Each dimension includes `provider_scores` showing per-provider breakdown.

**Coverage matrix merge:** Union all requirements from all providers' `spec_coverage_matrix` arrays. For each requirement, coverage = min across providers using the ordering Full > Weak > Missing. If ANY provider marks a requirement as Missing, the merged result is Missing. If any marks Weak and none marks Missing, the merged result is Weak.

```json
{
  "spec_coverage_matrix": [
    {"requirement": "R1", "coverage": "Missing", "provider_coverage": {"claude": "Full", "codex": "Missing"}},
    {"requirement": "R2", "coverage": "Full", "provider_coverage": {"claude": "Full", "codex": "Full"}}
  ]
}
```

**Remediation bean merge:** Union all providers' `remediation_beans` arrays. Deduplicate by requirement — if multiple providers produce remediation beans for the same requirement, keep the one with the most specific description (longest body). Each merged remediation bean includes a `source_providers` field listing which providers flagged it.

Disagreements (spread >= 3 between providers on any dimension) are written to `disagreements-holistic.json`. Include disagreement data in evaluator feedback when re-dispatching holistic review.

If only one provider is configured, `merge-scorecards.sh` still runs (single-element array) to ensure consistent scorecard format.

## 2c. Check Holistic Thresholds

<HARD-GATE>
After receiving the holistic scorecard, you MUST run:
  scripts/check-thresholds.sh --scorecard scorecard-holistic.json --criteria criteria-holistic.json
  scripts/check-convergence.sh --current {verdict_file} --history {holistic_history_file} --max-dispatches {max_iterations} --current-dispatches {current_iteration}
Act on the scripts' verdicts. Do NOT compute thresholds or convergence yourself.
</HARD-GATE>

Holistic thresholds default to those in `skills/develop/holistic-review.md`:
- Integration: 7
- Coherence: 7
- Holistic Spec Fidelity: 8
- Polish: 6
- Runtime Health: 9

These are overridden by `evaluators.holistic.thresholds` in `orchestrate.json` if present.

The holistic review uses the same convergence protocol as per-task evaluation (two consecutive passes needed). The `check-convergence.sh` call uses a holistic-specific history file to track holistic dispatch history separately from per-task dispatch history.

The dispatch budget for holistic review is `evaluators.holistic.max_iterations` from `orchestrate.json` (default 3).

## 2d. Handle Remediation

If holistic review **FAILS**:

1. Check the `remediation_beans` array in the holistic scorecard
2. If non-empty, create remediation task beans:
   ```bash
   # For each remediation bean in the array:
   beans create --parent <epic-id> --title "Fix: ..." --body "<description>" --eval "<eval block>"
   ```
3. Each remediation bean is a child of the epic, traced back to its source (spec coverage gap or failing dimension)
4. For each remediation bean, invoke:
   ```
   Skill("fiddle:develop-loop", args: "--bean <remediation-bean-id> --epic <epic-id>")
   ```
5. After all remediation beans complete, re-run holistic review (back to step 2a)

Track the holistic iteration count. Increment on each holistic review cycle.

<HARD-GATE>
If holistic review has been run `evaluators.holistic.max_iterations` times (default 3) and still fails:
  STOP. Mark the epic as `needs-attention`. Escalate to human with:
  - The latest holistic scorecard
  - The spec coverage matrix showing remaining gaps
  - The list of dimensions still below threshold
  - The full remediation history (which beans were created and their outcomes)
Do NOT continue iterating. Do NOT lower thresholds. Do NOT rationalize.
</HARD-GATE>

| Result | Action |
|---|---|
| **CONVERGED** | Holistic review passed (two consecutive passes). Proceed to step 2e. |
| **FAIL** | Generate remediation beans → invoke `Skill("fiddle:develop-loop")` for each → re-run holistic review (back to 2a). |
| **PASS_PENDING** | Re-run holistic review without remediation — scorecard may stabilize. → Back to 2b. |
| **PASS_REGRESSED** | Generate remediation beans targeting regressed dimensions → invoke develop-loop → re-run holistic review. → Back to 2a. |
| **DISPATCHES_EXCEEDED** | Max holistic iterations reached. Escalate to human. |

## 2e. Stop Runtimes

<HARD-GATE>
After holistic review completes (CONVERGED or escalated):
  Run: scripts/stop-runtimes.sh --state <runtime-state-file>
Do NOT leave processes running after holistic review.
</HARD-GATE>

Return to orchestrator with result: CONVERGED or ESCALATED.
