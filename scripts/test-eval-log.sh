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

for TOOL in beans jq; do
  command -v "$TOOL" >/dev/null 2>&1 || { echo "SKIP: test-eval-log.sh needs $TOOL on the PATH, and $TOOL is absent"; exit 0; }
done

WORK=$(mktemp -d "${TMPDIR:-/tmp}/test-eval-log-XXXXXX") || { echo "FAIL: test-eval-log.sh cannot make a temporary directory"; exit 2; }
trap 'rm -rf "$WORK"' EXIT INT TERM

export BEANS_PATH="$WORK/store"
mkdir -p "$BEANS_PATH"
beans init --beans-path "$BEANS_PATH" >/dev/null 2>&1 || { echo "FAIL: beans init cannot write the store at $BEANS_PATH"; exit 2; }

BEAN_ID=$(beans create "Test eval log" -t task -s in-progress --json 2>/dev/null | jq -r '.bean.id // .id')
[ -n "$BEAN_ID" ] && [ "$BEAN_ID" != "null" ] || { echo "FAIL: beans create returned no id in $BEANS_PATH"; exit 2; }

echo "Test 1: Init eval log on bean"
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --init --base-sha "abc1234"
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "BASE_SHA: abc1234" && assert_eq "base_sha in body" "yes" "yes" || assert_eq "base_sha in body" "yes" "no"

echo "Test 2: Append iteration"
cat > "$WORK/scorecard.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":7,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 1 --scorecard "$WORK/scorecard.json" --dispatches 1 --guidance "Looks good"
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "### Iteration 1" && assert_eq "iteration 1 in body" "yes" "yes" || assert_eq "iteration 1 in body" "yes" "no"
echo "$BODY" | grep -q "total_dispatches: 1" && assert_eq "total_dispatches updated" "yes" "yes" || assert_eq "total_dispatches updated" "yes" "no"

echo "Test 3: Parse eval log"
OUTPUT=$("$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$BEAN_ID")
assert_eq "base_sha parsed" "abc1234" "$(echo "$OUTPUT" | jq -r '.base_sha')"
assert_eq "iteration_count" "1" "$(echo "$OUTPUT" | jq -r '.iteration_count')"
assert_eq "total_dispatches" "1" "$(echo "$OUTPUT" | jq -r '.total_dispatches')"

echo "Test 4: Append second iteration with FAIL"
cat > "$WORK/scorecard2.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":5,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 2 --scorecard "$WORK/scorecard2.json" --dispatches 2 --guidance "Needs improvement"
OUTPUT=$("$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$BEAN_ID")
assert_eq "iteration_count after 2nd" "2" "$(echo "$OUTPUT" | jq -r '.iteration_count')"
assert_eq "total_dispatches cumulative" "3" "$(echo "$OUTPUT" | jq -r '.total_dispatches')"
assert_eq "last_verdict is FAIL" "FAIL" "$(echo "$OUTPUT" | jq -r '.last_verdict')"
assert_eq "last_guidance" "Needs improvement" "$(echo "$OUTPUT" | jq -r '.last_guidance')"

echo "Test 5: Iteration with disagreements file"
cat > "$WORK/disagreements.json" << 'EOF'
[{"domain": "general", "dimension": "correctness", "spread": 3, "scores": {"claude": 9, "codex": 6}}]
EOF
cat > "$WORK/scorecard3.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":6,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 3 --scorecard "$WORK/scorecard3.json" --dispatches 2 --guidance "" --disagreements "$WORK/disagreements.json"
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "Disagreements:" && assert_eq "disagreements section present" "yes" "yes" || assert_eq "disagreements section present" "yes" "no"
echo "$BODY" | grep -q "general\.correctness: spread 3" && assert_eq "disagreement detail correct" "yes" "yes" || assert_eq "disagreement detail correct" "yes" "no"
echo "$BODY" | grep -q "claude: 9" && assert_eq "provider score claude" "yes" "yes" || assert_eq "provider score claude" "yes" "no"
echo "$BODY" | grep -q "codex: 6" && assert_eq "provider score codex" "yes" "yes" || assert_eq "provider score codex" "yes" "no"

echo "Test 6: Iteration without disagreements (backward compatible)"
cat > "$WORK/scorecard4.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 4 --scorecard "$WORK/scorecard4.json" --dispatches 1 --guidance ""
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
DISAGREE_COUNT=$(echo "$BODY" | grep -c "Disagreements:" || true)
assert_eq "only one disagreements section (from iter 3)" "1" "$DISAGREE_COUNT"

echo "Test 7: Iteration with empty disagreements array"
cat > "$WORK/disagreements-empty.json" << 'EOF'
[]
EOF
cat > "$WORK/scorecard5.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":9,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 5 --scorecard "$WORK/scorecard5.json" --dispatches 1 --guidance "" --disagreements "$WORK/disagreements-empty.json"
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
cat > "$WORK/antipatterns.json" << 'EOF'
[{"id": "ap-interface-any", "evidence": "used interface{} instead of any"}]
EOF
cat > "$WORK/scorecard6.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":6,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 6 --scorecard "$WORK/scorecard6.json" --dispatches 1 --guidance "" --antipatterns "$WORK/antipatterns.json"
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "Antipatterns detected:" && assert_eq "antipatterns section present" "yes" "yes" || assert_eq "antipatterns section present" "yes" "no"
echo "$BODY" | grep -q "ap-interface-any" && assert_eq "antipattern id present" "yes" "yes" || assert_eq "antipattern id present" "yes" "no"

echo "Test 10: Iteration without antipatterns (backward compatible)"
cat > "$WORK/scorecard7.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 7 --scorecard "$WORK/scorecard7.json" --dispatches 1 --guidance ""
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
ANTIPATTERN_COUNT=$(echo "$BODY" | grep -c "Antipatterns detected:" || true)
assert_eq "only one antipatterns section (from iter 6)" "1" "$ANTIPATTERN_COUNT"

echo "Test 11: Iteration with empty antipatterns array"
cat > "$WORK/antipatterns-empty.json" << 'EOF'
[]
EOF
cat > "$WORK/scorecard8.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":9,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 8 --scorecard "$WORK/scorecard8.json" --dispatches 1 --guidance "" --antipatterns "$WORK/antipatterns-empty.json"
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
ANTIPATTERN_COUNT=$(echo "$BODY" | grep -c "Antipatterns detected:" || true)
assert_eq "still only one antipatterns section (empty array ignored)" "1" "$ANTIPATTERN_COUNT"

echo "Test 12: Iteration with antipatterns alongside corrections"
cat > "$WORK/antipatterns2.json" << 'EOF'
[{"id": "ap-dead-code", "evidence": "unused helper retained"}]
EOF
cat > "$WORK/corrections.json" << 'EOF'
[{"domain": "general", "dimension": "correctness", "evaluator_score": 5, "human_score": 8, "reason": "false positive"}]
EOF
cat > "$WORK/scorecard9.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":5,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --iteration 9 --scorecard "$WORK/scorecard9.json" --dispatches 1 --guidance "" --corrections "$WORK/corrections.json" --antipatterns "$WORK/antipatterns2.json"
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "ap-dead-code" && assert_eq "antipattern id present alongside corrections" "yes" "yes" || assert_eq "antipattern id present alongside corrections" "yes" "no"
echo "$BODY" | grep -q "Human Corrections:" && assert_eq "corrections section present alongside antipatterns" "yes" "yes" || assert_eq "corrections section present alongside antipatterns" "yes" "no"

echo "Test 13: Spot-check entry uses Spot-Check heading with Human Corrections"
DISPATCHES_BEFORE=$("$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$BEAN_ID" | jq -r '.total_dispatches')
ITER_BEFORE=$("$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$BEAN_ID" | jq -r '.iteration_count')
cat > "$WORK/spotcheck-corrections.json" << 'EOF'
[{"domain": "general", "dimension": "correctness", "evaluator_score": 8, "human_score": 5, "reason": "blind spot-check: missed error path"}]
EOF
cat > "$WORK/scorecard-sc.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --spot-check --scorecard "$WORK/scorecard-sc.json" --guidance "blind spot-check" --corrections "$WORK/spotcheck-corrections.json"
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "### Spot-Check (" && assert_eq "spot-check heading present" "yes" "yes" || assert_eq "spot-check heading present" "yes" "no"
echo "$BODY" | grep -q "Human Corrections:" && assert_eq "spot-check human corrections present" "yes" "yes" || assert_eq "spot-check human corrections present" "yes" "no"

echo "Test 14: Spot-check does not add an iteration and requires no --iteration"
OUTPUT=$("$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$BEAN_ID")
assert_eq "iteration_count unchanged after spot-check" "$ITER_BEFORE" "$(echo "$OUTPUT" | jq -r '.iteration_count')"
assert_eq "total_dispatches uncorrupted by spot-check (default 0)" "$DISPATCHES_BEFORE" "$(echo "$OUTPUT" | jq -r '.total_dispatches')"

echo "Test 15: Regular mode still requires --iteration"
EXIT_CODE=0
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --scorecard "$WORK/scorecard-sc.json" --dispatches 1 2>/dev/null || EXIT_CODE=$?
assert_eq "regular mode missing --iteration → exit 2" "2" "$EXIT_CODE"

echo "Test 16: Spot-check preserves trend iteration count and final-iteration dimensions"
FIXT=$(mktemp -d "$WORK/fixture-XXXXXX")
beans init --beans-path "$FIXT" >/dev/null 2>&1
FE=$(beans create "Fixture Epic" --beans-path "$FIXT" -t epic -s completed --json 2>/dev/null | jq -r '.bean.id // .id')
FT=$(beans create "Fixture Task" --beans-path "$FIXT" -t task -s completed --parent "$FE" --json 2>/dev/null | jq -r '.bean.id // .id')
FEF=$(ls "$FIXT"/${FE}*.md); sed -i.bak "s/^created_at: .*/created_at: 2026-01-01T00:00:00Z/" "$FEF" && rm -f "$FEF.bak"
cat > "$WORK/fixt-i1.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":9,"threshold":7}}}},"criteria":[]}
EOF
cat > "$WORK/fixt-i2.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":6,"threshold":7}}}},"criteria":[]}
EOF
cat > "$WORK/fixt-sc.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":2,"threshold":7}}}},"criteria":[]}
EOF
cat > "$WORK/fixt-corr.json" << 'EOF'
[{"domain": "general", "dimension": "correctness", "evaluator_score": 6, "human_score": 2, "reason": "blind spot-check"}]
EOF
BEANS_PATH="$FIXT" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$FT" --init --base-sha fsha
BEANS_PATH="$FIXT" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$FT" --iteration 1 --scorecard "$WORK/fixt-i1.json" --dispatches 2 --guidance ""
BEANS_PATH="$FIXT" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$FT" --iteration 2 --scorecard "$WORK/fixt-i2.json" --dispatches 2 --guidance ""
BEANS_PATH="$FIXT" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$FT" --spot-check --scorecard "$WORK/fixt-sc.json" --guidance "blind spot-check" --corrections "$WORK/fixt-corr.json"
FIXT_PARSE=$(BEANS_PATH="$FIXT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$FT")
assert_eq "fixture parse iteration_count stays 2" "2" "$(echo "$FIXT_PARSE" | jq -r '.iteration_count')"
FIXT_TREND=$("$SCRIPT_DIR/trend-eval-history.sh" --beans-path "$FIXT")
assert_eq "fixture trend iterations mean is 2" "2" "$(echo "$FIXT_TREND" | jq -r '.epics[0].iterations.mean')"
assert_eq "fixture trend correctness from iter 2 not spot-check" "6" "$(echo "$FIXT_TREND" | jq -r '.epics[0].dimensions.correctness')"
rm -rf "$FIXT"


ug_bean() {
  UGT=$(mktemp -d "$WORK/ug-XXXXXX")
  beans init --beans-path "$UGT" >/dev/null 2>&1
  UG=$(beans create "Ungradeable log" --beans-path "$UGT" -t task -s in-progress --json 2>/dev/null | jq -r '.bean.id // .id')
  BEANS_PATH="$UGT" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$UG" --init --base-sha ugsha
}
ug_log() { BEANS_PATH="$UGT" "$SCRIPT_DIR/append-eval-log.sh" "$@"; }
ug_body() { beans show "$UG" --beans-path "$UGT" --json 2>/dev/null | jq -r '.body'; }
ug_dim() { ug_body | grep -e "^- $1:" | tail -1; }
ug_verdict() { BEANS_PATH="$UGT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$UG" | jq -r '.last_verdict'; }

echo "Test 17: a threshold-less dimension is ungraded in the log, not a bare score"
ug_bean
cat > "$WORK/ug-nothreshold.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":1},"code_quality":{"score":1,"threshold":6}}}},"criteria":[]}
EOF
ug_log --bean-id "$UG" --iteration 1 --scorecard "$WORK/ug-nothreshold.json" --dispatches 1 --guidance ""
assert_eq "threshold-less dimension names the missing threshold" \
  "- correctness: 1/10 (UNGRADED, no threshold recorded)" "$(ug_dim correctness)"
assert_eq "a graded dimension in the same entry still reads FAIL" \
  "- code_quality: 1/10 (FAIL, threshold 6)" "$(ug_dim code_quality)"
assert_eq "an entry with an ungraded dimension does not parse as PASS" "UNGRADED" "$(ug_verdict)"
rm -rf "$UGT"

echo "Test 18: a non-numeric score or threshold is ungraded, naming the type"
ug_bean
cat > "$WORK/ug-types.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":"1","threshold":7},"code_quality":{"score":8,"threshold":"6"},"ux":{"threshold":7}}}},"criteria":[]}
EOF
ug_log --bean-id "$UG" --iteration 1 --scorecard "$WORK/ug-types.json" --dispatches 1 --guidance ""
assert_eq "stringly-typed score named by type" \
  '- correctness: 1/10 (UNGRADED, score is string, not a number)' "$(ug_dim correctness)"
assert_eq "stringly-typed threshold named by type" \
  '- code_quality: 8/10 (UNGRADED, threshold is string, not a number)' "$(ug_dim code_quality)"
assert_eq "absent score named as missing" \
  '- ux: null/10 (UNGRADED, no score recorded)' "$(ug_dim ux)"
rm -rf "$UGT"

echo "Test 19: well-formed dimensions render exactly as before, byte for byte"
ug_bean
cat > "$WORK/ug-wf1.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[]}
EOF
cat > "$WORK/ug-wf2.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":5,"threshold":7}}}},"criteria":[]}
EOF
ug_log --bean-id "$UG" --iteration 1 --scorecard "$WORK/ug-wf1.json" --dispatches 1 --guidance ""
assert_eq "a passing dimension carries no annotation at all" "- correctness: 8/10" "$(ug_dim correctness)"
assert_eq "a passing entry parses as PASS" "PASS" "$(ug_verdict)"
ug_log --bean-id "$UG" --iteration 2 --scorecard "$WORK/ug-wf2.json" --dispatches 1 --guidance ""
assert_eq "a failing dimension keeps the (FAIL, threshold N) form" "- correctness: 5/10 (FAIL, threshold 7)" "$(ug_dim correctness)"
assert_eq "a failing entry still parses as FAIL" "FAIL" "$(ug_verdict)"
rm -rf "$UGT"

echo "Test 20: the SPEC_DEFECT route can still log a mis-shaped scorecard"
ug_bean
cat > "$WORK/ug-topkey.json" << 'EOF'
{"general":{"dimensions":{"correctness":{"score":1,"threshold":7}}},"criteria":[]}
EOF
cat > "$WORK/ug-nested.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":1,"threshold":7}}},"criteria":[{"id":"a"}]},"criteria":[]}
EOF
cat > "$WORK/ug-flatdim.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":7}}},"criteria":[]}
EOF
EXIT_CODE=0
OUT=$(ug_log --bean-id "$UG" --iteration 1 --scorecard "$WORK/ug-topkey.json" --dispatches 1 --guidance "" 2>&1) || EXIT_CODE=$?
assert_eq "a top-level domain key logs rather than aborting" "0" "$EXIT_CODE"
assert_eq "and says the domains key was missing" \
  '- domains (UNGRADED, no `domains` recorded)' "$(ug_body | grep -e '^- domains' | tail -1)"
EXIT_CODE=0
OUT=$(ug_log --bean-id "$UG" --iteration 2 --scorecard "$WORK/ug-nested.json" --dispatches 1 --guidance "" 2>&1) || EXIT_CODE=$?
assert_eq "criteria mis-nested under .domains logs rather than aborting" "0" "$EXIT_CODE"
assert_eq "and says that domain had no dimensions" \
  '- dimensions (UNGRADED, no `dimensions` recorded)' "$(ug_body | grep -e '^- dimensions' | tail -1)"
EXIT_CODE=0
OUT=$(ug_log --bean-id "$UG" --iteration 3 --scorecard "$WORK/ug-flatdim.json" --dispatches 1 --guidance "" 2>&1) || EXIT_CODE=$?
assert_eq "a dimension that is a bare number logs rather than aborting" "0" "$EXIT_CODE"
assert_eq "and names what it got instead of an object" \
  '- correctness: 7/10 (UNGRADED, dimension is number, not an object)' "$(ug_dim correctness)"
assert_eq "three mis-shaped entries, three iterations recorded" "3" \
  "$(BEANS_PATH="$UGT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$UG" | jq -r '.iteration_count')"
rm -rf "$UGT"

echo "Test 21: entries written before the annotation existed still parse"
ug_bean
beans update "$UG" --beans-path "$UGT" --body-append "$(printf '### Iteration 1 (2026-08-19T00:00:00Z)\ndispatches: 2\n**general:**\n- correctness: 9/10\n- code_quality: 5/10 (FAIL, threshold 6)\n**Guidance:** "old entry"')" >/dev/null 2>&1
OUTPUT=$(BEANS_PATH="$UGT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$UG")
assert_eq "pre-annotation base_sha still read" "ugsha" "$(echo "$OUTPUT" | jq -r '.base_sha')"
assert_eq "pre-annotation iteration counted" "1" "$(echo "$OUTPUT" | jq -r '.iteration_count')"
assert_eq "pre-annotation FAIL marker still wins" "FAIL" "$(echo "$OUTPUT" | jq -r '.last_verdict')"
assert_eq "pre-annotation guidance still read" "old entry" "$(echo "$OUTPUT" | jq -r '.last_guidance')"
rm -rf "$UGT"

echo "Test 22: a scorecard that cannot be read still leaves an entry, not silence"
ug_bean
: > "$WORK/ug-empty.json"
printf '{"domains":{"general":' > "$WORK/ug-truncated.json"
EXIT_CODE=0
ERR=$(ug_log --bean-id "$UG" --iteration 1 --scorecard "$WORK/ug-empty.json" --dispatches 1 --guidance "SPEC_DEFECT: unreadable" 2>&1) || EXIT_CODE=$?
assert_eq "an empty scorecard still logs" "0" "$EXIT_CODE"
assert_eq "and the entry says the card could not be read" \
  "**scorecard:** (UNGRADED, could not be read: $WORK/ug-empty.json)" "$(ug_body | grep -e '^\*\*scorecard:\*\*' | tail -1)"
assert_eq "and the iteration is on the record" "1" \
  "$(BEANS_PATH="$UGT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$UG" | jq -r '.iteration_count')"
assert_eq "and it does not parse as PASS" "UNGRADED" "$(ug_verdict)"
assert_eq "and the guidance survives" "SPEC_DEFECT: unreadable" \
  "$(BEANS_PATH="$UGT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$UG" | jq -r '.last_guidance')"
case "$ERR" in *"could not read scorecard"*) assert_eq "and the caller is told on stderr" "yes" "yes";;
  *) assert_eq "and the caller is told on stderr" "yes" "no (stderr was '$ERR')";; esac
EXIT_CODE=0
ERR=$(ug_log --bean-id "$UG" --iteration 2 --scorecard "$WORK/ug-truncated.json" --dispatches 1 --guidance "" 2>&1) || EXIT_CODE=$?
assert_eq "a truncated scorecard still logs" "0" "$EXIT_CODE"
assert_eq "two unreadable cards, two iterations recorded" "2" \
  "$(BEANS_PATH="$UGT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$UG" | jq -r '.iteration_count')"
rm -rf "$UGT"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
