---
name: orchestrate
description: Use when starting a full development lifecycle for a feature or epic — chains discover, define, develop, deliver phases with multi-model support and reaction engine
---

# Orchestrate


## Usage

Invoke as `fiddle:orchestrate <topic> [--epic <id>] [--no-triage] [--skip-discover] [--skip-challenge]`.

Automated outer loop: DISCOVER → DEFINE → DEVELOP → DELIVER. Sequences phase skills with configuration, status tracking, and resumption support.

Each phase is an independent skill (`fiddle:discover`, `fiddle:define`, `fiddle:develop`, `fiddle:deliver`) that can also be invoked standalone. Orchestrate's job is to sequence them, pass through configuration, and handle phase transitions.

ARGUMENTS: {ARGS}

## Configuration

### CLI Flags

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--epic <id>` | none | Resume an existing epic. Skips DISCOVER/DEFINE if beans exist |
| `--no-triage` | false | Skip quick-path triage, go straight to full flow |
| `--skip-discover` | false | Jump straight to DEFINE |
| `--skip-docs` | false | Passed through to discover phase — skip discover-docs |
| `--skip-challenge` | false | Passed through to discover and define phases |
| `--skip-panel` | false | Passed through to define phase |

Provider configuration lives in `orchestrate.json` only — no CLI overrides. Each phase reads its provider list from `providers.phases.<phase>`. Available providers are auto-detected at session start (see `hooks/session-start-check-providers.sh`).

### Config File

`orchestrate.json` in the project root is the live configuration and the authority on every value. This is the one place the file's schema is documented; other skills name the keys they read and link here instead of repeating the block, so a single copy cannot drift out of agreement with itself.

```json
{
  "providers": {
    "codex": { "command": "codex exec", "flags": "--json -s read-only" },
    "gemini": { "command": "gemini", "flags": "-o json --approval-mode auto_edit" },
    "phases": {
      "discover": ["codex"],
      "define": ["codex", "gemini"],
      "develop": [],
      "develop_holistic": ["codex"],
      "deliver": ["codex"]
    },
    "timeout": { "attended": 120, "unattended": 90 }
  },
  "evaluators": {
    "attended": false,
    "max_dispatches_per_task": 16,
    "domains": {
      "general": {
        "template": "evaluator-general",
        "providers": ["claude", "codex"],
        "calibration": "docs/evaluator-calibration-general.md",
        "antipatterns": "docs/antipatterns-general.md",
        "thresholds": {}
      }
    },
    "holistic": { "providers": ["claude"], "max_iterations": 4 },
    "spot_check": { "rate": 5 },
    "aging": { "window_days": 90, "quiet_epics": 3 }
  },
  "deliver": {
    "product_artifacts": {
      "templates_path": "docs/product/templates",
      "output_path": "docs/releases",
      "artifacts": ["release-notes", "social"]
    }
  },
  "plans": {}
}
```

The values above are the committed ones. Fallbacks apply only when a key is absent: `max_dispatches_per_task` 16, `holistic.max_iterations` 3, `holistic.providers` `["claude"]`, `spot_check.rate` 5 (0 or less disables the spot-check), `aging.window_days` 90, `aging.quiet_epics` 3. Read the file rather than relying on these numbers — a fallback only tells you what happens when the key is missing, not what the project currently runs.

In `evaluators.domains.<domain>.providers`, the array is an ordered preference list for selecting the single evaluator for that domain: the first available provider that differs from the implementer wins (implementers are always claude). It is not a dispatch fan-out. `evaluators.holistic.providers` behaves differently: holistic review dispatches to all listed providers.

### Provider Defaults

| Phase | Default Providers | Rationale |
|---|---|---|
| DISCOVER | codex | Research depth from two code-oriented models |
| DEFINE (panel) | codex, gemini | Maximum perspectives for architectural decisions |
| DEVELOP | none | Develop's single-pass domain-expert review handles this |
| DEVELOP (holistic) | codex | Outside perspective on the full epic |
| DELIVER | codex | Drift detection and docs review |

Claude is implicit — always present, never listed. When a phase lists "codex", the actual participants are Claude + Codex.

### Merge Order

Defaults → config file → CLI flags. Later values override earlier ones. Providers come from config file only (no CLI override).

Orchestrate reads `orchestrate.json` once during SETUP and computes final values. Phase skills also read `orchestrate.json` for their own defaults when invoked standalone.

## SETUP

Run this section immediately on invocation, before any phase.

### Step 1: Parse Configuration

1. Set provider defaults from the table above.
2. If `orchestrate.json` exists (project root): read it and parse each JSON key:
   - `providers` — provider definitions and phase assignments
   - `evaluators` — evaluator configuration: `attended`, `max_dispatches_per_task`, and domain definitions (each domain's `providers` is an ordered preference list for the single evaluator, not a fan-out; `holistic.providers` is dispatch-all)
3. Parse CLI flags from `{ARGS}`. Override any config file values.
4. Store final config values for use throughout the session.

### Step 2: Validate Epic (if --epic)

If `--epic <id>` was provided:
```bash
beans show <id> --json
```
Confirm it exists and is type `epic` or `milestone`. If not found, stop and report error to user.

### Step 3: Determine Phase

If `--epic <id>` was provided, detect the current phase from bean state for resumption:

```bash
beans list --parent <epic-id> --json
```

- **No child beans exist** → start at DEFINE
- **Child beans in `todo` or `in-progress`** → start at DEVELOP
- **All child beans `completed` or tagged `needs-attention`, AND no commit message containing "deliver-docs"** → start at DELIVER
- **Docs already evolved** (check `git log --oneline --grep="deliver-docs"`) → DONE. Report completion.

If no `--epic` was provided, start at DISCOVER (or DEFINE if `--skip-discover`).

Set the phase tag on the epic bean (if epic exists):
```bash
beans update <epic-id> --tag orchestrate-phase:<phase>
```

Jump to the determined phase section below.

## TRIAGE

Skip this phase if any of these hold: `--epic` was provided, `--no-triage` was set, `--skip-discover` was set.

Assess the prompt against quick-path criteria to decide: quickfix or full flow.

### Quick Path Criteria (all must hold)

1. **Single-focus**: The prompt describes one change, not a set of changes or an initiative
2. **Clear approach**: The implementation path is obvious — no architectural decisions or design trade-offs needed
3. **Small scope**: Likely ≤5 files to create or modify
4. **No new infrastructure**: No new patterns, abstractions, services, or build pipeline changes
5. **Self-contained**: No cross-cutting concerns, no coordination across subsystems

### Assessment

Evaluate the prompt against each criterion. If every criterion is met:

Use the `fiddle:quickfix` skill with the original prompt.

If quickfix completes successfully (returns a PR URL) → orchestrate is **done**. Skip all remaining phases.

If quickfix returns **TOO_COMPLEX** → continue with DISCOVER below. The quickfix skill handles its own cleanup (bean scrapping, worktree removal).

If any criterion fails, skip quickfix and fall through to DISCOVER.

## DISCOVER

Skip this phase if `--skip-discover` was set OR if `--epic` was provided and child beans already exist.

Build args for the discover phase:
- `<topic>`
- `--skip-docs` (if set)
- `--skip-challenge` (if set)

Invoke:
Use the `fiddle:discover` skill with the built args.

Transition:
```bash
beans update <epic-id> --remove-tag orchestrate-phase:DISCOVER --tag orchestrate-phase:DEFINE
```

Note: if epic does not yet exist at end of DISCOVER, skip the tag update — DEFINE will set it after epic creation.

Fall through to DEFINE.

## DEFINE

Build args for the define phase:
- `<topic>`
- `--skip-challenge` (if set)
- `--skip-panel` (if set)

Invoke:
Use the `fiddle:define` skill with the built args.

### Capture Epic ID

If `--epic` was not provided at invocation:

```bash
# Find the newly created epic from the plan
beans list --json -t epic -s todo
```

Take the most recently created epic ID. Store it for the remaining phases.

Transition:
```bash
beans update <epic-id> --remove-tag orchestrate-phase:DEFINE --tag orchestrate-phase:DEVELOP
```

Fall through to DEVELOP.

## DEVELOP

Build args for the develop phase:
- `--epic <epic-id>`

Invoke:
Use the `fiddle:develop` skill with `--epic <epic-id>`.

Transition:
```bash
beans update <epic-id> --remove-tag orchestrate-phase:DEVELOP --tag orchestrate-phase:DELIVER
```

Fall through to DELIVER.

## DELIVER

Build args for the deliver phase:
- `--epic <epic-id>`

Invoke:
Use the `fiddle:deliver` skill with the built args.

## CLEANUP

### Step 1: Clean Phase Tag

```bash
beans update <epic-id> --remove-tag orchestrate-phase:DELIVER
```

### Step 2: Summary

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

Remind the user: "Run `fiddle:deliver-docs --epic <epic-id>` to update project docs." (if deliver-docs was not already run in DELIVER).
