# 022 — An absent `--capability` resolves through the scheme

Status: accepted
Cites: fiddle_core::InvocationScheme, Selection::resolve

Amends nothing formally. It replaces an invariant that had never been written
down: *absent means `stub_mark`*, which two call sites in `fiddle-cli` implemented
and every capability ADR since M0 quietly assumed.

## Context

`fiddle run` and `fiddle inspect` both take `--capability <ID>`. Both resolved an
absent flag to M0's `stub_mark`, for every scheme, from M0 through M4a:

```rust
let selection = match capability {
    Some(requested) => Selection::parse(requested)?,
    None => Selection::Mark,
};
```

That was correct while `stub_mark` was the only capability, and it stayed
defensible through M1, M2 and M3, because each of those milestones is invoked over
a reference that names a work item and a caller who has a work item in hand has a
capability in mind. `--capability` was how they said which.

M4a broke the assumption, and nothing noticed for the length of the milestone.
[ADR 019](019-a-self-discovering-run-has-no-work-reference.md) gave `cve` a
reference that stands alone precisely because the orchestration discovers its own
work — so a caller has nothing in hand to say anything about, and `fiddle run cve`
is the whole invocation. Design §1 and design §6 both give it as
`fiddle run cve --mode unattended`, with no `--capability`, and that is what the
host workflow was to run nightly.

Typed at a shell, it did this:

```
$ fiddle run cve --mode unattended --config fiddle.toml
run cve
  outcome     = completed
  executed    = stub_mark completed (evidence stub:changes/cve.json)
  progress    = stub_mark/mark completed — wrote correlation marker c9c003885549069b
EXIT=0
```

**Worse than a skipped scan.** `stub_mark` wrote a correlation marker under this
reference's own slug, so the sweep was then *accounted for*:

```
$ fiddle inspect cve --capability cve_mitigate --config fiddle.toml
  changes     = marked c9c003885549069b (from stub:changes/cve.json)
  assessment  = satisfied (evidence stub:changes/cve.json)
  next action = complete
```

A host running the documented command nightly would report success having never
scanned. Thirty-four green beans and 963 passing tests did not see it because the
CVE acceptance suite reached the sweep through an explicit
`--capability cve_mitigate` — a flag that appears in no design section, no ADR and
no bean — so all 32 CVE lanes exercised the capability and none exercised the
invocation. The reference-to-capability seam belonged to no task.

Two ways to close it, and the losing one was chosen first. **Make the host pass
the flag and correct the documents** keeps this ADR unwritten and the default a
constant. It was rejected on the shape of what it asks an operator to type:
`fiddle run cve --capability cve_mitigate` states its subject twice, `cve` and
`cve_mitigate` being one fact spelled two ways, and requiring it would undo the
bare `fiddle run cve` that ADR 019 settled after rejecting three alternative
values. A milestone does not get to re-open a decision by way of a flag.

## Decision

**An absent `--capability` resolves through the reference's scheme.**

- `cve` selects `cve_mitigate`
- `beans`, `jira`, `scheduled` and `scanner` select `stub_mark`, exactly as before

`--capability` stays an explicit override for every scheme and every value; it
selects, and it is the only thing that overrides. Nothing about an unknown id
changes: it is a usage error listing what this build can run, never a silent
no-op.

Three properties keep this narrow:

- **One expression, two commands.** `run` and `inspect` resolve the default
  through a single function, `Selection::resolve`. The comment the old call sites
  carried — that a second spelling of the default "is exactly how the two commands
  would drift apart again" — is the reason, and it applies with more force now that
  the default is a rule rather than a constant.
- **The scheme match is exhaustive.** No `_ =>` arm. A scheme added later has a
  capability question to answer, and it is asked at the one site where the answer
  belongs; a wildcard would answer it silently with `stub_mark`, which is this
  defect one scheme along.
- **The reference is parsed before the flag is resolved.** It already was, for its
  own reason — a caller who mistyped an argument is told about the argument — and
  the default now depends on it.

## Consequences

**The rule a reader has to learn is longer than the one it replaces.** "Absent
means `stub_mark`" was one sentence and true everywhere; "absent means what the
scheme implies" cannot be stated without a table. Anyone who knew the old rule now
holds a belief that is false for exactly one scheme, and the two places that teach
it are `--help` and design §1, both updated with the change. This is the real cost
and it is accepted: the alternative was a documented invocation that ran the wrong
capability, which is a longer thing to learn the hard way.

**A scheme is now a claim about a capability.** Registering a scheme without
deciding its default does not compile. That is the property being bought — the
seam that belonged to no bean is now a match arm nobody can add a scheme past —
and it is also a new coupling between `fiddle-core`'s grammar and
`fiddle-cli`'s selection. The coupling lives entirely in the CLI: `InvocationScheme`
knows nothing about capabilities, and the mapping is not in the pure core.

**Inspect and run still agree, and that is asserted rather than argued.** They
share the resolution, so an unqualified `inspect cve` foresees `cve_mitigate` and
an unqualified `run cve` executes it. `inspect` takes the id only as far as the
derivation, so the change does not make the read-only command demand a scanner
credential — the property `capability_selection.rs` already pins for every value
of the flag now holds for every value of the default too.

**M0's invariant is now held by a test rather than by the shape of the code.**
While the default was a constant, "an unqualified run marks" was true by
construction. It is now true by a match arm, so it is asserted directly:
`a_reference_that_is_not_cve_still_selects_the_deterministic_capability` drives a
`beans` reference with no flag and reads `stub_mark` off the payload and the
marker off the fixture. M0's own acceptance lane is unmodified, which is the point
of that lane and the reason the guard was written beside the new behaviour rather
than into it.

**One M4a lane had to name what it used to default to.**
`a_run_over_a_trackerless_reference_is_not_a_failed_run` is about the *assessment*
of a reference naming no work item — upstream of every capability — and it reached
that assessment by running `cve` unqualified in an M0-shaped world. Under this
decision that world is asked for a scanner and a forge it does not describe, so the
lane now passes `--capability stub_mark`. Any lane that reads the default while
being about something else has the same fix, and the change makes them say so.

**A deployment scripting `fiddle run cve --capability cve_mitigate` keeps
working**, byte for byte. Nothing that was accepted becomes an error; one thing
that silently did the wrong work now does the right work.
