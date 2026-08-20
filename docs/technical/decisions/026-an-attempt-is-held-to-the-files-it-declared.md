# 026 — An attempt is held to the files it declared, and to nothing else

Status: accepted

## Context

M4a refused an edit for **what the file was**: `ForbiddenShape`, `ASSERTIONS`, the
`t.Skip` spellings, assertion-keyword counting, `is_go`, `is_go_test`, `is_go_mod`
and `replaces` read Go syntax to decide whether a change was legitimate. That is
ecosystem semantics on the Rust side of the line [ADR 025](025-the-agent-owns-ecosystem-semantics.md)
moves, so it is deleted rather than generalised. Something still has to hold the
attempt to a boundary, and the attempt already reports `changed_files` — a field
M1 asked for and nothing verified.

## Decision

**The only rule is that the diff touched exactly `changed_files`.** `undeclared`
(`capability/cve.rs:113-131`) compares the declared paths against the diff's paths
set-wise: a path changed and undeclared is a `DeclarationBreach`, a path declared
and unchanged is a `DeclarationBreach`, and a breach makes the attempt needs-work.
Rust does not know what any of those paths mean.

**One exclusion, and without it the rule refuses every run.** A sweep applies its
own bump *before* the model is briefed, so the worktree is already dirty when the
attempt begins. Paths the run edited itself are excused, and nothing beside them
is; the honest report of a bump needing no further work is therefore an **empty**
`changed_files`. The lane holding this puts both kinds of path in one diff, so an
exclusion widened later to cover the whole diff fails it.

## Consequences

**"Don't silence the tests" stops being a Rust guarantee.** The declared-files rule
permits a **declared** test-file edit, and there is no longer any code that could
object. The guarantee survives only where a deployment declares a test check in
`[[workspace.checks]]` — it is a deployment's responsibility now, not the
product's, and the manual has to say so. This is a real loss and it is what the
deletion costs.

**The check is post-hoc, deliberately.** The intent was declaration *before*
editing. M1's tool surface has no mid-attempt handshake, so the declaration arrives
*with* the result: the rule **rejects** an undeclared edit rather than preventing
one. A pre-declaration handshake needs a new tool round-trip and is explicitly out
of scope; if the post-hoc form proves insufficient, that is a later milestone's
change and not a defect in this one.

**What replaced a semantic judgement is a set comparison**, so the rule holds
identically in every ecosystem and says nothing about whether an edit was wise.
An agent that pins the wrong distribution in the right file passes it. That is
intended — the rescan is what judges the change ([ADR 027](027-nothing-in-rust-refuses-a-version.md)) —
but a reader who expected the declared-files rule to be a safety check should
expect only bookkeeping.

**The refusal names paths, so a breach is diagnosable.** `DeclarationBreach`
renders `changed without declaring: …` and `declared without changing: …`, which is
the whole of what Rust can say about a file whose meaning it does not know.
