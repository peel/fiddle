#!/usr/bin/env bash
set -euo pipefail

EPIC_FILE=""
CHILDREN_FILE=""
PREDECESSOR_FILE=""
DELIVERY_COMPLETE=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --epic) EPIC_FILE="${2:-}"; shift 2 ;;
    --children) CHILDREN_FILE="${2:-}"; shift 2 ;;
    --predecessor) PREDECESSOR_FILE="${2:-}"; shift 2 ;;
    --delivery-complete) DELIVERY_COMPLETE=true; shift ;;
    --help|-h)
      echo "Usage: resolve-orchestrate-phase.sh --epic <json-file> --children <json-file> [--predecessor <json-file>] [--delivery-complete]"
      exit 0
      ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ -f "$EPIC_FILE" ]] || { echo "epic JSON file is required" >&2; exit 2; }
[[ -f "$CHILDREN_FILE" ]] || { echo "children JSON file is required" >&2; exit 2; }
jq -e 'type == "object" and (.id | type == "string") and (.type | type == "string")' "$EPIC_FILE" >/dev/null
jq -e 'type == "array"' "$CHILDREN_FILE" >/dev/null

EPIC_ID=$(jq -r '.id' "$EPIC_FILE")
EPIC_TYPE=$(jq -r '.type' "$EPIC_FILE")
PLANNING_COUNT=$(jq '[.[] | select((.tags // []) | index("planning"))] | length' "$CHILDREN_FILE")
SEED_ID=$(jq -r '[.[] | select((.tags // []) | index("planning"))][0].id // empty' "$CHILDREN_FILE")
SEED_STATUS=$(jq -r '[.[] | select((.tags // []) | index("planning"))][0].status // empty' "$CHILDREN_FILE")
IMPLEMENTATION_COUNT=$(jq '[.[] | select(((.tags // []) | index("planning")) | not)] | length' "$CHILDREN_FILE")
PREDECESSOR_COUNT=$(jq '(.blocked_by // []) | length' "$EPIC_FILE")
PREDECESSOR_ID=$(jq -r '(.blocked_by // [])[0] // empty' "$EPIC_FILE")

emit() {
  local state="$1" reason="$2"
  jq -n \
    --arg state "$state" \
    --arg reason "$reason" \
    --arg epic_id "$EPIC_ID" \
    --arg seed_id "$SEED_ID" \
    --arg predecessor_id "$PREDECESSOR_ID" \
    '{state:$state, reason:$reason, epic_id:$epic_id}
     + (if $seed_id == "" then {} else {seed_id:$seed_id} end)
     + (if $predecessor_id == "" then {} else {predecessor_id:$predecessor_id} end)'
  exit 0
}

if [[ "$EPIC_TYPE" == "milestone" ]]; then
  emit INVALID "select an explicit child epic; milestone invocation never chooses one implicitly"
fi
[[ "$EPIC_TYPE" == "epic" ]] || emit INVALID "selected bean must be an epic"

if [[ "$PLANNING_COUNT" -eq 0 ]]; then
  if [[ "$IMPLEMENTATION_COUNT" -eq 0 ]]; then
    emit DEFINE "legacy epic has no child beans"
  fi
  if jq -e 'any(.[]; .status == "todo" or .status == "in-progress")' "$CHILDREN_FILE" >/dev/null; then
    emit DEVELOP "legacy epic has active implementation work"
  fi
  if jq -e 'all(.[]; .status == "completed" or ((.tags // []) | index("needs-attention")))' "$CHILDREN_FILE" >/dev/null; then
    if [[ "$DELIVERY_COMPLETE" == true ]]; then emit DONE "legacy epic delivery is complete"; fi
    emit DELIVER "legacy epic implementation work is terminal"
  fi
  emit INVALID "legacy epic child statuses are contradictory"
fi

[[ "$PLANNING_COUNT" -eq 1 ]] || emit INVALID "epic must contain exactly one planning seed"
[[ "$PREDECESSOR_COUNT" -le 1 ]] || emit INVALID "seed-aware epic must have at most one predecessor"

if jq -e --arg epic_id "$EPIC_ID" 'any(.[]; has("parent") and .parent != $epic_id)' "$CHILDREN_FILE" >/dev/null; then
  emit INVALID "child bean has a conflicting parent"
fi

# Generation identity binds a materialized bean to the seed that planned it, so a
# bean claiming an identity it does not have is a planning defect worth blocking on.
# Remediation beans are exempt for the same reason the planning seed is: holistic
# review creates them mid-epic (develop-holistic 2d) from a scorecard, not from the
# plan, so they have no plan position to carry. Requiring one would leave every epic
# that remediates permanently INVALID, and satisfying it would mean back-dating
# `generated-by` onto beans the seed never generated — recording a false provenance
# to pass a provenance check. An untagged bean that is neither remains invalid.
if [[ "$IMPLEMENTATION_COUNT" -gt 0 ]]; then
  if ! jq -e --arg seed_id "$SEED_ID" '
    [.[] | select((((.tags // []) | index("planning")) or ((.tags // []) | index("remediation"))) | not)]
    | all(.[];
        ([.tags[]? | select(startswith("generated-by:"))] | length) == 1
        and ([.tags[]? | select(startswith("plan-task:"))] | length) == 1
        and (([.tags[]? | select(startswith("generated-by:"))][0]) == ("generated-by:" + $seed_id)))
  ' "$CHILDREN_FILE" >/dev/null; then
    emit INVALID "generated implementation bean has an invalid generation identity"
  fi
  if jq -e '
    [.[]
      | select((((.tags // []) | index("planning")) or ((.tags // []) | index("remediation"))) | not)
      | (([.tags[] | select(startswith("generated-by:"))][0]) + "|" + ([.tags[] | select(startswith("plan-task:"))][0]))]
    | group_by(.)
    | any(.[]; length > 1)
  ' "$CHILDREN_FILE" >/dev/null; then
    emit INVALID "duplicate generation identity"
  fi
fi

if [[ "$PREDECESSOR_COUNT" -eq 1 ]]; then
  [[ -n "$PREDECESSOR_FILE" && -f "$PREDECESSOR_FILE" ]] || emit NEEDS_CONTEXT "predecessor bean is unavailable"
  OBSERVED_PREDECESSOR_ID=$(jq -r '.id // empty' "$PREDECESSOR_FILE")
  [[ "$OBSERVED_PREDECESSOR_ID" == "$PREDECESSOR_ID" ]] || emit NEEDS_CONTEXT "predecessor bean does not match blocked_by"
  PREDECESSOR_STATUS=$(jq -r '.status // empty' "$PREDECESSOR_FILE")
  [[ "$PREDECESSOR_STATUS" == "completed" ]] || emit NEEDS_CONTEXT "predecessor epic is not completed"
  if ! jq -e '(.body // "") | contains("<!-- milestone-handoff:start -->") and contains("<!-- milestone-handoff:end -->")' "$PREDECESSOR_FILE" >/dev/null; then
    emit NEEDS_CONTEXT "predecessor milestone handoff is unavailable"
  fi
fi

if [[ "$SEED_STATUS" == "todo" || "$SEED_STATUS" == "in-progress" ]]; then
  [[ "$IMPLEMENTATION_COUNT" -eq 0 ]] || emit INVALID "implementation beans exist before seed completion"
  emit SEED "planning seed requires execution or resumption"
fi

[[ "$SEED_STATUS" == "completed" ]] || emit NEEDS_CONTEXT "planning seed is not executable or completed"
[[ "$IMPLEMENTATION_COUNT" -gt 0 ]] || emit INVALID "completed seed has no generated implementation beans"

if jq -e '[.[] | select(((.tags // []) | index("planning")) | not)] | any(.[]; .status == "todo" or .status == "in-progress")' "$CHILDREN_FILE" >/dev/null; then
  emit DEVELOP "generated implementation work remains"
fi

if jq -e '[.[] | select(((.tags // []) | index("planning")) | not)] | all(.[]; .status == "completed" or ((.tags // []) | index("needs-attention")))' "$CHILDREN_FILE" >/dev/null; then
  if [[ "$DELIVERY_COMPLETE" == true ]]; then emit DONE "milestone delivery is complete"; fi
  emit DELIVER "generated implementation work is terminal"
fi

emit INVALID "generated implementation child statuses are contradictory"
