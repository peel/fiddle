---
name: define
description: Run the DEFINE phase — brainstorm approaches with optional panel enrichment, challenge chosen design, create implementation plan and beans. Use standalone or as part of orchestrate.
---

# Define


## Usage

Invoke as `fiddle:define <topic> [--skip-challenge] [--skip-panel]`.

Turn a confirmed scope into a validated design and implementation plan with beans ready for execution.

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--skip-challenge` | false | Skip the challenge step after design approval |
| `--skip-panel` | false | Skip panel enrichment during brainstorming |

### Config File

Read `orchestrate.json` (project root) if it exists. Extract:
- `providers.phases.define` — provider list for this phase (default: `["codex", "gemini"]`)
- Provider declarations (`providers.<name>.command`, `.flags`) for each provider
- `models.define` — model override for panel advocates and brainstorming subagents

CLI flags override config file values.

## Steps

### Step 1: Brainstorming

Build the brainstorming args:
- Always include `--from-orchestrate` (so brainstorming returns control after the design doc instead of chaining to writing-plans)
- If `--skip-panel` was set, include `--skip-panel`

Invoke:
Use the `fiddle:brainstorm` skill with `--from-orchestrate` and include `--skip-panel` if configured.

This explores intent, asks questions, proposes 2-3 approaches, runs panel enrichment (if providers are available and panel not skipped), and produces a design doc. Follow the skill's instructions completely.

### Step 2: Grill Design

Skip if `--skip-challenge` was set.

Invoke the challenge skill to stress-test the chosen design:
Use the `fiddle:challenge` skill with `--phase define`.

This walks the decision tree on edge cases, integration points, failure modes, panel dissent points, and sizing assumptions. Catches design holes before they become wasted beans.

If challenges surface issues that require design changes, update the design doc and commit the changes before proceeding.

### Step 3: Implementation Planning

Invoke the writing-plans skill:
Use the `fiddle:write-plan` skill with `--from-orchestrate`.

This creates a detailed implementation plan and decomposes it into beans. The `--from-orchestrate` flag causes it to return control here after bean creation instead of presenting execution handoff options.

### Step 4: Capture Epic ID

Find the newly created epic:
```bash
beans list --json -t epic -s todo
```

Take the most recently created epic ID. Report it to the user.
