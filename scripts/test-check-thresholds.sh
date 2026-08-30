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

assert_contains() {
  local desc="$1" needle="$2" haystack="$3"
  if [[ "$haystack" == *"$needle"* ]]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected to find '$needle' in: $haystack)"
  fi
}

assert_identical() {
  local desc="$1" expected_file="$2" actual_file="$3"
  if diff -u "$expected_file" "$actual_file" > /dev/null; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc"
    diff -u "$expected_file" "$actual_file" | sed 's/^/    /'
  fi
}

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

echo "Test 1: All dimensions pass"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 8, "threshold": 7},
        "domain_spec_fidelity": {"score": 9, "threshold": 8},
        "code_quality": {"score": 7, "threshold": 6}
      }
    }
  },
  "criteria": [{"id": "test-crit", "pass": true}]
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "test-crit", "pass": true}]
EOF

OUTFILE="$TMPDIR/out.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "all pass → exit 0" 0 "$EXIT_CODE"
assert_json "verdict is PASS" ".verdict" "PASS" "$OUTPUT"
assert_json "dimensions has correctness" '.dimensions["general.correctness"]' "8" "$OUTPUT"
assert_json "dimensions has domain_spec_fidelity" '.dimensions["general.domain_spec_fidelity"]' "9" "$OUTPUT"
assert_json "dimensions has code_quality" '.dimensions["general.code_quality"]' "7" "$OUTPUT"

echo "Test 2: One dimension below threshold"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 5, "threshold": 7},
        "domain_spec_fidelity": {"score": 9, "threshold": 8},
        "code_quality": {"score": 7, "threshold": 6}
      }
    }
  },
  "criteria": [{"id": "test-crit", "pass": true}]
}
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "one fail → exit 1" 1 "$EXIT_CODE"
assert_json "verdict is FAIL" ".verdict" "FAIL" "$OUTPUT"
assert_json "failing dim is correctness" ".failing_dimensions[0].dimension" "correctness" "$OUTPUT"
assert_json "dimensions has correctness score" '.dimensions["general.correctness"]' "5" "$OUTPUT"
assert_json "dimensions has domain_spec_fidelity score" '.dimensions["general.domain_spec_fidelity"]' "9" "$OUTPUT"
assert_json "dimensions has code_quality score" '.dimensions["general.code_quality"]' "7" "$OUTPUT"

echo "Test 3: Criterion fails"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 8, "threshold": 7},
        "domain_spec_fidelity": {"score": 9, "threshold": 8},
        "code_quality": {"score": 7, "threshold": 6}
      }
    }
  },
  "criteria": [{"id": "test-crit", "pass": false}]
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "test-crit", "pass": false}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "criterion fail → exit 1" 1 "$EXIT_CODE"
assert_json "verdict is FAIL" ".verdict" "FAIL" "$OUTPUT"
assert_json "dimensions present on crit fail" '.dimensions["general.correctness"]' "8" "$OUTPUT"

ERRFILE="$TMPDIR/err.txt"


echo "Test 4: A dimension with no threshold is refused, not passed"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 1, "comment": "x"},
        "code_quality": {"score": 1, "comment": "x"}
      }
    }
  },
  "criteria": [{"id": "test-crit", "pass": true}]
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "test-crit", "pass": true}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
ERRTEXT=$(cat "$ERRFILE")
assert_exit "no threshold -> exit 2" 2 "$EXIT_CODE"
assert_json "verdict is not PASS" ".verdict" "null" "$OUTPUT"
assert_contains "stderr names the missing field" 'missing `threshold`' "$ERRTEXT"
assert_contains "stderr names the first dimension" "domain general dimension correctness" "$ERRTEXT"
assert_contains "stderr names the second dimension too" "domain general dimension code_quality" "$ERRTEXT"
assert_json "stdout problems name the dimension" '(.problems // []) | map(select(contains("correctness"))) | length' "1" "$OUTPUT"

echo "Test 5: An ungraded criteria array is refused, not passed"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 8, "threshold": 7}
      }
    }
  },
  "criteria": [{"id": "an_ungraded_criterion", "evidence": ""}]
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "an_ungraded_criterion", "check": "the briefing text, not a grade"}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
ERRTEXT=$(cat "$ERRFILE")
assert_exit "ungraded criteria -> exit 2" 2 "$EXIT_CODE"
assert_json "verdict is not PASS" ".verdict" "null" "$OUTPUT"
assert_contains "stderr names the missing field" 'missing `pass`' "$ERRTEXT"
assert_contains "stderr names the criterion id" "criterion an_ungraded_criterion" "$ERRTEXT"

echo "Test 6: A non-boolean pass is refused"
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "stringly_typed", "pass": "false"}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
ERRTEXT=$(cat "$ERRFILE")
assert_exit "string pass -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr says pass must be boolean" '`pass` must be a boolean, got string' "$ERRTEXT"
assert_contains "stderr names the criterion id" "criterion stringly_typed" "$ERRTEXT"

echo "Test 7: A criterion with no id is refused"
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"pass": false, "evidence": "e"}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
ERRTEXT=$(cat "$ERRFILE")
assert_exit "no criterion id -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr says which entry lacks an id" 'criterion #0: missing `id`' "$ERRTEXT"

cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": 7, "pass": false, "evidence": "e"}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
ERRTEXT=$(cat "$ERRFILE")
assert_exit "non-string criterion id -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr says the id is the wrong type" 'criterion #0: `id` must be a string, got number' "$ERRTEXT"

echo "Test 8: A criteria file that is not an array is refused"
cat > "$TMPDIR/criteria.json" << 'EOF'
{"id": "test-crit", "pass": true}
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
ERRTEXT=$(cat "$ERRFILE")
assert_exit "criteria object -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr says criteria must be an array" "criteria must be a JSON array, got object" "$ERRTEXT"

echo "Test 9: Non-numeric scores and thresholds are refused"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": "1", "threshold": 7},
        "code_quality": {"score": 8, "threshold": "6"}
      }
    }
  }
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "test-crit", "pass": true}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
ERRTEXT=$(cat "$ERRFILE")
assert_exit "stringly-typed score -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr says score must be a number" '`score` must be a number, got string' "$ERRTEXT"
assert_contains "stderr says threshold must be a number" '`threshold` must be a number, got string' "$ERRTEXT"

echo "Test 10: A missing score is refused"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"threshold": 7, "comment": "x"}
      }
    }
  }
}
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
ERRTEXT=$(cat "$ERRFILE")
assert_exit "no score -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr names the missing field" 'missing `score`' "$ERRTEXT"

echo "Test 11: A domain key at top level instead of under domains is refused"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "provider": "codex",
  "general": {
    "dimensions": {
      "correctness": {"score": 1, "threshold": 7}
    }
  },
  "criteria": [{"id": "test-crit", "pass": true}]
}
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
ERRTEXT=$(cat "$ERRFILE")
assert_exit "no domains key -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr names the missing field" 'scorecard: missing `domains`' "$ERRTEXT"

echo "Test 12: Malformed JSON is refused before grading"
printf '%s' '{"domains": {' > "$TMPDIR/scorecard.json"

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "truncated scorecard -> exit 2" 2 "$EXIT_CODE"
assert_json "stdout reports the parse failure" ".error" "scorecard is not valid JSON" "$OUTPUT"

echo "Test 13: An evidence-only scorecard passes only when it declares the mode"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "mode": "evidence-only",
  "domains": {
    "general": {
      "dimensions": {}
    }
  },
  "criteria": [{"id": "tests-pass", "pass": true}]
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "tests-pass", "pass": true}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "declared evidence-only -> exit 0" 0 "$EXIT_CODE"
assert_json "verdict is PASS" ".verdict" "PASS" "$OUTPUT"
assert_json "dimensions map is empty" '.dimensions | length' "0" "$OUTPUT"
assert_json "the verdict carries the declaration forward" ".mode" "evidence-only" "$OUTPUT"

echo "Test 14: A real criterion failure reproduces its recorded verdict (fiddle-ek1e it2)"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
 "provider": "codex",
 "task_id": "fiddle-ek1e",
 "iteration": 2,
 "domains": {
  "general": {
   "dimensions": {
    "correctness": {"score": 8, "threshold": 7, "comment": "c"},
    "domain_spec_fidelity": {"score": 9, "threshold": 8, "comment": "c"},
    "code_quality": {"score": 7, "threshold": 6, "comment": "c"}
   }
  }
 },
 "criteria": [
  {"id": "a_foreign_marker_does_not_satisfy_a_sweep", "pass": false, "evidence": "e"},
  {"id": "the_fix_is_general_rather_than_a_special_case_on_a_spelling", "pass": true, "evidence": "e"},
  {"id": "what_a_second_sweep_concludes_is_recorded", "pass": true, "evidence": "e"}
 ]
}
EOF
jq -c '.criteria' "$TMPDIR/scorecard.json" > "$TMPDIR/criteria.json"
cat > "$TMPDIR/expected.json" << 'EOF'
{
  "verdict": "FAIL",
  "failing_dimensions": [],
  "failing_criteria": [
    "a_foreign_marker_does_not_satisfy_a_sweep"
  ],
  "passing_dimensions": [
    {
      "domain": "general",
      "dimension": "correctness",
      "score": 8,
      "threshold": 7
    },
    {
      "domain": "general",
      "dimension": "domain_spec_fidelity",
      "score": 9,
      "threshold": 8
    },
    {
      "domain": "general",
      "dimension": "code_quality",
      "score": 7,
      "threshold": 6
    }
  ],
  "dimensions": {
    "general.correctness": 8,
    "general.domain_spec_fidelity": 9,
    "general.code_quality": 7
  },
  "findings": []
}
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
assert_exit "recorded criterion failure -> exit 1" 1 "$EXIT_CODE"
assert_identical "output matches the recorded v-ek1e-it2 verdict byte for byte" "$TMPDIR/expected.json" "$OUTFILE"

echo "Test 15: A real dimension failure reproduces its recorded verdict (fiddle-o1ly it2)"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
 "provider": "codex",
 "task_id": "fiddle-o1ly",
 "iteration": 2,
 "domains": {
  "general": {
   "dimensions": {
    "correctness": {"score": 8, "threshold": 7, "comment": "c"},
    "domain_spec_fidelity": {"score": 7, "threshold": 8, "comment": "c"},
    "code_quality": {"score": 8, "threshold": 6, "comment": "c"}
   }
  }
 },
 "criteria": [
  {"id": "an_unrecognised_scheme_is_not_described_as_recognised", "pass": true, "evidence": "e"},
  {"id": "the_two_correct_messages_are_unchanged", "pass": true, "evidence": "e"}
 ]
}
EOF
jq -c '.criteria' "$TMPDIR/scorecard.json" > "$TMPDIR/criteria.json"
cat > "$TMPDIR/expected.json" << 'EOF'
{
  "verdict": "FAIL",
  "failing_dimensions": [
    {
      "domain": "general",
      "dimension": "domain_spec_fidelity",
      "score": 7,
      "threshold": 8
    }
  ],
  "failing_criteria": [],
  "passing_dimensions": [
    {
      "domain": "general",
      "dimension": "correctness",
      "score": 8,
      "threshold": 7
    },
    {
      "domain": "general",
      "dimension": "code_quality",
      "score": 8,
      "threshold": 6
    }
  ],
  "dimensions": {
    "general.correctness": 8,
    "general.domain_spec_fidelity": 7,
    "general.code_quality": 8
  },
  "findings": []
}
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
assert_exit "recorded dimension failure -> exit 1" 1 "$EXIT_CODE"
assert_identical "output matches the recorded v-o1ly-it2 verdict byte for byte" "$TMPDIR/expected.json" "$OUTFILE"

echo "Test 16: The criterion/met envelope every brief asked for is refused by name"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 8, "min": 7, "comment": "x"}
      }
    }
  }
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"criterion": "the gate reports every binary", "met": true, "evidence": "e"}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
ERRTEXT=$(cat "$ERRFILE")
assert_exit "criterion/met envelope -> exit 2" 2 "$EXIT_CODE"
assert_json "verdict is not PASS" ".verdict" "null" "$OUTPUT"
assert_contains "stderr names the wanted criteria shape" "[{id: string, pass: boolean}]" "$ERRTEXT"
assert_contains "stderr names the wanted dimension shape" "{score: number, threshold: number}" "$ERRTEXT"
assert_contains "stderr names the schema document" "skills/develop/scorecard-envelope.md" "$ERRTEXT"
assert_contains "stderr names criterion as the wrong spelling of id" 'missing `id` (found `criterion`' "$ERRTEXT"
assert_contains "stderr names met as the wrong spelling of pass" 'missing `pass` (found `met`' "$ERRTEXT"
assert_contains "stderr names min as the wrong spelling of threshold" 'missing `threshold` (found `min`' "$ERRTEXT"
assert_json "stdout carries the schema pointer" ".schema" "skills/develop/scorecard-envelope.md" "$OUTPUT"

echo "Test 17: A field that is simply absent reports no spurious misspelling"
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "bare", "evidence": "e"}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
ERRTEXT=$(cat "$ERRFILE")
assert_exit "absent pass -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr names the missing field" 'criterion bare: missing `pass`' "$ERRTEXT"
if [[ "$ERRTEXT" == *'criterion bare: missing `pass` (found'* ]]; then
  FAIL=$((FAIL+1)); echo "  FAIL: absent pass invents a misspelling"
else
  PASS=$((PASS+1)); echo "  PASS: absent pass reports no misspelling"
fi


echo "Test 18: A scorecard with no dimensions and no criteria is refused, not passed"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{"domains": {}, "criteria": []}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
ERRTEXT=$(cat "$ERRFILE")
assert_exit "nothing to grade -> exit 2" 2 "$EXIT_CODE"
assert_json "verdict is not PASS" ".verdict" "null" "$OUTPUT"
assert_json "stdout names the refusal" ".error" "scorecard has nothing to grade" "$OUTPUT"
assert_contains "stdout counts what it found" "0 dimensions and 0 criteria" "$OUTPUT"
assert_contains "stderr states the reason" "0 dimensions and 0 criteria" "$ERRTEXT"

echo "Test 18b: The merge product of a refused scorecard is refused (fiddle-0dzn)"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{"domains":{},"criteria":[]}
EOF
EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" --tree-sha deadbeef > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "empty merge product -> exit 2" 2 "$EXIT_CODE"
assert_json "no tree_sha is stamped on a refusal" ".tree_sha" "null" "$OUTPUT"

echo "Test 19: Zero dimensions beside real criteria is refused without a declaration (fiddle-ayrq)"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": { "general": { "dimensions": {} } },
  "criteria": [
    {"id": "a", "pass": true},
    {"id": "b", "pass": true},
    {"id": "c", "pass": true}
  ]
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "a", "pass": true}, {"id": "b", "pass": true}, {"id": "c", "pass": true}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
ERRTEXT=$(cat "$ERRFILE")
assert_exit "undeclared empty dimensions -> exit 2" 2 "$EXIT_CODE"
assert_json "verdict is not PASS" ".verdict" "null" "$OUTPUT"
assert_json "stdout names the refusal" ".error" "scorecard scored no dimensions and does not declare evidence-only" "$OUTPUT"
assert_contains "stderr names the declaration it wanted" 'mode' "$ERRTEXT"
assert_contains "stderr names the domain that scored nothing" "domain general" "$ERRTEXT"

echo "Test 19b: A mode the envelope does not accept is refused"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "mode": "evidence_only",
  "domains": { "general": { "dimensions": {} } },
  "criteria": [{"id": "a", "pass": true}]
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "a", "pass": true}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
ERRTEXT=$(cat "$ERRFILE")
assert_exit "unaccepted mode -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr names the value it got" 'evidence_only' "$ERRTEXT"

echo "Test 20: Criteria carrying one id twice are refused, not double-counted"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": {
      "dimensions": { "correctness": {"score": 8, "threshold": 7} }
    }
  }
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[
  {"id": "a", "pass": true},
  {"id": "b", "pass": true},
  {"id": "a", "pass": true},
  {"id": "b", "pass": false}
]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
ERRTEXT=$(cat "$ERRFILE")
assert_exit "duplicated criterion ids -> exit 2" 2 "$EXIT_CODE"
assert_json "verdict is neither PASS nor FAIL" ".verdict" "null" "$OUTPUT"
assert_contains "stderr names the duplicated id" 'duplicate criterion id: `a`' "$ERRTEXT"
assert_contains "stderr names the other duplicated id" 'duplicate criterion id: `b`' "$ERRTEXT"
assert_contains "stderr counts the entries against the ids" "4 criteria carry 2 distinct ids" "$ERRTEXT"

echo "Test 21: A domain scoring nothing beside a scored domain is refused at 1g, not here"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": { "dimensions": { "correctness": {"score": 8, "threshold": 7} } },
    "frontend": { "dimensions": {} }
  },
  "criteria": [{"id": "a", "pass": true}]
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[{"id": "a", "pass": true}]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "a partly scored card still grades -> exit 0" 0 "$EXIT_CODE"
assert_json "the scored dimension is graded" '.dimensions["general.correctness"]' "8" "$OUTPUT"
assert_json "the unscored domain contributes nothing" '.dimensions | length' "1" "$OUTPUT"

echo "Test 22: A holistic card carrying top-level criteria is refused, not graded (fiddle-9vpj)"
# The recorded holistic-review card for epic fiddle-yby8, iteration 5: real dimension
# scores and the five criteria the reviewer wrote for itself. Every dimension meets its
# threshold. Before this refusal, one self-authored criterion decided the epic's verdict.
cat > "$TMPDIR/yby8-i5.json" << 'EOF'
{
  "provider": "claude",
  "task_id": "fiddle-yby8",
  "iteration": 5,
  "domains": {
    "holistic": {
      "dimensions": {
        "integration": {"score": 7, "threshold": 7, "evidence": "The Jira product path joins cleanly end to end."},
        "coherence": {"score": 7, "threshold": 7, "evidence": "The load-bearing boundary is spelled the same way everywhere."},
        "holistic_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "All three named properties hold."},
        "polish": {"score": 7, "threshold": 6, "evidence": "Refusals name the key and the value."},
        "runtime_health": {"score": 9, "threshold": 9, "evidence": "No runtimes are configured; the gate stands in for them."}
      }
    }
  },
  "criteria": [
    {"id": "hn2r_table_row_restored", "pass": true},
    {"id": "no_other_orphaned_table_row", "pass": true},
    {"id": "gate_80_of_80_binaries", "pass": true},
    {"id": "boundary_stated_consistently", "pass": true},
    {"id": "defect_search_outside_the_reviewed_lineage", "pass": false}
  ]
}
EOF
jq '.criteria' "$TMPDIR/yby8-i5.json" > "$TMPDIR/yby8-i5-criteria.json"

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/yby8-i5.json" --criteria "$TMPDIR/yby8-i5-criteria.json" --tree-sha 1ba8240 > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
ERRTEXT=$(cat "$ERRFILE")
assert_exit "holistic card with top-level criteria -> exit 2" 2 "$EXIT_CODE"
assert_json "no verdict is computed" ".verdict" "null" "$OUTPUT"
assert_json "no tree_sha is stamped on a refusal" ".tree_sha" "null" "$OUTPUT"
assert_json "stdout names the refusal" ".error" \
  "holistic scorecard carries a top-level criteria array, which its contract does not define" "$OUTPUT"
assert_json "stdout cites the holistic contract" ".schema" "skills/develop/holistic-scorecard-schema.md" "$OUTPUT"
assert_contains "stderr counts the entries it will not grade" "top-level \`criteria\` carries 5 entries" "$ERRTEXT"
assert_contains "stderr names the self-authored criterion" "defect_search_outside_the_reviewed_lineage" "$ERRTEXT"
assert_contains "stderr says where a finding belongs" "remediation_beans" "$ERRTEXT"
assert_contains "stderr says where severity belongs" "a dimension score" "$ERRTEXT"
assert_contains "stderr cites the holistic contract" "skills/develop/holistic-scorecard-schema.md" "$ERRTEXT"

echo "Test 22b: The same card conforming grades on its dimensions alone"
jq '.criteria = []' "$TMPDIR/yby8-i5.json" > "$TMPDIR/yby8-i5-conforming.json"
echo '[]' > "$TMPDIR/yby8-i5-empty.json"

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/yby8-i5-conforming.json" --criteria "$TMPDIR/yby8-i5-empty.json" --tree-sha 1ba8240 > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "a conforming holistic card still grades -> exit 0" 0 "$EXIT_CODE"
assert_json "verdict is PASS on dimensions alone" ".verdict" "PASS" "$OUTPUT"
assert_json "integration is graded" '.dimensions["holistic.integration"]' "7" "$OUTPUT"
assert_json "runtime_health is graded" '.dimensions["holistic.runtime_health"]' "9" "$OUTPUT"
assert_json "no dimension fails" ".failing_dimensions | tojson" "[]" "$OUTPUT"

echo "Test 22c: The same criteria under a per-task domain are still graded"
# The refusal is keyed on the holistic domain, not on criteria being present. Without this
# case the check would pass while refusing every card that carries a criterion at all.
jq '{domains: {general: .domains.holistic}, criteria: .criteria}' "$TMPDIR/yby8-i5.json" > "$TMPDIR/yby8-i5-as-task.json"
jq '.criteria' "$TMPDIR/yby8-i5-as-task.json" > "$TMPDIR/yby8-i5-as-task-criteria.json"

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/yby8-i5-as-task.json" --criteria "$TMPDIR/yby8-i5-as-task-criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "a per-task card with criteria still grades -> exit 1" 1 "$EXIT_CODE"
assert_json "verdict is FAIL, not a refusal" ".verdict" "FAIL" "$OUTPUT"
assert_json "the failing criterion is named" ".failing_criteria[0]" "defect_search_outside_the_reviewed_lineage" "$OUTPUT"

echo "Test 22d: Criteria reaching the grader only through --criteria are refused too"
EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/yby8-i5-conforming.json" --criteria "$TMPDIR/yby8-i5-criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
ERRTEXT=$(cat "$ERRFILE")
assert_exit "a holistic card graded against a non-empty array -> exit 2" 2 "$EXIT_CODE"
assert_json "no verdict is computed" ".verdict" "null" "$OUTPUT"
assert_contains "stderr names the array it was handed" "--criteria: the graded array carries 5 entries" "$ERRTEXT"
CARD_LINE=$(echo "$ERRTEXT" | grep -c 'top-level `criteria` carries' || true)
if [ "$CARD_LINE" = "0" ]; then
  PASS=$((PASS+1)); echo "  PASS: the empty array on the card is not reported as an offender"
else
  FAIL=$((FAIL+1)); echo "  FAIL: the empty array on the card was reported as an offender"
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
