#!/usr/bin/env bash
set -euo pipefail

PROFILE=""
DOMAIN=""
DIMENSIONS=""
DIMENSIONS_GIVEN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile) PROFILE="${2:-}"; shift 2 ;;
    --domain) DOMAIN="${2:-}"; shift 2 ;;
    --dimensions) DIMENSIONS="${2:-}"; DIMENSIONS_GIVEN=true; shift 2 ;;
    *) echo "build-scorecard-schema: unknown flag '$1'" >&2; exit 2 ;;
  esac
done

case "$PROFILE" in
  evaluator)
    [[ -n "$DOMAIN" ]] || {
      echo "build-scorecard-schema: --domain is required for profile 'evaluator'" >&2; exit 2; }
    [[ "$DIMENSIONS_GIVEN" == true ]] || {
      echo "build-scorecard-schema: --dimensions is required for profile 'evaluator'; pass an empty value for an evidence-only card" >&2; exit 2; }
    ;;
  holistic)
    [[ -z "$DOMAIN" ]] || {
      echo "build-scorecard-schema: --domain is not accepted for profile 'holistic'; the domain is always 'holistic'" >&2; exit 2; }
    [[ "$DIMENSIONS_GIVEN" == false ]] || {
      echo "build-scorecard-schema: --dimensions is not accepted for profile 'holistic'; the five holistic dimensions are fixed" >&2; exit 2; }
    DOMAIN="holistic"
    DIMENSIONS="integration,coherence,holistic_spec_fidelity,polish,runtime_health"
    ;;
  "")
    echo "build-scorecard-schema: --profile is required (evaluator or holistic)" >&2; exit 2 ;;
  *)
    echo "build-scorecard-schema: unknown profile '$PROFILE'" >&2; exit 2 ;;
esac

if [[ "$DIMENSIONS" == *" "* ]]; then
  echo "build-scorecard-schema: --dimensions is a comma-separated list and must not contain spaces: '$DIMENSIONS'" >&2
  exit 2
fi

jq -n --arg domain "$DOMAIN" --arg dims "$DIMENSIONS" --arg profile "$PROFILE" '
def closed(props):
  { type: "object", additionalProperties: false, required: (props | keys), properties: props };

def scored_dimension:
  closed({
    score:     { type: "integer", minimum: 1, maximum: 10 },
    threshold: { type: "integer", minimum: 1, maximum: 10 },
    evidence:  { type: "string", minLength: 1 }
  });

($dims | split(",") | map(select(length > 0))) as $names
| ($names | map({ key: ., value: scored_dimension }) | from_entries) as $dimension_props
| closed($dimension_props) as $dimensions
| {
    provider:   { type: "string", minLength: 1 },
    task_id:    { type: "string" },
    iteration:  { type: "integer" },
    timestamp:  { type: "string" },
    domains:    closed({ ($domain): closed({ dimensions: $dimensions }) }),
    criteria: {
      type: "array",
      items: closed({
        id:       { type: "string" },
        pass:     { type: "boolean" },
        evidence: { type: "string", minLength: 1 }
      })
    },
    antipatterns_detected: {
      type: "array",
      items: {
        anyOf: [
          { type: "string" },
          closed({
            id:       { type: "string" },
            severity: { type: "string" },
            evidence: { type: "string" }
          })
        ]
      }
    },
    spec_defect: {
      anyOf: [
        { type: "null" },
        closed({ detected: { type: "boolean" }, reason: { type: "string", minLength: 1 } })
      ]
    },
    guidance:       { type: "string" },
    dispatch_count: { type: "integer" }
  }
| if ($names | length) == 0
  then . + { mode: { type: "string", enum: ["evidence-only"] } }
  else .
  end
| if $profile == "holistic"
  then . + {
    spec_coverage_matrix: {
      type: "array",
      items: closed({
        requirement: { type: "string" },
        coverage:    { type: "string", enum: ["Full", "Weak", "Missing"] },
        evidence:    { type: "string" }
      })
    },
    remediation_beans: {
      type: "array",
      items: closed({
        requirement: { type: "string" },
        title:       { type: "string" },
        description: { type: "string" },
        source:      { type: "string" }
      })
    }
  }
  else .
  end
| closed(.)
'
