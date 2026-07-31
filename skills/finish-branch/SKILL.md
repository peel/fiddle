---
name: finish-branch
description: Use when implementation is complete, all tests pass, and you need to decide how to integrate the work - guides completion of development work by presenting structured options for merge, PR, or cleanup
---

# Finishing a Development Branch

Verify tests, present the integration options, execute the chosen one, then clean up.

## Step 1: Verify Tests

Verify the test suite passes before presenting options — the options are all ways of shipping the work, and none of them are available for work that does not pass.

```bash
# Run project's test suite
npm test / cargo test / pytest / go test ./...
```

If tests fail, report and stop here:

```
Tests failing (<N> failures). Must fix before completing:

[Show failures]

Cannot proceed with merge/PR until tests pass.
```

If tests pass, continue to Step 2.

## Step 2: Determine Base Branch

```bash
# Try common base branches
git merge-base HEAD main 2>/dev/null || git merge-base HEAD master 2>/dev/null
```

Or ask: "This branch split from main - is that correct?"

## Step 3: Present Options

Present exactly these four options, with no added explanation — an open-ended "what next?" produces an ambiguous answer:

```
Implementation complete. What would you like to do?

1. Merge back to <base-branch> locally
2. Push and create a Pull Request
3. Keep the branch as-is (I'll handle it later)
4. Discard this work

Which option?
```

## Step 4: Execute Choice

### Option 1: Merge Locally

```bash
# Switch to base branch
git checkout <base-branch>

# Pull latest
git pull

# Merge feature branch
git merge <feature-branch>

# Verify tests on merged result
<test command>

# If tests pass
git branch -d <feature-branch>
```

The merged result gets its own test run: two branches that each pass can still conflict semantically. Then clean up the worktree (Step 5).

### Option 2: Push and Create PR

```bash
# Push branch
git push -u origin <feature-branch>

# Create PR
gh pr create --title "<title>" --body "$(cat <<'EOF'
## Summary
<2-3 bullets of what changed>

## Test Plan
- [ ] <verification steps>
EOF
)"
```

Keep the worktree: review feedback usually lands back on this branch. Force-push only when the user explicitly asks for it.

### Option 3: Keep As-Is

Report: "Keeping branch <name>. Worktree preserved at <path>." Keep the worktree.

### Option 4: Discard

Discarding is irreversible, so it takes a typed confirmation naming what will be lost — an accidental "yes" cannot recover the commits:

```
This will permanently delete:
- Branch <name>
- All commits: <commit-list>
- Worktree at <path>

Type 'discard' to confirm.
```

Wait for that exact word. Anything else is not a confirmation. Once confirmed:

```bash
git checkout <base-branch>
git branch -D <feature-branch>
```

Then clean up the worktree (Step 5).

## Step 5: Cleanup Worktree

For Options 1 and 4, check whether you are in a worktree and remove it:

```bash
git worktree list | grep $(git branch --show-current)
```

```bash
git worktree remove <worktree-path>
```

For Options 2 and 3, keep the worktree.

| Option | Merge | Push | Keep Worktree | Cleanup Branch |
|--------|-------|------|---------------|----------------|
| 1. Merge locally | ✓ | - | - | ✓ |
| 2. Create PR | - | ✓ | ✓ | - |
| 3. Keep as-is | - | - | ✓ | - |
| 4. Discard | - | - | - | ✓ (force) |

## Integration

**Called by:**
- **develop** - After all tasks complete and holistic review has run

**Pairs with:**
- **worktrees** - Cleans up worktree created by that skill
