#!/usr/bin/env bash
# test-runtime-e2e.sh — Integration test: runtime evaluation e2e lifecycle
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
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

assert_contains() {
  local desc="$1" file="$2" pattern="$3"
  if grep -q "$pattern" "$file" 2>/dev/null; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (pattern '$pattern' not found in $file)"
  fi
}

is_running() {
  kill -0 "$1" 2>/dev/null && echo "true" || echo "false"
}

TEST_TMPDIR=$(mktemp -d)
# Track PIDs to kill on cleanup
CLEANUP_PIDS=()

cleanup() {
  for pid in "${CLEANUP_PIDS[@]:-}"; do
    [[ -n "$pid" ]] || continue
    kill "$pid" 2>/dev/null || true
    kill -9 "$pid" 2>/dev/null || true
  done
  rm -rf "$TEST_TMPDIR"
}
trap 'cleanup' EXIT

# Pick a random high port to avoid conflicts
TEST_PORT=$((RANDOM + 10000))

# ═══════════════════════════════════════════════════════════════════════
echo "=== Test 1: Full lifecycle — start, curl, stop ==="
# ═══════════════════════════════════════════════════════════════════════

# 1a. Create domains config
cat > "$TEST_TMPDIR/domains.json" << EOF
{
  "domains": {
    "backend": {
      "template": "evaluator-general",
      "runtime": ["python3 -m http.server $TEST_PORT"],
      "ready_check": {"type": "http", "url": "http://localhost:$TEST_PORT", "timeout_ms": 10000, "retry_interval_ms": 500}
    }
  }
}
EOF

# 1b. Start runtimes and capture state JSON
EXIT_CODE=0
STATE_JSON=$("$SCRIPT_DIR/start-runtimes.sh" --domains "$TEST_TMPDIR/domains.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "start-runtimes.sh exits 0" 0 "$EXIT_CODE"

# 1c. Verify runtime state JSON has pid and port
HAS_PID=$(echo "$STATE_JSON" | jq -r '.[0].pid // empty' 2>/dev/null)
assert_true "state JSON has pid" "$([ -n "$HAS_PID" ] && echo "true" || echo "false")"

HAS_PORT=$(echo "$STATE_JSON" | jq -r '.[0].port // empty' 2>/dev/null)
assert_true "state JSON has port" "$([ -n "$HAS_PORT" ] && echo "true" || echo "false")"

# Track pid for cleanup in case stop-runtimes fails
if [ -n "$HAS_PID" ]; then
  CLEANUP_PIDS+=("$HAS_PID")
fi

# 1d. Curl the server and verify it responds
CURL_STATUS=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:$TEST_PORT/" 2>/dev/null || echo "0")
assert_true "curl server returns 200" "$([ "$CURL_STATUS" = "200" ] && echo "true" || echo "false")"

# 1e. Write state to file and stop runtimes
echo "$STATE_JSON" > "$TEST_TMPDIR/runtime-state.json"
EXIT_CODE=0
"$SCRIPT_DIR/stop-runtimes.sh" --state "$TEST_TMPDIR/runtime-state.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "stop-runtimes.sh exits 0" 0 "$EXIT_CODE"

# 1f. Verify the server process is dead
sleep 0.5
if [ -n "$HAS_PID" ]; then
  assert_true "server process is dead after stop" "$([ "$(is_running "$HAS_PID")" = "false" ] && echo "true" || echo "false")"
  # Remove from cleanup since it's already dead
  CLEANUP_PIDS=()
fi

echo ""
# ═══════════════════════════════════════════════════════════════════════
echo "=== Test 2: Runtime-evidence skill exists and has required sections ==="
# ═══════════════════════════════════════════════════════════════════════

EVIDENCE_SKILL="$PROJECT_DIR/skills/runtime-evidence/SKILL.md"

assert_true "runtime-evidence/SKILL.md exists" "$([ -f "$EVIDENCE_SKILL" ] && echo "true" || echo "false")"
assert_contains "states record-before-scoring" "$EVIDENCE_SKILL" "before scoring any dimension"
assert_contains "contains Evidence Gathering" "$EVIDENCE_SKILL" "Evidence Gathering"
assert_contains "contains Failure Classification" "$EVIDENCE_SKILL" "Failure Classification"

echo ""
# ═══════════════════════════════════════════════════════════════════════
echo "=== Test 3: Domain evaluator templates exist ==="
# ═══════════════════════════════════════════════════════════════════════

FRONTEND_TEMPLATE="$PROJECT_DIR/skills/evaluate/evaluator-frontend.md"
BACKEND_TEMPLATE="$PROJECT_DIR/skills/evaluate/evaluator-backend.md"

assert_true "evaluator-frontend.md exists" "$([ -f "$FRONTEND_TEMPLATE" ] && echo "true" || echo "false")"
assert_contains "frontend has Visual Quality" "$FRONTEND_TEMPLATE" "Visual Quality"
assert_contains "frontend has Craft" "$FRONTEND_TEMPLATE" "Craft"

assert_true "evaluator-backend.md exists" "$([ -f "$BACKEND_TEMPLATE" ] && echo "true" || echo "false")"
assert_contains "backend has API Contract Fidelity" "$BACKEND_TEMPLATE" "API Contract Fidelity"
assert_contains "backend has Error Handling" "$BACKEND_TEMPLATE" "Error Handling"

echo ""
# ═══════════════════════════════════════════════════════════════════════
echo "=== Test 4: Develop loop and holistic skills have runtime lifecycle ==="
# ═══════════════════════════════════════════════════════════════════════

DEVELOP_LOOP_DIR="$PROJECT_DIR/skills/develop-loop"
DEVELOP_HOLISTIC_DIR="$PROJECT_DIR/skills/develop-holistic"

# The skill is the directory, not the file. These assertions named
# `develop-loop/SKILL.md` and went red when the runtime lifecycle moved into
# `dispatch-and-evidence.md` in the SKILL.md → companion-file split, while the
# skill still said everything it had said before. What has to hold is that the
# skill *carries* the reference; which of its files carries it is layout, and this
# project has already reorganised that layout once.
#
# A pass prints where it found it, so the next move is visible in the log rather
# than silent, and a failure prints the whole tree it searched — otherwise "not
# referenced" and "searched the wrong directory" read identically.
assert_contains_tree() {
  local desc="$1" dir="$2" pattern="$3"
  local hits
  hits=$(grep -rl -- "$pattern" "$dir" 2>/dev/null | sed "s|^$dir/||" | sort | tr '\n' ' ')
  if [ -n "$hits" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc (in ${hits% })"
  else
    FAIL=$((FAIL+1))
    echo "  FAIL: $desc (pattern '$pattern' in none of: $(ls "$dir" 2>/dev/null | tr '\n' ' '))"
  fi
}

assert_contains_tree "develop-loop references start-runtimes.sh" "$DEVELOP_LOOP_DIR" "start-runtimes.sh"
assert_contains_tree "develop-loop references stop-runtimes.sh" "$DEVELOP_LOOP_DIR" "stop-runtimes.sh"
assert_contains_tree "develop-loop references runtime-evidence" "$DEVELOP_LOOP_DIR" "runtime-evidence"
assert_contains_tree "develop-holistic references start-runtimes.sh" "$DEVELOP_HOLISTIC_DIR" "start-runtimes.sh"
assert_contains_tree "develop-holistic references stop-runtimes.sh" "$DEVELOP_HOLISTIC_DIR" "stop-runtimes.sh"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
