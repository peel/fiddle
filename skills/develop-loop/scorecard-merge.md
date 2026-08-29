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

### Spec-Defect Check (on the merged card)

Both merges carry `spec_defect` through, so the check reads `scorecard.json`, the cross-domain
merged card produced at 1h. It does not read the raw per-provider cards. Until M5b the merge
dropped the field and the check had to run before normalizing; a merged card then read `null`
whether every evaluator cleared the spec or none of them mentioned it, and a bean flagged at
`fiddle-2690` reached `check-thresholds.sh` PASS and `check-convergence.sh` CONVERGED on a card
that showed nothing.

The merged `spec_defect` names one of three states, so absent is not read as false:

| `.spec_defect.state` | what it means | what to do |
|---|---|---|
| `detected` | at least one source card flagged the spec. `.reason` and `.sources` name the domain and provider that raised it | take the spec-defect exit below |
| `clear` | every source card carried the field and none flagged. `.detected` is `false` | continue to 1i |
| `not_reported` | at least one source card did not carry the field, or carried one stating no boolean `detected`. `.missing_from` names it, and no `detected` key is emitted | the card cannot answer. Re-dispatch that evaluator with `skills/develop/scorecard-envelope.md` in context, or read its raw card, and do not treat this as clear |

```bash
jq '.spec_defect | {state, reason, sources, missing_from}' scorecard.json
```

A merged card carrying no `spec_defect` key at all predates this rule. Treat it as `not_reported`.

If the merged card reads `state: "detected"`, this is not an ordinary threshold failure to re-implement:

- **Log first:** unlike the implementer path, this scorecard is a real evaluation that exists on disk, so complete normalization (1g), cross-domain merge (1h), and the eval log (1l) before routing. The merged scorecard keeps the dimension scores, satisfying `append-eval-log.sh`'s required `--scorecard`. Since 1l normally follows 1j/1k, run it now and skip 1i, 1j, 1k, and 1m for this bean.
- **Then route** as an implementer SPEC_DEFECT does, for the same reason (develop-loop step 1e): mark the bean `needs-attention`, record what about the spec is defective (the merged `reason`, which names each source as `{domain}/{provider}`) plus a `fiddle:define` re-entry pointer, escalate to human, and do not re-dispatch implementation.
- **Budget:** this evaluator dispatch does count against `max_dispatches_per_task` — it performed real evaluation work and produced a scorecard, so do not decrement `dispatch_count`. Only re-implementation is prevented. The implementer path decrements precisely because it produced no evaluation.

## Cross-Domain Merge (Step 1h)

After all domain evaluators return, merge their scorecards:

- **Union** scorecards across domains — each domain is scored independently
- The merged scorecard has all domains under `.domains`: `{"frontend": {...}, "backend": {...}}`
- **No shared dimensions** — `domain_spec_fidelity` in frontend is completely independent from `domain_spec_fidelity` in backend
- Each domain must independently meet its own thresholds

```bash cross-domain-merge
# Merge per-domain (already provider-merged) scorecards into a single cross-domain scorecard.
# Use only scorecard-{domain}.json files (not scorecard-{domain}-{provider}.json raw files).
jq -s '
  . as $cards |
  ([$cards[] | select((.spec_defect | type) == "object") | .spec_defect.sources[]?]) as $defect_sources |
  ([$cards[] | select((.spec_defect | type) == "object") | .spec_defect.reported_by[]?] | unique) as $defect_reported |
  (([$cards[] | select((.spec_defect | type) != "object") | .domains | keys[]] +
    [$cards[] | select((.spec_defect | type) == "object") | .spec_defect.missing_from[]?]) | unique) as $defect_missing |
  { domains: (reduce $cards[] as $s ({}; . + ($s.domains // {}))) ,
    criteria: [$cards[] | .criteria[]?],
    spec_defect: (
      { sources: $defect_sources, reported_by: $defect_reported, missing_from: $defect_missing } |
      if ($defect_sources | length) > 0 then
        . + { state: "detected", detected: true,
              reason: ([$defect_sources[] | "\(.domain)/\(.provider): \(.reason)"] | join(" | ")) }
      elif ($defect_missing | length) > 0 then . + { state: "not_reported" }
      else . + { state: "clear", detected: false } end
    ) } |
  if ($cards | all(.[]; .mode == "evidence-only")) then .mode = "evidence-only" else . end
' scorecard-general.json scorecard-frontend.json ... > scorecard.json

# Extract merged criteria
jq '.criteria' scorecard.json > criteria.json
```

`scripts/test-scorecard-merge-doc.sh` finds this block by the fence marker `cross-domain-merge`,
extracts the `jq -s` program, and runs it against fixtures. Keep the marker and the `jq -s '`
opening line. If the lane finds no block, it exits 2 and names the marker. It does not pass over
an empty extraction.

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
