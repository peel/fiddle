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
OUTFILE="$TMPDIR/out.json"

echo "Test 1: First PASS → PASS_PENDING"
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"PASS","failing_dimensions":[],"failing_criteria":[]}
EOF
echo "[]" > "$TMPDIR/history.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 60 --current-dispatches 2 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "first pass → exit 1" 1 "$EXIT_CODE"
assert_json "status PASS_PENDING" ".status" "PASS_PENDING" "$OUTPUT"

echo "Test 2: Two consecutive PASS → CONVERGED"
cat > "$TMPDIR/history.json" << 'EOF'
[{"verdict":"PASS","failing_dimensions":[],"failing_criteria":[],
  "dimensions":{"general.correctness":8,"general.code_quality":7}}]
EOF
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"PASS","failing_dimensions":[],"failing_criteria":[],
 "dimensions":{"general.correctness":8,"general.code_quality":7}}
EOF
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 60 --current-dispatches 4 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "two passes → exit 0" 0 "$EXIT_CODE"
assert_json "status CONVERGED" ".status" "CONVERGED" "$OUTPUT"

echo "Test 3: PASS but score regressed → PASS_REGRESSED"
cat > "$TMPDIR/history.json" << 'EOF'
[{"verdict":"PASS","failing_dimensions":[],"failing_criteria":[],
  "dimensions":{"general.correctness":9,"general.code_quality":7}}]
EOF
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"PASS","failing_dimensions":[],"failing_criteria":[],
 "dimensions":{"general.correctness":7,"general.code_quality":7}}
EOF
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 60 --current-dispatches 4 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "regression → exit 1" 1 "$EXIT_CODE"
assert_json "status PASS_REGRESSED" ".status" "PASS_REGRESSED" "$OUTPUT"

echo "Test 4: FAIL verdict"
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"FAIL","failing_dimensions":[{"dimension":"correctness"}],"failing_criteria":[]}
EOF
echo "[]" > "$TMPDIR/history.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 60 --current-dispatches 2 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "fail → exit 1" 1 "$EXIT_CODE"
assert_json "status FAIL" ".status" "FAIL" "$OUTPUT"

echo "Test 5: Dispatches exceeded"
echo "[]" > "$TMPDIR/history.json"
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"FAIL","failing_dimensions":[],"failing_criteria":[]}
EOF
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 5 --current-dispatches 6 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "dispatches exceeded → exit 2" 2 "$EXIT_CODE"
assert_json "status DISPATCHES_EXCEEDED" ".status" "DISPATCHES_EXCEEDED" "$OUTPUT"

echo "Test 6: PASS with empty dimensions map → CONVERGED on first pass"
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"PASS","failing_dimensions":[],"failing_criteria":[],"dimensions":{}}
EOF
echo "[]" > "$TMPDIR/history.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 60 --current-dispatches 2 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "evidence-only first pass → exit 0" 0 "$EXIT_CODE"
assert_json "status CONVERGED" ".status" "CONVERGED" "$OUTPUT"
assert_json "mode evidence-only" ".mode" "evidence-only" "$OUTPUT"

echo "Test 7: PASS with populated dimensions still requires double-pass"
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"PASS","failing_dimensions":[],"failing_criteria":[],"dimensions":{"general.correctness":8}}
EOF
echo "[]" > "$TMPDIR/history.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 60 --current-dispatches 2 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "judgment first pass → exit 1" 1 "$EXIT_CODE"
assert_json "status PASS_PENDING" ".status" "PASS_PENDING" "$OUTPUT"

echo "Test 8: evidence-only FAIL still fails"
cat > "$TMPDIR/current.json" << 'EOF'
{"verdict":"FAIL","failing_dimensions":[],"failing_criteria":["tests-pass"],"dimensions":{}}
EOF
echo "[]" > "$TMPDIR/history.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-convergence.sh" --current "$TMPDIR/current.json" --history "$TMPDIR/history.json" --max-dispatches 60 --current-dispatches 2 > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
assert_exit "evidence-only fail → exit 1" 1 "$EXIT_CODE"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
