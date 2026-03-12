---
name: orchestrate
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
   - `ralph {}` — set workers, max_review_cycles, max_impl_turns, max_review_turns
   - `reaction {}` — set ci_max_retries, stall_timeout_min, stall_max_respawns
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

### Step 1: Read Project Context

Gather context about the topic from the current project:

1. Read `docs/` directory — scan for design docs, ADRs, SYSTEM.md, BACKLOG.md
2. Read `CLAUDE.md` — understand project conventions and architecture
3. Check existing beans: `beans list --json` — understand what work already exists
4. Read relevant source files based on the topic (use Glob and Grep to find them)

Compile findings into a structured summary: what exists, what's relevant, what gaps remain.

### Step 2: External Research (multi_mcp)

If DISCOVER providers are configured (default: codex):

For each provider, call multi_mcp `chat`:
```
multi_mcp chat(
  provider: "<provider>",
  prompt: "Topic: <topic>. Project context: <summary from Step 1>. Research: ecosystem patterns, prior art, implementation approaches, potential pitfalls. Be specific and cite concrete examples."
)
```

If multi_mcp is not available, skip this step. Claude proceeds with internal knowledge only.

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
Skill(skill: "peel:panel", args: "<approaches from brainstorming> --providers <define_providers>")
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

### Step 1: Invoke Ralph

Invoke ralph-subs-implement with the epic and ralph configuration:

```
Skill(skill: "ralph-subs-implement", args: "--epic <epic-id> --workers <workers> --max-review-cycles <max_review_cycles>")
```

Ralph handles the full implement → review cycle for each bean. Let it run.

For `critical` and `high` priority beans: instruct the review coordinator to additionally call multi_mcp `code_review` with configured DEVELOP providers, if any are set.

### Step 2: Reaction Engine

The reaction engine monitors between ralph's turns. After each ralph turn completes (you regain control), run these checks before handing back to ralph:

#### CI Failure Detection

For each `in-progress` bean, check its tags:
```bash
beans show <bean-id> --json
```

If the bean has a `ci-retries:N` tag:
- If `N < ci_max_retries`: ralph will handle the retry. No action needed.
- If `N >= ci_max_retries`: escalate.
  ```bash
  beans update <bean-id> --tag needs-attention
  echo "$(date +%H:%M) impl-<bean-id> failed ${N}x → needs attention" >> .claude/orchestrate-events.log
  ```

#### Stall Detection

For each `in-progress` bean, check the `## Progress` section in its body:
```bash
beans show <bean-id> --json
```

Parse the last timestamp from `## Progress` entries (format: `- HH:MM ...`). If the last entry is older than `stall_timeout_min` minutes:

Check the `stall-respawns:N` tag:
- If `N < stall_max_respawns` (or tag doesn't exist):
  ```bash
  beans update <bean-id> --tag stall-respawns:$((N+1))
  echo "$(date +%H:%M) impl-<bean-id> stalled → respawned ($((N+1))/${stall_max_respawns})" >> .claude/orchestrate-events.log
  ```
  Ralph's next cycle will spawn a fresh implementer that reads `## Progress` and continues.

- If `N >= stall_max_respawns`: escalate.
  ```bash
  beans update <bean-id> --tag needs-attention
  echo "$(date +%H:%M) impl-<bean-id> stalled ${N}x → needs attention" >> .claude/orchestrate-events.log
  ```

#### Review Overflow

If a bean's review cycle count (from tags) reaches `max_review_cycles`:
```bash
beans update <bean-id> --tag needs-attention
echo "$(date +%H:%M) review-<bean-id> overflow → needs attention" >> .claude/orchestrate-events.log
```

#### Tag Reset

When a bean advances from implement to review phase, reset retry tags:
```bash
beans update <bean-id> --remove-tag ci-retries:* --remove-tag stall-respawns:*
```

#### All Beans Parked

After running checks, if:
- No unblocked `todo` beans remain
- No `in-progress` beans remain
- Some beans are tagged `needs-attention`

Then notify the user:
```
"Waiting on your input for N beans:
- <bean-id>: <title> — <reason from event log>
- ...

You can: fix the issue and remove needs-attention tag, scrub the bean, or rework the scope."
```

Log:
```bash
echo "$(date +%H:%M) all beans parked — waiting on user for ${N} needs-attention" >> .claude/orchestrate-events.log
```

Wait for the user to address the parked beans. When they remove `needs-attention` tags or scrub beans, resume ralph.

### Step 3: Holistic Review

When all epic beans are `completed` or `needs-attention` (none in `todo` or `in-progress`):

1. Ralph's epic holistic review runs automatically (opus model) — it reviews the full diff across all beans
2. If DEVELOP holistic providers are configured, call multi_mcp `compare`:
   ```
   multi_mcp compare(
     provider: "<develop_holistic_provider>",
     prompt: "Design doc: <design doc content>. Full diff: <git diff main...epic/<epic-id>>. Did the implementation match the design? Flag: inconsistencies, missed requirements, naming conflicts, dead code."
   )
   ```
3. If holistic review creates fix beans → log "back to DEVELOP", loop to Step 1
4. If clean → transition to DELIVER

### Step 4: Transition

```bash
echo "$(date +%H:%M) DEVELOP complete" >> .claude/orchestrate-events.log
echo "PHASE:DELIVER" >> .claude/orchestrate-events.log
```

Fall through to DELIVER.

<!-- PHASE: DELIVER appended by subsequent task -->
