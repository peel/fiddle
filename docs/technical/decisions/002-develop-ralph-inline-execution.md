# 002 — Run ralph inline, not as a nested subagent

Date: 2026-03-26
Status: superseded by 004
Supersedes 001.
Cites: ralph, RALPH_STATUS, TeamCreate, SendMessage

None of the names above survive in this repository. ADR 004 replaced them.

## Context

ADR 001 spawned ralph as a background subagent to save the leader's context. Ralph has to spawn its own implementers and review coordinators, and a subagent cannot reliably spawn a sub-subagent. The nested agent stalled, so the "Ralph Subs" mode did not work.

## Decision

Run both ralph variants inline in the main session. Let the variant decide worker dispatch alone: subs dispatch through `Agent()`, and team dispatch through `TeamCreate` and `SendMessage`. Remove `RALPH_STATUS`, because ralph no longer exits.

## Consequences

- Both execution modes work, because nothing nests a subagent.
- Ralph's loop consumes the leader's context. The project gives up the fresh context window ADR 001 bought. A 1M window carries most epics, and a user can respawn a session for a large one.
- The develop skill is simpler. It parses no `RALPH_STATUS` and waits on no task output.
- Ralph can show the user a diff during the loop, rather than batching every diff to the end.
