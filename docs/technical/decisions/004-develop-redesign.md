# 004 — Replace the three develop variants with one develop protocol

Date: 2026-03-28
Status: accepted; amended by the note below
Supersedes 001 and 002.
Cites: skills/develop/SKILL.md, skills/develop-loop/SKILL.md, skills/develop-holistic/SKILL.md

## Context

The develop phase had three faults. A coordinator that spawned a reviewer sub-subagent stalled, and a batch step deferred every merge conflict. The develop-subs and develop-team variants duplicated their logic.

## Decision

Replace the three variants with one develop protocol. Track state on beans, review the epic as a whole, and defer finishing to the end. Give a large epic a swarm mode, running one worktree per bean with flat subagents.

## Consequences

- One entry point replaces three. The project gives up the per-variant tuning that develop-subs and develop-team allowed.
- Nothing nests a subagent, because swarm uses flat subagents and reviews inline.
- Swarm rebases each branch before review, so an incremental merge replaces the deferred batch one.
- Develop owns the lifecycle, so the composed skills skip their own finishing step.
- A user picks one of three execution modes.

## Amendment (M4c) — the swarm mode and the composition were not built

The decision to replace the three variants stands, and `skills/develop/SKILL.md` is the single entry point it asked for.

Three of its claims do not describe this build. There is no swarm mode: no skill, script or config key in this repository names one, so no epic runs one worktree per bean. There are not three execution modes, only the sequential per-task loop that `skills/develop/SKILL.md` step 2 describes. Nothing composes the `subagent-driven-development` or `executing-plans` skills either, so no skill is patched to skip finishing; `skills/finish-branch/SKILL.md` owns that step, and develop step 4 calls it.

What replaced the composition is ADR 005's split into an orchestrator and two sub-skills, and ADR 009's validator scripts. A reader who wants the develop protocol this build has should read those two and `skills/develop/SKILL.md`, not this decision.
