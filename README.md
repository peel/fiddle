# Peel Orchestrator

Claude Code plugin for automated development lifecycle. Chains discover, define, develop, deliver phases with multi-model support and a reaction engine.

## Flow

```
/peel:docs-discover → /peel:orchestrate → /peel:docs-evolve
                          │
                    DISCOVER → DEFINE → DEVELOP → DELIVER
                          │       │        │         │
                       research  panel   ralph    drift analysis
                       codex+   debate  implement  docs update
                       gemini   review  reaction
```

## Skills

### Orchestration
- `/peel:orchestrate <topic>` — full lifecycle across 4 phases
- `/peel:panel <topic>` — multi-model adversarial analysis (Codex + Gemini + Claude)
- `/peel:ralph-subs-implement --epic <id>` — parallel bean implementation with tiered review
- `/peel:ralph-beans-implement --epic <id>` — team-based bean implementation variant

### Documentation
- `/peel:docs-discover [scope]` — Socratic dialogue to bootstrap or review docs
- `/peel:docs-evolve [--epic <id>]` — post-ship update of technical docs, ADRs, backlog
- `/peel:adr <title>` — architecture decision record
- `/peel:feedback <signal>` — append user feedback
- `/peel:backlog <idea>` — append idea or debt item

### Planning
- `/peel:bean-decomposition` — task sizing rules for implementation plans

### Maintenance
- `/peel:patch-superpowers` — re-apply beans integration patches to superpowers skills

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

Add the peel marketplace, then install:

```
# In Claude Code settings, add marketplace:
github:peel/peel-marketplace
```
