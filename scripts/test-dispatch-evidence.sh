#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PASS=0; FAIL=0

assert_contains() {
  local desc="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -qF "$needle"; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (missing '$needle')"
  fi
}

assert_not_contains() {
  local desc="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -qF "$needle"; then
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (unexpected '$needle')"
  else
    PASS=$((PASS+1)); echo "  PASS: $desc"
  fi
}

assert_exit() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected exit $expected, got $actual)"
  fi
}

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

mkdir -p "$TMPDIR/hooks" "$TMPDIR/skills/develop" "$TMPDIR/bin"
cp "$REPO_DIR/hooks/dispatch-provider.sh" "$TMPDIR/hooks/"
cp "$REPO_DIR/skills/develop/provider-context.md" "$TMPDIR/skills/develop/"

cat > "$TMPDIR/bin/fake-provider" << 'EOF'
#!/usr/bin/env bash
cat
EOF
chmod +x "$TMPDIR/bin/fake-provider"
export PATH="$TMPDIR/bin:$PATH"

cat > "$TMPDIR/orchestrate.json" << 'EOF'
{"providers":{"fake":{"command":"fake-provider"}}}
EOF

echo "TestOutput: 12 passed, 0 failed" > "$TMPDIR/evidence.txt"
echo "diff --git a/x b/x" > "$TMPDIR/diff.txt"

DISPATCH="$TMPDIR/hooks/dispatch-provider.sh"

echo "Test 1: --evidence-file appends ## Evidence section"
EXIT_CODE=0
OUTPUT=$("$DISPATCH" fake --role evaluator --topic t --instructions i \
  --evidence-file "$TMPDIR/evidence.txt" 2>&1) || EXIT_CODE=$?
assert_exit "dispatch with evidence → exit 0" 0 "$EXIT_CODE"
assert_contains "payload has ## Evidence header" "## Evidence" "$OUTPUT"
assert_contains "payload has evidence content" "TestOutput: 12 passed, 0 failed" "$OUTPUT"

echo "Test 2: omitting --evidence-file produces no ## Evidence section"
EXIT_CODE=0
OUTPUT=$("$DISPATCH" fake --role evaluator --topic t --instructions i 2>&1) || EXIT_CODE=$?
assert_exit "dispatch without evidence → exit 0" 0 "$EXIT_CODE"
assert_not_contains "payload has no ## Evidence header" "## Evidence" "$OUTPUT"

echo "Test 3: --diff-file behavior unchanged alongside --evidence-file"
EXIT_CODE=0
OUTPUT=$("$DISPATCH" fake --role evaluator --topic t --instructions i \
  --diff-file "$TMPDIR/diff.txt" --evidence-file "$TMPDIR/evidence.txt" 2>&1) || EXIT_CODE=$?
assert_exit "dispatch with diff and evidence → exit 0" 0 "$EXIT_CODE"
assert_contains "payload has ## Diff header" "## Diff" "$OUTPUT"
assert_contains "payload has diff content" "diff --git a/x b/x" "$OUTPUT"
assert_contains "payload has ## Evidence header" "## Evidence" "$OUTPUT"
assert_contains "payload has evidence content" "TestOutput: 12 passed, 0 failed" "$OUTPUT"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
