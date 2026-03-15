#!/usr/bin/env bash
# SessionStart hook: detect missing provider CLIs and nudge the user.
set -uo pipefail

# Only act if orchestrate.conf exists in the project
CONF="${CLAUDE_PROJECT_DIR:-.}/orchestrate.conf"
[[ -f "$CONF" ]] || exit 0

# Extract unique provider names from orchestrate.conf
# Matches quoted strings inside brackets: ["codex", "gemini"]
PROVIDERS=$(grep -oE '"[a-z]+"' "$CONF" | tr -d '"' | sort -u)

[[ -z "$PROVIDERS" ]] && exit 0

needs_setup=()

for provider in $PROVIDERS; do
  if ! command -v "$provider" &>/dev/null; then
    needs_setup+=("$provider (not installed)")
  fi
done

if [[ ${#needs_setup[@]} -gt 0 ]]; then
  echo "fiddle: provider setup needed:"
  for item in "${needs_setup[@]}"; do
    echo "  - $item"
  done
  echo "Install missing providers to enable multi-model features."
fi

exit 0
