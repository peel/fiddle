#!/usr/bin/env bash

set -uo pipefail

INPUT=$(cat)
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // ""')

case "$TOOL_NAME" in
  Edit|Write) ;;
  *) exit 0 ;;
esac

FILE_PATH=$(echo "$INPUT" | jq -r '.tool_input.file_path // ""')
if [[ -z "$FILE_PATH" ]]; then
  exit 0
fi

if ! command -v clash &>/dev/null; then
  exit 0
fi

WORKTREE_COUNT=$(git worktree list 2>/dev/null | wc -l | tr -d ' ')
if [[ "$WORKTREE_COUNT" -lt 2 ]]; then
  exit 0
fi

CLASH_OUTPUT=$(clash check "$FILE_PATH" 2>/dev/null)
CLASH_EXIT=$?

if [[ "$CLASH_EXIT" -eq 2 ]]; then
  echo "CLASH: '$FILE_PATH' conflicts with another worktree."
  echo "$CLASH_OUTPUT"
  echo "Consider: coordinate with the other worktree, or scope changes to avoid overlap."
fi

exit 0
