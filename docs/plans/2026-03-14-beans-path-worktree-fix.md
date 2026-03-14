# Beans --beans-path Worktree Fix

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Ensure all bean CLI operations from worktree agents target the main directory's `.beans/`, so status and progress updates are immediately visible to the TUI and lead.

**Architecture:** Add a `{MAIN_BEANS_PATH}` placeholder to agent role templates. The lead computes it once at startup (absolute path to main checkout's `.beans/`) and substitutes it into all spawned agent prompts. All `beans` CLI calls in worktree contexts use `beans --beans-path {MAIN_BEANS_PATH}`. Implementers are explicitly prohibited from changing bean status — only the lead does that.

**Tech Stack:** Markdown skill files, beans CLI `--beans-path` flag

---

### Task 1: Add `{MAIN_BEANS_PATH}` to implementer role

**Files:**
- Modify: `skills/ralph-beans-implement/roles/implementer.md`

**Step 1: Add `{MAIN_BEANS_PATH}` to the Workspace section**

After the existing `{WORKTREE_PATH}` line (line 23), add a new line for `{MAIN_BEANS_PATH}` and define a beans command alias. Replace the current `## Workspace` section:

```markdown
## Workspace

{WORKTREE_PATH}

**Beans path**: {MAIN_BEANS_PATH}

**If a worktree path is set above:** `cd` to that path before doing any work. All file reads, edits, builds, and tests happen inside the worktree. Commit to the worktree branch — the lead merges it back. For ALL `beans` CLI calls, use `beans --beans-path {MAIN_BEANS_PATH}` to target the main directory. This ensures progress updates and status are visible to the TUI immediately.

**If no worktree path is set:** You work in the main checkout. Use `beans` normally. Follow the git coordination protocol below to avoid conflicts with other workers.
```

**Step 2: Add status update prohibition**

After the `## Instructions` header (before step 1), add:

```markdown
**IMPORTANT: Do NOT change bean status** (e.g., `--status completed`, `--status todo`). Only the team lead manages status transitions. You may update the bean body (progress entries via `--body-append`) — always with `--beans-path {MAIN_BEANS_PATH}`.
```

**Step 3: Update all `beans update` commands in Instructions to use `--beans-path`**

There are 3 `beans update` calls in the Instructions section (lines 35-41). Change each from:
- `beans update {BEAN_ID} --body-append ...`
to:
- `beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append ...`

Specifically:
- Line 35: `beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append "## Progress"`
- Line 37: `beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append "- $(date +%H:%M) test: {what} — {why}"`
- Line 41: `beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append "- $(date +%H:%M) pass: {what} — {why}"`

Also update the refactor line (line 42):
- `beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append "- $(date +%H:%M) refactor: ..."` (if present in the text)

**Step 4: Verify the edited file**

Read the file and confirm:
- `{MAIN_BEANS_PATH}` appears in the Workspace section
- All `beans update` calls use `--beans-path {MAIN_BEANS_PATH}`
- The status prohibition is clearly stated
- No bare `beans update` calls remain

---

### Task 2: Add `{MAIN_BEANS_PATH}` to review coordinator role

**Files:**
- Modify: `skills/ralph-beans-implement/roles/review-coordinator.md`

**Step 1: Add `{MAIN_BEANS_PATH}` to the bean metadata section**

After line 10 (`**Worktree Path**: {WORKTREE_PATH}`), add:

```markdown
**Beans Path**: {MAIN_BEANS_PATH}
```

**Step 2: Update `beans update` calls in Step 5 to use `--beans-path`**

Lines 89-90 have two `beans update` calls for persisting review feedback. Change from:
- `beans update {BEAN_ID} --body-append "## Progress"`
- `beans update {BEAN_ID} --body-append "- $(date +%H:%M) review-c{REVIEW_CYCLE}: ..."`

To:
- `beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append "## Progress"`
- `beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append "- $(date +%H:%M) review-c{REVIEW_CYCLE}: ..."`

**Step 3: Verify the edited file**

Read the file and confirm:
- `{MAIN_BEANS_PATH}` appears in the metadata section
- All `beans update` calls use `--beans-path {MAIN_BEANS_PATH}`
- No bare `beans update` calls remain

---

### Task 3: Update SKILL.md for `{MAIN_BEANS_PATH}` computation and usage

**Files:**
- Modify: `skills/ralph-beans-implement/SKILL.md`

**Step 1: Add `{MAIN_BEANS_PATH}` computation to Setup**

In the `## Setup (first turn only)` section (line 29), add a new step after step 1 and before step 2. The lead computes and stores the absolute path to the main `.beans/` directory:

After line 31 (`1. \`BEANS_LIST\` — if no incomplete beans, stop`), insert:

```markdown
2. Compute `MAIN_BEANS_PATH`: the absolute path to `.beans/` in the main checkout. Store this value — it will be substituted into all agent prompts as `{MAIN_BEANS_PATH}`. Example: if main checkout is `/Users/peel/wrk/board`, then `MAIN_BEANS_PATH=/Users/peel/wrk/board/.beans`.
```

Renumber subsequent steps (current 2→3, 3→4, 4→5, 5→6).

**Step 2: Add `{MAIN_BEANS_PATH}` to Implementer Spawn placeholder list**

Line 111 currently reads:
```
1. Read `.claude/skills/ralph-beans-implement/roles/implementer.md`, replace placeholders (`{BEAN_ID}`, `{BEAN_TITLE}`, `{BEAN_BODY}`, `{WORKTREE_PATH}`)
```

Change to:
```
1. Read `.claude/skills/ralph-beans-implement/roles/implementer.md`, replace placeholders (`{BEAN_ID}`, `{BEAN_TITLE}`, `{BEAN_BODY}`, `{WORKTREE_PATH}`, `{MAIN_BEANS_PATH}`)
```

**Step 3: Add `{MAIN_BEANS_PATH}` to Review Coordinator Spawn placeholder list**

Line 133 currently reads:
```
1. Read `.claude/skills/ralph-beans-implement/roles/review-coordinator.md`, replace placeholders (`{BEAN_ID}`, `{BEAN_TITLE}`, `{BEAN_BODY}`, `{WORKTREE_PATH}`, `{REVIEW_CYCLE}`, `{PREVIOUS_ISSUES}`, `{REVIEWER_LIST}`)
```

Change to:
```
1. Read `.claude/skills/ralph-beans-implement/roles/review-coordinator.md`, replace placeholders (`{BEAN_ID}`, `{BEAN_TITLE}`, `{BEAN_BODY}`, `{WORKTREE_PATH}`, `{MAIN_BEANS_PATH}`, `{REVIEW_CYCLE}`, `{PREVIOUS_ISSUES}`, `{REVIEWER_LIST}`)
```

**Step 4: Add `--beans-path` to lead's own bean commands**

The lead runs `beans update` throughout SKILL.md (lines 45-46, 57, 77, 82-84). These can target the wrong `.beans/` if the lead's cwd is in a worktree after verification. Add a rule in the `## Rules` section (after line 157):

```markdown
- **Bean commands from worktree context:** After any `cd {worktree_path}`, use `beans --beans-path $MAIN_BEANS_PATH` for all subsequent `beans` commands until you return to the main checkout. Alternatively, always use `--beans-path $MAIN_BEANS_PATH` for safety — it's harmless when already in main.
```

**Step 5: Verify the edited file**

Read the file and confirm:
- Setup includes MAIN_BEANS_PATH computation step
- Both spawn sections list `{MAIN_BEANS_PATH}` in placeholders
- Rules section includes the worktree context guidance

---

### Task 4: Update lead-procedures.md for `--beans-path` in verification flow

**Files:**
- Modify: `skills/ralph-beans-implement/roles/lead-procedures.md`

**Step 1: Add `--beans-path` reminder to Lead Verification**

The verification procedure (line 22-34) has the lead `cd {worktree_path}` and then potentially running `beans update` commands. After the verification block (after line 34), add a reminder:

```markdown
**After verification:** If you ran `cd {worktree_path}`, your cwd is now inside the worktree. All subsequent `beans` commands MUST use `beans --beans-path $MAIN_BEANS_PATH` to target the main directory. This applies to the `beans update` calls that follow verification (e.g., `--tag role:review`, `--status completed`).
```

**Step 2: Verify the edited file**

Read the file and confirm:
- The `--beans-path` reminder appears after the verification block
- It's clear that bean commands after `cd` must target main

---
