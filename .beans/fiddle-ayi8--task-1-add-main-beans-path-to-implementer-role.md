---
# fiddle-ayi8
title: 'Task 1: Add {MAIN_BEANS_PATH} to implementer role'
status: completed
type: task
priority: normal
tags:
    - branch
    - agent:implementer
    - reviewers:baseline
created_at: 2026-03-14T19:38:37Z
updated_at: 2026-03-14T19:58:24Z
parent: fiddle-vfl9
---

Plan: docs/plans/2026-03-14-beans-path-worktree-fix.md Task 1

Files:
- Modify: skills/ralph-beans-implement/roles/implementer.md

Step 1: Add {MAIN_BEANS_PATH} to the Workspace section

After the existing {WORKTREE_PATH} line, add a new line for {MAIN_BEANS_PATH} and update the worktree instruction. Replace the current ## Workspace section with:

## Workspace

{WORKTREE_PATH}

**Beans path**: {MAIN_BEANS_PATH}

**If a worktree path is set above:** cd to that path before doing any work. All file reads, edits, builds, and tests happen inside the worktree. Commit to the worktree branch — the lead merges it back. For ALL beans CLI calls, use beans --beans-path {MAIN_BEANS_PATH} to target the main directory. This ensures progress updates and status are visible to the TUI immediately.

**If no worktree path is set:** You work in the main checkout. Use beans normally. Follow the git coordination protocol below to avoid conflicts with other workers.

Step 2: Add status update prohibition

After the ## Instructions header (before step 1), add:

**IMPORTANT: Do NOT change bean status** (e.g., --status completed, --status todo). Only the team lead manages status transitions. You may update the bean body (progress entries via --body-append) — always with --beans-path {MAIN_BEANS_PATH}.

Step 3: Update all beans update commands in Instructions to use --beans-path

Change each beans update call from:
- beans update {BEAN_ID} --body-append ...
to:
- beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append ...

3 occurrences: the "## Progress" append, the "test:" append, and the "pass:" append.

Step 4: Verify the edited file

Read the file and confirm:
- {MAIN_BEANS_PATH} appears in the Workspace section
- All beans update calls use --beans-path {MAIN_BEANS_PATH}
- The status prohibition is clearly stated
- No bare beans update calls remain
