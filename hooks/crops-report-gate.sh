#!/usr/bin/env bash
set -euo pipefail

INPUT=$(cat)

AGENT_NAME=$(echo "$INPUT" | jq -r '.agent_name // .agent_id // ""')
echo "$AGENT_NAME" | grep -qE '^impl-' || exit 0

STOP_HOOK_ACTIVE=$(echo "$INPUT" | jq -r '.stop_hook_active // false')
[ "$STOP_HOOK_ACTIVE" = "true" ] && exit 0

BEAN_ID=$(echo "$AGENT_NAME" | sed 's/^impl-//' | sed 's/-fix[0-9]*//')
[ -z "$BEAN_ID" ] && exit 0

_d="$PWD"; BEANS_DIR=""
while [ "$_d" != "/" ]; do
  [ -f "$_d/.beans.yml" ] && BEANS_DIR="$_d/.beans" && break
  _d="$(dirname "$_d")"
done
[ -z "$BEANS_DIR" ] && exit 0

BEAN_BODY=$(beans --beans-path "$BEANS_DIR" show "$BEAN_ID" --json 2>/dev/null | jq -r '.body // ""' || true)

if echo "$BEAN_BODY" | grep -q "## Decisions"; then
  exit 0
fi

echo "Before finishing, report at least one architectural/design decision:" >&2
echo "  crops report decisions $BEAN_ID \\" >&2
echo "    --decision 'What you chose' \\" >&2
echo "    --reasoning 'Why this over alternatives'" >&2
exit 2
