#!/usr/bin/env bash
# Dispatch a prompt to an external provider CLI.
# Usage: dispatch-provider.sh <provider-name> [--check] --role <role> --topic <topic> --instructions <text>
#   [--approaches <text>] [--design-doc-file <path>] [--diff-file <path>] [--evidence-file <path>] [--previous-feedback-file <path>]
#
# --check: Validate provider is configured and CLI is on PATH, then exit 0/1.
#          Outputs JSON: {"provider":"<name>","available":true/false,"command":"..."}
#
# Reads orchestrate.json for provider command/flags, builds prompt from template,
# strips empty sections, pipes to provider CLI, outputs result to stdout.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CONF="$PROJECT_DIR/orchestrate.json"
TEMPLATE="$PROJECT_DIR/skills/develop/provider-context.md"

# --- Parse args ---
PROVIDER=""
ROLE=""
TOPIC=""
INSTRUCTIONS=""
APPROACHES=""
DESIGN_DOC=""
DIFF=""
EVIDENCE=""
PREVIOUS_FEEDBACK=""

PROVIDER="${1:?Usage: dispatch-provider.sh <provider> [--check] --role ... --topic ... --instructions ...}"
shift

CHECK_ONLY=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --check) CHECK_ONLY=true; shift ;;
    --role) ROLE="$2"; shift 2 ;;
    --topic) TOPIC="$2"; shift 2 ;;
    --instructions) INSTRUCTIONS="$2"; shift 2 ;;
    --approaches) APPROACHES="$2"; shift 2 ;;
    --design-doc-file) DESIGN_DOC="$(cat "$2")"; shift 2 ;;
    --diff-file) DIFF="$(cat "$2")"; shift 2 ;;
    --evidence-file) EVIDENCE="$(cat "$2")"; shift 2 ;;
    --previous-feedback-file) PREVIOUS_FEEDBACK="$(cat "$2")"; shift 2 ;;
    *) echo "Unknown flag: $1" >&2; exit 1 ;;
  esac
done

# --- Read config ---
[[ -f "$CONF" ]] || { echo "Config not found: $CONF" >&2; exit 1; }

COMMAND=$(jq -r --arg p "$PROVIDER" '.providers[$p].command // empty' "$CONF")
FLAGS=$(jq -r --arg p "$PROVIDER" '.providers[$p].flags // empty' "$CONF")

# --- Check mode: validate and exit ---
if [[ "$CHECK_ONLY" == true ]]; then
  CLI_BIN="${COMMAND%% *}"  # first word of command, e.g. "codex" from "codex exec"
  if [[ -n "$COMMAND" ]] && command -v "$CLI_BIN" &>/dev/null; then
    echo "{\"provider\":\"$PROVIDER\",\"available\":true,\"command\":\"$COMMAND\"}"
    exit 0
  else
    echo "{\"provider\":\"$PROVIDER\",\"available\":false,\"command\":\"$COMMAND\"}"
    exit 1
  fi
fi

[[ -z "$ROLE" ]] && { echo "--role is required" >&2; exit 1; }
[[ -z "$TOPIC" ]] && { echo "--topic is required" >&2; exit 1; }
[[ -z "$INSTRUCTIONS" ]] && { echo "--instructions is required" >&2; exit 1; }

[[ -z "$COMMAND" ]] && { echo "No config for provider '$PROVIDER' in $CONF" >&2; exit 1; }

# --- Build prompt from template ---
[[ -f "$TEMPLATE" ]] || { echo "Template not found: $TEMPLATE" >&2; exit 1; }

PROMPT=$(cat "$TEMPLATE")
PROMPT="${PROMPT//\{PROVIDER_ROLE\}/$ROLE}"
PROMPT="${PROMPT//\{TOPIC\}/$TOPIC}"
PROMPT="${PROMPT//\{INSTRUCTIONS\}/$INSTRUCTIONS}"
PROMPT="${PROMPT//\{APPROACHES\}/$APPROACHES}"
PROMPT="${PROMPT//\{DESIGN_DOC\}/$DESIGN_DOC}"
PROMPT="${PROMPT//\{DIFF\}/$DIFF}"
PROMPT="${PROMPT//\{EVIDENCE\}/$EVIDENCE}"
PROMPT="${PROMPT//\{PREVIOUS_FEEDBACK\}/$PREVIOUS_FEEDBACK}"

# Strip sections where the value is empty (header + empty line)
PROMPT=$(echo "$PROMPT" | awk '
  /^## / { header=$0; value=""; next_is_value=1; next }
  next_is_value { value=$0; next_is_value=0;
    if (value != "" && value !~ /^[[:space:]]*$/) { print header; print value }
    next
  }
  { print }
')

# --- Dispatch ---
PROMPT_FILE=$(mktemp /tmp/provider-XXXX.md)
echo "$PROMPT" > "$PROMPT_FILE"
trap 'rm -f "$PROMPT_FILE"' EXIT

eval $COMMAND $FLAGS < "$PROMPT_FILE"
