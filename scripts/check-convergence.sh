#!/usr/bin/env bash
set -euo pipefail

CURRENT="" HISTORY="" MAX_DISPATCHES=60 CURRENT_DISPATCHES=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --current) CURRENT="$2"; shift 2;;
    --history) HISTORY="$2"; shift 2;;
    --max-dispatches) MAX_DISPATCHES="$2"; shift 2;;
    --current-dispatches) CURRENT_DISPATCHES="$2"; shift 2;;
    *) echo "Unknown arg: $1" >&2; exit 2;;
  esac
done

[[ -f "$CURRENT" ]] || { echo '{"error":"current file not found"}'; exit 2; }
[[ -f "$HISTORY" ]] || { echo '{"error":"history file not found"}'; exit 2; }

emit_budget_exhausted() {
  jq -n --argjson dispatches "$CURRENT_DISPATCHES" --argjson budget "$MAX_DISPATCHES" \
    '{"status":"DISPATCHES_EXCEEDED","dispatches":$dispatches,"budget":$budget}'
  exit 2
}

require_next_dispatch_or_exhaust() {
  if [[ "$CURRENT_DISPATCHES" -ge "$MAX_DISPATCHES" ]]; then
    emit_budget_exhausted
  fi
}

if [[ "$CURRENT_DISPATCHES" -gt "$MAX_DISPATCHES" ]]; then
  emit_budget_exhausted
fi

VERDICT=$(jq -r '.verdict' "$CURRENT")

if [[ "$VERDICT" != "PASS" ]]; then
  require_next_dispatch_or_exhaust
  ITERATION=$(jq 'length + 1' "$HISTORY")
  jq -n --argjson iteration "$ITERATION" '{"status":"FAIL","iteration":$iteration}'
  exit 1
fi

DIM_COUNT=$(jq 'if (.dimensions | type) == "object" then (.dimensions | length) else -1 end' "$CURRENT")
if [[ "$DIM_COUNT" -eq 0 ]]; then
  echo '{"status":"CONVERGED","mode":"evidence-only"}'
  exit 0
fi

HISTORY_LEN=$(jq 'length' "$HISTORY")
if [[ "$HISTORY_LEN" -eq 0 ]]; then
  require_next_dispatch_or_exhaust
  echo '{"status":"PASS_PENDING"}'
  exit 1
fi

LAST_VERDICT=$(jq -r '.[-1].verdict' "$HISTORY")
if [[ "$LAST_VERDICT" != "PASS" ]]; then
  require_next_dispatch_or_exhaust
  echo '{"status":"PASS_PENDING"}'
  exit 1
fi

REGRESSIONS=$(jq -c --slurpfile hist "$HISTORY" '
  .dimensions as $current |
  ($hist[0] | .[-1].dimensions) as $previous |
  [($current | to_entries[]) |
   . as $entry |
   ($previous[$entry.key] // 0) as $prev_score |
   select($entry.value < $prev_score) |
   $entry.key]
' "$CURRENT")

REG_COUNT=$(echo "$REGRESSIONS" | jq 'length')
if [[ "$REG_COUNT" -gt 0 ]]; then
  require_next_dispatch_or_exhaust
  jq -n --argjson regressions "$REGRESSIONS" \
    '{"status":"PASS_REGRESSED","regressions":$regressions}'
  exit 1
fi

echo '{"status":"CONVERGED"}'
exit 0
