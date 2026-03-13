#!/usr/bin/env bash
# SessionStart hook: detect installed-but-unconfigured providers and suggest /fiddle:init.
set -euo pipefail

# Only act if orchestrate.conf exists in the project
CONF="${CLAUDE_PROJECT_DIR:-.}/orchestrate.conf"
[[ -f "$CONF" ]] || exit 0

# Extract unique provider names from orchestrate.conf
# Matches quoted strings inside brackets: ["codex", "gemini"]
PROVIDERS=$(grep -oE '"[a-z]+"' "$CONF" | tr -d '"' | sort -u)

[[ -z "$PROVIDERS" ]] && exit 0

# MCP config locations to check
PROJECT_MCP="${CLAUDE_PROJECT_DIR:-.}/.mcp.json"
GLOBAL_MCP="$HOME/.claude.json"

needs_setup=()

for provider in $PROVIDERS; do
  case "$provider" in
    codex)
      # Codex needs MCP config — check if binary exists and MCP entry is missing
      if command -v codex &>/dev/null; then
        configured=false
        for f in "$PROJECT_MCP" "$GLOBAL_MCP"; do
          if [[ -f "$f" ]] && jq -e '.mcpServers.codex // empty' "$f" &>/dev/null; then
            configured=true
            break
          fi
        done
        if [[ "$configured" == "false" ]]; then
          needs_setup+=("codex (installed, MCP not configured)")
        fi
      fi
      ;;
    gemini)
      # Gemini uses CLI, not MCP — just check if it's on PATH
      if ! command -v gemini &>/dev/null; then
        needs_setup+=("gemini (not installed)")
      fi
      ;;
  esac
done

if [[ ${#needs_setup[@]} -gt 0 ]]; then
  echo "fiddle: provider setup needed:"
  for item in "${needs_setup[@]}"; do
    echo "  - $item"
  done
  echo "Run /fiddle:init to configure."
fi

exit 0
