#!/usr/bin/env bash
# parse-eval-log.sh — Extract evaluation state from a bean's body.
# Exit 0 = log found and parsed, 1 = no evaluation log on bean.
set -euo pipefail

BEAN_ID=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bean-id) BEAN_ID="$2"; shift 2;;
    *) echo "Unknown arg: $1" >&2; exit 2;;
  esac
done

[[ -n "$BEAN_ID" ]] || { echo "Missing --bean-id" >&2; exit 2; }

BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body') || { echo '{"error":"bean not found"}'; exit 1; }

# Check if evaluation log exists
if ! echo "$BODY" | grep -q "## Evaluation Log"; then
  echo '{"error":"no evaluation log found"}'
  exit 1
fi

# Extract BASE_SHA (macOS-compatible: use sed instead of grep -oP)
BASE_SHA=$(echo "$BODY" | sed -n 's/^BASE_SHA: \([^ ]*\)/\1/p' | head -1)
BASE_SHA="${BASE_SHA:-}"

# Extract total_dispatches
TOTAL_DISPATCHES=$(echo "$BODY" | sed -n 's/^total_dispatches: \([0-9][0-9]*\)/\1/p' | tail -1)
TOTAL_DISPATCHES="${TOTAL_DISPATCHES:-0}"

# Count iterations
ITERATION_COUNT=$(echo "$BODY" | grep -c '### Iteration ' || true)

# Extract last guidance (macOS-compatible: use sed instead of grep -oP)
LAST_GUIDANCE=$(echo "$BODY" | sed -n 's/^\*\*Guidance:\*\* "\(.*\)"/\1/p' | tail -1)
LAST_GUIDANCE="${LAST_GUIDANCE:-}"

# Extract last verdict from the last iteration's markers.
#
# UNGRADED is checked before FAIL and outranks it. append-eval-log.sh writes it
# for a dimension it could not compare at all — no threshold, a non-numeric
# score, a mis-shaped scorecard — and that is a stronger statement than a
# failure: it says the entry is not a verdict, so a restart must re-evaluate
# rather than read a position out of it. Without this branch such an entry
# carries no FAIL marker anywhere and falls through to PASS, which is the false
# pass check-thresholds.sh was fixed for, resurfacing in the one record that
# outlives the session. Both are non-PASS, so the resuming action is unchanged.
LAST_VERDICT="UNKNOWN"
if [[ "$ITERATION_COUNT" -gt 0 ]]; then
  # Get the last iteration section (from last "### Iteration" to end)
  LAST_SECTION=$(echo "$BODY" | awk '/### Iteration '"$ITERATION_COUNT"'/{found=1} found{print}')
  if echo "$LAST_SECTION" | grep -q "(UNGRADED,"; then
    LAST_VERDICT="UNGRADED"
  elif echo "$LAST_SECTION" | grep -q "FAIL"; then
    LAST_VERDICT="FAIL"
  else
    LAST_VERDICT="PASS"
  fi
fi

jq -n \
  --arg base_sha "$BASE_SHA" \
  --argjson iteration_count "$ITERATION_COUNT" \
  --argjson total_dispatches "$TOTAL_DISPATCHES" \
  --arg last_verdict "$LAST_VERDICT" \
  --arg last_guidance "$LAST_GUIDANCE" \
  '{base_sha: $base_sha, iteration_count: $iteration_count, total_dispatches: $total_dispatches, last_verdict: $last_verdict, last_guidance: $last_guidance}'
