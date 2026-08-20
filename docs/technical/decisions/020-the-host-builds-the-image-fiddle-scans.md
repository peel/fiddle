# 020 — The host workflow builds the image, and fiddle pairs the digest with the revision

Status: accepted
Cites: TreeObservation::scanned_image_digest, TreeObservation::base_revision, Capability::tree_observation, ScanReport::image_digest, OrchestrationCve::image, crates/fiddle-acceptance/tests/cve_mitigation.rs::an_unusable_scanner_exits_eleven_and_reaches_no_forge, crates/fiddle-acceptance/tests/cve_mitigation.rs::a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch

## Context

Design §2.1's Prepare is a detached worktree at the observed revision, and then a `docker build`. Only the first half is implemented, because the plan's coverage table assigned the build half to no task. So the capability scans the configured image before a worktree exists, reversing the design's order. `[orchestration.cve] image` names that image.

## Decision

Build the image in the host workflow, not in fiddle. Scan an image the host built, and build none. Record both facts as one record, so the correspondence between the scanned image and the remediated tree is checkable.

## Consequences

- A bundle now answers which image the verdicts were measured against, and which tree was remediated. Neither half reached a durable artifact before.
- The record carries the digest, not the tag. A tag is a name whoever pushes next can move. Recording it would record the very thing that makes the question worth asking.
- The pair cannot be published apart. `TreeObservation` has no `Default` and one producer, and the sweep is entered only with a scan document. A run that made no tree publishes no `tree` key at all.
- This deliberately claims nothing about provenance. Fiddle did not build the image and cannot know it came from that revision. The record is a correspondence made checkable rather than a verified one.
- The project gave up the strong check, and now owes two halves rather than one omission. Every capability other than the sweep returns `None` from `Capability::tree_observation`, so no earlier bundle changes shape.

## Why fiddle does not build

The epic's hard constraint is that the gate is offline and credential-free, and a real build pulls base layers. The scripted-stub approach that carries `wizcli`, `gh`, `git` and `go` does not extend here, because a stubbed build yields an image whose digest means nothing, and the digest is the correspondence at issue. Building inside the capability would buy a connection between the tree and an artifact nobody ships.

Nothing connected the two facts before. `ScanReport::image_digest` was parsed by the `wizcli` adapter and read by nothing, so it died with the process. It is not what makes a re-scan comparable, because the rescan verdict compares the scanner version against the repair's recorded scan, never a digest. It reached no bundle, no receipt and no verdict report, so a clean result was attributable to a scanner and never to an image.

`CveMitigate::sweep` is the only place in the build holding both facts, because the scan happened before there was a tree and the checkout never sees a scanner. So the pair is assembled there or nowhere.

## The two alternatives that lost

**Require the configuration to declare the revision the image was built from, and refuse a run where that disagrees.** This is the strong version, and it turns an assumption into a checked precondition. It is not M4a's, for a reason about populating rather than checking: nothing in M4a writes such a field. A field no caller sets is either off by default, asserting nothing, or refuses every existing run. Both halves have to land together, and the populating half is a workflow step.

**Record the absence and assert nothing.** Honest but weaker than what was available. The digest was already parsed, so publishing it beside the revision costs one field and turns a fact that died in memory into one an auditor can read.

`an_unusable_scanner_exits_eleven_and_reaches_no_forge` asserts the absence of the half-pair, and `a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch` asserts the digest is the scan's resolution rather than the configured tag.

The host half of the strong check is a workflow step that builds at the checked-out revision and passes it to fiddle, which is M4b's scope. The fiddle half is a configuration field carrying the declared build revision, compared against the checkout's revision and refused on disagreement, and no milestone owns it today. `docs/BACKLOG.md`'s 2026-08-18 entry records that half, because neither this file nor the code can hold an unowned task.
