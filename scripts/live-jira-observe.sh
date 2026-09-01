#!/usr/bin/env bash
set -euo pipefail

LANE_HOME="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PORT_SOURCE="$LANE_HOME/../crates/fiddle-runtime/src/jira/work_item.rs"

TMP=""
RAW_LABELS=unread
RAW_DESCRIPTION=unread
RAW_COMMENTS=unread

fail() { echo "live-jira-observe: FAIL: $*" >&2; exit 1; }
note() { echo "live-jira-observe: $*"; }

fields_the_port_asks_for() {
  [ -f "$PORT_SOURCE" ] \
    || fail "the port's field list lives in $PORT_SOURCE and that file is absent, so this lane cannot say which fields it is grading"
  local asked field
  asked=$(sed -n 's/^const FIELDS: &str = "\([^"]*\)";$/\1/p' "$PORT_SOURCE")
  [ -n "$asked" ] \
    || fail "$PORT_SOURCE declares no \`const FIELDS\` this lane can read, so this lane cannot say which fields the port asks for"
  for field in status updated labels description comment; do
    case ",$asked," in
      *",$field,"*) ;;
      *) fail "the port asks for \`$asked\` and this lane grades \`$field\`, which is not in that list; the lane and the port have drifted, and a shape nobody asked for is not a measurement" ;;
    esac
  done
  printf '%s\n' "$asked"
}

shape_of() {
  local answer="$1" path="$2"
  jq -r --arg path "$path" '
    getpath($path | split("."))
    | if type == "object" then "a document"
      elif type == "array" then "a list"
      elif type == "string" then "text"
      elif type == "number" then "a number"
      elif type == "boolean" then "a true or a false"
      else "absent" end' "$answer"
}

record_labels() {
  local answer="$1" shape count untyped
  shape=$(shape_of "$answer" fields.labels)
  case "$shape" in
    absent)
      RAW_LABELS=absent
      note "\`fields.labels\` is absent, so the port reads no label off this issue"
      return 0
      ;;
    "a list") ;;
    *) fail "\`fields.labels\` is $shape, and \`labels_in\` reads a list of labels off it" ;;
  esac
  count=$(jq '.fields.labels | length' "$answer")
  untyped=$(jq '[.fields.labels[] | select(type != "string")] | length' "$answer")
  [ "$untyped" -eq 0 ] \
    || fail "\`fields.labels\` holds $untyped of $count entries that are not text, and \`labels_in\` reads a label as text"
  RAW_LABELS=$count
  note "\`fields.labels\` is a list of $count, and $count of $count entries are text"
}

record_description() {
  local answer="$1" shape bytes kind version
  shape=$(shape_of "$answer" fields.description)
  case "$shape" in
    absent)
      RAW_DESCRIPTION=absent
      note "\`fields.description\` is absent, so the port reads no description off this issue"
      return 0
      ;;
    text | "a document") ;;
    *) fail "\`fields.description\` is $shape, and \`description_in\` reads text or a document off it" ;;
  esac
  bytes=$(jq -j -c '.fields.description' "$answer" | wc -c | tr -d ' ')
  RAW_DESCRIPTION=$bytes
  if [ "$shape" = "a document" ]; then
    kind=$(jq -r '.fields.description.type // "unstated"' "$answer")
    version=$(jq -r '.fields.description.version // "unstated"' "$answer")
    note "\`fields.description\` is a document of $bytes bytes, type \`$kind\`, version \`$version\`, which \`written\` flattens rather than reads verbatim"
  else
    note "\`fields.description\` is text of $bytes bytes"
  fi
}

record_comment() {
  local answer="$1" shape carried total page unnamed silent
  shape=$(shape_of "$answer" fields.comment)
  case "$shape" in
    absent)
      RAW_COMMENTS=absent
      note "\`fields.comment\` is absent, so the port reads no conversation off this issue"
      return 0
      ;;
    "a document") ;;
    *) fail "\`fields.comment\` is $shape, and \`comments_in\` reads a comment container off it" ;;
  esac
  [ "$(shape_of "$answer" fields.comment.comments)" = "a list" ] \
    || fail "\`fields.comment\` carries no \`comments\` list, so it says nothing about who replied"
  [ "$(shape_of "$answer" fields.comment.total)" = "a number" ] \
    || fail "\`fields.comment\` carries no \`total\` number, so an absent reply would be a floor and not an answer"
  carried=$(jq '.fields.comment.comments | length' "$answer")
  total=$(jq '.fields.comment.total' "$answer")
  page=$(jq -r '.fields.comment.maxResults // "unstated"' "$answer")
  [ "$total" -le "$carried" ] \
    || fail "\`fields.comment\` carried $carried of $total comments, so an absent reply would be a floor and not an answer"
  unnamed=$(jq '[.fields.comment.comments[] | select((.id | type) != "string" or (.author.accountId | type) != "string")] | length' "$answer")
  [ "$unnamed" -eq 0 ] \
    || fail "\`fields.comment.comments\` holds $unnamed of $carried comments carrying no \`id\` or no \`author.accountId\`, and \`reply_from\` reads neither as a reply"
  silent=$(jq '[.fields.comment.comments[] | select((.body | type) as $held | $held != "object" and $held != "string" and $held != "array")] | length' "$answer")
  RAW_COMMENTS=$carried
  note "\`fields.comment\` is a container of $carried of $total comments, page size $page, all $carried carrying \`id\` and \`author.accountId\`, and $silent of $carried carrying a body \`written\` reads as nothing"
}

refuse_unless_available() {
  local reported="$1" reason
  jq -e . "$reported" >/dev/null 2>&1 || fail "fiddle's stdout is not JSON:
$(cat "$reported")"
  reason=$(jq -r '.observations.work_item.unavailable.reason // empty' "$reported")
  [ -z "$reason" ] || fail "fiddle read no work item off the answer and said: $reason"
  reason=$(jq -r '.observations.work_item.not_applicable.reason // empty' "$reported")
  [ -z "$reason" ] || fail "fiddle called the work item not applicable and said: $reason"
  jq -e '.observations.work_item.available' "$reported" >/dev/null \
    || fail "fiddle reported no available work item and no reason for it, so its answer says nothing this lane can grade"
}

agree_or_refuse() {
  local reported="$1" labels description comments
  labels=$(jq -r '.observations.work_item.available.value.labels | if . == null then "absent" else length end' "$reported")
  description=$(jq -r '.observations.work_item.available.value.description | if . == null then "absent" else length end' "$reported")
  comments=$(jq -r '.observations.work_item.available.value.comments | if . == null then "absent" else length end' "$reported")
  [ "$labels" = "$RAW_LABELS" ] \
    || fail "the direct read carried $RAW_LABELS for \`fields.labels\` and fiddle reported $labels; either the two reads saw different revisions of the issue or the port reads that field differently from this lane"
  [ "$comments" = "$RAW_COMMENTS" ] \
    || fail "the direct read carried $RAW_COMMENTS for \`fields.comment\` and fiddle reported $comments comments; either the two reads saw different revisions of the issue or the port reads that field differently from this lane"
  case "$RAW_DESCRIPTION" in
    absent)
      [ "$description" = absent ] \
        || fail "the direct read carried no \`fields.description\` and fiddle reported one of $description characters"
      ;;
    *)
      [ "$description" != absent ] \
        || fail "the direct read carried $RAW_DESCRIPTION bytes of \`fields.description\` and fiddle reported none"
      [ "$description" -gt 0 ] \
        || fail "the direct read carried $RAW_DESCRIPTION bytes of \`fields.description\` and \`written\` flattened it to nothing, so the port reads this document as silence"
      ;;
  esac
  note "fiddle reported $labels labels, a description of $description characters and $comments comments off the same issue, which agrees with the answer above"
}

main() {
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

  local PROJECT
  case "$JIRA_ISSUE" in
    *-*) PROJECT="${JIRA_ISSUE%%-*}" ;;
    *) fail "JIRA_ISSUE must be an issue key of the form PROJECT-1 and this is not one: $JIRA_ISSUE" ;;
  esac

  local FIELDS
  FIELDS=$(fields_the_port_asks_for)

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
  note "the port asks for \`$FIELDS\`, read off crates/fiddle-runtime/src/jira/work_item.rs"

  local raw_issue updated offset
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

  curl -fsSL \
    -u "$JIRA_USER_EMAIL:$JIRA_API_TOKEN" \
    -H "Accept: application/json" \
    "$JIRA_SITE/rest/api/3/issue/$JIRA_ISSUE?fields=$FIELDS" \
    > "$TMP/asked.json" \
    || fail "the site would not answer a read of \`$FIELDS\`, which is the field list the port sends"

  if grep -F -q -f "$TMP/needle" "$TMP/asked.json"; then
    fail "a credential reached the site's answer; not printing it"
  fi
  jq -e . "$TMP/asked.json" >/dev/null 2>&1 || fail "the site's answer to \`$FIELDS\` is not JSON:
$(cat "$TMP/asked.json")"

  local code=0
  "$FIDDLE_BIN" inspect "jira:$JIRA_ISSUE" --json \
    --config "$TMP/fiddle.toml" \
    > "$TMP/inspect.json" 2> "$TMP/inspect.err" || code=$?

  if grep -F -q -f "$TMP/needle" "$TMP/inspect.json" "$TMP/inspect.err"; then
    fail "a credential reached fiddle's own output; not printing it"
  fi

  [ "$code" = 0 ] || fail "fiddle inspect exited $code:
$(cat "$TMP/inspect.err")"

  local asked_updated
  asked_updated=$(jq -r '.fields.updated' "$TMP/asked.json")
  [ "$asked_updated" = "$updated" ] \
    || fail "the issue moved between the two direct reads, from \`$updated\` to \`$asked_updated\`; the shapes below would compare two different answers, so run the lane again"

  record_labels "$TMP/asked.json"
  record_description "$TMP/asked.json"
  record_comment "$TMP/asked.json"

  refuse_unless_available "$TMP/inspect.json"

  [ "$(jq -r '.observations.work_item.available.value.status' "$TMP/inspect.json")" != "null" ] \
    || fail "the issue reported no status"
  [ "$(jq -r '.observations.work_item.available.revision' "$TMP/inspect.json")" != "null" ] \
    || fail "the issue reported no revision, so no target identity can name a state of it"
  local state
  state=$(jq -r '.observations.work_item.available.value.projected_status.state' "$TMP/inspect.json")
  [ "$state" != "null" ] || fail "no typed state was projected"
  [ "$state" != "unknown" ] || note "the real status maps to no configured name and no known category: record it"

  note "status \`$(jq -r '.observations.work_item.available.value.status' "$TMP/inspect.json")\` projects to \`$state\`"
  note "revision $(jq -r '.observations.work_item.available.revision' "$TMP/inspect.json"), canonicalised from \`$updated\`"

  agree_or_refuse "$TMP/inspect.json"

  echo "--- the real issue as \`fields=status,updated\` returns it, recorded so M5b designs against a measurement ---"
  jq . <<<"$raw_issue"

  note "PASS: $JIRA_SITE answered for $JIRA_ISSUE, both reads asked for \`$FIELDS\`, and the lines above record what came back for \`labels\`, \`description\` and \`comment\`"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
  main "$@"
fi
