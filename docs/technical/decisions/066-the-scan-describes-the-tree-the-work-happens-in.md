# 066 — The scan describes the tree the work happens in

Status: accepted
Cites: CveMitigate::sweep, baseline, observed_tree, commit_log_dedup, Run::unusable

## Context

The findings came from an image the host built from the dispatched ref. The workspace is created at the reused pull request's head whenever one is open. Nothing reconciled the two.

Run 32712406518 shows the cost.

| tree | `github.com/golang-jwt/jwt/v4` |
| --- | --- |
| `fiddle/cve-e2e-probe`, which the scanned image was built from | 4.5.0 |
| `security/cve-remediation-2026-08-23`, #254's head, where the workspace was created | 4.5.2 |

The brief said "CVE-2025-30204 in github.com/golang-jwt/jwt/v4 4.5.0, fixed in 4.5.2". The requirement was already at 4.5.2 in the tree the agent could read. It spent forty turns looking for work that did not exist, rearranging `require` lines, and reported nothing. Every check stayed green, which is ADR 065 holding.

The same transcript read `probe_cve.go` in its pre-`2c9e8e0e` form, which is independent proof of which tree was open.

## Decision

Scan the tree the work will happen in. `sweep` plans, counts, reads the forge, checks out and creates the workspace **before** it scans. The baseline runs first and its `docker build` produces the image from the working tree, so the scan that follows describes that tree.

An advisory the reparented tree has already fixed therefore never reaches the brief. It is not in the scan.

## Consequences

- A finding now describes the tree the agent reads. That was the whole defect.
- The two early exits no longer carry findings. A run that stops on the attempt bound, or because it could not read the pull request's checks, says nothing was built or scanned, because nothing was. Reporting findings from a tree nobody would touch was worse than reporting none.
- Those exits are now cheaper: they stop before a container build.
- The scan is coupled to the check list. It is the deployment's `docker build -t <image> .` that produces what `[orchestration.cve] image` names, and nothing checks that the two agree. A deployment that tags its build differently scans a stale image, and fiddle cannot tell.
- **The forge is now asked before the scan.** `an_unusable_scanner_exits_eleven_and_reaches_no_forge` recorded the opposite decision and its reason: a scan with no document reached no forge at all. The tree has to be chosen before it can be scanned, so a failed scan now costs two read-only label queries. The test is renamed and states the new order.
- **A failed scan now costs a worktree.** The same lane asserted that a scan with no document created none. It creates one, because the tree must exist to be scanned.
- `commit_log_dedup` runs against the same tree, and did not catch this case. Why it did not is unestablished and is not fixed here.

## What this record cannot prove

The offline harness cannot test this. Its scripted scanner returns a fixture document whichever tree it is pointed at, so it cannot express "the scan reflects what the workspace contains". No acceptance lane here fails if the ordering is reverted.

The evidence is the production run above, and the guard is the ordering in `sweep`. The stub now records its working directory, so the next diagnosis of this kind reads it rather than inferring it.
