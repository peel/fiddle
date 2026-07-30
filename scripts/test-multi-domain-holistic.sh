#!/usr/bin/env bash
# test-multi-domain-holistic.sh — Integration test: multi-domain evaluation + holistic review
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
echo "=== Test 1: resolve-domains.sh — frontend+backend from config ==="
# ═══════════════════════════════════════════════════════════════════════════════

cat > "$TMPDIR/orchestrate.json" << 'EOF'
{
  "evaluators": {
    "domains": {
      "general": {
        "template": "evaluator-general",
        "providers": ["claude"]
      },
      "frontend": {
        "template": "evaluator-frontend",
        "providers": ["claude"],
        "runtime": ["python3 -m http.server 8080"],
        "ready_check": {
          "type": "tcp",
          "port": 8080,
          "timeout_ms": 15000
        }
      },
      "backend": {
        "template": "evaluator-backend",
        "providers": ["claude", "codex"],
        "runtime": ["python3 -m http.server 9090"],
        "ready_check": {
          "type": "http",
          "url": "http://localhost:9090/health",
          "timeout_ms": 10000
        }
      }
    }
  }
}
EOF

EXIT_CODE=0
OUTPUT=$("$SCRIPT_DIR/resolve-domains.sh" --domains "frontend,backend" --config "$TMPDIR/orchestrate.json" 2>/dev/null) || EXIT_CODE=$?
assert_exit "resolve frontend+backend → exit 0" 0 "$EXIT_CODE"
assert_json "returns 2 domains" ". | length" "2" "$OUTPUT"
# resolve-domains.sh sorts alphabetically via jq unique
assert_json "first domain is backend (sorted)" ".[0].domain" "backend" "$OUTPUT"
assert_json "backend template correct" ".[0].template" "evaluator-backend" "$OUTPUT"
assert_json "backend resolved_via is config" ".[0].resolved_via" "config" "$OUTPUT"
assert_json "backend has 2 providers" ".[0].providers | length" "2" "$OUTPUT"
assert_json "backend has runtime" ".[0].runtime[0]" "python3 -m http.server 9090" "$OUTPUT"
assert_json "backend ready_check type" ".[0].ready_check.type" "http" "$OUTPUT"
assert_json "second domain is frontend (sorted)" ".[1].domain" "frontend" "$OUTPUT"
assert_json "frontend template correct" ".[1].template" "evaluator-frontend" "$OUTPUT"
assert_json "frontend resolved_via is config" ".[1].resolved_via" "config" "$OUTPUT"
assert_json "frontend has runtime" ".[1].runtime[0]" "python3 -m http.server 8080" "$OUTPUT"
assert_json "frontend ready_check type" ".[1].ready_check.type" "tcp" "$OUTPUT"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 2: Cross-domain scorecard merge — union of frontend+backend ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Create per-domain scorecards as evaluators would produce them
cat > "$TMPDIR/scorecard-frontend.json" << 'EOF'
{
  "domains": {
    "frontend": {
      "dimensions": {
        "correctness": {"score": 8, "threshold": 7, "evidence": "UI renders correctly"},
        "domain_spec_fidelity": {"score": 9, "threshold": 8, "evidence": "Matches mockups"},
        "code_quality": {"score": 7, "threshold": 6, "evidence": "Clean component structure"}
      }
    }
  },
  "criteria": [
    {"id": "fe-layout-matches", "pass": true},
    {"id": "fe-responsive", "pass": true}
  ]
}
EOF

cat > "$TMPDIR/scorecard-backend.json" << 'EOF'
{
  "domains": {
    "backend": {
      "dimensions": {
        "correctness": {"score": 9, "threshold": 7, "evidence": "All endpoints return expected data"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "API matches spec"},
        "code_quality": {"score": 8, "threshold": 6, "evidence": "Clean handler structure"}
      }
    }
  },
  "criteria": [
    {"id": "be-health-endpoint", "pass": true},
    {"id": "be-crud-operations", "pass": true}
  ]
}
EOF

# Merge using the jq command from SKILL.md step 1g
MERGED=$(jq -s '
  { domains: (reduce .[] as $s ({}; . + ($s.domains // {}))) ,
    criteria: [.[] | .criteria[]?] }
' "$TMPDIR/scorecard-frontend.json" "$TMPDIR/scorecard-backend.json")
echo "$MERGED" > "$TMPDIR/scorecard.json"

assert_json "merged has frontend domain" ".domains.frontend | type" "object" "$MERGED"
assert_json "merged has backend domain" ".domains.backend | type" "object" "$MERGED"
assert_json "merged frontend.correctness score" ".domains.frontend.dimensions.correctness.score" "8" "$MERGED"
assert_json "merged backend.correctness score" ".domains.backend.dimensions.correctness.score" "9" "$MERGED"
assert_json "merged criteria count is 4" ".criteria | length" "4" "$MERGED"
assert_json "first criterion from frontend" ".criteria[0].id" "fe-layout-matches" "$MERGED"
assert_json "third criterion from backend" ".criteria[2].id" "be-health-endpoint" "$MERGED"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 3: check-thresholds.sh — merged multi-domain scorecard PASS ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Extract merged criteria
jq '.criteria' "$TMPDIR/scorecard.json" > "$TMPDIR/criteria.json"

OUTFILE="$TMPDIR/thresholds-out.json"
EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/scorecard.json" --criteria "$TMPDIR/criteria.json" > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
THRESH_OUTPUT=$(cat "$OUTFILE")
assert_exit "all domains pass thresholds → exit 0" 0 "$EXIT_CODE"
assert_json "verdict is PASS" ".verdict" "PASS" "$THRESH_OUTPUT"
assert_json "dimensions has frontend.correctness" '.dimensions["frontend.correctness"]' "8" "$THRESH_OUTPUT"
assert_json "dimensions has frontend.domain_spec_fidelity" '.dimensions["frontend.domain_spec_fidelity"]' "9" "$THRESH_OUTPUT"
assert_json "dimensions has backend.correctness" '.dimensions["backend.correctness"]' "9" "$THRESH_OUTPUT"
assert_json "dimensions has backend.domain_spec_fidelity" '.dimensions["backend.domain_spec_fidelity"]' "8" "$THRESH_OUTPUT"
assert_json "no failing dimensions" ".failing_dimensions | length" "0" "$THRESH_OUTPUT"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 4: check-thresholds.sh — one domain fails, other passes ==="
# ═══════════════════════════════════════════════════════════════════════════════

cat > "$TMPDIR/scorecard-partial-fail.json" << 'EOF'
{
  "domains": {
    "frontend": {
      "dimensions": {
        "correctness": {"score": 5, "threshold": 7, "evidence": "Missing key features"},
        "domain_spec_fidelity": {"score": 9, "threshold": 8, "evidence": "Matches mockups"}
      }
    },
    "backend": {
      "dimensions": {
        "correctness": {"score": 9, "threshold": 7, "evidence": "All endpoints work"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "API matches spec"}
      }
    }
  },
  "criteria": [{"id": "crit-1", "pass": true}]
}
EOF
jq '.criteria' "$TMPDIR/scorecard-partial-fail.json" > "$TMPDIR/criteria-partial-fail.json"

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" \
  --scorecard "$TMPDIR/scorecard-partial-fail.json" \
  --criteria "$TMPDIR/criteria-partial-fail.json" > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
THRESH_OUTPUT=$(cat "$OUTFILE")
assert_exit "one domain fails → exit 1" 1 "$EXIT_CODE"
assert_json "verdict is FAIL" ".verdict" "FAIL" "$THRESH_OUTPUT"
assert_json "1 failing dimension" ".failing_dimensions | length" "1" "$THRESH_OUTPUT"
assert_json "failing dimension is correctness" ".failing_dimensions[0].dimension" "correctness" "$THRESH_OUTPUT"
assert_json "failing domain is frontend" ".failing_dimensions[0].domain" "frontend" "$THRESH_OUTPUT"
assert_json "failing score is 5" ".failing_dimensions[0].score" "5" "$THRESH_OUTPUT"
# Backend dimensions still present and correct
assert_json "backend.correctness still in dimensions map" '.dimensions["backend.correctness"]' "9" "$THRESH_OUTPUT"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 5: Holistic review file — dimensions exist and have correct thresholds ==="
# ═══════════════════════════════════════════════════════════════════════════════

HOLISTIC_FILE="$PROJECT_ROOT/skills/develop/holistic-review.md"
DIMENSIONS_FILE="$PROJECT_ROOT/skills/develop/holistic-dimensions.md"
SCHEMA_FILE="$PROJECT_ROOT/skills/develop/holistic-scorecard-schema.md"
assert_file_exists "holistic-review.md exists" "$HOLISTIC_FILE"
assert_file_exists "holistic-dimensions.md exists" "$DIMENSIONS_FILE"
assert_file_exists "holistic-scorecard-schema.md exists" "$SCHEMA_FILE"

HOLISTIC_CONTENT=$(cat "$HOLISTIC_FILE" "$DIMENSIONS_FILE" "$SCHEMA_FILE")

# Check all 5 holistic dimensions are present
assert_contains "holistic has Integration dimension" "## Integration" "$HOLISTIC_CONTENT"
assert_contains "holistic has Coherence dimension" "## Coherence" "$HOLISTIC_CONTENT"
assert_contains "holistic has Holistic Spec Fidelity dimension" "## Holistic Spec Fidelity" "$HOLISTIC_CONTENT"
assert_contains "holistic has Polish dimension" "## Polish" "$HOLISTIC_CONTENT"
assert_contains "holistic has Runtime Health dimension" "## Runtime Health" "$HOLISTIC_CONTENT"

# Check thresholds
assert_contains "Integration threshold: 7" "Default threshold: 7" "$HOLISTIC_CONTENT"
assert_contains "Holistic Spec Fidelity threshold: 8" "Default threshold: 8" "$HOLISTIC_CONTENT"
assert_contains "Polish threshold: 6" "Default threshold: 6" "$HOLISTIC_CONTENT"
assert_contains "Runtime Health threshold: 9" "Default threshold: 9" "$HOLISTIC_CONTENT"

# Check the runtime-interaction invariant is stated (was: HARD-GATE markup, removed in the
# Claude-5 slim-down; the invariant it guarded is what matters)
assert_contains "holistic states runtime-before-scoring" "before scoring any holistic dimension" "$HOLISTIC_CONTENT"

# Check spec coverage matrix section
assert_contains "holistic has spec_coverage_matrix" "spec_coverage_matrix" "$HOLISTIC_CONTENT"

# Check remediation beans section
assert_contains "holistic has remediation_beans" "remediation_beans" "$HOLISTIC_CONTENT"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 6: Holistic scorecard passes check-thresholds.sh ==="
# ═══════════════════════════════════════════════════════════════════════════════

cat > "$TMPDIR/scorecard-holistic.json" << 'EOF'
{
  "domains": {
    "holistic": {
      "dimensions": {
        "integration": {"score": 8, "threshold": 7, "evidence": "Cross-domain flows work"},
        "coherence": {"score": 7, "threshold": 7, "evidence": "Unified design language"},
        "holistic_spec_fidelity": {"score": 9, "threshold": 8, "evidence": "Spec fully realized"},
        "polish": {"score": 7, "threshold": 6, "evidence": "Presentable quality"},
        "runtime_health": {"score": 9, "threshold": 9, "evidence": "Clean startup, zero errors"}
      }
    }
  },
  "criteria": []
}
EOF
cat > "$TMPDIR/criteria-holistic.json" << 'EOF'
[]
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" \
  --scorecard "$TMPDIR/scorecard-holistic.json" \
  --criteria "$TMPDIR/criteria-holistic.json" > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
THRESH_OUTPUT=$(cat "$OUTFILE")
assert_exit "holistic all pass → exit 0" 0 "$EXIT_CODE"
assert_json "holistic verdict is PASS" ".verdict" "PASS" "$THRESH_OUTPUT"
assert_json "dimensions has holistic.integration" '.dimensions["holistic.integration"]' "8" "$THRESH_OUTPUT"
assert_json "dimensions has holistic.coherence" '.dimensions["holistic.coherence"]' "7" "$THRESH_OUTPUT"
assert_json "dimensions has holistic.holistic_spec_fidelity" '.dimensions["holistic.holistic_spec_fidelity"]' "9" "$THRESH_OUTPUT"
assert_json "dimensions has holistic.polish" '.dimensions["holistic.polish"]' "7" "$THRESH_OUTPUT"
assert_json "dimensions has holistic.runtime_health" '.dimensions["holistic.runtime_health"]' "9" "$THRESH_OUTPUT"
assert_json "no failing dimensions" ".failing_dimensions | length" "0" "$THRESH_OUTPUT"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 7: Holistic scorecard FAIL — dimension below threshold ==="
# ═══════════════════════════════════════════════════════════════════════════════

cat > "$TMPDIR/scorecard-holistic-fail.json" << 'EOF'
{
  "domains": {
    "holistic": {
      "dimensions": {
        "integration": {"score": 8, "threshold": 7, "evidence": "Good integration"},
        "coherence": {"score": 7, "threshold": 7, "evidence": "Unified"},
        "holistic_spec_fidelity": {"score": 6, "threshold": 8, "evidence": "Major gaps remain"},
        "polish": {"score": 7, "threshold": 6, "evidence": "Good enough"},
        "runtime_health": {"score": 5, "threshold": 9, "evidence": "Console errors present"}
      }
    }
  },
  "criteria": []
}
EOF

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" \
  --scorecard "$TMPDIR/scorecard-holistic-fail.json" \
  --criteria "$TMPDIR/criteria-holistic.json" > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
THRESH_OUTPUT=$(cat "$OUTFILE")
assert_exit "holistic dimension fails → exit 1" 1 "$EXIT_CODE"
assert_json "holistic verdict is FAIL" ".verdict" "FAIL" "$THRESH_OUTPUT"
assert_json "2 failing dimensions" ".failing_dimensions | length" "2" "$THRESH_OUTPUT"
# Failing dimensions should include holistic_spec_fidelity and runtime_health
FAILING_DIMS=$(echo "$THRESH_OUTPUT" | jq -r '[.failing_dimensions[].dimension] | sort | join(",")')
if [ "$FAILING_DIMS" = "holistic_spec_fidelity,runtime_health" ]; then
  PASS=$((PASS+1)); echo "  PASS: correct failing dimensions (holistic_spec_fidelity, runtime_health)"
else
  FAIL=$((FAIL+1)); echo "  FAIL: wrong failing dimensions (expected 'holistic_spec_fidelity,runtime_health', got '$FAILING_DIMS')"
fi

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 8: Spec coverage matrix and remediation beans structure ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Verify that a holistic scorecard with spec_coverage_matrix and remediation_beans
# can be parsed and validated structurally
cat > "$TMPDIR/holistic-full.json" << 'EOF'
{
  "domain": "holistic",
  "dimensions": {
    "integration": {"score": 7, "threshold": 7, "evidence": "Works"},
    "coherence": {"score": 7, "threshold": 7, "evidence": "Unified"},
    "holistic_spec_fidelity": {"score": 7, "threshold": 8, "evidence": "Gaps remain"},
    "polish": {"score": 6, "threshold": 6, "evidence": "OK"},
    "runtime_health": {"score": 9, "threshold": 9, "evidence": "Clean"}
  },
  "spec_coverage_matrix": [
    {"requirement": "Login screen", "coverage": "Full", "evidence": "Screenshot shows login form"},
    {"requirement": "Dashboard layout", "coverage": "Weak", "evidence": "Layout exists but spacing off"},
    {"requirement": "Export feature", "coverage": "Missing", "evidence": "Not implemented"}
  ],
  "remediation_beans": [
    {
      "title": "Fix: Export feature not implemented",
      "description": "The design spec requires an export feature. No evidence found.",
      "source": "spec_coverage:Missing",
      "eval": {
        "criteria": [
          {"id": "export_visible", "description": "Export button visible on dashboard", "threshold": 8}
        ]
      }
    },
    {
      "title": "Fix: Holistic Spec Fidelity below threshold (scored 7, needs 8)",
      "description": "Overall spec vision not fully realized.",
      "source": "dimension:holistic_spec_fidelity",
      "eval": {
        "criteria": [
          {"id": "spec_gap_addressed", "description": "Remaining spec gaps addressed", "threshold": 8}
        ]
      }
    }
  ]
}
EOF

FULL_HOLISTIC=$(cat "$TMPDIR/holistic-full.json")

# Validate spec_coverage_matrix structure
assert_json "spec_coverage_matrix has 3 entries" ".spec_coverage_matrix | length" "3" "$FULL_HOLISTIC"
assert_json "first coverage is Full" ".spec_coverage_matrix[0].coverage" "Full" "$FULL_HOLISTIC"
assert_json "second coverage is Weak" ".spec_coverage_matrix[1].coverage" "Weak" "$FULL_HOLISTIC"
assert_json "third coverage is Missing" ".spec_coverage_matrix[2].coverage" "Missing" "$FULL_HOLISTIC"
assert_json "each entry has requirement" ".spec_coverage_matrix[0].requirement" "Login screen" "$FULL_HOLISTIC"
assert_json "each entry has evidence" ".spec_coverage_matrix[0].evidence" "Screenshot shows login form" "$FULL_HOLISTIC"

# Validate remediation_beans structure
assert_json "remediation_beans has 2 entries" ".remediation_beans | length" "2" "$FULL_HOLISTIC"
assert_json "first remediation has title starting with Fix:" '.remediation_beans[0].title | startswith("Fix:")' "true" "$FULL_HOLISTIC"
assert_json "first remediation has source" ".remediation_beans[0].source" "spec_coverage:Missing" "$FULL_HOLISTIC"
assert_json "second remediation has dimension source" ".remediation_beans[1].source" "dimension:holistic_spec_fidelity" "$FULL_HOLISTIC"
assert_json "remediation has eval block" ".remediation_beans[0].eval.criteria | length" "1" "$FULL_HOLISTIC"
assert_json "remediation criterion has id" ".remediation_beans[0].eval.criteria[0].id" "export_visible" "$FULL_HOLISTIC"

# Verify Missing entries map to remediation beans
MISSING_COUNT=$(echo "$FULL_HOLISTIC" | jq '[.spec_coverage_matrix[] | select(.coverage == "Missing")] | length')
MISSING_REMEDIATIONS=$(echo "$FULL_HOLISTIC" | jq '[.remediation_beans[] | select(.source | startswith("spec_coverage:"))] | length')
if [ "$MISSING_COUNT" = "$MISSING_REMEDIATIONS" ]; then
  PASS=$((PASS+1)); echo "  PASS: every Missing coverage entry has a remediation bean"
else
  FAIL=$((FAIL+1)); echo "  FAIL: Missing count ($MISSING_COUNT) != remediation count ($MISSING_REMEDIATIONS)"
fi

# Verify Weak entries do NOT generate remediation beans
WEAK_REMEDIATIONS=$(echo "$FULL_HOLISTIC" | jq '[.remediation_beans[] | select(.source == "spec_coverage:Weak")] | length')
if [ "$WEAK_REMEDIATIONS" = "0" ]; then
  PASS=$((PASS+1)); echo "  PASS: Weak entries do not generate remediation beans"
else
  FAIL=$((FAIL+1)); echo "  FAIL: Weak entries should not have remediation beans (found $WEAK_REMEDIATIONS)"
fi

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 9: SKILL.md structural verification ==="
# ═══════════════════════════════════════════════════════════════════════════════

DEVELOP_SKILL="$PROJECT_ROOT/skills/develop/SKILL.md"
DEVELOP_LOOP_SKILL="$PROJECT_ROOT/skills/develop-loop/SKILL.md"
DEVELOP_HOLISTIC_SKILL="$PROJECT_ROOT/skills/develop-holistic/SKILL.md"
assert_file_exists "develop SKILL.md exists" "$DEVELOP_SKILL"
assert_file_exists "develop-loop SKILL.md exists" "$DEVELOP_LOOP_SKILL"
assert_file_exists "develop-holistic SKILL.md exists" "$DEVELOP_HOLISTIC_SKILL"
SKILL_CONTENT=$(cat "$DEVELOP_SKILL" "$DEVELOP_LOOP_SKILL" "$DEVELOP_HOLISTIC_SKILL" "$HOLISTIC_FILE")

# Step 3: Holistic Review
assert_contains "SKILL.md has Step 3: Holistic Review" "## Step 3: Holistic Review" "$SKILL_CONTENT"

# References resolve-domains.sh
assert_contains "SKILL.md references resolve-domains.sh" "resolve-domains.sh" "$SKILL_CONTENT"

# References holistic-review.md
assert_contains "SKILL.md references holistic-review.md" "holistic-review.md" "$SKILL_CONTENT"

# Remediation loop documentation (Step 2d)
assert_contains "SKILL.md has remediation handling" "## 2d. Handle Remediation" "$SKILL_CONTENT"
assert_contains "SKILL.md mentions remediation_beans" "remediation_beans" "$SKILL_CONTENT"

# runtime_order handling
assert_contains "SKILL.md references runtime_order" "runtime_order" "$SKILL_CONTENT"

# Holistic thresholds documented
assert_contains "SKILL.md documents holistic thresholds" "Integration: 7" "$SKILL_CONTENT"
assert_contains "SKILL.md documents coherence threshold" "Coherence: 7" "$SKILL_CONTENT"
assert_contains "SKILL.md documents spec fidelity threshold" "Holistic Spec Fidelity: 8" "$SKILL_CONTENT"
assert_contains "SKILL.md documents polish threshold" "Polish: 6" "$SKILL_CONTENT"
assert_contains "SKILL.md documents runtime health threshold" "Runtime Health: 9" "$SKILL_CONTENT"

# Holistic convergence protocol
assert_contains "SKILL.md documents holistic convergence" "check-convergence.sh" "$SKILL_CONTENT"
assert_contains "SKILL.md documents holistic dispatch budget" "max_iterations" "$SKILL_CONTENT"

# HARD-GATE for runtime start before holistic
assert_contains "SKILL.md states runtimes-before-holistic" "Every domain runtime must be running before holistic review" "$SKILL_CONTENT"

# Stop runtimes after holistic
assert_contains "SKILL.md documents runtime stop after holistic" "## 2e. Stop Runtimes" "$SKILL_CONTENT"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 10: Evaluator template files exist for frontend and backend ==="
# ═══════════════════════════════════════════════════════════════════════════════

assert_file_exists "evaluator-frontend.md exists" "$PROJECT_ROOT/skills/evaluate/evaluator-frontend.md"
assert_file_exists "evaluator-backend.md exists" "$PROJECT_ROOT/skills/evaluate/evaluator-backend.md"
assert_file_exists "evaluator-general.md exists" "$PROJECT_ROOT/skills/evaluate/evaluator-general.md"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 11: End-to-end — resolve, evaluate per-domain, merge, check ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Simulate the full multi-domain evaluation flow:
# 1. Resolve domains
# 2. Create per-domain scorecards
# 3. Merge scorecards
# 4. Run check-thresholds on the merged result

# Step 1: Resolve
RESOLVED=$("$SCRIPT_DIR/resolve-domains.sh" --domains "frontend,backend" --config "$TMPDIR/orchestrate.json" 2>/dev/null)
DOMAIN_COUNT=$(echo "$RESOLVED" | jq '. | length')
assert_json "e2e: resolved 2 domains" ". | length" "2" "$RESOLVED"

# Step 2: Create per-domain scorecards (simulating evaluator output)
cat > "$TMPDIR/e2e-scorecard-backend.json" << 'EOF'
{
  "domains": {
    "backend": {
      "dimensions": {
        "correctness": {"score": 8, "threshold": 7, "evidence": "API tests pass"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "Endpoints match spec"},
        "code_quality": {"score": 7, "threshold": 6, "evidence": "Clean Go code"}
      }
    }
  },
  "criteria": [
    {"id": "be-health", "pass": true},
    {"id": "be-crud", "pass": true}
  ]
}
EOF

cat > "$TMPDIR/e2e-scorecard-frontend.json" << 'EOF'
{
  "domains": {
    "frontend": {
      "dimensions": {
        "correctness": {"score": 7, "threshold": 7, "evidence": "UI renders"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "Matches design"},
        "code_quality": {"score": 8, "threshold": 6, "evidence": "Good component structure"}
      }
    }
  },
  "criteria": [
    {"id": "fe-layout", "pass": true},
    {"id": "fe-responsive", "pass": true}
  ]
}
EOF

# Step 3: Merge using the exact SKILL.md jq command
jq -s '
  { domains: (reduce .[] as $s ({}; . + ($s.domains // {}))) ,
    criteria: [.[] | .criteria[]?] }
' "$TMPDIR/e2e-scorecard-backend.json" "$TMPDIR/e2e-scorecard-frontend.json" > "$TMPDIR/e2e-scorecard.json"
jq '.criteria' "$TMPDIR/e2e-scorecard.json" > "$TMPDIR/e2e-criteria.json"

E2E_MERGED=$(cat "$TMPDIR/e2e-scorecard.json")
assert_json "e2e: merged has both domains" ".domains | keys | length" "2" "$E2E_MERGED"
assert_json "e2e: merged criteria count" ".criteria | length" "4" "$E2E_MERGED"

# Step 4: Check thresholds on merged scorecard
EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" \
  --scorecard "$TMPDIR/e2e-scorecard.json" \
  --criteria "$TMPDIR/e2e-criteria.json" > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
E2E_RESULT=$(cat "$OUTFILE")
assert_exit "e2e: merged scorecard passes → exit 0" 0 "$EXIT_CODE"
assert_json "e2e: verdict is PASS" ".verdict" "PASS" "$E2E_RESULT"

# Verify the dimensions flat map has domain-prefixed keys for all 6 dimensions
DIM_COUNT=$(echo "$E2E_RESULT" | jq '.dimensions | keys | length')
if [ "$DIM_COUNT" = "6" ]; then
  PASS=$((PASS+1)); echo "  PASS: e2e: dimensions map has 6 entries (3 per domain)"
else
  FAIL=$((FAIL+1)); echo "  FAIL: e2e: expected 6 dimension entries, got $DIM_COUNT"
fi

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 12: Coverage matrix — all coverage values are valid ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Test that the coverage matrix only allows Full/Weak/Missing
COVERAGE_VALID=$(echo "$FULL_HOLISTIC" | jq '[.spec_coverage_matrix[].coverage] | all(. == "Full" or . == "Weak" or . == "Missing")')
if [ "$COVERAGE_VALID" = "true" ]; then
  PASS=$((PASS+1)); echo "  PASS: all coverage values are Full, Weak, or Missing"
else
  FAIL=$((FAIL+1)); echo "  FAIL: invalid coverage values found"
fi

# Test that all matrix entries have required fields
MATRIX_FIELDS_VALID=$(echo "$FULL_HOLISTIC" | jq '[.spec_coverage_matrix[] | has("requirement", "coverage", "evidence")] | all')
if [ "$MATRIX_FIELDS_VALID" = "true" ]; then
  PASS=$((PASS+1)); echo "  PASS: all matrix entries have requirement, coverage, evidence fields"
else
  FAIL=$((FAIL+1)); echo "  FAIL: matrix entries missing required fields"
fi

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 13: Holistic review docs — dimension scoring rubrics 1-10 ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Each dimension should have a 1-10 rubric
for dim in "Integration" "Coherence" "Holistic Spec Fidelity" "Polish" "Runtime Health"; do
  if echo "$HOLISTIC_CONTENT" | grep -q "## $dim"; then
    PASS=$((PASS+1)); echo "  PASS: dimension '$dim' has section header"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: dimension '$dim' missing section header"
  fi
done

# Check that rubrics contain score 1 and score 10
assert_contains "rubrics contain score 1" "^ 1 " "$HOLISTIC_CONTENT"
assert_contains "rubrics contain score 10" "^10 " "$HOLISTIC_CONTENT"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 14: Holistic review — cross-domain integration check ==="
# ═══════════════════════════════════════════════════════════════════════════════

assert_contains "holistic has cross-domain integration check" "## Cross-Domain Integration Check" "$HOLISTIC_CONTENT"
assert_contains "holistic checks API contract compliance" "API contract compliance" "$HOLISTIC_CONTENT"
assert_contains "holistic checks data flow end-to-end" "Data flow end-to-end" "$HOLISTIC_CONTENT"
assert_contains "holistic checks error propagation" "Error propagation" "$HOLISTIC_CONTENT"
assert_contains "holistic checks state consistency" "State consistency" "$HOLISTIC_CONTENT"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "=== Test 15: Domains are independent — no shared dimensions ==="
# ═══════════════════════════════════════════════════════════════════════════════

# Verify that when both domains have the same dimension name (e.g. correctness),
# they are scored independently in the merged scorecard
cat > "$TMPDIR/indep-scorecard.json" << 'EOF'
{
  "domains": {
    "frontend": {
      "dimensions": {
        "correctness": {"score": 5, "threshold": 7, "evidence": "Frontend fails"},
        "domain_spec_fidelity": {"score": 9, "threshold": 8, "evidence": "Good"}
      }
    },
    "backend": {
      "dimensions": {
        "correctness": {"score": 9, "threshold": 7, "evidence": "Backend perfect"},
        "domain_spec_fidelity": {"score": 8, "threshold": 8, "evidence": "Good"}
      }
    }
  },
  "criteria": [{"id": "c1", "pass": true}]
}
EOF
jq '.criteria' "$TMPDIR/indep-scorecard.json" > "$TMPDIR/indep-criteria.json"

EXIT_CODE=0
"$SCRIPT_DIR/check-thresholds.sh" \
  --scorecard "$TMPDIR/indep-scorecard.json" \
  --criteria "$TMPDIR/indep-criteria.json" > "$OUTFILE" 2>/dev/null || EXIT_CODE=$?
INDEP_OUTPUT=$(cat "$OUTFILE")
assert_exit "independent scoring: one domain fails → exit 1" 1 "$EXIT_CODE"
assert_json "only 1 failing dimension" ".failing_dimensions | length" "1" "$INDEP_OUTPUT"
assert_json "failing domain is frontend" ".failing_dimensions[0].domain" "frontend" "$INDEP_OUTPUT"
assert_json "backend correctness still passes in map" '.dimensions["backend.correctness"]' "9" "$INDEP_OUTPUT"
assert_json "frontend correctness shows failing score" '.dimensions["frontend.correctness"]' "5" "$INDEP_OUTPUT"

# ═══════════════════════════════════════════════════════════════════════════════
echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
