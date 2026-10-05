#!/usr/bin/env bash
set -uo pipefail

SCRIPT="$(cd "$(dirname "$0")" && pwd)/check-free-space.sh"
FAILED=0
CHECKED=0

fail() { echo "test-check-free-space: FAIL: $*" >&2; FAILED=$((FAILED + 1)); }

expect() {
  CHECKED=$((CHECKED + 1))
  local what="$1" code="$2" words="$3"; shift 3
  local out rc
  out=$("$@" 2>&1); rc=$?
  [ "$rc" -eq "$code" ] || fail "$what: exited $rc, expected $code: $out"
  case "$out" in *"$words"*) ;; *) fail "$what: expected '$words' in: $out" ;; esac
}

DIR=$(mktemp -d "${TMPDIR:-/tmp}/check-free-space-XXXXXX") || exit 2
trap 'rm -rf "$DIR"' EXIT
mkdir -p "$DIR/target"
head -c 65536 /dev/zero > "$DIR/target/built"

expect "a small floor passes and prints both numbers" 0 "KB needed" \
  env FIDDLE_GATE_FLOOR_KB=1 bash "$SCRIPT" "$DIR" "$DIR/target"
expect "the build output, being larger than the floor, is what is needed" 0 "the size of $DIR/target" \
  env FIDDLE_GATE_FLOOR_KB=1 bash "$SCRIPT" "$DIR" "$DIR/target"
expect "a floor larger than the build output says the floor decided it" 0 "the floor, which is more than" \
  env FIDDLE_GATE_FLOOR_KB=999999 bash "$SCRIPT" "$DIR" "$DIR/target"
expect "a floor no disk holds refuses with exit 2 and a reason" 2 "CANNOT RUN" \
  env FIDDLE_GATE_FLOOR_KB=999999999999 bash "$SCRIPT" "$DIR" "$DIR/target"
expect "a refusal names what to reclaim" 2 "reclaim the target directories" \
  env FIDDLE_GATE_FLOOR_KB=999999999999 bash "$SCRIPT" "$DIR"
expect "a directory that does not exist is refused, not measured as empty" 2 "does not exist" \
  bash "$SCRIPT" "$DIR/absent"
expect "a floor that is not a number is refused" 2 "not a number" \
  env FIDDLE_GATE_FLOOR_KB=ten bash "$SCRIPT" "$DIR"

[ "$CHECKED" -eq 7 ] || { echo "test-check-free-space: ran $CHECKED of 7 checks" >&2; exit 2; }
[ "$FAILED" -eq 0 ] || exit 1
echo "test-check-free-space: 7 of 7 checks passed"
