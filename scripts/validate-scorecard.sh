#!/usr/bin/env bash
# validate-scorecard.sh — Gate an evaluator scorecard before develop-loop merges it.
# The expected criteria ids arrive via --criteria-ids (a comma list the orchestrator
# extracts from the bean's eval block, mirroring resolve-domains.sh --domains); this
# script parses NO YAML.
#
# A valid scorecard: is valid JSON; has a non-empty `provider` string; every
# `domains.<domain>.dimensions` is an object (an explicitly empty `{}` is valid
# evidence-only) and every scored dimension carries a non-empty justification in
# `evidence` or in `comment` (the field name the provider-context schema shows
# external evaluators, so both are accepted); its
# `criteria[]` ids exactly match the --criteria-ids set (no extras, none missing)
# and each criterion carries non-empty `evidence`; and any `spec_defect` with
# `detected == true` carries a non-empty `reason`.
#
# Exit codes:
#   0  Scorecard is valid
#   2  Scorecard is invalid or input is invalid; a JSON array of error strings
#      is printed to stderr
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: validate-scorecard.sh --scorecard <file> --criteria-ids <comma-list>

Validate an evaluator scorecard against the shared schema and the task's criteria.

Options:
  --scorecard     Path to the scorecard JSON file
  --criteria-ids  Comma-separated criterion ids the scorecard must cover exactly
  --help,-h       Show this help message

Exit codes:
  0  Scorecard is valid
  2  Scorecard is invalid or input invalid (JSON error array on stderr)
USAGE
}

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

  ($card[0]) as $c |
  ([$ids | split(",")[] | gsub("^\\s+|\\s+$"; "") | select(length > 0)] | unique) as $expected |

  [
    # provider must be a non-empty string
    (if ($c.provider | nonempty) then empty else "provider must be a non-empty string" end),

    # dimensions per domain: object type, non-empty evidence (or comment) on each scored dimension
    ($c.domains // {} | to_entries[] |
      .key as $domain | (.value.dimensions) as $dims |
      if ($dims | type) != "object" then
        "domain \($domain): dimensions must be an object"
      else
        ($dims | to_entries[] |
          if ((.value.evidence | nonempty) or (.value.comment | nonempty)) then empty
          else "domain \($domain) dimension \(.key): evidence must be non-empty" end)
      end),

    # criteria must be an array
    (if ($c.criteria | type) != "array" then "criteria must be an array" else empty end),

    # each criterion carries non-empty evidence
    ((if ($c.criteria | type) == "array" then $c.criteria else [] end)[] |
      if (.evidence | nonempty) then empty
      else "criterion \(.id // "?"): evidence must be non-empty" end),

    # criteria ids must exactly match the expected set
    (((if ($c.criteria | type) == "array" then $c.criteria else [] end) | map(.id) | unique) as $actual |
      (($actual - $expected)[] | "unexpected criterion id: \(.)"),
      (($expected - $actual)[] | "missing criterion id: \(.)")),

    # spec_defect detected must carry a non-empty reason
    (if ($c.spec_defect | type) == "object" and ($c.spec_defect.detected == true) then
       if ($c.spec_defect.reason | nonempty) then empty
       else "spec_defect detected but reason is empty" end
     else empty end)
  ]
')

if [[ "$(echo "$FAILURES" | jq 'length')" -gt 0 ]]; then
  echo "$FAILURES" | jq -c '.' >&2
  exit 2
fi

exit 0
