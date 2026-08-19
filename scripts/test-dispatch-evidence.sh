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
# Every real payload is a markdown document with headings of its own. The
# stripper exists to drop unfilled *template* sections; scanning the assembled
# prompt made it delete the injected document's headings instead — including
# the two that define the scorecard schema.
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

echo "Test 8: a JSONL event stream is extracted to the agent message"
# codex exec --json emits JSONL; the reply is the text of the last completed
# agent_message item, JSON-escaped inside the event. Forwarding the raw stream
# leaves the caller hand-extracting an escaped object out of JSONL.
cat > "$TMPDIR/bin/fake-jsonl" << 'EOF'
#!/usr/bin/env bash
cat > /dev/null
printf '%s\n' '{"type":"thread.started","thread_id":"t1"}'
printf '%s\n' 'not json at all'
printf '%s\n' '{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"thinking out loud"}}'
printf '%s\n' '{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"{\"provider\":\"codex\",\"criteria\":[{\"id\":\"c1\",\"pass\":true}]}"}}'
printf '%s\n' '{"type":"turn.completed","usage":{"output_tokens":14}}'
EOF
chmod +x "$TMPDIR/bin/fake-jsonl"
cat > "$TMPDIR/orchestrate.json" << 'EOF'
{"providers":{"fake":{"command":"fake-provider"},
              "streamer":{"command":"fake-jsonl","extract":"codex-jsonl"}}}
EOF
EXIT_CODE=0
OUTPUT=$("$DISPATCH" streamer --role evaluator --topic t --instructions i 2>/dev/null) || EXIT_CODE=$?
assert_exit "dispatch against a JSONL provider → exit 0" 0 "$EXIT_CODE"
assert_contains "extracted text is the scorecard" '"provider":"codex"' "$OUTPUT"
assert_not_contains "no event envelope survives" "thread.started" "$OUTPUT"
assert_not_contains "no reasoning item survives" "thinking out loud" "$OUTPUT"
if echo "$OUTPUT" | jq -e '.criteria[0].id == "c1"' >/dev/null 2>&1; then
  PASS=$((PASS+1)); echo "  PASS: extracted output parses as the scorecard JSON"
else
  FAIL=$((FAIL+1)); echo "  FAIL: extracted output parses as the scorecard JSON"
fi

echo "Test 9: a stream with no agent message fails loudly"
cat > "$TMPDIR/bin/fake-silent" << 'EOF'
#!/usr/bin/env bash
cat > /dev/null
printf '%s\n' '{"type":"thread.started","thread_id":"t1"}'
printf '%s\n' '{"type":"turn.failed","error":"rate limited"}'
EOF
chmod +x "$TMPDIR/bin/fake-silent"
cat > "$TMPDIR/orchestrate.json" << 'EOF'
{"providers":{"silent":{"command":"fake-silent","extract":"codex-jsonl"}}}
EOF
EXIT_CODE=0
STDERR=$("$DISPATCH" silent --role evaluator --topic t --instructions i 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "empty extraction → non-zero exit" 1 "$EXIT_CODE"
assert_contains "raw stream reaches stderr for diagnosis" "turn.failed" "$STDERR"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
