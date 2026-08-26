#!/usr/bin/env bash
set -euo pipefail

fail() { echo "live-jira-observe: FAIL: $*" >&2; exit 1; }
note() { echo "live-jira-observe: $*"; }

: "${JIRA_USER_EMAIL:?live-jira-observe.sh needs JIRA_USER_EMAIL. This lane fails rather than skips, because a silently-skipped lane cannot be told from a passing one.}"
: "${JIRA_API_TOKEN:?live-jira-observe.sh needs JIRA_API_TOKEN. This lane fails rather than skips, because a silently-skipped lane cannot be told from a passing one.}"
: "${JIRA_SITE:?live-jira-observe.sh needs JIRA_SITE, as in JIRA_SITE=https://snplow.atlassian.net. This lane fails rather than skips, because a silently-skipped lane cannot be told from a passing one.}"
: "${JIRA_ISSUE:?live-jira-observe.sh needs JIRA_ISSUE, the issue key to read. This lane fails rather than skips, because a silently-skipped lane cannot be told from a passing one.}"
: "${FIDDLE_BIN:?live-jira-observe.sh needs FIDDLE_BIN — the path to the compiled fiddle, as in FIDDLE_BIN=\"\$PWD/target/release/fiddle\". This lane fails rather than skips, because a lane that read a fiddle it did not name measures an unknown build.}"

[ -x "$FIDDLE_BIN" ] || fail "FIDDLE_BIN is not an executable file: $FIDDLE_BIN"
command -v curl >/dev/null 2>&1 || fail "curl must be on PATH"
command -v jq >/dev/null 2>&1 || fail "jq must be on PATH"

case "$JIRA_SITE" in
  https://*) ;;
  *) fail "JIRA_SITE must be an https origin and this is not one: $JIRA_SITE" ;;
esac

case "$JIRA_ISSUE" in
  *-*) PROJECT="${JIRA_ISSUE%%-*}" ;;
  *) fail "JIRA_ISSUE must be an issue key of the form PROJECT-1 and this is not one: $JIRA_ISSUE" ;;
esac

TMP=$(mktemp -d "${TMPDIR:-/tmp}/fiddle-live-jira-XXXXXX")
trap 'rm -rf "$TMP"' EXIT

printf '%s\n' "$JIRA_API_TOKEN" > "$TMP/needle"
chmod 600 "$TMP/needle"

mkdir -p "$TMP/stub-state/work" "$TMP/stub-state/changes"

cat > "$TMP/fiddle.toml" <<TOML
[project]
name = "live-jira-observe"

[stub]
root = "$TMP/stub-state"

[report]
dir = "$TMP/reports"

[jira]
site = "$JIRA_SITE"
project = "$PROJECT"
user = { env = "JIRA_USER_EMAIL" }
token = { env = "JIRA_API_TOKEN" }
timeout = "60s"
TOML

if grep -F -q -f "$TMP/needle" "$TMP/fiddle.toml"; then
  fail "the token reached the generated document; the document names variables and carries no value"
fi

note "reading $JIRA_ISSUE from $JIRA_SITE directly, beside what fiddle reports"

raw_issue=$(curl -fsSL \
  -u "$JIRA_USER_EMAIL:$JIRA_API_TOKEN" \
  -H "Accept: application/json" \
  "$JIRA_SITE/rest/api/3/issue/$JIRA_ISSUE?fields=status,updated") \
  || fail "the site would not answer a direct read"

jq -e '.fields.updated' <<<"$raw_issue" >/dev/null \
  || fail "fields.updated is absent, and it is the revision the design uses"
jq -e 'has("version") | not' <<<"$raw_issue" >/dev/null \
  || note "this site DOES expose an issue version; ADR 077 and the target format should be revisited"

updated=$(jq -r '.fields.updated' <<<"$raw_issue")
offset=$(sed -E 's/^.*[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?//' <<<"$updated")
note "fields.updated is \`$updated\`, whose offset is \`$offset\`"
case "$offset" in
  Z | z) note "the offset is the RFC 3339 zulu designator, so Rfc3339 alone reads it" ;;
  *:*) note "the offset carries a colon, so Rfc3339 alone reads it" ;;
  [+-][0-9][0-9][0-9][0-9]) note "the offset is colonless, which is not RFC 3339 and is why read_instant carries two further format descriptions" ;;
  *) note "the offset \`$offset\` is a shape this lane has no name for, and read_instant is the thing that decides whether it parses" ;;
esac

code=0
"$FIDDLE_BIN" inspect "jira:$JIRA_ISSUE" --json \
  --config "$TMP/fiddle.toml" \
  > "$TMP/inspect.json" 2> "$TMP/inspect.err" || code=$?

if grep -F -q -f "$TMP/needle" "$TMP/inspect.json" "$TMP/inspect.err"; then
  fail "a credential reached fiddle's own output; not printing it"
fi

[ "$code" = 0 ] || fail "fiddle inspect exited $code:
$(cat "$TMP/inspect.err")"

reported=$(cat "$TMP/inspect.json")
jq -e . <<<"$reported" >/dev/null || fail "fiddle's stdout is not JSON:
$reported"

[ "$(jq -r '.observations.work_item.available.value.status' <<<"$reported")" != "null" ] \
  || fail "the issue reported no status"
[ "$(jq -r '.observations.work_item.available.revision' <<<"$reported")" != "null" ] \
  || fail "the issue reported no revision, so no target identity can name a state of it"
state=$(jq -r '.observations.work_item.available.value.projected.state' <<<"$reported")
[ "$state" != "null" ] || fail "no typed state was projected"
[ "$state" != "unknown" ] || note "the real status maps to no configured name and no known category: record it"

note "status \`$(jq -r '.observations.work_item.available.value.status' <<<"$reported")\` projects to \`$state\`"
note "revision $(jq -r '.observations.work_item.available.revision' <<<"$reported"), canonicalised from \`$updated\`"

echo "--- the real issue, recorded so M5b designs against a measurement ---"
jq . <<<"$raw_issue"

note "PASS: $JIRA_SITE answered for $JIRA_ISSUE, fields.updated is present, and fiddle read a status, a projection and a revision off it"
