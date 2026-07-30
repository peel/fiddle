---
name: quickfix
description: Use when the user wants a quick one-shot implementation of a small, unambiguous task — skips full orchestrate in favor of clarify, implement, PR
---

# Quickfix

One-shot implementation for small, unambiguous tasks: clarify → implement → PR → done. No evaluator loop.

Invoke as `fiddle:quickfix <prompt>`.

ARGUMENTS: {ARGS}

## Scope

Quickfix handles tasks where the full DISCOVER -> DEFINE -> DEVELOP -> DELIVER cycle is overkill:
- Bug fixes with clear reproduction
- Small features with obvious implementation
- Config changes, dependency updates
- Refactors with well-defined scope
- Skill/doc edits with clear intent

## Escape Hatch

At any point, if the task proves more complex than expected:
1. Report: "This is more complex than a quickfix. Falling back to full orchestrate."
2. Clean up partial work (revert commits in worktree if any, remove worktree)
3. Scrap the bean: `beans update <bean-id> -s scrapped --body-append "## Reasons for Scrapping\n\nTask exceeded quickfix scope. Falling back to full orchestrate."`
4. Return status: TOO_COMPLEX

Take the escape hatch rather than pushing through — a quickfix that has grown complex has skipped the design and evaluation steps that a task of that size needs.

### Complexity Tripwires

Bail if you encounter any of these during implementation:
- Need to create more than 5 files
- Need to modify more than 3 existing modules with interconnected changes
- Discover an architectural decision with multiple valid approaches
- Tests require new infrastructure (test harness, fixtures, mocks) that doesn't exist
- Changes ripple into areas you didn't anticipate

## Step 1: Clarify

Read the prompt. Explore the codebase to understand what needs changing:
- Identify the files involved
- Understand the current behavior
- Understand the desired behavior

Self-serve first: use `tilth` when available, `rg` for search, and normal file reads to answer your own questions from the codebase before asking the user anything.

If anything remains genuinely ambiguous after codebase exploration, ask up to 3 focused questions in a single batch. Wait for answers.

If everything is clear, state what you'll do in one sentence and proceed.

## Step 2: Bean

Create a single task bean (no epic wrapper):

```bash
beans create "<concise task title>" -t task -s in-progress -d "<description with acceptance criteria>"
```

Store the bean ID.

## Step 3: Worktree

Use the `fiddle:worktrees` skill.

All subsequent work happens in this worktree.

## Step 4: Implement

Implement the change directly. No evaluator loop.

1. Read all relevant files
2. Make the changes
3. Write or update tests (if the change is testable code)
4. Self-review: check for obvious issues, pattern violations, missing edge cases
5. Commit with a conventional commit message. Include `Bean: <bean-id>` trailer.

Follow the project's existing patterns. Don't restructure beyond the task's scope.

## Step 5: Verify

Run the project's test suite:

```bash
# Use whatever test command the project uses
rtk cargo test / rtk npm test / rtk pytest / rtk go test ./...
```

If tests pass, proceed to Step 6.

If tests fail:
1. Fix the failures (one attempt)
2. Re-run tests
3. If still failing, trigger the escape hatch (TOO_COMPLEX)

## Step 6: PR

```bash
rtk git push -u origin <branch>

gh pr create --title "<conventional-commit-style title>" --body "$(cat <<'EOF'
## Summary
<2-3 bullets of what changed>

## Test Plan
- <verification steps>

Bean: <bean-id>
EOF
)"
```

Capture the PR URL from the output.

## Step 7: Complete

```bash
beans update <bean-id> -s completed --body-append "## Summary of Changes\n\n<what was done>\n\nPR: <pr-url>"
```

Report to user:

```
Done. PR: <pr-url>
```

Return the PR URL. Orchestrate is finished.
