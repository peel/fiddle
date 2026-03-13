# Automated MCP Provider Setup

## Problem

Fiddle skills invoke external providers (Codex via MCP, Gemini via CLI) but users must manually configure the MCP server in their Claude Code settings. There's no discovery mechanism — users only learn about this requirement by reading the README or hitting runtime fallbacks.

## Solution

A SessionStart hook that detects unconfigured providers and nudges the user, paired with a `/fiddle:init` skill that writes the MCP config.

## Components

### 1. SessionStart hook (`hooks/session-start-check-providers.sh`)

Runs on every session start:

1. Check if `orchestrate.conf` exists in the project root — bail if not (not a fiddle project)
2. Parse declared providers from `orchestrate.conf`
3. For each declared provider, check if the binary is on PATH (`which codex`, `which gemini`)
4. For each available binary, check if the MCP server is already configured in `.mcp.json` (project) or `~/.claude.json` (global)
5. If any provider is installed but unconfigured, print a message suggesting `/fiddle:init`
6. If all configured or none installed, stay silent

### 2. `/fiddle:init` skill (`skills/init/SKILL.md`)

When invoked:

1. Parse `orchestrate.conf` for declared providers
2. Detect which are on PATH (`codex`, `gemini`)
3. Report findings: "Found codex on PATH, gemini not found"
4. Ask user: write to project `.mcp.json` or global `~/.claude.json`?
5. Read existing target file (if any), merge new MCP entries without clobbering existing ones
6. Write config — for Codex: `{"codex": {"command": "codex", "args": ["mcp"]}}`
7. Remind user about auth: "Run `codex --login` to authenticate"

## Behavior

- **Idempotent**: Re-running init when already configured is a no-op (reports "already configured")
- **Non-destructive**: Merges into existing `.mcp.json`, never overwrites other servers
- **Gemini**: Uses CLI not MCP — init verifies it's on PATH and mentions auth, no MCP entry needed
- **No marker file**: The hook checks actual MCP config files for server entries

## File changes

```
skills/init/SKILL.md                       — new skill
hooks/session-start-check-providers.sh     — new hook script
hooks/hooks.json                           — add SessionStart entry
README.md                                  — document /fiddle:init
```
