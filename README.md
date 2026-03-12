# Fiddle

Claude Code plugin for automated development lifecycle. Chains discover, define, develop, deliver phases with multi-model support and a reaction engine.

## Orchestrate

`/fiddle:orchestrate <topic>` runs the full lifecycle. Each phase invokes other skills:

```
DISCOVER
  └─ fiddle:docs-discover        — scan project docs, code, beans
  └─ Codex MCP / Gemini CLI    — external research (optional)
  └─ Socratic dialogue          — confirm scope with user

DEFINE
  └─ superpowers:brainstorming  — explore intent, produce 2-3 approaches
  └─ fiddle:panel                 — adversarial debate across models
  └─ superpowers:writing-plans  — implementation plan + bean decomposition
     └─ fiddle:bean-decomposition — task sizing rules

DEVELOP
  └─ fiddle:ralph-subs-implement  — parallel bean implementation
     └─ implementers (sonnet)   — write code in worktrees
     └─ review coordinators     — tiered review (haiku → sonnet)
  └─ reaction engine            — CI failure, stall, review overflow detection
  └─ holistic review (opus)     — cross-bean consistency check

DELIVER
  └─ Codex MCP / Gemini CLI    — drift analysis vs design doc (optional)
  └─ fiddle:docs-evolve           — update SYSTEM.md, ADRs, BACKLOG
  └─ close epic
```

## All Skills

| Skill | Purpose |
|-------|---------|
| `fiddle:orchestrate` | Full lifecycle orchestrator (DISCOVER → DEFINE → DEVELOP → DELIVER) |
| `fiddle:panel` | Multi-model adversarial analysis with cross-review and synthesis |
| `fiddle:ralph-subs-implement` | Parallel bean implementation with subagents and tiered review |
| `fiddle:ralph-beans-implement` | Team-based bean implementation variant |
| `fiddle:docs-discover` | Socratic dialogue to bootstrap or review project docs |
| `fiddle:docs-evolve` | Post-ship update of technical docs, ADRs, backlog |
| `fiddle:bean-decomposition` | Task sizing rules for implementation plans |
| `fiddle:adr` | Create architecture decision record |
| `fiddle:feedback` | Append user feedback signal |
| `fiddle:backlog` | Append idea or debt item |
| `fiddle:patch-superpowers` | Re-apply beans integration patches to superpowers skills |

## Configuration

Orchestrate reads `.claude/orchestrate.conf` (HCL) from your project root. All blocks are optional — defaults apply when omitted.

```hcl
providers {
  discover         = ["codex"]
  define           = ["codex", "gemini"]
  develop          = []
  develop_holistic = ["codex"]
  deliver          = ["codex"]
}

ralph {
  workers           = 2
  max_review_cycles = 3
  max_impl_turns    = 50
  max_review_turns  = 30
}

reaction {
  ci_max_retries      = 3
  stall_timeout_min   = 15
  stall_max_respawns  = 2
}
```

Merge order: defaults → config file → CLI flags. `--providers` sets all phases; per-phase flags override.

## External Providers

Orchestrate and panel use external models for multi-perspective analysis:

| Provider | Interface | Auth |
|----------|-----------|------|
| Codex | MCP server (`codex mcp`) | `codex --login` |
| Gemini | CLI (`gemini`) | `gemini auth` |

Both optional — skills fall back to Claude-only subagents without them.

## Hooks

- `task-completed-verify.sh` — gates task completion with build/test verification

## Install

Development:

```bash
claude --plugin-dir /path/to/fiddle
```

Via marketplace:

```
/plugin marketplace add github:peel/peel-marketplace
/plugin install fiddle
```
