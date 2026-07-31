# 005 — Split develop into orchestrator + sub-skills

**Date:** 2026-04-02
**Status:** accepted; partially superseded by [009]

<!-- The split into orchestrator plus sub-skills stands. Two elements below are
     superseded by 009: the bean body validation HARD-GATE is now
     scripts/validate-bean-body.sh, and the Iron Laws that drove the duplicated
     bytes were deleted, their content absorbed into develop-loop. -->


## Context

develop/SKILL.md was 34KB (628 lines) — a monolithic file containing setup, per-task evaluation loop (13 substeps), holistic review (5 substeps), completion, historical notes, and redundant constraints. Its size caused two problems: high per-invocation token cost, and agents silently skipping the entire protocol because they found it overwhelming and rationalized "this is too simple for the full loop."

## Decision

Split develop into a thin orchestrator (~5KB) that delegates to two sub-skills: develop-loop (~20KB, per-task evaluation) and develop-holistic (~9KB, cross-domain review). The orchestrator adds a bean body validation HARD-GATE that requires eval block, files section, and steps checklist before entering the loop.

## Consequences

- Peak single-agent token load drops from 34KB to 20.5KB (40% reduction). The orchestrator itself drops to 4.8KB — much harder for agents to rationalize skipping.
- Bean body quality is now enforced at the gate, preventing thin descriptions from reaching implementers.
- Total bytes across develop files increased (34KB → 34KB across 3 files) due to duplicated frontmatter and Iron Laws. This is acceptable — the goal was per-invocation load, not total file size.
- Sub-skills must be kept in sync when the evaluation protocol changes — three files to update instead of one.
