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

TEST_TMPDIR=$(mktemp -d)
trap 'rm -rf "$TEST_TMPDIR"' EXIT

# A PATH of system dirs cannot hide binaries reliably (macOS ships /usr/bin/jq,
# and providers may live anywhere). Use a stub dir holding only what the
# selection script itself needs: bash for the shebang and jq for JSON output.
STUB_BIN="$TEST_TMPDIR/stub-bin"
mkdir -p "$STUB_BIN"
ln -s "$(command -v bash)" "$STUB_BIN/bash"
ln -s "$(command -v jq)" "$STUB_BIN/jq"

FAKE_BIN="$TEST_TMPDIR/bin"
mkdir -p "$FAKE_BIN"
printf '#!/bin/sh\nexit 0\n' > "$FAKE_BIN/codex"; chmod +x "$FAKE_BIN/codex"

echo "Test 1: external provider available and differs from implementer"
EXIT_CODE=0
OUT=$(PATH="$FAKE_BIN:$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "codex,claude" --implementer claude) || EXIT_CODE=$?
assert_exit "selection exits 0" 0 "$EXIT_CODE"
assert_json "picks codex" ".provider" "codex" "$OUT"

echo "Test 2: unavailable external falls back to implementer provider"
EXIT_CODE=0
OUT=$(PATH="$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "codex,claude" --implementer claude) || EXIT_CODE=$?
assert_exit "fallback exits 0" 0 "$EXIT_CODE"
assert_json "falls back to claude" ".provider" "claude" "$OUT"
assert_json "reason mentions fallback" '.reason | test("fallback")' "true" "$OUT"

echo "Test 3: preference order respected among differing providers"
printf '#!/bin/sh\nexit 0\n' > "$FAKE_BIN/gemini"; chmod +x "$FAKE_BIN/gemini"
OUT=$(PATH="$FAKE_BIN:$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "gemini,codex" --implementer claude)
assert_json "first preferred wins" ".provider" "gemini" "$OUT"

echo "Test 4: missing --preference is invalid input"
EXIT_CODE=0
"$SCRIPT_DIR/select-evaluator-provider.sh" --implementer claude 2>/dev/null || EXIT_CODE=$?
assert_exit "missing preference exits 2" 2 "$EXIT_CODE"

echo "Test 5: empty preference list still returns claude"
OUT=$(PATH="$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference " " --implementer claude)
assert_json "defaults to claude" ".provider" "claude" "$OUT"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
