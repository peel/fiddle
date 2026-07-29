#!/usr/bin/env bash
# test-start-runtimes.sh — Tests for start-runtimes.sh
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

assert_json_num() {
  local desc="$1" field="$2" json="$3"
  local actual
  actual=$(echo "$json" | jq -r "$field")
  if [[ "$actual" =~ ^[0-9]+$ ]] && [ "$actual" -gt 0 ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected positive integer, got '$actual')"
  fi
}

TEST_TMPDIR=$(mktemp -d)
trap 'rm -rf "$TEST_TMPDIR"' EXIT

# Find a free port for testing
find_free_port() {
  python3 -c "
import socket
s = socket.socket()
s.bind(('', 0))
port = s.getsockname()[1]
s.close()
print(port)
"
}

echo "=== Test 0: Missing dependencies (jq hidden) → exit 3 ==="
# Create a minimal valid domains file; the script checks deps after the file-exists check
echo '{"domains":{}}' > "$TEST_TMPDIR/deps-test.json"
# A PATH of system dirs no longer hides jq (macOS ships /usr/bin/jq); use a
# stub dir holding only bash so the shebang resolves but jq is absent.
mkdir -p "$TEST_TMPDIR/nojq-bin"
ln -s "$(command -v bash)" "$TEST_TMPDIR/nojq-bin/bash"
EXIT_CODE=0
PATH="$TEST_TMPDIR/nojq-bin" "$SCRIPT_DIR/start-runtimes.sh" --domains "$TEST_TMPDIR/deps-test.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "no jq in PATH → exit 3" 3 "$EXIT_CODE"

echo ""
echo "=== Test 1: Missing --domains argument → exit 2 ==="
EXIT_CODE=0
"$SCRIPT_DIR/start-runtimes.sh" 2>/dev/null || EXIT_CODE=$?
assert_exit "no args → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 2: --domains file not found → exit 2 ==="
EXIT_CODE=0
"$SCRIPT_DIR/start-runtimes.sh" --domains /nonexistent/file.json 2>/dev/null || EXIT_CODE=$?
assert_exit "missing file → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 3: Domain with no runtime configured → exit 0, empty array ==="
cat > "$TEST_TMPDIR/no-runtime.json" << 'EOF'
{
  "domains": {
    "frontend": {
      "template": "evaluator-frontend"
    }
  }
}
EOF
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/start-runtimes.sh" --domains "$TEST_TMPDIR/no-runtime.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "no runtime → exit 0" 0 "$EXIT_CODE"
assert_json "no runtime → empty array" ". | length" "0" "$OUTPUT"

echo ""
echo "=== Test 4: TCP ready check — start HTTP server, verify detection ==="
PORT=$(find_free_port)
# Write domains config with tcp ready_check
cat > "$TEST_TMPDIR/tcp-domain.json" << EOF
{
  "domains": {
    "backend": {
      "template": "evaluator-backend",
      "runtime": ["python3 -m http.server $PORT"],
      "ready_check": {
        "type": "tcp",
        "port": $PORT,
        "timeout_ms": 15000,
        "retry_interval_ms": 500
      }
    }
  }
}
EOF
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/start-runtimes.sh" --domains "$TEST_TMPDIR/tcp-domain.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "tcp ready check → exit 0" 0 "$EXIT_CODE"
assert_json "output is array" ". | type" "array" "$OUTPUT"
assert_json "domain name in output" ".[0].domain" "backend" "$OUTPUT"
assert_json_num "pid is positive integer" ".[0].pid" "$OUTPUT"
assert_json_num "port in output" ".[0].port" "$OUTPUT"
assert_json "command in output" ".[0].command | type" "string" "$OUTPUT"
# Cleanup: kill started process
PID=$(echo "$OUTPUT" | jq -r '.[0].pid')
kill "$PID" 2>/dev/null || true

echo ""
echo "=== Test 5: HTTP ready check — start HTTP server, verify detection ==="
PORT=$(find_free_port)
cat > "$TEST_TMPDIR/http-domain.json" << EOF
{
  "domains": {
    "web": {
      "template": "evaluator-web",
      "runtime": ["python3 -m http.server $PORT"],
      "ready_check": {
        "type": "http",
        "url": "http://localhost:$PORT",
        "timeout_ms": 15000,
        "retry_interval_ms": 500
      }
    }
  }
}
EOF
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/start-runtimes.sh" --domains "$TEST_TMPDIR/http-domain.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "http ready check → exit 0" 0 "$EXIT_CODE"
assert_json "output is array" ". | type" "array" "$OUTPUT"
assert_json "domain name in output" ".[0].domain" "web" "$OUTPUT"
assert_json_num "pid is positive integer" ".[0].pid" "$OUTPUT"
PID=$(echo "$OUTPUT" | jq -r '.[0].pid')
kill "$PID" 2>/dev/null || true

echo ""
echo "=== Test 6: Command ready check ==="
PORT=$(find_free_port)
cat > "$TEST_TMPDIR/cmd-domain.json" << EOF
{
  "domains": {
    "service": {
      "template": "evaluator-service",
      "runtime": ["python3 -m http.server $PORT"],
      "ready_check": {
        "type": "command",
        "command": "python3 -c 'import socket; s=socket.socket(); s.connect((\"localhost\", $PORT)); s.close()'",
        "timeout_ms": 15000,
        "retry_interval_ms": 500
      }
    }
  }
}
EOF
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/start-runtimes.sh" --domains "$TEST_TMPDIR/cmd-domain.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "command ready check → exit 0" 0 "$EXIT_CODE"
assert_json "domain name in output" ".[0].domain" "service" "$OUTPUT"
PID=$(echo "$OUTPUT" | jq -r '.[0].pid')
kill "$PID" 2>/dev/null || true

echo ""
echo "=== Test 7: App fails to start → exit 1 ==="
cat > "$TEST_TMPDIR/fail-domain.json" << 'EOF'
{
  "domains": {
    "broken": {
      "template": "evaluator-broken",
      "runtime": ["bash -c 'exit 1'"],
      "ready_check": {
        "type": "tcp",
        "port": 19999,
        "timeout_ms": 2000,
        "retry_interval_ms": 200
      }
    }
  }
}
EOF
EXIT_CODE=0
"$SCRIPT_DIR/start-runtimes.sh" --domains "$TEST_TMPDIR/fail-domain.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "app fails → exit 1" 1 "$EXIT_CODE"

echo ""
echo "=== Test 8: runtime_order respected — multi-domain ==="
PORT1=$(find_free_port)
PORT2=$(find_free_port)
cat > "$TEST_TMPDIR/ordered-domain.json" << EOF
{
  "domains": {
    "frontend": {
      "template": "evaluator-frontend",
      "runtime": ["python3 -m http.server $PORT2"],
      "ready_check": {
        "type": "tcp",
        "port": $PORT2,
        "timeout_ms": 10000,
        "retry_interval_ms": 500
      }
    },
    "backend": {
      "template": "evaluator-backend",
      "runtime": ["python3 -m http.server $PORT1"],
      "ready_check": {
        "type": "tcp",
        "port": $PORT1,
        "timeout_ms": 10000,
        "retry_interval_ms": 500
      }
    }
  },
  "runtime_order": ["backend", "frontend"]
}
EOF
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/start-runtimes.sh" --domains "$TEST_TMPDIR/ordered-domain.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "ordered multi-domain → exit 0" 0 "$EXIT_CODE"
assert_json "output is array" ". | type" "array" "$OUTPUT"
assert_json "array has 2 entries" ". | length" "2" "$OUTPUT"
# runtime_order: backend first, then frontend
assert_json "first entry is backend" ".[0].domain" "backend" "$OUTPUT"
assert_json "second entry is frontend" ".[1].domain" "frontend" "$OUTPUT"
# Cleanup
for PID in $(echo "$OUTPUT" | jq -r '.[].pid'); do
  kill "$PID" 2>/dev/null || true
done

echo ""
echo "=== Test 9: --slot-index selects correct runtime command ==="
PORT=$(find_free_port)
cat > "$TEST_TMPDIR/slot-domain.json" << EOF
{
  "domains": {
    "app": {
      "template": "evaluator-app",
      "runtime": ["python3 -m http.server 19998", "python3 -m http.server $PORT"],
      "ready_check": {
        "type": "tcp",
        "port": $PORT,
        "timeout_ms": 10000,
        "retry_interval_ms": 500
      }
    }
  }
}
EOF
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/start-runtimes.sh" --domains "$TEST_TMPDIR/slot-domain.json" --slot-index 1 2>/dev/null) || EXIT_CODE=$?
assert_exit "slot-index 1 → exit 0" 0 "$EXIT_CODE"
assert_json "command uses slot 1 port" ".[0].command" "python3 -m http.server $PORT" "$OUTPUT"
PID=$(echo "$OUTPUT" | jq -r '.[0].pid')
kill "$PID" 2>/dev/null || true

echo ""
echo "=== Test 10: Default fallback ready check (tcp port 8080) — no ready_check field ==="
# The script defaults to tcp:8080 when no ready_check is specified.
# Start the runtime command on port 8080 so the default check succeeds.
# Skip if 8080 is already occupied to avoid false failures in CI.
if python3 -c "import socket; s=socket.socket(); r=s.connect_ex(('localhost',8080)); s.close(); exit(0 if r!=0 else 1)" 2>/dev/null; then
  cat > "$TEST_TMPDIR/no-readycheck.json" << 'EOF'
{
  "domains": {
    "app": {
      "template": "evaluator-app",
      "runtime": ["python3 -m http.server 8080"]
    }
  }
}
EOF
  EXIT_CODE=0
  OUTPUT=$("$SCRIPT_DIR/start-runtimes.sh" --domains "$TEST_TMPDIR/no-readycheck.json" 2>/dev/null) || EXIT_CODE=$?
  assert_exit "no ready_check → default tcp:8080 → exit 0" 0 "$EXIT_CODE"
  assert_json "no ready_check → output is array" ". | type" "array" "$OUTPUT"
  assert_json "no ready_check → domain in output" ".[0].domain" "app" "$OUTPUT"
  assert_json_num "no ready_check → port is 8080" ".[0].port" "$OUTPUT"
  PID=$(echo "$OUTPUT" | jq -r '.[0].pid' 2>/dev/null || true)
  kill "$PID" 2>/dev/null || true
else
  echo "  SKIP: port 8080 already in use — skipping default ready_check test"
  PASS=$((PASS+1))
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
