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

<!-- PHASES: DISCOVER, DEFINE, DEVELOP, DELIVER appended by subsequent tasks -->
