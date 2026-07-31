#!/usr/bin/env bash
# test-assemble-evaluator-context.sh — assemble-evaluator-context.sh contract
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

assert_contains() {
  local desc="$1" needle="$2" haystack="$3"
  if printf '%s' "$haystack" | grep -qF "$needle"; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected to contain '$needle')"
  fi
}

assert_not_contains() {
  local desc="$1" needle="$2" haystack="$3"
  if printf '%s' "$haystack" | grep -qF "$needle"; then
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (should not contain '$needle')"
  else
    PASS=$((PASS+1)); echo "  PASS: $desc"
  fi
}

assert_order() {
  local desc="$1" first="$2" second="$3" haystack="$4"
  local l1 l2
  l1=$(printf '%s' "$haystack" | grep -nF "$first" | head -1 | cut -d: -f1)
  l2=$(printf '%s' "$haystack" | grep -nF "$second" | head -1 | cut -d: -f1)
  if [ -n "$l1" ] && [ -n "$l2" ] && [ "$l1" -lt "$l2" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc ('$first' at ${l1:-none}, '$second' at ${l2:-none})"
  fi
}

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

# Build a fake project root: protocol with the placeholder, one domain template,
# calibration and antipattern files each carrying a ## Retired section.
mkdir -p "$TMPDIR/skills/evaluate" "$TMPDIR/docs"
cat > "$TMPDIR/skills/evaluate/SKILL.md" << 'EOF'
PROTOCOL_MARKER
## Antipattern Checking

{ANTIPATTERNS}

tail of protocol
EOF
printf 'GENERAL_TEMPLATE_MARKER\n' > "$TMPDIR/skills/evaluate/evaluator-general.md"
printf 'INFRA_TEMPLATE_MARKER\n' > "$TMPDIR/skills/evaluate/evaluator-infrastructure.md"
cat > "$TMPDIR/docs/evaluator-calibration-general.md" << 'EOF'
LIVE_ANCHOR
## Retired
RETIRED_ANCHOR
EOF
cat > "$TMPDIR/docs/antipatterns-general.md" << 'EOF'
LIVE_ANTIPATTERN
## Retired
RETIRED_ANTIPATTERN
EOF
cat > "$TMPDIR/docs/custom-calibration.md" << 'EOF'
CUSTOM_ANCHOR
EOF

run() { "$SCRIPT_DIR/assemble-evaluator-context.sh" --root "$TMPDIR" "$@"; }

echo "=== Test 1: protocol precedes the domain template ==="
cat > "$TMPDIR/orchestrate.json" << 'EOF'
{"evaluators":{"domains":{"general":{"template":"evaluator-general"}}}}
EOF
EXIT_CODE=0
OUT=$(run --domain general --config "$TMPDIR/orchestrate.json") || EXIT_CODE=$?
assert_exit "assembles → exit 0" 0 "$EXIT_CODE"
assert_contains "includes protocol" "PROTOCOL_MARKER" "$OUT"
assert_contains "includes domain template" "GENERAL_TEMPLATE_MARKER" "$OUT"
assert_order "protocol before template" "PROTOCOL_MARKER" "GENERAL_TEMPLATE_MARKER" "$OUT"

echo ""
echo "=== Test 2: calibration loads from the default path and follows the template ==="
assert_contains "includes live anchor" "LIVE_ANCHOR" "$OUT"
assert_order "template before calibration" "GENERAL_TEMPLATE_MARKER" "LIVE_ANCHOR" "$OUT"

echo ""
echo "=== Test 3: retired content is excluded ==="
assert_not_contains "retired anchor excluded" "RETIRED_ANCHOR" "$OUT"
assert_not_contains "retired antipattern excluded" "RETIRED_ANTIPATTERN" "$OUT"

echo ""
echo "=== Test 4: antipatterns replace the placeholder in the protocol ==="
cat > "$TMPDIR/orchestrate.json" << 'EOF'
{"evaluators":{"domains":{"general":{"template":"evaluator-general","antipatterns":"docs/antipatterns-general.md"}}}}
EOF
OUT=$(run --domain general --config "$TMPDIR/orchestrate.json")
assert_contains "includes live antipattern" "LIVE_ANTIPATTERN" "$OUT"
assert_not_contains "placeholder consumed" "{ANTIPATTERNS}" "$OUT"
assert_order "antipattern sits inside the protocol, before its tail" "LIVE_ANTIPATTERN" "tail of protocol" "$OUT"

echo ""
echo "=== Test 5: configured calibration path overrides the default ==="
cat > "$TMPDIR/orchestrate.json" << 'EOF'
{"evaluators":{"domains":{"general":{"template":"evaluator-general","calibration":"docs/custom-calibration.md"}}}}
EOF
OUT=$(run --domain general --config "$TMPDIR/orchestrate.json")
assert_contains "includes configured anchor" "CUSTOM_ANCHOR" "$OUT"
assert_not_contains "default anchor not loaded" "LIVE_ANCHOR" "$OUT"

echo ""
echo "=== Test 6: no calibration and no antipatterns still assembles ==="
cat > "$TMPDIR/orchestrate.json" << 'EOF'
{"evaluators":{"domains":{"infrastructure":{"template":"evaluator-infrastructure"}}}}
EOF
EXIT_CODE=0
OUT=$(run --domain infrastructure --config "$TMPDIR/orchestrate.json") || EXIT_CODE=$?
assert_exit "no optional files → exit 0" 0 "$EXIT_CODE"
assert_contains "includes infra template" "INFRA_TEMPLATE_MARKER" "$OUT"
assert_not_contains "placeholder consumed when empty" "{ANTIPATTERNS}" "$OUT"

echo ""
echo "=== Test 7: unknown domain falls back to evaluator-general ==="
EXIT_CODE=0
OUT=$(run --domain nosuchdomain --config "$TMPDIR/orchestrate.json") || EXIT_CODE=$?
assert_exit "unknown domain → exit 0 via fallback" 0 "$EXIT_CODE"
assert_contains "falls back to general template" "GENERAL_TEMPLATE_MARKER" "$OUT"

echo ""
echo "=== Test 8: invalid input exits 2 with a JSON error ==="
EXIT_CODE=0
ERR=$(run --config "$TMPDIR/orchestrate.json" 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "missing --domain → exit 2" 2 "$EXIT_CODE"
assert_contains "error is JSON" '"error"' "$ERR"

EXIT_CODE=0
run --domain general --config "$TMPDIR/nope.json" >/dev/null 2>&1 || EXIT_CODE=$?
assert_exit "missing config → exit 2" 2 "$EXIT_CODE"

printf 'not json' > "$TMPDIR/bad.json"
EXIT_CODE=0
run --domain general --config "$TMPDIR/bad.json" >/dev/null 2>&1 || EXIT_CODE=$?
assert_exit "invalid JSON → exit 2" 2 "$EXIT_CODE"

EXIT_CODE=0
run --domain general --config "$TMPDIR/orchestrate.json" --root "$TMPDIR/missing" >/dev/null 2>&1 || EXIT_CODE=$?
assert_exit "missing protocol file → exit 2" 2 "$EXIT_CODE"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
