# 003 — Reach every external provider through its CLI

Date: 2026-03-15
Status: accepted
Cites: hooks/dispatch-provider.sh, skills/develop/provider-context.md, orchestrate.json

## Context

Fiddle called Codex as an MCP server and Gemini as a CLI. The MCP route needed a `.mcp.json` per project, which was fragile and needed its own init flow. Both tools also ship a full CLI that runs in the project directory and reads the filesystem.

## Decision

Drop the Codex MCP route and call `codex exec` instead. Reach every external provider through one dispatch procedure. The procedure reads each provider's command and flags from the `providers` block in `orchestrate.json`. Build the prompt from one template and run the call as a background task.

## Consequences

- No project needs MCP configuration. The project loses MCP's tool calling and streaming for codex. Getting them back would mean revisiting this decision.
- Every provider is invoked the same way, so a new provider is a config block rather than a new integration.
- A provider CLI reads the codebase from the project directory, so no prompt has to carry that context.
- Every provider call is asynchronous and collected on an event, so nothing blocks.
- This ADR named `roles/provider-dispatch.md`, `roles/provider-context.md` and `orchestrate.conf`. The dispatch hook, `skills/develop/provider-context.md` and `orchestrate.json` replaced them.
