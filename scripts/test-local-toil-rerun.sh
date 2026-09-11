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
OUT=$(bash "$SCRIPT" --config "$CONFIG" --ref jira:ISP-1 2>&1)
check "it resolves the root relative to the document" "state/changes/ISP-1.json" "$OUT"
check "and names it absolutely rather than relative to the caller" "record for ISP-1 is /" "$OUT"
check "it says the machine already behaves as a runner does" "already behaves as a runner" "$OUT"

echo "a record that exists is reported, and left alone without --forget"
printf '{"marker":"abcd1234"}' > "$DIR/state/changes/ISP-1.json"
OUT=$(bash "$SCRIPT" --config "$CONFIG" --ref jira:ISP-1 2>&1)
check "it reads the marker out of the record" "abcd1234" "$OUT"
check "it says what the record does to a run" "reaches no step" "$OUT"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/state/changes/ISP-1.json" ] \
  || fail "the record was removed without --forget, so a read-only look is not read-only"

echo "--forget removes it"
OUT=$(bash "$SCRIPT" --config "$CONFIG" --ref jira:ISP-1 --forget 2>&1)
check "it says it removed the record" "removed" "$OUT"
CHECKED=$((CHECKED + 1))
[ -f "$DIR/state/changes/ISP-1.json" ] \
  && fail "--forget reported a removal and the record is still there"

echo "it prints the run and does not run it"
OUT=$(bash "$SCRIPT" --config "$CONFIG" --ref jira:ISP-1 2>&1)
check "it prints the command to type" "fiddle run jira:ISP-1" "$OUT"
CHECKED=$((CHECKED + 1))
grep -qE '^[[:space:]]*(FIDDLE_TRANSCRIPT=1 )?fiddle run' "$SCRIPT" \
  && fail "the script runs fiddle itself, and a toil run writes to a repository and a ticket"

if [ "$FAILED" -ne 0 ]; then
  echo "test-local-toil-rerun: FAIL ($FAILED of $CHECKED checks)" >&2
  exit 1
fi
echo "test-local-toil-rerun: PASS ($CHECKED checks)"
