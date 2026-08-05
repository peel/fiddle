---
name: develop-holistic
description: Use after all per-task evaluations complete — assesses cross-domain integration and creates remediation beans
---

# Develop Holistic — Cross-Domain Integration Review


## Usage

Invoke as `fiddle:develop-holistic --epic <id>`.

Assess the full system as an integrated whole: start every domain runtime, dispatch a holistic reviewer per provider, merge their scorecards, then remediate and re-review until converged or the budget is exhausted. This runs after the per-task loop has finished with every bean, because per-task scores are produced in isolation and say nothing about cross-domain coherence.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`: `--epic <id>` (required, the epic to review holistically).

Config: see `skills/orchestrate/SKILL.md` for the schema. This skill reads `evaluators.holistic.providers`, `evaluators.holistic.max_iterations`, and `evaluators.holistic.thresholds`.

`providers` (default `["claude"]` when absent) is a dispatch fan-out here, not the ordered preference list a per-task domain uses: holistic review dispatches to every listed provider. `max_iterations` is the holistic dispatch budget, falling back to 3 when the key is absent; read the live value from `orchestrate.json`.

## 2a. Pre-flight

Every task bean has to be `completed` or `needs-attention` (escalated) first:

```bash
beans list --parent <epic-id> --json
```

If any task bean is still `todo` or `in-progress`, return to the orchestrator — those beans get processed before the whole can be assessed.

Then collect every unique domain from all task beans' eval blocks and start their runtimes:

```bash
scripts/start-runtimes.sh --domains <all-domains-resolved.json>
```

Every domain runtime must be running before holistic review begins: the seams between domains are the only thing this review adds, and they are invisible without exercising the running system.

- Exit 3 (harness failure): retry once; if the retry fails, escalate to human.
- Exit 1 or 2 (app/config issue): include the error in the holistic reviewer's context and proceed — Runtime Health will reflect the failure.

## 2a-2. Gather the Holistic Evidence Pack

With the runtimes up, capture the evidence the reviewers will score from, into `evidence-holistic.txt` with a `### <source>` header per section:

1. The project's test-suite output for the epic as a whole.
2. Output from any invariant or validation scripts the epic's beans name.
3. A probe transcript per runtime-configured domain: each runtime answering on its port, the primary cross-domain flows exercised end to end, screenshots or response bodies for the states reached, and console output including warnings.

The reviewers interpret this pack rather than driving the runtimes themselves, which is what lets a provider running read-only score runtime dimensions at all. A reviewer left to gather its own evidence either cannot (no writable temp dir, no tool access) or gathers something different from its peers, and neither outcome is comparable across providers.

## 2b. Dispatch Holistic Reviewer (Per-Provider)

For each provider in `evaluators.holistic.providers`, dispatch a holistic reviewer.

**Provider `claude`:** resolve the `holistic` model for phase `develop`; pass it only when returned, then dispatch a subagent on `skills/develop/holistic-review.md` with `evidence-holistic.txt` in context.

**External provider:** dispatch via the provider hook:

```bash
hooks/dispatch-provider.sh <provider> \
  --role holistic-reviewer \
  --topic "Holistic review: cross-domain integration assessment" \
  --instructions "$(cat skills/develop/holistic-review.md)" \
  --diff-file <diff-file> \
  --design-doc-file <design-doc-file> \
  --evidence-file evidence-holistic.txt
```

Every holistic reviewer, claude or external, gets the same context:

- The evidence pack from 2a-2: `evidence-holistic.txt`
- The full diff since the epic's base SHA, before any task started: `git diff {epic-base-sha}...HEAD`
- The design spec / plan document
- All task bean bodies for their spec requirements: `beans list --parent <epic-id> --json`
- Runtime state for every domain (ports, domain names, ready status)

Each reviewer returns one JSON scorecard with `domain: "holistic"` and the dimensions `integration`, `coherence`, `holistic_spec_fidelity`, `polish`, `runtime_health`; a `spec_coverage_matrix` array classifying every spec requirement as Full/Weak/Missing; a `remediation_beans` array for the gaps; and a `provider` field naming its producer. Save each one separately:

```bash
cat > scorecard-holistic-{provider}.json   # ← holistic reviewer output for this provider
```

Each holistic provider dispatch counts 1 against the holistic budget, so track `holistic_dispatch_count` across iterations (2 providers = 2 dispatches per iteration). The `--current-dispatches` passed to `check-convergence.sh` in 2c is that total, not the iteration count.

## 2b-2. Merge Holistic Provider Scorecards

Merge the collected scorecards with the script rather than reconciling scores yourself:

```bash
jq -s '.' scorecard-holistic-*.json | \
  scripts/merge-scorecards.sh > scorecard-holistic.json 2> disagreements-holistic.json
```

Threshold checks use the merged scorecard. The merge is conservative: each dimension's final score is the minimum across providers, with `provider_scores` recording the per-provider breakdown. A single configured provider still goes through the script (as a single-element array) so the scorecard format stays uniform.

**Coverage matrix merge:** union all requirements across providers' `spec_coverage_matrix` arrays, taking per-requirement coverage as the minimum on the ordering Full > Weak > Missing. Any provider marking a requirement Missing makes the merged result Missing; Weak with no Missing makes it Weak.

```json
{
  "spec_coverage_matrix": [
    {"requirement": "R1", "coverage": "Missing", "provider_coverage": {"claude": "Full", "codex": "Missing"}},
    {"requirement": "R2", "coverage": "Full", "provider_coverage": {"claude": "Full", "codex": "Full"}}
  ]
}
```

**Remediation bean merge:** union all providers' `remediation_beans` arrays, deduplicating by requirement — where several providers flag the same requirement, keep the most specific description (longest body) and list the flagging providers in `source_providers`.

Disagreements (spread >= 3 between providers on a dimension) land in `disagreements-holistic.json`. Include them in reviewer feedback when re-dispatching holistic review.

## 2c. Check Holistic Thresholds

Run both scripts on the merged scorecard and act on their verdicts:

```bash
scripts/check-thresholds.sh --scorecard scorecard-holistic.json --criteria criteria-holistic.json
scripts/check-convergence.sh --current {verdict_file} --history {holistic_history_file} --max-dispatches {max_iterations} --current-dispatches {holistic_dispatch_count}
```

Holistic thresholds default to those in `skills/develop/holistic-dimensions.md`:
- Integration: 7
- Coherence: 7
- Holistic Spec Fidelity: 8
- Polish: 6
- Runtime Health: 9

`evaluators.holistic.thresholds` in `orchestrate.json` overrides them when present.

Convergence follows the same protocol as per-task evaluation — two consecutive passes — but against a holistic-specific history file, keeping holistic dispatch history separate from per-task history.

## 2d. Handle Remediation

On FAIL, read the `remediation_beans` array in the merged scorecard and create a child bean of the epic for each entry, traced back to its source (a spec coverage gap or a failing dimension):

```bash
beans create --parent <epic-id> --title "Fix: ..." --body "<description>" --eval "<eval block>"
```

Then run each through the per-task loop: use `fiddle:develop-loop` with `--bean <remediation-bean-id> --epic <epic-id>`.

Once every remediation bean completes, re-run holistic review from 2a and increment the holistic iteration count.

| Result | Action |
|---|---|
| **CONVERGED** | Holistic review passed (two consecutive passes). Proceed to 2e. |
| **FAIL** | Create remediation beans → use `fiddle:develop-loop` for each → re-run holistic review. → 2a |
| **PASS_PENDING** | Re-run holistic review without remediation; the scorecard may stabilize. → 2b |
| **PASS_REGRESSED** | Create remediation beans targeting the regressed dimensions → develop-loop → re-run holistic review. → 2a |
| **DISPATCHES_EXCEEDED** | Budget exhausted. Escalate to human. |

On DISPATCHES_EXCEEDED, stop and hand the epic to the human: mark it `needs-attention` and report the latest holistic scorecard, the coverage matrix with its remaining gaps, the dimensions still below threshold, and the full remediation history (which beans were created and how they turned out). The budget is the only protection against remediating forever on an epic that is not converging, so spending past it — or lowering thresholds to fit — hides exactly the problem it just surfaced.

## 2e. Stop Runtimes

Once holistic review finishes, converged or escalated, run `scripts/stop-runtimes.sh --state <runtime-state-file>` so no processes outlive the review.

Return to the orchestrator with CONVERGED or ESCALATED.
