#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
HOOK="$SCRIPT_DIR/../hooks/develop-verdict-gate.sh"
PASS=0; FAIL=0

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

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

echo "Test 1: no marker → allow (fail-open, empty output)"
OUT=$(CLAUDE_PROJECT_DIR="$TMPDIR" "$HOOK"); EXIT_CODE=$?
assert_exit "no marker exits 0" 0 "$EXIT_CODE"
[ -z "$OUT" ] && { PASS=$((PASS+1)); echo "  PASS: empty output"; } || { FAIL=$((FAIL+1)); echo "  FAIL: expected empty output"; }

echo "Test 2: active marker → block with reason naming the bean"
mkdir -p "$TMPDIR/.fiddle"
echo "fiddle-sip9" > "$TMPDIR/.fiddle/active-bean"
OUT=$(CLAUDE_PROJECT_DIR="$TMPDIR" "$HOOK"); EXIT_CODE=$?
assert_exit "marker exits 0" 0 "$EXIT_CODE"
assert_json "decision block" ".decision" "block" "$OUT"
assert_json "reason names bean" '.reason | test("fiddle-sip9")' "true" "$OUT"

echo "Test 3: empty marker file → allow (fail-open)"
: > "$TMPDIR/.fiddle/active-bean"
OUT=$(CLAUDE_PROJECT_DIR="$TMPDIR" "$HOOK"); EXIT_CODE=$?
assert_exit "empty marker exits 0" 0 "$EXIT_CODE"
[ -z "$OUT" ] && { PASS=$((PASS+1)); echo "  PASS: empty output"; } || { FAIL=$((FAIL+1)); echo "  FAIL: expected empty output"; }

echo; echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
