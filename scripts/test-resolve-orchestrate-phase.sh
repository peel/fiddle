#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
RESOLVER="$SCRIPT_DIR/resolve-orchestrate-phase.sh"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

PASS=0
FAIL=0

write_json() {
  local name="$1" json="$2"
  printf '%s\n' "$json" > "$TMP_DIR/$name.json"
  printf '%s' "$TMP_DIR/$name.json"
}

assert_state() {
  local description="$1" expected_state="$2" epic_json="$3" children_json="$4"
  shift 4
  local output actual_state
  if ! output=$("$RESOLVER" --epic "$epic_json" --children "$children_json" "$@" 2>"$TMP_DIR/error"); then
    FAIL=$((FAIL + 1))
    echo "  FAIL: $description (resolver failed: $(cat "$TMP_DIR/error"))"
    return
  fi
  actual_state=$(jq -r '.state' <<<"$output")
  if [[ "$actual_state" == "$expected_state" ]]; then
    PASS=$((PASS + 1))
    echo "  PASS: $description"
  else
    FAIL=$((FAIL + 1))
    echo "  FAIL: $description (expected $expected_state, got $actual_state: $output)"
  fi
}

EPIC=$(write_json epic '{"id":"factory-m0","type":"epic","status":"todo","tags":["agentic-factory"],"blocked_by":[]}')
MILESTONE=$(write_json milestone '{"id":"factory-v1","type":"milestone","status":"todo","tags":["agentic-factory"]}')
EMPTY=$(write_json empty '[]')

SEED_TODO=$(write_json seed-todo '[{"id":"seed-m0","type":"task","status":"todo","tags":["planning"]}]')
SEED_ACTIVE=$(write_json seed-active '[{"id":"seed-m0","type":"task","status":"in-progress","tags":["planning"]}]')
SEED_AND_WORK=$(write_json seed-and-work '[{"id":"seed-m0","type":"task","status":"completed","tags":["planning"]},{"id":"task-1","type":"task","status":"todo","tags":["generated-by:seed-m0","plan-task:1"]}]')
ALL_TERMINAL=$(write_json all-terminal '[{"id":"seed-m0","type":"task","status":"completed","tags":["planning"]},{"id":"task-1","type":"task","status":"completed","tags":["generated-by:seed-m0","plan-task:1"]}]')
PREMATURE_WORK=$(write_json premature-work '[{"id":"seed-m0","type":"task","status":"todo","tags":["planning"]},{"id":"task-1","type":"task","status":"todo","tags":["generated-by:seed-m0","plan-task:1"]}]')
TWO_SEEDS=$(write_json two-seeds '[{"id":"seed-a","type":"task","status":"todo","tags":["planning"]},{"id":"seed-b","type":"task","status":"todo","tags":["planning"]}]')
COMPLETED_SEED_ONLY=$(write_json completed-seed-only '[{"id":"seed-m0","type":"task","status":"completed","tags":["planning"]}]')
DUPLICATE_GENERATION=$(write_json duplicate-generation '[{"id":"seed-m0","type":"task","status":"completed","tags":["planning"]},{"id":"task-1a","type":"task","status":"todo","tags":["generated-by:seed-m0","plan-task:1"]},{"id":"task-1b","type":"task","status":"todo","tags":["generated-by:seed-m0","plan-task:1"]}]')
CONFLICTING_PARENT=$(write_json conflicting-parent '[{"id":"seed-m0","type":"task","status":"completed","parent":"factory-m0","tags":["planning"]},{"id":"task-1","type":"task","status":"todo","parent":"another-epic","tags":["generated-by:seed-m0","plan-task:1"]}]')
LEGACY_WORK=$(write_json legacy-work '[{"id":"task-legacy","type":"task","status":"todo","tags":[]}]')
MILESTONE_CHILDREN=$(write_json milestone-children '[{"id":"factory-m0","type":"epic","status":"todo","tags":["agentic-factory"]}]')

echo "Seed-aware routing"
assert_state "todo seed routes to SEED" SEED "$EPIC" "$SEED_TODO"
assert_state "interrupted seed routes to SEED" SEED "$EPIC" "$SEED_ACTIVE"
assert_state "completed seed with work routes to DEVELOP" DEVELOP "$EPIC" "$SEED_AND_WORK"
assert_state "terminal work routes to DELIVER" DELIVER "$EPIC" "$ALL_TERMINAL"
assert_state "delivery-complete terminal work routes to DONE" DONE "$EPIC" "$ALL_TERMINAL" --delivery-complete

echo "Invalid routing"
assert_state "implementation cannot precede seed completion" INVALID "$EPIC" "$PREMATURE_WORK"
assert_state "multiple planning seeds are invalid" INVALID "$EPIC" "$TWO_SEEDS"
assert_state "completed seed without generated work is invalid" INVALID "$EPIC" "$COMPLETED_SEED_ONLY"
assert_state "duplicate generation identity is invalid" INVALID "$EPIC" "$DUPLICATE_GENERATION"
assert_state "conflicting child parent is invalid" INVALID "$EPIC" "$CONFLICTING_PARENT"
assert_state "top milestone does not select a child implicitly" INVALID "$MILESTONE" "$MILESTONE_CHILDREN"

echo "Legacy routing"
assert_state "empty legacy epic routes to DEFINE" DEFINE "$EPIC" "$EMPTY"
assert_state "legacy epic with work routes to DEVELOP" DEVELOP "$EPIC" "$LEGACY_WORK"

PREDECESSOR_EPIC=$(write_json predecessor-epic '{"id":"factory-m1","type":"epic","status":"todo","tags":["agentic-factory"],"blocked_by":["factory-m0"]}')
PREDECESSOR_COMPLETE=$(write_json predecessor-complete '{"id":"factory-m0","type":"epic","status":"completed","body":"<!-- milestone-handoff:start -->\n## Milestone Handoff\n\n- Capability now available: skeleton\n<!-- milestone-handoff:end -->"}')
PREDECESSOR_NO_HANDOFF=$(write_json predecessor-no-handoff '{"id":"factory-m0","type":"epic","status":"completed","body":"No handoff"}')

echo "Predecessor context"
assert_state "completed predecessor handoff permits SEED" SEED "$PREDECESSOR_EPIC" "$SEED_TODO" --predecessor "$PREDECESSOR_COMPLETE"
assert_state "missing predecessor handoff needs context" NEEDS_CONTEXT "$PREDECESSOR_EPIC" "$SEED_TODO" --predecessor "$PREDECESSOR_NO_HANDOFF"
assert_state "missing predecessor bean needs context" NEEDS_CONTEXT "$PREDECESSOR_EPIC" "$SEED_TODO"

echo
echo "$PASS passed, $FAIL failed"
[[ "$FAIL" -eq 0 ]]
