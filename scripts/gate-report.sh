#!/usr/bin/env bash
set -uo pipefail

LOG_DIR="${1:-}"
if [ -z "$LOG_DIR" ] || [ ! -d "$LOG_DIR" ]; then
  echo "usage: gate-report.sh <log-dir>" >&2
  exit 2
fi

FAILED=0

TEST_LOG="$LOG_DIR/test.log"
if [ ! -f "$TEST_LOG" ]; then
  echo "REPORT UNRELIABLE: no test log at $TEST_LOG, so this step measured nothing." >&2
  exit 3
fi
EXPECTED_LANES="$LOG_DIR/expected-lanes"
OBSERVED_LANES="$LOG_DIR/observed-lanes"

: > "$EXPECTED_LANES"
if [ -f "$LOG_DIR/enumerated" ]; then
  {
    jq -r 'select(.reason == "compiler-artifact") | select(.profile.test == true) | .executable | select(. != null)' \
      "$LOG_DIR/targets.json" | sed 's|.*/||'
    jq -r '.packages[] | .targets[] | select(.doctest == true) | "doc:\(.name)"' "$LOG_DIR/metadata.json"
  } 2>/dev/null | sort -u > "$EXPECTED_LANES"
fi
EXPECTED=$(wc -l < "$EXPECTED_LANES" | tr -d ' ')

{
  sed -n 's/^ *Running .*(\(.*\))$/\1/p' "$TEST_LOG" | sed 's|.*/||'
  sed -n 's/^ *Doc-tests \([A-Za-z0-9_][A-Za-z0-9_]*\)$/doc:\1/p' "$TEST_LOG"
} | sort -u > "$OBSERVED_LANES"

awk -v expected="$EXPECTED" '
  /^ *Running .*\(.*\)$/ || /^ *Doc-tests [A-Za-z0-9_]+$/ {
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
    if (pending == "?" || pending == "") orphans++
    if (f > 0) bad = bad sprintf("  !! %-26s %s passed, %s FAILED\n", pending, p, f)
    else       tbl = tbl sprintf("  %-26s %s\n", pending, p)
    pending = "?"
  }
  END {
    printf "%s", tbl
    if (bad != "") { printf "\nFAILING LANES:\n%s", bad }
    if (expected + 0 > 0)
      printf "\n  TOTALS: %d passed, %d failed, %d ignored, %d of %d binaries\n", P, F, G, rows, expected
    else
      printf "\n  TOTALS: %d passed, %d failed, %d ignored, %d binaries of an unknown total\n", P, F, G, rows
    printf "ROWS %d\n", rows > "/dev/stderr"
    printf "FAILED %d\n", F > "/dev/stderr"
    printf "ORPHANS %d\n", orphans + 0 > "/dev/stderr"
  }
' "$TEST_LOG" 2> "$LOG_DIR/counts" || FAILED=1

ROWS=$(awk '/^ROWS/{print $2}' "$LOG_DIR/counts")
TFAIL=$(awk '/^FAILED/{print $2}' "$LOG_DIR/counts")
ORPHANS=$(awk '/^ORPHANS/{print $2}' "$LOG_DIR/counts")
RESULTS=$(grep -c '^test result:' "$TEST_LOG" 2>/dev/null); RESULTS=${RESULTS:-0}
MARKERS=$(grep -c '^test result: FAILED' "$TEST_LOG" 2>/dev/null); MARKERS=${MARKERS:-0}

if [ "${ROWS:-0}" != "$RESULTS" ]; then
  echo; echo "REPORT UNRELIABLE: table has ${ROWS:-0} rows but the log has $RESULTS results."
  echo "Do not quote these numbers. Log kept at: $LOG_DIR"; exit 3
fi
if [ "${ORPHANS:-0}" != "0" ]; then
  echo; echo "REPORT UNRELIABLE: ${ORPHANS} result lines follow no binary header, so the table"
  echo "attributes them to the wrong lane. Do not quote these numbers. Log kept at: $LOG_DIR"
  exit 3
fi

if [ "$EXPECTED" -eq 0 ]; then
  echo
  echo "COVERAGE UNKNOWN: cargo enumerated no test targets, so the ${ROWS:-0} binaries above"
  echo "cannot be checked against a complete run. Read them as a floor, not as coverage."
  tail -5 "$LOG_DIR/targets.log" 2>/dev/null
  FAILED=1
elif [ "${ROWS:-0}" -ne "$EXPECTED" ]; then
  echo
  if [ "${ROWS:-0}" -lt "$EXPECTED" ]; then
    echo "INCOMPLETE: ${ROWS:-0} of $EXPECTED test binaries reported a result."
    echo "The TOTALS above are where the run stopped, not what this workspace covers."
  else
    echo "OVER-COUNTED: ${ROWS:-0} results against $EXPECTED enumerated binaries, so the"
    echo "enumeration is incomplete and the TOTALS cannot be read as coverage."
  fi
  NOT_REACHED=$(comm -23 "$EXPECTED_LANES" "$OBSERVED_LANES")
  if [ -n "$NOT_REACHED" ]; then
    echo "NOT REACHED:"
    printf '%s\n' "$NOT_REACHED" | sed -E 's/-[0-9a-f]{16}$//' | sed 's/^/  /'
  else
    echo "Every enumerated binary was started; the shortfall is binaries that produced"
    echo "no test result line at all — a crash or a signal, not a skipped lane."
  fi
  FAILED=1
fi
if [ "${TFAIL:-0}" != "0" ] || [ "$MARKERS" != "0" ]; then FAILED=1; fi

exit "$FAILED"
