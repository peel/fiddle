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

echo "=== Test 1: Two providers same domain — min score wins, disagreements detected ==="
INPUT='[
  {
    "task_id": "bean-1",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": {
      "general": {
        "dimensions": {
          "correctness": {"score": 9, "evidence": "good", "threshold": 7},
          "code_quality": {"score": 7, "evidence": "ok", "threshold": 6}
        }
      }
    },
    "criteria": [
      {"id": "c1", "pass": true, "evidence": "yes"},
      {"id": "c2", "pass": true, "evidence": "yes"}
    ],
    "antipatterns_detected": [],
    "guidance": "guidance-claude",
    "dispatch_count": 1
  },
  {
    "task_id": "bean-1",
    "iteration": 1,
    "timestamp": "2026-01-01T00:01:00Z",
    "provider": "codex",
    "domains": {
      "general": {
        "dimensions": {
          "correctness": {"score": 6, "evidence": "issues", "threshold": 7},
          "code_quality": {"score": 7, "evidence": "ok", "threshold": 6}
        }
      }
    },
    "criteria": [
      {"id": "c1", "pass": true, "evidence": "yes"},
      {"id": "c2", "pass": false, "evidence": "no"}
    ],
    "antipatterns_detected": [],
    "guidance": "guidance-codex",
    "dispatch_count": 1
  }
]'

EXIT_CODE=0
STDERR_FILE="$TMPDIR/stderr1.txt"
OUTPUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>"$STDERR_FILE") || EXIT_CODE=$?
STDERR_OUTPUT=$(cat "$STDERR_FILE")

assert_exit "two providers → exit 0" 0 "$EXIT_CODE"
assert_json "correctness score is min(9,6)=6" '.domains.general.dimensions.correctness.score' "6" "$OUTPUT"
assert_json "code_quality score is min(7,7)=7" '.domains.general.dimensions.code_quality.score' "7" "$OUTPUT"
assert_json "correctness threshold preserved" '.domains.general.dimensions.correctness.threshold' "7" "$OUTPUT"
assert_json "code_quality threshold preserved" '.domains.general.dimensions.code_quality.threshold' "6" "$OUTPUT"
assert_json "correctness provider_scores.claude=9" '.domains.general.dimensions.correctness.provider_scores.claude' "9" "$OUTPUT"
assert_json "correctness provider_scores.codex=6" '.domains.general.dimensions.correctness.provider_scores.codex' "6" "$OUTPUT"
assert_json "code_quality provider_scores.claude=7" '.domains.general.dimensions.code_quality.provider_scores.claude' "7" "$OUTPUT"
assert_json "code_quality provider_scores.codex=7" '.domains.general.dimensions.code_quality.provider_scores.codex' "7" "$OUTPUT"
assert_json "criterion c1 passes (both pass)" '.criteria[] | select(.id=="c1") | .pass' "true" "$OUTPUT"
assert_json "criterion c2 fails (any fail)" '.criteria[] | select(.id=="c2") | .pass' "false" "$OUTPUT"
assert_json "disagreement on stderr for correctness" '.[0].dimension' "correctness" "$STDERR_OUTPUT"
assert_json "disagreement domain is general" '.[0].domain' "general" "$STDERR_OUTPUT"
assert_json "disagreement spread is 3" '.[0].spread' "3" "$STDERR_OUTPUT"
assert_json "disagreement scores.claude=9" '.[0].scores.claude' "9" "$STDERR_OUTPUT"
assert_json "disagreement scores.codex=6" '.[0].scores.codex' "6" "$STDERR_OUTPUT"
assert_json "only 1 disagreement (code_quality excluded)" '. | length' "1" "$STDERR_OUTPUT"

echo ""
echo "=== Test 2: Single provider — passthrough with provider_scores ==="
INPUT_SINGLE='[
  {
    "task_id": "bean-2",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": {
      "general": {
        "dimensions": {
          "correctness": {"score": 8, "evidence": "good", "threshold": 7}
        }
      }
    },
    "criteria": [
      {"id": "c1", "pass": true, "evidence": "yes"}
    ],
    "antipatterns_detected": [],
    "guidance": "single guidance",
    "dispatch_count": 1
  }
]'

EXIT_CODE=0
STDERR_FILE="$TMPDIR/stderr2.txt"
OUTPUT=$(echo "$INPUT_SINGLE" | "$SCRIPT_DIR/merge-scorecards.sh" 2>"$STDERR_FILE") || EXIT_CODE=$?
STDERR_OUTPUT=$(cat "$STDERR_FILE")

assert_exit "single provider → exit 0" 0 "$EXIT_CODE"
assert_json "correctness score passthrough" '.domains.general.dimensions.correctness.score' "8" "$OUTPUT"
assert_json "correctness threshold passthrough" '.domains.general.dimensions.correctness.threshold' "7" "$OUTPUT"
assert_json "provider_scores added for single" '.domains.general.dimensions.correctness.provider_scores.claude' "8" "$OUTPUT"
assert_json "criteria pass through" '.criteria[0].pass' "true" "$OUTPUT"
assert_json "criteria id preserved" '.criteria[0].id' "c1" "$OUTPUT"
assert_json "no disagreements for single provider" '. | length' "0" "$STDERR_OUTPUT"

echo ""
echo "=== Test 3: Multi-domain — each domain merged independently ==="
INPUT_MULTI='[
  {
    "task_id": "bean-3",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": {
      "frontend": {
        "dimensions": {
          "correctness": {"score": 9, "evidence": "great", "threshold": 7}
        }
      },
      "backend": {
        "dimensions": {
          "correctness": {"score": 8, "evidence": "ok", "threshold": 7}
        }
      }
    },
    "criteria": [{"id": "c1", "pass": true, "evidence": "yes"}],
    "antipatterns_detected": [],
    "guidance": "g1",
    "dispatch_count": 1
  },
  {
    "task_id": "bean-3",
    "iteration": 1,
    "timestamp": "2026-01-01T00:01:00Z",
    "provider": "codex",
    "domains": {
      "frontend": {
        "dimensions": {
          "correctness": {"score": 5, "evidence": "bad", "threshold": 7}
        }
      },
      "backend": {
        "dimensions": {
          "correctness": {"score": 7, "evidence": "fine", "threshold": 7}
        }
      }
    },
    "criteria": [{"id": "c1", "pass": true, "evidence": "yes"}],
    "antipatterns_detected": [],
    "guidance": "g2",
    "dispatch_count": 1
  }
]'

EXIT_CODE=0
STDERR_FILE="$TMPDIR/stderr3.txt"
OUTPUT=$(echo "$INPUT_MULTI" | "$SCRIPT_DIR/merge-scorecards.sh" 2>"$STDERR_FILE") || EXIT_CODE=$?
STDERR_OUTPUT=$(cat "$STDERR_FILE")

assert_exit "multi-domain → exit 0" 0 "$EXIT_CODE"
assert_json "frontend correctness min(9,5)=5" '.domains.frontend.dimensions.correctness.score' "5" "$OUTPUT"
assert_json "frontend provider_scores.claude=9" '.domains.frontend.dimensions.correctness.provider_scores.claude' "9" "$OUTPUT"
assert_json "frontend provider_scores.codex=5" '.domains.frontend.dimensions.correctness.provider_scores.codex' "5" "$OUTPUT"
assert_json "backend correctness min(8,7)=7" '.domains.backend.dimensions.correctness.score' "7" "$OUTPUT"
assert_json "backend provider_scores.claude=8" '.domains.backend.dimensions.correctness.provider_scores.claude' "8" "$OUTPUT"
assert_json "backend provider_scores.codex=7" '.domains.backend.dimensions.correctness.provider_scores.codex' "7" "$OUTPUT"
assert_json "disagreement count is 1 (frontend only)" '. | length' "1" "$STDERR_OUTPUT"
assert_json "disagreement is on frontend" '.[0].domain' "frontend" "$STDERR_OUTPUT"
assert_json "disagreement spread is 4" '.[0].spread' "4" "$STDERR_OUTPUT"

echo ""
echo "=== Test 4: Malformed input — not valid JSON ==="
EXIT_CODE=0
echo "not json at all" | "$SCRIPT_DIR/merge-scorecards.sh" >/dev/null 2>/dev/null || EXIT_CODE=$?
assert_exit "malformed JSON → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 5: Malformed input — empty array ==="
EXIT_CODE=0
echo "[]" | "$SCRIPT_DIR/merge-scorecards.sh" >/dev/null 2>/dev/null || EXIT_CODE=$?
assert_exit "empty array → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 6: Malformed input — not an array ==="
EXIT_CODE=0
echo '{"not": "array"}' | "$SCRIPT_DIR/merge-scorecards.sh" >/dev/null 2>/dev/null || EXIT_CODE=$?
assert_exit "not an array → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 7: Three providers — min still wins ==="
INPUT_THREE='[
  {
    "task_id": "bean-4",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": {
      "general": {
        "dimensions": {
          "correctness": {"score": 9, "evidence": "great", "threshold": 7}
        }
      }
    },
    "criteria": [{"id": "c1", "pass": true, "evidence": "yes"}],
    "antipatterns_detected": [],
    "guidance": "g1",
    "dispatch_count": 1
  },
  {
    "task_id": "bean-4",
    "iteration": 1,
    "timestamp": "2026-01-01T00:01:00Z",
    "provider": "codex",
    "domains": {
      "general": {
        "dimensions": {
          "correctness": {"score": 7, "evidence": "ok", "threshold": 7}
        }
      }
    },
    "criteria": [{"id": "c1", "pass": true, "evidence": "yes"}],
    "antipatterns_detected": [],
    "guidance": "g2",
    "dispatch_count": 1
  },
  {
    "task_id": "bean-4",
    "iteration": 1,
    "timestamp": "2026-01-01T00:02:00Z",
    "provider": "gemini",
    "domains": {
      "general": {
        "dimensions": {
          "correctness": {"score": 5, "evidence": "bad", "threshold": 7}
        }
      }
    },
    "criteria": [{"id": "c1", "pass": false, "evidence": "no"}],
    "antipatterns_detected": [],
    "guidance": "g3",
    "dispatch_count": 1
  }
]'

EXIT_CODE=0
STDERR_FILE="$TMPDIR/stderr7.txt"
OUTPUT=$(echo "$INPUT_THREE" | "$SCRIPT_DIR/merge-scorecards.sh" 2>"$STDERR_FILE") || EXIT_CODE=$?
STDERR_OUTPUT=$(cat "$STDERR_FILE")

assert_exit "three providers → exit 0" 0 "$EXIT_CODE"
assert_json "correctness min(9,7,5)=5" '.domains.general.dimensions.correctness.score' "5" "$OUTPUT"
assert_json "provider_scores.claude=9" '.domains.general.dimensions.correctness.provider_scores.claude' "9" "$OUTPUT"
assert_json "provider_scores.codex=7" '.domains.general.dimensions.correctness.provider_scores.codex' "7" "$OUTPUT"
assert_json "provider_scores.gemini=5" '.domains.general.dimensions.correctness.provider_scores.gemini' "5" "$OUTPUT"
assert_json "criterion c1 fails (gemini fails)" '.criteria[0].pass' "false" "$OUTPUT"
assert_json "disagreement spread is 4" '.[0].spread' "4" "$STDERR_OUTPUT"

echo ""
echo "=== Test 8: No disagreements when spread < 3 ==="
INPUT_NO_DISAGREE='[
  {
    "task_id": "bean-5",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": {
      "general": {
        "dimensions": {
          "correctness": {"score": 8, "evidence": "good", "threshold": 7}
        }
      }
    },
    "criteria": [{"id": "c1", "pass": true, "evidence": "yes"}],
    "antipatterns_detected": [],
    "guidance": "g1",
    "dispatch_count": 1
  },
  {
    "task_id": "bean-5",
    "iteration": 1,
    "timestamp": "2026-01-01T00:01:00Z",
    "provider": "codex",
    "domains": {
      "general": {
        "dimensions": {
          "correctness": {"score": 6, "evidence": "ok", "threshold": 7}
        }
      }
    },
    "criteria": [{"id": "c1", "pass": true, "evidence": "yes"}],
    "antipatterns_detected": [],
    "guidance": "g2",
    "dispatch_count": 1
  }
]'

EXIT_CODE=0
STDERR_FILE="$TMPDIR/stderr8.txt"
OUTPUT=$(echo "$INPUT_NO_DISAGREE" | "$SCRIPT_DIR/merge-scorecards.sh" 2>"$STDERR_FILE") || EXIT_CODE=$?
STDERR_OUTPUT=$(cat "$STDERR_FILE")

assert_exit "spread=2 → exit 0" 0 "$EXIT_CODE"
assert_json "no disagreements (spread 2 < 3)" '. | length' "0" "$STDERR_OUTPUT"
assert_json "correctness min(8,6)=6" '.domains.general.dimensions.correctness.score' "6" "$OUTPUT"

echo ""
echo "=== Test 9: Metadata fields preserved from first scorecard ==="
EXIT_CODE=0
OUTPUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null) || EXIT_CODE=$?

assert_exit "metadata → exit 0" 0 "$EXIT_CODE"
assert_json "task_id preserved" '.task_id' "bean-1" "$OUTPUT"
assert_json "iteration preserved" '.iteration' "1" "$OUTPUT"

echo ""
echo "=== Test 10: single-element array normalizes without min-merge artifacts ==="
cat > "$TMPDIR/single.json" << 'EOF'
[{"provider":"codex","domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[{"id":"tests-pass","pass":true}]}]
EOF
EXIT_CODE=0
OUT=$("$SCRIPT_DIR/merge-scorecards.sh" < "$TMPDIR/single.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "single-element pin → exit 0" 0 "$EXIT_CODE"
assert_json "score preserved" ".domains.general.dimensions.correctness.score" "8" "$OUT"
assert_json "criteria preserved" ".criteria[0].pass" "true" "$OUT"

echo ""
echo "=== Test 11: scorecard without criteria key → exit 2 ==="
cat > "$TMPDIR/no-criteria.json" << 'EOF'
[{"provider":"codex","domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}}}]
EOF
EXIT_CODE=0
STDERR_FILE="$TMPDIR/stderr11.txt"
"$SCRIPT_DIR/merge-scorecards.sh" < "$TMPDIR/no-criteria.json" >/dev/null 2>"$STDERR_FILE" || EXIT_CODE=$?
assert_exit "missing criteria key → exit 2" 2 "$EXIT_CODE"
assert_json "stderr carries JSON error" '.error | length > 0' "true" "$(cat "$STDERR_FILE")"

echo ""
echo "=== Test 12: present-but-empty criteria [] normalizes with exit 0 ==="
cat > "$TMPDIR/empty-criteria.json" << 'EOF'
[{"provider":"codex","domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[]}]
EOF
EXIT_CODE=0
OUT=$("$SCRIPT_DIR/merge-scorecards.sh" < "$TMPDIR/empty-criteria.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "empty criteria array → exit 0" 0 "$EXIT_CODE"
assert_json "criteria normalizes to empty array" '.criteria | length' "0" "$OUT"

echo ""
echo "=== Test 13: holistic scorecards preserve conservative coverage and remediation ==="
cat > "$TMPDIR/holistic.json" <<'EOF'
[
  {
    "task_id": "epic-1",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": {"holistic": {"dimensions": {"integration": {"score": 8, "threshold": 7, "evidence": "claude"}}}},
    "criteria": [],
    "spec_coverage_matrix": [
      {"requirement": "R1", "coverage": "Full", "evidence": "claude full"},
      {"requirement": "R2", "coverage": "Weak", "evidence": "claude weak"}
    ],
    "remediation_beans": [
      {"requirement": "R1", "title": "Fix R1", "description": "short", "source": "spec_coverage:Missing"}
    ]
  },
  {
    "task_id": "epic-1",
    "iteration": 1,
    "timestamp": "2026-01-01T00:01:00Z",
    "provider": "codex",
    "domains": {"holistic": {"dimensions": {"integration": {"score": 6, "threshold": 7, "evidence": "codex"}}}},
    "criteria": [],
    "spec_coverage_matrix": [
      {"requirement": "R1", "coverage": "Missing", "evidence": "codex missing"},
      {"requirement": "R2", "coverage": "Full", "evidence": "codex full"}
    ],
    "remediation_beans": [
      {"requirement": "R1", "title": "Fix R1 thoroughly", "description": "the more specific remediation", "source": "spec_coverage:Missing"}
    ]
  }
]
EOF
EXIT_CODE=0
OUT=$("$SCRIPT_DIR/merge-scorecards.sh" < "$TMPDIR/holistic.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "holistic merge → exit 0" 0 "$EXIT_CODE"
assert_json "holistic dimension still min-merges" '.domains.holistic.dimensions.integration.score' "6" "$OUT"
assert_json "R1 takes conservative Missing coverage" '.spec_coverage_matrix[] | select(.requirement=="R1") | .coverage' "Missing" "$OUT"
assert_json "R2 takes conservative Weak coverage" '.spec_coverage_matrix[] | select(.requirement=="R2") | .coverage' "Weak" "$OUT"
assert_json "R1 records claude coverage" '.spec_coverage_matrix[] | select(.requirement=="R1") | .provider_coverage.claude' "Full" "$OUT"
assert_json "R1 records codex coverage" '.spec_coverage_matrix[] | select(.requirement=="R1") | .provider_coverage.codex' "Missing" "$OUT"
assert_json "remediation deduplicates by requirement" '[.remediation_beans[] | select(.requirement=="R1")] | length' "1" "$OUT"
assert_json "remediation keeps most specific description" '.remediation_beans[] | select(.requirement=="R1") | .description' "the more specific remediation" "$OUT"
assert_json "remediation records source providers" '.remediation_beans[] | select(.requirement=="R1") | .source_providers | sort | join(",")' "claude,codex" "$OUT"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
