# Orchestrate-Panel Integration Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Integrate panel into brainstorming as an enrichment step and make brainstorming/writing-plans orchestrate-aware via `--from-orchestrate` flag so they return control when called from the orchestrate DEFINE phase.

**Architecture:** Patch brainstorming to call panel after proposing approaches and accept `--from-orchestrate` flag for terminal state. Patch writing-plans to skip handoff when `--from-orchestrate` is set. Update orchestrate DEFINE (remove panel step, pass flag) and DEVELOP (add execution choice with config default).

**Tech Stack:** Markdown skill files

---

### Task 1: Patch brainstorming — add ARGS line and panel enrichment

**Files:**
- Modify: `skills/patch-superpowers/SKILL.md` (add brainstorming patch documentation)

**Step 1: Read current patch-superpowers**

Read `skills/patch-superpowers/SKILL.md` to confirm current state.

**Step 2: Update overview**

Replace line 10:
```
After updating the superpowers plugin, run this skill to re-apply beans integration. Patches two cached skills in-place: `writing-plans` and `executing-plans`. Brainstorming needs no changes — it already points to `writing-plans`.
```
With:
```
After updating the superpowers plugin, run this skill to re-apply beans integration. Patches three cached skills in-place: `brainstorming`, `writing-plans`, and `executing-plans`.
```

**Step 3: Update Step 1 find command**

Replace the find command grep pattern:
```bash
find "$CLAUDE_CONFIG_DIR/plugins/cache/superpowers-marketplace" -name "SKILL.md" | grep -E "(writing-plans|executing-plans)/" | sort
```
With:
```bash
find "$CLAUDE_CONFIG_DIR/plugins/cache/superpowers-marketplace" -name "SKILL.md" | grep -E "(brainstorming|writing-plans|executing-plans)/" | sort
```

**Step 4: Update Step 2 check patch state**

Replace:
```
Read both files. Check for the marker `[BEANS-PATCHED]` at the end. If both are already patched, report "Already patched" and stop.
```
With:
```
Read all three files. Check for the marker `[BEANS-PATCHED]` at the end of each. If all are already patched, report "Already patched" and stop. Patch only the files that are missing the marker.
```

**Step 5: Add new Step 3 for brainstorming patch (before current Step 3)**

Renumber current Steps 3, 4, 5 to Steps 4, 5, 6. Insert new Step 3:

````markdown
## Step 3: Patch Brainstorming

**Purpose:** Add panel enrichment after approach generation and orchestrate-aware terminal state via `--from-orchestrate` flag.

**3a.** After the frontmatter closing `---`, insert an ARGS line. Replace:

```
# Brainstorming Ideas Into Designs
```

With:

```
# Brainstorming Ideas Into Designs

ARGUMENTS: {ARGS}

## Configuration

Parse from `{ARGS}`:

| Flag | Default | Description |
|---|---|---|
| `--skip-panel` | false | Skip panel enrichment after proposing approaches |
| `--from-orchestrate` | false | Return control after design doc instead of chaining to writing-plans |
```

**3b.** In the Checklist section, replace item 3:

```
3. **Propose 2-3 approaches** — with trade-offs and your recommendation
```

With:

```
3. **Propose 2-3 approaches** — with trade-offs and your recommendation
4. **Panel enrichment** — if `--skip-panel` not set and external providers available (codex MCP tool or gemini on PATH), invoke `fiddle:panel` with the proposed approaches. Present panel commentary (consensus, disagreements, tradeoffs) alongside the approaches when asking the user to pick.
```

Renumber subsequent items (old 4→5, 5→6, 6→7).

**3c.** In the Process Flow graph, insert the panel enrichment node. Replace:

```
    "Propose 2-3 approaches" -> "Present design sections";
```

With:

```
    "Propose 2-3 approaches" -> "Panel enrichment?" [label="--skip-panel not set\nand providers available"];
    "Propose 2-3 approaches" -> "Present design sections" [label="--skip-panel set\nor no providers"];
    "Panel enrichment?" -> "Present design sections";
```

Add the node declaration:
```
    "Panel enrichment?" [shape=diamond];
```

**3d.** In the Process Flow graph, replace the terminal state. Replace:

```
    "Write design doc" -> "Invoke writing-plans skill";
```

With:

```
    "Write design doc" -> "--from-orchestrate?" [label="check flag"];
    "--from-orchestrate?" -> "STOP" [label="flag set"];
    "--from-orchestrate?" -> "Invoke writing-plans skill" [label="flag not set"];
```

Add node declarations:
```
    "--from-orchestrate?" [shape=diamond];
    "STOP" [shape=doublecircle];
```

**3e.** Replace the terminal state text after the graph. Replace:

```
**The terminal state is invoking writing-plans.** Do NOT invoke frontend-design, mcp-builder, or any other implementation skill. The ONLY skill you invoke after brainstorming is writing-plans.
```

With:

```
**Terminal state:** After writing the design doc, check if `--from-orchestrate` was set in `{ARGS}`. If set, STOP — do not invoke writing-plans. Control returns to the caller. If not set (standalone use), invoke writing-plans as the next step. Do NOT invoke frontend-design, mcp-builder, or any other implementation skill.
```

**3f.** In the "After the Design" section, replace the Implementation subsection:

```
**Implementation:**
- Invoke the writing-plans skill to create a detailed implementation plan
- Do NOT invoke any other skill. writing-plans is the next step.
```

With:

```
**Implementation:**
- If `--from-orchestrate` is set: STOP here. Do not invoke writing-plans. Control returns to the caller.
- If not set: invoke the writing-plans skill to create a detailed implementation plan. Do NOT invoke any other skill.
```

Append marker: `<!-- [BEANS-PATCHED] -->`

**Step 6: Verify**

Read the patched brainstorming file and confirm:
- Has ARGS line with `--skip-panel` and `--from-orchestrate` flags
- Checklist has panel enrichment item
- Process flow has panel and `--from-orchestrate` nodes
- Terminal state is flag-dependent
- Has `[BEANS-PATCHED]` marker
````

**Step 7: Commit**

```bash
git add skills/patch-superpowers/SKILL.md
git commit -m "feat: add brainstorming patch to patch-superpowers"
```

---

### Task 2: Patch writing-plans — add orchestrate-aware handoff to patch-superpowers

**Files:**
- Modify: `skills/patch-superpowers/SKILL.md`

**Step 1: Read current file**

Read `skills/patch-superpowers/SKILL.md` to confirm current state after Task 1.

**Step 2: Update writing-plans patch (Step 4, formerly Step 3)**

In Step 4 (Patch Writing-Plans), add a new sub-step **4c** (before current 4b which becomes 4d). Insert after the beans creation section (4a content):

````markdown
**4c.** Insert ARGS parsing and orchestrate-aware handoff. In the plan header section, after any existing ARGS handling, ensure `--from-orchestrate` is parsed from `{ARGS}`.

Insert immediately before `## Execution Handoff`:

```markdown
## Orchestrate Context Check

Before presenting the execution handoff, check if `--from-orchestrate` was set in `{ARGS}`.

If set: STOP here. Do not present execution options. Report: "Plan complete. Beans created. Returning control to orchestrate." Control returns to the caller which will handle execution in the DEVELOP phase.

If not set: proceed to Execution Handoff below.
```
````

**Step 3: Update verify step**

In Step 6 (Verify, formerly Step 5), update the writing-plans check:

Replace:
```
- Writing-plans → has "Create Beans from Plan" section with self-contained bean bodies, handoff has 4 options (including Ralph Beans)
```
With:
```
- Writing-plans → has "Create Beans from Plan" section with self-contained bean bodies, has "Orchestrate Context Check" with `--from-orchestrate` flag before handoff, handoff has 4 options (including Ralph Beans)
```

**Step 4: Commit**

```bash
git add skills/patch-superpowers/SKILL.md
git commit -m "feat: add orchestrate-aware handoff to writing-plans patch"
```

---

### Task 3: Update orchestrate DEFINE phase

**Files:**
- Modify: `skills/orchestrate/SKILL.md:191-244`

**Step 1: Read current DEFINE section**

Read `skills/orchestrate/SKILL.md` lines 191-244.

**Step 2: Replace the entire DEFINE section**

Replace from `## DEFINE` to `Fall through to DEVELOP.` with:

```markdown
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
echo "$(date +%H:%M) DEFINE complete — $(beans list --parent <epic-id> --json | jq 'length') beans created" >> .claude/orchestrate-events.log
echo "PHASE:DEVELOP" >> .claude/orchestrate-events.log
```

Fall through to DEVELOP.
```

**Step 3: Verify**

Read the file and confirm DEFINE has 4 steps, no separate panel step, both skill calls pass `--from-orchestrate`.

**Step 4: Commit**

```bash
git add skills/orchestrate/SKILL.md
git commit -m "feat: simplify orchestrate DEFINE — panel now inside brainstorming"
```

---

### Task 4: Update orchestrate DEVELOP phase — add execution choice

**Files:**
- Modify: `skills/orchestrate/SKILL.md` (DEVELOP section)

**Step 1: Read current DEVELOP section**

Read `skills/orchestrate/SKILL.md` from `## DEVELOP` onward.

**Step 2: Insert Step 0 before current Step 1**

Insert after `## DEVELOP` and before `### Step 1: Spawn Ralph Subagent`:

```markdown
### Step 0: Execution Choice

Check `orchestrate.conf` for a `develop.execution` setting. If set, use that value. If not set, present options to the user:

```
"Beans are ready. How would you like to execute?

1. **Ralph Subs (automated, this session)** — I spawn ralph as a background subagent. Automated implement/review cycles. I'll wait and handle the result.

2. **Tmux Team (automated, parallel)** — Launch ralph with parallel workers in tmux panes via conductor agent.

3. **Hands-on (manual)** — You implement the beans yourself. Tell me when you're done and I'll continue with holistic review."
```

Wait for the user's choice (or use config value).

- **If Ralph Subs:** proceed to Step 1 (Spawn Ralph Subagent) as normal.
- **If Tmux Team:** proceed to Step 1 but use `ralph-beans-implement` (team variant) instead of `ralph-subs-implement`.
- **If Hands-on:** log the choice, then wait for the user to signal completion. When they do, skip to Step 3 (Holistic Review).

Log:
```bash
echo "$(date +%H:%M) execution choice: <choice>" >> .claude/orchestrate-events.log
```
```

**Step 3: Update Configuration section**

In the Config File section, add to the HCL example:

```hcl
develop {
  execution = "ralph-subs"  // or "tmux-team" or "hands-on"
}
```

**Step 4: Verify**

Read the file and confirm DEVELOP has Step 0 with three execution options and config override, plus the config file section documents `develop.execution`.

**Step 5: Commit**

```bash
git add skills/orchestrate/SKILL.md
git commit -m "feat: add execution choice to orchestrate DEVELOP phase"
```

---

### Task 5: Apply patches to cached brainstorming skill

**Files:**
- Modify: `$CLAUDE_CONFIG_DIR/plugins/cache/superpowers-marketplace/superpowers/*/skills/brainstorming/SKILL.md`

**Step 1: Find the cached file**

```bash
find "$CLAUDE_CONFIG_DIR/plugins/cache/superpowers-marketplace" -name "SKILL.md" -path "*/brainstorming/*"
```

**Step 2: Check patch state**

Read the file. If `[BEANS-PATCHED]` marker exists, skip.

**Step 3: Apply all patches from patch-superpowers Step 3**

Apply edits 3a through 3f as documented in the updated patch-superpowers skill. Key changes:
- 3a: Add ARGS line with `--skip-panel` and `--from-orchestrate` flags
- 3b: Add panel enrichment to checklist, renumber items
- 3c: Add panel node to process flow graph
- 3d: Add `--from-orchestrate` check to graph terminal state
- 3e: Replace terminal state text with flag-dependent version
- 3f: Replace Implementation subsection with flag-dependent version
- Append `[BEANS-PATCHED]` marker

**Step 4: Verify**

Read the patched file and confirm all changes per patch-superpowers Step 3 verify checklist.

**Step 5: No commit needed**

Cached file — not version controlled.

---

### Task 6: Apply patches to cached writing-plans skill

**Files:**
- Modify: `$CLAUDE_CONFIG_DIR/plugins/cache/superpowers-marketplace/superpowers/*/skills/writing-plans/SKILL.md`

**Step 1: Find the cached file**

```bash
find "$CLAUDE_CONFIG_DIR/plugins/cache/superpowers-marketplace" -name "SKILL.md" -path "*/writing-plans/*"
```

**Step 2: Check patch state**

Read the file. Check if "Orchestrate Context Check" section already exists. If so, skip.

**Step 3: Apply orchestrate-aware handoff patch**

Insert the "Orchestrate Context Check" section before the "Execution Handoff" section, as documented in patch-superpowers Step 4c. The section checks `--from-orchestrate` in `{ARGS}` and STOPs if set.

**Step 4: Verify**

Read the patched file and confirm the orchestrate context check exists before the execution handoff and references `--from-orchestrate` flag.

**Step 5: No commit needed**

Cached file — not version controlled.

---

### Task 7: Configurable per-phase model settings

**Files:**
- Modify: `skills/orchestrate/SKILL.md` (Configuration section + DEVELOP ralph spawn)
- Modify: `skills/panel/SKILL.md` (model references in Agent spawns)
- Modify: `skills/ralph-subs-implement/SKILL.md` (implementer + review model references)
- Modify: `skills/ralph-subs-implement/roles/review-coordinator.md` (tier-1/tier-2 model references)
- Modify: `skills/ralph-beans-implement/SKILL.md` (implementer + review model references)
- Modify: `skills/ralph-beans-implement/roles/review-coordinator.md` (tier-1/tier-2 model references)

**Step 1: Update orchestrate.conf schema**

In `skills/orchestrate/SKILL.md`, add the `models {}` block to the Config File HCL example:

```hcl
models {
  # discover = "sonnet"
  # define   = "sonnet"
  # deliver  = "sonnet"

  develop {
    # standard = "sonnet"
    lite = "sonnet"
  }
}
```

Add to the Configuration section a Models Defaults table:

| Config key | Roles | Default |
|---|---|---|
| `models.discover` | All DISCOVER subagents | `"default"` (session model) |
| `models.define` | Panel advocates, brainstorming subagents | `"default"` |
| `models.develop.standard` | Implementers, tier-2 review, ralph orchestrator | `"default"` |
| `models.develop.lite` | Tier-1 review (quick pass) | `"sonnet"` |
| `models.deliver` | Drift analysis, docs review | `"default"` |

Document that `"default"` means inherit the session model, and omitted keys are treated as `"default"`.

**Step 2: Update orchestrate SETUP config parsing**

In Step 1 (Parse Configuration), add parsing of the `models {}` block. Store resolved model values for use throughout the session. Apply defaults for any omitted keys.

**Step 3: Update orchestrate ralph spawn**

In DEVELOP Step 1, replace hardcoded `model: "sonnet"` with `model: <models.develop.standard>` (resolved config value, or omit the model parameter if `"default"` to inherit).

**Step 4: Update panel skill**

In `skills/panel/SKILL.md`, replace all `model: "haiku"` in Agent spawns with a config-read instruction:

"Read `orchestrate.conf` if it exists. Use `models.define` for the model. If not set or file doesn't exist, omit the model parameter (inherit session model)."

Replace the 3 hardcoded `model: "haiku"` references.

**Step 5: Update ralph-subs-implement**

In `skills/ralph-subs-implement/SKILL.md`, replace hardcoded `model: "sonnet"` in implementer and review coordinator spawns with config-read instructions:

- Implementer spawn: use `models.develop.standard` (default: omit for session model)
- Review coordinator spawn: use `models.develop.standard` (default: omit for session model)

In `skills/ralph-subs-implement/roles/review-coordinator.md`:
- Tier-1 review spawn: replace `model: "haiku"` with `models.develop.lite` (default: `"sonnet"`)
- Tier-2 review spawn: replace `model: "sonnet"` with `models.develop.standard` (default: omit for session model)

**Step 6: Update ralph-beans-implement**

Same changes as Step 5 but in `skills/ralph-beans-implement/SKILL.md` and `skills/ralph-beans-implement/roles/review-coordinator.md`.

**Step 7: Verify**

Grep for hardcoded model references — should find zero:
```bash
grep -rn 'model: "haiku"\|model: "sonnet"' skills/
```

**Step 8: Commit**

```bash
git add skills/
git commit -m "feat: replace hardcoded model references with per-phase config"
```
