# 002 — Route worktree agent bean operations to main checkout via --beans-path

**Date:** 2026-03-14
**Status:** accepted

## Context

When ralph spawns implementer and reviewer agents in git worktrees, their working directory is inside the worktree — not the main checkout where `.beans/` lives. Bean CLI calls from worktree agents either failed ("bean not found") or silently operated on the wrong directory, making progress updates invisible to the TUI and lead until the worktree was merged back.

## Decision

Introduce a `{MAIN_BEANS_PATH}` placeholder in all agent role templates. The lead computes the absolute path to the main checkout's `.beans/` once at startup and substitutes it into every spawned agent's prompt. All `beans` CLI calls in agent templates use `beans --beans-path {MAIN_BEANS_PATH}`. Implementers are explicitly prohibited from changing bean status — only the lead manages status transitions. The lead itself is reminded to use `--beans-path` after `cd`-ing into a worktree for verification.

## Consequences

- Bean updates from worktree agents are immediately visible to the TUI and lead, regardless of the agent's working directory.
- The `--beans-path` flag is harmless when already in the main checkout, so it can be used unconditionally for safety.
- Implementers can no longer accidentally change bean status, preventing race conditions between the lead's status management and agent actions.
- The lead must compute and propagate `MAIN_BEANS_PATH` to every agent — if omitted, agents fall back to cwd-based resolution which may fail in worktrees.
