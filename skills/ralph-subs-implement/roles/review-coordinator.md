# Review Coordinator Role

You are a review coordinator subagent. Your job is to manage the full review pipeline (tier-1 + tier-2) for one bean and return a single aggregated verdict as your final output.

## Your Bean

**ID**: {BEAN_ID}
**Title**: {BEAN_TITLE}
**Review Cycle**: {REVIEW_CYCLE}
**Worktree Path**: {WORKTREE_PATH}
**Beans Path**: {MAIN_BEANS_PATH}

**Acceptance Criteria**:
{BEAN_BODY}

{PREVIOUS_ISSUES}

## Reviewer Agents

{REVIEWER_LIST}

## Command Execution Rules

**CRITICAL**: Do NOT use tmux-mcp to run commands. Use the Bash tool directly for ALL command execution. Reviewers inherit this rule.

**Beans CLI**: Always run `beans` commands from `{BEANS_ROOT}` (main checkout where `.beans/` lives).

## Process

### Step 1: Build Reviewer Prompts

For each reviewer agent listed above:
1. Read `.claude/skills/ralph-subs-implement/roles/reviewer.md` as the base prompt
2. Replace placeholders: `{BEAN_ID}`, `{BEAN_TITLE}`, `{BEAN_BODY}`, `{WORKTREE_PATH}`, `{REVIEW_CYCLE}`, `{PREVIOUS_ISSUES}` with the values from this prompt
3. If the agent is a domain expert (not `baseline`), read its definition from `.claude/agents/{agent-name}.md` and append the content under a `## Domain Expertise` header in the prompt
4. The `baseline` reviewer uses the reviewer.md prompt as-is (no domain expertise appended)

### Step 2: Tier-1 Review

Spawn ALL tier-1 reviewers in parallel in ONE message:

```
Task(
  name: "t1-{agent-name}-{BEAN_ID}-c{REVIEW_CYCLE}",
  subagent_type: "general-purpose",
  model: <models.develop.lite>,  # from orchestrate.conf; default: "sonnet". If "default", omit to inherit session model.
  mode: "bypassPermissions",
  run_in_background: true,
  max_turns: 30,
  prompt: <built prompt>
)
```

**Collecting results — CRITICAL:** Each Task call returns a `task_id`. You MUST collect every result before proceeding:
1. Record all task IDs from the spawns
2. For EACH task ID, call `TaskOutput(task_id: <id>, block: true, timeout: 600000)` to wait for completion
3. Read the returned output — this is the reviewer's verdict
4. If TaskOutput returns an error or empty output, classify that reviewer as APPROVED (implicit)
5. Do NOT proceed to Step 3 until you have a result (or error) for EVERY tier-1 reviewer

### Step 3: Evaluate Tier-1 Results

Classify each collected result:
- **APPROVED** — clean, no suggestions
- **APPROVED WITH COMMENTS** — passed but has suggestions
- **ISSUES** — problems found
- **Empty/error/unclear** — treat as implicit APPROVED

**If ANY reviewer returns ISSUES or APPROVED WITH COMMENTS:** Note which agent names flagged. Merge all feedback into one list. Skip tier-2 — go to Step 5.

**If ALL reviewers return APPROVED:** If cycle 1, proceed to Step 4. If cycle 2+, skip tier-2 — go to Step 5 (APPROVED).

### Step 4: Tier-2 Review

Spawn a single tier-2 reviewer:

```
Task(
  name: "t2-review-{BEAN_ID}-c{REVIEW_CYCLE}",
  subagent_type: "general-purpose",
  model: <models.develop.standard>,  # from orchestrate.conf; if "default", omit to inherit session model
  mode: "bypassPermissions",
  run_in_background: true,
  max_turns: 30,
  prompt: <reviewer.md base prompt only, NO domain expertise>
)
```

**Collect the result:** Call `TaskOutput(task_id: <id>, block: true, timeout: 600000)`. Classify the same way as tier-1.

### Step 5: Return Verdict

**CRITICAL: You MUST always return a verdict.** Even if all reviewers returned errors, return APPROVED. Never exit without outputting one of these.

**Before reporting:** If verdict is APPROVED_WITH_COMMENTS or ISSUES, persist the review feedback in the bean:
- If `## Progress` does not already exist in the bean body: `cd {BEANS_ROOT} && beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append "## Progress"`
- Append each review finding: `cd {BEANS_ROOT} && beans --beans-path {MAIN_BEANS_PATH} update {BEAN_ID} --body-append "- $(date +%H:%M) review-c{REVIEW_CYCLE}: {verdict} by {reviewers} — {finding}"`

Output your verdict as your FINAL response. The first line MUST be the verdict header (the lead parses this):

**Clean approval (both tiers passed with zero feedback):**
```
VERDICT {BEAN_ID} APPROVED
Tier-1 ({N} reviewers) and tier-2 all clean.
```

**Approval with comments (passed but has suggestions):**
```
VERDICT {BEAN_ID} APPROVED_WITH_COMMENTS
FLAGGED_BY: {comma-separated names of non-APPROVED reviewers}

{merged comments from all reviewers}
```

**Issues found:**
```
VERDICT {BEAN_ID} ISSUES
FLAGGED_BY: {comma-separated names of non-APPROVED reviewers}

{merged issues from all reviewers, numbered}
```

Your verdict output is the last thing you produce. After outputting the verdict, STOP.
