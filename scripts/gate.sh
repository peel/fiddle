#!/usr/bin/env bash
# gate.sh — the accumulated gate, in one nix shell entry.
#
# Every command here was already being run per bean; what this file removes is
# the cost of running them one shell at a time. `nix develop -c <cmd>` costs
# ~2.5s of shell entry, and an implementer verifying a bean was issuing ~18 of
# them — plus, measurably, re-issuing some it had already run, because the model
# recomposes the shell line each time and does not notice the repeat.
#
# It also prints the per-binary test counts, so the named regression lanes do
# not need to be re-run individually to report them. That was the larger waste:
# ten scoped runs after a clean full run, re-proving what the full run proved.
#
# Usage:
#   scripts/gate.sh            # fmt, clippy, test, build --release
#   scripts/gate.sh --full     # the above plus nix flake check
#   scripts/gate.sh --quick    # fmt, clippy, test — no release build
set -uo pipefail

MODE="${1:-default}"
cd "$(dirname "$0")/.." || exit 2
FAILED=0

run() {
  local label="$1"; shift
  local start elapsed status
  start=$(date +%s)
  if "$@" > "/tmp/gate-$label.log" 2>&1; then status="ok"; else status="FAILED"; FAILED=1; fi
  elapsed=$(( $(date +%s) - start ))
  printf '%-22s %-7s %3ds\n' "$label" "$status" "$elapsed"
  [ "$status" = "FAILED" ] && { echo "--- $label ---"; tail -30 "/tmp/gate-$label.log"; }
  return 0
}

# One shell entry for everything cargo. Entering it per command is what this
# script exists to stop.
nix develop -c bash -euo pipefail -c '
  set +e
  fail=0
  t0=$(date +%s); cargo fmt --all --check                                            > /tmp/gate-fmt.log 2>&1    || fail=1
  t1=$(date +%s); cargo clippy --workspace --all-targets --all-features -- -D warnings > /tmp/gate-clippy.log 2>&1 || fail=1
  t2=$(date +%s); cargo test --workspace --all-features                              > /tmp/gate-test.log 2>&1   || fail=1
  t3=$(date +%s)
  printf "fmt %ds  clippy %ds  test %ds\n" $((t1-t0)) $((t2-t1)) $((t3-t2))
  exit $fail
' || FAILED=1

echo
echo "=== per-binary counts (parse these; do not re-run the lanes individually) ==="
grep -E "^(test result:|     Running|   Doc-tests)" /tmp/gate-test.log 2>/dev/null \
  | paste - - 2>/dev/null | sed 's/  */ /g' | head -40
echo
echo "=== totals ==="
awk '/[0-9]+ passed/{for(i=1;i<=NF;i++){if($i=="passed;")p+=$(i-1); if($i=="failed;")f+=$(i-1); if($i=="ignored;")g+=$(i-1)}} END{printf "  %d passed, %d failed, %d ignored, %d binaries\n", p, f, g, n}' n="$(grep -c 'test result:' /tmp/gate-test.log 2>/dev/null)" /tmp/gate-test.log 2>/dev/null

if [ "$MODE" != "--quick" ]; then
  run "build --release" nix develop -c cargo build --release
fi
if [ "$MODE" = "--full" ]; then
  run "nix flake check" nix flake check
fi

echo
[ "$FAILED" -eq 0 ] && echo "GATE: PASS" || echo "GATE: FAIL"
exit "$FAILED"
