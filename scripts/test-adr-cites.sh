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

fresh() {
  rm -rf "$WORK/tree"
  mkdir -p "$WORK/tree/docs/technical/decisions" \
           "$WORK/tree/crates/fiddle-core/src" \
           "$WORK/tree/crates/fiddle-runtime/src/workspace" \
           "$WORK/tree/crates/fiddle-acceptance/tests" \
           "$WORK/tree/scripts"
  printf 'pub fn selected() {}\npub struct Severities;\n' > "$WORK/tree/crates/fiddle-core/src/finding.rs"
  printf 'pub fn run() {}\n' > "$WORK/tree/crates/fiddle-runtime/src/workspace/command.rs"
  printf 'fn boundary() {}\n' > "$WORK/tree/crates/fiddle-acceptance/tests/crate_boundary.rs"
  printf 'echo hi\n' > "$WORK/tree/scripts/gate.sh"
}

adr() {
  local name="$1" cites="$2"
  {
    printf '# %s\n\n' "$name"
    printf 'Status: accepted\n'
    [ -n "$cites" ] && printf 'Cites: %s\n' "$cites"
    printf '\n## Decision\n\nsomething\n'
  } > "$WORK/tree/docs/technical/decisions/$name.md"
}

run() { "$SCRIPT_DIR/check-adr-cites.sh" --root "$WORK/tree" "$@"; }

echo "=== Test 1: every cited symbol resolves under crates/ ==="
fresh
adr "021-a-decision" "fiddle_core::selected, Severities"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "resolving Cites → exit 0" 0 "$EXIT_CODE"
assert_contains "reports what it measured" "2 cited symbols, 0 unresolved" "$OUT"

echo ""
echo "=== Test 2: a cited symbol that resolves to nothing fails ==="
fresh
adr "021-a-decision" "fiddle_core::selected, fiddle_core::deleted_yesterday"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "unresolved Cites → exit 1" 1 "$EXIT_CODE"
assert_contains "names the symbol" "deleted_yesterday resolves to nothing" "$OUT"

echo ""
echo "=== Test 3: an ADR at or above the floor must carry the line ==="
fresh
adr "021-a-decision" ""
adr "022-another" "selected"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "missing Cites above the floor → exit 1" 1 "$EXIT_CODE"
assert_contains "names the ADR and the floor" "021-a-decision.md: no Cites: line" "$OUT"

echo ""
echo "=== Test 4: an ADR below the floor is not retrofitted ==="
fresh
adr "019-older" ""
adr "021-a-decision" "selected"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "missing Cites below the floor → exit 0" 0 "$EXIT_CODE"
assert_contains "counts both ADRs" "2 ADRs" "$OUT"

echo ""
echo "=== Test 5: the floor is configurable ==="
fresh
adr "021-a-decision" ""
adr "025-newer" "selected"
EXIT_CODE=0
OUT=$(run --floor 25 2>&1) || EXIT_CODE=$?
assert_exit "raised floor exempts 021 → exit 0" 0 "$EXIT_CODE"

echo ""
echo "=== Test 6: 'none' is a deliberate answer, not a resolvable symbol ==="
fresh
adr "021-a-decision" "none"
adr "022-another" "selected"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "Cites: none → exit 0" 0 "$EXIT_CODE"
assert_contains "none is not counted as measured" "1 cited symbols" "$OUT"

echo ""
echo "=== Test 7: a run that measured nothing is not a pass ==="
fresh
adr "021-a-decision" "none"
adr "022-another" "none"
EXIT_CODE=0
ERR=$(run 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "every ADR citing nothing → exit 2" 2 "$EXIT_CODE"
assert_contains "says it measured nothing" "measured nothing" "$ERR"

fresh
EXIT_CODE=0
ERR=$(run 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "no ADRs matched → exit 2" 2 "$EXIT_CODE"
assert_contains "error is JSON" '"error"' "$ERR"

echo ""
echo "=== Test 8: a missing tree is bad input, not a pass ==="
EXIT_CODE=0
ERR=$("$SCRIPT_DIR/check-adr-cites.sh" --root "$WORK/nosuchtree" 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "no decisions directory → exit 2" 2 "$EXIT_CODE"

fresh
rm -rf "$WORK/tree/crates"
adr "021-a-decision" "selected"
EXIT_CODE=0
ERR=$(run 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "no crates directory → exit 2" 2 "$EXIT_CODE"

echo ""
echo "=== Test 9: an unknown argument is refused ==="
EXIT_CODE=0
ERR=$("$SCRIPT_DIR/check-adr-cites.sh" --wat 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "unknown argument → exit 2" 2 "$EXIT_CODE"
assert_contains "error is JSON" '"error"' "$ERR"

echo ""
echo "=== Test 10: a cited path resolves as a file, not as file content ==="
fresh
adr "021-a-decision" "workspace/command.rs"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a partial path nothing mentions → exit 0" 0 "$EXIT_CODE"
assert_contains "the path counted as measured" "1 cited symbols" "$OUT"
if [ -z "$(grep -rlF 'workspace/command.rs' "$WORK/tree/crates")" ]; then
  PASS=$((PASS+1)); echo "  PASS: nothing under crates/ mentions the path, so a content grep would have failed it"
else
  FAIL=$((FAIL+1)); echo "  FAIL: the premise is a path no file mentions"
fi

fresh
adr "021-a-decision" "crates/fiddle-acceptance/tests/crate_boundary.rs"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a repository-relative path → exit 0" 0 "$EXIT_CODE"

fresh
adr "021-a-decision" "scripts/gate.sh"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a path outside crates/ → exit 0" 0 "$EXIT_CODE"

echo ""
echo "=== Test 11: a cited path that names no file fails ==="
fresh
adr "021-a-decision" "workspace/deleted_yesterday.rs"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a path that names no file → exit 1" 1 "$EXIT_CODE"
assert_contains "says the file is missing, not the symbol" "workspace/deleted_yesterday.rs names no file" "$OUT"

fresh
mkdir -p "$WORK/tree/target/debug/build"
printf 'pub fn gone() {}\n' > "$WORK/tree/target/debug/build/generated.rs"
adr "021-a-decision" "build/generated.rs"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a build artefact cannot satisfy a citation → exit 1" 1 "$EXIT_CODE"

echo ""
echo "=== Test 12: a path with a symbol after it is still a symbol ==="
fresh
adr "021-a-decision" "workspace/command.rs::selected"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "file.rs::symbol resolves by content → exit 0" 0 "$EXIT_CODE"

fresh
adr "021-a-decision" "workspace/command.rs::deleted_yesterday"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "file.rs::missing_symbol → exit 1" 1 "$EXIT_CODE"
assert_contains "reports the symbol, not the file" "deleted_yesterday resolves to nothing" "$OUT"

echo ""
echo "=== Test 13: a symbol outside crates/ resolves ==="
fresh
mkdir -p "$WORK/tree/skills/orchestrate"
printf 'The lead reads RALPH_STATUS once.\n' > "$WORK/tree/skills/orchestrate/resumption.md"
adr "021-a-decision" "RALPH_STATUS"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a symbol under skills/ → exit 0" 0 "$EXIT_CODE"
if [ -z "$(grep -rlF 'RALPH_STATUS' "$WORK/tree/crates")" ]; then
  PASS=$((PASS+1)); echo "  PASS: nothing under crates/ holds the symbol, so a crates-only grep would have failed it"
else
  FAIL=$((FAIL+1)); echo "  FAIL: the premise is a symbol crates/ does not hold"
fi

fresh
mkdir -p "$WORK/tree/target/debug" "$WORK/tree/.beans"
printf 'pub fn only_in_target() {}\n' > "$WORK/tree/target/debug/generated.rs"
printf 'only_in_beans\n' > "$WORK/tree/.beans/021.md"
adr "021-a-decision" "only_in_target"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a symbol only in target/ → exit 1" 1 "$EXIT_CODE"
adr "021-a-decision" "only_in_beans"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a symbol only in .beans/ → exit 1" 1 "$EXIT_CODE"

echo ""
echo "=== Test 14: an entry keeps its interior blanks ==="
fresh
printf 'git status --porcelain=v1 -z -uno\n' > "$WORK/tree/crates/fiddle-runtime/src/workspace/args.rs"
adr "021-a-decision" "git status --porcelain=v1 -z -uno, selected"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a multi-word entry that the tree holds → exit 0" 0 "$EXIT_CODE"
assert_contains "the entry counted once, not once per word" "2 cited symbols" "$OUT"

fresh
printf 'pub fn gitstatus() {}\n' > "$WORK/tree/crates/fiddle-core/src/squashed.rs"
adr "021-a-decision" "git status"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "removing the blanks must not manufacture a match → exit 1" 1 "$EXIT_CODE"
assert_contains "reports the entry as written" "Cites: git status resolves to nothing" "$OUT"

fresh
adr "021-a-decision" "$(printf '  selected  ,\tSeverities\t')"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "blanks at an entry's ends are trimmed → exit 0" 0 "$EXIT_CODE"

echo ""
echo "=== Test 15: an ADR cannot satisfy its own citation ==="
fresh
adr "021-a-decision" "phantom_symbol"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "the Cites: line is not evidence → exit 1" 1 "$EXIT_CODE"
if grep -qF 'phantom_symbol' "$WORK/tree/docs/technical/decisions/021-a-decision.md"; then
  PASS=$((PASS+1)); echo "  PASS: the ADR holds the symbol, so a tree-wide grep would have passed it"
else
  FAIL=$((FAIL+1)); echo "  FAIL: the premise is a symbol the ADR itself holds"
fi

fresh
adr "021-a-decision" "phantom_symbol"
adr "022-another" "selected"
printf 'The neighbour names phantom_symbol.\n' >> "$WORK/tree/docs/technical/decisions/022-another.md"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a neighbouring ADR is not evidence → exit 1" 1 "$EXIT_CODE"

echo ""
echo "=== Test 16: the repository's own ADRs pass ==="
EXIT_CODE=0
OUT=$("$SCRIPT_DIR/check-adr-cites.sh" --root "$SCRIPT_DIR/.." 2>&1) || EXIT_CODE=$?
assert_exit "this tree's decisions/ → exit 0" 0 "$EXIT_CODE"
assert_contains "nothing unresolved" "0 unresolved" "$OUT"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
