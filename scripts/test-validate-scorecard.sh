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
  # run <criteria-ids> ; sets EXIT_CODE, writes stderr to $ERR
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

echo "Test 6: explicit empty dimensions {} → exit 0"
cat > "$SC" << 'EOF'
{
  "provider": "claude",
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

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
