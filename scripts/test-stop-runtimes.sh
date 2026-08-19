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

assert_true() {
  local desc="$1" result="$2"
  if [ "$result" = "true" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected true, got '$result')"
  fi
}

is_running() {
  kill -0 "$1" 2>/dev/null && echo "true" || echo "false"
}

TEST_TMPDIR=$(mktemp -d)
trap 'rm -rf "$TEST_TMPDIR"' EXIT

echo "=== Test 1: Missing --state argument → exit 2 ==="
EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" 2>/dev/null || EXIT_CODE=$?
assert_exit "no args → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 2: Nonexistent state file → exit 2 ==="
EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state /nonexistent/runtime-state.json 2>/dev/null || EXIT_CODE=$?
assert_exit "missing file → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 3: Invalid JSON in state file → exit 2 ==="
echo "not json at all" > "$TEST_TMPDIR/bad.json"
EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state "$TEST_TMPDIR/bad.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "invalid JSON → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 4: Empty state array → exit 0 ==="
echo "[]" > "$TEST_TMPDIR/empty.json"
EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state "$TEST_TMPDIR/empty.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "empty array → exit 0" 0 "$EXIT_CODE"

echo ""
echo "=== Test 5: Single process — start sleep, stop, verify dead ==="
sleep 9999 &
SLEEP_PID=$!
assert_true "sleep process started" "$(is_running "$SLEEP_PID")"

cat > "$TEST_TMPDIR/single.json" << EOF
[{"domain":"test","pid":$SLEEP_PID,"port":0,"command":"sleep 9999"}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state "$TEST_TMPDIR/single.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "single process → exit 0" 0 "$EXIT_CODE"

sleep 0.5
assert_true "process is dead after stop" "$([ "$(is_running "$SLEEP_PID")" = "false" ] && echo "true" || echo "false")"

echo ""
echo "=== Test 6: Multiple processes — start 2 sleeps, stop both, verify both dead ==="
sleep 9999 &
PID1=$!
sleep 9999 &
PID2=$!
assert_true "process 1 started" "$(is_running "$PID1")"
assert_true "process 2 started" "$(is_running "$PID2")"

cat > "$TEST_TMPDIR/multi.json" << EOF
[{"domain":"backend","pid":$PID1,"port":8080,"command":"sleep 9999"},
 {"domain":"frontend","pid":$PID2,"port":8081,"command":"sleep 9999"}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state "$TEST_TMPDIR/multi.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "multiple processes → exit 0" 0 "$EXIT_CODE"

sleep 0.5
assert_true "process 1 is dead" "$([ "$(is_running "$PID1")" = "false" ] && echo "true" || echo "false")"
assert_true "process 2 is dead" "$([ "$(is_running "$PID2")" = "false" ] && echo "true" || echo "false")"

echo ""
echo "=== Test 7: Already dead process — verify graceful handling (exit 0) ==="
DEAD_PID=2147483647
cat > "$TEST_TMPDIR/dead.json" << EOF
[{"domain":"ghost","pid":$DEAD_PID,"port":0,"command":"sleep 9999"}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state "$TEST_TMPDIR/dead.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "already dead PID → exit 0" 0 "$EXIT_CODE"

echo ""
echo "=== Test 8: --state with no value → exit 2 ==="
EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state 2>/dev/null || EXIT_CODE=$?
assert_exit "--state with no value → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 9: Non-numeric PID → exit 2 ==="
cat > "$TEST_TMPDIR/bad-pid-alpha.json" << 'EOF'
[{"domain":"test","pid":"abc","port":0,"command":"sleep 9999"}]
EOF
EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state "$TEST_TMPDIR/bad-pid-alpha.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "non-numeric PID → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 10: Zero PID → exit 2 ==="
cat > "$TEST_TMPDIR/bad-pid-zero.json" << 'EOF'
[{"domain":"test","pid":0,"port":0,"command":"sleep 9999"}]
EOF
EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state "$TEST_TMPDIR/bad-pid-zero.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "zero PID → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 11: Negative PID → exit 2 ==="
cat > "$TEST_TMPDIR/bad-pid-neg.json" << 'EOF'
[{"domain":"test","pid":-1,"port":0,"command":"sleep 9999"}]
EOF
EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state "$TEST_TMPDIR/bad-pid-neg.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "negative PID → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 12: SIGKILL fallback — SIGTERM-ignoring process killed within ~12s ==="
bash -c 'trap "" TERM; sleep 9999' &
STUBBORN_PID=$!
sleep 0.3
assert_true "SIGTERM-ignoring process started" "$(is_running "$STUBBORN_PID")"

cat > "$TEST_TMPDIR/stubborn.json" << EOF
[{"domain":"stubborn","pid":$STUBBORN_PID,"port":0,"command":"trap TERM sleep"}]
EOF

START_TIME=$(date +%s)
EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state "$TEST_TMPDIR/stubborn.json" 2>/dev/null || EXIT_CODE=$?
END_TIME=$(date +%s)
ELAPSED=$(( END_TIME - START_TIME ))

assert_exit "SIGKILL fallback → exit 0" 0 "$EXIT_CODE"

sleep 0.5
assert_true "stubborn process is dead after SIGKILL" "$([ "$(is_running "$STUBBORN_PID")" = "false" ] && echo "true" || echo "false")"

if [[ $ELAPSED -ge 8 && $ELAPSED -le 15 ]]; then
  PASS=$((PASS+1)); echo "  PASS: SIGKILL fallback took ~${ELAPSED}s (expected 8-15s)"
else
  FAIL=$((FAIL+1)); echo "  FAIL: SIGKILL fallback took ${ELAPSED}s (expected 8-15s)"
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
