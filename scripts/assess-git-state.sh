#!/usr/bin/env bash
set -euo pipefail

BASE_SHA=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-sha)
      [[ $# -ge 2 ]] || { echo '{"error":"--base-sha requires a value"}' >&2; exit 2; }
      BASE_SHA="$2"; shift 2;;
    *) echo "Unknown arg: $1" >&2; exit 2;;
  esac
done

[[ -n "$BASE_SHA" ]] || { echo '{"error":"missing --base-sha"}'; exit 2; }

if git ls-files --unmerged | grep -q .; then
  CONFLICT_FILES=$(git ls-files --unmerged | awk '{print $4}' | sort -u | jq -R -s 'split("\n") | map(select(. != ""))')
  jq -n --argjson files "$CONFLICT_FILES" '{"state":"CORRUPTED","reason":"merge conflict","files":$files}'
  exit 2
fi

DIRTY_FILES=$(git status --porcelain 2>/dev/null | cut -c4-)
if [[ -n "$DIRTY_FILES" ]]; then
  FILES_JSON=$(echo "$DIRTY_FILES" | jq -R -s 'split("\n") | map(select(. != ""))')
  jq -n --argjson files "$FILES_JSON" '{"state":"DIRTY","uncommitted_files":$files}'
  exit 1
fi

if ! git cat-file -e "$BASE_SHA" 2>/dev/null; then
  jq -n --arg sha "$BASE_SHA" '{"state":"CORRUPTED","reason":"base SHA not found","base_sha":$sha}'
  exit 2
fi

HEAD_SHA=$(git rev-parse HEAD)
COMMITS_AHEAD=$(git rev-list "$BASE_SHA".."$HEAD_SHA" --count 2>/dev/null || echo "0")

jq -n --arg head "$HEAD_SHA" --argjson ahead "$COMMITS_AHEAD" \
  '{"state":"CLEAN","head_sha":$head,"commits_ahead":$ahead}'
exit 0
