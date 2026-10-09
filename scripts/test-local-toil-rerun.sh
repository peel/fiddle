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
[ "$(id -u)" -ne 0 ] || { echo "test-local-toil-rerun: running as root, a directory with mode 000 is still readable, so the permission case would measure nothing; nothing was checked" >&2; exit 2; }
export FIDDLE
export RERUN_JIRA_USER=rerun-suite-user RERUN_JIRA_TOKEN=rerun-suite-token-not-a-secret

FAILED=0
CHECKED=0

fail() { echo "test-local-toil-rerun: FAIL: $*" >&2; FAILED=$((FAILED + 1)); }

check() {
  CHECKED=$((CHECKED + 1))
  case "$3" in
    *"$2"*) ;;
    *) fail "$1 (expected '$2' in: $3)" ;;
  esac
}

lacks() {
  CHECKED=$((CHECKED + 1))
  case "$3" in
    *"$2"*) fail "$1 (did not expect '$2' in: $3)" ;;
  esac
}

holds() {
  CHECKED=$((CHECKED + 1))
  local what="$1"; shift
  "$@" || fail "$what"
}

DIR=$(mktemp -d "${TMPDIR:-/tmp}/local-toil-rerun-XXXXXX") || exit 2
trap 'chmod -R u+rwx "$DIR" 2>/dev/null; rm -rf "$DIR"' EXIT INT TERM

document() {
  cat <<TOML
[project]
name = "p"

[stub]
$1

[report]
dir = "reports"

[jira]
site = "https://127.0.0.1:9"
project = "ISP"
user = { env = "RERUN_JIRA_USER" }
token = { env = "RERUN_JIRA_TOKEN" }
timeout = "1s"
TOML
}

CONFIG="$DIR/deployment.toml"
document 'root = "state"' > "$CONFIG"
mkdir -p "$DIR/state/changes"
PR7="$DIR/state/changes/acme+icecube+7.json"

script_in() {
  local dir="$1"; shift
  OUT=$(cd "$dir" && bash "$SCRIPT" "$@" 2>&1); RC=$?
}

same_as_fiddle() {
  local what="$1" dir="$2" config="$3" reference="$4"
  local answer changes assessment
  answer=$(cd "$dir" && "$FIDDLE" inspect "$reference" --config "$config" --json 2>/dev/null)
  changes=$(printf '%s' "$answer" | jq -c '.observations.changes')
  assessment=$(printf '%s' "$answer" | jq -c '.assessment')
  CHECKED=$((CHECKED + 1))
  [ -n "$changes" ] && [ "$changes" != null ] || fail "$what: fiddle inspect gave no change observation, so the case measures nothing: $answer"
  script_in "$dir" --config "$config" --ref "$reference"
  CHECKED=$((CHECKED + 1))
  [ "$RC" -eq 0 ] || fail "$what: exited $RC: $OUT"
  CHECKED=$((CHECKED + 1))
  printf '%s\n' "$OUT" | grep -qxF "local-toil-rerun: fiddle's change observation: $changes" \
    || fail "$what: did not print fiddle's change observation $changes: $OUT"
  CHECKED=$((CHECKED + 1))
  printf '%s\n' "$OUT" | grep -qxF "local-toil-rerun: fiddle's assessment: $assessment" \
    || fail "$what: did not print fiddle's assessment $assessment: $OUT"
  lacks "$what never says a record does not exist" "does not exist" "$OUT"
  KIND_SAID=$(printf '%s' "$changes" | jq -r 'keys[0]')
}

echo "the script prints what fiddle inspect answers, for every state the record is in"
same_as_fiddle "no record" "$DIR" "$CONFIG" 'pr:acme/icecube#7'
holds "fiddle reports no record as available" [ "$KIND_SAID" = available ]
check "no record is restated from fiddle's answer" "fiddle reports the change record stub:changes/acme+icecube+7.json available" "$OUT"

printf '{"marker":"m0000001"}' > "$PR7"
same_as_fiddle "a record fiddle wrote" "$DIR" "$CONFIG" 'pr:acme/icecube#7'
holds "fiddle reports a record it wrote as available" [ "$KIND_SAID" = available ]
check "the marker is fiddle's" '"marker":"m0000001"' "$OUT"

printf 'garbage' > "$PR7"
same_as_fiddle "garbage" "$DIR" "$CONFIG" 'pr:acme/icecube#7'
holds "fiddle reports garbage as unavailable" [ "$KIND_SAID" = unavailable ]
check "garbage is restated with fiddle's reason" "unavailable: stub source malformed" "$OUT"

printf '{"marker":"m0000001"}' > "$PR7"
chmod 000 "$DIR/state/changes"
same_as_fiddle "a records directory that cannot be read" "$DIR" "$CONFIG" 'pr:acme/icecube#7'
holds "fiddle reports a record under an unreadable directory as unavailable" [ "$KIND_SAID" = unavailable ]
check "and the script restates fiddle's reason" "Permission denied" "$OUT"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --forget
holds "--forget under an unreadable directory exits 2: $OUT" [ "$RC" -eq 2 ]
check "and says it removed nothing" "--forget removed nothing" "$OUT"
chmod 755 "$DIR/state/changes"
holds "--forget under an unreadable directory left the record" [ -f "$PR7" ]

FILEROOT="$DIR/fileroot"
mkdir -p "$FILEROOT"
document 'root = "state"' > "$FILEROOT/deployment.toml"
printf 'a file' > "$FILEROOT/state"
same_as_fiddle "a root that is a file" "$FILEROOT" deployment.toml 'pr:acme/icecube#7'
holds "fiddle reports a record under a file as unavailable" [ "$KIND_SAID" = unavailable ]
check "and the script restates fiddle's reason" "Not a directory" "$OUT"

printf '[1,2]' > "$DIR/state/changes/ISP-3.json"
same_as_fiddle "a jira record fiddle cannot read" "$DIR" "$CONFIG" jira:ISP-3
holds "fiddle reports it unavailable" [ "$KIND_SAID" = unavailable ]

echo "a reference fiddle refuses is refused with fiddle's code and fiddle's words"
refused_as_fiddle() {
  local what="$1" reference="$2" said rc
  said=$(cd "$DIR" && "$FIDDLE" inspect "$reference" --config "$CONFIG" --json 2>&1 >/dev/null); rc=$?
  CHECKED=$((CHECKED + 1))
  [ "$rc" -ne 0 ] || fail "$what: fiddle accepted '$reference', so the case measures nothing"
  script_in "$DIR" --config "$CONFIG" --ref "$reference"
  CHECKED=$((CHECKED + 1))
  [ "$RC" -eq "$rc" ] || fail "$what: exited $RC, and fiddle exits $rc"
  CHECKED=$((CHECKED + 1))
  [ "$OUT" = "$said" ] || fail "$what: printed something other than fiddle's error: $OUT"
}
refused_as_fiddle "a pull request without a repository" 'pr:acme#7'
check "fiddle's refusal names the form" 'write `pr:<owner>/<repo>#<number>`' "$OUT"
refused_as_fiddle "a pr reference with no value" 'pr:'
refused_as_fiddle "a jira value that names a location" 'jira:../x'
refused_as_fiddle "a number with a leading zero" 'pr:acme/icecube#07'

echo "--forget removes only the record fiddle named, and only what it can remove whole"
REAL=$(cd "$DIR" && pwd -P)
printf '{"marker":"m0000001"}' > "$PR7"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --forget
holds "--forget on a record fiddle reads exits 0: $OUT" [ "$RC" -eq 0 ]
check "it names the path it removed under fiddle's root" "removed $REAL/state/changes/acme+icecube+7.json" "$OUT"
holds "the record is gone" [ ! -e "$PR7" ]

printf 'garbage' > "$PR7"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --forget
holds "--forget on garbage exits 0: $OUT" [ "$RC" -eq 0 ]
holds "the garbage is gone" [ ! -e "$PR7" ]

mkdir "$PR7"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --forget
holds "--forget on an empty directory exits 0: $OUT" [ "$RC" -eq 0 ]
holds "the empty directory is gone" [ ! -e "$PR7" ]

mkdir -p "$PR7/inner"
printf 'keep me' > "$PR7/inner/note"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --forget
holds "--forget on a directory that holds files exits 2: $OUT" [ "$RC" -eq 2 ]
check "and says it removed nothing" "--forget removed nothing" "$OUT"
holds "what the directory held is kept" [ -f "$PR7/inner/note" ]
rm -rf "$PR7"

printf '{"marker":"m0000001"}' > "$DIR/target-record"
ln -s "$DIR/target-record" "$PR7"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --forget
holds "--forget on a symlink exits 0: $OUT" [ "$RC" -eq 0 ]
holds "the symlink is gone" [ ! -L "$PR7" ]
holds "the file it pointed at is kept" [ -f "$DIR/target-record" ]

script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --forget
holds "--forget with nothing at the path exits 2: $OUT" [ "$RC" -eq 2 ]
check "and says it removed nothing" "--forget removed nothing" "$OUT"

echo "a relative root is the one fiddle resolves from the working directory"
mkdir -p "$DIR/conf/state/changes" "$DIR/work/state/changes"
cp "$CONFIG" "$DIR/conf/deployment.toml"
printf '{"marker":"work0001"}' > "$DIR/work/state/changes/ISP-2.json"
printf '{"marker":"conf0001"}' > "$DIR/conf/state/changes/ISP-2.json"
same_as_fiddle "a record under the working directory" "$DIR/work" ../conf/deployment.toml jira:ISP-2
check "fiddle read the record under the working directory" '"marker":"work0001"' "$OUT"
script_in "$DIR/work" --config ../conf/deployment.toml --ref jira:ISP-2 --forget
holds "--forget removed the record fiddle reads" [ ! -e "$DIR/work/state/changes/ISP-2.json" ]
holds "and left the one beside the document" [ -f "$DIR/conf/state/changes/ISP-2.json" ]

echo "a root holding a control character is refused by --forget"
controlled() {
  local what="$1" line="$2" here="$DIR/controlled-$3" pattern="$4"
  mkdir -p "$here"
  document "$line" > "$here/deployment.toml"
  local reported
  reported=$(cd "$here" && "$FIDDLE" config check --config deployment.toml --json 2>/dev/null | jq -r --arg p "$pattern" '.stub.root | test($p)')
  CHECKED=$((CHECKED + 1))
  [ "$reported" = true ] || fail "$what: fiddle does not report the root with that character, so the case measures nothing"
  mkdir -p "$here/st/changes"
  printf '{"marker":"decoy001"}' > "$here/st/changes/acme+icecube+7.json"
  script_in "$here" --config deployment.toml --ref 'pr:acme/icecube#7' --forget
  holds "$what: exited $RC, not 2: $OUT" [ "$RC" -eq 2 ]
  check "$what: says why" "holds a control character" "$OUT"
  holds "$what: the record under the stripped root is kept" [ -f "$here/st/changes/acme+icecube+7.json" ]
}
controlled "a root ending in a newline" 'root = "st\n"' newline '\n$'
controlled "a root holding U+0085" 'root = "st\u0085"' c1 "$(printf '\302\205')"
controlled "a root holding DEL" 'root = "st\u007f"' del "$(printf '\177')"

echo "when fiddle cannot be run or answers in another shape, nothing is judged"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --fiddle "$DIR/no-such-fiddle"
holds "a missing fiddle exits 2: $OUT" [ "$RC" -eq 2 ]
check "and says fiddle cannot be run" "cannot run fiddle" "$OUT"
OUT=$(cd "$DIR" && env FIDDLE="$DIR/no-such-fiddle" bash "$SCRIPT" --config "$CONFIG" --ref 'pr:acme/icecube#7' 2>&1); RC=$?
holds "a missing fiddle named by FIDDLE exits 2: $OUT" [ "$RC" -eq 2 ]
printf '#!/bin/sh\necho "fiddle failed in its own words" >&2\nexit 3\n' > "$DIR/failing-fiddle"
chmod +x "$DIR/failing-fiddle"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --fiddle "$DIR/failing-fiddle"
holds "a failing fiddle's code is the script's: $RC" [ "$RC" -eq 3 ]
holds "and its words are printed as they are: $OUT" [ "$OUT" = "fiddle failed in its own words" ]
printf '#!/bin/sh\necho "{}"\n' > "$DIR/shapeless-fiddle"
chmod +x "$DIR/shapeless-fiddle"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --fiddle "$DIR/shapeless-fiddle"
holds "an answer in another shape exits 2: $OUT" [ "$RC" -eq 2 ]
lacks "and nothing is restated from it" "fiddle reports the change record" "$OUT"
printf '#!/bin/sh\nif [ "$1" = config ]; then echo "no root for you" >&2; exit 4; fi\nexec "%s" "$@"\n' "$FIDDLE" > "$DIR/rootless-fiddle"
chmod +x "$DIR/rootless-fiddle"
printf '{"marker":"m0000001"}' > "$PR7"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7' --fiddle "$DIR/rootless-fiddle" --forget
holds "--forget when config check fails exits 2: $OUT" [ "$RC" -eq 2 ]
check "and prints fiddle's words" "no root for you" "$OUT"
holds "and removes nothing" [ -f "$PR7" ]

echo "an incomplete invocation is refused with a reason"
refuses() {
  local what="$1" reason="$2"; shift 2
  OUT=$(cd "$DIR" && bash "$SCRIPT" "$@" 2>&1); RC=$?
  holds "$what exits 2: $RC $OUT" [ "$RC" -eq 2 ]
  check "$what says why" "$reason" "$OUT"
}
refuses "no argument" "--config is required"
refuses "no reference" "--ref is required" --config "$CONFIG"
refuses "--config with no value" "--config takes a value" --ref 'pr:acme/icecube#7' --config
refuses "--ref with no value" "--ref takes a value" --config "$CONFIG" --ref
refuses "--fiddle with no value" "--fiddle takes a value" --config "$CONFIG" --ref 'pr:acme/icecube#7' --fiddle

echo "it prints the run and does not run it"
script_in "$DIR" --config "$CONFIG" --ref 'pr:acme/icecube#7'
check "it prints the run with the reference quoted" 'fiddle run "pr:acme/icecube#7" --config' "$OUT"
CHECKED=$((CHECKED + 1))
grep -nE '^[^#]*"\$FIDDLE_BIN"[[:space:]]+run\b|^[[:space:]]*(FIDDLE_TRANSCRIPT=1 )?fiddle run' "$SCRIPT" \
  && fail "the script runs fiddle run itself"

if [ "$FAILED" -ne 0 ]; then
  echo "test-local-toil-rerun: FAIL ($FAILED of $CHECKED checks)" >&2
  exit 1
fi
echo "test-local-toil-rerun: PASS ($CHECKED checks)"
