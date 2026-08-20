# 035 — The purity boundary is enforced by the build, not by review

Status: accepted

Cites: crates/fiddle-acceptance/tests/crate_boundary.rs,
crates/fiddle-acceptance/tests/harness_discipline.rs, support::fiddle_binary

## Context

`fiddle-core` is the pure domain and must reach no process, file, socket,
environment or clock. A reviewer can read an import list; a reviewer cannot read a
transitive dependency closure. Two boundaries in this workspace have the same
shape, and both were held by habit.

## Decision

Hold each boundary with a test that fails the build. Walk `fiddle-core`'s full
resolved closure with `cargo metadata` and fail on `tokio`, `rig-core`,
`rig-agent`, `reqwest`, `hyper` or `mio`. Grep its sources and fail on
`std::process`, `std::fs`, `std::net`, `std::env`, `SystemTime::now` or
`Instant::now`, comments included.

## Consequences

**`rig-agent` is named separately for a reason.** rig 0.41 moved `Agent`,
`AgentBuilder` and `AgentRun` out of `rig-core`, so a denylist naming only the
latter would let the model client back in.

**An acceptance lane resolves its binary through `support::fiddle_binary()`**, which
builds it and takes the path cargo reports. `harness_discipline.rs` fails if any
acceptance source names `cargo_bin`, because resolving a path by convention
silently tests whatever the last build left.

**The grep matches comments deliberately.** A commented-out `std::fs` call is a
reader's evidence that the boundary was once crossed, and this repository carries no
comments anyway (ADR 024).

**What was given up: the denylist is a list.** It names what has been tried, not
every effect that exists, so a novel crate reaching the same effect passes the walk
until someone adds its name.
