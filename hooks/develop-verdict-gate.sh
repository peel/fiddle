#!/usr/bin/env bash
# Stop hook: while a develop-loop bean is active without a terminal verdict,
# block turn-end so the loop continues. Fail-open on any missing dependency.
set -uo pipefail

MARKER="${CLAUDE_PROJECT_DIR:-.}/.fiddle/active-bean"
[[ -f "$MARKER" ]] || exit 0
BEAN_ID="$(cat "$MARKER" 2>/dev/null | head -1 | tr -d '[:space:]')"
[[ -n "$BEAN_ID" ]] || exit 0
command -v jq &>/dev/null || exit 0

jq -n --arg reason \
  "develop-loop bean $BEAN_ID has no terminal verdict. Continue the loop: run the evaluation chain to CONVERGED, or record needs-attention escalation, then clear .fiddle/active-bean." \
  '{"decision":"block","reason":$reason}'
exit 0
