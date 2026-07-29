#!/usr/bin/env bash
# test-multi-provider.sh — Integration test: provider selection and holistic
# multi-provider merge. Tests select-evaluator-provider.sh preference order,
# merge-scorecards.sh min-merge (holistic path), single-element normalization,
# disagreement surfacing, budget accounting
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
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
  if echo "$haystack" | grep -q "$needle"; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected to contain '$needle')"
  fi
}

assert_file_exists() {
  local desc="$1" path="$2"
  if [ -f "$path" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (file not found: $path)"
  fi
}

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

# ═══════════════════════════════════════════════════════════════════════════════
echo "=== Test 1: Per-task selection honors provider preference order ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Selection needs only bash and jq on PATH; fake provider binaries control
# which providers look available (same technique as test-select-evaluator-provider.sh).
STUB_BIN="$TMPDIR/stub-bin"
mkdir -p "$STUB_BIN"
ln -s "$(command -v bash)" "$STUB_BIN/bash"
ln -s "$(command -v jq)" "$STUB_BIN/jq"

FAKE_BIN="$TMPDIR/fake-bin"
mkdir -p "$FAKE_BIN"
printf '#!/bin/sh\nexit 0\n' > "$FAKE_BIN/codex"; chmod +x "$FAKE_BIN/codex"
printf '#!/bin/sh\nexit 0\n' > "$FAKE_BIN/gemini"; chmod +x "$FAKE_BIN/gemini"

EXIT_CODE=0
SEL_OUT=$(PATH="$FAKE_BIN:$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "codex,gemini" --implementer claude) || EXIT_CODE=$?
assert_exit "selection exits 0" 0 "$EXIT_CODE"
assert_json "first preferred provider wins" ".provider" "codex" "$SEL_OUT"

SEL_OUT=$(PATH="$FAKE_BIN:$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "gemini,codex" --implementer claude)
assert_json "reversed preference flips the selection" ".provider" "gemini" "$SEL_OUT"

SEL_OUT=$(PATH="$FAKE_BIN:$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "claude,codex" --implementer claude)
assert_json "implementer provider is skipped for a differing one" ".provider" "codex" "$SEL_OUT"

SEL_OUT=$(PATH="$STUB_BIN" "$SCRIPT_DIR/select-evaluator-provider.sh" \
  --preference "codex,gemini" --implementer claude)
assert_json "no external available falls back to implementer" ".provider" "claude" "$SEL_OUT"
assert_json "fallback reason recorded" '.reason | test("fallback")' "true" "$SEL_OUT"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 2: Two-provider merge (holistic path) — min scores, provider_scores, disagreements ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Create mock provider scorecards for domain "general"
cat > "$TMPDIR/scorecard-claude.json" << 'EOF'
{
  "provider": "claude",
  "task_id": "test-task-1",
  "iteration": 1,
  "timestamp": "2026-03-30T00:00:00Z",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 9, "threshold": 7, "evidence": "All tests pass"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "Matches spec"},
        "code_quality": {"score": 7, "threshold": 6, "evidence": "Clean code"}
      }
    }
  },
  "criteria": [
    {"id": "c_file_exists", "pass": true, "evidence": "File found"},
    {"id": "c_tests_pass", "pass": true, "evidence": "Tests green"}
  ],
  "antipatterns_detected": [],
  "guidance": "Claude guidance",
  "dispatch_count": 1
}
EOF

cat > "$TMPDIR/scorecard-codex.json" << 'EOF'
{
  "provider": "codex",
  "task_id": "test-task-1",
  "iteration": 1,
  "timestamp": "2026-03-30T00:00:00Z",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 6, "threshold": 7, "evidence": "Missing edge cases"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "Matches spec"},
        "code_quality": {"score": 8, "threshold": 6, "evidence": "Well structured"}
      }
    }
  },
  "criteria": [
    {"id": "c_file_exists", "pass": true, "evidence": "File found"},
    {"id": "c_tests_pass", "pass": false, "evidence": "Edge case test fails"}
  ],
  "antipatterns_detected": [],
  "guidance": "Codex guidance",
  "dispatch_count": 1
}
EOF

# Merge the two scorecards
MERGED_OUTPUT=$( jq -s '.' "$TMPDIR/scorecard-claude.json" "$TMPDIR/scorecard-codex.json" | "$SCRIPT_DIR/merge-scorecards.sh" 2>"$TMPDIR/disagreements-1.json" )

EXIT_CODE=0
echo "$MERGED_OUTPUT" | jq empty 2>/dev/null || EXIT_CODE=$?
assert_exit "merge produces valid JSON" 0 "$EXIT_CODE"

# Verify min scores
assert_json "correctness min score = 6 (min of 9, 6)" ".domains.general.dimensions.correctness.score" "6" "$MERGED_OUTPUT"
assert_json "domain_spec_fidelity min score = 8 (min of 8, 8)" ".domains.general.dimensions.domain_spec_fidelity.score" "8" "$MERGED_OUTPUT"
assert_json "code_quality min score = 7 (min of 7, 8)" ".domains.general.dimensions.code_quality.score" "7" "$MERGED_OUTPUT"

# Verify provider_scores recorded
assert_json "correctness claude provider_score = 9" ".domains.general.dimensions.correctness.provider_scores.claude" "9" "$MERGED_OUTPUT"
assert_json "correctness codex provider_score = 6" ".domains.general.dimensions.correctness.provider_scores.codex" "6" "$MERGED_OUTPUT"
assert_json "code_quality claude provider_score = 7" ".domains.general.dimensions.code_quality.provider_scores.claude" "7" "$MERGED_OUTPUT"
assert_json "code_quality codex provider_score = 8" ".domains.general.dimensions.code_quality.provider_scores.codex" "8" "$MERGED_OUTPUT"

# Verify criteria merge: any fail = fail
assert_json "criteria c_file_exists pass (both pass)" '.criteria[] | select(.id == "c_file_exists") | .pass' "true" "$MERGED_OUTPUT"
assert_json "criteria c_tests_pass fail (one fails)" '.criteria[] | select(.id == "c_tests_pass") | .pass' "false" "$MERGED_OUTPUT"

# Verify dispatch_count merged (1 + 1 = 2)
assert_json "dispatch_count merged = 2" ".dispatch_count" "2" "$MERGED_OUTPUT"

# Verify guidance merged
assert_contains "guidance contains Claude guidance" "Claude guidance" "$MERGED_OUTPUT"
assert_contains "guidance contains Codex guidance" "Codex guidance" "$MERGED_OUTPUT"

# Verify disagreements on correctness (spread = 3, >= 3)
DISAGREE_1=$(cat "$TMPDIR/disagreements-1.json")
assert_json "disagreement found for correctness" ".[0].dimension" "correctness" "$DISAGREE_1"
assert_json "disagreement domain is general" ".[0].domain" "general" "$DISAGREE_1"
assert_json "disagreement spread is 3" ".[0].spread" "3" "$DISAGREE_1"
assert_json "disagreement claude score = 9" ".[0].scores.claude" "9" "$DISAGREE_1"
assert_json "disagreement codex score = 6" ".[0].scores.codex" "6" "$DISAGREE_1"

# No disagreement on domain_spec_fidelity (spread = 0) or code_quality (spread = 1)
DISAGREE_COUNT=$(echo "$DISAGREE_1" | jq 'length')
if [ "$DISAGREE_COUNT" = "1" ]; then
  PASS=$((PASS+1)); echo "  PASS: only 1 disagreement (correctness, spread >= 3)"
else
  FAIL=$((FAIL+1)); echo "  FAIL: expected 1 disagreement, got $DISAGREE_COUNT"
fi

# Check-thresholds on merged scorecard — correctness 6 < 7 should FAIL
echo "$MERGED_OUTPUT" > "$TMPDIR/merged-scorecard-1.json"
jq '.criteria' "$TMPDIR/merged-scorecard-1.json" > "$TMPDIR/merged-criteria-1.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" \
  --scorecard "$TMPDIR/merged-scorecard-1.json" \
  --criteria "$TMPDIR/merged-criteria-1.json" > "$TMPDIR/thresholds-1.json" 2>/dev/null || EXIT_CODE=$?
THRESH_1=$(cat "$TMPDIR/thresholds-1.json")
assert_exit "merged scorecard with correctness=6 → exit 1 (FAIL)" 1 "$EXIT_CODE"
assert_json "verdict is FAIL" ".verdict" "FAIL" "$THRESH_1"
assert_json "failing dimension is correctness" ".failing_dimensions[0].dimension" "correctness" "$THRESH_1"
assert_json "failing domain is general" ".failing_dimensions[0].domain" "general" "$THRESH_1"
assert_json "failing score is 6" ".failing_dimensions[0].score" "6" "$THRESH_1"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 3: Single-element normalization preserves scores (per-task path) ==="
# ═══════════════════════════════════════════════════════════════════════════════

cat > "$TMPDIR/scorecard-single.json" << 'EOF'
{
  "provider": "claude",
  "task_id": "test-task-2",
  "iteration": 1,
  "timestamp": "2026-03-30T00:00:00Z",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 8, "threshold": 7, "evidence": "Solid"},
        "domain_spec_fidelity": {"score": 9, "threshold": 8, "evidence": "Great"},
        "code_quality": {"score": 7, "threshold": 6, "evidence": "Clean"}
      }
    }
  },
  "criteria": [
    {"id": "c_file_exists", "pass": true, "evidence": "Yes"}
  ],
  "antipatterns_detected": [],
  "guidance": "Single provider guidance",
  "dispatch_count": 1
}
EOF

SINGLE_MERGED=$( jq -s '.' "$TMPDIR/scorecard-single.json" | "$SCRIPT_DIR/merge-scorecards.sh" 2>"$TMPDIR/disagreements-single.json" )

# Scores should pass through unchanged
assert_json "single provider: correctness = 8" ".domains.general.dimensions.correctness.score" "8" "$SINGLE_MERGED"
assert_json "single provider: domain_spec_fidelity = 9" ".domains.general.dimensions.domain_spec_fidelity.score" "9" "$SINGLE_MERGED"
assert_json "single provider: code_quality = 7" ".domains.general.dimensions.code_quality.score" "7" "$SINGLE_MERGED"

# provider_scores should still be present with single provider
assert_json "single provider: correctness provider_scores.claude = 8" ".domains.general.dimensions.correctness.provider_scores.claude" "8" "$SINGLE_MERGED"

# No disagreements with single provider
DISAGREE_SINGLE=$(cat "$TMPDIR/disagreements-single.json")
assert_json "single provider: no disagreements" ". | length" "0" "$DISAGREE_SINGLE"

# dispatch_count passthrough
assert_json "single provider: dispatch_count = 1" ".dispatch_count" "1" "$SINGLE_MERGED"

# Check-thresholds should PASS
echo "$SINGLE_MERGED" > "$TMPDIR/single-scorecard.json"
jq '.criteria' "$TMPDIR/single-scorecard.json" > "$TMPDIR/single-criteria.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" \
  --scorecard "$TMPDIR/single-scorecard.json" \
  --criteria "$TMPDIR/single-criteria.json" > "$TMPDIR/thresholds-single.json" 2>/dev/null || EXIT_CODE=$?
THRESH_SINGLE=$(cat "$TMPDIR/thresholds-single.json")
assert_exit "single provider all pass → exit 0" 0 "$EXIT_CODE"
assert_json "single provider verdict is PASS" ".verdict" "PASS" "$THRESH_SINGLE"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 4: Multi-provider + multi-domain merge (generic merge behavior) ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Two providers, two domains (frontend and backend)
cat > "$TMPDIR/scorecard-claude-multi.json" << 'EOF'
{
  "provider": "claude",
  "task_id": "test-task-3",
  "iteration": 1,
  "timestamp": "2026-03-30T00:00:00Z",
  "domains": {
    "frontend": {
      "dimensions": {
        "correctness": {"score": 9, "threshold": 7, "evidence": "UI works"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "Matches design"}
      }
    },
    "backend": {
      "dimensions": {
        "correctness": {"score": 8, "threshold": 7, "evidence": "API works"},
        "domain_spec_fidelity": {"score": 9, "threshold": 8, "evidence": "Matches spec"}
      }
    }
  },
  "criteria": [
    {"id": "fe-layout", "pass": true, "evidence": "Good"},
    {"id": "be-health", "pass": true, "evidence": "Healthy"}
  ],
  "antipatterns_detected": [],
  "guidance": "Claude multi-domain",
  "dispatch_count": 1
}
EOF

cat > "$TMPDIR/scorecard-codex-multi.json" << 'EOF'
{
  "provider": "codex",
  "task_id": "test-task-3",
  "iteration": 1,
  "timestamp": "2026-03-30T00:00:00Z",
  "domains": {
    "frontend": {
      "dimensions": {
        "correctness": {"score": 5, "threshold": 7, "evidence": "Missing features"},
        "domain_spec_fidelity": {"score": 7, "threshold": 8, "evidence": "Close but gaps"}
      }
    },
    "backend": {
      "dimensions": {
        "correctness": {"score": 7, "threshold": 7, "evidence": "Mostly works"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "Matches spec"}
      }
    }
  },
  "criteria": [
    {"id": "fe-layout", "pass": false, "evidence": "Layout broken"},
    {"id": "be-health", "pass": true, "evidence": "Healthy"}
  ],
  "antipatterns_detected": [],
  "guidance": "Codex multi-domain",
  "dispatch_count": 1
}
EOF

MULTI_MERGED=$( jq -s '.' "$TMPDIR/scorecard-claude-multi.json" "$TMPDIR/scorecard-codex-multi.json" | "$SCRIPT_DIR/merge-scorecards.sh" 2>"$TMPDIR/disagreements-multi.json" )

# Verify each domain merged independently
assert_json "multi-domain: frontend.correctness min = 5" ".domains.frontend.dimensions.correctness.score" "5" "$MULTI_MERGED"
assert_json "multi-domain: frontend.domain_spec_fidelity min = 7" ".domains.frontend.dimensions.domain_spec_fidelity.score" "7" "$MULTI_MERGED"
assert_json "multi-domain: backend.correctness min = 7" ".domains.backend.dimensions.correctness.score" "7" "$MULTI_MERGED"
assert_json "multi-domain: backend.domain_spec_fidelity min = 8" ".domains.backend.dimensions.domain_spec_fidelity.score" "8" "$MULTI_MERGED"

# Verify provider_scores per domain
assert_json "multi-domain: frontend.correctness claude = 9" ".domains.frontend.dimensions.correctness.provider_scores.claude" "9" "$MULTI_MERGED"
assert_json "multi-domain: frontend.correctness codex = 5" ".domains.frontend.dimensions.correctness.provider_scores.codex" "5" "$MULTI_MERGED"
assert_json "multi-domain: backend.correctness claude = 8" ".domains.backend.dimensions.correctness.provider_scores.claude" "8" "$MULTI_MERGED"
assert_json "multi-domain: backend.correctness codex = 7" ".domains.backend.dimensions.correctness.provider_scores.codex" "7" "$MULTI_MERGED"

# Criteria: fe-layout should fail (one provider fails it)
assert_json "multi-domain: fe-layout fails" '.criteria[] | select(.id == "fe-layout") | .pass' "false" "$MULTI_MERGED"
assert_json "multi-domain: be-health passes" '.criteria[] | select(.id == "be-health") | .pass' "true" "$MULTI_MERGED"

# Disagreements: frontend.correctness spread = 4 (9 vs 5), frontend.domain_spec_fidelity spread = 1 (no)
# backend.correctness spread = 1 (no), backend.domain_spec_fidelity spread = 1 (no)
DISAGREE_MULTI=$(cat "$TMPDIR/disagreements-multi.json")
assert_json "multi-domain: 1 disagreement (frontend.correctness)" ". | length" "1" "$DISAGREE_MULTI"
assert_json "multi-domain: disagreement domain is frontend" ".[0].domain" "frontend" "$DISAGREE_MULTI"
assert_json "multi-domain: disagreement dimension is correctness" ".[0].dimension" "correctness" "$DISAGREE_MULTI"
assert_json "multi-domain: disagreement spread is 4" ".[0].spread" "4" "$DISAGREE_MULTI"

# Check thresholds on multi-domain merged scorecard
echo "$MULTI_MERGED" > "$TMPDIR/multi-scorecard.json"
jq '.criteria' "$TMPDIR/multi-scorecard.json" > "$TMPDIR/multi-criteria.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" \
  --scorecard "$TMPDIR/multi-scorecard.json" \
  --criteria "$TMPDIR/multi-criteria.json" > "$TMPDIR/thresholds-multi.json" 2>/dev/null || EXIT_CODE=$?
THRESH_MULTI=$(cat "$TMPDIR/thresholds-multi.json")
assert_exit "multi-domain merged: failures → exit 1" 1 "$EXIT_CODE"
assert_json "multi-domain: verdict is FAIL" ".verdict" "FAIL" "$THRESH_MULTI"
# frontend.correctness = 5 < 7, frontend.domain_spec_fidelity = 7 < 8
FAIL_COUNT=$(echo "$THRESH_MULTI" | jq '.failing_dimensions | length')
if [ "$FAIL_COUNT" = "2" ]; then
  PASS=$((PASS+1)); echo "  PASS: multi-domain: 2 failing dimensions (frontend.correctness, frontend.domain_spec_fidelity)"
else
  FAIL=$((FAIL+1)); echo "  FAIL: expected 2 failing dimensions, got $FAIL_COUNT"
fi
# Backend should all pass
assert_json "multi-domain: backend.correctness in dimensions map" '.dimensions["backend.correctness"]' "7" "$THRESH_MULTI"
assert_json "multi-domain: backend.domain_spec_fidelity in dimensions map" '.dimensions["backend.domain_spec_fidelity"]' "8" "$THRESH_MULTI"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 5: Disagreement logging via append-eval-log.sh (holistic) ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Create a disagreement file
cat > "$TMPDIR/disagreements-for-log.json" << 'EOF'
[
  {
    "domain": "general",
    "dimension": "correctness",
    "spread": 3,
    "scores": {"claude": 9, "codex": 6}
  }
]
EOF

# append-eval-log.sh requires a real bean, which we can't create in tests.
# Instead, verify the disagreement file is valid JSON and has correct structure.
DISAGREE_LOG=$(cat "$TMPDIR/disagreements-for-log.json")
assert_json "disagreement log: valid structure" ".[0].domain" "general" "$DISAGREE_LOG"
assert_json "disagreement log: has spread" ".[0].spread" "3" "$DISAGREE_LOG"
assert_json "disagreement log: has scores object" ".[0].scores | keys | length" "2" "$DISAGREE_LOG"

# Verify append-eval-log.sh can parse the disagreement file (dry-run the jq logic)
DISAGREE_SECTION=$(jq -r '
  if length == 0 then "" else
    "\n**Disagreements:**" +
    (map("\n- \(.domain).\(.dimension): spread \(.spread) (" +
      ([.scores | to_entries[] | "\(.key): \(.value)"] | join(", ")) +
    ")") | join(""))
  end
' "$TMPDIR/disagreements-for-log.json")
assert_contains "disagreement section has header" "Disagreements" "$DISAGREE_SECTION"
assert_contains "disagreement section has domain.dimension" "general.correctness" "$DISAGREE_SECTION"
assert_contains "disagreement section has spread" "spread 3" "$DISAGREE_SECTION"
assert_contains "disagreement section has claude score" "claude: 9" "$DISAGREE_SECTION"
assert_contains "disagreement section has codex score" "codex: 6" "$DISAGREE_SECTION"

# Verify empty disagreements produce empty string
cat > "$TMPDIR/disagreements-empty.json" << 'EOF'
[]
EOF
EMPTY_SECTION=$(jq -r '
  if length == 0 then "" else
    "\n**Disagreements:**" +
    (map("\n- \(.domain).\(.dimension): spread \(.spread)") | join(""))
  end
' "$TMPDIR/disagreements-empty.json")
if [ -z "$EMPTY_SECTION" ]; then
  PASS=$((PASS+1)); echo "  PASS: empty disagreements produce empty string"
else
  FAIL=$((FAIL+1)); echo "  FAIL: empty disagreements should produce empty string, got '$EMPTY_SECTION'"
fi

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 6: Holistic dispatch budget accounting ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Holistic review dispatches every configured provider each iteration; each
# provider dispatch counts 1 toward the budget, so 2 providers means
# 2 dispatches per holistic iteration.

# Simulate: holistic iteration 1 with 2 provider dispatches
echo "$THRESH_1" > "$TMPDIR/convergence-current-1.json"
echo "[]" > "$TMPDIR/convergence-history.json"
EXIT_CODE=0
CONV_1=$("$SCRIPT_DIR/check-convergence.sh" \
  --current "$TMPDIR/convergence-current-1.json" \
  --history "$TMPDIR/convergence-history.json" \
  --max-dispatches 10 \
  --current-dispatches 2) || EXIT_CODE=$?
assert_exit "iteration 1 FAIL → exit 1" 1 "$EXIT_CODE"
assert_json "convergence status is FAIL" ".status" "FAIL" "$CONV_1"

# Simulate: holistic iteration 2 with cumulative 4 dispatches (2 per iteration)
echo "$THRESH_SINGLE" > "$TMPDIR/convergence-current-2.json"
jq -n --argjson t1 "$(cat "$TMPDIR/convergence-current-1.json")" '[$t1]' > "$TMPDIR/convergence-history-2.json"
EXIT_CODE=0
CONV_2=$("$SCRIPT_DIR/check-convergence.sh" \
  --current "$TMPDIR/convergence-current-2.json" \
  --history "$TMPDIR/convergence-history-2.json" \
  --max-dispatches 10 \
  --current-dispatches 4) || EXIT_CODE=$?
assert_exit "iteration 2 first PASS → PASS_PENDING exit 1" 1 "$EXIT_CODE"
assert_json "convergence status is PASS_PENDING" ".status" "PASS_PENDING" "$CONV_2"

# Budget exceeded: dispatches >= max
EXIT_CODE=0
CONV_BUDGET=$("$SCRIPT_DIR/check-convergence.sh" \
  --current "$TMPDIR/convergence-current-2.json" \
  --history "$TMPDIR/convergence-history-2.json" \
  --max-dispatches 4 \
  --current-dispatches 4) || EXIT_CODE=$?
assert_exit "budget exceeded → exit 2" 2 "$EXIT_CODE"
assert_json "budget exceeded status" ".status" "DISPATCHES_EXCEEDED" "$CONV_BUDGET"
assert_json "budget exceeded dispatches = 4" ".dispatches" "4" "$CONV_BUDGET"
assert_json "budget exceeded budget = 4" ".budget" "4" "$CONV_BUDGET"

# Under budget: dispatches < max
EXIT_CODE=0
CONV_UNDER=$("$SCRIPT_DIR/check-convergence.sh" \
  --current "$TMPDIR/convergence-current-2.json" \
  --history "$TMPDIR/convergence-history-2.json" \
  --max-dispatches 10 \
  --current-dispatches 4) || EXIT_CODE=$?
assert_exit "under budget → not exit 2" 1 "$EXIT_CODE"
# Should be PASS_PENDING, not DISPATCHES_EXCEEDED
assert_json "under budget: status is not DISPATCHES_EXCEEDED" ".status" "PASS_PENDING" "$CONV_UNDER"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 7: Holistic convergence with multi-provider — FAIL → PASS_PENDING → CONVERGED ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Iteration 1: FAIL (correctness below threshold after merge)
# We already have THRESH_1 (FAIL verdict) from Test 1

# Iteration 1 convergence check
echo "$THRESH_1" > "$TMPDIR/conv-iter1-current.json"
echo "[]" > "$TMPDIR/conv-history.json"
EXIT_CODE=0
CONV_ITER1=$("$SCRIPT_DIR/check-convergence.sh" \
  --current "$TMPDIR/conv-iter1-current.json" \
  --history "$TMPDIR/conv-history.json" \
  --max-dispatches 20 \
  --current-dispatches 2) || EXIT_CODE=$?
assert_exit "convergence iter 1: FAIL → exit 1" 1 "$EXIT_CODE"
assert_json "convergence iter 1: status FAIL" ".status" "FAIL" "$CONV_ITER1"

# Iteration 2: Both providers now agree, all pass
cat > "$TMPDIR/scorecard-claude-iter2.json" << 'EOF'
{
  "provider": "claude",
  "task_id": "test-task-1",
  "iteration": 2,
  "timestamp": "2026-03-30T00:01:00Z",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 8, "threshold": 7, "evidence": "Fixed edge cases"},
        "domain_spec_fidelity": {"score": 9, "threshold": 8, "evidence": "Fully matches"},
        "code_quality": {"score": 8, "threshold": 6, "evidence": "Improved"}
      }
    }
  },
  "criteria": [
    {"id": "c_file_exists", "pass": true, "evidence": "File found"},
    {"id": "c_tests_pass", "pass": true, "evidence": "All green"}
  ],
  "antipatterns_detected": [],
  "guidance": "",
  "dispatch_count": 1
}
EOF

cat > "$TMPDIR/scorecard-codex-iter2.json" << 'EOF'
{
  "provider": "codex",
  "task_id": "test-task-1",
  "iteration": 2,
  "timestamp": "2026-03-30T00:01:00Z",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 7, "threshold": 7, "evidence": "Edge cases handled"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "Matches spec"},
        "code_quality": {"score": 7, "threshold": 6, "evidence": "Clean"}
      }
    }
  },
  "criteria": [
    {"id": "c_file_exists", "pass": true, "evidence": "File found"},
    {"id": "c_tests_pass", "pass": true, "evidence": "All green"}
  ],
  "antipatterns_detected": [],
  "guidance": "",
  "dispatch_count": 1
}
EOF

MERGED_ITER2=$( jq -s '.' "$TMPDIR/scorecard-claude-iter2.json" "$TMPDIR/scorecard-codex-iter2.json" | "$SCRIPT_DIR/merge-scorecards.sh" 2>"$TMPDIR/disagreements-iter2.json" )
echo "$MERGED_ITER2" > "$TMPDIR/merged-scorecard-iter2.json"
jq '.criteria' "$TMPDIR/merged-scorecard-iter2.json" > "$TMPDIR/merged-criteria-iter2.json"

# Check thresholds — should PASS now
EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" \
  --scorecard "$TMPDIR/merged-scorecard-iter2.json" \
  --criteria "$TMPDIR/merged-criteria-iter2.json" > "$TMPDIR/thresholds-iter2.json" 2>/dev/null || EXIT_CODE=$?
THRESH_ITER2=$(cat "$TMPDIR/thresholds-iter2.json")
assert_exit "iter 2 thresholds PASS → exit 0" 0 "$EXIT_CODE"
assert_json "iter 2 verdict is PASS" ".verdict" "PASS" "$THRESH_ITER2"

# No disagreements in iter 2 (max spread = 1)
DISAGREE_ITER2=$(cat "$TMPDIR/disagreements-iter2.json")
assert_json "iter 2: no disagreements" ". | length" "0" "$DISAGREE_ITER2"

# Convergence check: iteration 2 — first PASS → PASS_PENDING
jq -n --argjson t1 "$(cat "$TMPDIR/thresholds-iter2.json")" "[\$t1]" > /dev/null 2>&1
jq -n --argjson t1 "$(cat "$TMPDIR/conv-iter1-current.json")" '[$t1]' > "$TMPDIR/conv-history-2.json"
EXIT_CODE=0
CONV_ITER2=$("$SCRIPT_DIR/check-convergence.sh" \
  --current "$TMPDIR/thresholds-iter2.json" \
  --history "$TMPDIR/conv-history-2.json" \
  --max-dispatches 20 \
  --current-dispatches 4) || EXIT_CODE=$?
assert_exit "convergence iter 2: first PASS → PASS_PENDING exit 1" 1 "$EXIT_CODE"
assert_json "convergence iter 2: status PASS_PENDING" ".status" "PASS_PENDING" "$CONV_ITER2"

# Iteration 3: Second consecutive PASS → CONVERGED
# Simulate iteration 3 with same-ish passing scores
cat > "$TMPDIR/scorecard-claude-iter3.json" << 'EOF'
{
  "provider": "claude",
  "task_id": "test-task-1",
  "iteration": 3,
  "timestamp": "2026-03-30T00:02:00Z",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 8, "threshold": 7, "evidence": "Stable"},
        "domain_spec_fidelity": {"score": 9, "threshold": 8, "evidence": "Stable"},
        "code_quality": {"score": 8, "threshold": 6, "evidence": "Stable"}
      }
    }
  },
  "criteria": [
    {"id": "c_file_exists", "pass": true, "evidence": "Yes"},
    {"id": "c_tests_pass", "pass": true, "evidence": "Yes"}
  ],
  "antipatterns_detected": [],
  "guidance": "",
  "dispatch_count": 1
}
EOF

cat > "$TMPDIR/scorecard-codex-iter3.json" << 'EOF'
{
  "provider": "codex",
  "task_id": "test-task-1",
  "iteration": 3,
  "timestamp": "2026-03-30T00:02:00Z",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": {"score": 8, "threshold": 7, "evidence": "Stable"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "Stable"},
        "code_quality": {"score": 7, "threshold": 6, "evidence": "Stable"}
      }
    }
  },
  "criteria": [
    {"id": "c_file_exists", "pass": true, "evidence": "Yes"},
    {"id": "c_tests_pass", "pass": true, "evidence": "Yes"}
  ],
  "antipatterns_detected": [],
  "guidance": "",
  "dispatch_count": 1
}
EOF

MERGED_ITER3=$( jq -s '.' "$TMPDIR/scorecard-claude-iter3.json" "$TMPDIR/scorecard-codex-iter3.json" | "$SCRIPT_DIR/merge-scorecards.sh" 2>/dev/null )
echo "$MERGED_ITER3" > "$TMPDIR/merged-scorecard-iter3.json"
jq '.criteria' "$TMPDIR/merged-scorecard-iter3.json" > "$TMPDIR/merged-criteria-iter3.json"

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" \
  --scorecard "$TMPDIR/merged-scorecard-iter3.json" \
  --criteria "$TMPDIR/merged-criteria-iter3.json" > "$TMPDIR/thresholds-iter3.json" 2>/dev/null || EXIT_CODE=$?
THRESH_ITER3=$(cat "$TMPDIR/thresholds-iter3.json")
assert_exit "iter 3 thresholds PASS → exit 0" 0 "$EXIT_CODE"

# Build history with iter 1 (FAIL) and iter 2 (PASS)
jq -s '.' "$TMPDIR/conv-iter1-current.json" "$TMPDIR/thresholds-iter2.json" > "$TMPDIR/conv-history-3.json"

EXIT_CODE=0
CONV_ITER3=$("$SCRIPT_DIR/check-convergence.sh" \
  --current "$TMPDIR/thresholds-iter3.json" \
  --history "$TMPDIR/conv-history-3.json" \
  --max-dispatches 20 \
  --current-dispatches 6) || EXIT_CODE=$?
assert_exit "convergence iter 3: two consecutive PASS → CONVERGED exit 0" 0 "$EXIT_CODE"
assert_json "convergence iter 3: status CONVERGED" ".status" "CONVERGED" "$CONV_ITER3"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
