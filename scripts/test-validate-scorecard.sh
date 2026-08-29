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

assert_contains() {
  local desc="$1" needle="$2" file="$3"
  if grep -q "$needle" "$file"; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected stderr to contain '$needle')"
    echo "    stderr was: $(cat "$file")"
  fi
}

assert_json_array() {
  local desc="$1" file="$2"
  if jq -e 'type == "array"' "$file" >/dev/null 2>&1; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (stderr is not a JSON array)"
    echo "    stderr was: $(cat "$file")"
  fi
}

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT
SC="$TMPDIR/scorecard.json"
ERR="$TMPDIR/err.json"

run() {
  EXIT_CODE=0
  "$SCRIPT_DIR/validate-scorecard.sh" --scorecard "$SC" --criteria-ids "$1" >/dev/null 2>"$ERR" || EXIT_CODE=$?
}

echo "Test 1: valid scorecard → exit 0"
cat > "$SC" << 'EOF'
{
  "task_id": "t-1",
  "iteration": 1,
  "provider": "claude",
  "domains": {
    "infrastructure": {
      "dimensions": {
        "correctness": { "score": 8, "evidence": "traced main path, verified", "threshold": 7 }
      }
    }
  },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "scriptX line 12: exits 0" },
    { "id": "b", "pass": true, "evidence": "scriptX line 30: exits 2" }
  ],
  "antipatterns_detected": [],
  "spec_defect": null
}
EOF
run "a,b"
assert_exit "valid → exit 0" 0 "$EXIT_CODE"

echo "Test 2: missing provider → exit 2"
cat > "$SC" << 'EOF'
{
  "task_id": "t-2",
  "mode": "evidence-only",
  "domains": { "infrastructure": { "dimensions": {} } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" },
    { "id": "b", "pass": true, "evidence": "e2" }
  ]
}
EOF
run "a,b"
assert_exit "missing provider → exit 2" 2 "$EXIT_CODE"
assert_json_array "error is a JSON array" "$ERR"
assert_contains "names provider" "provider" "$ERR"

echo "Test 3: criteria id mismatch (extra + missing) → exit 2 naming ids"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "mode": "evidence-only",
  "domains": { "infrastructure": { "dimensions": {} } },
  "criteria": [
    { "id": "id_alpha", "pass": true, "evidence": "e1" },
    { "id": "id_gamma", "pass": true, "evidence": "e2" }
  ]
}
EOF
run "id_alpha,id_beta"
assert_exit "criteria mismatch → exit 2" 2 "$EXIT_CODE"
assert_contains "names unexpected id id_gamma" "id_gamma" "$ERR"
assert_contains "names missing id id_beta" "id_beta" "$ERR"

echo "Test 4a: empty evidence on a criterion → exit 2"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "mode": "evidence-only",
  "domains": { "infrastructure": { "dimensions": {} } },
  "criteria": [
    { "id": "id_alpha", "pass": true, "evidence": "e1" },
    { "id": "id_beta", "pass": true, "evidence": "" }
  ]
}
EOF
run "id_alpha,id_beta"
assert_exit "empty criterion evidence → exit 2" 2 "$EXIT_CODE"
assert_contains "names criterion id_beta" "id_beta" "$ERR"

echo "Test 4b: empty evidence on a scored dimension → exit 2"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": {
    "infrastructure": {
      "dimensions": {
        "correctness": { "score": 8, "evidence": "", "threshold": 7 }
      }
    }
  },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" },
    { "id": "b", "pass": true, "evidence": "e2" }
  ]
}
EOF
run "a,b"
assert_exit "empty dimension evidence → exit 2" 2 "$EXIT_CODE"
assert_contains "names correctness dimension" "correctness" "$ERR"

echo "Test 4c: dimension justification under comment only → exit 0"
cat > "$SC" << 'EOF'
{
  "provider": "codex",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": { "score": 8, "comment": "read validate-scorecard.sh, exit paths agree", "threshold": 7 }
      }
    }
  },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" },
    { "id": "b", "pass": true, "evidence": "e2" }
  ]
}
EOF
run "a,b"
assert_exit "comment accepted as evidence alias → exit 0" 0 "$EXIT_CODE"

echo "Test 4d: dimension with both evidence and comment empty → exit 2"
cat > "$SC" << 'EOF'
{
  "provider": "codex",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": { "score": 8, "evidence": "", "comment": "  ", "threshold": 7 }
      }
    }
  },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" },
    { "id": "b", "pass": true, "evidence": "e2" }
  ]
}
EOF
run "a,b"
assert_exit "empty evidence and comment → exit 2" 2 "$EXIT_CODE"
assert_contains "names correctness dimension" "correctness" "$ERR"

echo "Test 5: dimensions present but not an object → exit 2"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "infrastructure": { "dimensions": "oops" } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" },
    { "id": "b", "pass": true, "evidence": "e2" }
  ]
}
EOF
run "a,b"
assert_exit "dimensions not object → exit 2" 2 "$EXIT_CODE"
assert_contains "names dimensions" "dimensions" "$ERR"

echo "Test 6: explicit empty dimensions {} with the declaration → exit 0"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "mode": "evidence-only",
  "domains": { "infrastructure": { "dimensions": {} } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" },
    { "id": "b", "pass": true, "evidence": "e2" }
  ]
}
EOF
run "a,b"
assert_exit "evidence-only empty dimensions → exit 0" 0 "$EXIT_CODE"

echo "Test 7: spec_defect detected:true without reason → exit 2"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "mode": "evidence-only",
  "domains": { "infrastructure": { "dimensions": {} } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" },
    { "id": "b", "pass": true, "evidence": "e2" }
  ],
  "spec_defect": { "detected": true, "reason": "" }
}
EOF
run "a,b"
assert_exit "spec_defect no reason → exit 2" 2 "$EXIT_CODE"
assert_contains "names spec_defect" "spec_defect" "$ERR"

echo "Test 7b: spec_defect detected:false without reason → exit 0"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "mode": "evidence-only",
  "domains": { "infrastructure": { "dimensions": {} } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" },
    { "id": "b", "pass": true, "evidence": "e2" }
  ],
  "spec_defect": { "detected": false }
}
EOF
run "a,b"
assert_exit "spec_defect not detected → exit 0" 0 "$EXIT_CODE"

echo "Test 8: malformed JSON → exit 2"
cat > "$SC" << 'EOF'
{ "provider": "claude", "criteria": [ { "id": "a" ,
EOF
run "a,b"
assert_exit "malformed JSON → exit 2" 2 "$EXIT_CODE"
assert_json_array "malformed error is a JSON array" "$ERR"

echo "Test 9: criterion/met instead of id/pass → exit 2 naming both spellings"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "mode": "evidence-only",
  "domains": { "general": { "dimensions": {} } },
  "criteria": [
    { "criterion": "a", "met": true, "evidence": "e1" }
  ]
}
EOF
run "a"
assert_exit "criterion/met → exit 2" 2 "$EXIT_CODE"
assert_json_array "error is a JSON array" "$ERR"
assert_contains "names criterion as the wrong spelling of id" 'missing .id. (found .criterion.' "$ERR"
assert_contains "names met as the wrong spelling of pass" 'missing .pass. (found .met.' "$ERR"
assert_contains "points at the envelope document" "skills/develop/scorecard-envelope.md" "$ERR"

echo "Test 10: a dimension with no threshold → exit 2, before the grader ever sees it"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": { "score": 8, "evidence": "traced it" }
      }
    }
  },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ]
}
EOF
run "a"
assert_exit "missing threshold → exit 2" 2 "$EXIT_CODE"
assert_contains "names the missing threshold" 'missing .threshold.' "$ERR"
assert_contains "names the dimension" "domain general dimension correctness" "$ERR"

echo "Test 11: a stringly-typed score → exit 2"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": {
    "general": {
      "dimensions": {
        "correctness": { "score": "8", "threshold": 7, "evidence": "traced it" }
      }
    }
  },
  "criteria": [
    { "id": "a", "pass": "true", "evidence": "e1" }
  ]
}
EOF
run "a"
assert_exit "stringly-typed score and pass → exit 2" 2 "$EXIT_CODE"
assert_contains "says score must be a number" 'score. must be a number, got string' "$ERR"
assert_contains "says pass must be a boolean" 'pass. must be a boolean, got string' "$ERR"

echo "Test 12: criteria mis-nested under domains is reported, not a jq crash"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "mode": "evidence-only",
  "domains": {
    "criteria": [
      { "id": "a", "pass": true, "evidence": "e1" }
    ]
  }
}
EOF
run "a"
assert_exit "mis-nested criteria → exit 2" 2 "$EXIT_CODE"
assert_json_array "error is a JSON array, not a jq trace" "$ERR"
assert_contains "names the mis-nested key as a domain" "domain criteria" "$ERR"

echo "Test 13: empty dimensions with no declaration → exit 2 (fiddle-ayrq)"
cat > "$SC" << 'EOF'
{
  "provider": "codex",
  "domains": { "general": { "dimensions": {} } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" },
    { "id": "b", "pass": true, "evidence": "e2" }
  ]
}
EOF
run "a,b"
assert_exit "undeclared empty dimensions → exit 2" 2 "$EXIT_CODE"
assert_json_array "error is a JSON array" "$ERR"
assert_contains "names the domain that scored nothing" "domain general" "$ERR"
assert_contains "names the declaration it wanted" 'mode' "$ERR"

echo "Test 14: no domains at all → exit 2 for the same reason"
cat > "$SC" << 'EOF'
{
  "provider": "codex",
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ]
}
EOF
run "a"
assert_exit "no domains → exit 2" 2 "$EXIT_CODE"
assert_contains "names the missing scores" "scored no dimensions" "$ERR"

echo "Test 15: one criterion id twice → exit 2, not silently collapsed"
cat > "$SC" << 'EOF'
{
  "provider": "codex",
  "mode": "evidence-only",
  "domains": { "general": { "dimensions": {} } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" },
    { "id": "a", "pass": false, "evidence": "e2" },
    { "id": "b", "pass": true, "evidence": "e3" }
  ]
}
EOF
run "a,b"
assert_exit "duplicated criterion id → exit 2" 2 "$EXIT_CODE"
assert_contains "names the duplicated id" "duplicate criterion id" "$ERR"

echo "Test 16: a mode the envelope does not accept → exit 2"
cat > "$SC" << 'EOF'
{
  "provider": "codex",
  "mode": "evidence_only",
  "domains": { "general": { "dimensions": {} } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ]
}
EOF
run "a"
assert_exit "unaccepted mode → exit 2" 2 "$EXIT_CODE"
assert_contains "names the value it got" "evidence_only" "$ERR"

echo "Test 17: a spec_defect that is not null and not an object -> exit 2"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "general": { "dimensions": { "correctness": { "score": 8, "threshold": 7, "evidence": "e1" } } } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ],
  "spec_defect": true
}
EOF
run "a"
assert_exit "a boolean spec_defect → exit 2" 2 "$EXIT_CODE"
assert_contains "names the type it got" "got boolean" "$ERR"

cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "general": { "dimensions": { "correctness": { "score": 8, "threshold": 7, "evidence": "e1" } } } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ],
  "spec_defect": "unknown"
}
EOF
run "a"
assert_exit "a string spec_defect → exit 2" 2 "$EXIT_CODE"
assert_contains "names the type it got" "got string" "$ERR"

cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "general": { "dimensions": { "correctness": { "score": 8, "threshold": 7, "evidence": "e1" } } } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ],
  "spec_defect": ["detected"]
}
EOF
run "a"
assert_exit "a array spec_defect → exit 2" 2 "$EXIT_CODE"
assert_contains "names the type it got" "got array" "$ERR"

cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "general": { "dimensions": { "correctness": { "score": 8, "threshold": 7, "evidence": "e1" } } } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ],
  "spec_defect": 0
}
EOF
run "a"
assert_exit "a number spec_defect → exit 2" 2 "$EXIT_CODE"
assert_contains "names the type it got" "got number" "$ERR"

echo "Test 17e: a spec_defect object whose detected is not a boolean → exit 2"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "general": { "dimensions": { "correctness": { "score": 8, "threshold": 7, "evidence": "e1" } } } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ],
  "spec_defect": { "detected": "true" }
}
EOF
run "a"
assert_exit "a stringly-typed detected → exit 2" 2 "$EXIT_CODE"
assert_contains "names detected" "detected" "$ERR"

echo "Test 17f: a spec_defect object carrying no detected → exit 2"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "general": { "dimensions": { "correctness": { "score": 8, "threshold": 7, "evidence": "e1" } } } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ],
  "spec_defect": {}
}
EOF
run "a"
assert_exit "an empty spec_defect object → exit 2" 2 "$EXIT_CODE"
assert_contains "names detected" "detected" "$ERR"

echo "Test 17g: null and both boolean verdicts still pass, so the refusal is not vacuous"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "general": { "dimensions": { "correctness": { "score": 8, "threshold": 7, "evidence": "e1" } } } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ],
  "spec_defect": null
}
EOF
run "a"
assert_exit "a null spec_defect → exit 0" 0 "$EXIT_CODE"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "general": { "dimensions": { "correctness": { "score": 8, "threshold": 7, "evidence": "e1" } } } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ],
  "spec_defect": { "detected": false }
}
EOF
run "a"
assert_exit "detected false → exit 0" 0 "$EXIT_CODE"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "general": { "dimensions": { "correctness": { "score": 8, "threshold": 7, "evidence": "e1" } } } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ],
  "spec_defect": { "detected": true, "reason": "the spec names a dropped table" }
}
EOF
run "a"
assert_exit "detected true with a reason → exit 0" 0 "$EXIT_CODE"

echo "Test 17h: a card that leaves spec_defect out is still accepted here, and the merge names it"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
  "domains": { "general": { "dimensions": { "correctness": { "score": 8, "threshold": 7, "evidence": "e1" } } } },
  "criteria": [
    { "id": "a", "pass": true, "evidence": "e1" }
  ]
}
EOF
run "a"
assert_exit "an absent spec_defect → exit 0" 0 "$EXIT_CODE"


echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
