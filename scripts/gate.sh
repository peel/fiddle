#!/usr/bin/env bash
# gate.sh — the accumulated gate, in one nix shell entry.
#
# `nix develop -c <cmd>` costs ~2.5s of shell entry, and an implementer verifying
# a bean was issuing ~18 of them — plus, measurably, re-issuing some it had
# already run, because the model recomposes the shell line each time and does not
# notice the repeat. This runs them once and prints the per-binary counts, so the
# named regression lanes need not be re-run individually to report them.
#
# Two rules this file learned the hard way, on its first day, from an implementer
# that checked its output against its own log:
#
#   1. LOGS ARE PER-INVOCATION. The first version wrote to fixed /tmp paths. Five
#      worktrees exist under .worktrees/; two agents running this at once
#      truncated and interleaved into the same files, and whichever finished last
#      left a file a different agent had already parsed. It printed GATE: FAIL
#      over a green tree once, and a table missing a whole lane — with every
#      label after it shifted by one — the next run.
#   2. A REPORT THAT CAN DISAGREE WITH ITS LOG MUST SAY SO. The old table paired
#      lines with `paste - -`, which assumes strict alternation in the grep
#      stream. When that assumption breaks the table is silently wrong, and it is
#      wrong in both directions: it can hide a real failure as easily as invent
#      one. This version derives every number in a single awk pass and refuses to
#      print a table it cannot reconcile with the log.
#
# Usage:
#   scripts/gate.sh            # fmt, clippy, test, shell suites, build --release
#   scripts/gate.sh --full     # the above plus nix flake check
#   scripts/gate.sh --quick    # fmt, clippy, test only (no shell suites)
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

# One pass, a two-state machine: remember the binary a `Running`/`Doc-tests` line
# names, attribute the next `test result:` to it. Nothing depends on stream
# parity, so a line arriving out of order cannot shift a label onto another lane.
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

# Reconcile: the table must account for exactly the `test result:` lines in the
# log, and the log's own failure count must agree. If they do not, the table is
# not evidence and must not be presented as any.
ROWS=$(awk '/^ROWS/{print $2}' "$LOG_DIR/counts")
TFAIL=$(awk '/^FAILED/{print $2}' "$LOG_DIR/counts")
# `grep -c` prints its count AND exits 1 when the count is zero, so a
# `|| echo 0` fallback appends a SECOND zero and the variable becomes "0\n0",
# which compares unequal to "0". That is how this script's first fix printed
# GATE: FAIL over a green tree — the same false alarm it was written to stop,
# one layer down. Let the count stand on its own and default it explicitly.
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

# The shell suites, which until 2026-08-13 this gate did not run at all. Two of
# them had been red on `main` for long enough that a person found them by reading
# a transcript at finish-branch rather than the day the change landed — and the
# change was a hook gaining an ownership guard, so the suites were stale rather
# than the behaviour wrong. 49s measured against the Rust step's ~194s.
#
# Three things this step deliberately does not do, each of which was measured:
#
#   1. IT DOES NOT FOLD INTO `TOTALS`. The awk above attributes `test result:`
#      lines to Rust binaries. Two suites print no parseable result line at all
#      — one says "Maki installer tests passed", another prints a bare
#      " 19 passed, 0 failed" — so a parser keyed on either would read two green
#      suites as having measured nothing. This gets its own tally line.
#   2. IT DOES NOT USE THE LOOP'S LAST EXIT CODE. `for t in …; do bash "$t"; done`
#      exits with the *last* suite's status, which is the same defect as a
#      driver's exit code standing in for a lane's. Every suite's code is
#      recorded against its name.
#   3. IT REFUSES AN EMPTY GLOB. Zero suites run and zero suites failing must not
#      render identically; a moved directory would otherwise read as a pass.
#
# They need nothing the dev shell adds — bash, jq and coreutils — so they run
# without a second `nix develop` entry. Three of them bind a port or spawn
# processes; that is the whole flake surface, and it is deliberately gated rather
# than excluded, because the runtime lifecycle is precisely what went stale here.
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
