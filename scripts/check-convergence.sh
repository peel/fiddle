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
HISTORY_LEN=$(jq 'length' "$HISTORY")

CURRENT_TREE=$(jq -r '.tree_sha // "" | tostring' "$CURRENT")
PREVIOUS_TREE=""
PREVIOUS_VERDICT=""
if [[ "$HISTORY_LEN" -gt 0 ]]; then
  PREVIOUS_TREE=$(jq -r '.[-1].tree_sha // "" | tostring' "$HISTORY")
  PREVIOUS_VERDICT=$(jq -r '.[-1].verdict // ""' "$HISTORY")
fi

if [[ -z "$CURRENT_TREE" || -z "$PREVIOUS_TREE" ]]; then
  TREE_COMPARISON="unknown"
elif [[ "$CURRENT_TREE" == "$PREVIOUS_TREE" ]]; then
  TREE_COMPARISON="unchanged"
else
  TREE_COMPARISON="changed"
fi

contradictions_of_previous_pass() {
  jq -c --slurpfile hist "$HISTORY" '
    ($hist[0] | .[-1]) as $previous |
    {
      criteria: [.failing_criteria[]? | . as $criterion |
        select(([$previous.failing_criteria[]?] | index($criterion)) == null)],
      dimensions: [.failing_dimensions[]? |
        "\(.domain).\(.dimension)" as $key |
        select((([$previous.passing_dimensions[]? | "\(.domain).\(.dimension)"]) | index($key)) != null) |
        $key]
    }
  ' "$CURRENT"
}

findings_absent_from_previous() {
  jq -c --slurpfile hist "$HISTORY" '
    def above_low: ((.severity // "unspecified") | ascii_downcase) != "low";
    [$hist[0] | .[-1].findings[]? | .id] as $already_reported |
    [.findings[]? | select(above_low) | . as $finding |
     select(($already_reported | index($finding.id)) == null)]
  ' "$CURRENT"
}

score_deltas_against_previous() {
  jq -c --slurpfile hist "$HISTORY" '
    ($hist[0] | .[-1].dimensions // {}) as $previous |
    [.dimensions // {} | to_entries[] |
     select($previous[.key] != null and $previous[.key] != .value) |
     {dimension: .key, previous: $previous[.key], current: .value}]
  ' "$CURRENT"
}

emit_contested() {
  local criteria="$1" dimensions="$2" findings="$3"
  jq -n --arg tree_sha "$CURRENT_TREE" \
        --argjson criteria "$criteria" \
        --argjson dimensions "$dimensions" \
        --argjson findings "$findings" \
        '{"status":"CONTESTED","tree_comparison":"unchanged","tree_sha":$tree_sha,
          "contested_criteria":$criteria,"contested_dimensions":$dimensions,
          "new_findings":$findings}'
  exit 2
}

if [[ "$VERDICT" != "PASS" ]]; then
  if [[ "$TREE_COMPARISON" == "unchanged" && "$PREVIOUS_VERDICT" == "PASS" ]]; then
    CONTRADICTIONS=$(contradictions_of_previous_pass)
    CONTRADICTION_COUNT=$(echo "$CONTRADICTIONS" | jq '(.criteria | length) + (.dimensions | length)')
    if [[ "$CONTRADICTION_COUNT" -gt 0 ]]; then
      emit_contested \
        "$(echo "$CONTRADICTIONS" | jq -c '.criteria')" \
        "$(echo "$CONTRADICTIONS" | jq -c '.dimensions')" \
        '[]'
    fi
  fi
  require_next_dispatch_or_exhaust
  ITERATION=$(jq 'length + 1' "$HISTORY")
  jq -n --argjson iteration "$ITERATION" --arg tree_comparison "$TREE_COMPARISON" \
    '{"status":"FAIL","iteration":$iteration,"tree_comparison":$tree_comparison}'
  exit 1
fi

DIM_COUNT=$(jq 'if (.dimensions | type) == "object" then (.dimensions | length) else -1 end' "$CURRENT")
if [[ "$DIM_COUNT" -eq 0 ]]; then
  echo '{"status":"CONVERGED","mode":"evidence-only"}'
  exit 0
fi

if [[ "$HISTORY_LEN" -eq 0 ]]; then
  require_next_dispatch_or_exhaust
  echo '{"status":"PASS_PENDING"}'
  exit 1
fi

if [[ "$PREVIOUS_VERDICT" != "PASS" ]]; then
  require_next_dispatch_or_exhaust
  jq -n --arg tree_comparison "$TREE_COMPARISON" \
    '{"status":"PASS_PENDING","tree_comparison":$tree_comparison}'
  exit 1
fi

if [[ "$TREE_COMPARISON" == "unchanged" ]]; then
  NEW_FINDINGS=$(findings_absent_from_previous)
  if [[ "$(echo "$NEW_FINDINGS" | jq 'length')" -gt 0 ]]; then
    emit_contested '[]' '[]' "$NEW_FINDINGS"
  fi
  jq -n --arg tree_sha "$CURRENT_TREE" \
        --argjson deltas "$(score_deltas_against_previous)" \
        '{"status":"CONVERGED","tree_comparison":"unchanged","tree_sha":$tree_sha,
          "ignored_score_deltas":$deltas}'
  exit 0
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
  jq -n --argjson regressions "$REGRESSIONS" --arg tree_comparison "$TREE_COMPARISON" \
    '{"status":"PASS_REGRESSED","regressions":$regressions,"tree_comparison":$tree_comparison}'
  exit 1
fi

jq -n --arg tree_comparison "$TREE_COMPARISON" \
  '{"status":"CONVERGED","tree_comparison":$tree_comparison}'
exit 0
