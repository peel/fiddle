---
name: worktrees
description: Use when starting feature work that needs isolation from current workspace or before executing implementation plans - creates isolated git worktrees with smart directory selection and safety verification
---

# Using Git Worktrees

Create an isolated workspace for feature work: pick the worktree directory systematically, verify it is safe, then set it up and confirm a clean baseline.

## Directory Selection

Follow this priority order: an existing worktree directory, then a CLAUDE.md preference, then ask.

### 1. Check Existing Directories

```bash
# Check in priority order
ls -d .worktrees 2>/dev/null     # Preferred (hidden)
ls -d worktrees 2>/dev/null      # Alternative
```

If one is found, use it. If both exist, `.worktrees` wins.

### 2. Check CLAUDE.md

```bash
grep -i "worktree.*director" CLAUDE.md 2>/dev/null
```

If a preference is specified there, use it without asking.

### 3. Ask User

If no directory exists and CLAUDE.md has no preference:

```
No worktree directory found. Where should I create worktrees?

1. .worktrees/ (project-local, hidden)
2. ~/.config/fiddle/worktrees/<project-name>/ (global location)

Which would you prefer?
```

Guessing the location instead of asking creates inconsistency with the project's own conventions.

## Safety Verification

A project-local worktree directory (`.worktrees` or `worktrees`) is confirmed git-ignored before the worktree is created, because an unignored one puts every file of every branch you check out into this repository's `git status` and eventually into a commit.

```bash
# Check if directory is ignored (respects local, global, and system gitignore)
git check-ignore -q .worktrees 2>/dev/null || git check-ignore -q worktrees 2>/dev/null
```

If it is not ignored, fix it before proceeding: add the line to `.gitignore`, commit that change, then create the worktree.

The global directory (`~/.config/fiddle/worktrees`) needs no such check, being outside the project entirely.

## Creation Steps

### 1. Detect Project Name

```bash
project=$(basename "$(git rev-parse --show-toplevel)")
```

### 2. Create Worktree

```bash
# Determine full path
case $LOCATION in
  .worktrees|worktrees)
    path="$LOCATION/$BRANCH_NAME"
    ;;
  ~/.config/fiddle/worktrees/*)
    path="~/.config/fiddle/worktrees/$project/$BRANCH_NAME"
    ;;
esac

# Create worktree with new branch
git worktree add "$path" -b "$BRANCH_NAME"
cd "$path"
```

### 3. Run Project Setup

Auto-detect the toolchain from the project's own files rather than assuming a fixed command:

```bash
# Node.js
if [ -f package.json ]; then npm install; fi

# Rust
if [ -f Cargo.toml ]; then cargo build; fi

# Python
if [ -f requirements.txt ]; then pip install -r requirements.txt; fi
if [ -f pyproject.toml ]; then poetry install; fi

# Go
if [ -f go.mod ]; then go mod download; fi
```

### 4. Verify Clean Baseline

Run the project's test suite before any work starts, so a failure later can be attributed to the work rather than inherited from the branch:

```bash
# Examples - use project-appropriate command
npm test
cargo test
pytest
go test ./...
```

If tests fail, report the failures and ask whether to proceed or investigate. If they pass, report ready.

### 5. Report Location

```
Worktree ready at <full-path>
Tests passing (<N> tests, 0 failures)
Ready to implement <feature-name>
```

## Integration

**Called by:**
- **brainstorming** (Phase 4) - REQUIRED when design is approved and implementation follows
- **subagent-driven-development** - REQUIRED before executing any tasks
- **executing-plans** - REQUIRED before executing any tasks
- Any skill needing isolated workspace

**Pairs with:**
- **finishing-a-development-branch** - REQUIRED for cleanup after work complete
