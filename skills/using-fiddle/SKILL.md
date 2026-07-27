---
name: using-fiddle
description: Use when working with the Fiddle portable skills library, choosing lifecycle skills, or adapting Fiddle instructions across Claude, Codex, and Pi harnesses.
---

# Using Fiddle

Fiddle is a portable Agent Skills library for a development lifecycle:
DISCOVER -> DEFINE -> DEVELOP -> DELIVER.

## Routing

- Small, clear, one-shot implementation: use `fiddle:quickfix`.
- Full feature, epic, ambiguous design, or multi-step work: use `fiddle:orchestrate`.
- Existing epic with task beans ready: use `fiddle:develop`.
- Finished implementation needing drift/docs/archive work: use `fiddle:deliver`.
- Planning from an approved spec: use `fiddle:write-plan`.
- Debugging, verification, TDD, or worktree setup: use the matching `fiddle:debug`, `fiddle:verify`, `fiddle:tdd`, or `fiddle:worktrees` skill.

## Harness Mapping

Fiddle skills may contain Claude-style tool names as shorthand for agent actions. Load the mapping for the current harness when a skill mentions literal tools, background subagents, or plugin-root paths:

- Claude Code: `references/claude-tools.md`
- Codex: `references/codex-tools.md`
- Pi: `references/pi-tools.md`

Treat literal Claude API examples such as `Skill(...)` or `Agent(...)` as intent unless the current harness supports that exact call. Use the mapped mechanism instead.

## Provider Model Policy

Prefer inheriting the current session model. Only pass explicit model names when the user or local config requires one.

External providers are optional. If a provider CLI is unavailable, continue with the current harness and report the reduced coverage.
