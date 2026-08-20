# 015 — M2 reaches GitHub through `gh api -i`

Date: 2026-08-10
Status: accepted
Cites: GhCli::api, GhCli::parse, GhCli::redact, GhError::outcome, GhError::Killed, GhError::Push, GitError::outcome, process::run_bounded, GitHub::cli, ensure_branch_published, crates/fiddle-runtime/src/github/checks.rs, scripts/live-github.sh

## Context

M2 gives fiddle its first capability that changes something outside this process. `publish_change` pushes a branch, opens a pull request and requests a check, each an HTTP request to GitHub. Two shapes were available: a native client inside `fiddle-runtime`, or the `gh` binary driven as a subprocess.

## Decision

Make `gh api -i` the GitHub adapter, and pass `-i` on every call. Read the HTTP status off the first line of stdout, and consult the exit code for three things only. Give the runtime the deadline, because `gh` has no timeout flag.

## Consequences

- `-i` is load-bearing, and nothing enforces it but this file and `GhCli::api`, which appends it so no caller can forget. A second call path that builds its own `Command` would break it. No test can catch a path that does not exist yet.
- The scripted stub is a fixture reached through a product seam. So the suite's GitHub coverage is exactly as good as that stub's fidelity. `github_cli.rs`, `effect_protocol.rs`, `pull_request_effect.rs`, `check_effect.rs` and `exactly_once.rs` all run against it. Every claim they make is about this adapter's reading of a response, never about GitHub.
- A `gh` that cannot be run at all is `Malformed`, not `Auth`. A configured program that is not on `PATH` fails at spawn. The run then reports the runner as wrong rather than the credential as absent.
- The project gave up typed failures. `gh` returns bytes on two streams. So redaction is a discipline held by hand at each site rather than a property of a type.
- One publication spawns ten children, and at M2's scale that is invisible beside a five-minute timeout.

## Why a subprocess

The usual argument for a CLI did not apply, and that is worth saying before the ones that did. `reqwest`, `hyper` and `hyper-rustls` are already in this workspace's resolved graph through `rig-core`, so "a CLI avoids a new dependency" would have been false. `fiddle-core`'s purity is untouched either way, because `crate_boundary.rs` walks that crate's closure and this code is not in it.

What did apply is that the credential is the asset. A subprocess is the only adapter whose whole view of the world fits on one screen: `crates/fiddle-runtime/src/github/cli.rs` builds a child from `env_clear` plus five names and nothing else. No equivalent statement can be made about a library sharing this process's address space, environment and TLS configuration. This project reached the same conclusion once before, for different reasons, in ADR 003.

## Why the status line and not the exit code

`gh help exit-codes` documents the whole set: 0 success, 1 any failure, 2 cancelled, 4 authentication required. A 404, a 422 and a 500 are all exit 1, so the status is not in the exit code, and an adapter that branched on exit 1 would read a surface that cannot answer the question.

The three things the exit code does report are authentication, cancellation, and a child that died rather than answered. `GhCli::parse` reads the status off stdout for everything else.

That is not bookkeeping. The whole of M2 rests on telling a refusal apart from a lost answer, and `GhError::outcome` draws that line per status: a 422 is `Unknown`, because one number covers "already exists" and "malformed"; a 5xx is `Unknown`; every other 4xx is `NotCommitted`. None of those three arms exists if nothing reads the status.

## The deadline and the seam

`[github] timeout`, default five minutes, bounds one round trip, and `process::run_bounded` enforces it with the process group and cancellation token the workspace runner already uses. This cost carries a rebate: a `gh` killed after it dispatched a request is a real ambiguous write rather than a simulated one, and `GhError::Killed` mapping to `EffectOutcome::Unknown` keeps it from being reported as a failure and retried into a duplicate.

The operator seam is `[github] cli = { program, args }`, defaulting to bare `gh`, exactly as `[workspace] check` already works. The deterministic suite substitutes a scripted `gh` there, behind the `gh-stub` feature, whose `required-features` keeps it out of `cargo build --release`. Nothing fake enters the product to make the suite possible.

## What this costs

**1. `gh`'s output can move under us, and the seam mitigates that without removing it.** `parse` splits the response on `\r\n\r\n`, falls back to `\n\n`, requires the first line to start with `HTTP/`, lowercases header names because HTTP/2 does, and accepts an empty body as `Null` because a workflow dispatch answers 204. Every one of those is a guess about a format nobody promised. A `gh` that reformats what `-i` prints makes this adapter report `Malformed`, classified `NotCommitted`, which is the safe direction and a stop rather than an adaptation. The seam lets an operator pin a known `gh`, which answers for a deployment and not for the project: the suite substitutes at that seam, so it proves this parser reads what the stub prints and can never prove the stub prints what `gh` prints. Only `scripts/live-github.sh` runs the parser against a real `gh`, and that lane does not gate.

**2. One operation is not `gh` at all.** A ref can only be created pointing at an object the remote already holds, so `ensure_branch_published` pushes objects and ref together with `git push`. `GhError` therefore carries `Push(#[from] GitError)`, and `GhError::outcome` delegates to `GitError::outcome` rather than mapping a push failure onto a status nobody sent. Fabricating a status was judged worse. The residue is a second spawn site with a second credential channel, seven names and no `HOME`, which this decision's one-screen argument has to be made twice for.

**3. Errors arrive as text, and text is where a credential leaks.** `stderr` is quoted in exactly one place, the arm with no HTTP status line, because that is the one failure an operator cannot diagnose without it. Everything else that could reach a log goes through `GhCli::redact` and a length bound first.

## What would reverse it

**A milestone that has to author a check run.** Only a GitHub App may create a check run, which is why M2 observes and dispatches but never publishes a check result. App authentication means signing a JWT with a private key and exchanging it for an installation token. `gh` does not do that, and a wrapper that did would hold the private key outside the one construction site this decision preserves. That trigger is already visible from here.

**A `gh` whose `-i` output changes shape.** The failure is loud, on the first call, and the immediate fix is `[github] cli.program`. The decision reverses only if pinning a version becomes the deployment's problem rather than an incident.

**Effect volume where a spawn per request costs something measurable.** Not M2's problem, and stated so the next person recognises it as this decision's bill.

Reversing does not mean deleting `gh`. `IntegrationOperation` is written against `inspect`, `apply` and `GhError`, not against a process, so a native client would replace `GhCli` and leave the executor, the identity derivation, the policy combination and all three operations untouched. That is why this is an adapter decision rather than an architectural one.

This supersedes no earlier ADR. It extends ADR 003's reasoning from an optional analysis provider to a credential-carrying adapter on the write path, where the argument is about blast radius rather than setup cost.
