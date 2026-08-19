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

cat > "$TMPDIR/orchestrate.json" << 'EOF'
{
  "evaluators": {
    "domains": {
      "general": {
        "template": "evaluator-general",
        "providers": ["claude"]
      },
      "frontend": {
        "template": "evaluator-frontend",
        "providers": ["claude"],
        "runtime": ["flutter run --debug"],
        "ready_check": {
          "type": "tcp",
          "port": 8080,
          "timeout_ms": 15000
        },
        "runtime_agent": "skills/frontend-runtime/agent.md",
        "stack_agents": ["skills/flutter/agent.md"]
      },
      "backend": {
        "template": "evaluator-backend",
        "providers": ["claude", "codex"],
        "runtime": ["go run ./cmd/server"],
        "ready_check": {
          "type": "http",
          "url": "http://localhost:9090/health",
          "timeout_ms": 10000
        }
      }
    }
  }
}
EOF

echo "=== Test 1: Single domain resolves to config ==="
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --domains "frontend" --config "$TMPDIR/orchestrate.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "single domain → exit 0" 0 "$EXIT_CODE"
assert_json "output is array" ". | type" "array" "$OUTPUT"
assert_json "array has 1 entry" ". | length" "1" "$OUTPUT"
assert_json "domain is frontend" ".[0].domain" "frontend" "$OUTPUT"
assert_json "template is evaluator-frontend" ".[0].template" "evaluator-frontend" "$OUTPUT"
assert_json "resolved_via is config" ".[0].resolved_via" "config" "$OUTPUT"
assert_json "runtime is array" ".[0].runtime | type" "array" "$OUTPUT"
assert_json "runtime[0] matches" ".[0].runtime[0]" "flutter run --debug" "$OUTPUT"
assert_json "ready_check present" ".[0].ready_check.type" "tcp" "$OUTPUT"
assert_json "ready_check port" ".[0].ready_check.port" "8080" "$OUTPUT"
assert_json "providers copied" ".[0].providers[0]" "claude" "$OUTPUT"
assert_json "runtime_agent copied" ".[0].runtime_agent" "skills/frontend-runtime/agent.md" "$OUTPUT"
assert_json "stack_agents copied" ".[0].stack_agents[0]" "skills/flutter/agent.md" "$OUTPUT"

echo ""
echo "=== Test 2: Multiple domains both resolved ==="
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --domains "frontend,backend" --config "$TMPDIR/orchestrate.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "multi domain → exit 0" 0 "$EXIT_CODE"
assert_json "array has 2 entries" ". | length" "2" "$OUTPUT"
assert_json "first domain is backend (sorted)" ".[0].domain" "backend" "$OUTPUT"
assert_json "first template" ".[0].template" "evaluator-backend" "$OUTPUT"
assert_json "first resolved_via" ".[0].resolved_via" "config" "$OUTPUT"
assert_json "second domain is frontend (sorted)" ".[1].domain" "frontend" "$OUTPUT"
assert_json "second template" ".[1].template" "evaluator-frontend" "$OUTPUT"
assert_json "second resolved_via" ".[1].resolved_via" "config" "$OUTPUT"
assert_json "backend has runtime" ".[0].runtime[0]" "go run ./cmd/server" "$OUTPUT"
assert_json "backend ready_check type" ".[0].ready_check.type" "http" "$OUTPUT"
assert_json "backend has 2 providers" ".[0].providers | length" "2" "$OUTPUT"

echo ""
echo "=== Test 3: Unknown domain falls back to general ==="
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --domains "mobile" --config "$TMPDIR/orchestrate.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "unknown domain → exit 0 (fallback)" 0 "$EXIT_CODE"
assert_json "array has 1 entry" ". | length" "1" "$OUTPUT"
assert_json "domain is mobile" ".[0].domain" "mobile" "$OUTPUT"
assert_json "template is evaluator-general" ".[0].template" "evaluator-general" "$OUTPUT"
assert_json "resolved_via is fallback" ".[0].resolved_via" "fallback" "$OUTPUT"

echo ""
echo "=== Test 4: Mix of known and unknown domains ==="
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --domains "frontend,mobile" --config "$TMPDIR/orchestrate.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "mixed domains → exit 0" 0 "$EXIT_CODE"
assert_json "array has 2 entries" ". | length" "2" "$OUTPUT"
assert_json "first is frontend config" ".[0].resolved_via" "config" "$OUTPUT"
assert_json "second is fallback" ".[1].resolved_via" "fallback" "$OUTPUT"
assert_json "second domain is mobile" ".[1].domain" "mobile" "$OUTPUT"
assert_json "fallback template" ".[1].template" "evaluator-general" "$OUTPUT"

echo ""
echo "=== Test 5: Missing --domains argument → exit 2 ==="
EXIT_CODE=0
"$SCRIPT_DIR/resolve-domains.sh" --config "$TMPDIR/orchestrate.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "missing --domains → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 6: Missing --config argument → exit 2 ==="
EXIT_CODE=0
"$SCRIPT_DIR/resolve-domains.sh" --domains "frontend" 2>/dev/null || EXIT_CODE=$?
assert_exit "missing --config → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 7: Config file not found → exit 2 ==="
EXIT_CODE=0
"$SCRIPT_DIR/resolve-domains.sh" --domains "frontend" --config "/nonexistent/orchestrate.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "config not found → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 8: Invalid JSON in config → exit 2 ==="
echo "not valid json" > "$TMPDIR/bad.json"
EXIT_CODE=0
"$SCRIPT_DIR/resolve-domains.sh" --domains "frontend" --config "$TMPDIR/bad.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "invalid JSON → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 9: Empty domains string → exit 2 ==="
EXIT_CODE=0
"$SCRIPT_DIR/resolve-domains.sh" --domains "" --config "$TMPDIR/orchestrate.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "empty domains → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 10: Unknown argument → exit 2 ==="
EXIT_CODE=0
"$SCRIPT_DIR/resolve-domains.sh" --bogus "foo" 2>/dev/null || EXIT_CODE=$?
assert_exit "unknown arg → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 11: 'general' domain resolves from config ==="
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --domains "general" --config "$TMPDIR/orchestrate.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "general domain → exit 0" 0 "$EXIT_CODE"
assert_json "domain is general" ".[0].domain" "general" "$OUTPUT"
assert_json "template is evaluator-general" ".[0].template" "evaluator-general" "$OUTPUT"
assert_json "resolved_via is config" ".[0].resolved_via" "config" "$OUTPUT"
assert_json "providers from config" ".[0].providers[0]" "claude" "$OUTPUT"

echo ""
echo "=== Test 12: Fallback copies general config fields ==="
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --domains "unknown" --config "$TMPDIR/orchestrate.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "fallback domain → exit 0" 0 "$EXIT_CODE"
assert_json "fallback gets general providers" ".[0].providers[0]" "claude" "$OUTPUT"

echo ""
echo "=== Test 13: Whitespace around domain names is trimmed ==="
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --domains " frontend , backend " --config "$TMPDIR/orchestrate.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "whitespace trimmed → exit 0" 0 "$EXIT_CODE"
assert_json "array has 2 entries" ". | length" "2" "$OUTPUT"
assert_json "first domain is backend (sorted)" ".[0].domain" "backend" "$OUTPUT"
assert_json "first resolved_via config" ".[0].resolved_via" "config" "$OUTPUT"
assert_json "second domain is frontend (sorted)" ".[1].domain" "frontend" "$OUTPUT"
assert_json "second resolved_via config" ".[1].resolved_via" "config" "$OUTPUT"

echo ""
echo "=== Test 14: Duplicate domains are deduplicated ==="
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --domains "frontend,frontend" --config "$TMPDIR/orchestrate.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "dedup → exit 0" 0 "$EXIT_CODE"
assert_json "array has 1 entry (deduped)" ". | length" "1" "$OUTPUT"
assert_json "domain is frontend" ".[0].domain" "frontend" "$OUTPUT"
assert_json "resolved_via config" ".[0].resolved_via" "config" "$OUTPUT"

echo ""
echo "=== Test 15: --help flag exits 0 and prints usage ==="
EXIT_CODE=0
HELP_OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --help 2>&1) || EXIT_CODE=$?
assert_exit "--help → exit 0" 0 "$EXIT_CODE"
if echo "$HELP_OUTPUT" | grep -q "Usage:"; then
  PASS=$((PASS+1)); echo "  PASS: --help prints usage text"
else
  FAIL=$((FAIL+1)); echo "  FAIL: --help does not print usage text"
fi

echo ""
echo "=== Test 16: -h flag exits 0 and prints usage ==="
EXIT_CODE=0
HELP_OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" -h 2>&1) || EXIT_CODE=$?
assert_exit "-h → exit 0" 0 "$EXIT_CODE"
if echo "$HELP_OUTPUT" | grep -q "Usage:"; then
  PASS=$((PASS+1)); echo "  PASS: -h prints usage text"
else
  FAIL=$((FAIL+1)); echo "  FAIL: -h does not print usage text"
fi

echo ""
echo "=== Test 17: Whitespace-only domain after trim is excluded ==="
EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --domains "frontend, , backend" --config "$TMPDIR/orchestrate.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "whitespace-only entries filtered → exit 0" 0 "$EXIT_CODE"
assert_json "array has 2 entries" ". | length" "2" "$OUTPUT"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
