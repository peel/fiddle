#!/usr/bin/env bash
# TaskCompleted hook: gate task completion on build/test verification.
# Receives JSON on stdin. Exits 0 to allow, 2 to reject with feedback.
#
# Fires for ralph-beans teams. Skips reviewer completions (no code changes).
# Finds the worktree from bean tags via beans CLI.
# Runs: go build, go test -short, and flutter test (if dart files changed).

set -uo pipefail

INPUT=$(cat)
TEAMMATE=$(echo "$INPUT" | jq -r '.teammate_name // ""')
TASK_SUBJECT=$(echo "$INPUT" | jq -r '.task_subject // ""')
TEAM_NAME=$(echo "$INPUT" | jq -r '.team_name // ""')
CWD=$(echo "$INPUT" | jq -r '.cwd // ""')

# Only gate ralph-beans teams
case "$TEAM_NAME" in
  ralph-beans-*) ;;
  *) exit 0 ;;
esac

# Skip reviewer completions — they don't produce code changes
case "$TEAMMATE" in
  t1-*|t2-*) exit 0 ;;
esac

# Extract bean ID from task subject (format: "{bean-id}: {bean-title}")
BEAN_ID="${TASK_SUBJECT%%:*}"
BEAN_ID="${BEAN_ID// /}"

if [[ -z "$BEAN_ID" ]]; then
  exit 0
fi

# Find worktree path from bean's worktree-slot tag
WORKTREE_SLOT=$(cd "$CWD" && beans show "$BEAN_ID" --json 2>/dev/null \
  | jq -r '(.tags // [])[] | select(startswith("worktree-slot:")) | sub("worktree-slot:"; "")' 2>/dev/null || true)

if [[ -n "$WORKTREE_SLOT" ]]; then
  WORK_DIR="$CWD/.worktrees/$WORKTREE_SLOT"
else
  WORK_DIR="$CWD"
fi

if [[ ! -d "$WORK_DIR" ]]; then
  echo "Worktree directory $WORK_DIR does not exist." >&2
  exit 2
fi

ERRORS=""

# Go verification
if [[ -d "$WORK_DIR/api" ]]; then
  BUILD_OUT=$(cd "$WORK_DIR/api" && direnv exec . go build ./... 2>&1)
  if [[ $? -ne 0 ]]; then
    ERRORS+="go build failed in $WORK_DIR/api:\n$BUILD_OUT\n\n"
  else
    TEST_OUT=$(cd "$WORK_DIR/api" && direnv exec . go test -short ./... 2>&1)
    if [[ $? -ne 0 ]]; then
      ERRORS+="go test -short failed in $WORK_DIR/api:\n$TEST_OUT\n\n"
    fi
  fi
fi

# Flutter verification (only if dart files changed)
if [[ -z "$ERRORS" && -d "$WORK_DIR/app" ]]; then
  DART_CHANGED=$(cd "$WORK_DIR" && git diff --name-only HEAD~1 2>/dev/null | grep '\.dart$' || true)
  if [[ -n "$DART_CHANGED" ]]; then
    FLUTTER_OUT=$(cd "$WORK_DIR/app" && direnv exec . flutter test 2>&1)
    if [[ $? -ne 0 ]]; then
      ERRORS+="flutter test failed in $WORK_DIR/app:\n$FLUTTER_OUT\n\n"
    fi
  fi
fi

if [[ -n "$ERRORS" ]]; then
  echo -e "$ERRORS" >&2
  echo "Build/test verification failed. Fix the errors before completing this task." >&2
  exit 2
fi

exit 0
