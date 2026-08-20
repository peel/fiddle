# 002 — Route a worktree agent's bean operations to the main checkout

Date: 2026-03-14
Status: accepted
Cites: MAIN_BEANS_PATH, `beans --beans-path`, skills/orchestrate/resumption.md

## Context

An agent spawned in a git worktree has its working directory inside that worktree. The main checkout holds `.beans/`, so a bean command from a worktree either failed or wrote to the wrong directory. Progress updates stayed invisible to the lead until somebody merged the worktree back.

## Decision

Compute the main checkout's `.beans/` path once at startup and pass it to every agent as `MAIN_BEANS_PATH`. Give every bean command in an agent template the `--beans-path` flag. Let the lead alone change a bean's status.

## Consequences

- The lead and the TUI see a worktree agent's bean updates at once, whatever the agent's working directory.
- `--beans-path` is harmless in the main checkout, so an agent can pass it unconditionally.
- An implementer can no longer change a bean's status. The project gives up parallel status writes to remove the race with the lead.
- The lead must propagate the path to every agent. An agent that does not receive it falls back to its working directory, which fails in a worktree.
