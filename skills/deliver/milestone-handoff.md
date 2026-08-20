# Milestone handoff

Publish a compact restart boundary for the next seed-aware epic. Use the canonical main-worktree Beans path. If the epic has no direct child tagged `planning`, it is legacy and this protocol does not apply.

Gather the delivered capability, durable decisions and contracts, material discoveries, verification and acceptance evidence, unresolved risks/debt, and links to the RFC/design, plan, PRs, and permanent docs. Summarize evidence; do not include raw model transcripts, credentials, or temporary context files.

Replace, rather than append, exactly one block in the epic body:

```markdown
<!-- milestone-handoff:start -->
## Milestone Handoff

- Capability now available: ...
- Decisions and contracts: ...
- Verification and acceptance evidence: ...
- Evaluation cost: dispatches to convergence, and `unchanged_tree_reevaluations` summed over the epic's beans (`scripts/trend-eval-history.sh`)
- Risks and debt: ...
- Sources and permanent context: ...
<!-- milestone-handoff:end -->
```

`Evaluation cost` carries what the milestone spent to believe itself, not just what it concluded. `unchanged_tree_reevaluations` is the count of dispatches that re-judged a tree the previous dispatch had already judged: a milestone that converged on the strength of several of those converged partly by asking again, and the successor reading this handoff is entitled to know that. Zero is a substantive answer and should be stated as one.

Use a temporary body file so existing epic context is preserved. Validate that both markers occur exactly once and every field is substantive before closing the epic.

If the epic has a parent milestone, maintain one marker-delimited `Milestone Handoff Index` in the parent's body. Upsert one entry keyed by epic ID and point to the epic handoff; do not copy the handoff contents into the index. The immediate successor reads this epic through its `blocked_by` edge, while the parent index provides navigation across completed milestones.
