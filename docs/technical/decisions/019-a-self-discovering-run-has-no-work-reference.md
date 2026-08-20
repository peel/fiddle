# 019 — A self-discovering run is referenced by its scheme alone

Status: accepted; amended in M4a by the note below, which leaves the decision standing and corrects what was read into it
Amends 011, which stands.
Cites: InvocationScheme::stands_alone, InvocationScheme::listed_naming_work, InvocationScheme::listed_standing_alone, InvocationRef::slug, InvocationRef::as_str, InvocationRefError::EmptyValue, InvocationRefError::Malformed, CliError::UnimplementedForm, Addressed::of, MitigateConfig, crates/fiddle-acceptance/tests/inspect_ref.rs

## Context

`run` takes a required positional documented as the work to run, as `<scheme>:<value>`, and `FromStr` rejects an empty value. For `beans:fiddle-m0-demo` and `jira:ICE-1` the value names which piece of work, and the ports read it. M4's CVE orchestration is trackerless and discovers its own work, so there is no piece of work to name.

## Decision

Let a scheme stand alone as a complete invocation reference when its orchestration discovers its own work. Parse `fiddle run cve`, and invoke the CVE orchestration with no value. Keep the presence of a value meaningful, so the grammar states the difference and no sentinel word has to.

## Consequences

- ADR 011's threat model does not apply to a bare reference, and that is the strongest argument for it. A bare reference has no externally-supplied component, so the vulnerability class is absent rather than defended against.
- Every valued reference is still validated exactly as 011 requires. This narrows what can carry an external string rather than widening it.
- `InvocationRef::as_str` promises canonical text that round-trips through `FromStr`, and that contract widens to the valueless form. `cve` renders as `cve` and parses back, never as `cve:`.
- The project gave up a stable identity across a later change. Adding a value to a deployment that runs the bare form changes its `effect_id`. So adopting one is a deliberate identity change.
- M7's Stabilize is trackerless in the same way and inherits this decision rather than re-deciding it.

## Three properties keep this narrow

**Bare is per-scheme, not general.** `beans` and `jira` name a work item and still require a value, and `scanner` names a finding and still requires one. Whether a scheme may stand alone is a property of the scheme, which `InvocationScheme::stands_alone` holds.

**`cve:` remains an error.** A scheme with no colon is the bare form, and a colon with nothing after it is still `EmptyValue`. The four rejection diagnostics stay pairwise distinct, which `inspect_ref.rs` asserts.

**The slug of a bare reference is the scheme.** Artifacts land under `.fiddle/reports/cve/<attempt-id>/`. ADR 011 records the slug as `<scheme>-<value>`; with no value there is no separator, and a valued reference cannot collide with a bare one because a present value is never empty.

## Why every candidate value was rejected

Each was a restatement of something the configuration already holds, or invented. Drafts tried three: `scanner:<component>`, which put a component where the scheme's value class is a finding id; `scheduled:nightly`, which made fiddle model a trigger it neither controls nor verifies while hashing it into `effect_id`; and `cve:sweep`, which kept the slot and filled it with a word carrying no information.

A restated value is not merely redundant. `effect_id` derives from the project, the reference, the kind and the target, so an operator writing `cve:icecube-prod` where another wrote `cve:icecube` computes a different identity for the same work. That is a second branch and a second pull request from a difference that means nothing.

This is also the second thing in this orchestration for which the tracker-shaped contract does not apply. `WorkStateView::work_item` is `Observation::NotApplicable`, because a trackerless run has no work item to observe. Two of them is the signal that the shape is wrong rather than that a placeholder is needed.

Deduplication bounds the blast radius of a later identity change, because it finds an existing shared pull request by reading its `security/cve` label back from the forge rather than by recomputing an identity. `effect_id` stops a duplicate effect; the label finds existing work. Those are different jobs, and earlier drafts of the M4 design conflated them.

## Amendment (M4a) — the valued form parses, and nothing acts on it

Everything above stands as grammar, and that is the half worth keeping. It is why `cve:` is still `EmptyValue`, why a bare reference's slug is the scheme, and why ADR 011 still validates every value that is present.

What does not hold is the implication the Decision invited, and which four operator-facing surfaces went on to state: that `cve:CVE-2026-1234` remediates one finding. Nothing in this build remediates one named finding. `MitigateConfig` declares no advisory field and the capability scans `[orchestration.cve] image` alone. So a run over the valued form either blocked on a work-item read with no source, exiting 20 and publishing a bundle under `cve-CVE-2026-1234/`, or, handed a stub work file, swept the whole image while deriving `effect_id` from the narrowed reference. The second is the duplicate-effect hazard the Context names, arriving by the other door.

So the CLI refuses `<scheme>:<value>` for any scheme that stands alone, on the invalid-input row, before it opens the configuration document. The refusal names the reference and gives the one invocation this build implements.

```
$ fiddle run cve:CVE-2026-1234
  × `cve:CVE-2026-1234` is not implemented in this build
  help: `cve` discovers its own work; write `cve` to sweep what the configuration names
```

`inspect` refuses it identically, and that is deliberate. `inspect` is read-only and credential-free for every input and stays so, because the refusal is reached before the document is read. Its purpose is to say what a run would do, so a build where `run` refuses a reference while `inspect` reports a plan for it would have the read-only command describing work the binary cannot do. Reporting a diagnostic is a thing a read-only command may always do; reporting a plan nobody can execute is not.

Narrowing was the alternative, and it is M4b's scope. It needs an `effect_id` story of its own, because an identity derived from a narrowed reference must not collide with the sweep's. An accepted-but-not-implemented disclosure in the manner of `max_capability_attempts` was also rejected, because a configuration bound that is unenforced is not the same risk as an invocation that would silently act on the wrong scope.

Kept deliberately, so the milestone that implements narrowing has something to build on: the `Cve` scheme still parses a value, `cve:` is still an error, the bare slug is still the scheme, and `Addressed::of` still decides on the value rather than on the scheme.

**How the surfaces came to advertise it.** Two were introduced by the commit that corrected the help text under the subject "help text describes the grammar the binary has, not the one it had", and it described a grammar the binary does not have yet. The lane that should have caught it made the same mistake in test form, asserting that the `cve:` diagnostic contained `cve:<identifier>` and calling that the valued form. It pinned the sentence rather than the behaviour, so it could not notice the sentence was false. What replaces it reads `--help` and each diagnostic off the compiled binary: `no_operator_facing_surface_promises_the_valued_form`, beside `the_valued_form_of_a_self_discovering_scheme_is_refused`.

**A fifth surface was outside that lane's reach.** A doc comment in `crates/fiddle-core/src/identity.rs` also stated the valued form as present fact, and a lane reading the compiled binary cannot see source prose. Neither mechanical guard holds it: rustdoc strips `#[cfg(test)]` before collecting doctests, and the sentence stood at five sites when it was found, four of them legitimately framed as history. So a source comment contradicting the binary was held by review alone.

**A sixth was the same class pointing the other way.** Every surface counted above promised the valued form. `InvocationRefError::Malformed` denied the bare one, reading "invocation reference must be `<scheme>:<value>`" and offering a colon and a valued example. That is a grammar `fiddle run cve` is excluded from, and it is where a mistyped `cve` arrives, because `cvfoo` carries no separator. So the caller this milestone exists for was the caller the sentence was most wrong for. The lane above passed over it necessarily, because it searches for `cve:` carrying a value, and text saying `cve` requires a value never writes `cve:`. Both strings now come from `InvocationScheme::stands_alone`'s two halves through one function, so the two doors a caller can arrive by cannot describe different grammars.

**What the lane holds is detectability, not accuracy.** It establishes that every standing-alone scheme is named wherever the grammar is described, which makes a wrong description findable. It does not establish that the description is right, and no acceptance lane can: a lane can assert that a string is present, absent or shaped a certain way, and cannot assert that a sentence means what it should. Measured: `Malformed`'s message rewritten to name `cve` while still being false of it leaves the whole file green. So prose in a diagnostic contradicting the binary is held by review here, exactly as a source comment is. `docs/BACKLOG.md` carries both measurements.

ADR 024 has since removed every comment from this tree, including the lane's own. The bound above is now recorded here and in `docs/BACKLOG.md` alone.
