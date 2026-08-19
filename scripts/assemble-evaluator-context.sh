#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: assemble-evaluator-context.sh --domain <name> [--config <path>] [--root <path>]

Emit the static evaluator context for a domain on stdout.

Options:
  --domain   Domain name (required), e.g. general, infrastructure, frontend
  --config   Path to orchestrate.json (default: <root>/orchestrate.json)
  --root     Project root (default: the parent directory of this script)
  --help,-h  Show this help

Exit codes:
  0  Context assembled on stdout
  2  Invalid input
USAGE
}

die() { jq -n --arg error "$1" '{"error":$error}' >&2; exit 2; }

DOMAIN="" CONFIG="" ROOT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h) usage; exit 0;;
    --domain) [[ $# -ge 2 ]] || die "missing value for --domain"; DOMAIN="$2"; shift 2;;
    --config) [[ $# -ge 2 ]] || die "missing value for --config"; CONFIG="$2"; shift 2;;
    --root)   [[ $# -ge 2 ]] || die "missing value for --root";   ROOT="$2";   shift 2;;
    *) die "unknown argument: $1";;
  esac
done

command -v jq >/dev/null 2>&1 || { echo '{"error":"jq not found"}' >&2; exit 2; }

[[ -n "$DOMAIN" ]] || die "missing --domain"

if [[ -z "$ROOT" ]]; then
  ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi
[[ -n "$CONFIG" ]] || CONFIG="$ROOT/orchestrate.json"

[[ -f "$CONFIG" ]] || die "config file not found: $CONFIG"
jq empty "$CONFIG" 2>/dev/null || die "invalid JSON in config file: $CONFIG"

TEMPLATE=$(jq -r --arg d "$DOMAIN" \
  '.evaluators.domains[$d].template // "evaluator-general"' "$CONFIG")

PROTOCOL_FILE="$ROOT/skills/evaluate/SKILL.md"
TEMPLATE_FILE="$ROOT/skills/evaluate/$TEMPLATE.md"
[[ -f "$PROTOCOL_FILE" ]] || die "evaluation protocol not found: $PROTOCOL_FILE"
[[ -f "$TEMPLATE_FILE" ]] || die "domain template not found: $TEMPLATE_FILE"

live_content() {
  local file="$1"
  [[ -f "$file" ]] || return 0
  sed '/^## Retired/,$d' "$file"
}

resolve_optional() {
  local key="$1" default_rel="$2" configured
  configured=$(jq -r --arg d "$DOMAIN" --arg k "$key" \
    '.evaluators.domains[$d][$k] // empty' "$CONFIG")
  if [[ -n "$configured" ]]; then
    [[ -f "$ROOT/$configured" ]] && printf '%s' "$ROOT/$configured"
    return 0
  fi
  [[ -f "$ROOT/$default_rel" ]] && printf '%s' "$ROOT/$default_rel"
  return 0
}

ANTIPATTERNS_FILE=$(resolve_optional antipatterns "docs/antipatterns-$DOMAIN.md")
CALIBRATION_FILE=$(resolve_optional calibration "docs/evaluator-calibration-$DOMAIN.md")

ANTIPATTERNS_CONTENT=""
[[ -n "$ANTIPATTERNS_FILE" ]] && ANTIPATTERNS_CONTENT=$(live_content "$ANTIPATTERNS_FILE")

while IFS= read -r line || [[ -n "$line" ]]; do
  if [[ "$line" == '{ANTIPATTERNS}' ]]; then
    [[ -n "$ANTIPATTERNS_CONTENT" ]] && printf '%s\n' "$ANTIPATTERNS_CONTENT"
  else
    printf '%s\n' "$line"
  fi
done < "$PROTOCOL_FILE"

printf '\n'
cat "$TEMPLATE_FILE"

if [[ -n "$CALIBRATION_FILE" ]]; then
  CALIBRATION_CONTENT=$(live_content "$CALIBRATION_FILE")
  if [[ -n "$CALIBRATION_CONTENT" ]]; then
    printf '\n'
    printf '%s\n' "$CALIBRATION_CONTENT"
  fi
fi
