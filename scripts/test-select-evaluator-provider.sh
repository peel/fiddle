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

echo "Test 6: implementer not in preference list is still the fallback"
CODEX_ONLY_BIN="$TEST_TMPDIR/codex-only-bin"
mkdir -p "$CODEX_ONLY_BIN"
printf '#!/bin/sh\nexit 0\n' > "$CODEX_ONLY_BIN/codex"; chmod +x "$CODEX_ONLY_BIN/codex"
EXIT_CODE=0
OUT=$(PATH="$CODEX_ONLY_BIN:$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "gemini" --implementer codex) || EXIT_CODE=$?
assert_exit "unlisted implementer fallback exits 0" 0 "$EXIT_CODE"
assert_json "falls back to unlisted implementer" ".provider" "codex" "$OUT"
assert_json "reason names the implementer fallback" '.reason | test("implementer")' "true" "$OUT"

echo "Test 7: nothing available and implementer is claude returns claude"
OUT=$(PATH="$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "gemini" --implementer claude)
assert_json "returns claude" ".provider" "claude" "$OUT"
assert_json "reason names the implementer fallback" '.reason | test("implementer")' "true" "$OUT"

echo "Test 8: nothing available and implementer unavailable returns claude"
OUT=$(PATH="$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "gemini" --implementer codex)
assert_json "returns claude as last resort" ".provider" "claude" "$OUT"
assert_json "reason distinguishes the last resort" '.reason | test("implementer")' "false" "$OUT"

echo "Test 9: dangling --preference is invalid input"
EXIT_CODE=0
ERR=$("$SCRIPT_DIR/select-evaluator-provider.sh" --preference 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "dangling --preference exits 2" 2 "$EXIT_CODE"
JSON_OK=0
echo "$ERR" | jq -e . >/dev/null 2>&1 || JSON_OK=$?
assert_exit "dangling --preference stderr is JSON" 0 "$JSON_OK"

echo "Test 10: dangling --implementer is invalid input"
EXIT_CODE=0
ERR=$("$SCRIPT_DIR/select-evaluator-provider.sh" --preference "claude" --implementer 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "dangling --implementer exits 2" 2 "$EXIT_CODE"
JSON_OK=0
echo "$ERR" | jq -e . >/dev/null 2>&1 || JSON_OK=$?
assert_exit "dangling --implementer stderr is JSON" 0 "$JSON_OK"

echo "Test 11: unknown argument containing a double quote emits valid JSON"
EXIT_CODE=0
ERR=$("$SCRIPT_DIR/select-evaluator-provider.sh" --preference "claude" '--bad"arg' 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "quoted unknown argument exits 2" 2 "$EXIT_CODE"
JSON_OK=0
echo "$ERR" | jq -e . >/dev/null 2>&1 || JSON_OK=$?
assert_exit "quoted unknown argument stderr is JSON" 0 "$JSON_OK"

echo "Test 12: selection succeeds without a writable temp directory"
RO_TMPDIR="$TEST_TMPDIR/ro"
mkdir -p "$RO_TMPDIR"
chmod 500 "$RO_TMPDIR"
NO_WRITE=()
if command -v sandbox-exec >/dev/null 2>&1; then
  NO_WRITE=("$(command -v sandbox-exec)" -p '(version 1)(allow default)(deny file-write*)(allow file-write-data (literal "/dev/null"))')
fi
SYSTEM_BASH=/bin/bash
[ -x "$SYSTEM_BASH" ] || SYSTEM_BASH=$(command -v bash)
EXIT_CODE=0
OUT=$(TMPDIR="$RO_TMPDIR" PATH="$FAKE_BIN:$STUB_BIN" "${NO_WRITE[@]}" "$SYSTEM_BASH" \
  "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "codex,claude" --implementer claude) || EXIT_CODE=$?
assert_exit "read-only environment exits 0" 0 "$EXIT_CODE"
assert_json "read-only environment picks codex" ".provider" "codex" "$OUT"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
