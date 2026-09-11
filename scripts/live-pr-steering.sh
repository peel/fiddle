#!/usr/bin/env bash
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2

REPO="${FIDDLE_LIVE_REPO:-}"
PR="${FIDDLE_LIVE_PR:-}"
BRANCH="${FIDDLE_LIVE_BRANCH:-}"
BASE="${FIDDLE_LIVE_BASE:-main}"

fail() { echo "live-pr-steering: FAIL: $*" >&2; exit 1; }
note() { echo "live-pr-steering: $*"; }

[ -n "$REPO" ] || fail "FIDDLE_LIVE_REPO is unset. This lane reads a real pull request through fiddle's own reader, and a lane that read nothing would report the same silence as a lane that found no direction."
[ -n "$PR" ] || fail "FIDDLE_LIVE_PR is unset, so there is no pull request to read."
[ -n "$BRANCH" ] || fail "FIDDLE_LIVE_BRANCH is unset, so the lookup half of this lane has no branch to find the pull request from."

command -v gh >/dev/null 2>&1 || fail "gh is not on the PATH, and this lane reads github through it exactly as a run does."

TOKEN="${FIDDLE_LIVE_TOKEN:-}"
if [ -z "$TOKEN" ]; then
  TOKEN=$(gh auth token 2>/dev/null) || true
  [ -n "$TOKEN" ] || fail "no FIDDLE_LIVE_TOKEN and gh holds no token, so nothing can authenticate."
  note "no FIDDLE_LIVE_TOKEN, so the token gh already holds is used"
fi

note "reading $REPO#$PR and branch $BRANCH into $BASE"

FIDDLE_LIVE_TOKEN="$TOKEN" \
FIDDLE_LIVE_REPO="$REPO" \
FIDDLE_LIVE_PR="$PR" \
FIDDLE_LIVE_BRANCH="$BRANCH" \
FIDDLE_LIVE_BASE="$BASE" \
FIDDLE_LIVE_HEAD="${FIDDLE_LIVE_HEAD:-}" \
FIDDLE_LIVE_EXPECT_DIRECTION="${FIDDLE_LIVE_EXPECT_DIRECTION:-}" \
  cargo test --all-features --test live_pr_steering -- --ignored --nocapture --test-threads=1
RC=$?

if [ "$RC" -ne 0 ]; then
  echo "live-pr-steering: FAIL (cargo test exited $RC)" >&2
  exit 1
fi

echo "live-pr-steering: PASS ($REPO#$PR read through fiddle's own reader, nothing written)"
