#!/usr/bin/env bash
# test-merge-scorecards.sh — Tests for merge-scorecards.sh
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

# ── Test 1: Two providers, same domain — min scores, disagreements ────────────
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
# Min score for correctness: min(9,6) = 6
assert_json "correctness score is min(9,6)=6" '.domains.general.dimensions.correctness.score' "6" "$OUTPUT"
# Min score for code_quality: min(7,7) = 7
assert_json "code_quality score is min(7,7)=7" '.domains.general.dimensions.code_quality.score' "7" "$OUTPUT"
# Thresholds preserved
assert_json "correctness threshold preserved" '.domains.general.dimensions.correctness.threshold' "7" "$OUTPUT"
assert_json "code_quality threshold preserved" '.domains.general.dimensions.code_quality.threshold' "6" "$OUTPUT"
# Provider scores recorded
assert_json "correctness provider_scores.claude=9" '.domains.general.dimensions.correctness.provider_scores.claude' "9" "$OUTPUT"
assert_json "correctness provider_scores.codex=6" '.domains.general.dimensions.correctness.provider_scores.codex' "6" "$OUTPUT"
assert_json "code_quality provider_scores.claude=7" '.domains.general.dimensions.code_quality.provider_scores.claude' "7" "$OUTPUT"
assert_json "code_quality provider_scores.codex=7" '.domains.general.dimensions.code_quality.provider_scores.codex' "7" "$OUTPUT"
# Criteria merge: c1 both pass → pass, c2 codex fails → fail
assert_json "criterion c1 passes (both pass)" '.criteria[] | select(.id=="c1") | .pass' "true" "$OUTPUT"
assert_json "criterion c2 fails (any fail)" '.criteria[] | select(.id=="c2") | .pass' "false" "$OUTPUT"
# Disagreements on stderr: correctness spread=3 (9-6=3)
assert_json "disagreement on stderr for correctness" '.[0].dimension' "correctness" "$STDERR_OUTPUT"
assert_json "disagreement domain is general" '.[0].domain' "general" "$STDERR_OUTPUT"
assert_json "disagreement spread is 3" '.[0].spread' "3" "$STDERR_OUTPUT"
assert_json "disagreement scores.claude=9" '.[0].scores.claude' "9" "$STDERR_OUTPUT"
assert_json "disagreement scores.codex=6" '.[0].scores.codex' "6" "$STDERR_OUTPUT"
# No disagreement for code_quality (spread=0)
assert_json "only 1 disagreement (code_quality excluded)" '. | length' "1" "$STDERR_OUTPUT"

# ── Test 2: Single provider — passthrough with provider_scores wrapper ────────
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
# No disagreements for single provider
assert_json "no disagreements for single provider" '. | length' "0" "$STDERR_OUTPUT"

# ── Test 3: Multi-domain — each domain merged independently ───────────────────
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
# Frontend: min(9,5)=5
assert_json "frontend correctness min(9,5)=5" '.domains.frontend.dimensions.correctness.score' "5" "$OUTPUT"
assert_json "frontend provider_scores.claude=9" '.domains.frontend.dimensions.correctness.provider_scores.claude' "9" "$OUTPUT"
assert_json "frontend provider_scores.codex=5" '.domains.frontend.dimensions.correctness.provider_scores.codex' "5" "$OUTPUT"
# Backend: min(8,7)=7
assert_json "backend correctness min(8,7)=7" '.domains.backend.dimensions.correctness.score' "7" "$OUTPUT"
assert_json "backend provider_scores.claude=8" '.domains.backend.dimensions.correctness.provider_scores.claude' "8" "$OUTPUT"
assert_json "backend provider_scores.codex=7" '.domains.backend.dimensions.correctness.provider_scores.codex' "7" "$OUTPUT"
# Disagreement: frontend spread=4 (>=3), backend spread=1 (<3)
assert_json "disagreement count is 1 (frontend only)" '. | length' "1" "$STDERR_OUTPUT"
assert_json "disagreement is on frontend" '.[0].domain' "frontend" "$STDERR_OUTPUT"
assert_json "disagreement spread is 4" '.[0].spread' "4" "$STDERR_OUTPUT"

# ── Test 4: Malformed input — not valid JSON ──────────────────────────────────
echo ""
echo "=== Test 4: Malformed input — not valid JSON ==="
EXIT_CODE=0
echo "not json at all" | "$SCRIPT_DIR/merge-scorecards.sh" >/dev/null 2>/dev/null || EXIT_CODE=$?
assert_exit "malformed JSON → exit 2" 2 "$EXIT_CODE"

# ── Test 5: Malformed input — empty array ─────────────────────────────────────
echo ""
echo "=== Test 5: Malformed input — empty array ==="
EXIT_CODE=0
echo "[]" | "$SCRIPT_DIR/merge-scorecards.sh" >/dev/null 2>/dev/null || EXIT_CODE=$?
assert_exit "empty array → exit 2" 2 "$EXIT_CODE"

# ── Test 6: Malformed input — not an array ────────────────────────────────────
echo ""
echo "=== Test 6: Malformed input — not an array ==="
EXIT_CODE=0
echo '{"not": "array"}' | "$SCRIPT_DIR/merge-scorecards.sh" >/dev/null 2>/dev/null || EXIT_CODE=$?
assert_exit "not an array → exit 2" 2 "$EXIT_CODE"

# ── Test 7: Three providers — min still wins ──────────────────────────────────
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
# Disagreement: spread = 9-5 = 4
assert_json "disagreement spread is 4" '.[0].spread' "4" "$STDERR_OUTPUT"

# ── Test 8: No disagreements when spread < 3 ─────────────────────────────────
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

# ── Test 9: Metadata fields preserved ─────────────────────────────────────────
echo ""
echo "=== Test 9: Metadata fields preserved from first scorecard ==="
EXIT_CODE=0
OUTPUT=$(echo "$INPUT" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null) || EXIT_CODE=$?

assert_exit "metadata → exit 0" 0 "$EXIT_CODE"
assert_json "task_id preserved" '.task_id' "bean-1" "$OUTPUT"
assert_json "iteration preserved" '.iteration' "1" "$OUTPUT"

# ── Test 10: Single-element array — normalization pin ────────────────────────
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
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
