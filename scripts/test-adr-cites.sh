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

adr_body() {
  local name="$1" cites="$2" retired="$3" body="$4"
  {
    printf '# %s\n\n' "$name"
    printf 'Status: accepted\n'
    [ -n "$cites" ] && printf 'Cites: %s\n' "$cites"
    [ -n "$retired" ] && printf 'Retired: %s\n' "$retired"
    printf '\n## Decision\n\n%s\n' "$body"
  } > "$WORK/tree/docs/technical/decisions/$name.md"
}

run() { "$SCRIPT_DIR/check-adr-cites.sh" --root "$WORK/tree" "$@"; }

echo "=== Test 1: every cited symbol resolves under crates/ ==="
fresh
adr "021-a-decision" "fiddle_core::selected, Severities"
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "resolving Cites → exit 0" 0 "$EXIT_CODE"
assert_contains "reports what it measured" "2 cited symbols, 0 body names, 0 retired names, 0 unresolved" "$OUT"

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
echo "=== Test 17: a test name the body cites must resolve ==="
fresh
printf 'fn a_run_that_reads_the_claim_files_no_second_issue() {}\n' \
  > "$WORK/tree/crates/fiddle-acceptance/tests/claims.rs"
adr_body "021-a-decision" "selected" "" \
  'The bound is held by `a_run_that_reads_the_claim_files_no_second_issue`.'
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a body name the tree holds → exit 0" 0 "$EXIT_CODE"
assert_contains "the body is measured and its count reported" "1 body names" "$OUT"

fresh
adr_body "021-a-decision" "selected" "" \
  'The bound is held by `a_run_that_reads_the_claim_files_no_second_issue`.'
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a body name nothing holds → exit 1" 1 "$EXIT_CODE"
assert_contains "names the ADR and the dangling name" \
  "021-a-decision.md: the body cites a_run_that_reads_the_claim_files_no_second_issue" "$OUT"

fresh
adr_body "021-a-decision" "selected" "" \
  'Held by `a_run_that_reads_the_claim_files_no_second_issue`.'
adr_body "022-another" "selected" "" \
  'The neighbour names `a_run_that_reads_the_claim_files_no_second_issue` too.'
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a neighbouring ADR is not evidence for a body name → exit 1" 1 "$EXIT_CODE"
assert_contains "both ADRs are reported, not one" "022-another.md: the body cites" "$OUT"

echo ""
echo "=== Test 18: the body rule counts test-shaped names and nothing else ==="
fresh
printf 'pub fn selected() {}\n' > "$WORK/tree/crates/fiddle-core/src/finding.rs"
adr_body "021-a-decision" "selected" "" \
  'A `Filing` carrying `effect_id` and `two_word` and `three_word_name`, beside `a_name_of_four_words`.'
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "short backticked names are not body citations → exit 1" 1 "$EXIT_CODE"
assert_contains "only the four-part name was measured" "1 body names" "$OUT"
assert_contains "and it is the one reported" "the body cites a_name_of_four_words" "$OUT"

fresh
adr_body "021-a-decision" "selected" "" \
  'Neither `a_name_of_four_words` nor `another_name_of_four_words` resolves.'
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "two dangling names → exit 1" 1 "$EXIT_CODE"
assert_contains "the denominator counts both" "2 body names" "$OUT"
assert_contains "and the unresolved count is not one" "2 unresolved" "$OUT"

echo ""
echo "=== Test 19: Retired: names what the tree no longer holds, and is policed ==="
fresh
adr_body "021-a-decision" "selected" "a_name_of_four_words" \
  'The helper `a_name_of_four_words` is gone.'
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a retired body name → exit 0" 0 "$EXIT_CODE"
assert_contains "it is counted as retired, not as a body citation" "0 body names, 1 retired names" "$OUT"

fresh
printf 'fn a_name_of_four_words() {}\n' > "$WORK/tree/crates/fiddle-acceptance/tests/claims.rs"
adr_body "021-a-decision" "selected" "a_name_of_four_words" \
  'The helper `a_name_of_four_words` is gone.'
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "retiring a name the tree still holds → exit 1" 1 "$EXIT_CODE"
assert_contains "says the retirement is false" \
  "Retired: a_name_of_four_words still resolves" "$OUT"

fresh
adr_body "021-a-decision" "selected" "a_name_of_four_words" \
  'This record names no such thing.'
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "retiring a name the body never cites → exit 1" 1 "$EXIT_CODE"
assert_contains "says the entry is dead" \
  "Retired: a_name_of_four_words is named nowhere in the body" "$OUT"

fresh
adr_body "021-a-decision" "selected" "a_name_of_four_words" \
  'The helper `a_name_of_four_words` is gone, and `another_name_of_four_words` is not.'
EXIT_CODE=0
OUT=$(run 2>&1) || EXIT_CODE=$?
assert_exit "a retirement exempts one name and not its neighbour → exit 1" 1 "$EXIT_CODE"
assert_contains "the neighbour is still measured" \
  "the body cites another_name_of_four_words" "$OUT"

echo ""
echo "=== Test 20: the repository's own ADRs pass ==="
EXIT_CODE=0
OUT=$("$SCRIPT_DIR/check-adr-cites.sh" --root "$SCRIPT_DIR/.." 2>&1) || EXIT_CODE=$?
assert_exit "this tree's decisions/ → exit 0" 0 "$EXIT_CODE"
assert_contains "nothing unresolved" "0 unresolved" "$OUT"
if printf '%s' "$OUT" | grep -qE '[1-9][0-9]* body names'; then
  PASS=$((PASS+1)); echo "  PASS: the bodies of this tree's ADRs were measured, not skipped"
else
  FAIL=$((FAIL+1)); echo "  FAIL: 0 body names means the body rule measured nothing: $OUT"
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
