# 019 — A self-discovering run is referenced by its scheme alone

Status: accepted

Amends [011](011-invocation-reference-value-grammar.md), which stands.

## Context

`run` takes a required positional documented as "The work to run, as
`<scheme>:<value>`", and `FromStr` rejects an empty value with
`InvocationRefError::EmptyValue`. For `beans:fiddle-m0-demo` and `jira:ICE-1` the
value names *which piece of work*, and the ports are asked to read it.

M4's CVE orchestration is trackerless and discovers its own work: it scans the
project it was pointed at and remediates what it finds. There is no piece of work
to name. The reference workflow this milestone replaces builds one image from one
Dockerfile, and the skill it replaces describes "this repository's single
container image", so there is exactly one sweep per project.

Every candidate value was therefore either a restatement of something the
configuration already holds — `[project] name`, `[github] repo` — or invented.
Drafts of the M4 design tried three in turn: `scanner:<component>`, which put a
component where the scheme's value class is a finding id;
`scheduled:nightly`, which made fiddle model a trigger it neither controls nor can
verify while still hashing it into `effect_id`; and `cve:sweep`, which kept the
slot and filled it with a word carrying no information.

A restated value is not merely redundant. `effect_id` derives from
`(project, invocation_ref, kind, target)`, so an operator writing
`cve:icecube-prod` where another wrote `cve:icecube` computes a different identity
for the same work — a second branch and a second pull request from a difference
that means nothing.

This is also the second thing in this one orchestration for which the tracker-shaped
contract does not apply: `WorkStateView::work_item` is
`Observation::NotApplicable`, because a trackerless run has no work item to
observe. Two of them is the signal that the shape is wrong rather than that a
placeholder is needed.

## Decision

A scheme may stand alone as a complete invocation reference when its orchestration
discovers its own work. `fiddle run cve` parses, and the CVE orchestration is
invoked with no value.

Three properties keep this narrow:

- **Bare is per-scheme, not general.** `beans` and `jira` name a work item and
  still require a value; `scanner` names a finding and still requires one. `cve`
  admits both forms, and whether a scheme may stand alone is a property of the
  scheme rather than of the caller.
- **`cve:` remains an error.** A scheme with no colon is the bare form; a colon
  with nothing after it is still `EmptyValue`. The four rejection diagnostics stay
  pairwise distinct, which `inspect_ref.rs` asserts.
- **The slug of a bare reference is the scheme.** Artifacts land under
  `.fiddle/reports/cve/<attempt-id>/`. ADR 011 records that the slug is
  `<scheme>-<value>`; with no value there is no separator and no trailing one, and
  a valued reference cannot collide with a bare one because a present value is
  never empty.

The presence of a value carries meaning rather than decoration: it distinguishes
requirement 22's two arms. `cve` discovers its own findings; `cve:CVE-2026-1234`
remediates the one finding a caller handed in. The grammar states the difference,
so no sentinel word has to.

## Consequences

**ADR 011's threat model does not apply to a bare reference, and that is the
strongest argument for it.** 011 constrains the value because it arrives from
outside and is interpolated into every derived path. A bare reference has no
externally-supplied component, so there is nothing to sanitise — the vulnerability
class is absent rather than defended against. Every valued reference is still
validated exactly as 011 requires; this narrows what can carry an external string
rather than widening it.

`InvocationRef::as_str` documents itself as "the canonical `<scheme>:<value>`
text" that round-trips through `FromStr`. That contract widens to include the
valueless form, and the round trip must hold for it — `cve` renders as `cve` and
parses back, never as `cve:`.

Adding a value later to a deployment that runs the bare form changes its
`effect_id`. In practice the blast radius is bounded, because deduplication finds
an existing shared pull request by reading its `security/cve` label back from the
forge rather than by recomputing an identity; `effect_id` stops a *duplicate
effect*, while the label is what finds *existing work*. Those are different jobs
and earlier drafts of the M4 design conflated them. A deployment that ever needs
to sweep two components within one project will need a value, and adopting one is
a deliberate identity change rather than a transparent addition.

M7's Stabilize is trackerless and self-discovering in the same way and inherits
this decision rather than re-deciding it.
