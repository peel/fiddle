#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../hooks/develop-verdict-gate.sh"
PASS=0; FAIL=0

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

arm_marker() {
  mkdir -p "$TMPDIR/.fiddle"
  printf '%s\n' "$@" > "$TMPDIR/.fiddle/active-bean"
}

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
arm_marker "fiddle-sip9"
OUT=$(hook_out "{\"session_id\":\"$OWNER_SESSION\"}"); EXIT_CODE=$?
assert_exit "ownerless marker exits 0" 0 "$EXIT_CODE"
assert_empty "empty output" "$OUT"

echo "Test 5: owner line that is not a session → allow"
arm_marker "fiddle-sip9" "owner=$OWNER_SESSION"
OUT=$(hook_out "{\"session_id\":\"$OWNER_SESSION\"}"); EXIT_CODE=$?
assert_exit "unrecognised owner line exits 0" 0 "$EXIT_CODE"
assert_empty "empty output" "$OUT"

echo "Test 6: marker owned by another session → allow"
arm_marker "fiddle-sip9" "session=$OWNER_SESSION"
OUT=$(hook_out "{\"session_id\":\"$OTHER_SESSION\"}"); EXIT_CODE=$?
assert_exit "another session exits 0" 0 "$EXIT_CODE"
assert_empty "empty output" "$OUT"

echo "Test 7: owned marker but no session on stdin → allow"
arm_marker "fiddle-sip9" "session=$OWNER_SESSION"
OUT=$(hook_out "{}"); EXIT_CODE=$?
assert_exit "sessionless payload exits 0" 0 "$EXIT_CODE"
assert_empty "empty output" "$OUT"

echo; echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
