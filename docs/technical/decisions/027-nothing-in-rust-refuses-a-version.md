# 027 — Nothing in Rust refuses a version

Status: accepted
Cites: fiddle_core::AgentBudget, config::CheckRef

## Context

`cve/version.rs` compared mixed `v`-prefixed versions, selected the smallest patch
within the same minor, and **refused a major bump**. That refusal was deliberate in
M4a, not an accident of the implementation: it was provable, it was cheap, and it
was the strongest thing the milestone could say about what a sweep would do to a
repository. It is also ecosystem semantics — what a major bump *means* is a
question only the ecosystem answers — so it falls on the agent's side of the line
[ADR 025](025-the-agent-owns-ecosystem-semantics.md) draws.

## Decision

**Rust refuses nothing about versions.** The agent chooses the version, including
whether to take a major bump. `cve/version.rs` is deleted rather than relaxed.

**The rescan is the guarantee.** One attempt, one commit, one rescan: a finding
that does not clear makes the whole attempt needs-work and the commit is reverted,
including edits that did clear their findings.

## Consequences

**The property M4a could prove is now model output.** M4a could demonstrate that it
took the smallest same-minor patch and refused a major bump. Nothing demonstrates
that now. A major bump that clears the finding and passes every declared check
**lands**, and a reader who knew the old rule should stop expecting a same-minor
diff. This is the accepted cost of a capability that works outside Go, stated here
rather than discovered in a pull request.

**What a deployment can still say about version choice, it says through
`[[workspace.checks]]`.** A build, a test suite and a lint are how a repository
objects to a bump it cannot absorb; the rescan is how it objects to one that did
not fix anything. Neither of those is version arithmetic, and version arithmetic is
not coming back — a refusal that Rust can state is a refusal Rust has to justify in
an ecosystem it does not read.

**The five `AgentBudget` bounds are the run's single grant.** `max_turns`,
`max_tokens`, `deadline`, `max_changed_files` and `tool_timeout` were a per-group
allowance while grouping existed. Grouping is gone with `cve/group.rs`, one attempt
covers every selected finding, and the same five numbers now bound the whole sweep
rather than each batch within it. A deployment that sized them for one group of a
few findings has sized them for all of them, which is a smaller grant than it looks
and the thing to re-check first when an unattended sweep exhausts its budget.
