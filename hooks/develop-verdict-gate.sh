#!/usr/bin/env bash
set -uo pipefail

MARKER="${CLAUDE_PROJECT_DIR:-.}/.fiddle/active-bean"
[[ -f "$MARKER" ]] || exit 0
BEAN_ID="$(sed -n 1p "$MARKER" 2>/dev/null | tr -d '[:space:]')"
[[ -n "$BEAN_ID" ]] || exit 0
command -v jq &>/dev/null || exit 0

OWNER="$(sed -n 2p "$MARKER" 2>/dev/null | tr -d '[:space:]')"
[[ "$OWNER" == session=* ]] || exit 0
HOOK_INPUT="$(cat 2>/dev/null || true)"
SESSION_ID="$(jq -r '.session_id // empty' <<<"$HOOK_INPUT" 2>/dev/null)"
[[ -n "$SESSION_ID" ]] || exit 0
[[ "session=$SESSION_ID" == "$OWNER" ]] || exit 0

jq -n --arg reason \
  "develop-loop bean $BEAN_ID has no terminal verdict. Continue the loop: run the evaluation chain to CONVERGED, or record needs-attention escalation, then clear .fiddle/active-bean." \
  '{"decision":"block","reason":$reason}'
exit 0
