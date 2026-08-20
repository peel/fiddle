#!/usr/bin/env bash
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

if ! echo "$BODY" | grep -q "## Evaluation Log"; then
  echo '{"error":"no evaluation log found"}'
  exit 1
fi

BASE_SHA=$(echo "$BODY" | sed -n 's/^BASE_SHA: \([^ ]*\)/\1/p' | head -1)
BASE_SHA="${BASE_SHA:-}"

TOTAL_DISPATCHES=$(echo "$BODY" | sed -n 's/^total_dispatches: \([0-9][0-9]*\)/\1/p' | tail -1)
TOTAL_DISPATCHES="${TOTAL_DISPATCHES:-0}"

ITERATION_COUNT=$(echo "$BODY" | grep -c '### Iteration ' || true)

LAST_GUIDANCE=$(echo "$BODY" | sed -n 's/^\*\*Guidance:\*\* "\(.*\)"/\1/p' | tail -1)
LAST_GUIDANCE="${LAST_GUIDANCE:-}"

ITERATION_ROWS=$(echo "$BODY" | awk '
  /^### Iteration /{ if (n != "") print n "\t" d "\t" t "\t" c; n=$3; d=""; t=""; c=""; next }
  /^dispatches: /{ d=$2; next }
  /^tree: /{ t=$2; next }
  /^convergence: /{ c=$2; next }
  END{ if (n != "") print n "\t" d "\t" t "\t" c }
')

ITERATIONS=$(printf '%s' "$ITERATION_ROWS" | jq -R -s -c '
  split("\n") | map(select(length > 0) | split("\t")) |
  map({
    iteration: (.[0] | tonumber? // .[0]),
    dispatches: (.[1] | tonumber? // null),
    tree: (.[2] // ""),
    convergence: (.[3] // "")
  })
')

REEVALUATIONS=$(printf '%s' "$ITERATIONS" | jq '
  . as $i |
  [range(1; ($i | length)) | . as $n |
   select($i[$n].tree != "" and $i[$n].tree == $i[$n - 1].tree)] | length
')

LAST_VERDICT="UNKNOWN"
if [[ "$ITERATION_COUNT" -gt 0 ]]; then
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
  --argjson iterations "$ITERATIONS" \
  --argjson unchanged_tree_reevaluations "$REEVALUATIONS" \
  '{base_sha: $base_sha, iteration_count: $iteration_count, total_dispatches: $total_dispatches, last_verdict: $last_verdict, last_guidance: $last_guidance, iterations: $iterations, unchanged_tree_reevaluations: $unchanged_tree_reevaluations}'
