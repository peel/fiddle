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

DUPES=$(ls "$DECISIONS" | grep -E '^[0-9]{3}-' | cut -c1-3 | sort | uniq -d)
if [ -n "$DUPES" ]; then
  printf '{"error":"two records share a number","numbers":"%s"}\n' "$(echo $DUPES)" >&2
  for n in $DUPES; do ls "$DECISIONS/$n"-* >&2; done
  exit 2
fi

LEAVES=$(mktemp "${TMPDIR:-/tmp}/adr-cites-XXXXXX") || exit 2
GONE=$(mktemp "${TMPDIR:-/tmp}/adr-gone-XXXXXX") || exit 2
NAMES=$(mktemp "${TMPDIR:-/tmp}/adr-body-XXXXXX") || exit 2
trap 'rm -f "$LEAVES" "$GONE" "$NAMES"' EXIT INT TERM

names_a_path() {
  case "$1" in
    */*.rs|*/*.sh|*/*.toml|*/*.md) return 0 ;;
    *) return 1 ;;
  esac
}

find_file() {
  find "$ROOT" \
    -name .git -prune -o \
    -name target -prune -o \
    -name .beans -prune -o \
    -name node_modules -prune -o \
    -type f -path "*/$1" -print 2>/dev/null | head -1
}

resolves_in_tree() {
  grep -rqF \
    --exclude-dir=.git \
    --exclude-dir=target \
    --exclude-dir=.beans \
    --exclude-dir=node_modules \
    --exclude-dir=decisions \
    -- "$1" "$ROOT"
}

split_entries() {
  printf '%s' "${1#*:}" | tr ',' '\n' \
    | sed -e 's/^[[:blank:]]*//' -e 's/[[:blank:]]*$//' \
    | grep -v '^$'
}

prose_of() {
  grep -v '^Cites:' "$1" | grep -v '^Retired:'
}

body_names_of() {
  prose_of "$1" \
    | grep -ohE '`[a-z][a-z0-9_]*`' \
    | tr -d '`' \
    | awk -F_ 'NF>=4' \
    | sort -u
}

adrs=0
entries=0
bodies=0
gones=0
violations=0

for adr in "$DECISIONS"/[0-9][0-9][0-9]-*.md; do
  [ -f "$adr" ] || continue
  adrs=$((adrs + 1))
  base=$(basename "$adr")
  number=$((10#${base:0:3}))
  line=$(grep -m1 '^Cites:' "$adr")
  retired=$(grep -m1 '^Retired:' "$adr")

  : > "$GONE"
  if [ -n "$retired" ]; then
    split_entries "$retired" > "$GONE"
    while IFS= read -r entry; do
      gones=$((gones + 1))
      if resolves_in_tree "$entry"; then
        printf '%s: Retired: %s still resolves in the repository\n' "$base" "$entry"
        violations=$((violations + 1))
      fi
      if ! prose_of "$adr" | grep -qF -- "\`$entry\`"; then
        printf '%s: Retired: %s is named nowhere in the body\n' "$base" "$entry"
        violations=$((violations + 1))
      fi
    done < "$GONE"
  fi

  body_names_of "$adr" > "$NAMES"
  while IFS= read -r name; do
    grep -qxF -- "$name" "$GONE" && continue
    bodies=$((bodies + 1))
    if ! resolves_in_tree "$name"; then
      printf '%s: the body cites %s, which resolves to nothing in the repository\n' "$base" "$name"
      violations=$((violations + 1))
    fi
  done < "$NAMES"

  if [ -z "$line" ]; then
    if [ "$number" -ge "$FLOOR" ]; then
      printf '%s: no Cites: line, and %03d is at or above the retrofit floor of %03d\n' "$base" "$number" "$FLOOR"
      violations=$((violations + 1))
    fi
    continue
  fi

  split_entries "$line" > "$LEAVES"

  while IFS= read -r entry; do
    [ "$entry" = "none" ] && continue
    entries=$((entries + 1))
    if names_a_path "$entry"; then
      if [ -z "$(find_file "$entry")" ]; then
        printf '%s: Cites: %s names no file in the repository\n' "$base" "$entry"
        violations=$((violations + 1))
      fi
      continue
    fi
    leaf="${entry##*::}"
    if ! resolves_in_tree "$leaf"; then
      printf '%s: Cites: %s resolves to nothing in the repository\n' "$base" "$leaf"
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

printf '%d ADRs, %d cited symbols, %d body names, %d retired names, %d unresolved\n' \
  "$adrs" "$entries" "$bodies" "$gones" "$violations"
[ "$violations" -eq 0 ]
