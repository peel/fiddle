#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0; FAIL=0

assert_eq() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"
  fi
}

# Create a test bean
BEAN_ID=$(beans create "Test eval log" -t task -s in-progress --json 2>/dev/null | jq -r '.bean.id // .id')
trap "beans update $BEAN_ID -s scrapped 2>/dev/null || true" EXIT

echo "Test 1: Init eval log on bean"
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --init --base-sha "abc1234"
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "BASE_SHA: abc1234" && assert_eq "base_sha in body" "yes" "yes" || assert_eq "base_sha in body" "yes" "no"

echo "Test 2: Append iteration"
cat > /tmp/test-scorecard.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":7,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 1 --scorecard /tmp/test-scorecard.json --dispatches 1 --guidance "Looks good"
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "### Iteration 1" && assert_eq "iteration 1 in body" "yes" "yes" || assert_eq "iteration 1 in body" "yes" "no"
echo "$BODY" | grep -q "total_dispatches: 1" && assert_eq "total_dispatches updated" "yes" "yes" || assert_eq "total_dispatches updated" "yes" "no"

echo "Test 3: Parse eval log"
OUTPUT=$("$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$BEAN_ID")
assert_eq "base_sha parsed" "abc1234" "$(echo "$OUTPUT" | jq -r '.base_sha')"
assert_eq "iteration_count" "1" "$(echo "$OUTPUT" | jq -r '.iteration_count')"
assert_eq "total_dispatches" "1" "$(echo "$OUTPUT" | jq -r '.total_dispatches')"

echo "Test 4: Append second iteration with FAIL"
cat > /tmp/test-scorecard2.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":5,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 2 --scorecard /tmp/test-scorecard2.json --dispatches 2 --guidance "Needs improvement"
OUTPUT=$("$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$BEAN_ID")
assert_eq "iteration_count after 2nd" "2" "$(echo "$OUTPUT" | jq -r '.iteration_count')"
assert_eq "total_dispatches cumulative" "3" "$(echo "$OUTPUT" | jq -r '.total_dispatches')"
assert_eq "last_verdict is FAIL" "FAIL" "$(echo "$OUTPUT" | jq -r '.last_verdict')"
assert_eq "last_guidance" "Needs improvement" "$(echo "$OUTPUT" | jq -r '.last_guidance')"

echo "Test 5: Iteration with disagreements file"
cat > /tmp/test-disagreements.json << 'EOF'
[{"domain": "general", "dimension": "correctness", "spread": 3, "scores": {"claude": 9, "codex": 6}}]
EOF
cat > /tmp/test-scorecard3.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":6,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 3 --scorecard /tmp/test-scorecard3.json --dispatches 2 --guidance "" --disagreements /tmp/test-disagreements.json
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "Disagreements:" && assert_eq "disagreements section present" "yes" "yes" || assert_eq "disagreements section present" "yes" "no"
echo "$BODY" | grep -q "general\.correctness: spread 3" && assert_eq "disagreement detail correct" "yes" "yes" || assert_eq "disagreement detail correct" "yes" "no"
echo "$BODY" | grep -q "claude: 9" && assert_eq "provider score claude" "yes" "yes" || assert_eq "provider score claude" "yes" "no"
echo "$BODY" | grep -q "codex: 6" && assert_eq "provider score codex" "yes" "yes" || assert_eq "provider score codex" "yes" "no"

echo "Test 6: Iteration without disagreements (backward compatible)"
cat > /tmp/test-scorecard4.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 4 --scorecard /tmp/test-scorecard4.json --dispatches 1 --guidance ""
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
# Iteration 4 should NOT have a Disagreements section — count occurrences
DISAGREE_COUNT=$(echo "$BODY" | grep -c "Disagreements:" || true)
assert_eq "only one disagreements section (from iter 3)" "1" "$DISAGREE_COUNT"

echo "Test 7: Iteration with empty disagreements array"
cat > /tmp/test-disagreements-empty.json << 'EOF'
[]
EOF
cat > /tmp/test-scorecard5.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":9,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 5 --scorecard /tmp/test-scorecard5.json --dispatches 1 --guidance "" --disagreements /tmp/test-disagreements-empty.json
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
DISAGREE_COUNT=$(echo "$BODY" | grep -c "Disagreements:" || true)
assert_eq "still only one disagreements section (empty array ignored)" "1" "$DISAGREE_COUNT"

echo "Test 8: Missing --bean-id errors"
EXIT_CODE=0
"$SCRIPT_DIR/append-eval-log.sh" --init --base-sha "x" 2>/dev/null || EXIT_CODE=$?
assert_eq "append missing bean-id → exit 2" "2" "$EXIT_CODE"

EXIT_CODE=0
"$SCRIPT_DIR/parse-eval-log.sh" 2>/dev/null || EXIT_CODE=$?
assert_eq "parse missing bean-id → exit 2" "2" "$EXIT_CODE"

echo "Test 9: Iteration with antipatterns file"
cat > /tmp/test-antipatterns.json << 'EOF'
[{"id": "ap-interface-any", "evidence": "used interface{} instead of any"}]
EOF
cat > /tmp/test-scorecard6.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":6,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 6 --scorecard /tmp/test-scorecard6.json --dispatches 1 --guidance "" --antipatterns /tmp/test-antipatterns.json
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "Antipatterns detected:" && assert_eq "antipatterns section present" "yes" "yes" || assert_eq "antipatterns section present" "yes" "no"
echo "$BODY" | grep -q "ap-interface-any" && assert_eq "antipattern id present" "yes" "yes" || assert_eq "antipattern id present" "yes" "no"

echo "Test 10: Iteration without antipatterns (backward compatible)"
cat > /tmp/test-scorecard7.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 7 --scorecard /tmp/test-scorecard7.json --dispatches 1 --guidance ""
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
ANTIPATTERN_COUNT=$(echo "$BODY" | grep -c "Antipatterns detected:" || true)
assert_eq "only one antipatterns section (from iter 6)" "1" "$ANTIPATTERN_COUNT"

echo "Test 11: Iteration with empty antipatterns array"
cat > /tmp/test-antipatterns-empty.json << 'EOF'
[]
EOF
cat > /tmp/test-scorecard8.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":9,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 8 --scorecard /tmp/test-scorecard8.json --dispatches 1 --guidance "" --antipatterns /tmp/test-antipatterns-empty.json
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
ANTIPATTERN_COUNT=$(echo "$BODY" | grep -c "Antipatterns detected:" || true)
assert_eq "still only one antipatterns section (empty array ignored)" "1" "$ANTIPATTERN_COUNT"

echo "Test 12: Iteration with antipatterns alongside corrections"
cat > /tmp/test-antipatterns2.json << 'EOF'
[{"id": "ap-dead-code", "evidence": "unused helper retained"}]
EOF
cat > /tmp/test-corrections.json << 'EOF'
[{"domain": "general", "dimension": "correctness", "evaluator_score": 5, "human_score": 8, "reason": "false positive"}]
EOF
cat > /tmp/test-scorecard9.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":5,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 9 --scorecard /tmp/test-scorecard9.json --dispatches 1 --guidance "" --corrections /tmp/test-corrections.json --antipatterns /tmp/test-antipatterns2.json
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "ap-dead-code" && assert_eq "antipattern id present alongside corrections" "yes" "yes" || assert_eq "antipattern id present alongside corrections" "yes" "no"
echo "$BODY" | grep -q "Human Corrections:" && assert_eq "corrections section present alongside antipatterns" "yes" "yes" || assert_eq "corrections section present alongside antipatterns" "yes" "no"

echo ""
echo "Results: $PASS passed, $FAIL failed"
rm -f /tmp/test-scorecard.json /tmp/test-scorecard2.json /tmp/test-scorecard3.json /tmp/test-scorecard4.json /tmp/test-scorecard5.json /tmp/test-scorecard6.json /tmp/test-scorecard7.json /tmp/test-scorecard8.json /tmp/test-scorecard9.json /tmp/test-disagreements.json /tmp/test-disagreements-empty.json /tmp/test-antipatterns.json /tmp/test-antipatterns-empty.json /tmp/test-antipatterns2.json /tmp/test-corrections.json
[ "$FAIL" -eq 0 ] || exit 1
