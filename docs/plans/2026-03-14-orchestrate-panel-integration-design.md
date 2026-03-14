# Orchestrate-Panel Integration Design

## Problem

Brainstorming (`superpowers:brainstorming`) hardcodes "invoke writing-plans" as its terminal state. When orchestrate calls brainstorming in the DEFINE phase, it chains directly into writing-plans — skipping the panel and preventing orchestrate from regaining control for subsequent phases (DEVELOP, DELIVER).

Additionally, writing-plans presents execution handoff options that conflict with orchestrate's DEVELOP phase, which should own the execution choice.

## Solution

Integrate the panel into brainstorming as an enrichment step. Make both brainstorming and writing-plans orchestrate-aware so they return control when called from orchestrate.

### Orchestrate Context Detection

Both skills detect orchestrate context by checking for `.claude/orchestrate-events.log` containing `PHASE:DEFINE`. This file is created by orchestrate's SETUP phase before DEFINE begins.

- **Present:** Return control to orchestrate (do not chain to next skill).
- **Absent:** Standalone use — chain as today.

## Changes

### 1. Brainstorming Patch

**Panel enrichment (after step 3 — propose approaches):**

- Parse `{ARGS}` for `--skip-panel`.
- If `--skip-panel` is not set: check if external providers are available (codex MCP tool exists, gemini on PATH).
- If providers available: invoke `fiddle:panel` with the proposed approaches. Present panel commentary (consensus, disagreements, tradeoffs) alongside the approaches when asking the user to pick.
- If `--skip-panel` is set or no providers available: skip, continue as today.

**Orchestrate-aware terminal state (after step 5 — write design doc):**

- Check for `.claude/orchestrate-events.log` with `PHASE:DEFINE`.
- If present: STOP. Do not invoke writing-plans. Control returns to orchestrate.
- If absent: invoke writing-plans as today.

### 2. Writing-Plans Patch Update

**Orchestrate-aware handoff (after bean creation):**

- Check for `.claude/orchestrate-events.log` with `PHASE:DEFINE`.
- If present: STOP after bean creation. No handoff prompt. Control returns to orchestrate.
- If absent: present handoff options as today (Subagent-Driven, Parallel Session, Beans Batch, Ralph Beans).

Bean creation section is unchanged.

### 3. Orchestrate DEFINE Phase

Simplified from 5 steps to 4:

1. **Brainstorming** — `Skill(skill: "superpowers:brainstorming")`. Includes panel enrichment internally. Returns control after design doc.
2. **Implementation Planning** — `Skill(skill: "superpowers:writing-plans")`. Produces plan + beans. Returns control after bean creation.
3. **Capture Epic ID** — `beans list --json -t epic -s todo`. Unchanged.
4. **Transition** — Log event, fall through to DEVELOP. Unchanged.

Old Step 2 (separate panel invocation) removed.

### 4. Orchestrate DEVELOP Phase

New Step 0 before spawning ralph:

**Execution choice** — present options to the user:

- **Ralph Subs (background subagent)** — spawn `ralph-subs-implement` as background Agent in this session. Automated implement/review cycles.
- **Tmux Team (conductor agent)** — launch parallel workers in tmux panes via conductor.
- **Hands-on** — user implements beans manually. Orchestrate waits for user to signal completion.

Execute based on choice. Remaining DEVELOP steps (holistic review, transition) unchanged.

### 5. Patch-Superpowers Skill Update

- Update overview: "Patches three cached skills: `writing-plans`, `executing-plans`, and `brainstorming`."
- Add brainstorming patch step (panel enrichment + orchestrate-aware terminal state).
- Update writing-plans patch to include orchestrate-aware handoff after bean creation.
- Executing-plans patch unchanged.

## Flow Diagrams

### Standalone brainstorming

```
explore → questions → propose approaches → [panel enrichment] →
present with commentary → user picks → design → design doc → writing-plans
```

### From orchestrate

```
DEFINE:
  brainstorming (explore → questions → approaches → [panel] → user picks → design → doc → STOP)
  → writing-plans (plan → beans → STOP)
  → capture epic → transition

DEVELOP:
  execution choice (ralph-subs | tmux team | hands-on)
  → execute → holistic review → transition

DELIVER:
  drift analysis → docs-evolve → close epic
```
