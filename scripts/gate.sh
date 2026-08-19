#!/usr/bin/env bash
set -uo pipefail

MODE="${1:-default}"
cd "$(dirname "$0")/.." || exit 2

LOG_DIR=$(mktemp -d "${TMPDIR:-/tmp}/fiddle-gate-XXXXXX") || exit 2
trap 'rm -rf "$LOG_DIR"' EXIT INT TERM
echo "logs: $LOG_DIR"

FAILED=0
INNER=0

nix develop -c bash -uo pipefail -c '
  LOG_DIR="$1"
  fail=0
  t0=$(date +%s); cargo fmt --all --check                                              > "$LOG_DIR/fmt.log"    2>&1 || fail=1
  t1=$(date +%s); cargo clippy --workspace --all-targets --all-features -- -D warnings > "$LOG_DIR/clippy.log" 2>&1 || fail=1
  t2=$(date +%s); cargo test --workspace --all-features                                > "$LOG_DIR/test.log"   2>&1 || fail=1
  t3=$(date +%s)
  printf "fmt %ds  clippy %ds  test %ds\n" $((t1-t0)) $((t2-t1)) $((t3-t2))
  exit $fail
' _ "$LOG_DIR" || INNER=1
[ "$INNER" -ne 0 ] && FAILED=1

TEST_LOG="$LOG_DIR/test.log"

awk '
  /^ *(Running|Doc-tests)/ {
    name = $0
    sub(/.*Running +/, "", name); sub(/.*Doc-tests +/, "", name)
    sub(/ *\(.*/, "", name); sub(/^unittests +/, "unit:", name); sub(/^tests\//, "", name); sub(/\.rs$/, "", name)
    pending = name; next
  }
  /^test result:/ {
    for (i = 1; i <= NF; i++) {
      if ($i == "passed;")  p = $(i-1)
      if ($i == "failed;")  f = $(i-1)
      if ($i == "ignored;") g = $(i-1)
    }
    P += p; F += f; G += g; rows++
    if (f > 0) bad = bad sprintf("  !! %-26s %s passed, %s FAILED\n", pending, p, f)
    else       tbl = tbl sprintf("  %-26s %s\n", pending, p)
    pending = "?"
  }
  END {
    printf "%s", tbl
    if (bad != "") { printf "\nFAILING LANES:\n%s", bad }
    printf "\n  TOTALS: %d passed, %d failed, %d ignored, %d binaries\n", P, F, G, rows
    printf "ROWS %d\n", rows > "/dev/stderr"
    printf "FAILED %d\n", F > "/dev/stderr"
  }
' "$TEST_LOG" 2> "$LOG_DIR/counts" || FAILED=1

ROWS=$(awk '/^ROWS/{print $2}' "$LOG_DIR/counts")
TFAIL=$(awk '/^FAILED/{print $2}' "$LOG_DIR/counts")
RESULTS=$(grep -c '^test result:' "$TEST_LOG" 2>/dev/null); RESULTS=${RESULTS:-0}
MARKERS=$(grep -c '^test result: FAILED' "$TEST_LOG" 2>/dev/null); MARKERS=${MARKERS:-0}

if [ "${ROWS:-0}" != "$RESULTS" ]; then
  echo; echo "REPORT UNRELIABLE: table has ${ROWS:-0} rows but the log has $RESULTS results."
  echo "Do not quote these numbers. Log kept at: $LOG_DIR"; trap - EXIT; exit 3
fi
if [ "${TFAIL:-0}" != "0" ] || [ "$MARKERS" != "0" ]; then FAILED=1; fi

for L in fmt clippy; do
  if [ -s "$LOG_DIR/$L.log" ] && [ "$INNER" -ne 0 ]; then echo "--- $L ---"; tail -20 "$LOG_DIR/$L.log"; fi
done
[ "$FAILED" -ne 0 ] && { echo "--- last failing test output ---"; grep -B 2 -A 12 '^failures:' "$TEST_LOG" 2>/dev/null | head -40; }

if [ "$MODE" != "--quick" ]; then
  SHELL_PASS=0; SHELL_FAIL=0; SHELL_TOTAL=0
  : > "$LOG_DIR/shell.log"
  for t in scripts/test-*.sh; do
    [ -f "$t" ] || continue
    SHELL_TOTAL=$((SHELL_TOTAL + 1))
    bash "$t" > "$LOG_DIR/shell-$(basename "$t" .sh).log" 2>&1
    rc=$?
    printf '%s exit=%d\n' "$t" "$rc" >> "$LOG_DIR/shell.log"
    if [ "$rc" -eq 0 ]; then
      SHELL_PASS=$((SHELL_PASS + 1))
    else
      SHELL_FAIL=$((SHELL_FAIL + 1))
      echo "  !! $t exit=$rc"
      tail -12 "$LOG_DIR/shell-$(basename "$t" .sh).log"
    fi
  done
  if [ "$SHELL_TOTAL" -eq 0 ]; then
    echo "  REPORT UNRELIABLE: no scripts/test-*.sh matched, so this step measured nothing."
    FAILED=1
  fi
  printf "  SHELL SUITES: %d passed, %d failed of %d\n" "$SHELL_PASS" "$SHELL_FAIL" "$SHELL_TOTAL"
  [ "$SHELL_FAIL" -ne 0 ] && FAILED=1
fi

if [ "$MODE" != "--quick" ]; then
  if nix develop -c cargo build --release > "$LOG_DIR/build.log" 2>&1; then echo "build --release  ok"; else echo "build --release  FAILED"; tail -20 "$LOG_DIR/build.log"; FAILED=1; fi
fi
if [ "$MODE" = "--full" ]; then
  if nix flake check > "$LOG_DIR/flake.log" 2>&1; then echo "nix flake check  ok"; else echo "nix flake check  FAILED"; tail -20 "$LOG_DIR/flake.log"; FAILED=1; fi
fi

echo
[ "$FAILED" -eq 0 ] && echo "GATE: PASS" || { echo "GATE: FAIL  (logs kept: $LOG_DIR)"; trap - EXIT; }
exit "$FAILED"
