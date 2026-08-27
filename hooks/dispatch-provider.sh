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
DOMAIN=""
DIMENSIONS=""
DIMENSIONS_GIVEN=false

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
    --domain) DOMAIN="$2"; shift 2 ;;
    --dimensions) DIMENSIONS="$2"; DIMENSIONS_GIVEN=true; shift 2 ;;
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

WORK_DIR=$(mktemp -d "${TMPDIR:-/tmp}/provider-XXXXXX")
trap 'rm -rf "$WORK_DIR"' EXIT
PROMPT_FILE="$WORK_DIR/prompt.md"
RAW_FILE="$WORK_DIR/raw"
MESSAGE_FILE="$WORK_DIR/message"
SCHEMA_FILE="$WORK_DIR/schema.json"
echo "$PROMPT" > "$PROMPT_FILE"

PROVIDER_ARGS=""
add_provider_arg() { PROVIDER_ARGS="$PROVIDER_ARGS $(printf '%q' "$1")"; }

SCHEMA_REQUESTED=false
if [[ "$EXTRACT" == "codex-last-message" ]]; then
  add_provider_arg "-o"
  add_provider_arg "$MESSAGE_FILE"

  SCHEMA_PROFILE=$(jq -r --arg p "$PROVIDER" --arg r "$ROLE" \
    '.providers[$p].schema_roles[$r] // empty' "$CONF")

  if [[ -n "$SCHEMA_PROFILE" ]]; then
    BUILDER="$PROJECT_DIR/scripts/build-scorecard-schema.sh"
    [[ -x "$BUILDER" ]] || {
      echo "dispatch-provider: role '$ROLE' needs schema profile '$SCHEMA_PROFILE' but $BUILDER is not executable" >&2
      exit 2
    }
    BUILD_ARGS=(--profile "$SCHEMA_PROFILE")
    if [[ "$SCHEMA_PROFILE" == "evaluator" ]]; then
      [[ -n "$DOMAIN" ]] || {
        echo "dispatch-provider: role '$ROLE' on provider '$PROVIDER' carries schema profile 'evaluator', which needs --domain" >&2
        exit 2
      }
      [[ "$DIMENSIONS_GIVEN" == true ]] || {
        echo "dispatch-provider: role '$ROLE' on provider '$PROVIDER' carries schema profile 'evaluator', which needs --dimensions; pass an empty value for an evidence-only card" >&2
        exit 2
      }
      BUILD_ARGS+=(--domain "$DOMAIN" --dimensions "$DIMENSIONS")
    fi
    "$BUILDER" "${BUILD_ARGS[@]}" > "$SCHEMA_FILE" || {
      echo "dispatch-provider: could not build the '$SCHEMA_PROFILE' scorecard schema for role '$ROLE'" >&2
      exit 2
    }
    add_provider_arg "--output-schema"
    add_provider_arg "$SCHEMA_FILE"
    SCHEMA_REQUESTED=true
  fi
fi

PROVIDER_EXIT=0
eval $COMMAND $FLAGS $PROVIDER_ARGS < "$PROMPT_FILE" > "$RAW_FILE" || PROVIDER_EXIT=$?

refuse() {
  echo "dispatch-provider: $1" >&2
  shift
  cat "$@" >&2
  exit 2
}

case "$EXTRACT" in
  ""|raw)
    cat "$RAW_FILE"
    ;;
  codex-last-message)
    [[ -f "$MESSAGE_FILE" ]] || refuse \
      "provider '$PROVIDER' wrote no last-message file (provider exit $PROVIDER_EXIT); its raw output follows" \
      "$RAW_FILE"
    grep -q '[^[:space:]]' "$MESSAGE_FILE" || refuse \
      "provider '$PROVIDER' wrote an empty last message (provider exit $PROVIDER_EXIT); its raw output follows" \
      "$RAW_FILE"
    if [[ "$SCHEMA_REQUESTED" == true ]] && ! jq -e . "$MESSAGE_FILE" > /dev/null 2>&1; then
      refuse \
        "provider '$PROVIDER' answered role '$ROLE' under a schema with text that is not one JSON value (provider exit $PROVIDER_EXIT); the answer follows" \
        "$MESSAGE_FILE"
    fi
    cat "$MESSAGE_FILE"
    ;;
  *)
    echo "dispatch-provider: unknown extract mode '$EXTRACT' for provider '$PROVIDER'" >&2
    exit 1
    ;;
esac

exit "$PROVIDER_EXIT"
