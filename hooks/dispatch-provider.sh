#!/usr/bin/env bash
# Dispatch a prompt to an external provider CLI.
# Usage: dispatch-provider.sh <provider-name> [--check] --role <role> --topic <topic> --instructions <text>
#   [--approaches <text>] [--design-doc-file <path>] [--diff-file <path>] [--evidence-file <path>] [--previous-feedback-file <path>]
#
# --check: Validate provider is configured and CLI is on PATH, then exit 0/1.
#          Outputs JSON: {"provider":"<name>","available":true/false,"command":"..."}
#
# Reads orchestrate.json for provider command/flags, builds prompt from template,
# drops unfilled sections, pipes to provider CLI, outputs the reply to stdout.
#
# A provider may set "extract" in orchestrate.json to say how its reply is
# carried on stdout: absent (or "raw") for plain text, "codex-jsonl" for a
# `codex exec --json` event stream.
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
# How to read the reply out of this CLI's stdout. Absent means plain text.
EXTRACT=$(jq -r --arg p "$PROVIDER" '.providers[$p].extract // empty' "$CONF")

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

# Bash 5.2+ treats '&' specially in parameter-substitution replacements when
# patsub_replacement is enabled. Split around each single-use marker instead so
# provider payloads remain literal.
replace_prompt_marker() {
  local marker="$1" value="$2"
  if [[ "$PROMPT" == *"$marker"* ]]; then
    PROMPT="${PROMPT%%"$marker"*}${value}${PROMPT#*"$marker"}"
  fi
}

# Drop the sections whose value is empty, on the TEMPLATE and before any
# substitution. The earlier version stripped after substitution, scanning the
# assembled prompt: every payload is a markdown document carrying "## " headings
# of its own, and a heading followed by a blank line — ordinary markdown — looked
# exactly like an unfilled section, so the payload's headings were deleted
# instead. Pruning by marker name touches only lines the template owns.
EMPTY_MARKERS=""
add_if_empty() {
  [[ -n "$2" ]] || EMPTY_MARKERS="${EMPTY_MARKERS}${1}"$'\n'
}
add_if_empty "{PROVIDER_ROLE}"      "$ROLE"
add_if_empty "{TOPIC}"              "$TOPIC"
add_if_empty "{INSTRUCTIONS}"       "$INSTRUCTIONS"
add_if_empty "{APPROACHES}"         "$APPROACHES"
add_if_empty "{DESIGN_DOC}"         "$DESIGN_DOC"
add_if_empty "{DIFF}"               "$DIFF"
add_if_empty "{EVIDENCE}"           "$EVIDENCE"
add_if_empty "{PREVIOUS_FEEDBACK}"  "$PREVIOUS_FEEDBACK"

PROMPT=$(printf '%s' "$PROMPT" | awk -v empty="$EMPTY_MARKERS" '
  BEGIN { n = split(empty, a, "\n"); for (i = 1; i <= n; i++) if (a[i] != "") drop[a[i]] = 1 }
  # A section is "## Header" immediately followed by its single marker line.
  /^## / {
    header = $0
    if ((getline marker) <= 0) { print header; next }
    if (marker in drop) { pending_blank = 1; next }   # drop header, marker, trailing blank
    print header; print marker; next
  }
  pending_blank { pending_blank = 0; if ($0 ~ /^[[:space:]]*$/) next }
  { print }
')

replace_prompt_marker "{PROVIDER_ROLE}" "$ROLE"
replace_prompt_marker "{TOPIC}" "$TOPIC"
replace_prompt_marker "{INSTRUCTIONS}" "$INSTRUCTIONS"
replace_prompt_marker "{APPROACHES}" "$APPROACHES"
replace_prompt_marker "{DESIGN_DOC}" "$DESIGN_DOC"
replace_prompt_marker "{DIFF}" "$DIFF"
replace_prompt_marker "{EVIDENCE}" "$EVIDENCE"
replace_prompt_marker "{PREVIOUS_FEEDBACK}" "$PREVIOUS_FEEDBACK"

# --- Dispatch ---
PROMPT_FILE=$(mktemp /tmp/provider-XXXX.md)
RAW_FILE=$(mktemp /tmp/provider-raw-XXXX)
echo "$PROMPT" > "$PROMPT_FILE"
trap 'rm -f "$PROMPT_FILE" "$RAW_FILE"' EXIT

PROVIDER_EXIT=0
eval $COMMAND $FLAGS < "$PROMPT_FILE" > "$RAW_FILE" || PROVIDER_EXIT=$?

# --- Extract the reply from the provider's transport ---
# Some CLIs answer in plain text; others stream structured events and carry the
# reply escaped inside one of them. Emitting the raw stream left every caller
# hand-extracting an escaped JSON object out of JSONL.
case "$EXTRACT" in
  ""|raw)
    cat "$RAW_FILE"
    ;;
  codex-jsonl)
    # `codex exec --json` emits one JSON event per line; the answer is the text
    # of the last completed agent_message item. fromjson? skips non-JSON lines
    # rather than aborting on the CLI's occasional plain-text notices.
    REPLY=$(jq -Rrn '[inputs
      | fromjson?
      | select(.type == "item.completed")
      | .item
      | select(.type == "agent_message")
      | .text] | last // empty' < "$RAW_FILE")
    if [[ -z "$REPLY" ]]; then
      echo "dispatch-provider: no agent_message in '$PROVIDER' output (provider exit $PROVIDER_EXIT); raw stream follows" >&2
      cat "$RAW_FILE" >&2
      exit 1
    fi
    printf '%s\n' "$REPLY"
    ;;
  *)
    echo "dispatch-provider: unknown extract mode '$EXTRACT' for provider '$PROVIDER'" >&2
    exit 1
    ;;
esac

exit "$PROVIDER_EXIT"
