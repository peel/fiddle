---
name: fiddle:orchestrate
description: Use when starting a full development lifecycle for a feature or epic — chains discover, define, develop, deliver phases with multi-model support and reaction engine
disable-model-invocation: true
argument-hint: <topic> [--epic <id>] [--skip-discover] [--providers codex,gemini]
---

# Orchestrate

Automated outer loop: DISCOVER → DEFINE → DEVELOP → DELIVER. Chains existing skills with multi-model input and a reaction engine that self-heals before escalating.

ARGUMENTS: {ARGS}

## Configuration

### CLI Flags

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--epic <id>` | none | Resume an existing epic. Skips DISCOVER/DEFINE if beans exist |
| `--skip-discover` | false | Jump straight to DEFINE |
| `--providers <list>` | per-phase defaults | Global provider override (comma-separated) |
| `--discover-providers <list>` | codex | Override DISCOVER phase providers |
| `--define-providers <list>` | codex,gemini | Override DEFINE phase providers |
| `--develop-providers <list>` | none | Override DEVELOP phase providers |
| `--develop-holistic-providers <list>` | codex | Override holistic review providers |
| `--deliver-providers <list>` | codex | Override DELIVER phase providers |
| `--workers <N>` | 2 | Parallel worker count for ralph |
| `--max-review-cycles <N>` | 3 | Max review cycles before escalating |
| `--max-total-turns <N>` | 200 | Max agent turns for ralph subagent |

### Config File

Read `.claude/orchestrate.conf` if it exists. Format is HCL:

```hcl
providers {
  discover         = ["codex"]
  define           = ["codex", "gemini"]
  develop          = []
  develop_holistic = ["codex"]
  deliver          = ["codex"]
}

ralph {
  workers            = 2
  max_review_cycles  = 3
  max_impl_turns     = 50
  max_review_turns   = 30
  max_total_turns    = 200
  ci_max_retries     = 3
  stall_timeout_min  = 15
  stall_max_respawns = 2
}
```

### Provider Defaults

| Phase | Default Providers | Rationale |
|---|---|---|
| DISCOVER | codex | Research depth from two code-oriented models |
| DEFINE (panel) | codex, gemini | Maximum perspectives for architectural decisions |
| DEVELOP (ralph) | none | Ralph's tiered review handles this |
| DEVELOP (holistic) | codex | Outside perspective on the full epic |
| DELIVER | codex | Drift detection and docs review |

Claude is implicit — always present, never listed. When a phase lists "codex", the actual participants are Claude + Codex.

### Merge Order

Defaults → config file → CLI flags. Later values override earlier ones. `--providers` sets all phases; per-phase flags override that.

## SETUP

Run this section immediately on invocation, before any phase.

### Step 1: Parse Configuration

1. Set provider defaults from the table above
2. If `.claude/orchestrate.conf` exists: read it with the Read tool. Parse each HCL block:
   - `providers {}` — override provider defaults for each phase
   - `ralph {}` — set workers, max_review_cycles, max_impl_turns, max_review_turns, max_total_turns, ci_max_retries, stall_timeout_min, stall_max_respawns
3. Parse CLI flags from `{ARGS}`. Override any config file values.
4. Store final config values for use throughout the session.

### Step 2: Validate Epic (if --epic)

If `--epic <id>` was provided:
```bash
beans show <id> --json
```
Confirm it exists and is type `epic` or `milestone`. If not found, stop and report error to user.

### Step 3: Create Status Pane

Split the current tmux window to create a status pane:
```bash
# Get current pane ID
CURRENT_PANE=$(tmux display-message -p '#{pane_id}')
# Split horizontally (side by side), 40% width for status
tmux split-window -h -l 40% "bash scripts/orchestrate-status.sh <epic-id>"
# Return focus to the main pane
tmux select-pane -t "$CURRENT_PANE"
```

If the status script is not available yet (placeholder), skip this step silently.

### Step 4: Initialize Event Log

```bash
mkdir -p .claude
echo "$(date +%H:%M) orchestrate started: <topic>" >> .claude/orchestrate-events.log
```

### Step 5: Determine Phase

If `--epic <id>` was provided, detect the current phase from bean state for resumption:

```bash
beans list --parent <epic-id> --json
```

- **No child beans exist** → start at DEFINE
- **Child beans in `todo` or `in-progress`** → start at DEVELOP
- **All child beans `completed` or tagged `needs-attention`, AND no commit message containing "docs-evolve"** → start at DELIVER
- **Docs already evolved** (check `git log --oneline --grep="docs-evolve"`) → DONE. Report completion.

If no `--epic` was provided, start at DISCOVER (or DEFINE if `--skip-discover`).

Log the phase:
```bash
echo "PHASE:<phase>" >> .claude/orchestrate-events.log
```

Jump to the determined phase section below.

## DISCOVER

Skip this phase if `--skip-discover` was set OR if `--epic` was provided and child beans already exist.

### Step 1: Docs Discovery

Invoke docs-discover to gather project context and identify gaps:
```
Skill(skill: "fiddle:docs-discover", args: "<topic>")
```

This reads existing docs, CLAUDE.md, beans, and relevant source files. It produces a structured summary of what exists, what's relevant, and what gaps remain.

### Step 2: External Research

If DISCOVER providers are configured (default: codex):

For each provider, call its MCP tool directly:

**Codex:**
```
mcp__codex__codex(
  prompt: "Topic: <topic>. Project context: <summary from Step 1>. Research: ecosystem patterns, prior art, implementation approaches, potential pitfalls. Be specific and cite concrete examples."
)
```

**Gemini:** Spawn via Bash:
```bash
gemini -o json --approval-mode auto_edit "Topic: <topic>. Project context: <summary from Step 1>. Research: ecosystem patterns, prior art, implementation approaches, potential pitfalls. Be specific and cite concrete examples."
```

If a provider's MCP server or CLI is not available, skip it. Claude proceeds with internal knowledge only.

### Step 3: Socratic Dialogue

Present findings to the user as a Socratic dialogue — Claude synthesizes the evidence and asks clarifying questions:

1. Summarize what you found (project context + external research)
2. Identify key decisions that need to be made
3. Ask the user to confirm the scope: "Based on this research, the scope appears to be: [X]. Does this match your intent? Any adjustments?"

Wait for user confirmation before proceeding.

### Step 4: Transition

```bash
echo "$(date +%H:%M) DISCOVER complete" >> .claude/orchestrate-events.log
echo "PHASE:DEFINE" >> .claude/orchestrate-events.log
```

Fall through to DEFINE.

## DEFINE

### Step 1: Brainstorming

Invoke the brainstorming skill:
```
Skill(skill: "superpowers:brainstorming")
```

This explores the user's intent, asks questions, and produces 2-3 candidate approaches. Follow the skill's instructions completely.

### Step 2: Panel Discussion

Invoke the panel skill on the proposed approaches:
```
Skill(skill: "fiddle:panel", args: "<approaches from brainstorming> --providers <define_providers>")
```

The panel runs structured adversarial analysis across configured providers. Wait for the panel's verdict.

**If full consensus:** Proceed automatically with the recommended approach. Report to user: "Panel reached consensus on [approach]. Proceeding."

**If disagreement:** Present the panel's output to the user. Ask them to pick an approach. Wait for their decision.

### Step 3: Implementation Planning

Invoke the writing-plans skill with the chosen approach:
```
Skill(skill: "superpowers:writing-plans")
```

This creates a detailed implementation plan and decomposes it into beans via `bean-decomposition`. Follow the skill's instructions completely.

After the plan is written and approved, beans should exist under an epic.

### Step 4: Capture Epic ID

If `--epic` was not provided at invocation:

```bash
# Find the newly created epic from the plan
beans list --json -t epic -s todo
```

Take the most recently created epic ID. Store it for the remaining phases.

### Step 5: Transition

```bash
echo "$(date +%H:%M) DEFINE complete — $(beans list --parent <epic-id> --json | jq 'length') beans created" >> .claude/orchestrate-events.log
echo "PHASE:DEVELOP" >> .claude/orchestrate-events.log
```

Fall through to DEVELOP.

## DEVELOP

### Step 1: Spawn Ralph Subagent

Read the ralph skill file to build the prompt:
```bash
cat .claude/skills/ralph-subs-implement/SKILL.md
```

Spawn ralph as a background subagent with fresh context:

```
ralph_task = Agent(
  name: "ralph-develop-<epic-id>",
  subagent_type: "general-purpose",
  model: "sonnet",
  mode: "bypassPermissions",
  run_in_background: true,
  max_turns: <max_total_turns>,
  prompt: "<contents of ralph SKILL.md, with the following args substituted:>
    --epic <epic-id> --workers <workers> --max-review-cycles <max_review_cycles>
    --max-impl-turns <max_impl_turns> --max-review-turns <max_review_turns>
    --ci-max-retries <ci_max_retries> --stall-timeout-min <stall_timeout_min>
    --stall-max-respawns <stall_max_respawns> --caller orchestrate"
)
```

For `critical` and `high` priority beans: include in the prompt an instruction for the review coordinator to additionally request a code review from configured DEVELOP providers via their MCP tools (codex: `mcp__codex__codex`, gemini: via CLI).

Wait for the result:
```
result = TaskOutput(task_id: ralph_task.id, block: true, timeout: 3600000)
```

Log:
```bash
echo "$(date +%H:%M) ralph subagent returned" >> .claude/orchestrate-events.log
```

### Step 2: Handle Ralph Result

Parse the `result` from Step 1:

**Case 1 — `RALPH_STATUS: COMPLETE`:**
Ralph finished all beans successfully. Proceed to Step 3 (Holistic Review).

**Case 2 — `RALPH_STATUS: PARKED`:**
Some beans need attention. Parse the needs-attention bean list from the result.

Present to user:
```
"Waiting on your input for N beans:
- <bean-id>: <title> — <reason>
- ...

You can: fix the issue and remove needs-attention tag, scrub the bean, or rework the scope."
```

Log:
```bash
echo "$(date +%H:%M) ralph parked — waiting on user for ${N} needs-attention" >> .claude/orchestrate-events.log
```

Wait for the user to address the parked beans. When they respond, respawn ralph — loop back to Step 1.

**Case 3 — Empty result, error, or max_turns exhausted:**
Check bean state:
```bash
beans list --parent <epic-id> --json
```

Present bean summary to user (completed, in-progress, todo, needs-attention counts). Ask: "Ralph's context was exhausted. Respawn to continue, or proceed to holistic review with current state?"

- If user says respawn → loop to Step 1
- If user says proceed → Step 3

### Step 3: Holistic Review

When all epic beans are `completed` or `needs-attention` (none in `todo` or `in-progress`):

1. Run the external holistic review. If DEVELOP holistic providers are configured, request comparison via their MCP tools:

   **Codex:**
   ```
   mcp__codex__codex(
     prompt: "Design doc: <design doc content>. Full diff: <git diff main...epic/<epic-id>>. Did the implementation match the design? Flag: inconsistencies, missed requirements, naming conflicts, dead code."
   )
   ```

   **Gemini:** Spawn via Bash with the same prompt.

2. If no provider is available, perform the holistic review yourself: read the design doc, review the full diff, and compare.
3. If holistic review creates fix beans → log "back to DEVELOP", loop to Step 1
4. If clean → transition to DELIVER

### Step 4: Transition

```bash
echo "$(date +%H:%M) DEVELOP complete" >> .claude/orchestrate-events.log
echo "PHASE:DELIVER" >> .claude/orchestrate-events.log
```

Fall through to DELIVER.

## DELIVER

### Step 1: Drift Analysis

If DELIVER providers are configured (default: codex), request drift analysis via their MCP tools:

**Codex:**
```
mcp__codex__codex(
  prompt: "Design doc: <read the design doc referenced in the epic bean body>. Full diff: <git diff main...epic/<epic-id> or git diff main...HEAD>. Analyze: did the implementation match the design? Flag any drift, missing features, scope creep, or unintended changes."
)
```

**Gemini:** Spawn via Bash with the same prompt.

If no provider is available, perform the drift analysis yourself: read the design doc, review the full diff, and compare.

Present the drift analysis to the user:
```
"Drift analysis complete:
- Implemented as designed: [list]
- Drift detected: [list with explanations]
- Missing from design: [list]
- Added beyond design: [list]

Proceed with documentation update?"
```

Wait for user confirmation before proceeding.

### Step 2: Documentation Update

Invoke docs-evolve automatically:
```
Skill(skill: "fiddle:docs-evolve", args: "--epic <epic-id>")
```

This updates SYSTEM.md, creates ADRs for architectural decisions, and appends to BACKLOG.md.

Present the docs-evolve results to the user for confirmation. Wait for approval.

### Step 3: Close Epic

After user confirms documentation:
```bash
beans update <epic-id> --status completed
echo "$(date +%H:%M) DELIVER complete — epic closed" >> .claude/orchestrate-events.log
```

Fall through to CLEANUP.

## CLEANUP

### Step 1: Kill Status Pane

```bash
# Find and kill the status pane (it's running orchestrate-status.sh)
tmux list-panes -F '#{pane_id} #{pane_current_command}' | grep orchestrate-status | awk '{print $1}' | xargs -I{} tmux kill-pane -t {}
```

If the pane doesn't exist, skip silently.

### Step 2: Remove Event Log

```bash
rm -f .claude/orchestrate-events.log
```

### Step 3: Summary

Count final bean states:
```bash
beans list --parent <epic-id> --json
```

Report to user:
```
"Epic <epic-id> complete.
- <N> beans completed
- <M> beans needs-attention (unresolved)
- Total duration: <first event timestamp> to now"
```

Remind the user: "Run `/fiddle:docs-evolve --epic <epic-id>` to update project docs." (if docs-evolve was not already run in DELIVER).
