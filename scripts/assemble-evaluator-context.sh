#!/usr/bin/env bash
# assemble-evaluator-context.sh — Emit the static evaluator context for one domain
# in the order skills/develop-loop/context-loading-order.md mandates: the
# evaluation protocol with its {ANTIPATTERNS} placeholder filled (positions 1 and
# 8), the domain template (position 2), then the project calibration anchors
# (position 3). Calibration and antipattern content is truncated at any
# "## Retired" heading, so retired entries stay out of evaluator context.
#
# Positions 4 through 7 (runtime evidence, runtime and stack agents, task
# criteria, prior scorecards) depend on run state rather than config and remain
# the caller's to append after this output.
#
# Exit 0 = assembled on stdout, 2 = invalid input (JSON error on stderr).
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

# Position 2: the domain's template, falling back to the general one so an
# unconfigured domain still gets scored against something.
TEMPLATE=$(jq -r --arg d "$DOMAIN" \
  '.evaluators.domains[$d].template // "evaluator-general"' "$CONFIG")

PROTOCOL_FILE="$ROOT/skills/evaluate/SKILL.md"
TEMPLATE_FILE="$ROOT/skills/evaluate/$TEMPLATE.md"
[[ -f "$PROTOCOL_FILE" ]] || die "evaluation protocol not found: $PROTOCOL_FILE"
[[ -f "$TEMPLATE_FILE" ]] || die "domain template not found: $TEMPLATE_FILE"

# Everything above a "## Retired" heading. Retired anchors and antipatterns are
# kept for audit by deliver 5g and must not reach an evaluator.
live_content() {
  local file="$1"
  [[ -f "$file" ]] || return 0
  sed '/^## Retired/,$d' "$file"
}

resolve_optional() {
  # Echo an absolute path for a configured key, or the default path when the key
  # is absent. Echoes nothing when neither resolves to an existing file.
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

# Position 1 plus 8: the protocol, with the placeholder line replaced by the live
# antipattern content (or dropped when there is none, so no evaluator is asked to
# check against a literal placeholder).
while IFS= read -r line || [[ -n "$line" ]]; do
  if [[ "$line" == '{ANTIPATTERNS}' ]]; then
    [[ -n "$ANTIPATTERNS_CONTENT" ]] && printf '%s\n' "$ANTIPATTERNS_CONTENT"
  else
    printf '%s\n' "$line"
  fi
done < "$PROTOCOL_FILE"

# Position 2.
printf '\n'
cat "$TEMPLATE_FILE"

# Position 3: the anchors have to be in context before the evaluator forms its own
# scale, which is why they follow the template rather than trailing the whole pack.
if [[ -n "$CALIBRATION_FILE" ]]; then
  CALIBRATION_CONTENT=$(live_content "$CALIBRATION_FILE")
  if [[ -n "$CALIBRATION_CONTENT" ]]; then
    printf '\n'
    printf '%s\n' "$CALIBRATION_CONTENT"
  fi
fi
