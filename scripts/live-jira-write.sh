#!/usr/bin/env bash
set -euo pipefail

fail() { echo "live-jira-write: FAIL: $*" >&2; exit 2; }
note() { echo "live-jira-write: $*"; }

needs() {
  local name="$1" why="$2"
  [ -n "${!name:-}" ] || fail "this lane needs $name. $why It fails rather than skips, because a silently-skipped lane cannot be told from a passing one."
}

needs JIRA_USER_EMAIL "It is the account the writes are made as."
needs JIRA_API_TOKEN "It is the credential, and it is read from the environment and never written to a file this lane generates."
needs JIRA_SITE "It is the site origin, as in JIRA_SITE=https://snplow.atlassian.net."
needs JIRA_WRITE_PROJECT "It is a DISPOSABLE project key this lane may create an issue in and then delete. This lane writes to a real site and must never be pointed at a project whose contents matter."

command -v curl >/dev/null 2>&1 || fail "curl must be on PATH"
command -v jq >/dev/null 2>&1 || fail "jq must be on PATH"

case "$JIRA_SITE" in
  https://*) ;;
  *) fail "JIRA_SITE must be an https origin and this is not one: $JIRA_SITE" ;;
esac

case "$JIRA_WRITE_PROJECT" in
  *[!A-Za-z0-9_]*) fail "JIRA_WRITE_PROJECT must be a bare project key and this is not one: $JIRA_WRITE_PROJECT" ;;
esac

if [ -n "${JIRA_ISSUE:-}" ] && [ "${JIRA_ISSUE%%-*}" = "$JIRA_WRITE_PROJECT" ]; then
  fail "JIRA_WRITE_PROJECT is $JIRA_WRITE_PROJECT, the project JIRA_ISSUE ($JIRA_ISSUE) is read from. A disposable project is not the project a read lane observes."
fi

TMP=$(mktemp -d "${TMPDIR:-/tmp}/fiddle-live-jira-write-XXXXXX")
LEFT_BEHIND=""
report_litter() {
  if [ -n "$LEFT_BEHIND" ]; then
    echo "live-jira-write: LEFT BEHIND in $JIRA_WRITE_PROJECT and not deleted:$LEFT_BEHIND" >&2
    echo "live-jira-write: delete them by hand, or this lane's next run will read them as an ambiguous marker" >&2
  fi
  rm -rf "$TMP"
}
trap report_litter EXIT

printf '%s\n' "$JIRA_API_TOKEN" > "$TMP/needle"
chmod 600 "$TMP/needle"

printf 'a decoy line carrying %s in the clear\n' "$JIRA_API_TOKEN" > "$TMP/planted"
chmod 600 "$TMP/planted"
grep -F -q -f "$TMP/needle" "$TMP/planted" \
  || fail "the credential search did not find a planted credential, so its finding nothing elsewhere would prove nothing"
note "the credential search finds a planted credential, so every absence below is a measurement"

mkdir -p "$TMP/stub-state/work" "$TMP/stub-state/changes"
cat > "$TMP/fiddle.toml" <<TOML
[project]
name = "live-jira-write"

[stub]
root = "$TMP/stub-state"

[report]
dir = "$TMP/reports"

[jira]
site = "$JIRA_SITE"
project = "$JIRA_WRITE_PROJECT"
user = { env = "JIRA_USER_EMAIL" }
token = { env = "JIRA_API_TOKEN" }
timeout = "60s"
TOML

if grep -F -q -f "$TMP/needle" "$TMP/fiddle.toml"; then
  fail "the token reached the generated document; the document names variables and carries no value"
fi
note "the generated configuration names JIRA_API_TOKEN and carries no value of it"

SEARCH_ROUTE="/rest/api/3/search/jql"
CREATE_ROUTE="/rest/api/3/issue"
MARKER="fx-live-$(date -u +%Y%m%d%H%M%S)-$$"
JQL="project = $JIRA_WRITE_PROJECT AND labels = $MARKER"
note "this run's marker is $MARKER, and it selects nothing that existed before this run"

creates=0
searches=0
CODE=""
CREATED_KEY=""
CREATED_AT=0

sent() {
  local into="$1" method="$2" route="$3" body="${4:-}"
  local code
  if [ -n "$body" ]; then
    code=$(curl -sS -o "$TMP/$into" -w '%{http_code}' -X "$method" \
      -u "$JIRA_USER_EMAIL:$JIRA_API_TOKEN" \
      -H "Accept: application/json" -H "Content-Type: application/json" \
      --data-binary "$body" \
      "$JIRA_SITE$route") || fail "the site would not answer $method $route"
  else
    code=$(curl -sS -o "$TMP/$into" -w '%{http_code}' -X "$method" \
      -u "$JIRA_USER_EMAIL:$JIRA_API_TOKEN" \
      -H "Accept: application/json" \
      "$JIRA_SITE$route") || fail "the site would not answer $method $route"
  fi
  if grep -F -q -f "$TMP/needle" "$TMP/$into"; then
    fail "the site echoed the credential in its answer to $method $route; not printing it"
  fi
  CODE="$code"
}

searched() {
  local into="$1" token="${2:-}"
  local code
  if [ -n "$token" ]; then
    code=$(curl -sS -G -o "$TMP/$into" -w '%{http_code}' \
      -u "$JIRA_USER_EMAIL:$JIRA_API_TOKEN" \
      -H "Accept: application/json" \
      --data-urlencode "jql=$JQL" --data-urlencode "fields=key" \
      --data-urlencode "nextPageToken=$token" \
      "$JIRA_SITE$SEARCH_ROUTE") || fail "the site would not answer a search"
  else
    code=$(curl -sS -G -o "$TMP/$into" -w '%{http_code}' \
      -u "$JIRA_USER_EMAIL:$JIRA_API_TOKEN" \
      -H "Accept: application/json" \
      --data-urlencode "jql=$JQL" --data-urlencode "fields=key" \
      "$JIRA_SITE$SEARCH_ROUTE") || fail "the site would not answer a search"
  fi
  if grep -F -q -f "$TMP/needle" "$TMP/$into"; then
    fail "the site echoed the credential in a search answer; not printing it"
  fi
  searches=$((searches + 1))
  CODE="$code"
}

every_match() {
  local token="" page=0
  : > "$TMP/matched"
  while [ "$page" -lt 1000 ]; do
    searched "page" "$token"
    [ "$CODE" = 200 ] || fail "a search for the marker answered HTTP $CODE: $(cat "$TMP/page")"
    jq -r '.issues[].key' "$TMP/page" >> "$TMP/matched"
    token=$(jq -r '.nextPageToken // empty' "$TMP/page")
    [ -n "$token" ] || { sort -u "$TMP/matched" -o "$TMP/matched"; return 0; }
    page=$((page + 1))
  done
  fail "the search offered a further page after 1000 of them, and a count taken from part of a result is a floor and never a total"
}

matched_count() { wc -l < "$TMP/matched" | tr -d ' '; }

file_once() {
  local phase="$1"
  CREATED_KEY=""
  every_match
  local held
  held=$(matched_count)
  case "$held" in
    0)
      local body
      body=$(jq -nc \
        --arg project "$JIRA_WRITE_PROJECT" \
        --arg marker "$MARKER" \
        --arg summary "fiddle live write lane $MARKER" \
        '{fields: {project: {key: $project}, issuetype: {name: "Task"}, summary: $summary, labels: ["fiddle-live-lane", $marker]}}')
      sent created POST "$CREATE_ROUTE" "$body"
      [ "$CODE" = 201 ] || fail "$phase: a create answered HTTP $CODE: $(cat "$TMP/created")"
      CREATED_KEY=$(jq -r '.key' "$TMP/created")
      CREATED_AT=$(date +%s)
      creates=$((creates + 1))
      LEFT_BEHIND="$LEFT_BEHIND $CREATED_KEY"
      note "$phase: the marker matched nothing, so this run created $CREATED_KEY"
      ;;
    1)
      note "$phase: the marker already matches $(cat "$TMP/matched"), so nothing was created"
      ;;
    *)
      fail "$phase: $held issues carry the marker $MARKER, and this write acts on one or none: $(tr '\n' ' ' < "$TMP/matched")"
      ;;
  esac
}

echo "=== the first run, which finds nothing and files ==="
file_once "run one"
[ -n "$CREATED_KEY" ] || fail "the first run created nothing, so the marker was not unique to this run"

echo "=== the second run, sent immediately, which is the interruption case ==="
file_once "run two"
note "MEASURED runs that created an issue: $creates of 2"

echo "=== how long the index took to admit the new issue ==="
LAG=unmeasured
for _ in $(seq 1 120); do
  every_match
  if [ "$(matched_count)" -ge 1 ]; then
    LAG=$(( $(date +%s) - CREATED_AT ))
    break
  fi
  sleep 1
done
if [ "$LAG" = unmeasured ]; then
  note "MEASURED the indexing lag: the marker was still unsearchable 120 seconds after the create was accepted, so the exactly-once window is longer than any wait this lane holds"
else
  note "MEASURED the indexing lag: the marker became searchable $LAG seconds after the create was accepted. The exactly-once claim is bounded by exactly this duration: a re-run inside it files a duplicate."
fi

echo "=== what the site holds now ==="
every_match
HELD=$(matched_count)
note "MEASURED issues carrying $MARKER after two runs: $HELD"
note "MEASURED searches sent: $searches; creates accepted: $creates"

echo "=== removing what this lane wrote ==="
printf '%s\n' $LEFT_BEHIND > "$TMP/to-delete"
cat "$TMP/matched" >> "$TMP/to-delete"
sort -u "$TMP/to-delete" -o "$TMP/to-delete"
REMAINING=""
DELETED=0
ASKED=0
while read -r key; do
  [ -n "$key" ] || continue
  ASKED=$((ASKED + 1))
  sent deleted DELETE "/rest/api/3/issue/$key"
  case "$CODE" in
    204) DELETED=$((DELETED + 1)); note "deleted $key" ;;
    *) note "the site would not delete $key: HTTP $CODE"; REMAINING="$REMAINING $key" ;;
  esac
done < "$TMP/to-delete"
note "deleted $DELETED of $ASKED issues this lane knows it wrote or matched"
LEFT_BEHIND="$REMAINING"

echo
note "NOT MEASURED: whether a page boundary shifts under a walk. Forcing it needs an issue indexed between two pages of one walk, which one process cannot arrange."
note "NOT MEASURED: concurrent duplicate invocations. The design scopes exactly-once to interruptions only, and one process cannot race itself."
note "NOT DRIVEN THROUGH FIDDLE: ticket_proposals in cve/verdict.rs reaches no run path (fiddle-zlc4), so no fiddle binary can file a verdict yet. This lane sends the same search-then-create requests FileVerdict sends and measures the site, not the build."

if [ "$creates" -ne 1 ] || [ "$HELD" -ne 1 ]; then
  fail "two runs left $HELD issues carrying the marker after $creates creates, and exactly-once means exactly one"
fi
note "PASS: two runs of the search-then-create protocol left exactly one issue, and the credential reached none of $((searches + creates + 1)) answers this lane read"
