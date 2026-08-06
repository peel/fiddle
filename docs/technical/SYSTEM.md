# System

## Overview

Fiddle is a portable Agent Skills library that orchestrates a four-phase development lifecycle (DISCOVER, DEFINE, DEVELOP, DELIVER) with optional multi-model support. It ships one canonical `skills/` tree plus thin Claude, Codex, and Pi manifests. External providers (Codex via CLI, Gemini via CLI) participate in debate and review phases but are optional — skills degrade to the current harness when providers are unavailable.

## Components

**Orchestrate** (`skills/orchestrate/SKILL.md`) — Top-level lifecycle coordinator. Its primary skill is a router; configuration and resumption details load from focused references. External provider calls go through `hooks/dispatch-provider.sh`.

**Panel** (`skills/panel/SKILL.md`) — Structured multi-model adversarial analysis. The current harness, Codex, and Gemini argue independent positions, cross-review, then the lead synthesizes a verdict. External providers are called via `hooks/dispatch-provider.sh`. Degrades to current-harness analysis when no external providers are available.

**Develop** (`skills/develop/SKILL.md`) — Thin orchestrator for the implementation phase. Validates bean bodies (eval block, files, steps checklist required), then delegates to sub-skills: `develop-loop` (`skills/develop-loop/SKILL.md`) handles per-task iteration for one bean at a time — implement, gather a per-domain evidence pack (tests, checks, runtime probes), dispatch ONE evaluator per domain (provider chosen by `scripts/select-evaluator-provider.sh` from the domain's ordered preference list, first available provider differing from the always-claude implementer), normalize the single scorecard, and converge via scripts; evidence-only scorecards (explicit empty dimensions) converge on a single pass. `develop-holistic` (`skills/develop-holistic/SKILL.md`) handles cross-domain integration review with remediation and keeps multi-provider dispatch with min-merge. All evaluation state tracked via beans and eval-log scripts.

**Using Fiddle** (`skills/using-fiddle/SKILL.md`) — Bootstrap skill for routing common requests, mapping Claude-style tool vocabulary across Claude, Codex, and Pi, and resolving internal subagent models.

**Hooks** (`hooks/`) — Claude-oriented hooks check provider binaries, add code-navigation guidance, guard archives, and report progress. `develop-verdict-gate.sh` is a Stop hook that blocks turn-end while `.fiddle/active-bean` names a develop-loop bean without a terminal verdict (fail-open when the marker is absent or jq is missing). Codex has a minimal `.codex/hooks.json`. Pi support in v1 is skill/package discovery, not hook parity.

**Challenge** (`skills/challenge/SKILL.md`) — Decision-tree interrogation skill. Walks every branch of a plan or design until shared understanding is reached. Phase-aware: in DISCOVER, opens by synthesizing findings and confirming scope; in DEFINE, challenges design edge cases and panel dissent. Also usable standalone.

**Supporting skills** — `fiddle:discover-docs` (project context scan), `fiddle:deliver-docs` (post-ship doc updates), `fiddle:define-beans` (task sizing), `fiddle:adr`/`fiddle:feedback`/`fiddle:backlog` (append-only records).

**Skill quality tooling** (`scripts/audit-skills.sh`, `scripts/check-portability.sh`) — Validates portable skill metadata, reachable companion documentation, primary-skill size, and optionally trigger-first descriptions. `skill-quality.yml` runs these checks and their fixtures in CI.

## Data

**`orchestrate.json`** (JSON) — Declares external provider participation, evaluator settings, plans, and internal subagent models. `models.roles.<role>` overrides `models.phases.<phase>`; `default` inherits the current session model. External provider CLI selection is independent. Merge order is defaults, config file, then CLI flags.

**`.claude/orchestrate-events.log`** — Ephemeral event log created during orchestrate runs. Tracks phase transitions, failures, escalations. Deleted on cleanup.

**Bean state** — Managed by external `beans` CLI. Epics, tasks, tags (worktree slots, CI retries, stall respawns, needs-attention). Beans are the unit of work for develop and swarm.

## Infrastructure

Runs entirely locally as portable skills. Claude loads `.claude-plugin/plugin.json`, Codex loads `.codex-plugin/plugin.json`, and Pi reads `package.json` with `pi.skills`. Requires bash and jq for helper scripts. External providers (codex, gemini) are optional local CLIs.


## Invariants

- Skills must degrade gracefully when external providers are unavailable. Never fail solely because a provider is missing — fall back to the current harness.
- Hooks must exit 0 on success or non-applicable scenarios. Exit 2 to reject with feedback (task-completed-verify pattern).
- Provider calls use `hooks/dispatch-provider.sh` — never inline provider CLI prompts in skill files.
- External provider calls run in parallel when the harness supports it; otherwise run sequentially and report reduced coverage.
- Append-only docs (FEEDBACK, BACKLOG, research logs) are never edited or deleted.
- Bean bodies must be self-contained — implementer agents work from the bean body alone without reading plan files.
- Design specs in `docs/specs/` and implementation plans in `docs/plans/` are local lifecycle artifacts and remain gitignored; bean bodies carry the durable executable contract.
- Worktree agents must route all bean CLI operations through `--beans-path` to the main checkout's `.beans/`. Only the lead manages bean status transitions.
- Evaluators interpret pre-gathered evidence packs; they never gather evidence themselves. Read-only external providers receive the pack via `dispatch-provider.sh --evidence-file`.
- Evidence-only scorecards emit an explicit `"dimensions": {}` — the key is never omitted; only the explicitly empty object signals single-pass convergence.
- Every scorecard must carry a criteria array; `merge-scorecards.sh` rejects criteria-less input with exit 2.
- Holistic reviewers use the canonical scorecard envelope. `merge-scorecards.sh` conservatively merges `spec_coverage_matrix` and deduplicates `remediation_beans` by requirement while retaining source providers.
- Skills are written as judgment plus rationale. Mechanical invariants live in scripts with exit-code contracts, not in prose, and skill files carry no emphatic markup (gate blocks, capitalized emphasis, rationalization tables, red-flag lists, announcement lines). Frontmatter `description` fields, JSON schemas, and quoted external content are the exceptions, since they are interface text rather than instruction. See the authoring note in `skills/using-fiddle/SKILL.md`.
- Internal subagent models resolve through `scripts/resolve-subagent-model.sh`: a role override wins over a phase default, while `default` omits an explicit model and inherits the session. Provider CLI selection never flows through this resolver.
- `scripts/audit-skills.sh` returns exit 2 with JSON errors for malformed metadata, missing references, orphaned companions, or configured primary-skill size violations.

## Known issues

- Default dispatch budgets cannot absorb the confirming double-pass (a pass on iteration N cannot confirm within budget N), and `check-convergence.sh`'s budget check is ambiguous between pre- and post-dispatch counts at the boundary.

---
Last reviewed: 2026-08-05
