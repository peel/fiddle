# 035 — The purity boundary is enforced by the build, not by review

Status: accepted
Cites: crates/fiddle-acceptance/tests/crate_boundary.rs, crates/fiddle-acceptance/tests/harness_discipline.rs, support::fiddle_binary, FORBIDDEN, BANNED

## Context

`fiddle-core` is the pure domain, and must reach no process, file, socket, environment or clock. A reviewer can read an import list, and a reviewer cannot read a transitive dependency closure. Two boundaries in this workspace have the same shape, and both were held by habit.

## Decision

Hold each boundary with a test that fails the build. Walk `fiddle-core`'s full resolved closure with `cargo metadata`, and fail on `tokio`, `rig-core`, `rig-agent`, `reqwest`, `hyper` or `mio`. Grep its sources and fail on `std::process`, `std::fs`, `std::net`, `std::env`, `SystemTime::now` or `Instant::now`.

## Consequences

- `rig-agent` is named separately for a reason. rig 0.41 moved `Agent`, `AgentBuilder` and `AgentRun` out of `rig-core`. A denylist naming only `rig-core` would let the model client back in.
- The denylist asserts its own contents. `crate_boundary.rs` fails if `FORBIDDEN` loses `rig-core`, `rig-agent` or `tokio`, so a quiet deletion cannot pass as a clean walk.
- An acceptance lane resolves its binary through `support::fiddle_binary`, which builds it and takes the path cargo reports. `harness_discipline.rs` fails if any acceptance source names `cargo_bin`.
- The source grep matches a comment, and the `cargo_bin` guard does not. A commented-out `std::fs` call is evidence the boundary was once crossed, and a commented `cargo_bin` resolves no path.
- What was given up: the denylist is a list. It names what has been tried, not every effect that exists. A novel crate reaching the same effect passes until someone adds its name.

## What the two greps do with a comment

`crate_boundary.rs` reads each source whole and asks `contains`, so a banned name inside a comment fails the build. That is deliberate.

`harness_discipline.rs` strips comments and string literals before looking, and its own tests pin that: `/// cargo_bin`, `/* cargo_bin */` and `"cargo_bin"` are all expected to pass. Resolving a path by convention is the defect, and prose about it is not.

ADR 024 removes every comment from this tree, which would make the first grep's comment coverage moot. It is not moot yet: `crates/fiddle-runtime/src/effect/mod.rs` still carries two doc comments, so the coverage is still doing work.
