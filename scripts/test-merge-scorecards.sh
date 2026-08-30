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
  actual=$(printf '%s' "$json" | jq -r "$field" 2>&1) || actual="jq refused: $actual"
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"
  fi
}

assert_equal() {
  local desc="$1" expected="$2" actual="$3"
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

echo "=== Test 14: an all-evidence-only merge carries the declaration forward ==="
INPUT='[
  {
    "task_id": "bean-14",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "mode": "evidence-only",
    "domains": { "general": { "dimensions": {} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e1"}]
  },
  {
    "task_id": "bean-14",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "mode": "evidence-only",
    "domains": { "general": { "dimensions": {} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e2"}]
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "merged card declares evidence-only" ".mode" "evidence-only" "$OUT"

echo "$OUT" > "$TMPDIR/merged-14.json"
echo "$OUT" | jq -c '.criteria' > "$TMPDIR/criteria-14.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/merged-14.json" --criteria "$TMPDIR/criteria-14.json" > "$TMPDIR/verdict-14.json" 2>/dev/null || EXIT_CODE=$?
assert_exit "the grader accepts the declared merge" 0 "$EXIT_CODE"
assert_json "the verdict carries the declaration" ".mode" "evidence-only" "$(cat "$TMPDIR/verdict-14.json")"

echo "=== Test 15: one scored card among evidence-only cards drops the declaration ==="
INPUT='[
  {
    "task_id": "bean-15",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "mode": "evidence-only",
    "domains": { "general": { "dimensions": {} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e1"}]
  },
  {
    "task_id": "bean-15",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e2"}]
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "merged card claims no declaration" ".mode" "null" "$OUT"
assert_json "the scored dimension survives the merge" '.domains.general.dimensions.correctness.score' "8" "$OUT"

echo ""
echo "=== Test 16: a flagged spec defect survives the merge and names its source ==="
INPUT='[
  {
    "task_id": "bean-16",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": {"detected": true, "reason": "the spec asks for a batch call the API does not have"}
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "the merged card carries the flag, which the dropped field could not" '.spec_defect.detected' "true" "$OUT"
assert_json "the merged card names the state, so the reader does not infer it from a missing key" '.spec_defect.state' "detected" "$OUT"
assert_json "the reason survives the merge" '.spec_defect.reason | contains("batch call the API does not have")' "true" "$OUT"
assert_json "the reason names the domain and the provider that raised it" '.spec_defect.reason | startswith("general/codex: ")' "true" "$OUT"
assert_json "one source raised it" '.spec_defect.sources | length' "1" "$OUT"
assert_json "the source names its provider" '.spec_defect.sources[0].provider' "codex" "$OUT"
assert_json "the source names its domain" '.spec_defect.sources[0].domain' "general" "$OUT"

echo ""
echo "=== Test 17: one domain flags and one does not, and only the flagging reason survives ==="
INPUT='[
  {
    "task_id": "bean-17",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": {"detected": true, "reason": "the premise about resolveIdentity is false"}
  },
  {
    "task_id": "bean-17",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": {"detected": false, "reason": "the spec reads sound"}
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "one flag among cards that agree otherwise still flags the merge" '.spec_defect.detected' "true" "$OUT"
assert_json "the surviving reason is the flagging one" '.spec_defect.reason' "general/claude: the premise about resolveIdentity is false" "$OUT"
assert_json "the card that reported no defect is not listed as a source" '[.spec_defect.sources[].provider] | join(",")' "claude" "$OUT"
assert_json "the reason does not carry the text of the card that flagged nothing" '.spec_defect.reason | contains("reads sound")' "false" "$OUT"

echo ""
echo "=== Test 18: every source reporting no defect merges to a clear card, not a flagged one ==="
INPUT='[
  {
    "task_id": "bean-18",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": null
  },
  {
    "task_id": "bean-18",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": null
  }
]'
CLEAR=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "a merge that flagged every card would fail here" '.spec_defect.detected' "false" "$CLEAR"
assert_json "the clear state is named" '.spec_defect.state' "clear" "$CLEAR"
assert_json "the clear card names the cards that reported" '.spec_defect.reported_by | join(",")' "claude,codex" "$CLEAR"
assert_json "the clear card carries the whole state and no reason to route on" '.spec_defect | keys | join(",")' "detected,missing_from,reported_by,sources,state" "$CLEAR"

echo ""
echo "=== Test 19: a field that never arrived does not read as a clean evaluation ==="
INPUT='[
  {
    "task_id": "bean-19",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}]
  },
  {
    "task_id": "bean-19",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}]
  }
]'
ABSENT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "the merged card always carries the key, so a reader never reads a dropped field as null" 'has("spec_defect")' "true" "$ABSENT"
assert_json "the state says the field never arrived" '.spec_defect.state' "not_reported" "$ABSENT"
assert_json "the unreported card carries no detected key, so no reader sees a false there" '.spec_defect | keys | join(",")' "missing_from,reported_by,sources,state" "$ABSENT"
assert_json "the card names who did not report" '.spec_defect.missing_from | join(",")' "claude,codex" "$ABSENT"
assert_equal "an unreported field and a cleared spec are different outputs" \
  "different" \
  "$(if [ "$(echo "$CLEAR" | jq -c .spec_defect)" = "$(echo "$ABSENT" | jq -c .spec_defect)" ]; then echo same; else echo different; fi)"

echo ""
echo "=== Test 20: one card omitting the field makes the whole merge unreported ==="
INPUT='[
  {
    "task_id": "bean-20",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": null
  },
  {
    "task_id": "bean-20",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}]
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "one silent card is enough to withhold the clear verdict" '.spec_defect.state' "not_reported" "$OUT"
assert_json "only the silent card is named" '.spec_defect.missing_from | join(",")' "codex" "$OUT"
assert_json "the card that did report is still named" '.spec_defect.reported_by | join(",")' "claude" "$OUT"

echo ""
echo "=== Test 21: a flag outranks a card that never reported ==="
INPUT='[
  {
    "task_id": "bean-21",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "frontend": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": {"detected": true, "reason": "the spec names a component that was deleted"}
  },
  {
    "task_id": "bean-21",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "frontend": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}]
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "the flag wins over the silence" '.spec_defect.state' "detected" "$OUT"
assert_json "the flagged domain is named" '.spec_defect.sources[0].domain' "frontend" "$OUT"
assert_json "the silent card is still named, so the flag does not hide it" '.spec_defect.missing_from | join(",")' "codex" "$OUT"

echo ""
echo "=== Test 22: a spec_defect that states nothing is not a clear one ==="
INPUT='[
  {
    "task_id": "bean-22",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": {"detected": "true"}
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "a string where the boolean belongs is not read as a flag" '.spec_defect.state' "not_reported" "$OUT"
assert_json "the card that stated nothing is named" '.spec_defect.missing_from | join(",")' "claude" "$OUT"
assert_json "and it is not counted as having reported" '.spec_defect.reported_by | length' "0" "$OUT"

echo ""
echo "=== Test 23: an empty spec_defect object states nothing either ==="
INPUT='[
  {
    "task_id": "bean-23",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": {}
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "an object carrying no verdict is not read as clear" '.spec_defect.state' "not_reported" "$OUT"
assert_json "the card that carried it is named as silent" '.spec_defect.missing_from | join(",")' "codex" "$OUT"

echo ""
echo "=== Test 24: a boolean where the spec_defect object belongs states nothing ==="
INPUT='[
  {
    "task_id": "bean-24",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": true
  },
  {
    "task_id": "bean-24",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": null
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "a boolean is not read as a clear verdict" '.spec_defect.state' "not_reported" "$OUT"
assert_json "the merged card states no detected verdict" '.spec_defect | has("detected")' "false" "$OUT"
assert_json "the card carrying a boolean is named as silent" '.spec_defect.missing_from | join(",")' "claude" "$OUT"
assert_json "the card that did report is still named" '.spec_defect.reported_by | join(",")' "codex" "$OUT"

echo ""
echo "=== Test 25: a string where the spec_defect object belongs states nothing ==="
INPUT='[
  {
    "task_id": "bean-25",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": "unknown"
  },
  {
    "task_id": "bean-25",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": null
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "a string is not read as a clear verdict" '.spec_defect.state' "not_reported" "$OUT"
assert_json "the merged card states no detected verdict" '.spec_defect | has("detected")' "false" "$OUT"
assert_json "the card carrying a string is named as silent" '.spec_defect.missing_from | join(",")' "claude" "$OUT"
assert_json "the card that did report is still named" '.spec_defect.reported_by | join(",")' "codex" "$OUT"

echo ""
echo "=== Test 26: an array where the spec_defect object belongs states nothing ==="
INPUT='[
  {
    "task_id": "bean-26",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": ["detected"]
  },
  {
    "task_id": "bean-26",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": null
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "an array is not read as a clear verdict" '.spec_defect.state' "not_reported" "$OUT"
assert_json "the merged card states no detected verdict" '.spec_defect | has("detected")' "false" "$OUT"
assert_json "the card carrying an array is named as silent" '.spec_defect.missing_from | join(",")' "claude" "$OUT"
assert_json "the card that did report is still named" '.spec_defect.reported_by | join(",")' "codex" "$OUT"

echo ""
echo "=== Test 27: a number where the spec_defect object belongs states nothing ==="
INPUT='[
  {
    "task_id": "bean-27",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": 0
  },
  {
    "task_id": "bean-27",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": null
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "a number is not read as a clear verdict" '.spec_defect.state' "not_reported" "$OUT"
assert_json "the merged card states no detected verdict" '.spec_defect | has("detected")' "false" "$OUT"
assert_json "the card carrying a number is named as silent" '.spec_defect.missing_from | join(",")' "claude" "$OUT"
assert_json "the card that did report is still named" '.spec_defect.reported_by | join(",")' "codex" "$OUT"

echo ""
echo "=== Test 28: every card reporting properly still merges to clear, so the type check is not vacuous ==="
INPUT='[
  {
    "task_id": "bean-28",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": {"detected": false}
  },
  {
    "task_id": "bean-28",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "domains": { "general": { "dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}} } },
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "spec_defect": null
  }
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "both reporting shapes still reach clear" '.spec_defect.state' "clear" "$OUT"
assert_json "and both are named as having reported" '.spec_defect.reported_by | join(",")' "claude,codex" "$OUT"
assert_json "with nothing left silent" '.spec_defect.missing_from | length' "0" "$OUT"

echo ""
echo "=== Test 29: every card is counted exactly once, whatever shape it carried ==="
INPUT='[
  {"task_id":"bean-29","iteration":1,"timestamp":"2026-01-01T00:00:00Z","provider":"p-absent",
   "domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"e"}}}},
   "criteria":[{"id":"c1","pass":true,"evidence":"e"}]},
  {"task_id":"bean-29","iteration":1,"timestamp":"2026-01-01T00:00:00Z","provider":"p-null",
   "domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"e"}}}},
   "criteria":[{"id":"c1","pass":true,"evidence":"e"}],"spec_defect":null},
  {"task_id":"bean-29","iteration":1,"timestamp":"2026-01-01T00:00:00Z","provider":"p-object",
   "domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"e"}}}},
   "criteria":[{"id":"c1","pass":true,"evidence":"e"}],"spec_defect":{"detected":false}},
  {"task_id":"bean-29","iteration":1,"timestamp":"2026-01-01T00:00:00Z","provider":"p-empty-object",
   "domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"e"}}}},
   "criteria":[{"id":"c1","pass":true,"evidence":"e"}],"spec_defect":{}},
  {"task_id":"bean-29","iteration":1,"timestamp":"2026-01-01T00:00:00Z","provider":"p-boolean",
   "domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"e"}}}},
   "criteria":[{"id":"c1","pass":true,"evidence":"e"}],"spec_defect":true},
  {"task_id":"bean-29","iteration":1,"timestamp":"2026-01-01T00:00:00Z","provider":"p-string",
   "domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"e"}}}},
   "criteria":[{"id":"c1","pass":true,"evidence":"e"}],"spec_defect":"unknown"},
  {"task_id":"bean-29","iteration":1,"timestamp":"2026-01-01T00:00:00Z","provider":"p-array",
   "domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"e"}}}},
   "criteria":[{"id":"c1","pass":true,"evidence":"e"}],"spec_defect":["detected"]},
  {"task_id":"bean-29","iteration":1,"timestamp":"2026-01-01T00:00:00Z","provider":"p-number",
   "domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"e"}}}},
   "criteria":[{"id":"c1","pass":true,"evidence":"e"}],"spec_defect":0}
]'
OUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null)
assert_json "eight cards land in exactly eight slots" \
  '(.spec_defect.reported_by | length) + (.spec_defect.missing_from | length)' "8" "$OUT"
assert_json "the two reporting shapes are the only ones counted as reporting" \
  '.spec_defect.reported_by | sort | join(",")' "p-null,p-object" "$OUT"
assert_json "every other shape is named as silent" \
  '.spec_defect.missing_from | sort | join(",")' "p-absent,p-array,p-boolean,p-empty-object,p-number,p-string" "$OUT"

echo ""
echo "Results: $PASS passed, $FAIL failed of $((PASS + FAIL))"
[ "$FAIL" -eq 0 ] || exit 1
