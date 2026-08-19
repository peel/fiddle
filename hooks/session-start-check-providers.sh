#!/usr/bin/env bash
set -uo pipefail

CONF="${CLAUDE_PROJECT_DIR:-.}/orchestrate.json"
[[ -f "$CONF" ]] || exit 0

PROVIDERS=$(jq -r '.providers.phases // {} | to_entries[].value[]' "$CONF" 2>/dev/null | sort -u)

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
