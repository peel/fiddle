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

echo "Test 13: An evidence-only scorecard still passes with an empty dimensions map"
cat > "$TMPDIR/scorecard.json" << 'EOF'
{
  "domains": {
    "general": {
      "dimensions": {}
    }
  },
  "criteria": []
}
EOF
cat > "$TMPDIR/criteria.json" << 'EOF'
[]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2> "$ERRFILE" || EXIT_CODE=$?
OUTPUT=$(cat "$OUTFILE")
assert_exit "evidence-only -> exit 0" 0 "$EXIT_CODE"
assert_json "verdict is PASS" ".verdict" "PASS" "$OUTPUT"
assert_json "dimensions map is empty" '.dimensions | length' "0" "$OUTPUT"


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
  }
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
  }
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

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
