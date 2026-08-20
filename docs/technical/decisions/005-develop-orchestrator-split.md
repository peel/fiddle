# 005 — Split develop into an orchestrator and two sub-skills

Date: 2026-04-02
Status: accepted; partially superseded by 009
Cites: skills/develop/SKILL.md, skills/develop-loop/SKILL.md, skills/develop-holistic/SKILL.md, scripts/validate-bean-body.sh

ADR 009 supersedes two elements below. The bean body gate is now `scripts/validate-bean-body.sh`, and the Iron Laws that drove the duplicated bytes are deleted.

## Context

`skills/develop/SKILL.md` held 34KB across 628 lines, carrying setup, two loops, completion, historical notes and repeated constraints. Its size cost tokens on every invocation. Agents also skipped the whole protocol, judging it too heavy for the task in hand.

## Decision

Split develop into a thin orchestrator and two sub-skills. Give `develop-loop` the per-task evaluation and `develop-holistic` the cross-domain review. Make the orchestrator check each bean body before the loop starts.

## Consequences

- The peak load on one agent fell from 34KB to 20.5KB, and the orchestrator itself to 4.8KB. An agent finds 4.8KB much harder to argue past.
- The gate requires an eval block, a files section and a steps checklist, so no implementer receives a thin body.
- The total bytes across the develop files did not fall. The project gave up that saving for the per-invocation saving, and paid it in duplicated frontmatter and Iron Laws.
- Three files now state the evaluation protocol. Every change to it has to reach all three.
