#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LANE="$SCRIPT_DIR/live-pr-steering.sh"
LANE_TEST="$SCRIPT_DIR/../crates/fiddle-runtime/tests/live_pr_steering.rs"

FAILED=0
CHECKED=0

fail() { echo "test-live-pr-steering: FAIL: $*" >&2; FAILED=$((FAILED + 1)); }

check() {
  CHECKED=$((CHECKED + 1))
  local what="$1" expected="$2" haystack="$3"
  case "$haystack" in
    *"$expected"*) ;;
    *) fail "$what (expected to contain '$expected' in: $haystack)" ;;
  esac
}

refuses() {
  CHECKED=$((CHECKED + 1))
  local what="$1"
  shift
  local out rc
  out=$(env -u FIDDLE_LIVE_REPO -u FIDDLE_LIVE_PR -u FIDDLE_LIVE_BRANCH "$@" bash "$LANE" 2>&1)
  rc=$?
  if [ "$rc" -eq 0 ]; then
    fail "$what: the lane exited 0 with its environment incomplete, so a lane that read nothing is indistinguishable from one that found no direction: $out"
    return
  fi
  case "$out" in
    *FAIL*) ;;
    *) fail "$what: the lane exited $rc and said nothing that names the refusal: $out" ;;
  esac
}

[ -f "$LANE" ] || { echo "test-live-pr-steering: $LANE is absent" >&2; exit 2; }
[ -f "$LANE_TEST" ] || { echo "test-live-pr-steering: $LANE_TEST is absent" >&2; exit 2; }

echo "an incomplete environment is refused rather than skipped"
refuses "no variable at all"
refuses "a repository and no pull request" FIDDLE_LIVE_REPO=acme/widget
refuses "a repository and a pull request and no branch" FIDDLE_LIVE_REPO=acme/widget FIDDLE_LIVE_PR=7

echo "the refusal names the variable it wanted"
OUT=$(env -u FIDDLE_LIVE_REPO -u FIDDLE_LIVE_PR -u FIDDLE_LIVE_BRANCH bash "$LANE" 2>&1)
check "the first refusal names FIDDLE_LIVE_REPO" "FIDDLE_LIVE_REPO is unset" "$OUT"

OUT=$(env -u FIDDLE_LIVE_PR -u FIDDLE_LIVE_BRANCH FIDDLE_LIVE_REPO=acme/widget bash "$LANE" 2>&1)
check "the second refusal names FIDDLE_LIVE_PR" "FIDDLE_LIVE_PR is unset" "$OUT"

OUT=$(env -u FIDDLE_LIVE_BRANCH FIDDLE_LIVE_REPO=acme/widget FIDDLE_LIVE_PR=7 bash "$LANE" 2>&1)
check "the third refusal names FIDDLE_LIVE_BRANCH" "FIDDLE_LIVE_BRANCH is unset" "$OUT"

echo "the lane writes nothing, and says so in its own text"
CHECKED=$((CHECKED + 1))
if grep -qE '"(POST|PATCH|PUT|DELETE)"' "$LANE_TEST"; then
  fail "the lane's test names a mutating method, and this lane reads a pull request and writes to none"
fi
check "the lane's pass line says nothing was written" "nothing written" "$(cat "$LANE")"

echo "the rust lane refuses an absent variable rather than returning an empty reading"
CHECKED=$((CHECKED + 1))
grep -q "refuses rather than skips" "$LANE_TEST" \
  || fail "the rust lane does not say why an absent variable is a refusal"

if [ "$FAILED" -ne 0 ]; then
  echo "test-live-pr-steering: FAIL ($FAILED of $CHECKED checks)" >&2
  exit 1
fi
echo "test-live-pr-steering: PASS ($CHECKED checks)"
