#!/usr/bin/env bash
set -euo pipefail

SCORECARD=""
CRITERIA=""
TREE_SHA=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --scorecard) SCORECARD="$2"; shift 2;;
    --criteria) CRITERIA="$2"; shift 2;;
    --tree-sha) TREE_SHA="$2"; shift 2;;
    *) echo "Unknown arg: $1" >&2; exit 2;;
  esac
done

[[ -f "$SCORECARD" ]] || { echo '{"error":"scorecard file not found"}'; exit 2; }
[[ -f "$CRITERIA" ]] || { echo '{"error":"criteria file not found"}'; exit 2; }

jq empty "$SCORECARD" 2>/dev/null || { echo '{"error":"scorecard is not valid JSON"}'; exit 2; }
jq empty "$CRITERIA" 2>/dev/null || { echo '{"error":"criteria is not valid JSON"}'; exit 2; }

UNGRADEABLE=$(jq -n --slurpfile card "$SCORECARD" --slurpfile crit "$CRITERIA" '
  def misspelling($accepted; $obj):
    [$accepted[] | select($obj[.] != null)] |
    if length > 0 then
      " (found `\(join("`, `"))`, which the scorecard envelope does not accept)"
    else "" end;

  def numeric($field; $aliases; $where; $obj):
    ($obj[$field]) as $value |
    if $value == null then
      "\($where): missing `\($field)`" + misspelling($aliases; $obj)
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
               numeric("score"; ["rating"]; $where; .value),
               numeric("threshold"; ["min", "target", "minimum"]; $where; .value)
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
           (if $entry.id == null then
              "\($where): missing `id`" + misspelling(["criterion", "name", "criterion_id"]; $entry)
            elif ($entry.id | type) != "string" then
              "\($where): `id` must be a string, got \($entry.id | type)"
            else empty end),
           (if $entry.pass == null then
              "\($where): missing `pass`" + misspelling(["met", "passed", "result", "status"]; $entry)
            elif ($entry.pass | type) != "boolean" then
              "\($where): `pass` must be a boolean, got \($entry.pass | type)"
            else empty end)
         end)
     end)
  ]
')

SCHEMA_DOC="skills/develop/scorecard-envelope.md"

if [[ "$(echo "$UNGRADEABLE" | jq 'length')" -gt 0 ]]; then
  {
    echo "scorecard cannot be graded: it does not match the scorecard envelope."
    echo "wanted: .domains.<domain>.dimensions.<name> = {score: number, threshold: number}"
    echo "        .criteria (a bare array) = [{id: string, pass: boolean}]"
    echo "schema: $SCHEMA_DOC — the accepted field names are exactly these, and a"
    echo "        card spelling them otherwise is refused rather than translated."
  } >&2
  echo "$UNGRADEABLE" | jq -r '.[]' >&2
  jq -n --argjson problems "$UNGRADEABLE" --arg schema "$SCHEMA_DOC" \
    '{error: "scorecard cannot be graded", schema: $schema, problems: $problems}'
  exit 2
fi

FAILING_DIMS=$(jq -c '
  [.domains | to_entries[] | .key as $domain |
   .value.dimensions | to_entries[] |
   select(.value.score < .value.threshold) |
   {domain: $domain, dimension: .key, score: .value.score, threshold: .value.threshold}]
' "$SCORECARD")

FAILING_CRITERIA=$(jq -c '[.[] | select(.pass == false) | .id]' "$CRITERIA")

FAIL_DIM_COUNT=$(echo "$FAILING_DIMS" | jq 'length')
FAIL_CRIT_COUNT=$(echo "$FAILING_CRITERIA" | jq 'length')

PASSING_DIMS=$(jq -c '
  [.domains | to_entries[] | .key as $domain |
   .value.dimensions | to_entries[] |
   select(.value.score >= .value.threshold) |
   {domain: $domain, dimension: .key, score: .value.score, threshold: .value.threshold}]
' "$SCORECARD")

FINDINGS=$(jq -c '
  [(.antipatterns_detected // [])[] |
   if type == "string" then {id: ., severity: "unspecified"}
   else {id: (.id // .antipattern // .antipattern_id // "unknown"),
         severity: (.severity // "unspecified")}
   end]
' "$SCORECARD")

DIMENSIONS_MAP=$(jq -c '
  [.domains | to_entries[] | .key as $domain |
   .value.dimensions | to_entries[] |
   {("\($domain).\(.key)"): .value.score}] | add // {}
' "$SCORECARD")

if [[ "$FAIL_DIM_COUNT" -eq 0 && "$FAIL_CRIT_COUNT" -eq 0 ]]; then
  jq -n --argjson passing "$PASSING_DIMS" \
        --argjson dimensions "$DIMENSIONS_MAP" \
        --argjson findings "$FINDINGS" \
        --arg tree_sha "$TREE_SHA" '{
    verdict: "PASS",
    tree_sha: $tree_sha,
    failing_dimensions: [],
    failing_criteria: [],
    passing_dimensions: $passing,
    dimensions: $dimensions,
    findings: $findings
  } | if .tree_sha == "" then del(.tree_sha) else . end'
  exit 0
else
  jq -n --argjson failing_dims "$FAILING_DIMS" \
        --argjson failing_crit "$FAILING_CRITERIA" \
        --argjson passing "$PASSING_DIMS" \
        --argjson dimensions "$DIMENSIONS_MAP" \
        --argjson findings "$FINDINGS" \
        --arg tree_sha "$TREE_SHA" '{
    verdict: "FAIL",
    tree_sha: $tree_sha,
    failing_dimensions: $failing_dims,
    failing_criteria: $failing_crit,
    passing_dimensions: $passing,
    dimensions: $dimensions,
    findings: $findings
  } | if .tree_sha == "" then del(.tree_sha) else . end'
  exit 1
fi
