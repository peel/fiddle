# Scorecard Merge Protocol

## Per-Domain Provider Merge (Step 1g)

After all provider scorecards are collected for each domain, merge them before threshold checks.

<HARD-GATE>
After receiving ALL provider scorecards for a domain, you MUST run:
  scripts/merge-scorecards.sh < scorecards-array.json > scorecard-{domain}.json 2> disagreements-{domain}.json
Use the merged scorecard for threshold checks. Do NOT merge scores yourself.
</HARD-GATE>

For each domain, build a JSON array of all provider scorecards for that domain, then pipe to `merge-scorecards.sh`:

```bash
# Collect all provider scorecards for a domain into a JSON array
jq -s '.' scorecard-{domain}-*.json | \
  scripts/merge-scorecards.sh > scorecard-{domain}.json 2> disagreements-{domain}.json
```

The merged scorecard uses conservative (min) scoring: for each dimension, the final score is the minimum across providers. Each dimension includes `provider_scores` showing per-provider breakdown:

```json
{
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 7, "threshold": 7, "provider_scores": {"claude": 8, "codex": 7}}
      }
    }
  }
}
```

Disagreements (spread >= 3 between providers on any dimension) are written to `disagreements-{domain}.json`. Include disagreement data in evaluator feedback when re-dispatching implementers.

After merging all domains, combine disagreement files for the eval log:

```bash
# Merge per-domain disagreement files into a single array
jq -s 'add // []' disagreements-*.json > disagreements.json
```

Pass the combined `disagreements.json` to `append-eval-log.sh` in the logging step.

If a domain has only one provider, `merge-scorecards.sh` still runs (single-element array) to ensure consistent scorecard format.

## Cross-Domain Merge (Step 1h)

After all domain evaluators return, merge their scorecards:

- **Union** scorecards across domains — each domain is scored independently
- The merged scorecard has all domains under `.domains`: `{"frontend": {...}, "backend": {...}}`
- **No shared dimensions** — `domain_spec_fidelity` in frontend is completely independent from `domain_spec_fidelity` in backend
- Each domain must independently meet its own thresholds

```bash
# Merge per-domain (already provider-merged) scorecards into a single cross-domain scorecard.
# Use only scorecard-{domain}.json files (not scorecard-{domain}-{provider}.json raw files).
jq -s '
  { domains: (reduce .[] as $s ({}; . + ($s.domains // {}))) ,
    criteria: [.[] | .criteria[]?] }
' scorecard-general.json scorecard-frontend.json ... > scorecard.json

# Extract merged criteria
jq '.criteria' scorecard.json > criteria.json
```

List only the per-domain merged scorecards (one per resolved domain). Do NOT include raw per-provider scorecards (`scorecard-{domain}-{provider}.json`) in this merge.

On failure, the merged scorecard identifies which domain(s) failed. Pass the merged scorecard to `check-thresholds.sh` — it already handles multi-domain scorecards.

Both files (`scorecard.json` and `criteria.json`) are then passed to the attended gate and subsequently to `check-thresholds.sh`.
