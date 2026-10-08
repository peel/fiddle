#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TEMPLATE="${1:-$ROOT/docs/technical/host-workflow-fiddle-ai.yml}"
MAIN="${2:-$ROOT/crates/fiddle-cli/src/main.rs}"

[ -f "$TEMPLATE" ] || { echo "test-host-workflow-fiddle-ai: no template at $TEMPLATE, so nothing was checked" >&2; exit 2; }
[ -f "$MAIN" ] || { echo "test-host-workflow-fiddle-ai: no $MAIN, so fiddle's own handle cannot be read" >&2; exit 2; }

FAILED=0
CHECKED=0

fail() { echo "test-host-workflow-fiddle-ai: FAIL: $*" >&2; FAILED=$((FAILED + 1)); }

holds() {
  CHECKED=$((CHECKED + 1))
  local what="$1" pattern="$2" text="$3"
  printf '%s\n' "$text" | grep -qE -- "$pattern" || fail "$what (no line matches /$pattern/)"
}

counts() {
  CHECKED=$((CHECKED + 1))
  local what="$1" want="$2" pattern="$3" text="$4" have
  have=$(printf '%s\n' "$text" | grep -oE -- "$pattern" | wc -l | tr -d ' ')
  [ "$have" = "$want" ] || fail "$what (found $have matches of /$pattern/, want $want)"
}

absent() {
  CHECKED=$((CHECKED + 1))
  local what="$1" pattern="$2" text="$3" found
  found=$(printf '%s\n' "$text" | grep -nE -- "$pattern" | head -1)
  [ -z "$found" ] || fail "$what (found: $found)"
}

block() {
  awk -v start="$1" -v stop="$2" '
    $0 ~ start { inside = 1; next }
    inside && $0 ~ stop { exit }
    inside { print }
  ' "$TEMPLATE"
}

BODY=$(grep -vE '^[[:space:]]*#' "$TEMPLATE")
HEADER=$(awk '/^[^#]/ { exit } { print }' "$TEMPLATE")
ON=$(block '^on:' '^[^[:space:]#]')
GUARD=$(block '^    if: >-' '^    runs-on:')
PERMISSIONS=$(block '^permissions:' '^[^[:space:]#]')
JOBS=$(block '^jobs:' '^[^[:space:]#]')
CONCURRENCY=$(block '^    concurrency:$' '^    [^[:space:]#]')
HANDLE=$(sed -nE 's/^const MENTION: &str = "([^"]+)";$/\1/p' "$MAIN")
ASSOCIATIONS='fromJSON\('"'"'\["OWNER", "MEMBER", "COLLABORATOR"\]'"'"'\)'

[ -n "$ON" ] || { echo "test-host-workflow-fiddle-ai: the template has no on: block, so no trigger was checked" >&2; exit 2; }
[ -n "$JOBS" ] || { echo "test-host-workflow-fiddle-ai: the template has no jobs: block, so no job was checked" >&2; exit 2; }
[ -n "$GUARD" ] || { echo "test-host-workflow-fiddle-ai: the template has no job if: block, so no guard was checked" >&2; exit 2; }
[ -n "$HANDLE" ] || { echo "test-host-workflow-fiddle-ai: $MAIN names no MENTION, so fiddle's own rule cannot be compared" >&2; exit 2; }

echo "it runs on the three events and on nothing else"
holds "a created issue comment" '^  issue_comment:$' "$ON"
holds "a submitted review" '^  pull_request_review:$' "$ON"
holds "a created review comment" '^  pull_request_review_comment:$' "$ON"
counts "the on: block names three events" 3 '^  [a-z_]+:' "$ON"
counts "two events act on creation" 2 'types: \[created\]$' "$ON"
counts "one acts on submission" 1 'types: \[submitted\]$' "$ON"

echo "one job runs, and it is the guarded one"
counts "the workflow holds exactly one job" 1 '^  [A-Za-z0-9_-]+:' "$JOBS"
holds "and that job is fiddle" '^  fiddle:$' "$JOBS"
counts "exactly one job-level if" 1 '^    if:' "$JOBS"
holds "fiddle's first key is the guard" '^  fiddle:$' "$(printf '%s\n' "$JOBS" | grep -B1 -E '^    if: >-$' | head -1)"

echo "its guard requires the mention, an entitled association and a sender that is no bot"
holds "a bot sender is excluded" "^      github\.event\.sender\.type != 'Bot'$" "$GUARD"
holds "and that exclusion is joined to the rest by and" '^      && \($' "$GUARD"
holds "an issue comment counts only on a pull request" '^          && github\.event\.issue\.pull_request$' "$GUARD"
counts "every event's text must mention @$HANDLE" 3 "contains\(github\.event\.(comment|review)\.body, '@$HANDLE'\)" "$GUARD"
counts "every event's author must be OWNER, MEMBER or COLLABORATOR" 3 "contains\($ASSOCIATIONS, github\.event\.(comment|review)\.author_association\)" "$GUARD"
counts "the three events are the only alternatives" 2 '\|\|' "$GUARD"
absent "no other association is entitled" 'CONTRIBUTOR|NONE|FIRST_TIME|MANNEQUIN' "$GUARD"
absent "no clause is always true" "(\btrue\b|always\(\)|!= ''\))" "$GUARD"

echo "only a run that passed the guard queues, one per pull request, and none is cancelled"
absent "no workflow-level concurrency, where an event the guard skips would replace a queued mention" '^concurrency:' "$BODY"
holds "the job holds the concurrency" '^    concurrency:$' "$BODY"
holds "one group per pull request number" '^      group: fiddle-ai-\$\{\{ github\.event\.issue\.number \|\| github\.event\.pull_request\.number \}\}$' "$CONCURRENCY"
holds "a queued run does not cancel a running one" '^      cancel-in-progress: false$' "$CONCURRENCY"
absent "nothing cancels in progress" 'cancel-in-progress: true' "$BODY"

echo "its own token reads, and the App's token does the work"
holds "the workflow token reads contents" '^  contents: read$' "$PERMISSIONS"
absent "the workflow token writes nothing" 'write' "$PERMISSIONS"
holds "the App token is minted" '^        uses: actions/create-github-app-token@v[0-9]+$' "$BODY"
holds "from the App id secret" '^          app-id: \$\{\{ secrets\.[A-Z_]+ \}\}$' "$BODY"
holds "and the private key secret" '^          private-key: \$\{\{ secrets\.[A-Z_]+ \}\}$' "$BODY"
holds "fiddle runs with the minted token" '^          FIDDLE_GITHUB_TOKEN: \$\{\{ steps\.app\.outputs\.token \}\}$' "$BODY"

echo "it checks out the default branch without persisting a credential"
counts "every checkout persists no credential" "$(printf '%s\n' "$BODY" | grep -cE 'uses: actions/checkout@')" '^          persist-credentials: false$' "$BODY"
holds "there is a checkout" 'uses: actions/checkout@v[0-9]+$' "$BODY"
counts "every checkout names the default branch, so no event checks out the pull request's merge ref" "$(printf '%s\n' "$BODY" | grep -cE 'uses: actions/checkout@')" '^          ref: \$\{\{ github\.event\.repository\.default_branch \}\}$' "$BODY"
counts "and no step names another ref" 1 '^[[:space:]]+ref:' "$BODY"

echo "it runs a pr: reference, so fiddle applies its own rule again"
holds "the run names the pull request" '"\$fiddle" run "pr:\$\{\{ github\.repository \}\}#\$\{PULL_REQUEST\}"' "$BODY"
holds "whose number comes from the event" '^          PULL_REQUEST: \$\{\{ github\.event\.issue\.number \|\| github\.event\.pull_request\.number \}\}$' "$BODY"
absent "no --capability widens what the run may do" '--capability' "$BODY"
counts "the guard's handle is the one fiddle reads" 3 "'@$HANDLE'" "$GUARD"
absent "a comment's text never reaches a shell" '\$\{\{ github\.event\.(comment|review|issue|pull_request)\.(body|title)' "$(printf '%s\n' "$BODY" | grep -vE "contains\(github\.event\.(comment|review)\.body")"

echo "its header names what the App and the deployment need"
for permission in 'contents: write' 'pull-requests: write' 'issues: write' 'checks: read' 'actions: read'; do
  holds "the App needs $permission" "^#   $permission " "$HEADER"
done
for event in issue_comment pull_request_review pull_request_review_comment; do
  holds "the App subscribes to $event" "^#   $event$" "$HEADER"
done
holds "the account fiddle writes as" '^#   account = "fiddle-ai\[bot\]"$' "$HEADER"
holds "the author's name" '^#   name = "fiddle-ai\[bot\]"$' "$HEADER"
holds "the author's noreply address" '^#   email = "<app-user-id>\+fiddle-ai\[bot\]@users\.noreply\.github\.com"$' "$HEADER"

if [ "$FAILED" -ne 0 ]; then
  echo "test-host-workflow-fiddle-ai: FAIL ($FAILED of $CHECKED checks)" >&2
  exit 1
fi
echo "test-host-workflow-fiddle-ai: PASS ($CHECKED checks)"
