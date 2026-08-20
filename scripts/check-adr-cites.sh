#!/usr/bin/env bash
set -uo pipefail

ROOT="."
FLOOR=21

while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT="${2:-}"; shift 2 || exit 2 ;;
    --floor) FLOOR="${2:-}"; shift 2 || exit 2 ;;
    *) printf '{"error":"unknown argument %s"}\n' "$1" >&2; exit 2 ;;
  esac
done

DECISIONS="$ROOT/docs/technical/decisions"
[ -d "$DECISIONS" ] || { printf '{"error":"no decisions directory at %s"}\n' "$DECISIONS" >&2; exit 2; }
[ -d "$ROOT/crates" ] || { printf '{"error":"no crates directory at %s"}\n' "$ROOT/crates" >&2; exit 2; }

LEAVES=$(mktemp "${TMPDIR:-/tmp}/adr-cites-XXXXXX") || exit 2
trap 'rm -f "$LEAVES"' EXIT INT TERM

adrs=0
entries=0
violations=0

for adr in "$DECISIONS"/[0-9][0-9][0-9]-*.md; do
  [ -f "$adr" ] || continue
  adrs=$((adrs + 1))
  base=$(basename "$adr")
  number=$((10#${base:0:3}))
  line=$(grep -m1 '^Cites:' "$adr")

  if [ -z "$line" ]; then
    if [ "$number" -ge "$FLOOR" ]; then
      printf '%s: no Cites: line, and %03d is at or above the retrofit floor of %03d\n' "$base" "$number" "$FLOOR"
      violations=$((violations + 1))
    fi
    continue
  fi

  printf '%s' "${line#Cites:}" | tr ',' '\n' | tr -d '[:blank:]' | grep -v '^$' > "$LEAVES"

  while read -r entry; do
    [ "$entry" = "none" ] && continue
    leaf="${entry##*::}"
    entries=$((entries + 1))
    if ! grep -rqF -- "$leaf" "$ROOT/crates"; then
      printf '%s: Cites: %s resolves to nothing under crates/\n' "$base" "$leaf"
      violations=$((violations + 1))
    fi
  done < "$LEAVES"
done

if [ "$adrs" -eq 0 ]; then
  printf '{"error":"no ADRs matched under %s, so this check measured nothing"}\n' "$DECISIONS" >&2
  exit 2
fi

if [ "$entries" -eq 0 ]; then
  printf '{"error":"no Cites: entries across %d ADRs, so this check measured nothing"}\n' "$adrs" >&2
  exit 2
fi

printf '%d ADRs, %d cited symbols, %d unresolved\n' "$adrs" "$entries" "$violations"
[ "$violations" -eq 0 ]
