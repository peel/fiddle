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
  t2=$(date +%s)
  if cargo test --workspace --all-features --no-run --message-format=json > "$LOG_DIR/targets.json" 2> "$LOG_DIR/targets.log" \
     && cargo metadata --no-deps --format-version 1 > "$LOG_DIR/metadata.json" 2> "$LOG_DIR/metadata.log"; then
    : > "$LOG_DIR/enumerated"
  else
    fail=1
  fi
  cargo test --workspace --all-features --no-fail-fast > "$LOG_DIR/test.log" 2>&1 || fail=1
  t3=$(date +%s)
  printf "fmt %ds  clippy %ds  test %ds\n" $((t1-t0)) $((t2-t1)) $((t3-t2))
  exit $fail
' _ "$LOG_DIR" || INNER=1
[ "$INNER" -ne 0 ] && FAILED=1

TEST_LOG="$LOG_DIR/test.log"

REPORT=0
scripts/gate-report.sh "$LOG_DIR" || REPORT=$?
if [ "$REPORT" -eq 3 ] || [ "$REPORT" -eq 2 ]; then trap - EXIT; exit 3; fi
[ "$REPORT" -ne 0 ] && FAILED=1

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
