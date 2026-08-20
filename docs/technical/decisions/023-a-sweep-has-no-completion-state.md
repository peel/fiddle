# 023 — A reference that names no work item has no completion state

Status: accepted
Cites: fiddle_core::assess, WorkStateView::has_completion_state, CapabilityAssessment

Amends [ADR 019](019-a-self-discovering-run-has-no-work-reference.md), which gave
`cve` a reference that stands alone and left open what its change set means. It
closes the half of [ADR 022](022-the-scheme-selects-the-capability.md)'s defect
that decision explicitly does not: 022 prevents new stub markers against a sweep's
reference and repairs no marker already on disk.

## Context

`fiddle_core::assess` reads a change set for the marker
`correlation_key(project, invocation_ref)` and calls the world `Satisfied` when it
finds it. Design §4.3's exactly-once rests on that: a second invocation over the
same work item recomputes the same key, recognises its own marker and completes
without doing the work twice.

**No capability enters the key.** It is derived from the project and the reference
and from nothing else, which is deliberate — the value has to be recomputable by a
later process on another machine, or the second-invocation proof is not checkable
— and it means every capability invoked over one reference writes the same
sixteen hex characters. ADR 022 has the transcript:

```
$ fiddle run cve --mode unattended --config fiddle.toml
  progress    = stub_mark/mark completed — wrote correlation marker c9c003885549069b

$ fiddle inspect cve --capability cve_mitigate --config fiddle.toml
  changes     = marked c9c003885549069b (from stub:changes/cve.json)
  assessment  = satisfied (evidence stub:changes/cve.json)
  next action = complete
```

M0's `stub_mark` marks a change set and scans nothing. The sweep's own marker over
`cve` is byte-identical to it, so the sweep read `Satisfied`, `derive_next`
returned `Complete` before the capability was consulted, and the run exited 0
reporting `completed` having never looked at the image.

ADR 022 stops the *accidental* route by resolving an absent `--capability` through
the scheme. It leaves three things standing:

- `fiddle run cve --capability stub_mark` is still a legal invocation and still
  writes that marker.
- A host that ran the documented command before 022 already has one on disk.
- The mechanism is general. A sweep's completion was evidenced by whatever the
  change port said about the reference, and any capability sharing a trackerless
  reference inherits it.

None of that is about the string `cve`. It is about what the reference *is*: a
reference that names no work item, because the orchestration behind it discovers
its own work — nobody files a ticket to ask a nightly job to look at a container
image. There is no work item whose completion could have been recorded, and a
marker on such a reference's change set therefore says only *some run wrote one*.
It cannot say which capability wrote it, and it cannot say whether the thing the
reference names — an image scanned — was ever done.

Three places the fix could go, and the losing two are instructive.

**Make the marker unwritable for such a reference.** Rejected: the marker is the
only local record that a run happened at all, and a run that recorded nothing is
one no later reader can see the shape of. The defect is in how the marker is
*read*, and removing the thing being misread throws away a diagnostic to avoid
correcting a rule.

**Put a precondition in the capability.** Rejected because it leaves the reading
wrong where an operator meets it. `derive_next` returns `Complete` before any
capability is consulted, so `fiddle inspect cve` would go on reporting
`satisfied` and `next action = complete` over an image nobody has scanned, and a
run would go on exiting 0 with the word `completed`. A capability that refused to
be skipped would also have to be written again, correctly, by every capability
that ever shares a trackerless reference.

## Decision

**Whether the change-set marker means "the work is done" is a property of the
reference, and a reference that names no work item has no completion state at
all.**

- `WorkStateView::has_completion_state` is that property, read off the work-item
  observation: `Observation::NotApplicable` and only that. A work item that
  *failed to read* still has one — a reference naming a tracker row does not stop
  naming one when the tracker is down — and keeping those apart is the distinction
  `assess` already holds two match arms for.
- `assess` branches on that predicate — it *calls* it rather than spelling the
  same condition out a second time as a pattern — and answers
  `CapabilityAssessment::NotStarted` whatever the change set carries. It does not
  read the marker, so no marker of any provenance can complete such an invocation.
  Sharing the one predicate with the outcome mapping below is deliberate: it is how
  the verdict and the outcome cannot come to disagree about which world a run was
  in, and it is what makes the rule falsifiable in one edit.
- The three-way marker rule of design §4.3 — absent is `NotStarted`, matching is
  `Satisfied`, differing is `Blocked` — is untouched for every reference that
  names a work item.
- An unreadable change set still `Blocked`s, for both kinds of reference. Nothing
  here relaxes fail-closed; a world fiddle did not see supports no conclusion
  about a sweep either.

**Such an invocation is idempotent by rescanning, not by remembering.** The second
night scans the image again, from scratch. What stops it doing the work twice is
design §4's dedup and nothing else: the commit-log read that finds the first
night's own `Fixes:` trailer and the open pull request it names, which is how a
second sweep reaches row 7 `already_in_progress` and lands nothing. That is why
the marker was never load-bearing for a sweep — the state that matters is on the
forge, where the work is, and it is read rather than remembered.

**A run over such a reference concludes from its execution.** `orchestration::run`
re-observes after executing and derives again, and for a reference with no
completion state that re-derivation is `Execute` — which is what having none
means, not evidence that the effect failed to survive. So `concluded` reads
`Execute` as `Completed` there, and the *outcome* rather than the next action is
what says the run finished.

## Consequences

**One bundle shape is new: `"outcome": "completed"` beside
`"next_action": {"execute": …}`.** For a sweep that is the truthful pair — this
run finished, and the reference is never done, because there is always another
night's scan. It is also the reading `fiddle inspect cve` gives before any run,
which was already true and is now true after one as well: the two commands agree,
which is the property they are supposed to have. The rule that `completed` can
never appear beside `blocked` is untouched.

**An automation that loops while `next_action` is not `complete` will loop on a
`cve` reference.** Accepted, and it is the cost of the decision rather than an
oversight: nothing in this build does that, the exit code an operator gates on is
derived from the outcome and is 0, and the alternative is a `complete` this world
cannot support. A supervisor for a self-discovering invocation is a schedule, not
a retry loop — which is what ADR 019 already implies by giving it no work
reference to make progress against.

**Anyone who knew that "a marker means the work is accounted for" now holds a
belief that is false for one shape of reference.** The rule is stated in three
places a reader actually arrives at: `assess`'s module header, the trackerless
branch itself, and `has_completion_state`. It is the second decision in two milestones
whose real cost is a longer rule — 022's was the same — and for the same reason:
the shorter rule was silently wrong about a documented invocation.

**The M4a acceptance suite stopped needing its workaround, which is the visible
proof.** `a_second_run_reads_the_first_runs_own_commit_body` deleted
`<stub.root>/changes/cve.json` between its two runs, through a helper called
`forget_that_the_last_run_happened`, because otherwise the second invocation
derived `Complete` and executed nothing. It now runs the second night with
*nothing* happening in between and asserts the first night's marker is still
sitting there. A suite that has to erase state to observe the behaviour it
documents was reporting the defect all along; the helper is gone and the marker is
asserted before and after instead.

**A capability sharing a trackerless reference inherits the rule rather than the
hole.** The predicate is on the view, so a scheme added later that discovers its
own work gets this behaviour without knowing it exists — and the pair of
decisions, `assess`'s branch and `concluded`'s, read the same predicate from the
same place, so they cannot come to disagree about which kind of reference a run is
under.

**One predicate with two readers means an inversion moves both, and that is a trap
for a test.** Flipping `has_completion_state` changes what a marker means and what
a run that already executed concluded, in one edit. That is the point — the two
derivations of a single run are supposed to move together, and the alternative is
the shape where a sweep reads `not_started` and still exits 11 — but the cost was
paid once and is worth recording. A test whose *premise* is "the reference has been
marked" must not establish that premise by running fiddle: such a run derives
through the very rule under test, so its outcome is not independent of the
mutation. While the condition had two spellings — the predicate, and an
`Observation::NotApplicable` pattern inside `assess` — inverting it moved only
`concluded`, turned the setup run into an exit-11 `Retryable`, and
`a_marker_against_a_trackerless_reference_does_not_account_the_sweep_as_done` went
red on its premise guard without ever reaching its claim. It now writes the marker
into the fixture directly; reachability of that world through the binary is
`a_run_over_a_trackerless_reference_is_not_a_failed_run`'s job, where no assessment
rule is under mutation.
