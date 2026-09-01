#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SHAPE="$SCRIPT_DIR/live-jira-search-shape.sh"
WRITE="$SCRIPT_DIR/live-jira-write.sh"
FILING="$SCRIPT_DIR/live-jira-file-verdict.sh"
FILING_TEST="$SCRIPT_DIR/../crates/fiddle-runtime/tests/live_jira_filing.rs"

UNREACHABLE="https://127.0.0.1:1"
FAILED=0
CHECKED=0

fail() { echo "test-live-jira-lanes: $*" >&2; FAILED=$((FAILED + 1)); }

ran() {
  local lane="$1"; shift
  OUT=$(env -i PATH="$PATH" HOME="$HOME" "$@" "$lane" 2>&1)
  CODE=$?
  CHECKED=$((CHECKED + 1))
}

refuses_without() {
  local lane="$1" absent="$2"; shift 2
  ran "$lane" "$@"
  if [ "$CODE" -eq 0 ]; then
    fail "$(basename "$lane") exited 0 with $absent absent; a lane that skips silently cannot be told from one that passed"
    return
  fi
  case "$OUT" in
    *"this lane needs $absent"*) ;;
    *) fail "$(basename "$lane") refused without $absent and did not name it: $OUT" ;;
  esac
}

for absent in JIRA_USER_EMAIL JIRA_API_TOKEN JIRA_SITE JIRA_SEARCH_PROJECT; do
  args=()
  [ "$absent" = JIRA_USER_EMAIL ] || args+=("JIRA_USER_EMAIL=bot@example.invalid")
  [ "$absent" = JIRA_API_TOKEN ] || args+=("JIRA_API_TOKEN=not-a-real-token")
  [ "$absent" = JIRA_SITE ] || args+=("JIRA_SITE=$UNREACHABLE")
  [ "$absent" = JIRA_SEARCH_PROJECT ] || args+=("JIRA_SEARCH_PROJECT=IDENT")
  refuses_without "$SHAPE" "$absent" "${args[@]}"
done

for absent in JIRA_USER_EMAIL JIRA_API_TOKEN JIRA_SITE JIRA_WRITE_PROJECT JIRA_LEDGER_ISSUE; do
  args=()
  [ "$absent" = JIRA_USER_EMAIL ] || args+=("JIRA_USER_EMAIL=bot@example.invalid")
  [ "$absent" = JIRA_API_TOKEN ] || args+=("JIRA_API_TOKEN=not-a-real-token")
  [ "$absent" = JIRA_SITE ] || args+=("JIRA_SITE=$UNREACHABLE")
  [ "$absent" = JIRA_WRITE_PROJECT ] || args+=("JIRA_WRITE_PROJECT=DISPOSABLE")
  [ "$absent" = JIRA_LEDGER_ISSUE ] || args+=("JIRA_LEDGER_ISSUE=DISPOSABLE-1")
  refuses_without "$WRITE" "$absent" "${args[@]}"
done

ran "$SHAPE" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
  JIRA_SITE=http://insecure.example.invalid JIRA_SEARCH_PROJECT=IDENT
[ "$CODE" -ne 0 ] || fail "the shape lane accepted a plaintext origin, and a credential rides every request it sends"
case "$OUT" in *"must be an https origin"*) ;; *) fail "the shape lane refused a plaintext origin without saying why: $OUT" ;; esac

ran "$WRITE" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
  JIRA_SITE="$UNREACHABLE" JIRA_WRITE_PROJECT=ISP JIRA_LEDGER_ISSUE=ISP-1 JIRA_ISSUE=ISP-1
[ "$CODE" -ne 0 ] || fail "the write lane accepted the project its read lane observes as a disposable one"
case "$OUT" in *"is not the project a read lane observes"*) ;; *) fail "the write lane refused a non-disposable project without saying why: $OUT" ;; esac

ran "$WRITE" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
  JIRA_SITE="$UNREACHABLE" JIRA_WRITE_PROJECT="not a key" JIRA_LEDGER_ISSUE=DISPOSABLE-1
[ "$CODE" -ne 0 ] || fail "the write lane accepted a project key that is not one"
case "$OUT" in *"must be a bare project key"*) ;; *) fail "the write lane refused a malformed key without saying why: $OUT" ;; esac

ran "$WRITE" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
  JIRA_SITE="$UNREACHABLE" JIRA_WRITE_PROJECT=DISPOSABLE JIRA_LEDGER_ISSUE=OTHER-1
[ "$CODE" -ne 0 ] || fail "the write lane accepted a ledger issue in another project, and a claim read there says nothing about the project it writes to"
case "$OUT" in *"The ledger is read with the same credential in the same project"*) ;; *) fail "the write lane refused a foreign ledger issue without saying why: $OUT" ;; esac

DELETES=$(grep -c 'DELETE' "$WRITE" || true)
PROPERTY_DELETES=$(grep 'DELETE' "$WRITE" | grep -c 'ROUTE' || true)
CLAIM_DELETES=$(grep 'DELETE' "$WRITE" | grep -c -E 'CLAIM_ROUTE|PROBE_ROUTE' || true)
if [ "$DELETES" -ne "$CLAIM_DELETES" ]; then
  fail "the write lane sends $DELETES deletes and only $CLAIM_DELETES of them name a property route, so at least one deletes something else. The operator ruled on 2026-08-28 that cleanup is a close and never a delete: ISP refuses a delete by policy, so a lane that deletes an issue leaves residue on every run. Deletes found: $(grep -n 'DELETE' "$WRITE" | tr '\n' ' ')"
fi
[ "$DELETES" -gt 0 ] || fail "the write lane sends no delete at all, so the count above compares nothing and would pass for a lane that deleted every issue through a helper this check cannot see"
[ "$PROPERTY_DELETES" -eq "$DELETES" ] || fail "a delete in the write lane names no route variable, so this check cannot say what it removes"
CHECKED=$((CHECKED + 1))

if grep -q -F "delete them by hand" "$WRITE"; then
  fail "the write lane still advises a reader to delete its residue by hand, which the operator cannot do in ISP"
fi
CHECKED=$((CHECKED + 1))

if ! grep -q -F 'select(.to.name == $wanted)' "$WRITE"; then
  fail "the write lane must resolve its closing transition by name to exactly one id. fiddle-pu2c MEASURED that a closing transition and Done share the category done, so a category match picks the wrong transition."
fi
CHECKED=$((CHECKED + 1))

if ! grep -q -F 'statusCategory != Done' "$WRITE"; then
  fail "the write lane must exclude closed issues from its marker search, or each run inherits the last run's closed ticket as an ambiguous match"
fi
CHECKED=$((CHECKED + 1))

for lane in "$SHAPE" "$WRITE"; do
  case "$lane" in
    "$SHAPE") ran "$lane" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
                 JIRA_SITE="$UNREACHABLE" JIRA_SEARCH_PROJECT=IDENT ;;
    *)        ran "$lane" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
                 JIRA_SITE="$UNREACHABLE" JIRA_WRITE_PROJECT=DISPOSABLE \
                 JIRA_LEDGER_ISSUE=DISPOSABLE-1 ;;
  esac
  [ "$CODE" -ne 0 ] || fail "$(basename "$lane") exited 0 against a site that answers nothing, so it reported a measurement it never took"
  case "$OUT" in
    *"this lane needs"*)
      fail "$(basename "$lane") reported a missing variable when every variable was given, so the refusals above would pass for a lane that refuses everything: $OUT"
      ;;
    *"would not answer"*) ;;
    *) fail "$(basename "$lane") gave every variable and an unreachable site, and said neither: $OUT" ;;
  esac
done

for absent in JIRA_USER_EMAIL JIRA_API_TOKEN JIRA_SITE JIRA_WRITE_PROJECT JIRA_LEDGER_ISSUE; do
  args=()
  [ "$absent" = JIRA_USER_EMAIL ] || args+=("JIRA_USER_EMAIL=bot@example.invalid")
  [ "$absent" = JIRA_API_TOKEN ] || args+=("JIRA_API_TOKEN=not-a-real-token")
  [ "$absent" = JIRA_SITE ] || args+=("JIRA_SITE=$UNREACHABLE")
  [ "$absent" = JIRA_WRITE_PROJECT ] || args+=("JIRA_WRITE_PROJECT=DISPOSABLE")
  [ "$absent" = JIRA_LEDGER_ISSUE ] || args+=("JIRA_LEDGER_ISSUE=DISPOSABLE-1")
  refuses_without "$FILING" "$absent" "${args[@]}"
done

ran "$FILING" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
  JIRA_SITE="$UNREACHABLE" JIRA_WRITE_PROJECT=ISP JIRA_LEDGER_ISSUE=ISP-1 JIRA_ISSUE=ISP-1
[ "$CODE" -ne 0 ] || fail "the filing lane accepted the project its read lane observes as a disposable one"
case "$OUT" in *"is not the project a read lane observes"*) ;; *) fail "the filing lane refused a non-disposable project without saying why: $OUT" ;; esac

[ -f "$FILING_TEST" ] || fail "the filing lane names a test binary this repository does not hold: $FILING_TEST"
CHECKED=$((CHECKED + 1))

NAMED=$(grep -o 'a_ticket_file_verdict_filed_is_found_by_a_later_inspect_against_the_real_site' "$FILING" | head -1)
[ -n "$NAMED" ] || fail "the filing lane must name the case it runs, or --exact selects nothing and cargo reports 0 tests as a pass"
grep -q "async fn $NAMED" "$FILING_TEST" \
  || fail "the filing lane runs \`$NAMED\` and $FILING_TEST declares no such case, so the lane would report 0 tests run as a pass"
CHECKED=$((CHECKED + 1))

grep -q -F '#[ignore' "$FILING_TEST" \
  || fail "the filing lane's case writes to a real site and must be #[ignore]d, or scripts/gate.sh would file a ticket on every run"
CHECKED=$((CHECKED + 1))

FILING_DELETES=$(grep -c '"DELETE"' "$FILING_TEST")
FILING_PROPERTY_DELETES=$(grep '"DELETE"' "$FILING_TEST" | grep -c -E 'claim_route|&probe')
[ "$FILING_DELETES" -gt 0 ] \
  || fail "the filing lane sends no delete at all, so the comparison below counts nothing and would pass for a lane that deleted every issue it filed"
[ "$FILING_DELETES" -eq "$FILING_PROPERTY_DELETES" ] \
  || fail "the filing lane sends $FILING_DELETES deletes and only $FILING_PROPERTY_DELETES of them name a property route. The operator ruled on 2026-08-28 that cleanup is a close and never a delete: ISP refuses a delete by policy, so a lane that deletes an issue leaves residue on every run. Deletes found: $(grep -n '"DELETE"' "$FILING_TEST" | tr '\n' ' ')"
for built in 'fn claim_route' 'let probe = format!'; do
  grep -A2 -F "$built" "$FILING_TEST" | grep -q -F '/properties/' \
    || fail "the filing lane's deletes name \`$built\` and that route is not built from /properties/, so the count above says nothing about what the deletes remove"
done
CHECKED=$((CHECKED + 1))

OBSERVE="$SCRIPT_DIR/live-jira-observe.sh"
FIXTURES=$(mktemp -d "${TMPDIR:-/tmp}/test-live-jira-observe-XXXXXX") || exit 2
trap 'rm -rf "$FIXTURES"' EXIT INT TERM

cat > "$FIXTURES/whole.json" <<'JSON'
{
  "id": "1",
  "key": "IDENT-1",
  "fields": {
    "status": { "id": "3", "name": "In Progress", "statusCategory": { "name": "indeterminate" } },
    "updated": "2026-09-01T12:22:29.347+0100",
    "labels": ["one", "two"],
    "description": { "type": "doc", "version": 1, "content": [ { "type": "paragraph", "content": [ { "type": "text", "text": "a paragraph" } ] } ] },
    "comment": {
      "comments": [ { "id": "10", "author": { "accountId": "acct-1" }, "body": { "type": "doc", "version": 1, "content": [] } } ],
      "total": 1,
      "maxResults": 1,
      "startAt": 0
    }
  }
}
JSON

broken() {
  local name="$1" filter="$2"
  jq "$filter" "$FIXTURES/whole.json" > "$FIXTURES/$name.json" \
    || { fail "the fixture $name could not be built from whole.json, so the case below would compare nothing"; return 1; }
}

observed() {
  OUT=$(bash -c '. "$1"; shift; "$@"' _ "$OBSERVE" "$@" 2>&1)
  CODE=$?
  CHECKED=$((CHECKED + 1))
}

records_all_three() {
  local answer="$1"
  OUT=$(bash -c '. "$1"; record_labels "$2"; record_description "$2"; record_comment "$2"' _ "$OBSERVE" "$answer" 2>&1)
  CODE=$?
  CHECKED=$((CHECKED + 1))
}

refuses_shape() {
  local recorder="$1" answer="$2" named="$3"
  observed "$recorder" "$FIXTURES/$answer.json"
  if [ "$CODE" -eq 0 ]; then
    fail "$recorder read $answer and exited 0, and JiraWorkItemPort refuses that shape, so the lane would report a shape fiddle could not parse"
    return
  fi
  case "$OUT" in
    *"$named"*) ;;
    *) fail "$recorder refused $answer without naming $named, so a reader cannot tell which field the site answered wrongly: $OUT" ;;
  esac
}

records_all_three "$FIXTURES/whole.json"
[ "$CODE" -eq 0 ] || fail "the lane refused a well-formed five-field answer, so the refusals below would pass for a lane that refuses everything: $OUT"
for named in '`fields.labels` is a list of 2' '`fields.description` is a document of' '`fields.comment` is a container of 1 of 1'; do
  case "$OUT" in
    *"$named"*) ;;
    *) fail "the lane read a well-formed answer and its record does not carry \"$named\", so a passing run says nothing about that field: $OUT" ;;
  esac
done

broken labels-document '.fields.labels = { "as": "a document" }'
broken labels-untyped '.fields.labels = ["one", 2]'
broken description-list '.fields.description = [1, 2]'
broken comment-text '.fields.comment = "the conversation arrived as text"'
broken comment-floor '.fields.comment.total = 3'
broken comment-listless 'del(.fields.comment.comments)'
broken comment-totalless 'del(.fields.comment.total)'
broken comment-unnamed 'del(.fields.comment.comments[0].author.accountId)'
broken labels-absent '.fields.labels = null'
broken description-absent '.fields.description = null'
broken comment-absent '.fields.comment = null'

refuses_shape record_labels labels-document '`fields.labels` is a document'
refuses_shape record_labels labels-untyped '`fields.labels` holds 1 of 2 entries that are not text'
refuses_shape record_description description-list '`fields.description` is a list'
refuses_shape record_comment comment-text '`fields.comment` is text'
refuses_shape record_comment comment-floor '`fields.comment` carried 1 of 3 comments'
refuses_shape record_comment comment-listless '`fields.comment` carries no `comments` list'
refuses_shape record_comment comment-totalless '`fields.comment` carries no `total` number'
refuses_shape record_comment comment-unnamed '`fields.comment.comments` holds 1 of 1 comments carrying no `id` or no `author.accountId`'

for absent in labels description comment; do
  observed "record_$absent" "$FIXTURES/$absent-absent.json"
  [ "$CODE" -eq 0 ] || fail "the lane refused an answer whose \`fields.$absent\` is absent, and JiraWorkItemPort reads an absent field as no value rather than a fault: $OUT"
  case "$OUT" in
    *"\`fields.$absent\` is absent"*) ;;
    *) fail "the lane read an absent \`fields.$absent\` and did not record it as absent, so a reader cannot tell absent from unread: $OUT" ;;
  esac
done

cat > "$FIXTURES/unavailable.json" <<'JSON'
{ "observations": { "work_item": { "unavailable": {
  "source": "jira:https://site.invalid/IDENT-1",
  "reason": "https://site.invalid: the site answered with something that is not an issue: `fields.labels` is a document, and a list of labels is what this port reads"
} } } }
JSON
cat > "$FIXTURES/available.json" <<'JSON'
{ "observations": { "work_item": { "available": {
  "source": "jira:https://site.invalid/IDENT-1",
  "revision": "2026-09-01T11:22:29.347Z",
  "value": { "id": "IDENT-1", "status": "In Progress", "labels": ["one", "two"], "description": "a paragraph", "comments": [ { "author": "acct-1", "text": "a reply" } ] }
} } } }
JSON

observed refuse_unless_available "$FIXTURES/available.json"
[ "$CODE" -eq 0 ] || fail "the lane refused an available observation, so the refusal below would pass for a lane that refuses everything: $OUT"

observed refuse_unless_available "$FIXTURES/unavailable.json"
[ "$CODE" -ne 0 ] || fail "the lane read an unavailable work item and exited 0, which is the defect fiddle-xkri exists for: a green lane over an answer the port could not parse"
case "$OUT" in
  *'`fields.labels`'*) ;;
  *) fail "the lane refused an unavailable work item without carrying the port's reason, and the reason is the only thing that names the field: $OUT" ;;
esac

AGREES=$(bash -c '. "$1"; RAW_LABELS=2; RAW_DESCRIPTION=108; RAW_COMMENTS=1; agree_or_refuse "$2"' _ "$OBSERVE" "$FIXTURES/available.json" 2>&1)
CODE=$?
CHECKED=$((CHECKED + 1))
[ "$CODE" -eq 0 ] || fail "the lane refused counts that agree with what fiddle reported, so the disagreements below would pass for a lane that refuses everything: $AGREES"

for pair in 'RAW_LABELS=3:fields.labels' 'RAW_COMMENTS=2:fields.comment' 'RAW_DESCRIPTION=absent:fields.description'; do
  setting="${pair%%:*}"; named="${pair##*:}"
  OUT=$(bash -c '. "$1"; RAW_LABELS=2; RAW_DESCRIPTION=108; RAW_COMMENTS=1; eval "$3"; agree_or_refuse "$2"' _ "$OBSERVE" "$FIXTURES/available.json" "$setting" 2>&1)
  CODE=$?
  CHECKED=$((CHECKED + 1))
  [ "$CODE" -ne 0 ] || fail "the direct read and fiddle disagreed on $named and the lane exited 0, so the two reads were never compared"
  case "$OUT" in
    *"$named"*) ;;
    *) fail "the lane refused a disagreement on $named without naming it: $OUT" ;;
  esac
done

observed fields_the_port_asks_for
[ "$CODE" -eq 0 ] || fail "the lane cannot read the port's field list out of the tree it ships in: $OUT"
for field in labels description comment; do
  case "$OUT" in
    *"$field"*) ;;
    *) fail "the field list the lane read carries no \`$field\`, so the shape it records is not the shape the port asks for: $OUT" ;;
  esac
done

printf 'const FIELDS: &str = "status,updated";\n' > "$FIXTURES/narrow.rs"
ran "$OBSERVE" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
  JIRA_SITE="$UNREACHABLE" JIRA_ISSUE=IDENT-1 FIDDLE_BIN=/bin/echo \
  PORT_SOURCE="$FIXTURES/narrow.rs"
[ "$CODE" -ne 0 ] || fail "the observe lane graded three fields against a port that asks for two and exited 0"
case "$OUT" in
  *"have drifted"*) ;;
  *) fail "the observe lane refused a narrowed field list without saying the lane and the port have drifted: $OUT" ;;
esac

ran "$OBSERVE" JIRA_USER_EMAIL=bot@example.invalid JIRA_API_TOKEN=not-a-real-token \
  JIRA_SITE="$UNREACHABLE" JIRA_ISSUE=IDENT-1 FIDDLE_BIN=/bin/echo
[ "$CODE" -ne 0 ] || fail "the observe lane exited 0 against a site that answers nothing, so it reported a measurement it never took"
case "$OUT" in
  *"have drifted"*)
    fail "the observe lane reported drift while reading the port source this repository ships, so the drift case above would pass for a lane that refuses every field list: $OUT"
    ;;
  *"would not answer a direct read"*) ;;
  *) fail "the observe lane was given every variable and an unreachable site, and said neither: $OUT" ;;
esac

OUT=$(bash -c '. "$1"; PORT_SOURCE="$2"; fields_the_port_asks_for' _ "$OBSERVE" "$FIXTURES/narrow.rs" 2>&1)
CODE=$?
CHECKED=$((CHECKED + 1))
[ "$CODE" -ne 0 ] || fail "the lane graded three fields against a port that asks for two and said nothing; fiddle-xkri exists because that drift went unreported"
case "$OUT" in
  *"have drifted"*) ;;
  *) fail "the lane refused a narrowed field list without saying the lane and the port have drifted: $OUT" ;;
esac

OUT=$(bash -c '. "$1"; PORT_SOURCE="$2"; fields_the_port_asks_for' _ "$OBSERVE" "$FIXTURES/no-such-source.rs" 2>&1)
CODE=$?
CHECKED=$((CHECKED + 1))
[ "$CODE" -ne 0 ] || fail "the lane read a field list out of a file that does not exist"

printf 'test-live-jira-lanes: %d cases run, %d failed\n' "$CHECKED" "$FAILED"
[ "$FAILED" -eq 0 ] || exit 1
printf 'Live jira lane refusal tests passed\n'
