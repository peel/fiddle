# Convergence and recovery

## 1g–1h. Merge Scorecards

Normalize each domain's scorecard and merge across domains following: `skills/develop-loop/scorecard-merge.md`

### The Spec-Defect Check reads the merged card

Run it on `scorecard.json`, the cross-domain merged card that 1h produced. That is the same card
every step after this one reads, so the check does not depend on anyone opening a raw
`scorecard-{domain}-{provider}.json` first. Both merges carry `spec_defect` through and state which
of three things happened:

```bash
jq -r '.spec_defect.state // "not_reported"' scorecard.json
```

- **`detected`** — take the spec-defect exit below. `.spec_defect.reason` names each source as
  `{domain}/{provider}`.
- **`clear`** — every source card carried the field and none flagged it. Continue to 1i.
- **`not_reported`** — at least one source card never carried the field, or carried one stating no
  boolean `detected`. `.spec_defect.missing_from` names it. This is not a clear result and is not a failing one. Re-dispatch that
  evaluator with `skills/develop/scorecard-envelope.md` in its context, or read its raw card, before
  continuing. A merged card with no `spec_defect` key at all predates this rule and reads the same
  way.

A `null` here used to mean either of the first two and could not be told apart, because
`merge-scorecards.sh` dropped the field. `fiddle-2690` was flagged by its evaluator, merged to
`null`, and passed `check-thresholds.sh` and `check-convergence.sh` on that card. A human caught it
by reading the source card, which is a habit and not a control. `scripts/test-merge-scorecards.sh`
and `scripts/test-scorecard-merge-doc.sh` now hold the case that fails if the two states produce one
output.

The spec-defect exit:

1. Run 1l now with the merged scorecard, so `append-eval-log.sh` records the iteration (the merged scorecard satisfies its `--scorecard` requirement).
2. Route to `needs-attention` per the scorecard-merge Spec-Defect Check: record the defect reason and `fiddle:define` re-entry pointer, escalate to human, do not re-dispatch.
3. `rm -f .fiddle/active-bean`
4. Skip 1i, 1j, 1k, and 1m for this bean.
5. Return to the orchestrator for the next bean.

### Precondition for 1j: the merged card must be gradeable

The graders assert this. Do not assert it by hand. `check-thresholds.sh` exits 2 with a reason,
rather than returning a verdict, when the merged card:

| the merged card | the reason it exits 2 |
|---|---|
| carries 0 dimensions and 0 criteria | `scorecard has nothing to grade` |
| scored no dimensions and declares no `mode` | `scorecard scored no dimensions and does not declare evidence-only` |
| carries one criterion id twice | `duplicate criterion id` |

Each is a mis-shaped card and not a failing one: repair it or re-dispatch, and never pass an exit-2
result to `check-convergence.sh`. The rule counts dimensions across the whole card, because a bean
may set thresholds for one domain and none for another. One domain scoring nothing is caught at 1g
instead, by `validate-scorecard.sh`, where every card carries exactly one domain.

An evaluation that scores no dimensions on purpose declares `"mode": "evidence-only"` on the card
(`skills/develop/scorecard-envelope.md`). Both merges carry the declaration forward only when every
input card holds it, `check-thresholds.sh` stamps it on the verdict, and `check-convergence.sh`
takes the one-pass evidence-only path only when it is there. An empty `dimensions` alone is not the
declaration.

Two beans in M5a reached this point with a card holding no scores, and both were caught only by a
check the operator ran by hand on every bean after `fiddle-0dzn`:

    DIMS=$(jq '[.domains[]?.dimensions // {} | length] | add // 0' scorecard.json)
    CRIT=$(jq 'length' criteria.json)
    [ "$DIMS" -gt 0 ] && [ "$CRIT" -gt 0 ] || exit 2

That check now lives in the tools. A precondition applied by hand protects only the beans whose
operator remembers to apply it.

## 1i. Attended Scorecard Gate

If `evaluators.attended` is true in orchestrate.json, follow: `skills/develop-loop/attended-gate.md`. When it is false, go straight to 1j.

## 1j–1k. Check Thresholds and Convergence

Run both scripts and act on their verdicts rather than judging thresholds or convergence yourself:

```bash
scripts/check-thresholds.sh --scorecard {scorecard_file} --criteria {criteria_file} --tree-sha "$(git rev-parse HEAD^{tree})"
scripts/check-convergence.sh --current {verdict_file} --history {history_file} --max-dispatches N --current-dispatches M
```

`check-thresholds.sh` takes the merged scorecard (from 1h, as corrected in 1i) and returns `PASS` (exit 0) or `FAIL` (exit 1), naming the failing domain(s) and including a `dimensions` flat map (`{"frontend.correctness": 8, ...}`). It returns neither when the input cannot be graded: a dimension with no `threshold`, a `--criteria` entry with no `pass`, one criterion id twice, or a card with no scores and no evidence-only declaration exits 2 with `{"error", "problems"}` on stdout, one stderr line per problem naming the missing field and the dimension or criterion id, and a stderr line naming the envelope it wanted (`skills/develop/scorecard-envelope.md`). That is a mis-shaped scorecard, not a failing one — repair it or re-dispatch, and do not feed an exit-2 result to `check-convergence.sh`. `{criteria_file}` is the scorecard's graded `criteria` array (`jq '.criteria' {scorecard_file}`), never the ungraded briefing array the evaluator was given. Pass that output to `check-convergence.sh` as `--current`, and append it to the `--history` array for later checks. `check-convergence.sh` returns:

- **CONVERGED** (exit 0) — two consecutive passes, with no score regression across a changed tree and no contradiction on an unchanged one. A verdict declaring `mode` `"evidence-only"` and carrying no dimension scores converges on one pass: it has no scores for a second pass to confirm. A verdict that carries no scores and no declaration takes the two-pass path
- **FAIL** (exit 1) — thresholds not met
- **PASS_PENDING** (exit 1) — passed once, needs a consecutive pass
- **PASS_REGRESSED** (exit 1) — passed but regressed on previously-passing dimensions, the tree having changed in between
- **CONTESTED** (exit 2) — two iterations on the same tree contradict each other about a criterion, a dimension, or a finding
- **DISPATCHES_EXCEEDED** (exit 2) — budget exhausted

`check-convergence.sh` also exits 2 with `{"error": "current verdict is not a check-thresholds.sh result"}` when `--current` names a file whose `verdict` is neither `PASS` nor `FAIL`. That is a refused or hand-made card reaching the convergence check, not a convergence result.

### Two iterations, two cases

`check-thresholds.sh` stamps the tree it graded onto the verdict it emits, so every history entry says which work it was judging. That is the seam because the verdict object is built in exactly one place and history is a list of those objects: a sha handed separately to `check-convergence.sh` could drift from the card it belongs to, and entries written before the field existed would silently acquire the current tree. Pass `git rev-parse HEAD^{tree}` rather than the commit — a tree names content, so an amend or a rebase that changes no bytes is correctly read as unchanged.

**The trees differ.** The two iterations judged two versions of the work. A dimension scoring below the previous iteration means remediation broke something an earlier pass had proved, and PASS_REGRESSED blocks on it. This is the case the guard was written for.

**The trees are identical.** Nothing was implemented in between, so the two iterations are not two versions of the work — they are two evaluators, and any difference between them is a difference of opinion about the same bytes. A score delta here is calibration, not regression: it is reported as `ignored_score_deltas` and does not block. What blocks is a contradiction about what is *true* of the code — a criterion a previous pass cleared that now fails, a dimension it cleared that is now below threshold, or a finding above `low` severity the previous iteration did not report (findings come from the scorecard's `antipatterns_detected`, whose entries may carry `severity`; an entry that states none counts). Those produce CONTESTED, which is terminal: one of the two evaluators is wrong about identical bytes, and no further dispatch establishes which.

CONTESTED is terminal by design. The cheapest response to a false block on an unchanged tree is to re-dispatch until two evaluators happen to agree, which is score-shopping wearing protocol compliance and is indistinguishable in the log from genuine convergence. A guard another roll satisfies is worse than no guard, so the same-tree disagreement that matters offers no further roll.

When either sha is absent — history written before the field, or a caller passing no `--tree-sha` — the comparison is `unknown` and the score-regression guard stays on. It is suppressed only where the tree is provably unchanged.

On DISPATCHES_EXCEEDED, stop and ask the human. The budget is the only protection against iterating forever on a bean that is not converging, so spending past it — or lowering thresholds to fit — hides exactly the problem it just surfaced.

The budget limits additional dispatches, not evaluation of work already returned. A terminal CONVERGED result from the final allowed dispatch wins; at the same count, FAIL, PASS_PENDING, or PASS_REGRESSED becomes DISPATCHES_EXCEEDED because satisfying it would require another dispatch. CONTESTED requires none, so it is reported as itself at any count — it names why the loop stopped, which DISPATCHES_EXCEEDED would hide.

## 1l. Log Evaluation

After every evaluation cycle:

```bash
scripts/append-eval-log.sh --bean-id {id} --iteration {N} --scorecard {scorecard_file} --dispatches {count} --tree-sha {tree} --convergence {status} --guidance {text} --antipatterns antipatterns.json
```

- `--dispatches` counts actual dispatches, not iterations.
- `--tree-sha` and `--convergence` put the tree that was judged and the convergence result on the entry, so the log shows what every dispatch looked at and what it decided. `scripts/parse-eval-log.sh` reads them back as `iterations[]` and counts `unchanged_tree_reevaluations`: dispatches that re-judged a tree the previous dispatch had already judged. The same count is maintained on the bean itself, next to `total_dispatches` in the Evaluation Log header, so it is read without running anything; `scripts/trend-eval-history.sh` sums it per epic and the milestone handoff carries the total forward. A bean that converged with several of those was evaluated repeatedly without the work changing, which is what score-shopping looks like from outside. Omitting these fields hides that, so pass them on every iteration.
- No `--disagreements` on the per-task path: one evaluator per domain produces no disagreements file, and that tracking is holistic-only.
- Record the provider and reason from `selected-provider-{domain}.json`, which captures fallback substitutions without one domain overwriting another.
- `--antipatterns` is optional: `jq -c '.antipatterns_detected // []' {scorecard_file} > antipatterns.json`. A non-empty array appends an **Antipatterns detected:** section, the durable per-epic record deliver 5g ages against.
- Pass `--corrections {corrections_json}` (array of `{domain, dimension, evaluator_score, human_score, reason}`) when the attended gate produced corrections.

The log is the loop's only state that survives a restart, so let the script write it.

## 1m. Act on Convergence Result

| Result | Action |
|---|---|
| **CONVERGED** | Mark bean `completed`. `rm -f .fiddle/active-bean`. Return to orchestrator. |
| **FAIL** | Re-dispatch implementer with the failing dimensions, their domains, and fix guidance. → 1d |
| **PASS_PENDING** | Re-evaluate without re-implementing; reuse the provider in `selected-provider-{domain}.json`. → 1e-2 |
| **PASS_REGRESSED** | Re-dispatch implementer with regression details (which dimensions in which domains, by how much). → 1d |
| **CONTESTED** | Mark bean `needs-attention`, recording the contested criteria, dimensions and findings alongside both iterations. `rm -f .fiddle/active-bean`. Escalate to human; do not re-dispatch. Return to orchestrator. |
| **DISPATCHES_EXCEEDED** | Mark bean `needs-attention`. `rm -f .fiddle/active-bean`. Escalate to human. Return to orchestrator. |

FAIL and PASS_REGRESSED go back through 1d, so their feedback omits hold-out criterion results and hold-out-derived guidance.

SPEC_DEFECT never reaches this table: the implementer-reported path exits at 1e and the evaluator-flagged path exits at the end of 1g–1h, both routing the bean to `needs-attention` before convergence is checked.
