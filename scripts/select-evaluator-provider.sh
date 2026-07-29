#!/usr/bin/env bash
# select-evaluator-provider.sh — Pick the evaluator provider for a domain.
# The providers list is an ordered preference; the first available provider
# that differs from the implementer's provider wins. Fallbacks: the
# implementer's provider (fresh context), then claude.
# Exit 0 = selected, 2 = invalid input.
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: select-evaluator-provider.sh --preference <comma-separated> [--implementer <name>]

Pick the evaluator provider for a domain from an ordered preference list.
The first available provider (claude is always available; others need
command -v) that differs from the implementer's provider wins. Falls back
to the implementer's provider in a fresh context, then to claude.

Options:
  --preference   Ordered comma-separated provider list (e.g. "codex,claude")
  --implementer  Provider that implemented the task (default: claude)
  --help,-h      Show this help message

Output: {"provider": "<name>", "reason": "<text>"}

Exit codes:
  0  Provider selected
  2  Invalid input (missing args, unknown argument)
USAGE
}

PREFERENCE=""
IMPLEMENTER="claude"

invalid() {
  jq -n --arg error "$1" '{"error":$error}' >&2
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h) usage; exit 0;;
    --preference) [[ $# -ge 2 ]] || invalid "missing value for --preference"; PREFERENCE="$2"; shift 2;;
    --implementer) [[ $# -ge 2 ]] || invalid "missing value for --implementer"; IMPLEMENTER="$2"; shift 2;;
    *) invalid "unknown argument: $1";;
  esac
done

[[ -n "$PREFERENCE" ]] || invalid "missing --preference"

available() {
  local p="$1"
  [[ "$p" == "claude" ]] && return 0
  command -v "$p" >/dev/null 2>&1
}

emit() {
  jq -n --arg provider "$1" --arg reason "$2" '{"provider":$provider,"reason":$reason}'
}

trim() {
  local s="$1"
  s="${s#"${s%%[![:space:]]*}"}"
  s="${s%"${s##*[![:space:]]}"}"
  printf '%s' "$s"
}

IFS=',' read -ra PROVIDERS <<< "$PREFERENCE"
for p in "${PROVIDERS[@]}"; do
  p="$(trim "$p")"
  [[ -z "$p" ]] && continue
  available "$p" || continue
  if [[ "$p" != "$IMPLEMENTER" ]]; then
    emit "$p" "first available provider differing from implementer"
    exit 0
  fi
done

if available "$IMPLEMENTER"; then
  emit "$IMPLEMENTER" "fallback: implementer provider in a fresh context"
  exit 0
fi
emit "claude" "fallback: no configured provider available"
