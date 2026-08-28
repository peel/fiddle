#!/usr/bin/env bash
set -euo pipefail

fail() { echo "live-jira-search-shape: FAIL: $*" >&2; exit 2; }
note() { echo "live-jira-search-shape: $*"; }

needs() {
  local name="$1" why="$2"
  [ -n "${!name:-}" ] || fail "this lane needs $name. $why It fails rather than skips, because a silently-skipped lane cannot be told from a passing one."
}

needs JIRA_USER_EMAIL "It is the account the requests authenticate as."
needs JIRA_API_TOKEN "It is the credential, and it is read from the environment and never written to a file this lane generates."
needs JIRA_SITE "It is the site origin, as in JIRA_SITE=https://snplow.atlassian.net."
needs JIRA_SEARCH_PROJECT "It is the project key to search in, and a search over an unnamed project measures an unknown result set."

command -v curl >/dev/null 2>&1 || fail "curl must be on PATH"
command -v jq >/dev/null 2>&1 || fail "jq must be on PATH"

case "$JIRA_SITE" in
  https://*) ;;
  *) fail "JIRA_SITE must be an https origin and this is not one: $JIRA_SITE" ;;
esac

TMP=$(mktemp -d "${TMPDIR:-/tmp}/fiddle-live-jira-shape-XXXXXX")
trap 'rm -rf "$TMP"' EXIT

printf '%s\n' "$JIRA_API_TOKEN" > "$TMP/needle"
chmod 600 "$TMP/needle"

printf 'a decoy line carrying %s in the clear\n' "$JIRA_API_TOKEN" > "$TMP/planted"
chmod 600 "$TMP/planted"
grep -F -q -f "$TMP/needle" "$TMP/planted" \
  || fail "the credential search did not find a planted credential, so its finding nothing elsewhere would prove nothing"
note "the credential search finds a planted credential, so every absence below is a measurement"

STATUS_ROUTE="/rest/api/3/project/$JIRA_SEARCH_PROJECT/statuses"
SEARCH_ROUTE="/rest/api/3/search/jql"
JQL="project = $JIRA_SEARCH_PROJECT"

asked=0
CODE=""
ask() {
  local into="$1"; shift
  local route="$1"; shift
  local code
  code=$(curl -sS -G -o "$TMP/$into" -w '%{http_code}' \
    -u "$JIRA_USER_EMAIL:$JIRA_API_TOKEN" \
    -H "Accept: application/json" \
    "$@" \
    "$JIRA_SITE$route") || fail "the site would not answer $route"
  if grep -F -q -f "$TMP/needle" "$TMP/$into"; then
    fail "the site echoed the credential in its answer to $route; not printing it"
  fi
  asked=$((asked + 1))
  CODE="$code"
}

members() { jq -r 'if type == "object" then (keys | join(", ")) else "not an object" end' "$TMP/$1"; }
issues_in() { jq -r '.issues | if type == "array" then length else "absent" end' "$TMP/$1"; }
first_key() { jq -r '.issues[0].key // "none"' "$TMP/$1"; }

echo "=== the search answer with no paging parameters ==="
ask plain "$SEARCH_ROUTE" --data-urlencode "jql=$JQL" --data-urlencode "fields=key"
[ "$CODE" = 200 ] || fail "a plain search answered HTTP $CODE, so no shape below could be measured: $(cat "$TMP/plain")"
note "top level members: $(members plain)"
DEFAULT_SIZE=$(issues_in plain)
note "MEASURED default page size: $DEFAULT_SIZE issues; the stub assumes 50"
case "$DEFAULT_SIZE" in
  50) note "the stub's page cap matches the site" ;;
  *) note "the stub's assumed 50 DIVERGES from the site's $DEFAULT_SIZE; stub_jira PAGE_CAP should be revisited" ;;
esac

if jq -e 'has("isLast")' "$TMP/plain" >/dev/null; then
  note "MEASURED isLast: present, and it reads $(jq -r '.isLast' "$TMP/plain")"
else
  note "MEASURED isLast: ABSENT on this site; stub_jira emits it and jira_effects.rs asserts on it, and that assertion must move. No walk breaks, because all_search_matches ends on the absence of nextPageToken."
fi
for withdrawn in total startAt maxResults; do
  if jq -e "has(\"$withdrawn\")" "$TMP/plain" >/dev/null; then
    note "MEASURED $withdrawn: PRESENT; the stub answers none, and a caller could read a count off one answer"
  else
    note "MEASURED $withdrawn: absent, as the stub answers"
  fi
done

echo "=== the search answer with maxResults=1 ==="
ask one "$SEARCH_ROUTE" --data-urlencode "jql=$JQL" --data-urlencode "fields=key" --data-urlencode "maxResults=1"
[ "$CODE" = 200 ] || fail "a one-issue page answered HTTP $CODE: $(cat "$TMP/one")"
note "a page of $(issues_in one) issue, first key $(first_key one)"
TOKEN_ONE=$(jq -r '.nextPageToken // "none"' "$TMP/one")
if [ "$TOKEN_ONE" = none ]; then
  note "MEASURED nextPageToken: ABSENT on a page that is not the last; every walk in this build ends on that absence and would stop one page in"
else
  note "MEASURED nextPageToken: present"
  ask two "$SEARCH_ROUTE" --data-urlencode "jql=$JQL" --data-urlencode "fields=key" --data-urlencode "maxResults=1" --data-urlencode "nextPageToken=$TOKEN_ONE"
  [ "$CODE" = 200 ] || fail "following a page token answered HTTP $CODE: $(cat "$TMP/two")"
  if [ "$(first_key one)" = "$(first_key two)" ]; then
    fail "the second page repeats the first key $(first_key one), so following the token did not advance and a walk would not terminate"
  fi
  note "MEASURED the token advances: page one is $(first_key one) and page two is $(first_key two)"
fi

echo "=== the search answer with maxResults=500 ==="
ask wide "$SEARCH_ROUTE" --data-urlencode "jql=$JQL" --data-urlencode "fields=key" --data-urlencode "maxResults=500"
if [ "$CODE" != 200 ]; then
  note "MEASURED the cap: asking for 500 answered HTTP $CODE, so the site refuses rather than caps: $(cat "$TMP/wide")"
elif jq -e 'has("nextPageToken")' "$TMP/wide" >/dev/null; then
  note "MEASURED the cap: asking for 500 answered $(issues_in wide) issues AND a further page token, so the site capped the page below what was asked for, as the stub does"
else
  note "MEASURED the cap: asking for 500 answered $(issues_in wide) issues and NO further page token, so the site served every match in one page rather than capping at its default of $DEFAULT_SIZE. jira_effects.rs asserts the stub caps whatever the caller asks for; that assertion holds of the stub and is NOT a fact about this site below $(issues_in wide) matches."
fi

echo "=== startAt, which the stub refuses with 400 ==="
ask offset "$SEARCH_ROUTE" --data-urlencode "jql=$JQL" --data-urlencode "fields=key" --data-urlencode "maxResults=1" --data-urlencode "startAt=1"
if [ "$CODE" != 200 ]; then
  note "MEASURED startAt: the site answered HTTP $CODE, so the stub's 400 is the site's behaviour and the divergence should stand as agreement: $(cat "$TMP/offset")"
elif [ "$(first_key offset)" = "$(first_key one)" ]; then
  note "MEASURED startAt: the site answered 200 and returned the SAME first key as the unparameterised page, so it ignores startAt silently. The stub's 400 is a deliberate divergence toward strictness and SHOULD STAND: a caller that believes startAt worked would read page one while asking for page two."
else
  note "MEASURED startAt: the site answered 200 and returned a DIFFERENT first key ($(first_key offset) against $(first_key one)), so this endpoint still honours the offset. The stub's 400 diverges from the site and should be revisited."
fi

echo "=== the workflow statuses ADR 077 left unmeasured ==="
ask statuses "$STATUS_ROUTE"
if [ "$CODE" = 200 ]; then
  note "MEASURED the real [jira.workflow] status names for $JIRA_SEARCH_PROJECT:"
  jq -r '.[] | .name as $type | .statuses[] | "  \($type): \(.name)  [category \(.statusCategory.key)]"' "$TMP/statuses" | sort -u
else
  note "the statuses route answered HTTP $CODE, so the [jira.workflow] names remain unmeasured: $(cat "$TMP/statuses")"
fi

echo "=== what a create requires, beside what FileVerdict sends ==="
ask types "/rest/api/3/issue/createmeta/$JIRA_SEARCH_PROJECT/issuetypes"
if [ "$CODE" != 200 ]; then
  note "the create metadata route answered HTTP $CODE, so what a create requires stays unmeasured: $(cat "$TMP/types")"
else
  TYPE_ID=$(jq -r '[.issueTypes[]? | select(.subtask == false)] | (map(select(.name == "Task")) + map(select(.name == "Story")) + .) | .[0].id // empty' "$TMP/types")
  TYPE_NAME=$(jq -r --arg id "$TYPE_ID" '.issueTypes[]? | select(.id == $id) | .name' "$TMP/types")
  if [ -z "$TYPE_ID" ]; then
    note "the project offers no non-subtask issue type, so what a create requires stays unmeasured"
  else
    note "measuring against issue type \`$TYPE_NAME\` (id $TYPE_ID), preferred as the type a filed verdict would be, of $(jq -r '.issueTypes | length' "$TMP/types") the project offers. A second type may require a different set."
    ask fields "/rest/api/3/issue/createmeta/$JIRA_SEARCH_PROJECT/issuetypes/$TYPE_ID"
    if [ "$CODE" != 200 ]; then
      note "the create field route answered HTTP $CODE: $(cat "$TMP/fields")"
    else
      jq -r '.fields[] | select(.required == true) | .fieldId' "$TMP/fields" | sort > "$TMP/required"
      note "MEASURED the required fields of a create: $(tr '\n' ' ' < "$TMP/required")"
      printf '%s\n' project summary labels description | sort > "$TMP/sent"
      MISSING=$(comm -23 "$TMP/required" "$TMP/sent" | tr '\n' ' ')
      if [ -n "${MISSING// /}" ]; then
        note "MEASURED a DIVERGENCE: FileVerdict::body in crates/fiddle-runtime/src/jira/file_verdict.rs sends project, summary, labels and description, and this site also requires: $MISSING"
        note "the create stub in crates/fiddle-runtime/tests/support/stub_jira.rs requires fields.project.key alone, so no hermetic lane can red this"
      else
        note "MEASURED no divergence: every field this site requires is one FileVerdict::body already sends"
      fi
    fi
  fi
fi

echo
note "$asked requests reached $JIRA_SITE, every one a GET, and none of their answers carried the credential"
note "NOT MEASURED here: whether a page boundary shifts under a walk, and how long the indexing lag is. Both need a write, and scripts/live-jira-write.sh takes them."
note "PASS: the shape above is a measurement of $JIRA_SITE and no longer an assumption of stub_jira.rs"
