# 025 — The agent owns ecosystem semantics; Rust owns the mechanical guarantees

Status: accepted

Amends the reasoning behind M4a's §2.4 bump-target rules. It does not supersede an
ADR: the position it corrects was never written down as a decision, which is part
of why it survived a whole milestone.

## Context

M4a shipped a CVE mitigation capability that **could not mitigate a Python
repository at all** — not less well, at all: 77 Go references in
`fiddle-runtime/src`, a whole `cve/go.rs`, and no ecosystem seam anywhere. The
cause was a reasoning error rather than an oversight. Deterministic version
arithmetic in Rust looked safer than model output, so the project's stated
principle — *agent when necessary, determinism where needed* — got applied with
**ecosystem semantics on the deterministic side**, and every rule that followed
was consistent with that reading. The rules were sound; the line was in the wrong
place.

## Decision

**Determinism keeps the mechanical guarantees. The agent takes the ecosystem
semantics.**

The agent decides which file fixes a finding, what version to move to, whether the
repository already carries the fix, and what it declines and why.

Rust keeps what never knew a language: the scanner adapter, the projection of a
report to six typed fields, the worktree, the attempt journal, cancellation, the
`[[workspace.checks]]` commands in document order, **the rescan**, and the forge
half — branch, one labelled pull request, effect identity, and pull-request dedup.

**Deletion, not abstraction.** A trait with one implementor that knows `go.mod` is
the failure this line exists to avoid, so roughly 2,280 lines went rather than
moved: `cve/attribute.rs`, `cve/version.rs`, `cve/go.rs`, `cve/group.rs`,
`cve/fold.rs`, the already-fixed computation in `cve/dedup.rs`, the forbidden-edit
half of `capability/cve.rs`, and the configuration key named `go`. Pull-request
dedup stayed, because "is there an open labelled pull request, and does this
branch's log already carry this change" is a git question.

## Consequences

**The claim is demonstrated rather than asserted.** `tests/fixtures/cve-{vulnerable,fixed}-py`
joins the Go pair — one changed line, `urllib3==2.0.4` to `urllib3==2.2.2` — and the
same acceptance lane passes against both **with zero changes to `crates/*/src`**.
The pair carries `requirements.txt` and **no lockfile**, where Go carries `go.mod`
plus `go.sum`, so it exercises the manifest-plus-lockfile assumption a word search
cannot see. A standing grep guard for `go.mod`, `go list`, `golang` was considered
and declined: word-absence is not assumption-absence, and a core that still assumes
a lockfile passes it. The one-time confirmation at completion provably cannot go
clean either — the tests that prove Go-ignorance must name Go words to assert their
absence, 11 residual hits by construction, at `capability/cve.rs:978` and `:1001-1006`.

**One exception stands, and it is not resolved here.** `names_a_fix`
(`crates/fiddle-runtime/src/cve/project.rs:197-202`) still has Rust decide that a
finding naming no published fix is `upstream_blocked`
(`cve/verdict.rs:328-334`) — so it never reaches the attempt and never reaches the
rescan. Design §2's own illustration of the agent's report is exactly that case
(`"note": "no published fix I can apply without a registry"`), so the example
chosen to explain the agent's contract describes a finding the agent never sees.
Whether "names no fixed version" is a projection fact or an ecosystem judgement is
open, tracked as `fiddle-lmqw`.

**The `already_fixed` field survives with a different producer.** Its two
computations are gone — `go list -m` and the commit-body reading — but the field in
`cve/verdict.rs` remains, now filled from a clean rescan over an attempt that
changed nothing (`capability/mitigate.rs:164-202`). Nothing pre-filters an
already-fixed finding: one that is fixed does not appear in the scan.

**A new capability must not re-derive the M4a line.** The principle does not say
which side a question falls on; that is the judgement, and this record is where it
was made. The test is whether a rule can be stated without naming an ecosystem.
