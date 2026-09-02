#!/usr/bin/env bash
set -uo pipefail

ROOT="."

while [ $# -gt 0 ]; do
  case "$1" in
    --root) ROOT="${2:-}"; shift 2 || exit 2 ;;
    *) printf '{"error":"unknown argument %s"}\n' "$1" >&2; exit 2 ;;
  esac
done

SYSTEM="$ROOT/docs/technical/SYSTEM.md"
ADR="$ROOT/docs/technical/decisions/083-toil-is-a-document-and-a-gate.md"
MAIN="$ROOT/crates/fiddle-cli/src/main.rs"

for FILE in "$SYSTEM" "$ADR" "$MAIN"; do
  [ -f "$FILE" ] || { printf '{"error":"no file at %s, so this check measured nothing"}\n' "$FILE" >&2; exit 2; }
done

ORDERING='after `selected_workflow` loads the document'
DENIED='before the document loads'

ROW=$(grep -F '| toil gate |' "$SYSTEM")
[ -n "$ROW" ] || { printf '{"error":"%s has no `| toil gate |` row, so the claim this check pins is not where it was"}\n' "$SYSTEM" >&2; exit 2; }

FAILED=0

if printf '%s' "$ROW" | grep -qF "$DENIED"; then
  printf '{"error":"the toil gate row in %s says \\"%s\\", and the binary loads the document first"}\n' "$SYSTEM" "$DENIED" >&2
  FAILED=1
fi

if ! printf '%s' "$ROW" | grep -qF "$ORDERING"; then
  printf '{"error":"the toil gate row in %s does not state the ordering ADR 083 states: %s"}\n' "$SYSTEM" "$ORDERING" >&2
  FAILED=1
fi

if ! grep -qF "$ORDERING" "$ADR"; then
  printf '{"error":"%s does not state the ordering %s states: %s"}\n' "$ADR" "$SYSTEM" "$ORDERING" >&2
  FAILED=1
fi

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

printf 'TOIL GATE ORDER: ok (selected_workflow at %s, qualified at %s, both records state the same ordering)\n' "$LOADS" "$GATES"
exit 0
