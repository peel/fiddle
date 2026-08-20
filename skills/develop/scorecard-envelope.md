# Scorecard Envelope

The one envelope every evaluator emits and every grading script reads. When a brief,
a skill, or a provider prompt describes scorecard fields, it cites this file rather than
restating the shape — three restatements are how `criterion`/`met` came to be asked for
and `id`/`pass` came to be required.

## The envelope

```json
{
  "provider": "claude",
  "task_id": "fiddle-abcd",
  "iteration": 1,
  "timestamp": "2026-08-19T12:00:00Z",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": { "score": 8, "threshold": 7, "evidence": "ran scripts/gate.sh, 53 binaries" }
      }
    }
  },
  "criteria": [
    { "id": "c1", "pass": true, "evidence": "gate.sh:47 carries --no-fail-fast" }
  ],
  "antipatterns_detected": [],
  "spec_defect": null,
  "guidance": "",
  "dispatch_count": 1
}
```

## Field names are exact

`scripts/check-thresholds.sh` compares `score` against `threshold` and reads `pass` off each
criterion. It cannot infer a field it was not given, so it refuses the whole scorecard rather
than grading around a gap. These are the only accepted names:

| Where | Field | Type | Notes |
| --- | --- | --- | --- |
| top level | `provider` | non-empty string | names the evaluator that produced this card |
| top level | `domains` | object | keyed by domain name; a top-level domain key instead is refused |
| `domains.<d>` | `dimensions` | object | explicitly `{}` for an evidence-only evaluation; never absent |
| `domains.<d>.dimensions.<k>` | `score` | number | 1-10 integer, never a string |
| `domains.<d>.dimensions.<k>` | `threshold` | number | the domain template's default, or the bean's override |
| `domains.<d>.dimensions.<k>` | `evidence` | non-empty string | `comment` is accepted as an alias |
| top level | `criteria` | array | one entry per criterion in the bean's Evaluation block |
| `criteria[]` | `id` | string | matches the bean's criterion id exactly |
| `criteria[]` | `pass` | boolean | `true`/`false`, never `"true"` |
| `criteria[]` | `evidence` | non-empty string | the artifact and line behind the verdict |

Names that look right and are not: `criterion` for `id`, `met` or `passed` or `result` for
`pass`, `min` or `target` for `threshold`, `rating` for `score`. A card using any of them is
refused, not translated. **Translating an evaluator's fields by hand before grading is the
defect, not the workaround** — it puts the orchestrator in the position of reshaping evidence
until the grader accepts it, which is the position a grader exists to prevent. Re-dispatch the
evaluator with this file in its context instead.

`threshold` in particular has no safe default. A dimension that omits it once let a
5/6/6/6/9 scorecard clear 7/7/8/6/9, because a comparison against `null` is not a comparison.

## Optional fields

- `antipatterns_detected` — array, empty when none found. Entries are either an id string or `{"id", "severity", "evidence"}`. `check-thresholds.sh` carries them into its verdict as `findings`, which is what convergence compares when two iterations graded the same tree; an entry stating `severity: "low"` is excluded from that comparison and one stating no severity is not. See `skills/develop-loop/convergence-and-recovery.md`.
- `spec_defect` — `null`, absent, or `{"detected": true, "reason": "<non-empty>"}`. Flags
  spec-vs-reality, not implementation-vs-spec; see `skills/evaluate/SKILL.md`.
- `guidance` — actionable fix instructions; empty string when every dimension passes.
- `dispatch_count` — always `1`; the orchestrator accumulates.
- `task_id`, `iteration`, `timestamp` — carried for the eval log, not read by the graders.

## What each script reads

| Script | Reads | Refuses |
| --- | --- | --- |
| `scripts/validate-scorecard.sh` | one raw per-provider card, plus `--criteria-ids` | any field above missing or mistyped, criteria ids not matching the bean in both directions, empty evidence, a `spec_defect` with no reason |
| `scripts/merge-scorecards.sh` | a JSON array of validated cards | — |
| `scripts/check-thresholds.sh` | `--scorecard` the merged card, `--criteria` its graded `criteria` array, `--tree-sha` the tree graded | a dimension with no numeric `score` or `threshold`, a criterion with no string `id` or boolean `pass` |

`check-thresholds.sh` also stamps whatever `--tree-sha` it is given onto the verdict it emits, as
`tree_sha`. Nothing in the envelope carries it: the tree is a property of the checkout that was
graded, not of the card, and the verdict is where convergence reads it to tell an iteration that
judged new work from one that re-judged the same tree. See
`skills/develop-loop/convergence-and-recovery.md`.

`check-thresholds.sh` takes the criteria as a **separate file holding the bare array**, which is
the scorecard's own graded array, not the ungraded briefing array the evaluator was handed:

```bash
jq '.criteria' scorecard.json > criteria-graded.json
scripts/check-thresholds.sh --scorecard scorecard.json --criteria criteria-graded.json
```

Both scripts exit 2 on a card they cannot read, printing one line per problem naming the field
and the dimension or criterion it belongs to. Exit 2 is not a FAIL verdict: repair or
re-dispatch, and never feed it to `check-convergence.sh`.
