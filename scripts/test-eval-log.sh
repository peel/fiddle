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

echo "Test 13: Spot-check entry uses Spot-Check heading with Human Corrections"
DISPATCHES_BEFORE=$("$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$BEAN_ID" | jq -r '.total_dispatches')
ITER_BEFORE=$("$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$BEAN_ID" | jq -r '.iteration_count')
cat > /tmp/test-spotcheck-corrections.json << 'EOF'
[{"domain": "general", "dimension": "correctness", "evaluator_score": 8, "human_score": 5, "reason": "blind spot-check: missed error path"}]
EOF
cat > /tmp/test-scorecard-sc.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[]}
EOF
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --spot-check --scorecard /tmp/test-scorecard-sc.json --guidance "blind spot-check" --corrections /tmp/test-spotcheck-corrections.json
BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body')
echo "$BODY" | grep -q "### Spot-Check (" && assert_eq "spot-check heading present" "yes" "yes" || assert_eq "spot-check heading present" "yes" "no"
echo "$BODY" | grep -q "Human Corrections:" && assert_eq "spot-check human corrections present" "yes" "yes" || assert_eq "spot-check human corrections present" "yes" "no"

echo "Test 14: Spot-check does not add an iteration and requires no --iteration"
OUTPUT=$("$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$BEAN_ID")
assert_eq "iteration_count unchanged after spot-check" "$ITER_BEFORE" "$(echo "$OUTPUT" | jq -r '.iteration_count')"
assert_eq "total_dispatches uncorrupted by spot-check (default 0)" "$DISPATCHES_BEFORE" "$(echo "$OUTPUT" | jq -r '.total_dispatches')"

echo "Test 15: Regular mode still requires --iteration"
EXIT_CODE=0
"$SCRIPT_DIR/append-eval-log.sh" --bean-id "$BEAN_ID" --scorecard /tmp/test-scorecard-sc.json --dispatches 1 2>/dev/null || EXIT_CODE=$?
assert_eq "regular mode missing --iteration → exit 2" "2" "$EXIT_CODE"

echo "Test 16: Spot-check preserves trend iteration count and final-iteration dimensions"
FIXT=$(mktemp -d)
beans init --beans-path "$FIXT" >/dev/null 2>&1
FE=$(beans create "Fixture Epic" --beans-path "$FIXT" -t epic -s completed --json 2>/dev/null | jq -r '.bean.id // .id')
FT=$(beans create "Fixture Task" --beans-path "$FIXT" -t task -s completed --parent "$FE" --json 2>/dev/null | jq -r '.bean.id // .id')
FEF=$(ls "$FIXT"/${FE}*.md); sed -i.bak "s/^created_at: .*/created_at: 2026-01-01T00:00:00Z/" "$FEF" && rm -f "$FEF.bak"
cat > /tmp/test-fixt-i1.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":9,"threshold":7}}}},"criteria":[]}
EOF
cat > /tmp/test-fixt-i2.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":6,"threshold":7}}}},"criteria":[]}
EOF
cat > /tmp/test-fixt-sc.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":2,"threshold":7}}}},"criteria":[]}
EOF
cat > /tmp/test-fixt-corr.json << 'EOF'
[{"domain": "general", "dimension": "correctness", "evaluator_score": 6, "human_score": 2, "reason": "blind spot-check"}]
EOF
BEANS_PATH="$FIXT" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$FT" --init --base-sha fsha
BEANS_PATH="$FIXT" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$FT" --iteration 1 --scorecard /tmp/test-fixt-i1.json --dispatches 2 --guidance ""
BEANS_PATH="$FIXT" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$FT" --iteration 2 --scorecard /tmp/test-fixt-i2.json --dispatches 2 --guidance ""
BEANS_PATH="$FIXT" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$FT" --spot-check --scorecard /tmp/test-fixt-sc.json --guidance "blind spot-check" --corrections /tmp/test-fixt-corr.json
FIXT_PARSE=$(BEANS_PATH="$FIXT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$FT")
assert_eq "fixture parse iteration_count stays 2" "2" "$(echo "$FIXT_PARSE" | jq -r '.iteration_count')"
FIXT_TREND=$("$SCRIPT_DIR/trend-eval-history.sh" --beans-path "$FIXT")
assert_eq "fixture trend iterations mean is 2" "2" "$(echo "$FIXT_TREND" | jq -r '.epics[0].iterations.mean')"
assert_eq "fixture trend correctness from iter 2 not spot-check" "6" "$(echo "$FIXT_TREND" | jq -r '.epics[0].dimensions.correctness')"
rm -rf "$FIXT"

# --- The durable record must not read as a clean sheet -----------------------
#
# `append-eval-log.sh` used to write its FAIL annotation with the same
# `score < threshold` comparison `check-thresholds.sh` was fixed for, and jq
# makes `1 < null` false and `"1" < 7` false too, so a dimension it could not
# compare rendered as a bare `1/10`. The log decides nothing, so these assert a
# rendering rather than a gate — nothing below refuses, because the SPEC_DEFECT
# route must be able to log before it routes. They matter because the log is the
# only loop state that survives a restart, and a record that cannot tell "clean"
# from "never compared" is the original defect one artifact along.

ug_bean() {  # a throwaway bean store, so these assertions own their whole body
  UGT=$(mktemp -d)
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
cat > /tmp/test-ug-nothreshold.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":1},"code_quality":{"score":1,"threshold":6}}}},"criteria":[]}
EOF
ug_log --bean-id "$UG" --iteration 1 --scorecard /tmp/test-ug-nothreshold.json --dispatches 1 --guidance ""
assert_eq "threshold-less dimension names the missing threshold" \
  "- correctness: 1/10 (UNGRADED, no threshold recorded)" "$(ug_dim correctness)"
assert_eq "a graded dimension in the same entry still reads FAIL" \
  "- code_quality: 1/10 (FAIL, threshold 6)" "$(ug_dim code_quality)"
assert_eq "an entry with an ungraded dimension does not parse as PASS" "UNGRADED" "$(ug_verdict)"
rm -rf "$UGT"

echo "Test 18: a non-numeric score or threshold is ungraded, naming the type"
ug_bean
cat > /tmp/test-ug-types.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":"1","threshold":7},"code_quality":{"score":8,"threshold":"6"},"ux":{"threshold":7}}}},"criteria":[]}
EOF
ug_log --bean-id "$UG" --iteration 1 --scorecard /tmp/test-ug-types.json --dispatches 1 --guidance ""
assert_eq "stringly-typed score named by type" \
  '- correctness: 1/10 (UNGRADED, score is string, not a number)' "$(ug_dim correctness)"
assert_eq "stringly-typed threshold named by type" \
  '- code_quality: 8/10 (UNGRADED, threshold is string, not a number)' "$(ug_dim code_quality)"
assert_eq "absent score named as missing" \
  '- ux: null/10 (UNGRADED, no score recorded)' "$(ug_dim ux)"
rm -rf "$UGT"

echo "Test 19: well-formed dimensions render exactly as before, byte for byte"
ug_bean
cat > /tmp/test-ug-wf1.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[]}
EOF
cat > /tmp/test-ug-wf2.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":5,"threshold":7}}}},"criteria":[]}
EOF
ug_log --bean-id "$UG" --iteration 1 --scorecard /tmp/test-ug-wf1.json --dispatches 1 --guidance ""
assert_eq "a passing dimension carries no annotation at all" "- correctness: 8/10" "$(ug_dim correctness)"
assert_eq "a passing entry parses as PASS" "PASS" "$(ug_verdict)"
ug_log --bean-id "$UG" --iteration 2 --scorecard /tmp/test-ug-wf2.json --dispatches 1 --guidance ""
assert_eq "a failing dimension keeps the (FAIL, threshold N) form" "- correctness: 5/10 (FAIL, threshold 7)" "$(ug_dim correctness)"
assert_eq "a failing entry still parses as FAIL" "FAIL" "$(ug_verdict)"
rm -rf "$UGT"

echo "Test 20: the SPEC_DEFECT route can still log a mis-shaped scorecard"
# `scorecard-merge.md` requires this path to log *before* routing, and the two
# envelope shapes this epic actually produced used to abort the logger with a raw
# jq error and exit 5 — no entry written at all, which is a worse record than a
# bare score. They must produce an entry that says what could not be read.
ug_bean
cat > /tmp/test-ug-topkey.json << 'EOF'
{"general":{"dimensions":{"correctness":{"score":1,"threshold":7}}},"criteria":[]}
EOF
cat > /tmp/test-ug-nested.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":1,"threshold":7}}},"criteria":[{"id":"a"}]},"criteria":[]}
EOF
cat > /tmp/test-ug-flatdim.json << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":7}}},"criteria":[]}
EOF
EXIT_CODE=0
OUT=$(ug_log --bean-id "$UG" --iteration 1 --scorecard /tmp/test-ug-topkey.json --dispatches 1 --guidance "" 2>&1) || EXIT_CODE=$?
assert_eq "a top-level domain key logs rather than aborting" "0" "$EXIT_CODE"
assert_eq "and says the domains key was missing" \
  '- domains (UNGRADED, no `domains` recorded)' "$(ug_body | grep -e '^- domains' | tail -1)"
EXIT_CODE=0
OUT=$(ug_log --bean-id "$UG" --iteration 2 --scorecard /tmp/test-ug-nested.json --dispatches 1 --guidance "" 2>&1) || EXIT_CODE=$?
assert_eq "criteria mis-nested under .domains logs rather than aborting" "0" "$EXIT_CODE"
assert_eq "and says that domain had no dimensions" \
  '- dimensions (UNGRADED, no `dimensions` recorded)' "$(ug_body | grep -e '^- dimensions' | tail -1)"
EXIT_CODE=0
OUT=$(ug_log --bean-id "$UG" --iteration 3 --scorecard /tmp/test-ug-flatdim.json --dispatches 1 --guidance "" 2>&1) || EXIT_CODE=$?
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
rm -f /tmp/test-ug-nothreshold.json /tmp/test-ug-types.json /tmp/test-ug-wf1.json /tmp/test-ug-wf2.json /tmp/test-ug-topkey.json /tmp/test-ug-nested.json /tmp/test-ug-flatdim.json

echo "Test 22: a scorecard that cannot be read still leaves an entry, not silence"
# merge-scorecards.sh exits 5 with ZERO bytes on stdout when its jq dies (its
# `2>/dev/null` swallows the reason), so the SPEC_DEFECT route can reach 1l
# holding an empty file. That used to append an empty string: exit 0, no entry,
# and a restart reading the log would see the iteration had never happened. It
# does not refuse — the route must log — it records that the card was unreadable
# and says so on stderr.
ug_bean
: > /tmp/test-ug-empty.json
printf '{"domains":{"general":' > /tmp/test-ug-truncated.json
EXIT_CODE=0
ERR=$(ug_log --bean-id "$UG" --iteration 1 --scorecard /tmp/test-ug-empty.json --dispatches 1 --guidance "SPEC_DEFECT: unreadable" 2>&1) || EXIT_CODE=$?
assert_eq "an empty scorecard still logs" "0" "$EXIT_CODE"
assert_eq "and the entry says the card could not be read" \
  '**scorecard:** (UNGRADED, could not be read: /tmp/test-ug-empty.json)' "$(ug_body | grep -e '^\*\*scorecard:\*\*' | tail -1)"
assert_eq "and the iteration is on the record" "1" \
  "$(BEANS_PATH="$UGT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$UG" | jq -r '.iteration_count')"
assert_eq "and it does not parse as PASS" "UNGRADED" "$(ug_verdict)"
assert_eq "and the guidance survives" "SPEC_DEFECT: unreadable" \
  "$(BEANS_PATH="$UGT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$UG" | jq -r '.last_guidance')"
case "$ERR" in *"could not read scorecard"*) assert_eq "and the caller is told on stderr" "yes" "yes";;
  *) assert_eq "and the caller is told on stderr" "yes" "no (stderr was '$ERR')";; esac
EXIT_CODE=0
ERR=$(ug_log --bean-id "$UG" --iteration 2 --scorecard /tmp/test-ug-truncated.json --dispatches 1 --guidance "" 2>&1) || EXIT_CODE=$?
assert_eq "a truncated scorecard still logs" "0" "$EXIT_CODE"
assert_eq "two unreadable cards, two iterations recorded" "2" \
  "$(BEANS_PATH="$UGT" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$UG" | jq -r '.iteration_count')"
rm -rf "$UGT"
rm -f /tmp/test-ug-empty.json /tmp/test-ug-truncated.json

echo ""
echo "Results: $PASS passed, $FAIL failed"
rm -f /tmp/test-scorecard.json /tmp/test-scorecard2.json /tmp/test-scorecard3.json /tmp/test-scorecard4.json /tmp/test-scorecard5.json /tmp/test-scorecard6.json /tmp/test-scorecard7.json /tmp/test-scorecard8.json /tmp/test-scorecard9.json /tmp/test-disagreements.json /tmp/test-disagreements-empty.json /tmp/test-antipatterns.json /tmp/test-antipatterns-empty.json /tmp/test-antipatterns2.json /tmp/test-corrections.json /tmp/test-spotcheck-corrections.json /tmp/test-scorecard-sc.json /tmp/test-fixt-i1.json /tmp/test-fixt-i2.json /tmp/test-fixt-sc.json /tmp/test-fixt-corr.json
[ "$FAIL" -eq 0 ] || exit 1
