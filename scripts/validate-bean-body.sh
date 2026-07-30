#!/usr/bin/env bash
# validate-bean-body.sh — Gate a task bean body for structural completeness.
# A complete body has: a fenced ```eval block containing both `domains:` and
# `criteria:`; a files section (## Files heading or a Files: line) with at least
# one `- Create:`/`- Modify:`/`- Test:`/`- Delete:` line; and at least one
# `- [ ]` checklist step. Container feature beans (--container) are exempt.
#
# Exit codes:
#   0  Body is complete (or --container was passed)
#   2  Body is incomplete or input is invalid; a JSON array of gap descriptions
#      is printed to stderr
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: validate-bean-body.sh --body <file> [--container]

Validate that a task bean body is structurally complete.

Options:
  --body       Path to the bean body file to validate
  --container  Mark as a pure container feature bean (exempt; exits 0)
  --help,-h    Show this help message

Exit codes:
  0  Body is complete (or --container was passed)
  2  Body is incomplete or input invalid (JSON error array on stderr)
USAGE
}

BODY=""
CONTAINER=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h) usage; exit 0;;
    --body) BODY="${2:-}"; shift 2;;
    --container) CONTAINER=1; shift;;
    *) echo '["unknown argument: '"$1"'"]' >&2; exit 2;;
  esac
done

# Container feature beans are exempt regardless of body content.
[[ "$CONTAINER" -eq 1 ]] && exit 0

command -v jq >/dev/null 2>&1 || { echo '["jq not found"]' >&2; exit 2; }

[[ -n "$BODY" ]]  || { echo '["missing --body"]' >&2; exit 2; }
[[ -f "$BODY" ]]  || { echo '["body file not found: '"$BODY"'"]' >&2; exit 2; }

FAILURES=()

# Extract the fenced eval block (between ```eval and the next ```), if any.
EVAL_BLOCK=$(awk '
  /^```eval[[:space:]]*$/ { inblock=1; next }
  inblock && /^```[[:space:]]*$/ { inblock=0; next }
  inblock { print }
' "$BODY")

if [[ -z "$EVAL_BLOCK" ]]; then
  FAILURES+=("missing fenced eval block containing domains: and criteria:")
else
  if ! grep -q 'domains:' <<<"$EVAL_BLOCK"; then
    FAILURES+=("eval block is missing domains:")
  fi
  if ! grep -q 'criteria:' <<<"$EVAL_BLOCK"; then
    FAILURES+=("eval block is missing criteria:")
  fi
fi

# Files section: a `## Files` heading or a `Files:` line, plus at least one
# Create/Modify/Test/Delete list item.
if grep -qE '^##[[:space:]]+Files[[:space:]]*$' "$BODY" || grep -qE '^Files:' "$BODY"; then
  if ! grep -qE '^[[:space:]]*-[[:space:]]+(Create|Modify|Test|Delete):' "$BODY"; then
    FAILURES+=("files section has no - Create:/- Modify:/- Test:/- Delete: line")
  fi
else
  FAILURES+=("missing files section (## Files heading or Files: line)")
fi

# At least one unchecked checklist step.
if ! grep -qE '^[[:space:]]*-[[:space:]]+\[ \]' "$BODY"; then
  FAILURES+=("no - [ ] checklist steps")
fi

if [[ ${#FAILURES[@]} -gt 0 ]]; then
  printf '%s\n' "${FAILURES[@]}" | jq -Rn '[inputs]' >&2
  exit 2
fi

exit 0
