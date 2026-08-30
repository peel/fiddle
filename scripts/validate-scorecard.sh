#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: validate-scorecard.sh --scorecard <file> --criteria-ids <comma-list>

Validate an evaluator scorecard against the shared schema and the task's criteria.
This is the pre-flight for check-thresholds.sh: it checks the same fields the grader
needs, so a mis-shaped envelope is caught where it was produced. The accepted field
names are listed in skills/develop/scorecard-envelope.md.

Options:
  --scorecard     Path to the scorecard JSON file
  --criteria-ids  Comma-separated criterion ids the scorecard must cover exactly
  --help,-h       Show this help message

Exit codes:
  0  Scorecard is valid
  2  Scorecard is invalid or input invalid (JSON error array on stderr)
USAGE
}

SCHEMA_DOC="skills/develop/scorecard-envelope.md"
SCORECARD=""
CRITERIA_IDS=""
CRITERIA_IDS_SET=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h) usage; exit 0;;
    --scorecard) SCORECARD="${2:-}"; shift 2;;
    --criteria-ids) CRITERIA_IDS="${2:-}"; CRITERIA_IDS_SET=1; shift 2;;
    *) echo '["unknown argument: '"$1"'"]' >&2; exit 2;;
  esac
done

command -v jq >/dev/null 2>&1 || { echo '["jq not found"]' >&2; exit 2; }

[[ -n "$SCORECARD" ]]        || { echo '["missing --scorecard"]' >&2; exit 2; }
[[ -f "$SCORECARD" ]]        || { echo '["scorecard file not found: '"$SCORECARD"'"]' >&2; exit 2; }
[[ "$CRITERIA_IDS_SET" -eq 1 ]] || { echo '["missing --criteria-ids"]' >&2; exit 2; }

if ! jq empty "$SCORECARD" 2>/dev/null; then
  echo '["scorecard is not valid JSON"]' >&2
  exit 2
fi

FAILURES=$(jq -n \
  --arg ids "$CRITERIA_IDS" \
  --slurpfile card "$SCORECARD" '
  def nonempty: type == "string" and (gsub("^\\s+|\\s+$"; "") | length > 0);

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
  ([$ids | split(",")[] | gsub("^\\s+|\\s+$"; "") | select(length > 0)] | unique) as $expected |

  [
    (if ($c.provider | nonempty) then empty else "provider must be a non-empty string" end),

    (if ($c.mode != null and $c.mode != "evidence-only") then
       "mode accepts only \"evidence-only\", got \($c.mode | tojson)"
     else empty end),

    (if (($c.domains == null) or (($c.domains | type) == "object")) then empty
     else "domains must be an object keyed by domain name, got \($c.domains | type)" end),

    (($c.domains | if type == "object" then . else {} end) | to_entries[] |
      .key as $domain |
      if (.value | type) != "object" then
        "domain \($domain): must be an object, got \(.value | type)"
      else
        (.value.dimensions) as $dims |
        if ($dims | type) != "object" then
          "domain \($domain): dimensions must be an object"
        else
          ($dims | to_entries[] |
            "domain \($domain) dimension \(.key)" as $where |
            if (.value | type) != "object" then
              "\($where): must be an object, got \(.value | type)"
            else
              (if ((.value.evidence | nonempty) or (.value.comment | nonempty)) then empty
               else "\($where): evidence must be non-empty" end),
              numeric("score"; ["rating"]; $where; .value),
              numeric("threshold"; ["min", "target", "minimum"]; $where; .value)
            end)
        end
      end),

    (($c.domains | if type == "object" then . else {} end) as $domains |
     ([$domains[] | if type == "object" then (.dimensions | if type == "object" then length else 0 end) else 0 end] | add // 0) as $scored |
     if $scored == 0 and $c.mode != "evidence-only" then
       "scorecard scored no dimensions and does not declare `mode`: \"evidence-only\"",
       ($domains | to_entries[] |
        select((.value | type) == "object" and ((.value.dimensions | type) == "object") and ((.value.dimensions | length) == 0)) |
        "domain \(.key): dimensions is empty, which only a declared evidence-only scorecard may be")
     else empty end),

    (if ($c.criteria | type) != "array" then "criteria must be an array" else empty end),

    ((if ($c.criteria | type) == "array" then $c.criteria else [] end) |
      [.[] | select(type == "object") | .id | select(type == "string")] |
      group_by(.) | map(select(length > 1)) | .[] |
      "duplicate criterion id: \(.[0]) appears \(length) times, so one verdict would overwrite another"),

    ((if ($c.criteria | type) == "array" then $c.criteria else [] end) | to_entries[] |
      .key as $index | .value as $entry |
      (if ($entry | type) == "object" and ($entry.id | type) == "string"
       then "criterion \($entry.id)" else "criterion #\($index)" end) as $where |
      if ($entry | type) != "object" then
        "\($where): must be an object, got \($entry | type)"
      else
        (if ($entry.evidence | nonempty) then empty
         else "\($where): evidence must be non-empty" end),
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
      end),

    (((if ($c.criteria | type) == "array" then $c.criteria else [] end) | map(.id) | unique) as $actual |
      (($actual - $expected)[] | "unexpected criterion id: \(.)"),
      (($expected - $actual)[] | "missing criterion id: \(.)")),

    (($c.spec_defect) as $defect |
     if $defect == null then empty
     elif ($defect | type) != "object" then
       "spec_defect must be null or an object stating `detected`, got \($defect | type)"
     elif ($defect.detected | type) != "boolean" then
       "spec_defect: `detected` must be a boolean, got \($defect.detected | type)"
     elif $defect.detected == true and (($defect.reason | nonempty) | not) then
       "spec_defect detected but reason is empty"
     else empty end)
  ]
')

if [[ "$(echo "$FAILURES" | jq 'length')" -gt 0 ]]; then
  echo "$FAILURES" | jq -c --arg schema "$SCHEMA_DOC" \
    '. + ["accepted field names: \($schema)"]' >&2
  exit 2
fi

exit 0
