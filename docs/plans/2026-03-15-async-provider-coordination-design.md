# Async Provider Coordination Design

Replace synchronous, blocking provider calls (Codex MCP, inline Gemini CLI) with parallel background tasks using CLI-only invocation, event-driven collection, and a standardized context transfer protocol.

## Motivation

Today, provider calls in panel debates and reviews are sequential in practice. Codex MCP and Gemini Bash calls block the calling agent. A "parallel" panel round actually runs Claude (background) → Codex (blocking) → Gemini (blocking). If a provider hangs, the session is stuck with no timeout or notification.

Inspired by the agentic-coding-squad project's hook-based signaling pattern, but implemented using Claude Code's native `run_in_background` + `TaskOutput` primitives which already provide event-driven notification without custom hooks.

## Decision: CLI-Only Providers

Drop Codex MCP (`mcp__codex__codex`) in favor of Codex CLI (`codex exec`). Both providers become uniform CLI tools invoked from the project directory. Benefits:

- No per-project MCP configuration maintenance
- Uniform invocation pattern for all providers
- CLIs have full filesystem access — they read the codebase directly
- Explicit CLI commands in config — no agent token waste on discovery

## Config Changes

`orchestrate.conf` gets explicit provider CLI definitions and phase-specific timeout settings:

```hcl
providers {
  codex {
    command = "codex exec"
    flags   = "--json -s read-only"
  }
  gemini {
    command = "gemini"
    flags   = "-o json --approval-mode auto_edit"
  }

  # Phase assignments (which providers participate)
  discover         = ["codex"]
  define           = ["codex", "gemini"]
  develop          = []
  develop_holistic = ["codex"]
  deliver          = ["codex"]

  # Timeout strategy per phase mode
  timeout {
    attended   = 120   # DISCOVER/DEFINE — soft timeout, escalate to user
    unattended = 90    # DEVELOP — hard timeout, kill and proceed
  }
}
```

The `command` + `flags` are read verbatim by the dispatch procedure. The existing `ralph {}` block stays unchanged.

## Provider Context Template

New file: `skills/ralph-subs-implement/roles/provider-context.md`

```markdown
# Provider Context

Respond with your analysis only — no preamble, no meta-commentary.

## Role
{PROVIDER_ROLE}

## Topic
{TOPIC}

## Approaches
{APPROACHES}

## Design Document
{DESIGN_DOC}

## Diff
{DIFF}

## Previous Feedback
{PREVIOUS_FEEDBACK}

## Instructions
{INSTRUCTIONS}
```

Placeholders are substituted by the calling skill. Sections with no value are stripped entirely — providers never see empty headers. CLIs read project constraints, codebase structure, and conventions from the filesystem via shared AGENTS.md (see Setup section).

## Provider Dispatch Procedure

New file: `skills/ralph-subs-implement/roles/provider-dispatch.md`

An on-demand procedure (like `lead-procedures.md`) that documents the exact steps:

### Read Config

1. Read `orchestrate.conf` → `providers.<name>` block for `command` and `flags`
2. If provider not in config, skip it silently

### Build Prompt

1. Read `roles/provider-context.md`
2. Substitute placeholders with values from the calling context
3. Strip sections where the value is empty
4. Write the final prompt to a temp file:
   ```bash
   PROMPT_FILE=$(mktemp /tmp/provider-XXXX.md)
   ```

### Dispatch (Background)

Fire the provider CLI as a background Bash task:

```
task = Bash(
  run_in_background: true,
  command: "<command> <flags> < \"$PROMPT_FILE\""
)
```

Record the task ID and provider name. When dispatching multiple providers, fire ALL in a single message — true parallelism.

### Collect Results

**Attended mode (DISCOVER, DEFINE):**

For each task, call `TaskOutput(task_id, timeout: attended * 1000)`.

If timeout fires before result, present to user:
- "`<provider>` has not responded after `<timeout>`s. Options: (1) keep waiting, (2) respawn, (3) kill and proceed without it"
- **Keep waiting**: re-call TaskOutput with extended timeout
- **Respawn**: TaskStop the hung task, re-run Dispatch for that provider only, collect again
- **Kill**: TaskStop and proceed with available results

**Unattended mode (DEVELOP):**

For each task, call `TaskOutput(task_id, timeout: unattended * 1000)`.

If timeout fires: TaskStop and proceed without. First-past-the-post: if 2+ providers have returned and remaining providers time out, proceed with available results. Do not wait for stragglers.

### Cleanup

```bash
rm -f "$PROMPT_FILE"
```

## Call Site Changes

### 1. Panel Phase 1 (independent positions)

Before: Claude Agent (background) + Codex MCP (blocking) + Gemini Bash (blocking) — sequential.
After: Claude Agent (background) + Codex CLI (background) + Gemini CLI (background) — all fired in one message. Collect via TaskOutput.

### 2. Panel Phase 2+ (cross-review rounds)

Same transformation. Previous round's positions go into `{PREVIOUS_FEEDBACK}`.

### 3. Orchestrate DISCOVER Step 2 (external research)

Follow provider-dispatch with attended mode. Topic + instructions into template.

### 4. Orchestrate DEVELOP Step 3 (holistic review)

Follow provider-dispatch with unattended mode. Design doc + full diff into template. First-past-the-post.

### 5. Orchestrate DELIVER Step 1 (drift analysis)

Follow provider-dispatch with attended mode. Design doc + diff into template.

## Migration — What Gets Removed

- All `mcp__codex__codex()` calls from panel SKILL.md and orchestrate SKILL.md
- All inline `gemini -o json ...` strings from skill files
- MCP availability checks in panel (`Check if mcp__codex__codex tool exists`)
- `session-start-check-providers.sh` updated: check CLI binaries on PATH instead of MCP config
- `/fiddle:init` simplified: verify CLIs on PATH, no MCP configuration flow for codex

## Setup

Document in README: symlink CLAUDE.md to AGENTS.md so all provider CLIs (codex, gemini) share the same project context baseline. The skills assume CLIs have project context because they run in the project directory.

## What We Did NOT Add

- No custom hooks — Claude Code's native `run_in_background` + `TaskOutput` already provides event-driven notification
- No polling loops — TaskOutput blocks until completion or timeout, response is processed immediately
- No retry logic — if a provider fails, proceed without it (attended mode lets user choose to respawn)
- No shell wrapper script — dispatch logic stays in markdown procedures, matching fiddle's "skills are markdown" paradigm
