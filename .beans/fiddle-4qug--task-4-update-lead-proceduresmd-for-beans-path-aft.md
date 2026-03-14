---
# fiddle-4qug
title: 'Task 4: Update lead-procedures.md for --beans-path after verification'
status: completed
type: task
priority: normal
tags:
    - branch
    - agent:implementer
    - reviewers:baseline
created_at: 2026-03-14T19:39:14Z
updated_at: 2026-03-14T20:01:05Z
parent: fiddle-vfl9
---

Plan: docs/plans/2026-03-14-beans-path-worktree-fix.md Task 4

Files:
- Modify: skills/ralph-beans-implement/roles/lead-procedures.md

Step 1: Add --beans-path reminder to Lead Verification

The verification procedure has the lead cd to the worktree path and then potentially running beans update commands. After the verification block (after "Otherwise: spawn fix implementer teammate..."), add:

**After verification:** If you ran cd {worktree_path}, your cwd is now inside the worktree. All subsequent beans commands MUST use beans --beans-path $MAIN_BEANS_PATH to target the main directory. This applies to the beans update calls that follow verification (e.g., --tag role:review, --status completed).

Step 2: Verify the edited file

Read the file and confirm:
- The --beans-path reminder appears after the verification block
- It is clear that bean commands after cd must target main
