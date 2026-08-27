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

`merge-scorecards.sh` does not carry the optional `spec_defect` field through — the normalized scorecard drops it, so detection happens before normalizing:

```bash
jq 'select(.spec_defect.detected == true) | {provider, reason: .spec_defect.reason}' scorecard-{domain}-{provider}.json
```

If the evaluator flagged `spec_defect.detected == true`, this is not an ordinary threshold failure to re-implement:

- **Log first:** unlike the implementer path, this scorecard is a real evaluation that exists on disk, so complete normalization (1g), cross-domain merge (1h), and the eval log (1l) before routing. The normalized scorecard drops `spec_defect` but keeps the dimension scores, satisfying `append-eval-log.sh`'s required `--scorecard`. Since 1l normally follows 1j/1k, run it now and skip 1i, 1j, 1k, and 1m for this bean.
- **Then route** as an implementer SPEC_DEFECT does, for the same reason (develop-loop step 1e): mark the bean `needs-attention`, record what about the spec is defective (the evaluator's `reason`) plus a `fiddle:define` re-entry pointer, escalate to human, and do not re-dispatch implementation.
- **Budget:** this evaluator dispatch does count against `max_dispatches_per_task` — it performed real evaluation work and produced a scorecard, so do not decrement `dispatch_count`. Only re-implementation is prevented. The implementer path decrements precisely because it produced no evaluation.

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
  . as $cards |
  { domains: (reduce $cards[] as $s ({}; . + ($s.domains // {}))) ,
    criteria: [$cards[] | .criteria[]?] } |
  if ($cards | all(.[]; .mode == "evidence-only")) then .mode = "evidence-only" else . end
' scorecard-general.json scorecard-frontend.json ... > scorecard.json

# Extract merged criteria
jq '.criteria' scorecard.json > criteria.json
```

The `mode` line carries an evidence-only declaration across the domain union, and only when every
domain declared it. Without that line the declaration is dropped here and `check-thresholds.sh`
refuses the merged card, because a card scoring no dimensions and declaring nothing cannot be told
from one whose evaluator dropped its scores. `merge-scorecards.sh` applies the same rule at 1g.

Criteria are concatenated, not deduplicated. Two domains that graded the same criterion id produce
that id twice, and `check-thresholds.sh` refuses the card rather than counting one verdict twice —
which is what an evaluator emitting another domain's ids produced in M5a: 8 criteria where 4 exist.

List only the per-domain merged scorecards (one per resolved domain), not the raw per-provider scorecards (`scorecard-{domain}-{provider}.json`).

On failure, the merged scorecard identifies which domain(s) failed. Pass the merged scorecard to `check-thresholds.sh` — it already handles multi-domain scorecards.

Both files (`scorecard.json` and `criteria.json`) are then passed to the attended gate and subsequently to `check-thresholds.sh`.
