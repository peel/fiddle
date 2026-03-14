---
# fiddle-y32a
title: 'Task 3: Update SKILL.md for MAIN_BEANS_PATH computation and spawning'
status: completed
type: task
priority: normal
tags:
    - branch
    - agent:implementer
    - reviewers:baseline
created_at: 2026-03-14T19:39:14Z
updated_at: 2026-03-14T20:00:29Z
parent: fiddle-vfl9
---

Plan: docs/plans/2026-03-14-beans-path-worktree-fix.md Task 3

Files:
- Modify: skills/ralph-beans-implement/SKILL.md

Step 1: Add MAIN_BEANS_PATH computation to Setup

In the ## Setup (first turn only) section, add a new step after step 1 (BEANS_LIST) and before step 2 (Discover agents). Insert:

2. Compute MAIN_BEANS_PATH: the absolute path to .beans/ in the main checkout. Store this value — it will be substituted into all agent prompts as {MAIN_BEANS_PATH}. Example: if main checkout is /Users/peel/wrk/board, then MAIN_BEANS_PATH=/Users/peel/wrk/board/.beans.

Renumber subsequent steps (current 2->3, 3->4, 4->5, 5->6).

Step 2: Add {MAIN_BEANS_PATH} to Implementer Spawn placeholder list

Change the placeholder list in Implementer Spawn from:
replace placeholders ({BEAN_ID}, {BEAN_TITLE}, {BEAN_BODY}, {WORKTREE_PATH})
to:
replace placeholders ({BEAN_ID}, {BEAN_TITLE}, {BEAN_BODY}, {WORKTREE_PATH}, {MAIN_BEANS_PATH})

Step 3: Add {MAIN_BEANS_PATH} to Review Coordinator Spawn placeholder list

Change the placeholder list from:
replace placeholders ({BEAN_ID}, {BEAN_TITLE}, {BEAN_BODY}, {WORKTREE_PATH}, {REVIEW_CYCLE}, {PREVIOUS_ISSUES}, {REVIEWER_LIST})
to:
replace placeholders ({BEAN_ID}, {BEAN_TITLE}, {BEAN_BODY}, {WORKTREE_PATH}, {MAIN_BEANS_PATH}, {REVIEW_CYCLE}, {PREVIOUS_ISSUES}, {REVIEWER_LIST})

Step 4: Add --beans-path rule for lead

In the ## Rules section (after the last rule), add:

- **Bean commands from worktree context:** After any cd {worktree_path}, use beans --beans-path $MAIN_BEANS_PATH for all subsequent beans commands until you return to the main checkout. Alternatively, always use --beans-path $MAIN_BEANS_PATH for safety — it is harmless when already in main.

Step 5: Verify the edited file

Read the file and confirm:
- Setup includes MAIN_BEANS_PATH computation step
- Both spawn sections list {MAIN_BEANS_PATH} in placeholders
- Rules section includes the worktree context guidance
