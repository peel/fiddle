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
  if [[ "$haystack" == *"$needle"* ]]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected to find '$needle' in:)"
    printf '%s\n' "$haystack" | sed 's/^/    /'
  fi
}

assert_absent() {
  local desc="$1" needle="$2" haystack="$3"
  if [[ "$haystack" != *"$needle"* ]]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (found '$needle', which should not be there)"
  fi
}

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

fixture() {
  local name="$1"
  DIR="$WORK/$name"
  mkdir -p "$DIR"
  printf '%s' "" > "$DIR/test.log"
}

enumerate() {
  : > "$DIR/enumerated"
  : > "$DIR/targets.json"
  for exe in "$@"; do
    printf '{"reason":"compiler-artifact","profile":{"test":true},"executable":"/repo/target/debug/deps/%s"}\n' \
      "$exe" >> "$DIR/targets.json"
  done
  printf '{"reason":"compiler-artifact","profile":{"test":false},"executable":"/repo/target/debug/libfiddle_core.rlib"}\n' \
    >> "$DIR/targets.json"
}

doctest_packages() {
  local entries=""
  for name in "$@"; do
    [ -n "$entries" ] && entries="$entries,"
    entries="$entries{\"name\":\"${name//_/-}\",\"targets\":[{\"name\":\"$name\",\"kind\":[\"lib\"],\"doctest\":true}]}"
  done
  printf '{"packages":[%s]}\n' "$entries" > "$DIR/metadata.json"
}

lane() {
  printf '     Running %s (target/debug/deps/%s)\n' "$1" "$2" >> "$DIR/test.log"
}

doc_lane() {
  printf '   Doc-tests %s\n' "$1" >> "$DIR/test.log"
}

result_ok() {
  printf 'test result: ok. %s passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n' "$1" >> "$DIR/test.log"
}

result_failed() {
  printf 'test result: FAILED. %s passed; %s failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n' "$1" "$2" >> "$DIR/test.log"
}

run() {
  RC=0
  OUT=$("$SCRIPT_DIR/gate-report.sh" "$DIR" 2>&1) || RC=$?
}

echo "Test 1: a complete green run reports every lane against the enumerated total"
fixture complete-green
enumerate fiddle_core-1111 binary_repair-2222
doctest_packages fiddle_core
lane "unittests src/lib.rs" fiddle_core-1111; result_ok 40
lane "tests/binary_repair.rs" binary_repair-2222; result_ok 12
doc_lane fiddle_core; result_ok 2
run
assert_exit "complete green -> exit 0" 0 "$RC"
assert_contains "TOTALS names the run and the total" "TOTALS: 54 passed, 0 failed, 0 ignored, 3 of 3 binaries" "$OUT"
assert_absent "no INCOMPLETE banner on a complete run" "INCOMPLETE" "$OUT"

echo "Test 2: the truncated red run the gate used to report as a denominator"
fixture truncated-red
enumerate binary_repair-2222222222222222 capability_selection-3333333333333333 cve_mitigation-4444444444444444 recovery_replay-5555555555555555
doctest_packages fiddle_core
lane "tests/binary_repair.rs" binary_repair-2222222222222222; result_ok 12
lane "tests/capability_selection.rs" capability_selection-3333333333333333; result_ok 86
lane "tests/cve_mitigation.rs" cve_mitigation-4444444444444444; result_failed 0 14
printf 'error: test failed, to rerun pass `-p fiddle-acceptance --test cve_mitigation`\n' >> "$DIR/test.log"
run
assert_exit "truncated red -> exit 1" 1 "$RC"
assert_contains "TOTALS carries the shortfall, not a bare count" "3 of 5 binaries" "$OUT"
assert_contains "says the run is incomplete" "INCOMPLETE: 3 of 5 test binaries reported a result." "$OUT"
assert_contains "refuses to let TOTALS read as coverage" "not what this workspace covers" "$OUT"
assert_contains "names the lane that never ran without its cargo hash" "  recovery_replay" "$OUT"
assert_contains "names the doc-test lane that never ran" "doc:fiddle_core" "$OUT"
assert_absent "does not list a lane that did run" "binary_repair" "${OUT#*NOT REACHED}"
assert_contains "still names the failing lane" "!! cve_mitigation" "$OUT"

echo "Test 3: a complete red run reports failures without an incompleteness banner"
fixture complete-red
enumerate fiddle_core-1111 cve_mitigation-4444
doctest_packages fiddle_core
lane "unittests src/lib.rs" fiddle_core-1111; result_ok 40
lane "tests/cve_mitigation.rs" cve_mitigation-4444; result_failed 0 14
doc_lane fiddle_core; result_ok 2
run
assert_exit "complete red -> exit 1" 1 "$RC"
assert_contains "TOTALS shows full coverage" "3 of 3 binaries" "$OUT"
assert_absent "no INCOMPLETE banner" "INCOMPLETE" "$OUT"
assert_contains "attributes the failure to the right lane" "!! cve_mitigation" "$OUT"
assert_contains "attributes the passing lane to the right row" "unit:src/lib" "$OUT"

echo "Test 4: a result line following no header is an attribution failure, not a row"
fixture orphan-result
enumerate fiddle_core-1111
doctest_packages fiddle_core
result_ok 40
lane "unittests src/lib.rs" fiddle_core-1111; result_ok 40
doc_lane fiddle_core; result_ok 2
run
assert_exit "orphan result -> exit 3" 3 "$RC"
assert_contains "says the report is unreliable" "REPORT UNRELIABLE" "$OUT"
assert_contains "says why the table cannot be trusted" "attributes them to the wrong lane" "$OUT"

echo "Test 5: with no enumeration the count is declared unknown rather than complete"
fixture no-enumeration
lane "tests/binary_repair.rs" binary_repair-2222; result_ok 12
printf 'error: could not compile `fiddle-core` (lib test)\n' > "$DIR/targets.log"
run
assert_exit "no enumeration -> exit 1" 1 "$RC"
assert_contains "TOTALS refuses to imply a total" "1 binaries of an unknown total" "$OUT"
assert_contains "says coverage is unknown" "COVERAGE UNKNOWN" "$OUT"
assert_contains "quotes why the enumeration failed" "could not compile" "$OUT"

echo "Test 6: a binary that starts and produces no result is distinguished from one never reached"
fixture started-no-result
enumerate fiddle_core-1111 crasher-6666
doctest_packages fiddle_core
lane "unittests src/lib.rs" fiddle_core-1111; result_ok 40
lane "tests/crasher.rs" crasher-6666
printf 'error: test failed, to rerun pass `-p fiddle-core --test crasher`\n' >> "$DIR/test.log"
doc_lane fiddle_core; result_ok 2
run
assert_exit "started but no result -> exit 1" 1 "$RC"
assert_contains "says the run is incomplete" "INCOMPLETE: 2 of 3" "$OUT"
assert_contains "names the shortfall as a crash, not a skipped lane" "no test result line at all" "$OUT"
assert_absent "does not claim the lane was never reached" "NOT REACHED" "$OUT"

echo "Test 7: shell-hook noise before cargo output creates no phantom lane"
fixture devenv-noise
enumerate fiddle_core-1111
doctest_packages fiddle_core
{
  printf 'Running tasks     devenv:enterShell\n'
  printf 'Running           devenv:files:cleanup\n'
  printf 'Running           devenv:enterTest\n'
} > "$DIR/test.log"
lane "unittests src/lib.rs" fiddle_core-1111; result_ok 40
doc_lane fiddle_core; result_ok 2
run
assert_exit "devenv noise -> exit 0" 0 "$RC"
assert_contains "counts only the real lanes" "2 of 2 binaries" "$OUT"
assert_absent "no devenv lane in the table" "devenv" "$OUT"

echo "Test 8: a missing test log measures nothing and says so"
fixture missing-log
rm -f "$DIR/test.log"
run
assert_exit "missing test log -> exit 3" 3 "$RC"
assert_contains "says it measured nothing" "measured nothing" "$OUT"

echo "Test 9: a log directory that does not exist is an input error"
DIR="$WORK/does-not-exist"
run
assert_exit "missing log dir -> exit 2" 2 "$RC"

echo "Test 10: a coloured log reports exactly what the same log without colour reports"
fixture plain-lanes
enumerate fiddle_core-1111 binary_repair-2222
doctest_packages fiddle_core
lane "unittests src/lib.rs" fiddle_core-1111; result_ok 40
lane "tests/binary_repair.rs" binary_repair-2222; result_ok 12
doc_lane fiddle_core; result_ok 2
run
PLAIN_OUT="$OUT"; PLAIN_RC="$RC"

fixture coloured-lanes
enumerate fiddle_core-1111 binary_repair-2222
doctest_packages fiddle_core
printf '\033[1m\033[92m     Running\033[0m unittests src/lib.rs (target/debug/deps/fiddle_core-1111)\n' >> "$DIR/test.log"
result_ok 40
printf '\033[1m\033[92m     Running\033[0m tests/binary_repair.rs (target/debug/deps/binary_repair-2222)\n' >> "$DIR/test.log"
result_ok 12
printf '\033[1m\033[92m   Doc-tests\033[0m fiddle_core\n' >> "$DIR/test.log"
result_ok 2
run

assert_exit "coloured lanes -> exit 0" 0 "$RC"
assert_exit "the plain control also passes" 0 "$PLAIN_RC"
assert_contains "TOTALS counts every lane" "TOTALS: 54 passed, 0 failed, 0 ignored, 3 of 3 binaries" "$OUT"
assert_contains "names the unit lane, not a question mark" "  unit:src/lib" "$OUT"
assert_contains "names the integration lane" "  binary_repair" "$OUT"
assert_absent "no orphan row survives" "?" "$OUT"
assert_absent "the report is not refused" "REPORT UNRELIABLE" "$OUT"
if [ "$OUT" = "$PLAIN_OUT" ]; then
  PASS=$((PASS+1)); echo "  PASS: colour changes nothing in the report"
else
  FAIL=$((FAIL+1)); echo "  FAIL: colour changes the report"
  printf 'coloured:\n%s\nplain:\n%s\n' "$OUT" "$PLAIN_OUT" | sed 's/^/    /'
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
