# 027 — Nothing in Rust refuses a version

Status: accepted
Cites: AgentBudget, `[[workspace.checks]]`, evaluate::RescanVerdict, cve::GroupStatus, cve::MigrationAttempt

`crates/fiddle-runtime/src/cve/version.rs` and `crates/fiddle-runtime/src/cve/group.rs` are deleted. The `cve` module holds `dedup`, `project` and `verdict`.

## Context

`cve/version.rs` compared mixed `v`-prefixed versions, took the smallest same-minor patch, and refused a major bump. That refusal was deliberate in M4a: it was provable, cheap, and the strongest claim the milestone could make. It is also ecosystem semantics, so it falls on the agent's side of the line ADR 025 draws.

## Decision

Refuse nothing in Rust about versions. Let the agent choose the version, including a major bump, and delete `cve/version.rs` rather than relax it. Make the rescan the guarantee: one attempt, one commit, one rescan.

## Consequences

- The property M4a could prove is now model output. M4a could demonstrate that it took the smallest same-minor patch and refused a major bump, and nothing demonstrates that now.
- The project gave up version arithmetic for a capability that works outside Go. A major bump that clears the finding and passes every declared check lands. A reader who knew the old rule should stop expecting a same-minor diff.
- A finding that does not clear makes the whole attempt needs-work, including edits that did clear their findings. M4b stopped reverting that attempt and publishes it as a draft nobody merges; see [043](043-an-unproved-attempt-is-published-as-its-own-draft.md). No finding in it is claimed as fixed, which is what this line was written to say.
- What a deployment can still say about version choice, it says through `[[workspace.checks]]`. A build, a test suite and a lint are how a repository objects to a bump it cannot absorb.
- The five `AgentBudget` bounds are now the run's single grant. `max_turns`, `max_tokens`, `deadline`, `max_changed_files` and `tool_timeout` were a per-group allowance while grouping existed.

M4a's refusal was the strongest thing that milestone could say about what a sweep would do to a repository. Version arithmetic is not coming back. A refusal that Rust can state is a refusal Rust has to justify in an ecosystem it does not read.

Grouping is gone with `cve/group.rs`, and one attempt now covers every selected finding. So the same five numbers bound the whole sweep rather than each batch within it. A deployment that sized them for one group of a few findings has sized them for all of them. That is a smaller grant than it looks. It is the thing to re-check first when an unattended sweep exhausts its budget.
