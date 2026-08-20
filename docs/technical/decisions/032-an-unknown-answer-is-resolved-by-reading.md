# 032 — An unknown answer is resolved by reading, never by repeating the write

Status: accepted
Cites: fiddle_runtime::effect::Executor::execute, effect::ReadRetry, read_until_settled, IntegrationOperation::apply, github::GhError::outcome, GhError::CancelledBeforeSpawn, GhError::CancelledAfterSpawn, git::GitError::Push, EffectError::Unresolved, fiddle_runtime::process::run_bounded

## Context

A mutation whose answer was lost leaves fiddle unable to say whether GitHub acted. Several ways of losing an answer wear a status code that looks decisive. A read is idempotent and a lost write is not.


## Decision

Call `IntegrationOperation::apply` exactly once per `Executor::execute`, on every path. Resolve an unknown outcome with `read_until_settled`, which is given nothing that could dispatch a write. Classify an ambiguous answer as `Unknown` rather than on its face.

## Consequences

- A 422 is never classified on its face. `GhError::outcome` returns `Unknown` for it, which forces the postcondition read that can tell a refusal from a success.
- `GitError::Push` is `Unknown` too. `git push --porcelain`'s `!` line is git's per-ref refusal channel, and its absence is not a refusal.
- A lost answer is a lost answer whichever way the child died. `run_bounded`'s `select!` reaps both arms with the same SIGKILL after a successful spawn.
- Exhausting the read budget returns the last observation unchanged. `EffectError::Unresolved` is its own leaf, because turning an absence into a success is how a duplicate is born.
- What was given up: a retry that would often have worked. Making the read and the write symmetric is cheaper, and it is precisely the mistake.

## The classification, and the two cancellations

A 5xx and a killed `gh` are `Unknown` for the neighbouring reason. Every other 4xx is `NotCommitted`.

Only a cancellation refused before spawning is `NotCommitted`. `GhError::CancelledBeforeSpawn` and `GhError::CancelledAfterSpawn` are separate variants rather than one with two producers, so a timeout and a post-spawn cancellation cannot be read as settled.

`EffectError::Unresolved` never degrades to `Committed` or `NotCommitted`. A duplicate branch, pull request or workflow run is what that degradation buys.

For most of M2 the cancellation arm was classified as settled. A `^C` during `POST .../pulls` skipped the postcondition read, and a fresh process could dispatch a second workflow run. Holistic review found it. The per-task harness could not, because it injected its ambiguous write only as `exit(137)`.
