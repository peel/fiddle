# Scorecard Merge Protocol

## Per-Domain Normalization (Step 1g)

Each domain has exactly one evaluator scorecard. Run it through
merge-scorecards.sh as a single-element array so downstream consumers see a
uniform shape:

    jq -s '.' scorecard-{domain}-{provider}.json | \
      scripts/merge-scorecards.sh > scorecard-{domain}.json

Provider min-merging and disagreement tracking apply only to holistic review
(see skills/develop-holistic/SKILL.md). The per-task path has no
disagreements file; pass nothing to --disagreements in the eval-log step.

### Spec-Defect Check (before normalization)

`merge-scorecards.sh` does NOT carry the optional `spec_defect` field through — the normalized scorecard drops it. So detection MUST happen BEFORE normalizing: scan the evaluator scorecard for a flagged spec defect:

```bash
jq 'select(.spec_defect.detected == true) | {provider, reason: .spec_defect.reason}' scorecard-{domain}-{provider}.json
```

If the evaluator flagged `spec_defect.detected == true`, do not treat this as an ordinary threshold failure to re-implement. Handle it as follows:

- **Log first:** unlike the implementer path, the evaluator scorecard here is a real evaluation that exists on disk, so complete the normal normalization (step 1g), cross-domain merge (step 1h), and eval-log (step 1l) before routing. The normalized scorecard drops `spec_defect` but retains the dimension scores, so `append-eval-log.sh` runs with its required `--scorecard` file and records the iteration normally. Because 1l normally runs after 1j/1k in the loop, run it NOW (right after the merge) and SKIP 1i, 1j, 1k, and 1m for this bean — the spec-defect HARD-GATE at the end of develop-loop step 1g–1h spells out this bypass.
- **Then route** the same way as an implementer SPEC_DEFECT (develop-loop step 1e): mark the bean `needs-attention`, record WHAT about the spec is defective (the evaluator's `reason`) plus a `fiddle:define` re-entry pointer, escalate to human, and skip re-dispatching implementation. A faithful implementation of a defective spec will not converge no matter how many iterations run.
- **Budget:** the evaluator dispatch that produced this discovery DOES count against `max_dispatches_per_task` — it performed real evaluation work and produced a scorecard, so do NOT decrement `dispatch_count`. Only re-implementation is prevented. (This is the deliberate asymmetry with the implementer path, where the single implementer dispatch IS decremented because it produced no evaluation.)

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
