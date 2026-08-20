# 023 — A reference that names no work item has no completion state

Status: accepted
Amends 019, which gave `cve` a reference that stands alone and left open what its change set means.
Cites: WorkStateView::has_completion_state, fiddle_core::assess, fiddle_core::correlation_key, fiddle_core::derive_next, orchestration::concluded, Observation::NotApplicable, Reason::AlreadyInProgress, crates/fiddle-acceptance/tests/cve_mitigation.rs::a_marker_against_a_trackerless_reference_does_not_account_the_sweep_as_done

It closes the half of ADR 022's defect that decision explicitly does not: 022 prevents a new stub marker against a sweep's reference, and repairs no marker already on disk.

## Context

`fiddle_core::assess` reads a change set for `correlation_key(project, invocation_ref)`, and calls the world `Satisfied` when it finds it. Design §4.3's exactly-once rests on that, because a second invocation recomputes the key and recognises its own marker. No capability enters the key, so every capability over one reference writes the same sixteen hex characters.

## Decision

Make the meaning of a change-set marker a property of the reference. Give a reference that names no work item no completion state at all, read off `WorkStateView::has_completion_state`. Have `assess` call that predicate and answer `NotStarted` whatever the change set carries.

## Consequences

- One bundle shape is new: `"outcome": "completed"` beside `"next_action": {"execute": …}`. For a sweep that is the truthful pair, because this run finished and the reference is never done.
- An automation that loops while `next_action` is not `complete` will loop on a `cve` reference. That is the cost rather than an oversight. The exit code an operator gates on is 0, and the alternative is a `complete` this world cannot support.
- The project gave up the short rule again. A marker used to mean the work is accounted for. Anyone who knew that now holds a belief false for one shape of reference.
- A capability sharing a trackerless reference inherits the rule rather than the hole. The predicate is on the view, so a scheme added later gets this behaviour without knowing it exists.
- One predicate with two readers means an inversion moves both. That is the point, and the trap is that a test cannot establish its premise by running fiddle.

## What the marker could and could not say

ADR 022 has the transcript. M0's `stub_mark` marks a change set and scans nothing, and the sweep's own marker over `cve` is byte-identical to it. So the sweep read `Satisfied`, `derive_next` returned `Complete` before the capability was consulted, and the run exited 0 reporting `completed` having never looked at the image.

The key is derived from the project and the reference and nothing else, which is deliberate: a later process on another machine has to recompute it, or the second-invocation proof is not checkable.

ADR 022 stops the accidental route and leaves three things standing. `fiddle run cve --capability stub_mark` is still legal and still writes that marker. A host that ran the documented command before 022 already has one on disk. And the mechanism is general, because a sweep's completion was evidenced by whatever the change port said about the reference.

None of that is about the string `cve`. It is about what the reference is: nobody files a ticket to ask a nightly job to look at a container image. There is no work item whose completion could have been recorded, so a marker on such a reference's change set says only that some run wrote one. It cannot say which capability wrote it, and it cannot say whether an image was ever scanned.

## The rule and its edges

A work item that failed to read still has a completion state, because a reference naming a tracker row does not stop naming one when the tracker is down. Only `Observation::NotApplicable` removes it, and keeping those apart is the distinction `assess` already holds two match arms for.

`assess` calls the predicate rather than spelling the same condition out a second time as a pattern. Sharing one predicate with the outcome mapping is how the verdict and the outcome cannot come to disagree about which world a run was in, and it is what makes the rule falsifiable in one edit.

The three-way marker rule of design §4.3 is untouched for every reference that names a work item: absent is `NotStarted`, matching is `Satisfied`, differing is `Blocked`. An unreadable change set still blocks, for both kinds of reference, because a world fiddle did not see supports no conclusion about a sweep either.

**Such an invocation is idempotent by rescanning, not by remembering.** The second night scans the image again from scratch. What stops it doing the work twice is design §4's dedup: the commit-log read that finds the first night's own `Fixes:` trailer and the open pull request it names, which is how a second sweep reaches `Reason::AlreadyInProgress` and lands nothing. The state that matters is on the forge, where the work is, and it is read rather than remembered.

**A run over such a reference concludes from its execution.** `orchestration::run` re-observes after executing and derives again, and for a reference with no completion state that re-derivation is `Execute`. That is what having none means, not evidence that the effect failed to survive. So `concluded` reads `Execute` as `Completed` there, and the outcome rather than the next action says the run finished.

## Two places the fix could have gone

**Make the marker unwritable for such a reference.** Rejected, because the marker is the only local record that a run happened at all, and a run that recorded nothing is one no later reader can see the shape of. The defect is in how the marker is read.

**Put a precondition in the capability.** Rejected because it leaves the reading wrong where an operator meets it. `derive_next` returns `Complete` before any capability is consulted, so `fiddle inspect cve` would go on reporting `satisfied` over an image nobody has scanned. Every capability that ever shares a trackerless reference would also have to be written again, correctly.

## The suite stopped needing its workaround

An M4a lane deleted `<stub.root>/changes/cve.json` between its two runs, through a helper called `forget_that_the_last_run_happened`, because otherwise the second invocation derived `Complete` and executed nothing. A suite that has to erase state to observe the behaviour it documents was reporting the defect all along. The helper is gone. The lane, now `a_second_run_opens_no_second_pull_request`, runs the second night with nothing happening in between, and asserts the marker at the end.

**A test whose premise is "the reference has been marked" must not establish that premise by running fiddle.** Such a run derives through the very rule under test, so its outcome is not independent of the mutation. While the condition had two spellings, the predicate and an `Observation::NotApplicable` pattern inside `assess`, inverting it moved only `concluded`, turned the setup run into an exit-11 `Retryable`, and `a_marker_against_a_trackerless_reference_does_not_account_the_sweep_as_done` went red on its premise guard without reaching its claim. It now writes the marker into the fixture directly. Reachability of that world through the binary is `a_run_over_a_trackerless_reference_is_not_a_failed_run`'s job, where no assessment rule is under mutation.

The rule is stated in three places a reader arrives at: `assess`'s module header, the trackerless branch itself, and `has_completion_state`. ADR 024 has since removed the module header, so the two symbols carry it.
