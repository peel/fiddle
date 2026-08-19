#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0; FAIL=0

assert_exit() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected exit $expected, got $actual)"
  fi
}

assert_json() {
  local desc="$1" field="$2" expected="$3" json="$4"
  local actual
  actual=$(echo "$json" | jq -r "$field")
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"
  fi
}

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

git init "$TMPDIR/repo" >/dev/null 2>&1
pushd "$TMPDIR/repo" >/dev/null

git config user.email "test@test.com"
git config user.name "Test"

echo "initial" > file.txt
git add file.txt
git commit -m "initial commit" >/dev/null 2>&1

FIRST_SHA=$(git rev-parse HEAD)

echo "second" > file2.txt
git add file2.txt
git commit -m "second commit" >/dev/null 2>&1

SECOND_SHA=$(git rev-parse HEAD)

popd >/dev/null

echo "Test 1: Clean state (HEAD is at base)"
EXIT_CODE=0
OUTPUT=$(cd "$TMPDIR/repo" && "$SCRIPT_DIR/assess-git-state.sh" --base-sha "$SECOND_SHA") || EXIT_CODE=$?
assert_exit "clean state → exit 0" 0 "$EXIT_CODE"
assert_json "clean state" ".state" "CLEAN" "$OUTPUT"
assert_json "commits ahead is 0" ".commits_ahead" "0" "$OUTPUT"

echo "Test 2: Clean state (HEAD ahead of base)"
EXIT_CODE=0
OUTPUT=$(cd "$TMPDIR/repo" && "$SCRIPT_DIR/assess-git-state.sh" --base-sha "$FIRST_SHA") || EXIT_CODE=$?
assert_exit "clean ahead → exit 0" 0 "$EXIT_CODE"
assert_json "clean ahead state" ".state" "CLEAN" "$OUTPUT"
assert_json "commits ahead is 1" ".commits_ahead" "1" "$OUTPUT"
assert_json "head sha matches" ".head_sha" "$SECOND_SHA" "$OUTPUT"

echo "Test 3: Dirty state (uncommitted changes)"
echo "dirty" > "$TMPDIR/repo/dirty.txt"
EXIT_CODE=0
OUTPUT=$(cd "$TMPDIR/repo" && "$SCRIPT_DIR/assess-git-state.sh" --base-sha "$SECOND_SHA") || EXIT_CODE=$?
assert_exit "dirty → exit 1" 1 "$EXIT_CODE"
assert_json "dirty state" ".state" "DIRTY" "$OUTPUT"
rm "$TMPDIR/repo/dirty.txt"

echo "Test 4: Missing --base-sha"
EXIT_CODE=0
OUTPUT=$(cd "$TMPDIR/repo" && "$SCRIPT_DIR/assess-git-state.sh" 2>/dev/null) || EXIT_CODE=$?
assert_exit "missing arg → exit 2" 2 "$EXIT_CODE"
assert_json "error message" ".error" "missing --base-sha" "$OUTPUT"

echo "Test 5: Invalid base SHA → CORRUPTED"
EXIT_CODE=0
OUTPUT=$(cd "$TMPDIR/repo" && "$SCRIPT_DIR/assess-git-state.sh" --base-sha "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef") || EXIT_CODE=$?
assert_exit "invalid SHA → exit 2" 2 "$EXIT_CODE"
assert_json "corrupted state" ".state" "CORRUPTED" "$OUTPUT"
assert_json "corrupted reason" ".reason" "base SHA not found" "$OUTPUT"

echo "Test 6: --base-sha without value"
EXIT_CODE=0
OUTPUT=$(cd "$TMPDIR/repo" && "$SCRIPT_DIR/assess-git-state.sh" --base-sha 2>/dev/null) || EXIT_CODE=$?
assert_exit "--base-sha no value → exit 2" 2 "$EXIT_CODE"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
