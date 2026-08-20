# 001 — Spawn ralph as a background subagent for the DEVELOP phase

Date: 2026-03-14
Status: superseded by 004
Cites: none

This record cites nothing. No name in this record survives in this repository. ADR 004 replaced them all.

## Context

The orchestrate skill invoked ralph through `Skill()`, so ralph ran in the leader's context. DISCOVER and DEFINE consumed that context first, and ralph's implementation cycles then exhausted the rest. The leader could not reach DELIVER, because the reaction engine also ran in the leader's context between ralph turns.

## Decision

Spawn ralph as a background subagent through `Agent()`, not inline through `Skill()`. Move the reaction checks into ralph's own loop, and report completion to the leader as `RALPH_STATUS`. Merge the `reaction {}` config block into `ralph {}`.

## Consequences

- The leader's context stays small enough for all four phases.
- Ralph owns its reaction checks, so it is self-contained and testable alone.
- The leader loses every view of ralph's intermediate state. It reads the final `RALPH_STATUS` and nothing else.
- Each respawn starts a fresh subagent, so ralph must re-derive its state from beans every time.
