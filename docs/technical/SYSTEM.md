# System

## Overview

Fiddle is a Claude Code plugin that orchestrates a four-phase development lifecycle (DISCOVER, DEFINE, DEVELOP, DELIVER) with optional multi-model support. It ships as a collection of skills (markdown instruction files), hooks (bash scripts on Claude Code events), and configuration (JSON). External providers (Codex via CLI, Gemini via CLI) participate in debate and review phases but are optional — all skills degrade to Claude-only subagents when providers are unavailable.

## Components

**Orchestrate** (`skills/orchestrate/SKILL.md`) — Top-level lifecycle coordinator. Reads config, chains phases. Delegates to other skills per phase. External provider calls go through the provider-dispatch procedure (`roles/provider-dispatch.md`).

**Panel** (`skills/panel/SKILL.md`) — Structured multi-model adversarial analysis. Claude, Codex, and Gemini argue independent positions, cross-review, then Claude synthesizes a verdict. External providers are called as async parallel background Bash tasks via the dispatch procedure (`roles/provider-dispatch.md`). Degrades to 2 Claude subagents when no external providers are available.

**Develop** (`skills/develop/SKILL.md`) — Thin orchestrator for the implementation phase. Validates bean bodies (eval block, files, steps checklist required), then delegates to sub-skills: `develop-loop` (`skills/develop-loop/SKILL.md`) handles per-task evaluation iteration (implement → evaluate → converge) for one bean at a time, `develop-holistic` (`skills/develop-holistic/SKILL.md`) handles cross-domain integration review with remediation. All evaluation state tracked via beans and eval-log scripts.

**Swarm** (`skills/develop-swarm/SKILL.md`) — Parallel worktree-per-bean execution with incremental rebase-before-review merge. Flat subagents (no coordinator nesting). Uses an assess-and-act orchestration loop. The lead computes `MAIN_BEANS_PATH` (absolute path to main checkout's `.beans/`) at startup and substitutes it into all agent prompts; worktree agents use `beans --beans-path {MAIN_BEANS_PATH}` so bean updates are always visible to the TUI and lead. Implementers are prohibited from changing bean status — only the lead manages status transitions.

**Hooks** (`hooks/`) — `session-start-check-providers.sh` checks CLI provider binaries are on PATH on session start. `task-completed-verify.sh` gates task completion with build/test verification (go build, go test, flutter test).

**Challenge** (`skills/challenge/SKILL.md`) — Decision-tree interrogation skill. Walks every branch of a plan or design until shared understanding is reached. Phase-aware: in DISCOVER, opens by synthesizing findings and confirming scope; in DEFINE, challenges design edge cases and panel dissent. Also usable standalone.

**Supporting skills** — `discover-docs` (project context scan), `deliver-docs` (post-ship doc updates), `define-beans` (task sizing), `adr`/`feedback`/`backlog` (append-only records).

## Data

**`orchestrate.json`** (JSON) — Declares which external providers are used per phase. The `plans {}` block controls where superpowers saves plans/specs and whether to commit them. Merge order: defaults, config file, CLI flags.

**`.claude/orchestrate-events.log`** — Ephemeral event log created during orchestrate runs. Tracks phase transitions, failures, escalations. Deleted on cleanup.

**Bean state** — Managed by external `beans` CLI. Epics, tasks, tags (worktree slots, CI retries, stall respawns, needs-attention). Beans are the unit of work for develop and swarm.

## Infrastructure

Runs entirely locally as a Claude Code plugin. No server, no cloud, no CI. Installed via `claude --plugin-dir` or the plugin marketplace. Requires bash and jq for hooks. External providers (codex, gemini) are optional local CLIs.


## Invariants

- Skills must degrade gracefully when external providers are unavailable. Never fail — fall back to Claude-only subagents.
- Hooks must exit 0 on success or non-applicable scenarios. Exit 2 to reject with feedback (task-completed-verify pattern).
- Provider calls use the dispatch procedure (`roles/provider-dispatch.md`) — never inline CLI commands in skill files.
- All external provider calls fire as background Bash tasks — never synchronous blocking calls.
- Append-only docs (FEEDBACK, BACKLOG, research logs) are never edited or deleted.
- Bean bodies must be self-contained — implementer agents work from the bean body alone without reading plan files.
- Worktree agents must route all bean CLI operations through `--beans-path` to the main checkout's `.beans/`. Only the lead manages bean status transitions.

## Known issues

None currently identified.

---
Last reviewed: 2026-04-02

