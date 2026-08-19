#!/usr/bin/env bash
# check-thresholds.sh — Compare scorecard against threshold config.
# Exit 0 = all pass, 1 = at least one fail, 2 = invalid or ungradeable input.
#
# An input the comparisons below cannot read is refused, never defaulted: exit 2
# with an {"error", "problems"} object on stdout and one line per problem on
# stderr, each naming the missing field and the dimension or criterion id it
# belongs to. Grading a scorecard by a threshold set its author was never given
# would be worse than erroring.
set -euo pipefail

SCORECARD=""
CRITERIA=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --scorecard) SCORECARD="$2"; shift 2;;
    --criteria) CRITERIA="$2"; shift 2;;
    *) echo "Unknown arg: $1" >&2; exit 2;;
  esac
done

[[ -f "$SCORECARD" ]] || { echo '{"error":"scorecard file not found"}'; exit 2; }
[[ -f "$CRITERIA" ]] || { echo '{"error":"criteria file not found"}'; exit 2; }

jq empty "$SCORECARD" 2>/dev/null || { echo '{"error":"scorecard is not valid JSON"}'; exit 2; }
jq empty "$CRITERIA" 2>/dev/null || { echo '{"error":"criteria is not valid JSON"}'; exit 2; }

# Require every field the comparisons below read, before comparing anything.
# jq makes `1 < null` false *and* `1 >= null` true, so a dimension with no
# threshold used to read as passing twice — absent from failing_dimensions and
# listed in passing_dimensions with a null threshold. Type order does the same
# for a stringly-typed score: `"1" < 7` is false and `"1" >= 7` is true. On the
# criteria side `select(.pass == false)` cannot tell an ungraded array from a
# clean one, and matches neither "false" nor null. Each of those reports a pass
# the script never established, so ungradeable input must refuse instead.
UNGRADEABLE=$(jq -n --slurpfile card "$SCORECARD" --slurpfile crit "$CRITERIA" '
  def numeric($field; $where; $value):
    if $value == null then "\($where): missing `\($field)`"
    elif ($value | type) != "number" then
      "\($where): `\($field)` must be a number, got \($value | type)"
    else empty end;

  ($card[0]) as $c |
  ($crit[0]) as $k |
  [
    (if ($c | type) != "object" then
       "scorecard must be a JSON object, got \($c | type)"
     elif $c.domains == null then
       "scorecard: missing `domains`"
     elif ($c.domains | type) != "object" then
       "scorecard: `domains` must be an object, got \($c.domains | type)"
     else
       ($c.domains | to_entries[] | .key as $domain | .value as $body |
         if ($body | type) != "object" then
           "domain \($domain): must be an object, got \($body | type)"
         elif $body.dimensions == null then
           "domain \($domain): missing `dimensions`"
         elif ($body.dimensions | type) != "object" then
           "domain \($domain): `dimensions` must be an object, got \($body.dimensions | type)"
         else
           ($body.dimensions | to_entries[] |
             "domain \($domain) dimension \(.key)" as $where |
             if (.value | type) != "object" then
               "\($where): must be an object, got \(.value | type)"
             else
               numeric("score"; $where; .value.score),
               numeric("threshold"; $where; .value.threshold)
             end)
         end)
     end),

    (if ($k | type) != "array" then
       "criteria must be a JSON array, got \($k | type)"
     else
       ($k | to_entries[] | .key as $index | .value as $entry |
         (if ($entry | type) == "object" and ($entry.id | type) == "string"
          then "criterion \($entry.id)" else "criterion #\($index)" end) as $where |
         if ($entry | type) != "object" then
           "\($where): must be an object, got \($entry | type)"
         else
           (if $entry.id == null then "\($where): missing `id`"
            elif ($entry.id | type) != "string" then
              "\($where): `id` must be a string, got \($entry.id | type)"
            else empty end),
           (if $entry.pass == null then "\($where): missing `pass`"
            elif ($entry.pass | type) != "boolean" then
              "\($where): `pass` must be a boolean, got \($entry.pass | type)"
            else empty end)
         end)
     end)
  ]
')

if [[ "$(echo "$UNGRADEABLE" | jq 'length')" -gt 0 ]]; then
  echo "$UNGRADEABLE" | jq -r '.[]' >&2
  jq -n --argjson problems "$UNGRADEABLE" \
    '{error: "scorecard cannot be graded", problems: $problems}'
  exit 2
fi

# Check dimensions against thresholds
FAILING_DIMS=$(jq -c '
  [.domains | to_entries[] | .key as $domain |
   .value.dimensions | to_entries[] |
   select(.value.score < .value.threshold) |
   {domain: $domain, dimension: .key, score: .value.score, threshold: .value.threshold}]
' "$SCORECARD")

# Check criteria
FAILING_CRITERIA=$(jq -c '[.[] | select(.pass == false) | .id]' "$CRITERIA")

FAIL_DIM_COUNT=$(echo "$FAILING_DIMS" | jq 'length')
FAIL_CRIT_COUNT=$(echo "$FAILING_CRITERIA" | jq 'length')

PASSING_DIMS=$(jq -c '
  [.domains | to_entries[] | .key as $domain |
   .value.dimensions | to_entries[] |
   select(.value.score >= .value.threshold) |
   {domain: $domain, dimension: .key, score: .value.score, threshold: .value.threshold}]
' "$SCORECARD")

# Build flat map of "domain.dimension": score for convergence detection
DIMENSIONS_MAP=$(jq -c '
  [.domains | to_entries[] | .key as $domain |
   .value.dimensions | to_entries[] |
   {("\($domain).\(.key)"): .value.score}] | add // {}
' "$SCORECARD")

if [[ "$FAIL_DIM_COUNT" -eq 0 && "$FAIL_CRIT_COUNT" -eq 0 ]]; then
  jq -n --argjson passing "$PASSING_DIMS" \
        --argjson dimensions "$DIMENSIONS_MAP" '{
    verdict: "PASS",
    failing_dimensions: [],
    failing_criteria: [],
    passing_dimensions: $passing,
    dimensions: $dimensions
  }'
  exit 0
else
  jq -n --argjson failing_dims "$FAILING_DIMS" \
        --argjson failing_crit "$FAILING_CRITERIA" \
        --argjson passing "$PASSING_DIMS" \
        --argjson dimensions "$DIMENSIONS_MAP" '{
    verdict: "FAIL",
    failing_dimensions: $failing_dims,
    failing_criteria: $failing_crit,
    passing_dimensions: $passing,
    dimensions: $dimensions
  }'
  exit 1
fi
