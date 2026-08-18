# 020 — The host workflow builds the image fiddle scans, and fiddle pairs the digest with the revision

Status: accepted

## Context

Design §2.1's Prepare is *a detached worktree at the observed revision, created per attempt and removed by the `Drop` guard, then `docker build`*. Only the first half is implemented. The plan's own spec-coverage table maps §2.1 to Task 5b — "allowlist unchanged, measured" — so the build half was never assigned to any task; it is a plan-coverage gap rather than an implementation slip.

The consequence is an ordering, and it is the reverse of the design's. `CveMitigate::execute` scans the statically configured `[orchestration.cve] image` and only then enters `sweep`, which is where the checkout is resolved and the worktree made — so the scan runs **before a worktree exists**, where §2.1's Prepare precedes §2.2's Scan on the page. The document every verdict in the run is measured against therefore describes whatever image currently carries that tag, not the tree being remediated.

Nothing connected the two. `ScanReport::image_digest` was parsed by the `wizcli` adapter and read by nothing at all: it died with the process, and neither of the two jobs Design §2.2 gives it was being done. It is not what makes the re-scan comparable — `evaluate`'s rescan verdict compares `scanner_version` against `Repair::scanned_at` and never a digest — and it reached no bundle, no receipt and no verdict report, so a clean result was attributable to a scanner but never to an image.

## Decision

**`docker build` happens in the host workflow, not in fiddle.** Fiddle scans an image the host built, and does not build it.

This is not only the chosen answer, it is the defensible one. The epic's hard constraint is that the gate is offline and credential-free; a real build pulls base layers. The scripted-stub approach that carries `wizcli`, `gh`, `git` and `go` does not extend here, because a stubbed `docker build` yields an image whose digest means nothing — which is precisely the correspondence at issue. Building inside the capability would buy a connection between the tree and an artefact that is not the artefact anybody ships.

Fiddle's half of the contract is therefore to make the correspondence **checkable**, and it does that by recording both facts as one record. `TreeObservation` gains a fourth key, `scanned_image_digest`, published in the bundle beside `base_revision`, `pr_head` and `attempt_tree`. `CveMitigate::sweep` is the only place in the build where both facts are in hand — the scan happened in `execute`, before there was a tree, and `Checkout` never sees a scanner — so the pair is assembled there or nowhere.

Three alternatives were weighed.

**Build the image in fiddle.** Rejected above: it is the offline gate's constraint, not a preference.

**Require the configuration to declare the revision the image was built from, and refuse a run where that disagrees with the revision about to be remediated.** This is the strong version, and it turns an assumption into a checked precondition. It is not M4a's, for a reason that is about populating rather than about checking: nothing in M4a writes such a field. A config field no caller sets is either off by default — no assertion at all — or refuses every existing run. Its two halves have to land together, and the half that populates it is the workflow step, which is M4b's. See Consequences.

**Record the absence and assert nothing.** Honest but weaker than what is available. The digest was already being parsed; publishing it beside the revision costs one field and turns a fact that died in memory into one an auditor can read.

## Consequences

A bundle now answers *which image were these verdicts measured against, and which tree was remediated?* — checkable by the workflow that did the build, or by a person, where before neither half was in any durable artefact. The digest, not the tag: a tag is a name whoever pushes next can move, and recording it would record the very thing that makes the question worth asking.

The pair cannot be published apart. `TreeObservation` has no `Default` and one producer, so no run records a revision whose image is unknown; and `sweep` is entered only with a scan document, so a run that scanned and made no tree publishes no `tree` key at all rather than a half-pair. `an_unusable_scanner_exits_eleven_and_reaches_no_forge` asserts that absence, and `a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch` asserts the digest is the scan's resolution and not the configured tag.

**What this deliberately does not claim.** That the image was built from that revision. Fiddle did not build it and cannot know. The record is a correspondence made checkable, not a verified one, and the doc comments on `TreeObservation::scanned_image_digest` and `observed_tree` say so at the sites, so a later reader cannot mistake the pair for provenance.

The bundle's `observations.tree` object gains a key. Every capability other than `CveMitigate` returns `None` from `Capability::tree_observation` and carries no `tree` key at all, so no M0, M1, M2 or M3 bundle changes shape.

**The strong check is still owed, and it is now two owed halves rather than an omission.** The host half — a workflow step that builds at the checked-out revision and passes it to fiddle — is M4b's, whose scope is the release artefact, the workflow in `snowplow-incubator/snowplow-identities` and the first real Wiz measurement. The fiddle half — a configuration field carrying the declared build revision, compared against `checkout.revision()` and refused on disagreement — is in no milestone's scope today, because M4b is the host side. `docs/BACKLOG.md`'s 2026-08-18 entry *"Verifying that the scanned image was built from the remediated tree needs a config field no milestone owns"* records that half, since neither this file nor the code can hold an unowned task.
