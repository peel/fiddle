---
name: fiddle:ralph-subs-implement
description: Execute beans tasks using subagents with ralph loop pattern. Implementers and review coordinators are background subagents; coordinators encapsulate the full tier-1/tier-2 review pipeline and return a verdict. Supports configurable parallelism.
disable-model-invocation: true
argument-hint: [--epic <id>] [--workers 2] [--max-review-cycles 3] [--max-impl-turns 50] [--max-review-turns 30] [--ci-max-retries 3] [--stall-timeout-min 15] [--stall-max-respawns 2] [--caller <name>]
---

# Ralph Beans Implementation (Subagent Variant)

Stateless orchestrator. All state lives in `beans` CLI. The lead derives what to do from `beans list` each turn.

## Configuration

Flags (all optional, order-independent):
- `--epic <id>` — scope to beans under this epic (uses `beans list --parent <id>`)
- `--workers N` (default: 2) — parallel beans in-flight
- `--max-review-cycles N` (default: 3) — max cycles before abandoning
- `--max-impl-turns N` (default: 50) — max agent turns per implementer spawn
- `--max-review-turns N` (default: 30) — max agent turns per review coordinator
- `--ci-max-retries N` (default: 3) — CI failures before tagging `needs-attention`
- `--stall-timeout-min N` (default: 15) — minutes of inactivity before respawn/escalation
- `--stall-max-respawns N` (default: 2) — stall respawns before tagging `needs-attention`
- `--caller <name>` (default: none) — when set, output machine-readable `RALPH_STATUS` on exit

**`BEANS_LIST` command:** When `--epic <id>` is set, every `beans list` invocation MUST include `--parent <id>`.
- With `--epic cv0e`: `beans list --parent cv0e --json`
- Without `--epic`: `beans list --json`

**Reading bean data:** Use `beans show <id> --json` via Bash. Do NOT pipe through `python3` or suppress stderr.

**STOP rule:** After processing any event, STOP — end your turn. Do not narrate or summarize. Exceptions noted inline.

## Setup (first turn only)

1. `BEANS_LIST` — if no incomplete beans, stop
2. Discover available agents: list `.claude/agents/*.md`, `~/.claude/agents/*.md`, and `.claude/skills/ralph-subs-implement/roles/*.md`. Read each file's opening lines to understand capabilities.
3. **Worktree setup** (when `--workers > 1`): Read `roles/lead-procedures.md` → follow "Worktree Setup".
4. Fall through to "Assess and Act"

## Every Turn: Assess and Act

Every bean goes through: **implement** → **review** (coordinator handles tier-1 + tier-2 internally).

Run `BEANS_LIST`. Then:

**Pre-step — Reaction checks:** For each `in-progress` leaf bean, run `beans show {id} --json` and check:

1. **CI failure escalation:** If the bean has a `ci-retries:N` tag and `N >= ci_max_retries`:
   ```bash
   beans update {id} --tag needs-attention
   echo "$(date +%H:%M) impl-{id} failed ${N}x → needs attention" >> .claude/orchestrate-events.log
   ```

2. **Stall detection:** Parse the last timestamp from `## Progress` entries (format: `- HH:MM ...`). If the last entry is older than `stall_timeout_min` minutes:
   - Check the `stall-respawns:N` tag:
     - If `N < stall_max_respawns` (or tag doesn't exist):
       ```bash
       beans update {id} --tag stall-respawns:$((N+1))
       echo "$(date +%H:%M) impl-{id} stalled → respawned ($((N+1))/${stall_max_respawns})" >> .claude/orchestrate-events.log
       ```
       Ralph's next cycle will spawn a fresh implementer that reads `## Progress` and continues.
     - If `N >= stall_max_respawns`:
       ```bash
       beans update {id} --tag needs-attention
       echo "$(date +%H:%M) impl-{id} stalled ${N}x → needs attention" >> .claude/orchestrate-events.log
       ```

3. **Review overflow:** If a bean's review cycle count (from tags) reaches `max_review_cycles`:
   ```bash
   beans update {id} --tag needs-attention
   echo "$(date +%H:%M) review-{id} overflow → needs attention" >> .claude/orchestrate-events.log
   ```

**Step 0 — Expand features:** For each **feature** bean in the list:
- If `todo` with no blockers → `beans update {id} --status in-progress`
- If `in-progress` → list children: `beans list --parent {id} --json`. Add children to work queue.
- If `in-progress` AND all children `completed` → `beans update {id} --status completed`

After expansion, work only with **leaf beans** (task, bug):

1. Count `in-progress` leaf beans = active_count
2. Count `todo` leaf beans with no blockers = ready_beans
3. available_slots = workers - active_count

**If ready_beans > 0 AND available_slots > 0:**
For each ready leaf bean (up to available_slots):
- `beans update {id} --status in-progress`
- Select agents for this bean (auto-select based on bean content)
- Tag the bean: `--tag agent:{impl-name} --tag reviewers:{r1}+{r2}`
- **Worktree assignment:** Check bean tags:
  - `worktree` tag (or no tag when `--workers > 1`): assign available worktree slot. Tag bean: `--tag worktree-slot:{prefix}-{N}`
  - `branch` tag: skip worktree, main checkout. Only ONE `branch`-tagged bean in-progress at a time.
- Spawn implementer (see "Implementer Spawn"). Launch all spawns in ONE message. STOP.

**If active_count > 0 AND ready_beans == 0:** STOP.

**If active_count == 0 AND ready_beans == 0:**

Check for `needs-attention` beans: `beans list --parent <epic-id> --json` and filter for `needs-attention` tag.

- **If needs-attention beans exist:** Output the following and STOP:
  ```
  RALPH_STATUS: PARKED
  Needs-attention beans:
  - <bean-id>: <title> — <reason from tags/event log>
  - ...
  ```

- **If all beans completed (no needs-attention):**
  Read `roles/lead-procedures.md` → run "Epic Holistic Review". If none → follow "Cleanup".
  After epic holistic review completes, output completion message:
  - If `--caller orchestrate` was set:
    ```
    RALPH_STATUS: COMPLETE
    Completed: <N> beans, Needs-attention: <M> beans
    ```
  - If no `--caller` flag: remind the user: "Epic complete. Run `/fiddle:docs-evolve --epic <id>` to update project docs."

## When a Background Task Completes

You are notified when background tasks finish. The notification includes the task name and ID. For each:

1. Identify the bean from the task name (`impl-{bean-slug}` or `review-{bean-slug}-c{cycle}`)
2. Read the result: `TaskOutput(task_id: <id>, block: false, timeout: 5000)`
3. Confirm via `BEANS_LIST` — check bean's role tag: `role:implement` or `role:review`

**Implementer result (non-empty):**
- Present the diff from the result to the user.
- Read `roles/lead-procedures.md` → run "Lead Verification". If fails → spawn fix implementer, skip review. STOP.
- `beans update {id} --remove-tag role:implement --remove-tag bg-task:{old_task_id} --remove-tag ci-retries --remove-tag stall-respawns --tag role:review`
- Spawn review coordinator (see "Review Coordinator Spawn"). STOP.

**Implementer result (empty/error):** Stale — read `roles/lead-procedures.md` → "Abandon Bean". STOP.

**Review coordinator result:**
The result starts with `VERDICT {bean-id} {TYPE}`. Parse the bean ID and verdict type from the first line.
- **`VERDICT {id} APPROVED`** → `beans update {id} --status completed`. Check parent feature: if parent exists and all siblings completed → complete feature. "Assess and Act". STOP.
- **`VERDICT {id} APPROVED_WITH_COMMENTS`** → present the comments to user. Parse `FLAGGED_BY:` line (second line). `beans update {id} --remove-tag role:review --tag role:review-fix-{cycle} --tag flagged-by:{reviewers}`. Spawn NEW implementer with suggestions (using `superpowers:receiving-code-review`). STOP.
- **`VERDICT {id} ISSUES`** → present the issues to user. Parse `FLAGGED_BY:` line (second line). If cycle >= max_review_cycles → read `roles/lead-procedures.md` → "Abandon Bean". STOP. Otherwise: `beans update {id} --remove-tag role:review --tag role:review-fix-{cycle} --tag flagged-by:{reviewers}`, spawn fix implementer. STOP.
- **Unparseable result** → log warning, find bean via task name. If cycle >= max_review_cycles → read `roles/lead-procedures.md` → "Abandon Bean". Otherwise: increment cycle, respawn review coordinator for that bean. STOP.

**User interrupts (esc) or says to proceed:** "Assess and Act" immediately. Re-read tags. For `in-progress` beans with `bg-task:*` tag, check task status via `TaskOutput(block: false)`. If completed, process result. If no active task, respawn.

## Spawning Agents

All agents are subagents (no `team_name`). The coordinator internally spawns reviewer sub-tasks — the lead never spawns reviewers directly.

### Implementer Spawn

1. Read `.claude/skills/ralph-subs-implement/roles/implementer.md`, replace placeholders (`{BEAN_ID}`, `{BEAN_TITLE}`, `{BEAN_BODY}`, `{WORKTREE_PATH}`)
2. If worktree assigned: omit the `## Git Coordination` section (between `<!-- CONDITIONAL -->` markers)
3. For fix cycles, append issues under `## Review Issues to Address`
4. Spawn and tag bean:
```
task = Task(
  name: "impl-{bean-slug}[-fix{cycle}]",
  subagent_type: "general-purpose",
  model: "sonnet",
  mode: "bypassPermissions",
  run_in_background: true,
  max_turns: <max-impl-turns>,
  prompt: ...
)
beans update {id} --tag role:implement --tag bg-task:{task_id}
```

### Review Coordinator Spawn

**Cycle 1:** Auto-select ALL domain agents relevant to the bean. Always include `baseline`.
**Cycle 2+:** Use only the reviewers from the bean's `flagged-by:*` tag (set by previous verdict).

1. Read `.claude/skills/ralph-subs-implement/roles/review-coordinator.md`, replace placeholders (`{BEAN_ID}`, `{BEAN_TITLE}`, `{BEAN_BODY}`, `{WORKTREE_PATH}`, `{REVIEW_CYCLE}`, `{PREVIOUS_ISSUES}`, `{REVIEWER_LIST}`)
2. Spawn and tag bean:
```
task = Task(
  name: "review-{bean-slug}-c{cycle}",
  subagent_type: "general-purpose",
  model: "sonnet",
  mode: "bypassPermissions",
  run_in_background: true,
  max_turns: <max-review-turns>,
  prompt: ...
)
beans update {id} --remove-tag bg-task:{old_task_id} --tag bg-task:{task_id}
```

## Rules

- Always `run_in_background: true` — never block on Task calls
- No `team_name` — all agents are subagents returning results directly
- Models: implementers=sonnet, coordinators=sonnet (tier-1 reviewers=haiku, tier-2=sonnet internally), epic holistic review=opus
- Fresh context per cycle — never resume agents
- Never implement beans yourself — delegate only
- Safe to kill and restart — beans CLI holds all state
