---
# fiddle-bqb7
title: 'Task 2: Add {MAIN_BEANS_PATH} to review coordinator role'
status: completed
type: task
priority: normal
tags:
    - reviewers:baseline
    - branch
    - agent:implementer
created_at: 2026-03-14T19:38:52Z
updated_at: 2026-03-14T19:59:10Z
parent: fiddle-vfl9
---

Plan: docs/plans/2026-03-14-beans-path-worktree-fix.md Task 2

Files:
- Modify: skills/ralph-beans-implement/roles/review-coordinator.md

Step 1: Add {MAIN_BEANS_PATH} to the bean metadata section

After the **Worktree Path**: {WORKTREE_PATH} line, add:

**Beans Path**: {MAIN_BEANS_PATH}

Step 2: Update beans update calls in Step 5 to use --beans-path

Lines 89-90 have two beans update calls for persisting review feedback. Change from:
- beans update {BEAN_ID} --body-append "## Progress"
- beans update {BEAN_ID} --body-append "- $(date +%H:%M) review-c{REVIEW_CYCLE}: ..."

To:
- beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append "## Progress"
- beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append "- $(date +%H:%M) review-c{REVIEW_CYCLE}: ..."

Step 3: Verify the edited file

Read the file and confirm:
- {MAIN_BEANS_PATH} appears in the metadata section
- All beans update calls use --beans-path {MAIN_BEANS_PATH}
- No bare beans update calls remain
