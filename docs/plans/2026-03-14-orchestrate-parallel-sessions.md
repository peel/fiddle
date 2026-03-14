# Orchestrate Parallel Sessions Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Eliminate the shared event log from orchestrate, deriving all progress from beans, enabling parallel orchestrate sessions.

**Architecture:** Replace `.claude/orchestrate-events.log` with an `orchestrate-phase:<PHASE>` tag on the epic bean. The status script switches from file I/O to pure beans queries. Phase transitions update the tag instead of appending to a log file.

**Tech Stack:** Bash (status script), Markdown (SKILL.md skill definition), beans CLI

---

### Task 1: Update orchestrate-status.sh to derive phase from beans

**Files:**
- Modify: `scripts/orchestrate-status.sh:19-25` (get_phase function)
- Modify: `scripts/orchestrate-status.sh:170-174` (event log tail section)

**Step 1: Write the failing test**

No automated tests for this script. Manual verification: run the script and confirm it displays phase correctly.

**Step 2: Replace get_phase() to read from bean tag instead of event log**

Replace the `get_phase` function (lines 19-25):

```bash
get_phase() {
  local tags
  tags=$(beans show "$EPIC_ID" --json 2>/dev/null | jq -r '(.tags // [])[]' 2>/dev/null)
  local phase
  phase=$(echo "$tags" | grep '^orchestrate-phase:' | tail -1 | cut -d: -f2)
  if [[ -n "$phase" ]]; then
    echo "$phase"
  else
    echo "SETUP"
  fi
}
```

**Step 3: Remove event log tail section**

Remove the event log tail display block (lines 170-174):

```bash
  # Event log tail
  if [[ -f "$EVENT_LOG" ]]; then
    grep -v "^PHASE:" "$EVENT_LOG" | tail -n "$EVENT_TAIL_LINES" | while IFS= read -r line; do
      printf "  ${DIM}%s${RESET}\n" "$line"
    done
  fi
```

**Step 4: Remove unused variables**

Remove `EVENT_LOG` and `EVENT_TAIL_LINES` variable declarations (lines 6-7):

```bash
EVENT_LOG=".claude/orchestrate-events.log"
EVENT_TAIL_LINES=8
```

**Step 5: Verify the script parses correctly**

Run: `bash -n scripts/orchestrate-status.sh`
Expected: no output (syntax OK)

**Step 6: Commit**

```bash
git add scripts/orchestrate-status.sh
git commit -m "refactor: derive orchestrate phase from bean tag instead of event log"
```

---

### Task 2: Update SKILL.md — remove event log, add phase tags

**Files:**
- Modify: `skills/orchestrate/SKILL.md`

**Step 1: Remove SETUP Step 4 (Initialize Event Log)**

Delete the entire "Step 4: Initialize Event Log" section (lines 138-143):

```markdown
### Step 4: Initialize Event Log

\```bash
mkdir -p .claude
echo "$(date +%H:%M) orchestrate started: <topic>" >> .claude/orchestrate-events.log
\```
```

**Step 2: Replace phase log in SETUP Step 5**

Replace lines 160-163:

```markdown
Log the phase:
\```bash
echo "PHASE:<phase>" >> .claude/orchestrate-events.log
\```
```

With:

```markdown
Set the phase tag on the epic bean (if epic exists):
\```bash
beans update <epic-id> --tag orchestrate-phase:<phase>
\```
```

**Step 3: Replace DISCOVER Step 4 transition**

Replace lines 212-214:

```markdown
\```bash
echo "$(date +%H:%M) DISCOVER complete" >> .claude/orchestrate-events.log
echo "PHASE:DEFINE" >> .claude/orchestrate-events.log
\```
```

With:

```markdown
\```bash
beans update <epic-id> --remove-tag orchestrate-phase:DISCOVER --tag orchestrate-phase:DEFINE
\```

Note: if epic does not yet exist at end of DISCOVER, skip the tag update — DEFINE will set it after epic creation.
```

**Step 4: Replace DEFINE Step 4 transition**

Replace lines 252-254:

```markdown
\```bash
echo "$(date +%H:%M) DEFINE complete — $(beans list --parent <epic-id> --json | jq 'length') beans created" >> .claude/orchestrate-events.log
echo "PHASE:DEVELOP" >> .claude/orchestrate-events.log
\```
```

With:

```markdown
\```bash
beans update <epic-id> --remove-tag orchestrate-phase:DEFINE --tag orchestrate-phase:DEVELOP
\```
```

**Step 5: Replace DEVELOP Step 0 log**

Replace line 285:

```markdown
\```bash
echo "$(date +%H:%M) execution choice: <choice>" >> .claude/orchestrate-events.log
\```
```

With: Remove entirely (execution choice is ephemeral, not worth tagging).

**Step 6: Replace DEVELOP Step 1 log (ralph returned)**

Replace lines 321-322:

```markdown
Log:
\```bash
echo "$(date +%H:%M) ralph subagent returned" >> .claude/orchestrate-events.log
\```
```

With: Remove entirely (ralph result is handled inline).

**Step 7: Replace DEVELOP Step 2 Case 2 log (ralph parked)**

Replace lines 345-346:

```markdown
Log:
\```bash
echo "$(date +%H:%M) ralph parked — waiting on user for ${N} needs-attention" >> .claude/orchestrate-events.log
\```
```

With: Remove entirely.

**Step 8: Replace DEVELOP Step 4 transition**

Replace lines 383-385:

```markdown
\```bash
echo "$(date +%H:%M) DEVELOP complete" >> .claude/orchestrate-events.log
echo "PHASE:DELIVER" >> .claude/orchestrate-events.log
\```
```

With:

```markdown
\```bash
beans update <epic-id> --remove-tag orchestrate-phase:DEVELOP --tag orchestrate-phase:DELIVER
\```
```

**Step 9: Replace DELIVER Step 3 log**

Replace line 436:

```markdown
echo "$(date +%H:%M) DELIVER complete — epic closed" >> .claude/orchestrate-events.log
```

With: Remove (epic status change to `completed` is sufficient).

**Step 10: Replace CLEANUP Step 2 (Remove Event Log)**

Delete the entire "Step 2: Remove Event Log" section (lines 453-455):

```markdown
### Step 2: Remove Event Log

\```bash
rm -f .claude/orchestrate-events.log
\```
```

Also remove the `orchestrate-phase` tag from the epic in cleanup:

```markdown
### Step 2: Clean Phase Tag

\```bash
beans update <epic-id> --remove-tag orchestrate-phase:DELIVER
\```
```

**Step 11: Update CLEANUP Step 3 summary**

Remove the "Total duration" line from the summary template (no event timestamps to derive it from).

**Step 12: Verify no remaining event log references**

Run: `grep -n "orchestrate-events" skills/orchestrate/SKILL.md`
Expected: no matches

**Step 13: Commit**

```bash
git add skills/orchestrate/SKILL.md
git commit -m "refactor: replace orchestrate event log with bean phase tags"
```
