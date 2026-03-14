# System

## Overview

Fiddle is a Claude Code plugin that orchestrates a four-phase development lifecycle (DISCOVER, DEFINE, DEVELOP, DELIVER) with optional multi-model support. It ships as a collection of skills (markdown instruction files), hooks (bash scripts on Claude Code events), and configuration (HCL). External providers (Codex via MCP, Gemini via CLI) participate in debate and review phases but are optional — all skills degrade to Claude-only subagents when providers are unavailable.

## Components

**Orchestrate** (`skills/orchestrate/SKILL.md`) — Top-level lifecycle coordinator. Reads config, chains phases. DEVELOP phase spawns ralph as a background subagent (`Agent()`) for context isolation, waits for `RALPH_STATUS` result, then runs external holistic review. Delegates to other skills per phase.

**Panel** (`skills/panel/SKILL.md`) — Structured multi-model adversarial analysis. Claude, Codex (MCP), and Gemini (CLI) argue independent positions, cross-review, then Claude synthesizes a verdict. Degrades to 2 Claude subagents when no external providers are available.

**Ralph** (`skills/ralph-subs-implement/SKILL.md`, `skills/ralph-beans-implement/SKILL.md`) — Parallel bean implementation. Dispatches implementer subagents (sonnet) in worktrees with tiered review (haiku then sonnet). Two variants: subagent-driven and team-based. The lead computes `MAIN_BEANS_PATH` (absolute path to main checkout's `.beans/`) at startup and substitutes it into all agent prompts; worktree agents use `beans --beans-path {MAIN_BEANS_PATH}` so bean updates are always visible to the TUI and lead. Implementers are prohibited from changing bean status — only the lead manages status transitions. Includes reaction checks (CI failure escalation, stall detection, review overflow) in its "Assess and Act" loop. When invoked with `--caller orchestrate`, outputs `RALPH_STATUS: COMPLETE` or `RALPH_STATUS: PARKED` on exit.

**Init** (`skills/init/SKILL.md`) — Provider setup skill. Detects installed binaries (codex, gemini) on PATH, checks existing MCP configuration, asks where to write config (project `.mcp.json` or global `~/.claude.json`), merges codex MCP entry non-destructively.

**Hooks** (`hooks/`) — `session-start-check-providers.sh` detects unconfigured providers on session start and nudges `/fiddle:init`. `task-completed-verify.sh` gates task completion with build/test verification (go build, go test, flutter test). Dispatched via `run-hook.cmd` (cross-platform polyglot wrapper).

**Supporting skills** — `docs-discover` (project context scan), `docs-evolve` (post-ship doc updates), `bean-decomposition` (task sizing), `adr`/`feedback`/`backlog` (append-only records).

## Data

**`orchestrate.conf`** (HCL) — Declares which external providers are used per phase. All ralph settings (worker counts, review cycle limits, turn budgets, reaction thresholds) live in a single `ralph {}` block. Merge order: defaults, config file, CLI flags.

**`.mcp.json`** (JSON) — Claude Code MCP server configuration. Written by `/fiddle:init`. Contains server entries like `{"codex": {"command": "codex", "args": ["mcp"]}}`. Can live at project level or `~/.claude.json` globally.

**`.claude/orchestrate-events.log`** — Ephemeral event log created during orchestrate runs. Tracks phase transitions, failures, escalations. Deleted on cleanup.

**Bean state** — Managed by external `beans` CLI. Epics, tasks, tags (worktree slots, CI retries, stall respawns, needs-attention). Beans are the unit of work for ralph.

## Infrastructure

Runs entirely locally as a Claude Code plugin. No server, no cloud, no CI. Installed via `claude --plugin-dir` or the plugin marketplace. Requires bash and jq for hooks. External providers (codex, gemini) are optional local CLIs.

## Invariants

- Skills must degrade gracefully when external providers are unavailable. Never fail — fall back to Claude-only subagents.
- Hooks must exit 0 on success or non-applicable scenarios. Exit 2 to reject with feedback (task-completed-verify pattern).
- `/fiddle:init` must never overwrite existing MCP server entries — merge only.
- Append-only docs (FEEDBACK, BACKLOG, research logs) are never edited or deleted.
- Bean bodies must be self-contained — implementer agents work from the bean body alone without reading plan files.
- Worktree agents must route all bean CLI operations through `--beans-path` to the main checkout's `.beans/`. Only the lead manages bean status transitions.

## Known issues

None currently identified.

---
Last reviewed: 2026-03-14

