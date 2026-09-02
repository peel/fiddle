#!/usr/bin/env bash
set -uo pipefail

ROOT="."

while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT="${2:-}"; shift 2 || exit 2 ;;
    *) printf '{"error":"unknown argument %s"}\n' "$1" >&2; exit 2 ;;
  esac
done

DOCUMENT="$ROOT/workflows/toil.toml"
SYSTEM="$ROOT/docs/technical/SYSTEM.md"
ADR="$ROOT/docs/technical/decisions/083-toil-is-a-document-and-a-gate.md"
COMMIT_ADR="$ROOT/docs/technical/decisions/082-a-step-earns-the-commit-the-branch-step-publishes.md"
MAIN="$ROOT/crates/fiddle-cli/src/main.rs"

for FILE in "$DOCUMENT" "$SYSTEM" "$ADR" "$COMMIT_ADR" "$MAIN"; do
  [ -f "$FILE" ] || { printf '{"error":"no file at %s, so this check measured nothing"}\n' "$FILE" >&2; exit 2; }
  [ -r "$FILE" ] || { printf '{"error":"%s cannot be read, so this check measured nothing"}\n' "$FILE" >&2; exit 2; }
done

ORDERING='after `selected_workflow` loads the document'
RETIRED=(
  'before the document loads'
  'before this document is loaded'
  'Eligibility is decided before'
)

WORDS=(zero one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen)

word_for() {
  [ "$1" -ge 1 ] 2>/dev/null && [ "$1" -lt "${#WORDS[@]}" ] || return 0
  printf '%s' "${WORDS[$1]}"
}

ROWS=$(grep -c '^| toil gate |' "$SYSTEM")
if [ "$ROWS" -ne 1 ]; then
  printf '{"error":"%s holds %s rows spelt `| toil gate |` and this check reads exactly one, so the claim it pins is not where it was"}\n' "$SYSTEM" "$ROWS" >&2
  exit 2
fi
ROW=$(grep '^| toil gate |' "$SYSTEM")

STEPS=$(grep -c '^\[\[steps\]\]' "$DOCUMENT")
if [ "$STEPS" -eq 0 ]; then
  printf '{"error":"%s names no `[[steps]]`, so the counts below would be read off nothing"}\n' "$DOCUMENT" >&2
  exit 2
fi

EFFECT_LINES=$(awk '
  function flush() { if (kind == "effect") print (name == "" ? "-" : name) }
  /^[[:space:]]*\[\[steps\]\][[:space:]]*$/ { flush(); kind = ""; name = ""; next }
  /^[[:space:]]*kind[[:space:]]*=/ { kind = $0; sub(/^[^"]*"/, "", kind); sub(/".*$/, "", kind); next }
  /^[[:space:]]*name[[:space:]]*=/ { name = $0; sub(/^[^"]*"/, "", name); sub(/".*$/, "", name); next }
  END { flush() }
' "$DOCUMENT")

EFFECTS=$(printf '%s\n' "$EFFECT_LINES" | grep -c '^..*$')
if [ "$EFFECTS" -eq 0 ]; then
  printf '{"error":"%s names %s steps and none of kind `effect`, so the effect order below would be read off nothing"}\n' "$DOCUMENT" "$STEPS" >&2
  exit 2
fi
if printf '%s\n' "$EFFECT_LINES" | grep -qx -- '-'; then
  printf '{"error":"an effect step in %s carries no `name`, so this check cannot say which effects the document names"}\n' "$DOCUMENT" >&2
  exit 2
fi

STEP_WORD=$(word_for "$STEPS")
EFFECT_WORD=$(word_for "$EFFECTS")
if [ -z "$STEP_WORD" ] || [ -z "$EFFECT_WORD" ]; then
  printf '{"error":"%s names %s steps and %s effect steps, and this check carries no number word for one of them, so it would compare nothing"}\n' "$DOCUMENT" "$STEPS" "$EFFECTS" >&2
  exit 2
fi

ORDER_PATTERN=""
while IFS= read -r NAME; do
  [ -n "$NAME" ] || continue
  ESCAPED=$(printf '%s' "$NAME" | sed 's/[].[\*^$()+?{}|\\]/\\&/g')
  if [ -z "$ORDER_PATTERN" ]; then
    ORDER_PATTERN="$ESCAPED"
  else
    ORDER_PATTERN="$ORDER_PATTERN.*$ESCAPED"
  fi
done <<EOF
$EFFECT_LINES
EOF

FAILED=0

for CLAUSE in "${RETIRED[@]}"; do
  if printf '%s' "$ROW" | grep -qF "$CLAUSE"; then
    printf '{"error":"the toil gate row in %s says \\"%s\\", and the binary loads the document first"}\n' "$SYSTEM" "$CLAUSE" >&2
    FAILED=1
  fi
  for FILE in "$ADR" "$DOCUMENT"; do
    if grep -qF "$CLAUSE" "$FILE"; then
      printf '{"error":"%s says \\"%s\\", and the binary loads the document first"}\n' "$FILE" "$CLAUSE" >&2
      FAILED=1
    fi
  done
done

if ! printf '%s' "$ROW" | grep -qF "$ORDERING"; then
  printf '{"error":"the toil gate row in %s does not state the ordering ADR 083 states: %s"}\n' "$SYSTEM" "$ORDERING" >&2
  FAILED=1
fi

for FILE in "$ADR" "$DOCUMENT"; do
  if ! grep -qF "$ORDERING" "$FILE"; then
    printf '{"error":"%s does not state the ordering %s states: %s"}\n' "$FILE" "$SYSTEM" "$ORDERING" >&2
    FAILED=1
  fi
done

for FILE in "$SYSTEM" "$ADR" "$COMMIT_ADR"; do
  if ! grep -qF "names $STEP_WORD steps" "$FILE"; then
    printf '{"error":"%s names %s steps and %s does not say `names %s steps`"}\n' "$DOCUMENT" "$STEPS" "$FILE" "$STEP_WORD" >&2
    FAILED=1
  fi
  if ! grep -qF "$EFFECT_WORD effect steps" "$FILE"; then
    printf '{"error":"%s names %s effect steps and %s does not say `%s effect steps`"}\n' "$DOCUMENT" "$EFFECTS" "$FILE" "$EFFECT_WORD" >&2
    FAILED=1
  fi
done

for FILE in "$SYSTEM" "$ADR" "$COMMIT_ADR"; do
  for WORD in "${WORDS[@]}"; do
    if [ "$WORD" != "$STEP_WORD" ] && grep -qF "$WORD steps" "$FILE"; then
      printf '{"error":"%s says `%s steps` and %s names %s"}\n' "$FILE" "$WORD" "$DOCUMENT" "$STEPS" >&2
      FAILED=1
    fi
    if [ "$WORD" != "$EFFECT_WORD" ] && grep -qF "$WORD effect steps" "$FILE"; then
      printf '{"error":"%s says `%s effect steps` and %s names %s"}\n' "$FILE" "$WORD" "$DOCUMENT" "$EFFECTS" >&2
      FAILED=1
    fi
  done
done

for FILE in "$SYSTEM" "$ADR" "$COMMIT_ADR"; do
  if ! grep -qE "$ORDER_PATTERN" "$FILE"; then
    printf '{"error":"%s names its effect steps in the order %s, and no line of %s names them in that order"}\n' "$DOCUMENT" "$(printf '%s' "$EFFECT_LINES" | tr '\n' ' ')" "$FILE" >&2
    FAILED=1
  fi
done

LOADS=$(grep -n 'selected_workflow(selection' "$MAIN" | head -1 | cut -d: -f1)
GATES=$(grep -n 'let qualification = qualified(' "$MAIN" | head -1 | cut -d: -f1)

if [ -z "$LOADS" ] || [ -z "$GATES" ]; then
  printf '{"error":"%s no longer calls `selected_workflow(selection` and `qualified(` where this check reads them (loads=%s gates=%s), so the source half measured nothing"}\n' "$MAIN" "${LOADS:-none}" "${GATES:-none}" >&2
  exit 2
fi

if [ "$LOADS" -ge "$GATES" ]; then
  printf '{"error":"%s calls selected_workflow at line %s and qualified at line %s, so the document no longer loads first and both records are now wrong"}\n' "$MAIN" "$LOADS" "$GATES" >&2
  FAILED=1
fi

if [ "$FAILED" -ne 0 ]; then
  echo "TOIL GATE ORDER: FAIL"
  exit 1
fi

printf 'TOIL GATE ORDER: ok (selected_workflow at %s, qualified at %s, and the document, its gate row and ADR 083 state that ordering and no retired one; %s names %s steps and %s effect steps, and SYSTEM.md, ADR 082 and ADR 083 each state both counts, name those effect steps in the document order, and state no other count)\n' "$LOADS" "$GATES" "$DOCUMENT" "$STEPS" "$EFFECTS"
exit 0
