#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CONF="$PROJECT_DIR/orchestrate.json"
TEMPLATE="$PROJECT_DIR/skills/develop/provider-context.md"

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

[[ -f "$CONF" ]] || { echo "Config not found: $CONF" >&2; exit 1; }

COMMAND=$(jq -r --arg p "$PROVIDER" '.providers[$p].command // empty' "$CONF")
FLAGS=$(jq -r --arg p "$PROVIDER" '.providers[$p].flags // empty' "$CONF")
EXTRACT=$(jq -r --arg p "$PROVIDER" '.providers[$p].extract // empty' "$CONF")

if [[ "$CHECK_ONLY" == true ]]; then
  CLI_BIN="${COMMAND%% *}"
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

[[ -f "$TEMPLATE" ]] || { echo "Template not found: $TEMPLATE" >&2; exit 1; }

PROMPT=$(cat "$TEMPLATE")

replace_prompt_marker() {
  local marker="$1" value="$2"
  if [[ "$PROMPT" == *"$marker"* ]]; then
    PROMPT="${PROMPT%%"$marker"*}${value}${PROMPT#*"$marker"}"
  fi
}

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

PROMPT_FILE=$(mktemp /tmp/provider-XXXX.md)
RAW_FILE=$(mktemp /tmp/provider-raw-XXXX)
echo "$PROMPT" > "$PROMPT_FILE"
trap 'rm -f "$PROMPT_FILE" "$RAW_FILE"' EXIT

PROVIDER_EXIT=0
eval $COMMAND $FLAGS < "$PROMPT_FILE" > "$RAW_FILE" || PROVIDER_EXIT=$?

case "$EXTRACT" in
  ""|raw)
    cat "$RAW_FILE"
    ;;
  codex-jsonl)
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
