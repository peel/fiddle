#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

assert_json() {
  local description="$1" expected="$2" query="$3" output="$4"
  local actual
  actual=$(jq -r "$query" <<<"$output" 2>/dev/null || true)
  if [[ "$actual" == "$expected" ]]; then
    PASS=$((PASS + 1)); echo "  PASS: $description"
  else
    FAIL=$((FAIL + 1)); echo "  FAIL: $description (expected $expected, got $actual)"
  fi
}

assert_file_contains() {
  local description="$1" pattern="$2" file="$3"
  if grep -F --quiet "$pattern" "$file"; then
    PASS=$((PASS + 1)); echo "  PASS: $description"
  else
    FAIL=$((FAIL + 1)); echo "  FAIL: $description (missing $pattern)"
  fi
}

assert_file_excludes() {
  local description="$1" pattern="$2" file="$3"
  if grep -F --quiet "$pattern" "$file"; then
    FAIL=$((FAIL + 1)); echo "  FAIL: $description (found $pattern)"
  else
    PASS=$((PASS + 1)); echo "  PASS: $description"
  fi
}

assert_exit() {
  local description="$1" expected="$2" actual="$3"
  if [[ "$expected" == "$actual" ]]; then
    PASS=$((PASS + 1)); echo "  PASS: $description"
  else
    FAIL=$((FAIL + 1)); echo "  FAIL: $description (expected exit $expected, got $actual)"
  fi
}

CONFIG="$TMPDIR/orchestrate.json"
cat > "$CONFIG" <<'EOF'
{
  "providers": {"codex": {"command": "codex exec"}},
  "models": {
    "phases": {"define": "slow", "develop": "smol"},
    "roles": {"panel": "smol", "implementer": "default", "evaluator": "slow"}
  }
}
EOF

resolve() {
  "$SCRIPT_DIR/resolve-subagent-model.sh" --config "$CONFIG" --phase "$1" --role "$2"
}

echo "Test 1: role override wins"
OUTPUT=$(resolve define panel)
assert_json "role source" role '.source' "$OUTPUT"
assert_json "role model" smol '.model' "$OUTPUT"

echo "Test 2: phase default wins without role override"
OUTPUT=$(resolve develop holistic)
assert_json "phase source" phase '.source' "$OUTPUT"
assert_json "phase model" smol '.model' "$OUTPUT"

echo "Test 3: default inherits the session model"
OUTPUT=$(resolve define implementer)
assert_json "explicit default source" role '.source' "$OUTPUT"
assert_json "default omits model" true 'has("model") | not' "$OUTPUT"

echo "Test 4: absent configuration inherits the session model"
OUTPUT=$(resolve discover brainstorm)
assert_json "absent source" default '.source' "$OUTPUT"
assert_json "absent omits model" true 'has("model") | not' "$OUTPUT"

echo "Test 5: all internal roles resolve"
for role_phase in panel:define brainstorm:define implementer:develop evaluator:develop holistic:develop deliver:deliver; do
  role=${role_phase%%:*}
  phase=${role_phase##*:}
  OUTPUT=$(resolve "$phase" "$role")
  assert_json "$role resolves" true '(.source == "role" or .source == "phase" or .source == "default")' "$OUTPUT"
done

echo "Test 6: invalid model fails without changing providers"
jq '.models.roles.panel = "codex"' "$CONFIG" > "$TMPDIR/invalid.json"
EXIT_CODE=0
"$SCRIPT_DIR/resolve-subagent-model.sh" --config "$TMPDIR/invalid.json" --phase define --role panel >"$TMPDIR/out.json" 2>"$TMPDIR/error.json" || EXIT_CODE=$?
assert_exit "unsupported model" 2 "$EXIT_CODE"
assert_json "error code" invalid-model '.error.code' "$(cat "$TMPDIR/error.json")"
assert_json "provider unchanged" "codex exec" '.providers.codex.command' "$(cat "$CONFIG")"

echo "Test 7: malformed model schema fails with invalid-config"
for fixture in models-scalar models-array roles-scalar roles-array phases-scalar phases-array; do
  case "$fixture" in
    models-scalar) jq '.models = "invalid"' "$CONFIG" > "$TMPDIR/$fixture.json" ;;
    models-array) jq '.models = []' "$CONFIG" > "$TMPDIR/$fixture.json" ;;
    roles-scalar) jq '.models.roles = "invalid"' "$CONFIG" > "$TMPDIR/$fixture.json" ;;
    roles-array) jq '.models.roles = []' "$CONFIG" > "$TMPDIR/$fixture.json" ;;
    phases-scalar) jq '.models.phases = "invalid"' "$CONFIG" > "$TMPDIR/$fixture.json" ;;
    phases-array) jq '.models.phases = []' "$CONFIG" > "$TMPDIR/$fixture.json" ;;
  esac
  EXIT_CODE=0
  "$SCRIPT_DIR/resolve-subagent-model.sh" --config "$TMPDIR/$fixture.json" --phase define --role panel >"$TMPDIR/$fixture.out.json" 2>"$TMPDIR/$fixture.error.json" || EXIT_CODE=$?
  assert_exit "$fixture exit" 2 "$EXIT_CODE"
  assert_json "$fixture error code" invalid-config '.error.code' "$(cat "$TMPDIR/$fixture.error.json")"
done

echo "Test 8: every configured model value is validated"
for fixture in selected-null unselected-unsupported phase-array-value; do
  case "$fixture" in
    selected-null) jq '.models.roles.panel = null' "$CONFIG" > "$TMPDIR/$fixture.json" ;;
    unselected-unsupported) jq '.models.roles.unused = "codex"' "$CONFIG" > "$TMPDIR/$fixture.json" ;;
    phase-array-value) jq '.models.phases.unused = []' "$CONFIG" > "$TMPDIR/$fixture.json" ;;
  esac
  EXIT_CODE=0
  "$SCRIPT_DIR/resolve-subagent-model.sh" --config "$TMPDIR/$fixture.json" --phase define --role panel >"$TMPDIR/$fixture.out.json" 2>"$TMPDIR/$fixture.error.json" || EXIT_CODE=$?
  assert_exit "$fixture exit" 2 "$EXIT_CODE"
  assert_json "$fixture error code" invalid-model '.error.code' "$(cat "$TMPDIR/$fixture.error.json")"
done

echo "Test 9: model configuration documentation matches the resolver contract"
CONFIGURATION_DOC="$SCRIPT_DIR/../skills/orchestrate/configuration.md"
DEFINE_DOC="$SCRIPT_DIR/../skills/define/SKILL.md"
DELIVER_DOC="$SCRIPT_DIR/../skills/deliver/SKILL.md"
SYSTEM_DOC="$SCRIPT_DIR/../docs/technical/SYSTEM.md"
assert_file_contains "configuration phases example" '"phases": {' "$CONFIGURATION_DOC"
assert_file_contains "configuration roles example" '"roles": {' "$CONFIGURATION_DOC"
for doc in "$CONFIGURATION_DOC" "$DEFINE_DOC" "$DELIVER_DOC" "$SYSTEM_DOC"; do
  assert_file_excludes "no models.define path in $(basename "$doc")" "models.define" "$doc"
  assert_file_excludes "no models.deliver path in $(basename "$doc")" "models.deliver" "$doc"
done

echo
printf 'Results: %d passed, %d failed\n' "$PASS" "$FAIL"
[[ "$FAIL" -eq 0 ]]
