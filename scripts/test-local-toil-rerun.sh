#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$SCRIPT_DIR/local-toil-rerun.sh"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
FIDDLE="${TEST_FIDDLE:-$ROOT/target/debug/fiddle}"

[ -x "$FIDDLE" ] || { echo "test-local-toil-rerun: no fiddle binary at $FIDDLE; build it with cargo build --bin fiddle (the gate's cargo test builds it before the shell suites run), and nothing was checked" >&2; exit 2; }
STALE=$(find "$ROOT/crates" -name '*.rs' -newer "$FIDDLE" -print -quit)
[ -z "$STALE" ] || { echo "test-local-toil-rerun: $FIDDLE is older than $STALE, so its answers may not be this tree's; rebuild it, and nothing was checked" >&2; exit 2; }
command -v jq >/dev/null 2>&1 || { echo "test-local-toil-rerun: jq is not on the PATH, and the script reads fiddle's answer with it; nothing was checked" >&2; exit 2; }
export FIDDLE
export RERUN_JIRA_USER=rerun-suite-user RERUN_JIRA_TOKEN=rerun-suite-token-not-a-secret

FAILED=0
CHECKED=0

fail() { echo "test-local-toil-rerun: FAIL: $*" >&2; FAILED=$((FAILED + 1)); }

check() {
  CHECKED=$((CHECKED + 1))
  local what="$1" expected="$2" haystack="$3"
  case "$haystack" in
    *"$expected"*) ;;
    *) fail "$what (expected '$expected' in: $haystack)" ;;
  esac
}

lacks() {
  CHECKED=$((CHECKED + 1))
  local what="$1" unexpected="$2" haystack="$3"
  case "$haystack" in
    *"$unexpected"*) fail "$what (did not expect '$unexpected' in: $haystack)" ;;
  esac
}

refuses() {
  CHECKED=$((CHECKED + 1))
  local what="$1"; shift
  local out rc
  out=$(bash "$SCRIPT" "$@" 2>&1); rc=$?
  [ "$rc" -ne 0 ] || fail "$what: exited 0 when it should refuse: $out"
  case "$out" in *FAIL*) ;; *) fail "$what: refused without saying why: $out" ;; esac
}

refuses_saying() {
  CHECKED=$((CHECKED + 1))
  local what="$1" reason="$2"; shift 2
  local out rc
  out=$(bash "$SCRIPT" "$@" 2>&1); rc=$?
  [ "$rc" -ne 0 ] || fail "$what: exited 0 when it should refuse: $out"
  case "$out" in
    *FAIL*"$reason"*) ;;
    *) fail "$what: refused without the reason '$reason': $out" ;;
  esac
}

run_from() {
  local dir="$1"; shift
  (cd "$dir" && bash "$SCRIPT" "$@" 2>&1)
}

DIR=$(mktemp -d "${TMPDIR:-/tmp}/local-toil-rerun-XXXXXX") || exit 2
trap 'rm -rf "$DIR"' EXIT INT TERM

CONFIG="$DIR/deployment.toml"
cat > "$CONFIG" <<'TOML'
[project]
name = "p"

[stub]
root = "state"

[report]
dir = "reports"

[jira]
site = "https://127.0.0.1:9"
project = "ISP"
user = { env = "RERUN_JIRA_USER" }
token = { env = "RERUN_JIRA_TOKEN" }
timeout = "1s"
TOML
mkdir -p "$DIR/state/changes"

fiddle_says() {
  local dir="$1" reference="$2"
  (cd "$dir" && "$FIDDLE" inspect "$reference" --config "$CONFIG" --json 2>/dev/null) \
    | jq -r '.observations.changes | keys[0]'
}

echo "an incomplete invocation is refused"
refuses "no argument at all"
refuses "a config and no reference" --config "$CONFIG"
refuses "a reference and no config" --ref jira:ISP-1
refuses "a config that does not exist" --config "$DIR/absent.toml" --ref jira:ISP-1
refuses "a reference that names no tracker" --config "$CONFIG" --ref beans:w-1

echo "an absent record is reported as the runner's own state"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref jira:ISP-1)
check "it resolves the root relative to where it runs" "record for ISP-1 is $(cd "$DIR" && pwd -P)/state/changes/ISP-1.json" "$OUT"
check "and names it absolutely rather than relative to the caller" "record for ISP-1 is /" "$OUT"
check "it says the machine already behaves as a runner does" "already behaves as a runner" "$OUT"

echo "a record that exists is reported, and left alone without --forget"
printf '{"marker":"abcd1234"}' > "$DIR/state/changes/ISP-1.json"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref jira:ISP-1)
check "it reports the marker fiddle read" "fiddle reads it, and it carries marker 'abcd1234'" "$OUT"
check "it reports fiddle's assessment, which is blocked here because the ticket cannot be read" "fiddle reports a run over this reference blocked: source jira:https://127.0.0.1:9/ISP-1 unavailable" "$OUT"
lacks "it claims no completion fiddle did not report" "reaches no step" "$OUT"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/state/changes/ISP-1.json" ] \
  || fail "the record was removed without --forget, so a read-only look is not read-only"

echo "--forget removes it"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref jira:ISP-1 --forget)
check "it says it removed the record" "removed" "$OUT"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/state/changes/ISP-1.json" ] \
  && fail "--forget reported a removal and the record is still there"

echo "a relative root is resolved against the working directory, as fiddle resolves it (ADR 092)"
mkdir -p "$DIR/conf" "$DIR/work/state/changes" || exit 2
cp "$CONFIG" "$DIR/conf/deployment.toml" || exit 2
printf '{"marker":"work0001"}' > "$DIR/work/state/changes/ISP-2.json"
mkdir -p "$DIR/conf/state/changes" || exit 2
printf '{"marker":"conf0001"}' > "$DIR/conf/state/changes/ISP-2.json"
OUT=$(run_from "$DIR/work" --config "$DIR/conf/deployment.toml" --ref jira:ISP-2 --forget)
check "it names the record under the working directory" "record for ISP-2 is $(cd "$DIR/work" && pwd -P)/state/changes/ISP-2.json" "$OUT"
check "fiddle reads the record under the working directory" "marker 'work0001'" "$OUT"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/work/state/changes/ISP-2.json" ] \
  && fail "--forget left the record under the working directory, which is the one fiddle reads"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/conf/state/changes/ISP-2.json" ] \
  || fail "--forget removed a record beside the document, which fiddle never reads"
OUT=$(run_from "$DIR/work" --config ../conf/deployment.toml --ref jira:ISP-2)
check "a relative --config is read from the working directory too" "record for ISP-2 is $(cd "$DIR/work" && pwd -P)/state/changes/ISP-2.json" "$OUT"

echo "a pr: reference names the record <owner>+<repo>+<n>"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7')
check "it names the pull request's record" "state/changes/acme+icecube+7.json" "$OUT"
check "it says a record does not stop a pr: run" "does not stop a pr: run" "$OUT"
printf '{"marker":"pr000007"}' > "$DIR/state/changes/acme+icecube+7.json"
printf '{"marker":"other"}' > "$DIR/state/changes/acme-icecube-7.json"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7')
check "fiddle reads the pull request record's marker" "fiddle reads it, and it carries marker 'pr000007'" "$OUT"
check "and assesses the run not started" "fiddle's assessment is not_started" "$OUT"
check "a record fiddle reads does not stop a pr: run" "a record fiddle reads does not stop a pr: run" "$OUT"
check "fiddle answers only the mentions not yet answered" "answers only the mentions not yet answered" "$OUT"
check "it says how to run a mention again" "delete fiddle's answer comment or post a new mention" "$OUT"
lacks "it never claims a pr: run stops at the record" "reaches no step" "$OUT"
lacks "it never claims a pr: run reports completed" "reports completed" "$OUT"
lacks "it never offers --forget as the way to run a pr: mention again" "pass --forget" "$OUT"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/state/changes/acme+icecube+7.json" ] \
  || fail "the record was removed without --forget"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --forget)
check "it says it removed the record" "removed" "$OUT"
check "and that the removal changes nothing for a pr: run" "changes nothing for a pr: run" "$OUT"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/state/changes/acme+icecube+7.json" ] \
  && fail "--forget reported a removal and changes/acme+icecube+7.json is still there"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/state/changes/acme-icecube-7.json" ] \
  || fail "--forget removed a record joined with dashes, which names another pull request"
check "it prints the pr: run quoted, so no shell reads # as a comment or a glob" 'fiddle run "pr:acme/icecube#7"' "$OUT"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#18446744073709551615')
check "the largest number a pull request can carry is accepted" "state/changes/acme+icecube+18446744073709551615.json" "$OUT"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:Acme.io/ice_cube-2#10')
check "a name with a dot, an underscore and a dash is a GitHub name" "state/changes/Acme.io+ice_cube-2+10.json" "$OUT"

echo "fiddle decides whether it can read a record, and the script reports what fiddle says"
UNREADABLE="fiddle reports this record unreadable, so a run over this reference is blocked until it is removed; --forget removes it"
PR9="$DIR/state/changes/acme+icecube+9.json"
fixture() {
  local what="$1" verdict="$2" body="$3"
  rm -rf "$PR9"
  printf '%s' "$body" > "$PR9"
  judged "$what" "$verdict"
}
judged() {
  local what="$1" verdict="$2"
  CHECKED=$((CHECKED + 1))
  local said
  said=$(fiddle_says "$DIR" 'pr:acme/icecube#9')
  [ "$said" = "$verdict" ] || fail "$what: the fixture expects fiddle to report $verdict, and fiddle reports '${said:-nothing}'"
  OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#9')
  case "$verdict" in
    available)
      check "$what is reported readable" "fiddle reads it" "$OUT"
      check "$what does not stop a pr: run" "a record fiddle reads does not stop a pr: run" "$OUT"
      lacks "$what is not reported unreadable" "unreadable" "$OUT"
      ;;
    unavailable)
      check "$what is reported unreadable and blocking" "$UNREADABLE" "$OUT"
      check "$what is reported with fiddle's own reason" "fiddle says: stub source" "$OUT"
      lacks "$what is not said to leave a pr: run alone" "does not stop a pr: run" "$OUT"
      ;;
  esac
}
DEEP=$(printf '{"marker":"a","other":'; printf '[%.0s' $(seq 200); printf ']%.0s' $(seq 200); printf '}')
fixture "a record fiddle wrote" available '{"marker":"0123456789abcdef"}'
fixture "garbage" unavailable 'not json'
fixture "an unknown field holding a lone surrogate" available '{"marker":"a","other":"\ud800"}'
fixture "an unknown field nested 200 deep" available "$DEEP"
fixture "an object without a marker" available '{}'
fixture "a marker that is a number" unavailable '{"marker":7}'
fixture "a marker named twice" unavailable '{"marker":"a","marker":"b"}'
fixture "a NaN" unavailable '{"marker":NaN}'
fixture "two JSON values" unavailable '{"marker":"a"} {"marker":"b"}'
fixture "a lone surrogate in the marker" unavailable '{"marker":"\ud800"}'
rm -rf "$PR9"
mkdir -p "$PR9"
judged "a directory at the record's path" unavailable
check "a directory is reported with fiddle's reason" "stub source unreadable" "$OUT"
CHECKED=$((CHECKED + 1))
[ -d "$PR9" ] || fail "an unreadable record was removed without --forget"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#9' --forget)
check "--forget removes what fiddle cannot read and says so" "no longer blocked by it" "$OUT"
CHECKED=$((CHECKED + 1))
[ -e "$PR9" ] && fail "--forget left the unreadable record in place"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#9')
check "with nothing at the path, the record is absent" "it does not exist" "$OUT"
printf '[1,2]' > "$DIR/state/changes/ISP-3.json"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref jira:ISP-3)
check "a record fiddle cannot read is reported for a jira reference too" "$UNREADABLE" "$OUT"
lacks "and is not said to complete the work" "reaches no step" "$OUT"

echo "when fiddle cannot be asked, nothing is judged"
WHERE="cannot ask fiddle where its records are; nothing was judged"
WHETHER="cannot ask fiddle whether it can read this record; nothing was judged"
asks_nothing() {
  CHECKED=$((CHECKED + 1))
  local what="$1" said="$2"; shift 2
  local out rc
  out=$(cd "$DIR" && "$@" 2>&1); rc=$?
  [ "$rc" -eq 2 ] || fail "$what: exited $rc, not 2: $out"
  case "$out" in
    *"$said"*) ;;
    *) fail "$what: did not say '$said': $out" ;;
  esac
  case "$out" in *"fiddle reads it"*|*"does not stop"*|*"unreadable, so"*|*"does not exist"*) fail "$what: judged the record anyway: $out" ;; esac
}
printf '{"marker":"a"}' > "$PR9"
asks_nothing "a fiddle that does not exist, named by --fiddle" "$WHERE" bash "$SCRIPT" --config "$CONFIG" --ref 'pr:acme/icecube#9' --fiddle "$DIR/no-such-fiddle"
asks_nothing "a fiddle that does not exist, named by FIDDLE" "$WHERE" env FIDDLE="$DIR/no-such-fiddle" bash "$SCRIPT" --config "$CONFIG" --ref 'pr:acme/icecube#9'
asks_nothing "no record, and no fiddle to say where records are" "$WHERE" env FIDDLE="$DIR/no-such-fiddle" bash "$SCRIPT" --config "$CONFIG" --ref 'pr:acme/icecube#10'
printf '#!/bin/sh\n"%s" "$@"\necho "and then it failed" >&2\nexit 3\n' "$FIDDLE" > "$DIR/failing-fiddle"
chmod +x "$DIR/failing-fiddle"
asks_nothing "a config check that answers and then exits non-zero" "$WHERE" bash "$SCRIPT" --config "$CONFIG" --ref 'pr:acme/icecube#9' --fiddle "$DIR/failing-fiddle"
printf '#!/bin/sh\necho "{}"\n' > "$DIR/silent-fiddle"
chmod +x "$DIR/silent-fiddle"
asks_nothing "a config check that names no root" "$WHERE" bash "$SCRIPT" --config "$CONFIG" --ref 'pr:acme/icecube#9' --fiddle "$DIR/silent-fiddle"
printf '#!/bin/sh\nif [ "$1" = inspect ]; then "%s" "$@"; echo "and then it failed" >&2; exit 3; fi\nexec "%s" "$@"\n' "$FIDDLE" "$FIDDLE" > "$DIR/failing-inspect"
chmod +x "$DIR/failing-inspect"
asks_nothing "an inspect that answers and then exits non-zero" "$WHETHER" bash "$SCRIPT" --config "$CONFIG" --ref 'pr:acme/icecube#9' --fiddle "$DIR/failing-inspect"
printf '#!/bin/sh\nif [ "$1" = inspect ]; then echo "{}"; exit 0; fi\nexec "%s" "$@"\n' "$FIDDLE" > "$DIR/silent-inspect"
chmod +x "$DIR/silent-inspect"
asks_nothing "an inspect that answers in another shape" "$WHETHER" bash "$SCRIPT" --config "$CONFIG" --ref 'pr:acme/icecube#9' --fiddle "$DIR/silent-inspect"
rm -f "$PR9"

echo "the root is where fiddle says it is, however the document spells it"
spelled() {
  local what="$1" line="$2"
  local here="$DIR/spelled-$3"
  mkdir -p "$here/state/changes" || exit 2
  sed "s|^root = .*|$line|" "$CONFIG" > "$here/deployment.toml"
  CHECKED=$((CHECKED + 1))
  grep -qxF "$line" "$here/deployment.toml" || fail "$what: the fixture document does not hold $line"
  local record="$here/state/changes/acme+icecube+11.json"
  printf '{"marker":"lit00001"}' > "$record"
  local said
  said=$(cd "$here" && "$FIDDLE" inspect 'pr:acme/icecube#11' --config deployment.toml --json 2>/dev/null | jq -r '.observations.changes | keys[0]')
  CHECKED=$((CHECKED + 1))
  [ "$said" = available ] || fail "$what: fiddle reports '$said' for a record it wrote"
  OUT=$(run_from "$here" --config deployment.toml --ref 'pr:acme/icecube#11')
  check "$what names the record fiddle reads" "record for acme+icecube+11 is $(cd "$here" && pwd -P)/state/changes/acme+icecube+11.json" "$OUT"
  check "$what reports the record fiddle reads" "fiddle reads it, and it carries marker 'lit00001'" "$OUT"
  printf 'garbage' > "$record"
  said=$(cd "$here" && "$FIDDLE" inspect 'pr:acme/icecube#11' --config deployment.toml --json 2>/dev/null | jq -r '.observations.changes | keys[0]')
  CHECKED=$((CHECKED + 1))
  [ "$said" = unavailable ] || fail "$what: fiddle reports '$said' for garbage"
  OUT=$(run_from "$here" --config deployment.toml --ref 'pr:acme/icecube#11' --forget)
  check "$what reports a record fiddle cannot read" "$UNREADABLE" "$OUT"
  CHECKED=$((CHECKED + 1))
  [ -e "$record" ] && fail "$what: --forget left the record fiddle cannot read"
}
spelled "a literal string root" "root = 'state'" literal
spelled "a root followed by a comment" 'root = "state" # c' commented

echo "--forget removes a file, a symlink or an empty directory, and nothing that holds files"
rm -rf "$PR9"
mkdir -p "$PR9/inner"
printf 'keep me' > "$PR9/inner/note"
CHECKED=$((CHECKED + 1))
[ "$(fiddle_says "$DIR" 'pr:acme/icecube#9')" = unavailable ] || fail "fiddle does not report a directory that holds files unreadable"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#9' --forget); RC=$?
CHECKED=$((CHECKED + 1))
[ "$RC" -eq 2 ] || fail "--forget over a directory that holds files exited $RC, not 2: $OUT"
check "--forget says why it leaves a directory that holds files" "a directory that holds files" "$OUT"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#9')
check "without --forget it says --forget will not remove a directory that holds files" "it is a directory that holds files, so --forget will not remove it" "$OUT"
lacks "and does not promise that --forget removes it" "; --forget removes it" "$OUT"
CHECKED=$((CHECKED + 1))
[ -f "$PR9/inner/note" ] || fail "--forget removed what a directory held"
rm -rf "$PR9"
mkdir -p "$PR9"
CHECKED=$((CHECKED + 1))
[ "$(fiddle_says "$DIR" 'pr:acme/icecube#9')" = unavailable ] || fail "fiddle does not report an empty directory unreadable"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#9' --forget)
check "--forget removes an empty directory fiddle cannot read" "no longer blocked by it" "$OUT"
CHECKED=$((CHECKED + 1))
[ -e "$PR9" ] && fail "--forget left the empty directory"
printf '{"marker":"a"}' > "$DIR/target-record"
ln -s "$DIR/target-record" "$PR9"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#9' --forget)
check "--forget removes a symlink fiddle reads through" "removed" "$OUT"
CHECKED=$((CHECKED + 1))
{ [ -e "$PR9" ] || [ -L "$PR9" ]; } && fail "--forget left the symlink"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/target-record" ] || fail "--forget removed the file a symlink pointed at"

echo "a malformed pr: reference is refused with the form it should take"
FORM='pr:<owner>/<repo>#<number>'
refuses_saying "a pull request without a repository" "$FORM" --config "$CONFIG" --ref 'pr:acme#7'
refuses_saying "a pull request without a number" "$FORM" --config "$CONFIG" --ref 'pr:acme/icecube'
refuses_saying "an empty number" "$FORM" --config "$CONFIG" --ref 'pr:acme/icecube#'
refuses_saying "a number with a leading zero" "$FORM" --config "$CONFIG" --ref 'pr:acme/icecube#07'
refuses_saying "a zero" "$FORM" --config "$CONFIG" --ref 'pr:acme/icecube#0'
refuses_saying "a number that is not decimal" "$FORM" --config "$CONFIG" --ref 'pr:acme/icecube#7a'
refuses_saying "an empty owner" "$FORM" --config "$CONFIG" --ref 'pr:/icecube#7'
refuses_saying "an empty repository" "$FORM" --config "$CONFIG" --ref 'pr:acme/#7'
refuses_saying "a repository holding a slash" "$FORM" --config "$CONFIG" --ref 'pr:acme/ice/cube#7'
refuses_saying "a name holding a space" "$FORM" --config "$CONFIG" --ref 'pr:acme/ice cube#7'
refuses_saying "a name holding a plus" "$FORM" --config "$CONFIG" --ref 'pr:acme/ice+cube#7'
refuses_saying "a second number" "$FORM" --config "$CONFIG" --ref 'pr:acme/icecube#7#8'
refuses_saying "a name holding a letter outside ASCII" "$FORM" --config "$CONFIG" --ref 'pr:acmé/icecube#7'
refuses_saying "a number of digits outside ASCII" "$FORM" --config "$CONFIG" --ref 'pr:acme/icecube#７'
refuses_saying "a reference that runs onto a second line" "$FORM" --config "$CONFIG" --ref $'pr:acme/icecube#7\nx'
refuses_saying "a number above the largest a pull request number can be" "$FORM" --config "$CONFIG" --ref 'pr:acme/icecube#18446744073709551616'
refuses_saying "a number of twenty-one digits" "$FORM" --config "$CONFIG" --ref 'pr:acme/icecube#100000000000000000000'
refuses_saying "a pr: reference with no value" "$FORM" --config "$CONFIG" --ref 'pr:'
refuses_saying "it quotes the reference it refused" "'pr:acme#7'" --config "$CONFIG" --ref 'pr:acme#7'

echo "it prints the run and does not run it"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref jira:ISP-1)
check "it prints the command to type" 'fiddle run "jira:ISP-1"' "$OUT"
CHECKED=$((CHECKED + 1))
grep -qE '^[[:space:]]*(FIDDLE_TRANSCRIPT=1 )?fiddle run' "$SCRIPT" \
  && fail "the script runs fiddle itself, and a toil run writes to a repository and a ticket"

if [ "$FAILED" -ne 0 ]; then
  echo "test-local-toil-rerun: FAIL ($FAILED of $CHECKED checks)" >&2
  exit 1
fi
echo "test-local-toil-rerun: PASS ($CHECKED checks)"
