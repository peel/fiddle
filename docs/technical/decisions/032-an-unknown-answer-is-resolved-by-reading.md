# 032 — An unknown answer is resolved by reading, never by repeating the write

Status: accepted

Cites: fiddle_runtime::effect::Executor::execute, effect::ReadRetry,
read_until_settled, IntegrationOperation::apply, github::GhError::outcome,
git::GitError::Push, fiddle_runtime::process::run_bounded

## Context

A mutation whose answer was lost leaves fiddle unable to say whether GitHub acted.
Several ways of losing an answer wear a status code that looks decisive: a 422
covers malformed input, invalid ref syntax, spam protection and "already exists"
with one number. A read is idempotent and a lost write is not.

## Decision

Call `IntegrationOperation::apply` exactly once per `Executor::execute`, on every
path. Resolve an unknown outcome with `read_until_settled`, which is given nothing
that could dispatch a write. Classify an ambiguous answer as `Unknown` rather than
on its face.

## Consequences

**A 422 is never classified on its face.** `GhError::outcome` returns `Unknown` for
it, which forces the caller into the postcondition read that can tell a refusal
from a success. 5xx and a killed `gh` are `Unknown` for the neighbouring reason;
every other 4xx is `NotCommitted`.

**`GitError::Push` is `Unknown` too.** `git push --porcelain`'s `!` line is git's
per-ref refusal channel, and its *absence* is not a refusal.

**A lost answer is a lost answer whichever way the child died.**
`run_bounded`'s `select!` reaps both arms with the same SIGKILL after a successful
spawn, so a timeout and a cancellation are indistinguishable as to whether the
request reached GitHub. Only a cancellation refused *before* spawning is
`NotCommitted`, and the two are separate variants rather than one with two
producers.

**Exhausting the read budget returns the last observation unchanged.**
`EffectError::Unresolved` is its own leaf, never a degradation to `Committed` or
`NotCommitted`, because turning an absence into a success is how a duplicate
branch, pull request or run is born.

**What was given up: a retry that would often have worked.** Making the read and
the write symmetric is cheaper and is precisely the mistake. For most of M2 the
cancellation arm was classified as settled, `^C` during `POST .../pulls` skipped
the postcondition read, and a fresh process could dispatch a second workflow run.
Holistic review found it; the per-task harness could not, because it injected its
ambiguous write only as `exit(137)`.
