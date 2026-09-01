# 022 — An absent `--capability` resolves through the scheme

Status: accepted
Cites: Selection::resolve, Selection::default_for, Selection::Toil, selected_workflow, SelectedWorkflow, build_capability, InvocationScheme, crates/fiddle-acceptance/tests/capability_selection.rs, crates/fiddle-acceptance/tests/cve_mitigation.rs::a_reference_that_is_not_cve_still_selects_the_deterministic_capability, crates/fiddle-acceptance/tests/cve_mitigation.rs::a_run_over_a_trackerless_reference_is_not_a_failed_run

It replaces an invariant nobody had written down: absent means `stub_mark`. Two call sites in `fiddle-cli` implemented it, and every capability ADR since M0 assumed it.

## Context

`fiddle run` and `fiddle inspect` both take `--capability <ID>`, and both resolved an absent flag to `stub_mark`. That stayed defensible through M1, M2 and M3, because each is invoked over a reference naming a work item. ADR 019 broke the assumption, and nothing noticed for the length of the milestone.

## Decision

Resolve an absent `--capability` through the reference's scheme. Keep `--capability` an explicit override for every scheme and every value. One row per scheme, and this table is the authority:

| scheme | absent `--capability` selects |
| --- | --- |
| `cve` | `cve_mitigate` |
| `jira` | `toil` — see the 2026-08-31 amendment; `stub_mark` as first accepted |
| `beans` | `stub_mark` |
| `scheduled` | `stub_mark` |
| `scanner` | `stub_mark` |

## Consequences

- The rule a reader has to learn is longer than the one it replaces. "Absent means `stub_mark`" was one sentence and true everywhere; "absent means what the scheme implies" needs a table.
- The project gave up the shorter rule. Anyone who knew it now holds a belief that is false for one scheme. `--help` and design §1 are the two places that teach the new one.
- A scheme is now a claim about a capability. Registering one without deciding its default does not compile, which is the property being bought.
- The coupling lives entirely in the CLI. `InvocationScheme` knows nothing about capabilities, and the mapping is not in the pure core.
- A deployment scripting `fiddle run cve --capability cve_mitigate` keeps working, byte for byte. One thing that silently did the wrong work now does the right work.

## What the assumption cost

`fiddle run cve --mode unattended` is how design §1 and design §6 both give the invocation, with no `--capability`, and it is what the host workflow was to run nightly. Typed at a shell it did this:

```
$ fiddle run cve --mode unattended --config fiddle.toml
run cve
  outcome     = completed
  executed    = stub_mark completed (evidence stub:changes/cve.json)
  progress    = stub_mark/mark completed — wrote correlation marker c9c003885549069b
EXIT=0
```

That is worse than a skipped scan, because `stub_mark` wrote a correlation marker under this reference's own slug and the sweep was then accounted for:

```
$ fiddle inspect cve --capability cve_mitigate --config fiddle.toml
  changes     = marked c9c003885549069b (from stub:changes/cve.json)
  assessment  = satisfied (evidence stub:changes/cve.json)
  next action = complete
```

A host running the documented command nightly would report success having never scanned. Thirty-four green beans and 963 passing tests did not see it, because the CVE acceptance suite reached the sweep through an explicit `--capability cve_mitigate`. That flag appears in no design section, no ADR and no bean, so all 32 CVE lanes exercised the capability and none exercised the invocation. The seam between a reference and a capability belonged to no task.

## Why the other closure lost

Making the host pass the flag, and correcting the documents, keeps this ADR unwritten and the default a constant. It was rejected on the shape of what it asks an operator to type. `fiddle run cve --capability cve_mitigate` states its subject twice, because `cve` and `cve_mitigate` are one fact in two spellings. Requiring it would undo the bare `fiddle run cve` that ADR 019 settled after rejecting three alternative values, and a milestone does not get to re-open a decision by way of a flag.

## Three properties keep this narrow

**One expression, two commands.** `run` and `inspect` resolve the default through `Selection::resolve`. The comment the old call sites carried said a second spelling of the default is exactly how the two commands would drift apart again, and that applies with more force now that the default is a rule rather than a constant.

**The scheme match is exhaustive.** `Selection::default_for` has no wildcard arm, so a scheme added later has a capability question to answer, asked at the one site where the answer belongs. A wildcard would answer it silently with `stub_mark`, which is this defect one scheme along.

**The reference is parsed before the flag is resolved.** It already was, so that a caller who mistyped an argument is told about the argument, and the default now depends on it.

## What the change touched

`inspect` and `run` share the resolution, so an unqualified `inspect cve` foresees `cve_mitigate` and an unqualified `run cve` executes it. `inspect` takes the id only as far as the derivation, so the change does not make the read-only command demand a scanner credential. The property `capability_selection.rs` pins for every value of the flag now holds for the default too.

M0's invariant was true by construction while the default was a constant, and is now true by a match arm, so `a_reference_that_is_not_cve_still_selects_the_deterministic_capability` asserts it directly. It drives a `beans` reference with no flag and reads `stub_mark` off the payload and the marker off the fixture. M0's own acceptance lane is unmodified, which is the point of that lane.

`a_run_over_a_trackerless_reference_is_not_a_failed_run` is about the assessment of a reference naming no work item, upstream of every capability, and it reached that assessment by running `cve` unqualified in an M0-shaped world. Under this decision that world is asked for a scanner and a forge it does not describe, so the lane now passes `--capability stub_mark`. Any lane that reads the default while being about something else has the same fix.


## Amended 2026-08-31: `jira` selects `toil`

M5c registered a sixth capability, `toil`, and changed the `jira` row of the
table in the Decision above. An absent `--capability` over a `jira` reference
selects `toil` and no longer selects `stub_mark`. Every other row is unchanged,
and the rule the record decided is unchanged: absent means what the scheme
implies. The table above carries the amended value; this section says what
changed and why.

`toil` is a workflow document rather than a Rust capability. It reads
`workflows/toil.toml`, resolved beside the configuration document, and it refuses
by naming that path when the document is not there. It never falls back to a
built-in capability, so the failure mode this record was written about cannot
recur through the new row: a deployment with no document is told so and exits 2.

The cost the record predicted is the cost paid. A lane that read the `jira`
default while being about something else now reads a different capability, and
the fix is the one this record already gives. Two `run` invocations pass
`--capability stub_mark` for that reason, both in the helper
`surfaces_of_every_jira_invocation` in
`crates/fiddle-acceptance/tests/jira_credential.rs`, which three tests share.

## Amended 2026-09-01: the document is read before any credential

`selected_workflow` reads and validates the selected workflow document in
`dispatch`, before the ports and before the forge credential resolve, and passes
the parsed document into `build_capability`. Reading it inside `build_capability`
put it after both, so an absent document over a deployment that had not exported
`FIDDLE_GITHUB_TOKEN` reported the absent credential and never named the path.
That is the naming half of this record's refusal weakening to whatever the
operator happened to have exported.

An unreadable document is a configuration error the operator fixes without any
credential, so reporting it first costs nothing an operator wanted. A credential
still gets named whenever the document is readable: with `workflows/toil.toml`
present, an absent `FIDDLE_GITHUB_TOKEN` reports `fiddle::config::credential_absent`
naming that variable, and an absent `JIRA_API_TOKEN` reports the same code naming
that one.
