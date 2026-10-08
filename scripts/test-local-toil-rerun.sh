#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$SCRIPT_DIR/local-toil-rerun.sh"

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
TOML
mkdir -p "$DIR/state/changes"

echo "an incomplete invocation is refused"
refuses "no argument at all"
refuses "a config and no reference" --config "$CONFIG"
refuses "a reference and no config" --ref jira:ISP-1
refuses "a config that does not exist" --config "$DIR/absent.toml" --ref jira:ISP-1
refuses "a reference that names no tracker" --config "$CONFIG" --ref beans:w-1

echo "an absent record is reported as the runner's own state"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref jira:ISP-1)
check "it resolves the root relative to where it runs" "record for ISP-1 is $(cd "$DIR" && pwd)/state/changes/ISP-1.json" "$OUT"
check "and names it absolutely rather than relative to the caller" "record for ISP-1 is /" "$OUT"
check "it says the machine already behaves as a runner does" "already behaves as a runner" "$OUT"

echo "a record that exists is reported, and left alone without --forget"
printf '{"marker":"abcd1234"}' > "$DIR/state/changes/ISP-1.json"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref jira:ISP-1)
check "it reads the marker out of the record" "abcd1234" "$OUT"
check "it says what the record does to a run" "reaches no step" "$OUT"
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
check "it names the record under the working directory" "record for ISP-2 is $(cd "$DIR/work" && pwd)/state/changes/ISP-2.json" "$OUT"
check "it reads the marker of the record fiddle would read" "work0001" "$OUT"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/work/state/changes/ISP-2.json" ] \
  && fail "--forget left the record under the working directory, which is the one fiddle reads"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/conf/state/changes/ISP-2.json" ] \
  || fail "--forget removed a record beside the document, which fiddle never reads"
OUT=$(run_from "$DIR/work" --config ../conf/deployment.toml --ref jira:ISP-2)
check "a relative --config is read from the working directory too" "record for ISP-2 is $(cd "$DIR/work" && pwd)/state/changes/ISP-2.json" "$OUT"

echo "a pr: reference names the record <owner>+<repo>+<n>"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7')
check "it names the pull request's record" "state/changes/acme+icecube+7.json" "$OUT"
check "and says it is absent" "already behaves as a runner" "$OUT"
printf '{"marker":"pr000007"}' > "$DIR/state/changes/acme+icecube+7.json"
printf '{"marker":"other"}' > "$DIR/state/changes/acme-icecube-7.json"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --forget)
check "it reads the pull request record's marker" "pr000007" "$OUT"
check "it says it removed the record" "removed" "$OUT"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/state/changes/acme+icecube+7.json" ] \
  && fail "--forget reported a removal and changes/acme+icecube+7.json is still there"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/state/changes/acme-icecube-7.json" ] \
  || fail "--forget removed a record joined with dashes, which names another pull request"
check "it prints the pr: run quoted, so no shell reads # as a comment or a glob" 'fiddle run "pr:acme/icecube#7"' "$OUT"
OUT=$(run_from "$DIR" --config "$CONFIG" --ref 'pr:Acme.io/ice_cube-2#10')
check "a name with a dot, an underscore and a dash is a GitHub name" "state/changes/Acme.io+ice_cube-2+10.json" "$OUT"

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
