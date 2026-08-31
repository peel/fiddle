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

printf 'Evidence keeps A&B and C&&D intact\n' > "$TMPDIR/evidence.txt"
printf 'Diff keeps E&F and G&&H intact\n' > "$TMPDIR/diff.txt"
printf 'Feedback keeps I&J and K&&L intact\n' > "$TMPDIR/feedback.txt"

DISPATCH="$TMPDIR/hooks/dispatch-provider.sh"

echo "Test 1: --evidence-file appends ## Evidence section"
EXIT_CODE=0
OUTPUT=$("$DISPATCH" fake --role evaluator --topic t --instructions i \
  --evidence-file "$TMPDIR/evidence.txt" 2>&1) || EXIT_CODE=$?
assert_exit "dispatch with evidence → exit 0" 0 "$EXIT_CODE"
assert_contains "payload has ## Evidence header" "## Evidence" "$OUTPUT"
assert_contains "payload has evidence content" "Evidence keeps A&B and C&&D intact" "$OUTPUT"

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
assert_contains "payload has diff content" "Diff keeps E&F and G&&H intact" "$OUTPUT"
assert_contains "payload has ## Evidence header" "## Evidence" "$OUTPUT"
assert_contains "payload has evidence content" "Evidence keeps A&B and C&&D intact" "$OUTPUT"

echo "Test 4: prompt sections preserve literal ampersands"
EXIT_CODE=0
OUTPUT=$("$DISPATCH" fake --role evaluator --topic t \
  --instructions "Instructions keep M&N and O&&P intact" \
  --diff-file "$TMPDIR/diff.txt" \
  --evidence-file "$TMPDIR/evidence.txt" \
  --previous-feedback-file "$TMPDIR/feedback.txt" 2>&1) || EXIT_CODE=$?
assert_exit "dispatch with literal ampersands → exit 0" 0 "$EXIT_CODE"
assert_contains "diff ampersands preserved" "Diff keeps E&F and G&&H intact" "$OUTPUT"
assert_contains "evidence ampersands preserved" "Evidence keeps A&B and C&&D intact" "$OUTPUT"
assert_contains "instruction ampersands preserved" "Instructions keep M&N and O&&P intact" "$OUTPUT"
assert_contains "feedback ampersands preserved" "Feedback keeps I&J and K&&L intact" "$OUTPUT"

echo "Test 5: a payload's own '## ' headings survive the empty-section strip"
cat > "$TMPDIR/payload.md" << 'EOF'
# Evaluate

## Distrust the Implementer

Do not take the implementer's claims as evidence.

## Scorecard JSON Output

Return this JSON structure to stdout.

## Output Contract

Your entire stdout is valid JSON.
EOF
EXIT_CODE=0
OUTPUT=$("$DISPATCH" fake --role evaluator --topic t \
  --instructions "$(cat "$TMPDIR/payload.md")" 2>&1) || EXIT_CODE=$?
assert_exit "dispatch with a multi-line instruction payload → exit 0" 0 "$EXIT_CODE"
assert_contains "payload keeps ## Scorecard JSON Output" "## Scorecard JSON Output" "$OUTPUT"
assert_contains "payload keeps ## Output Contract" "## Output Contract" "$OUTPUT"
assert_contains "payload keeps ## Distrust the Implementer" "## Distrust the Implementer" "$OUTPUT"
assert_contains "payload keeps its body text" "Your entire stdout is valid JSON." "$OUTPUT"

echo "Test 6: a payload heading survives in --diff/--evidence/--feedback too"
printf '## Evidence Pack\n\nPack body line.\n' > "$TMPDIR/evidence-headed.txt"
EXIT_CODE=0
OUTPUT=$("$DISPATCH" fake --role evaluator --topic t --instructions i \
  --evidence-file "$TMPDIR/evidence-headed.txt" 2>&1) || EXIT_CODE=$?
assert_exit "dispatch with a headed evidence file → exit 0" 0 "$EXIT_CODE"
assert_contains "evidence keeps its own ## heading" "## Evidence Pack" "$OUTPUT"
assert_contains "evidence keeps its body" "Pack body line." "$OUTPUT"

echo "Test 7: an unfilled template section is still dropped"
EXIT_CODE=0
OUTPUT=$("$DISPATCH" fake --role evaluator --topic t --instructions i 2>&1) || EXIT_CODE=$?
assert_exit "dispatch without optional sections → exit 0" 0 "$EXIT_CODE"
assert_not_contains "no ## Diff header" "## Diff" "$OUTPUT"
assert_not_contains "no ## Approaches header" "## Approaches" "$OUTPUT"
assert_not_contains "no ## Previous Feedback header" "## Previous Feedback" "$OUTPUT"
assert_not_contains "no unsubstituted marker remains" "{DIFF}" "$OUTPUT"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
