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
| `domains.<d>` | `dimensions` | object | `{}` only on a card that declares `mode` `"evidence-only"`; never absent |
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

## The envelope as a response schema

`scripts/build-scorecard-schema.sh` emits this envelope as a JSON Schema, and
`hooks/dispatch-provider.sh` hands that schema to a provider CLI able to constrain its
reply to it. `orchestrate.json` decides which roles get one: `providers.<p>.schema_roles`
maps a role to a profile, and a role absent from the map answers in prose with no schema.

The schema is built per dispatch rather than kept as a fixed file. The provider enforces
strict structured output, where every object closes with `additionalProperties: false`
and lists all its keys in `required`, so an object keyed by domain name or by dimension
name cannot be expressed. The builder is told the domain and the dimension names and
closes those objects around them. Measured against codex 0.146.0 on 2026-08-27: a schema
whose `domains` was an open map was refused with HTTP 400 `invalid_json_schema`, and the
closed form was accepted and honoured for the scored, evidence-only and holistic profiles.

The builder refuses rather than guessing: an unknown profile, a missing `--domain` on the
`evaluator` profile, a `--dimensions` flag that is absent rather than empty, and a
dimension list containing spaces all exit 2 with the reason. Absent and empty are
different here for the same reason they are on `dimensions` itself — a dropped flag and a
deliberate evidence-only card must not produce the same schema.

The schema carries the exact field names, the 1-10 bounds on `score` and `threshold`,
non-empty `evidence`, and the `"mode": "evidence-only"` declaration on a card that scores
nothing. It does not retire `validate-scorecard.sh`. Only a provider dispatched through a
schema-carrying role is constrained; a claude evaluator subagent is not, and the schema
says nothing about whether a criterion id matches the bean.

## Optional fields

- `antipatterns_detected` — array, empty when none found. Entries are either an id string or `{"id", "severity", "evidence"}`. `check-thresholds.sh` carries them into its verdict as `findings`, which is what convergence compares when two iterations graded the same tree; an entry stating `severity: "low"` is excluded from that comparison and one stating no severity is not. See `skills/develop-loop/convergence-and-recovery.md`.
- `mode` — absent on a scored card, or exactly `"evidence-only"`. It declares that the card
  scored no dimensions on purpose. `check-thresholds.sh` and `validate-scorecard.sh` both refuse a
  card that scored none and does not declare it, and `check-convergence.sh` takes the single-pass
  evidence-only path only when the declaration is present. An empty `dimensions` on its own cannot
  be told from an evaluator that dropped its scores: in M5a a well-formed card carrying
  `"dimensions": {}` beside three valid criteria was accepted by all three tools, and the bean
  would have converged with no scores at all. Absent and empty are different, so the intent is
  declared rather than inferred.
- `spec_defect` — `null`, or `{"detected": true|false, "reason": "<non-empty when detected>"}`.
  Flags spec-vs-reality, not implementation-vs-spec; see `skills/evaluate/SKILL.md`. `null` is a
  statement: the evaluator looked and the spec is sound. Leaving the key out is not that statement,
  and the merge tells the two apart — see the `merge-scorecards.sh` row below. Emit the key.
- `guidance` — actionable fix instructions; empty string when every dimension passes.
- `dispatch_count` — always `1`; the orchestrator accumulates.
- `task_id`, `iteration`, `timestamp` — carried for the eval log, not read by the graders.

## What each script reads

| Script | Reads | Refuses |
| --- | --- | --- |
| `scripts/validate-scorecard.sh` | one raw per-provider card, plus `--criteria-ids` | any field above missing or mistyped, criteria ids not matching the bean in both directions, one id twice, empty evidence, a `spec_defect` with no reason, zero scored dimensions with no `mode` declaration |
| `scripts/merge-scorecards.sh` | a JSON array of validated cards | — |
| `scripts/check-thresholds.sh` | `--scorecard` the merged card, `--criteria` its graded `criteria` array, `--tree-sha` the tree graded | a dimension with no numeric `score` or `threshold`, a criterion with no string `id` or boolean `pass`, one id twice, zero dimensions and zero criteria, zero scored dimensions with no `mode` declaration |

`merge-scorecards.sh` emits `spec_defect` on every merged card, as one of three states, so a
dropped field cannot read as a clean evaluation:

| `.spec_defect` | when | carries |
| --- | --- | --- |
| `{"state": "detected", "detected": true, ...}` | any source card set `detected: true` | `reason` as `{domain}/{provider}: {reason}` per source, and `sources` |
| `{"state": "clear", "detected": false, ...}` | every source card carried the key and none flagged | `reported_by` |
| `{"state": "not_reported", ...}` | at least one source card left the key out, or carried a `spec_defect` whose `detected` is not a boolean | `missing_from`, and **no `detected` key** |

A `detected` source outranks a silent one, and `missing_from` still names the silent card. The
cross-domain merge in `skills/develop-loop/scorecard-merge.md` applies the same three states across
domains, naming a source by its domain. Before M5b the merge dropped `spec_defect` entirely, so
a flagged bean and a clean one produced the same `null`.

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

A card with nothing in it is a card they cannot read. `check-thresholds.sh` exits 2 with
`{"error": "scorecard has nothing to grade"}` when the merged card carries zero dimensions and
zero criteria, because a PASS there reports an evaluation that never ran — which is what
`{"domains":{},"criteria":[]}`, the merge product of a refused card, used to return. One criterion
id appearing twice is refused for the same reason: the grader counts every entry, so a duplicate
is counted twice and one verdict overwrites the other in the merge.
