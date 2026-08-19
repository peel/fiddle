#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: resolve-domains.sh --domains <comma-separated> --config <path>

Resolve each domain name to its full evaluator config from orchestrate.json.

Options:
  --domains  Comma-separated list of domain names (e.g. "frontend,backend")
  --config   Path to orchestrate.json config file
  --help,-h  Show this help message

Exit codes:
  0  All domains resolved (from config or fallback)
  1  Application error
  2  Invalid input (missing args, bad config, etc.)
USAGE
}

DOMAINS=""
CONFIG=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h) usage; exit 0;;
    --domains) DOMAINS="$2"; shift 2;;
    --config) CONFIG="$2"; shift 2;;
    *) echo '{"error":"unknown argument: '"$1"'"}' >&2; exit 2;;
  esac
done

[[ -n "$DOMAINS" ]] || { echo '{"error":"missing --domains"}' >&2; exit 2; }
[[ -n "$CONFIG" ]]  || { echo '{"error":"missing --config"}' >&2; exit 2; }
[[ -f "$CONFIG" ]]  || { echo '{"error":"config file not found: '"$CONFIG"'"}' >&2; exit 2; }

command -v jq >/dev/null 2>&1 || { echo '{"error":"jq not found"}' >&2; exit 2; }

if ! jq empty "$CONFIG" 2>/dev/null; then
  echo '{"error":"invalid JSON in config file"}' >&2
  exit 2
fi

DOMAIN_LIST=$(echo "$DOMAINS" | jq -R '[split(",")[] | gsub("^\\s+|\\s+$"; "") | select(length > 0)] | unique')

OUTPUT=$(jq -n \
  --argjson domain_list "$DOMAIN_LIST" \
  --slurpfile config "$CONFIG" '
  ($config[0].evaluators.domains) as $all_domains |
  ($all_domains.general // {}) as $general_defaults |
  [
    $domain_list[] |
    . as $name |
    if $all_domains[$name] then
      # Known domain: copy all fields, add domain and resolved_via
      $all_domains[$name] + {
        "domain": $name,
        "resolved_via": "config"
      }
    else
      # Unknown domain: fallback to general defaults
      $general_defaults + {
        "domain": $name,
        "template": "evaluator-general",
        "resolved_via": "fallback"
      }
    end
  ]
')

echo "$OUTPUT"
exit 0
