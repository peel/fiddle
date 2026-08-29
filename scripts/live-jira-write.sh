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
needs JIRA_WRITE_PROJECT "It is the project key this lane files a test ticket in and then CLOSES. It never deletes: deletion is refused by policy in ISP, and a cleanup that depends on a permission the operator does not have leaves residue on every run."
needs JIRA_LEDGER_ISSUE "It is an existing issue in JIRA_WRITE_PROJECT that holds the claim ledger. This lane reads and writes properties on it and never closes it. Two rules bound it and only the first is enforced in code: it must name JIRA_WRITE_PROJECT, which this lane checks, and it must outlive every run, which no lane can check by reading one issue. A ticket an earlier run filed and closed satisfies the second, because a closed issue still answers a property read and the close list below refuses to name the ledger. What is forbidden is a ticket this run will file, because closing that ticket would take the ledger with it. The property probe below is the enforced half: a ledger that cannot hold a claim is refused before anything is written."

JIRA_ISSUE_TYPE="${JIRA_ISSUE_TYPE:-Task}"
JIRA_CLOSING_TRANSITION="${JIRA_CLOSING_TRANSITION-}"
[ -n "$JIRA_CLOSING_TRANSITION" ] || JIRA_CLOSING_TRANSITION="Won't Do"
JIRA_CLOSING_FALLBACK="${JIRA_CLOSING_FALLBACK:-Done}"

command -v curl >/dev/null 2>&1 || fail "curl must be on PATH"
command -v jq >/dev/null 2>&1 || fail "jq must be on PATH"

case "$JIRA_SITE" in
  https://*) ;;
  *) fail "JIRA_SITE must be an https origin and this is not one: $JIRA_SITE" ;;
esac

case "$JIRA_WRITE_PROJECT" in
  *[!A-Za-z0-9_]*) fail "JIRA_WRITE_PROJECT must be a bare project key and this is not one: $JIRA_WRITE_PROJECT" ;;
esac

if [ "${JIRA_LEDGER_ISSUE%%-*}" != "$JIRA_WRITE_PROJECT" ]; then
  fail "JIRA_LEDGER_ISSUE is $JIRA_LEDGER_ISSUE and JIRA_WRITE_PROJECT is $JIRA_WRITE_PROJECT. The ledger is read with the same credential in the same project, and one that names another project measures a workflow this lane never writes to."
fi

if [ -n "${JIRA_ISSUE:-}" ] && [ "${JIRA_ISSUE%%-*}" = "$JIRA_WRITE_PROJECT" ]; then
  fail "JIRA_WRITE_PROJECT is $JIRA_WRITE_PROJECT, the project JIRA_ISSUE ($JIRA_ISSUE) is read from. A project this lane writes to is not the project a read lane observes."
fi

TMP=$(mktemp -d "${TMPDIR:-/tmp}/fiddle-live-jira-write-XXXXXX")
LEFT_OPEN=""
report_litter() {
  if [ -n "$LEFT_OPEN" ]; then
    echo "live-jira-write: LEFT OPEN in $JIRA_WRITE_PROJECT and not closed:$LEFT_OPEN" >&2
    echo "live-jira-write: close each by hand through the $JIRA_CLOSING_TRANSITION transition. Do not delete them: this project refuses a delete by policy, and a run that inherits an open ticket carrying a live marker reads it as an ambiguous match." >&2
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
LEDGER_ROUTE="/rest/api/3/issue/$JIRA_LEDGER_ISSUE"
MARKER="fx-live-$(date -u +%Y%m%d%H%M%S)-$$"
CLAIM_ROUTE="$LEDGER_ROUTE/properties/$MARKER"
JQL="project = $JIRA_WRITE_PROJECT AND labels = $MARKER AND statusCategory != Done"
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
    jq -e '.issues | type == "array"' "$TMP/page" >/dev/null \
      || fail "a search answered no issues array, so a count taken from it would be a count of nothing: $(cat "$TMP/page")"
    jq -e 'all(.issues[]; has("key"))' "$TMP/page" >/dev/null \
      || fail "a search answered an issue with no key while fields=key was asked, so the shape this lane and FileVerdict both rely on has changed: $(cat "$TMP/page")"
    jq -r '.issues[].key' "$TMP/page" >> "$TMP/matched"
    token=$(jq -r '.nextPageToken // empty' "$TMP/page")
    [ -n "$token" ] || { sort -u "$TMP/matched" -o "$TMP/matched"; return 0; }
    page=$((page + 1))
  done
  fail "the search offered a further page after 1000 of them, and a count taken from part of a result is a floor and never a total"
}

matched_count() { wc -l < "$TMP/matched" | tr -d ' '; }

echo "=== the preconditions, all of them asserted before anything is written ==="

sent ledger GET "$LEDGER_ROUTE"
[ "$CODE" = 200 ] || fail "the ledger issue $JIRA_LEDGER_ISSUE answered HTTP $CODE. It must exist before this lane runs, because a claim cannot be written on an issue the site does not hold."
LEDGER_TYPE=$(jq -r '.fields.issuetype.name // "absent"' "$TMP/ledger")
[ "$LEDGER_TYPE" = "$JIRA_ISSUE_TYPE" ] || fail "the ledger issue $JIRA_LEDGER_ISSUE is a $LEDGER_TYPE and this lane files a $JIRA_ISSUE_TYPE. A Jira workflow is per issue type, so a closing transition resolved on the ledger would say nothing about the ticket this lane creates."
note "the ledger issue $JIRA_LEDGER_ISSUE exists and is a $LEDGER_TYPE, the type this lane files"

sent transitions GET "$LEDGER_ROUTE/transitions"
[ "$CODE" = 200 ] || fail "the transitions of $JIRA_LEDGER_ISSUE answered HTTP $CODE, so this lane cannot say whether it is able to close what it is about to write"
CLOSING_NAME="$JIRA_CLOSING_TRANSITION"
resolve_closing() {
  local wanted="$1"
  jq -r --arg wanted "$wanted" '[.transitions[] | select(.to.name == $wanted) | .id] | join(" ")' "$TMP/transitions"
}
CLOSING_IDS=$(resolve_closing "$CLOSING_NAME")
if [ -z "$CLOSING_IDS" ]; then
  note "the workflow offers no transition reaching $CLOSING_NAME, so this lane falls back to $JIRA_CLOSING_FALLBACK and the record must say so"
  CLOSING_NAME="$JIRA_CLOSING_FALLBACK"
  CLOSING_IDS=$(resolve_closing "$CLOSING_NAME")
fi
CLOSING_COUNT=$(printf '%s' "$CLOSING_IDS" | wc -w | tr -d ' ')
if [ "$CLOSING_COUNT" -ne 1 ]; then
  fail "$CLOSING_COUNT transitions reach $CLOSING_NAME from the ledger issue's state, and a close is sent as one id and never matched by category: fiddle-pu2c MEASURED that Won't Do and Done share the category done, so a category match cannot tell them apart. Offered: $(jq -r '[.transitions[] | "\(.id)->\(.to.name)"] | join(", ")' "$TMP/transitions")"
fi
note "the closing transition resolves to exactly one id on this workflow: $CLOSING_IDS -> $CLOSING_NAME"

PROBE_ROUTE="$LEDGER_ROUTE/properties/fiddle-write-lane-probe"
sent probe PUT "$PROBE_ROUTE" '{"probe":"the token can write a property on the ledger"}'
case "$CODE" in
  200|201) ;;
  *) fail "a property write on $JIRA_LEDGER_ISSUE answered HTTP $CODE. The claim ledger is the whole exactly-once mechanism, and a run that discovers it cannot write one after it has filed a ticket is the run that left ISP-272 and ISP-273 behind." ;;
esac
sent probe GET "$PROBE_ROUTE"
[ "$CODE" = 200 ] || fail "a property written on $JIRA_LEDGER_ISSUE read back HTTP $CODE with no wait, so this site does not offer the immediate consistency the ledger rests on"
sent probe DELETE "$PROBE_ROUTE"
[ "$CODE" = 204 ] || note "the probe property on $JIRA_LEDGER_ISSUE answered HTTP $CODE to a delete and remains"
note "MEASURED a property on $JIRA_LEDGER_ISSUE written, read back immediately and removed"

sent claim GET "$CLAIM_ROUTE"
[ "$CODE" = 404 ] || fail "the claim $MARKER already exists on $JIRA_LEDGER_ISSUE (HTTP $CODE), so this run's marker is not unique to this run"

every_match
[ "$(matched_count)" = 0 ] || fail "the marker $MARKER already matches $(tr '\n' ' ' < "$TMP/matched"), so this run's marker is not unique to this run"
note "the marker matches nothing and the ledger holds no claim for it, so every count below is this run's"

file_once() {
  local phase="$1"
  CREATED_KEY=""
  sent claim GET "$CLAIM_ROUTE"
  case "$CODE" in
    200)
      local held
      held=$(jq -r '.value.filed // "unsettled"' "$TMP/claim")
      case "$held" in
        unsettled)
          fail "$phase: the ledger holds a claim for $MARKER that names no issue. A create may have committed and this lane must not repeat it: read $JIRA_WRITE_PROJECT for the marker by hand and settle the claim before running again."
          ;;
        *)
          note "$phase: the ledger already claims $held for this marker, so nothing was created"
          ;;
      esac
      return 0
      ;;
    404) ;;
    *) fail "$phase: the claim read answered HTTP $CODE: $(cat "$TMP/claim")" ;;
  esac

  sent claimed PUT "$CLAIM_ROUTE" "$(jq -nc --arg marker "$MARKER" '{marker: $marker}')"
  case "$CODE" in
    200|201) ;;
    *) fail "$phase: the claim write answered HTTP $CODE and nothing was created: $(cat "$TMP/claimed")" ;;
  esac

  local body
  body=$(jq -nc \
    --arg project "$JIRA_WRITE_PROJECT" \
    --arg type "$JIRA_ISSUE_TYPE" \
    --arg marker "$MARKER" \
    --arg summary "fiddle live write lane $MARKER" \
    '{fields: {project: {key: $project}, issuetype: {name: $type}, summary: $summary, labels: ["fiddle-live-lane", $marker]},
      properties: [{key: $marker, value: {marker: $marker}}]}')
  sent created POST "$CREATE_ROUTE" "$body"
  local create_code="$CODE"
  if [ "$create_code" != 201 ]; then
    local refused
    refused=$(cat "$TMP/created")
    sent released DELETE "$CLAIM_ROUTE"
    fail "$phase: a create answered HTTP $create_code and the claim was released with HTTP $CODE, so the next run reads no claim and is not wedged on a create that never happened: $refused"
  fi
  CREATED_KEY=$(jq -r '.key' "$TMP/created")
  CREATED_AT=$(date +%s)
  creates=$((creates + 1))
  LEFT_OPEN="$LEFT_OPEN $CREATED_KEY"
  sent settled PUT "$CLAIM_ROUTE" "$(jq -nc --arg marker "$MARKER" --arg filed "$CREATED_KEY" '{marker: $marker, filed: $filed}')"
  case "$CODE" in
    200|201) ;;
    *) fail "$phase: $CREATED_KEY was created and the claim could not be given its key (HTTP $CODE), so the ledger names no issue and the next run cannot settle it by reading" ;;
  esac
  note "$phase: the ledger held no claim, so this run created $CREATED_KEY and the claim now names it"
}

echo "=== the first run, which finds no claim and files ==="
file_once "run one"
[ -n "$CREATED_KEY" ] || fail "the first run created nothing, so the marker was not unique to this run"
FILED_KEY="$CREATED_KEY"

echo "=== the second run, sent immediately, which is the interruption case ==="
file_once "run two"
note "MEASURED runs that created an issue: $creates of 2"

echo "=== what the search index says while the claim already knows ==="
every_match
note "MEASURED issues the index shows for this marker immediately after the create: $(matched_count) of $creates created"

echo "=== how long the index took to admit the new issue ==="
LAG=unmeasured
WAITED=0
for _ in $(seq 1 300); do
  every_match
  if [ "$(matched_count)" -eq "$creates" ] && grep -qx "$FILED_KEY" "$TMP/matched"; then
    LAG=$(( $(date +%s) - CREATED_AT ))
    break
  fi
  WAITED=$((WAITED + 1))
  sleep 1
done
if [ "$LAG" = unmeasured ]; then
  note "MEASURED the indexing lag: after $WAITED seconds the search still did not show exactly the $creates issue this run filed, so the lag is longer than any wait this lane holds and remains unmeasured"
else
  note "MEASURED the indexing lag: $LAG seconds after the create was accepted, the search showed exactly $creates issue and it was $FILED_KEY. This number is taken from a search whose count agrees with the number of creates; a number taken from a search that disagreed with it would be a number about a stale index."
fi

echo "=== what the site holds now ==="
every_match
HELD=$(matched_count)
note "MEASURED open issues carrying $MARKER after two runs: $HELD"
note "MEASURED searches sent: $searches; creates accepted: $creates"

echo "=== closing what this lane wrote, because this project does not delete ==="
: > "$TMP/to-close"
printf '%s\n' $LEFT_OPEN >> "$TMP/to-close"
cat "$TMP/matched" >> "$TMP/to-close"
sort -u "$TMP/to-close" -o "$TMP/to-close"
if grep -qx "$JIRA_LEDGER_ISSUE" "$TMP/to-close"; then
  fail "the close list names the ledger issue $JIRA_LEDGER_ISSUE. The ledger outlives every run and is never closed by one."
fi
STILL_OPEN=""
CLOSED=0
ASKED=0
while read -r key; do
  [ -n "$key" ] || continue
  ASKED=$((ASKED + 1))
  sent offered GET "/rest/api/3/issue/$key/transitions"
  if [ "$CODE" != 200 ]; then
    note "the transitions of $key answered HTTP $CODE"
    STILL_OPEN="$STILL_OPEN $key"
    continue
  fi
  ids=$(jq -r --arg wanted "$CLOSING_NAME" '[.transitions[] | select(.to.name == $wanted) | .id] | join(" ")' "$TMP/offered")
  count=$(printf '%s' "$ids" | wc -w | tr -d ' ')
  if [ "$count" -ne 1 ]; then
    note "$count transitions on $key reach $CLOSING_NAME, and a close is sent as one id and never chosen from several"
    STILL_OPEN="$STILL_OPEN $key"
    continue
  fi
  sent closed POST "/rest/api/3/issue/$key/transitions" "$(jq -nc --arg id "$ids" '{transition: {id: $id}}')"
  if [ "$CODE" != 204 ]; then
    note "the site would not close $key through transition $ids: HTTP $CODE"
    STILL_OPEN="$STILL_OPEN $key"
    continue
  fi
  sent verify GET "/rest/api/3/issue/$key"
  reached=$(jq -r '.fields.status.name // "unreadable"' "$TMP/verify")
  if [ "$reached" != "$CLOSING_NAME" ]; then
    note "$key answered 204 to the close and reads back as $reached, not $CLOSING_NAME"
    STILL_OPEN="$STILL_OPEN $key"
    continue
  fi
  CLOSED=$((CLOSED + 1))
  note "closed $key as $CLOSING_NAME through transition $ids, verified by a second read"
done < "$TMP/to-close"
note "closed $CLOSED of $ASKED issues this lane knows it wrote or matched"
LEFT_OPEN="$STILL_OPEN"

sent released DELETE "$CLAIM_ROUTE"
case "$CODE" in
  204) note "the claim for $MARKER was removed from $JIRA_LEDGER_ISSUE" ;;
  *) note "the claim for $MARKER answered HTTP $CODE to a delete and remains on $JIRA_LEDGER_ISSUE" ;;
esac

echo
note "NOT MEASURED: whether a page boundary shifts under a walk. Forcing it needs an issue indexed between two pages of one walk, which one process cannot arrange."
note "NOT MEASURED: concurrent duplicate invocations. The design scopes exactly-once to interruptions only, and one process cannot race itself."
note "NOT DRIVEN THROUGH FIDDLE: this lane sends the claim-then-create requests by hand, so it measures the site and not the build. A green run here is evidence about Atlassian and not about FileVerdict. scripts/live-jira-file-verdict.sh is the lane that drives FileVerdict itself."

if [ "$creates" -ne 1 ]; then
  fail "two runs sent $creates creates, and exactly-once across an interruption means exactly one"
fi
if [ -n "$LEFT_OPEN" ]; then
  fail "the cleanup left$LEFT_OPEN open, and an open ticket carrying a live marker is the ambiguous match the next run inherits"
fi
note "PASS: two runs of the claim-then-create protocol left exactly one issue, it was closed as $CLOSING_NAME, and the credential reached none of $((searches + ASKED + creates + 6)) answers this lane read"
