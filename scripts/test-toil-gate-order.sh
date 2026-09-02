#!/usr/bin/env bash
set -uo pipefail
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
  local desc="$1" needle="$2" haystack="$3"
  if printf '%s' "$haystack" | grep -qF "$needle"; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected to contain '$needle' in: $haystack)"
  fi
}

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

SYSTEM_REL="docs/technical/SYSTEM.md"
ADR_REL="docs/technical/decisions/083-toil-is-a-document-and-a-gate.md"
MAIN_REL="crates/fiddle-cli/src/main.rs"

ORDERING='after `selected_workflow` loads the document'

fresh() {
  rm -rf "$WORK/tree"
  mkdir -p "$WORK/tree/docs/technical/decisions" "$WORK/tree/crates/fiddle-cli/src"
  printf '| toil gate | `fiddle-runtime/src/toil` | thirteen rules | the gate runs in Rust %s and before the workflow runs (ADR 083) |\n' "$ORDERING" \
    > "$WORK/tree/$SYSTEM_REL"
  printf '# 083\n\n`toil::qualify` runs in `qualified`, %s and before the workflow runs.\n' "$ORDERING" \
    > "$WORK/tree/$ADR_REL"
  printf 'fn dispatch() {\n    let document = selected_workflow(selection, &cli.config)?;\n    let qualification = qualified(\n        &config,\n    );\n}\n' \
    > "$WORK/tree/$MAIN_REL"
}

run() { "$SCRIPT_DIR/check-toil-gate-order.sh" --root "$WORK/tree" 2>&1; }

echo "the three records agreeing is the passing case"
fresh
OUT=$(run); RC=$?
assert_exit "a tree whose row, ADR and call order agree passes" 0 "$RC"
assert_contains "and it prints the two line numbers it compared" "selected_workflow at 2" "$OUT"

echo "the row asserting the retired ordering fails"
fresh
printf '| toil gate | `fiddle-runtime/src/toil` | thirteen rules | the gate runs in Rust before the document loads (ADR 083) |\n' \
  > "$WORK/tree/$SYSTEM_REL"
OUT=$(run); RC=$?
assert_exit "the row this bean found fails" 1 "$RC"
assert_contains "and the reason quotes the retired clause" "before the document loads" "$OUT"

echo "a row that states no ordering at all fails"
fresh
printf '| toil gate | `fiddle-runtime/src/toil` | thirteen rules | a refusal is published on the ticket (ADR 083) |\n' \
  > "$WORK/tree/$SYSTEM_REL"
OUT=$(run); RC=$?
assert_exit "silence is not agreement" 1 "$RC"
assert_contains "and the reason names the ordering it wanted" "$ORDERING" "$OUT"

echo "the ADR dropping the ordering fails, so the pair is pinned in both directions"
fresh
printf '# 083\n\nEligibility is an outer Rust gate.\n' > "$WORK/tree/$ADR_REL"
OUT=$(run); RC=$?
assert_exit "an ADR that no longer states the ordering fails" 1 "$RC"
assert_contains "and the reason names the ADR" "083-toil-is-a-document-and-a-gate.md" "$OUT"

echo "the source moving under both records fails"
fresh
printf 'fn dispatch() {\n    let qualification = qualified(\n        &config,\n    );\n    let document = selected_workflow(selection, &cli.config)?;\n}\n' \
  > "$WORK/tree/$MAIN_REL"
OUT=$(run); RC=$?
assert_exit "a binary that gates before it loads fails, whatever the records say" 1 "$RC"
assert_contains "and the reason names both line numbers" "selected_workflow at line 5 and qualified at line 2" "$OUT"

echo "a precondition that cannot be read refuses rather than answering"
fresh
rm "$WORK/tree/$SYSTEM_REL"
OUT=$(run); RC=$?
assert_exit "a missing SYSTEM.md is exit 2 and not a pass" 2 "$RC"
assert_contains "and it says it measured nothing" "measured nothing" "$OUT"

fresh
printf 'fn dispatch() {\n    let document = load(selection)?;\n    let gate = gate();\n}\n' > "$WORK/tree/$MAIN_REL"
OUT=$(run); RC=$?
assert_exit "a main.rs whose two calls this check cannot find is exit 2 and not a pass" 2 "$RC"
assert_contains "and it names what it could not find" "measured nothing" "$OUT"

OUT=$("$SCRIPT_DIR/check-toil-gate-order.sh" --root "$WORK/nosuchtree" 2>&1); RC=$?
assert_exit "an absent root is exit 2" 2 "$RC"

OUT=$("$SCRIPT_DIR/check-toil-gate-order.sh" --wat 2>&1); RC=$?
assert_exit "an unknown argument is exit 2" 2 "$RC"

echo "and the real tree passes"
OUT=$("$SCRIPT_DIR/check-toil-gate-order.sh" --root "$SCRIPT_DIR/.." 2>&1); RC=$?
assert_exit "this repository's own three records agree" 0 "$RC"
assert_contains "and the pass names the ordering it compared" "TOIL GATE ORDER: ok" "$OUT"

echo
echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
