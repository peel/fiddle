# 015 — M2 reaches GitHub through `gh api -i`, not through a REST client

**Date:** 2026-08-10
**Status:** accepted

## Context

M2 gives fiddle its first capability that changes something outside this
process: `publish_change` pushes a branch, opens a pull request, and requests a
check. Every one of those is an HTTP request to GitHub, and something has to
make it.

Two shapes were available. A native client — `reqwest` or an SDK — built inside
`fiddle-runtime`, or the `gh` binary driven as a subprocess. **The usual
argument for the second did not apply here**, and it is worth saying so before
the ones that did: `reqwest`, `hyper` and `hyper-rustls` are already in this
workspace's resolved graph by way of `rig-core`, so "a CLI avoids a new
dependency" would have been false. `fiddle-core`'s purity is untouched either
way; `crate_boundary.rs` walks *that* crate's closure and this code is not in
it.

What did apply is that the credential is the asset, and a subprocess is the only
adapter whose entire view of the world can be enumerated in one screen.
`crates/fiddle-runtime/src/github/cli.rs` builds a child from `env_clear` plus
five names and nothing else; there is no equivalent statement to make about a
library that shares this process's address space, its environment and its TLS
configuration. This project already reached the same conclusion once, for
different reasons, in `003-cli-only-providers.md`.

## Decision

**`gh api -i` is the GitHub adapter, and `-i` is not a preference.**

`gh help exit-codes` documents the whole set: **0** success, **1** any failure,
**2** cancelled, **4** authentication required. A 404, a 422 and a 500 are all
exit **1**. The HTTP status is therefore simply not in the exit code, and an
adapter that branched on exit 1 would be reading a surface that cannot answer
the question. `GhCli::api` passes `-i` on every call without exception, and
`GhCli::parse` consults the exit code only for the three things it does report —
authentication (4), cancellation (2), and the child having died rather than
answered (no code at all, or a code at or above 128) — then reads the status off
the first line of stdout for everything else.

That is not bookkeeping. The whole of M2 rests on telling a refusal apart from a
lost answer, and `GhError::outcome` makes that distinction per status: a 422 is
`Unknown` because it covers "already exists" and "malformed" with one number, a
5xx is `Unknown`, every other 4xx is `NotCommitted`. None of those three arms
exists if the status is not read.

**The runtime owns the deadline, because `gh` has no timeout flag.**
`[github] timeout` (default 5m) bounds one round trip, and
`crates/fiddle-runtime/src/process.rs::run_bounded` enforces it with the process
group and the cancellation token the workspace runner already uses. This is a
cost with a rebate attached: a `gh` killed after it has dispatched a request is
a *real* ambiguous write rather than a simulated one, and `GhError::Killed` →
`EffectOutcome::Unknown` is what keeps it from being reported as a failure and
retried into a duplicate.

**The operator seam is `[github] cli = { program, args }`**, defaulting to bare
`gh`, exactly as `[workspace] check = { program, args }` already works. The
deterministic suite substitutes a scripted `gh` there — `gh_stub`, a `[[bin]]`
behind the `gh-stub` feature whose `required-features` keeps it out of
`cargo build --release`. Nothing fake enters the product to make the suite
possible.

## What this costs

Written down because a decision whose price is not recorded reads later as a
decision that had none.

**1. `gh`'s output is a surface that can move under us, and the seam mitigates
it without removing it.** `parse` splits the response on `\r\n\r\n`, falls back
to `\n\n` for a `gh` that normalised its line endings, requires the first line
to start with `HTTP/` before it will believe a status, lowercases header names
because HTTP/2 does, and accepts an empty body as `Null` because a
`workflow_dispatch` answers 204. Every one of those lines is a guess about a
format nobody promised us. If a future `gh` reorders or reformats what `-i`
prints, this adapter reports `GhError::Malformed` — classified `NotCommitted`,
which is the safe direction, but a run that stops rather than one that adapts.
`program` and `args` let an operator pin a known `gh` or put a wrapper in front
of it, which is a real answer for a deployment. It is not an answer for the
project: **the deterministic suite substitutes at that seam, so it proves this
parser reads what the stub prints, and can never prove the stub prints what `gh`
prints.** Only `scripts/live-github.sh` runs the parser against a real `gh`, and
that lane does not gate.

**2. One operation is not `gh` at all.** A ref can only be created pointing at
an object the remote already holds, so `ensure_branch_published` pushes objects
and ref together with `git push` and has no `POST /git/refs` to fail instead.
`GhError` therefore carries `Push(#[from] GitError)`, additively to the shape M2's
contracts pinned, and `GhError::outcome` **delegates** to `GitError::outcome`
rather than mapping a push failure onto an HTTP status nobody sent. The
alternative — fabricating a status — was judged worse. The residue is a second
spawn site with a second credential channel (`GIT_CONFIG_*`, seven names, no
`HOME`) that this ADR's "one screen" argument has to be made twice for, and one
stale sentence: `GhError`'s own doc still opens "Everything a `gh` invocation can
fail as", which the `Push` variant made one short of true. The variant documents
itself at length immediately below it; the summary line above it did not follow.

**3. A process per request.** Three effects, each inspected before *and* after its
mutation, is six `gh` reads plus two `gh` writes plus a `git push` and a `git
rev-parse` — ten children for one publication, before the postcondition read
spends any of its `[github] read_retry` budget. At M2's
one-capability-per-run scale this is invisible beside a five-minute timeout. It
is the first thing that stops being invisible if a later milestone publishes in
volume.

**4. Errors arrive as text, and text is where a credential leaks.** A library
returns typed failures; `gh` returns bytes on two streams. `stderr` is quoted in
exactly one place — the "no HTTP status line" arm, because that is the one
failure an operator cannot diagnose without it — and everything that could reach
a log goes through `GhCli::redact` and a length bound first. That is a
discipline held by hand at each site rather than a property of the type.

## What would reverse it

**A milestone that has to author a check run.** Only GitHub Apps may create
check runs, which is why M2 observes and dispatches but never publishes a check
result (`crates/fiddle-runtime/src/github/checks.rs`). App authentication means
signing a JWT with a private key and exchanging it for an installation token.
`gh` does not do that, and a wrapper that did would be holding the private key
outside the one construction site this decision exists to preserve. That is the
single most likely trigger, and it is already visible from here.

**A `gh` whose `-i` output shape changes.** The failure is loud — `Malformed`,
on the first call — and the immediate fix is `[github] cli.program`. The
decision only reverses if it happens often enough that pinning a version becomes
the deployment's problem rather than an incident.

**Effect volume where a spawn per request costs something measurable.** Not M2's
problem; stated so the next person recognises it as this decision's bill rather
than as a new one.

Reversing does *not* mean deleting `gh`. `IntegrationOperation` is written
against `inspect`/`apply` and `GhError`, not against a process — a native client
would replace `GhCli` and leave the executor, the identity derivation, the
policy combination and all three operations' logic untouched. That is the shape
of the seam, and it is why this is an adapter decision rather than an
architectural one.

## Consequences

**`-i` is load-bearing and nothing enforces it but this file and one method.**
Every call goes through `GhCli::api`, which appends `-i` itself, so no caller can
forget. The way this breaks is a *second* call path being added later that
builds its own `Command`; there is no test that would catch that, because a test
can only assert about the paths that exist.

**The scripted stub is a fixture reached through a product seam, and the suite's
GitHub coverage is exactly as good as that stub's fidelity.** `github_cli` (14
tests), `effect_protocol` (40), `pull_request_effect` (10), `check_effect` (14)
and `exactly_once` (5) all run against it. Every claim they make is a claim about
this adapter's *interpretation* of a response, which is the half worth gating;
none of them is a claim about GitHub.

**A `gh` that cannot be run at all is `Malformed`, not `Auth`.** A configured
`program` that is not on `PATH` fails at spawn and is reported as the runner
being wrong rather than the credential being absent, which is the correct reading
and an unobvious one. The two arms name different things because they can: the
spawn failure names the program and the OS error, since there is no child and
therefore no `stderr`; the "answered, but not with a status line" arm is the one
that quotes `stderr`, because a `program` that *is* a program but is not `gh`
usually says so on the other stream.

This supersedes no earlier ADR. It extends `003-cli-only-providers.md`'s reasoning
from optional analysis providers to a credential-carrying adapter on the write
path, where the argument is about blast radius rather than about setup cost.
