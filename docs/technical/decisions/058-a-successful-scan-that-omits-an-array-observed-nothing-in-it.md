# 058 — A successful scan that omits an array observed nothing in it

Status: accepted
Cites: Projection::succeeded, cve::project::succeeded, evaluate::RescanVerdict, Arm, a_successful_rescan_that_reports_no_arrays_at_all_is_proof

## Context

`rescan_verdict` returned `NotObserved` when either package array was absent. Wiz reports `null` for an array with no findings, and a distroless image reports `osPackages` null on every scan, including the first one.

So `RescanVerdict::Cleared` was unreachable for that deployment. No repair could be proved, whatever the agent did.

Run 32648667532 shows the cost. The agent made the correct minimal repair — `github.com/golang-jwt/jwt/v4` v4.5.0 to v4.5.2 in `go.mod` and `go.sum`, hashes matching. The rescan succeeded, examined the rebuilt image, and reported `libraries` null. The verdict was `needs_work`.

The first scan of the same image is the evidence for the convention. `osPackages` is null there too, and nothing reads that as an arm the scanner declined to examine.

## Decision

An absent array blocks proof only when the scan does not report success. `Projection` carries whether `status.state` is `SUCCESS`, and the verdict reads it.

`Arm` keeps saying what the document said. The array was absent. What changes is what fiddle concludes from it.

## Consequences

- A repair against an image with one reported arm can now be proved.
- `NotObserved` survives for the case it was written for: a document that does not declare success.
- A document with no `status` field counts as not successful, so fiddle refuses rather than infers. The scripted fixtures carry no status, so every test written before this record keeps its old result — which means those tests now pass for a reason they do not state. Tracked as `fiddle-ok47`.
- fiddle now depends on a third field of the scanner document. `ScanReport::findings` reads the arrays, the version parse reads `extraInfo`, and the verdict reads `status`. Each is a separate reader, and only the arrays were checked against a real document before this milestone.
