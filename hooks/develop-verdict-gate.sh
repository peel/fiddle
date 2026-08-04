#!/usr/bin/env bash
# Stop hook: while a develop-loop bean is active without a terminal verdict,
# block turn-end so the loop continues. Fail-open on any missing dependency.
#
# Ownership guard: the marker's second line records the session that armed it
# (session=<CLAUDE_CODE_SESSION_ID>). Only that session is blocked; every other
# session fails open, because a session that does not own the loop cannot
# legitimately run the evaluation chain, record an escalation, or clear the
# marker (doing so from a bystander session corrupts the owner's loop).
# A marker WITHOUT an owner line also fails open for everyone: ownership is a
# dependency for correct attribution, and this hook's contract is fail-open on
# missing dependencies. Loops armed by current develop-loop always write the
# owner line, so the gate is only ever silent for markers left by older runs.
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
