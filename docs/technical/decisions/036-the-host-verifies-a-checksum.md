# 036 — The host verifies a checksum twice before it runs the binary

Status: accepted
Cites: FIDDLE_TAG, FIDDLE_SHA256, fiddle-linux-amd64.sha256, sha256sum

## Context

A scheduled job downloads a fiddle release and runs it, and no person watches the run. A tag is a name, and a name can move to another commit. The release publishes the binary and a checksum file beside it.

## Decision

Compare the measured digest against two values before `chmod +x`. Compare it against the published `fiddle-linux-amd64.sha256`, and against `FIDDLE_SHA256`, a digest the host workflow pins. Fail the job on either mismatch, and move `FIDDLE_TAG` and `FIDDLE_SHA256` together.

## Consequences

- The published checksum cannot detect a re-cut tag. A re-cut tag republishes the binary and its sidecar together. The sidecar then agrees with the new binary, so a sidecar-only comparison passes and reports nothing.
- The stated rationale did not hold for the chosen mechanism. This project recommended a tag and a checksum. It justified the pair by saying a re-cut tag must not silently change a scheduled run. Only the pinned digest does that work. The justification began to hold when the second comparison was added.
- The sidecar comparison still earns its place. It catches a truncated download, and it catches a mismatched pair of assets.
- Both comparisons run before `chmod +x`. The step that downloads never executes what it downloaded.
- What was given up: a pinned digest is manual work. Every release needs an edit in the host workflow, and `FIDDLE_SHA256` holds a placeholder that no digest matches. A workflow applied without that edit fails on its first run.
