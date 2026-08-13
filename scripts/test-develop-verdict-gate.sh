#!/usr/bin/env bash
# test-develop-verdict-gate.sh — hooks/develop-verdict-gate.sh, through its
# ownership guard.
#
# The guard is the contract and not a detail: the marker's second line records
# `session=<CLAUDE_CODE_SESSION_ID>`, and the hook blocks turn-end for that
# session alone. Every other case fails open, because a session that does not own
# the loop cannot legitimately run the evaluation chain or clear the marker, and a
# hook that blocked it would trap a bystander in a loop it cannot end.
#
# So there is one blocking case and five fail-open ones, and the fail-open cases
# are what this suite is mostly about — each is a distinct guard in the hook, and
# a suite that drove only the happy path is how this file went stale: it asserted
# `decision == block` for a marker with **no owner line at all**, which the hook
# had by then been correctly failing open on.
#
# Every case feeds the hook stdin, including the ones that exit before reading it.
# A case that inherited the suite's own stdin would block on a terminal and eat
# the next case's input under a pipe.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../hooks/develop-verdict-gate.sh"
PASS=0; FAIL=0

# The session that armed the loop, and one that did not.
OWNER_SESSION="11111111-2222-3333-4444-555555555555"
OTHER_SESSION="99999999-8888-7777-6666-555555555555"

assert_exit() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then PASS=$((PASS+1)); echo "  PASS: $desc"
  else FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected exit $expected, got $actual)"; fi
}
assert_json() {
  local desc="$1" field="$2" expected="$3" json="$4"
  local actual; actual=$(echo "$json" | jq -r "$field")
  if [ "$expected" = "$actual" ]; then PASS=$((PASS+1)); echo "  PASS: $desc"
  else FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"; fi
}
assert_empty() {
  local desc="$1" out="$2"
  if [ -z "$out" ]; then PASS=$((PASS+1)); echo "  PASS: $desc"
  else FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected empty output, got '$out')"; fi
}

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

# Write the marker, one argument per line, so a case can arm it with an owner
# line, with a malformed one, or with none.
arm_marker() {
  mkdir -p "$TMPDIR/.fiddle"
  printf '%s\n' "$@" > "$TMPDIR/.fiddle/active-bean"
}

# Drive the hook the way the harness does: the marker on disk, the payload on
# stdin.
hook_out() {
  CLAUDE_PROJECT_DIR="$TMPDIR" "$HOOK" <<<"$1"
}

echo "Test 1: no marker → allow (fail-open, empty output)"
OUT=$(hook_out "{}"); EXIT_CODE=$?
assert_exit "no marker exits 0" 0 "$EXIT_CODE"
assert_empty "empty output" "$OUT"

echo "Test 2: marker owned by this session → block with reason naming the bean"
arm_marker "fiddle-sip9" "session=$OWNER_SESSION"
OUT=$(hook_out "{\"session_id\":\"$OWNER_SESSION\"}"); EXIT_CODE=$?
assert_exit "owned marker exits 0" 0 "$EXIT_CODE"
assert_json "decision block" ".decision" "block" "$OUT"
assert_json "reason names bean" '.reason | test("fiddle-sip9")' "true" "$OUT"
assert_json "reason says how to end the loop" '.reason | test("active-bean")' "true" "$OUT"

echo "Test 3: empty marker file → allow (fail-open)"
: > "$TMPDIR/.fiddle/active-bean"
OUT=$(hook_out "{\"session_id\":\"$OWNER_SESSION\"}"); EXIT_CODE=$?
assert_exit "empty marker exits 0" 0 "$EXIT_CODE"
assert_empty "empty output" "$OUT"

echo "Test 4: marker with no owner line → allow, for every session"
# A marker from a run older than the ownership guard. Nobody owns it, so nobody
# is blocked by it — which is what this suite used to assert the opposite of.
arm_marker "fiddle-sip9"
OUT=$(hook_out "{\"session_id\":\"$OWNER_SESSION\"}"); EXIT_CODE=$?
assert_exit "ownerless marker exits 0" 0 "$EXIT_CODE"
assert_empty "empty output" "$OUT"

echo "Test 5: owner line that is not a session → allow"
# The guard is a prefix match, so a second line carrying something else is a
# marker this hook does not understand rather than one it may act on.
#
# **Which guard delivers this is not the guard you would name.** Deleting the
# hook's `[[ "$OWNER" == session=* ]]` line leaves this suite at 16 passed, 0
# failed — measured, not reasoned: the equality check below it compares against
# `session=$SESSION_ID`, whose left side always carries the prefix, so an owner
# line that lacks it cannot match either way. The prefix guard is therefore
# redundant *today* and worth keeping, because it stops being redundant the moment
# anyone compares bare ids. This case pins the behaviour the contract promises,
# which is what should survive that refactor, rather than the line that currently
# implements it.
arm_marker "fiddle-sip9" "owner=$OWNER_SESSION"
OUT=$(hook_out "{\"session_id\":\"$OWNER_SESSION\"}"); EXIT_CODE=$?
assert_exit "unrecognised owner line exits 0" 0 "$EXIT_CODE"
assert_empty "empty output" "$OUT"

echo "Test 6: marker owned by another session → allow"
# The case the guard exists for: a bystander session must not be dragooned into
# a loop it cannot end.
arm_marker "fiddle-sip9" "session=$OWNER_SESSION"
OUT=$(hook_out "{\"session_id\":\"$OTHER_SESSION\"}"); EXIT_CODE=$?
assert_exit "another session exits 0" 0 "$EXIT_CODE"
assert_empty "empty output" "$OUT"

echo "Test 7: owned marker but no session on stdin → allow"
# A payload without a session id cannot be attributed, and attribution is a
# dependency like any other: missing means fail open.
arm_marker "fiddle-sip9" "session=$OWNER_SESSION"
OUT=$(hook_out "{}"); EXIT_CODE=$?
assert_exit "sessionless payload exits 0" 0 "$EXIT_CODE"
assert_empty "empty output" "$OUT"

echo; echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
