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

Read `orchestrate.conf` (project root) if it exists. Format is HCL:

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

  discover         = ["codex"]
  define           = ["codex", "gemini"]
  develop          = []
  develop_holistic = ["codex"]
  deliver          = ["codex"]

  timeout {
    attended   = 120
    unattended = 90
  }
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

models {
  # discover = "sonnet"
  # define   = "sonnet"
  # deliver  = "sonnet"

  develop {
    # standard = "sonnet"
    lite = "sonnet"
  }
}

develop {
  # execution = "ralph-subs"  // or "tmux-team", "hands-on", "hands-on-parallel"
}
```

### Model Defaults

| Config key | Roles | Default |
|---|---|---|
| models.discover | All DISCOVER subagents | "default" (session model) |
| models.define | Panel advocates, brainstorming subagents | "default" |
| models.develop.standard | Implementers, tier-2 review, ralph orchestrator | "default" |
| models.develop.lite | Tier-1 review (quick pass) | "sonnet" |
| models.deliver | Drift analysis, docs review | "default" |

"default" means inherit the session model — the agent omits the `model:` parameter so the parent's model is used. Omitted keys are treated as "default".

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

1. Set provider defaults from the table above. Set model defaults from the Model Defaults table.
2. If `orchestrate.conf` exists (project root): read it with the Read tool. Parse each HCL block:
   - `providers {}` — override provider defaults for each phase
   - `ralph {}` — set workers, max_review_cycles, max_impl_turns, max_review_turns, max_total_turns, ci_max_retries, stall_timeout_min, stall_max_respawns
   - `models {}` — override model defaults for each phase. Nested `develop {}` block contains `standard` and `lite` keys. "default" means omit the `model:` parameter to inherit the session model.
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

### Step 4: Determine Phase

If `--epic <id>` was provided, detect the current phase from bean state for resumption:

```bash
beans list --parent <epic-id> --json
```

- **No child beans exist** → start at DEFINE
- **Child beans in `todo` or `in-progress`** → start at DEVELOP
- **All child beans `completed` or tagged `needs-attention`, AND no commit message containing "docs-evolve"** → start at DELIVER
- **Docs already evolved** (check `git log --oneline --grep="docs-evolve"`) → DONE. Report completion.

If no `--epic` was provided, start at DISCOVER (or DEFINE if `--skip-discover`).

Set the phase tag on the epic bean (if epic exists):
```bash
beans update <epic-id> --tag orchestrate-phase:<phase>
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

Read `roles/provider-dispatch.md` (resolve relative to this skill's base directory: `../ralph-subs-implement/roles/provider-dispatch.md`). Follow the dispatch procedure for each provider in the discover phase list with these template values:

- `PROVIDER_ROLE` = "Research analyst"
- `TOPIC` = `<topic>`
- `INSTRUCTIONS` = "Research: ecosystem patterns, prior art, implementation approaches, potential pitfalls. Be specific and cite concrete examples."

Also read `roles/provider-context.md` (`../ralph-subs-implement/roles/provider-context.md`) for prompt construction.

Dispatch all providers in parallel. Collect results in **attended** mode.

If no provider CLI is available, skip and proceed with Claude's internal knowledge only.

### Step 3: Socratic Dialogue

Present findings to the user as a Socratic dialogue — Claude synthesizes the evidence and asks clarifying questions:

1. Summarize what you found (project context + external research)
2. Identify key decisions that need to be made
3. Ask the user to confirm the scope: "Based on this research, the scope appears to be: [X]. Does this match your intent? Any adjustments?"

Wait for user confirmation before proceeding.

### Step 4: Transition

```bash
beans update <epic-id> --remove-tag orchestrate-phase:DISCOVER --tag orchestrate-phase:DEFINE
```

Note: if epic does not yet exist at end of DISCOVER, skip the tag update — DEFINE will set it after epic creation.

Fall through to DEFINE.

## DEFINE

### Step 1: Brainstorming

Invoke the brainstorming skill with the orchestrate flag:
```
Skill(skill: "superpowers:brainstorming", args: "--from-orchestrate")
```

This explores the user's intent, asks questions, proposes 2-3 approaches, runs panel enrichment (if providers are available), and produces a design doc. The `--from-orchestrate` flag causes the skill to return control here after writing the design doc instead of chaining to writing-plans. Follow the skill's instructions completely.

### Step 2: Implementation Planning

Invoke the writing-plans skill with the orchestrate flag:
```
Skill(skill: "superpowers:writing-plans", args: "--from-orchestrate")
```

This creates a detailed implementation plan and decomposes it into beans. The `--from-orchestrate` flag causes the skill to return control here after bean creation instead of presenting execution handoff options.

### Step 3: Capture Epic ID

If `--epic` was not provided at invocation:

```bash
# Find the newly created epic from the plan
beans list --json -t epic -s todo
```

Take the most recently created epic ID. Store it for the remaining phases.

### Step 4: Transition

```bash
beans update <epic-id> --remove-tag orchestrate-phase:DEFINE --tag orchestrate-phase:DEVELOP
```

Fall through to DEVELOP.

## DEVELOP

### Step 0: Execution Choice

Check `orchestrate.conf` for a `develop.execution` setting. If set, use that value without prompting. If not set, present options and **wait for the user to pick a number**:

```
"Beans are ready. Pick an execution mode (1-4):

1. Ralph Subs — automated background subagent with implement/review cycles
2. Tmux Team — parallel workers in tmux panes via conductor agent
3. Hands-on (this session) — superpowers:subagent-driven-development with human checkpoints
4. Hands-on (parallel session) — superpowers:executing-plans in a new session"
```

<HARD-GATE>
Do NOT proceed until the user has explicitly chosen 1, 2, 3, or 4. Do NOT assume a default. Do NOT auto-select. Wait for their response.
</HARD-GATE>

- **If Ralph Subs:** proceed to Step 1 (Spawn Ralph Subagent) as normal.
- **If Tmux Team:** proceed to Step 1 but use `ralph-beans-implement` (team variant) instead of `ralph-subs-implement`.
- **If Hands-on (this session):** invoke `Skill(skill: "superpowers:subagent-driven-development")`. When execution completes, proceed to Step 3 (Holistic Review).
- **If Hands-on (parallel session):** guide the user to open a new session and run `superpowers:executing-plans`. Wait for the user to signal completion, then proceed to Step 3 (Holistic Review).

### Step 1: Spawn Ralph Subagent

Read the ralph skill file to build the prompt. Which skill depends on the execution choice from Step 0:
- **Ralph Subs (option 1):** `ralph-subs-implement/SKILL.md`
- **Tmux Team (option 2):** `ralph-beans-implement/SKILL.md`

Use the Read tool to load the SKILL.md file. The file is in a sibling directory relative to this skill's base directory: `../<ralph-variant>/SKILL.md` (resolve against the "Base directory for this skill" shown when this skill was loaded). Do NOT use the Skill tool — these skills have `disable-model-invocation` since they are agent prompts, not directly invocable skills.

Spawn ralph as a background subagent with fresh context:

```
ralph_task = Agent(
  name: "ralph-develop-<epic-id>",
  subagent_type: "general-purpose",
  model: <models.develop.standard>,  # if "default", omit model parameter to inherit session model
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

For `critical` and `high` priority beans: include in the prompt an instruction for the review coordinator to additionally request a code review from configured DEVELOP providers via the provider-dispatch procedure.

Wait for the result:
```
result = TaskOutput(task_id: ralph_task.id, block: true, timeout: 3600000)
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

1. Run the external holistic review. If DEVELOP holistic providers are configured, read `roles/provider-dispatch.md` (`../ralph-subs-implement/roles/provider-dispatch.md`) and follow the dispatch procedure for each provider in the develop_holistic phase list with these template values:

   - `PROVIDER_ROLE` = "Holistic reviewer"
   - `TOPIC` = "Epic holistic review for `<epic-id>`"
   - `DESIGN_DOC` = `<design doc content>`
   - `DIFF` = `<git diff main...epic/<epic-id>>`
   - `INSTRUCTIONS` = "Did the implementation match the design? Flag: inconsistencies, missed requirements, naming conflicts, dead code."

   Also read `roles/provider-context.md` (`../ralph-subs-implement/roles/provider-context.md`) for prompt construction.

   Dispatch all providers in parallel. Collect results in **unattended** mode (first-past-the-post).

2. If no provider is available, perform the holistic review yourself: read the design doc, review the full diff, and compare.
3. If holistic review creates fix beans → log "back to DEVELOP", loop to Step 1
4. If clean → transition to DELIVER

### Step 4: Transition

```bash
beans update <epic-id> --remove-tag orchestrate-phase:DEVELOP --tag orchestrate-phase:DELIVER
```

Fall through to DELIVER.

## DELIVER

### Step 1: Drift Analysis

If DELIVER providers are configured (default: codex), read `roles/provider-dispatch.md` (`../ralph-subs-implement/roles/provider-dispatch.md`) and follow the dispatch procedure for each provider in the deliver phase list with these template values:

- `PROVIDER_ROLE` = "Drift analyst"
- `TOPIC` = "Design vs implementation drift for `<epic-id>`"
- `DESIGN_DOC` = `<read the design doc referenced in the epic bean body>`
- `DIFF` = `<git diff main...epic/<epic-id> or git diff main...HEAD>`
- `INSTRUCTIONS` = "Analyze: did the implementation match the design? Flag any drift, missing features, scope creep, or unintended changes."

Also read `roles/provider-context.md` (`../ralph-subs-implement/roles/provider-context.md`) for prompt construction.

Dispatch all providers in parallel. Collect results in **attended** mode.

If no provider CLI is available, perform the drift analysis yourself: read the design doc, review the full diff, and compare.

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
```

Fall through to CLEANUP.

## CLEANUP

### Step 1: Kill Status Pane

```bash
# Find and kill the status pane (it's running orchestrate-status.sh)
tmux list-panes -F '#{pane_id} #{pane_current_command}' | grep orchestrate-status | awk '{print $1}' | xargs -I{} tmux kill-pane -t {}
```

If the pane doesn't exist, skip silently.

### Step 2: Clean Phase Tag

```bash
beans update <epic-id> --remove-tag orchestrate-phase:DELIVER
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
- <M> beans needs-attention (unresolved)"
```

Remind the user: "Run `/fiddle:docs-evolve --epic <epic-id>` to update project docs." (if docs-evolve was not already run in DELIVER).
