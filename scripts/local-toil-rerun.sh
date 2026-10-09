#!/usr/bin/env bash
set -uo pipefail
LC_ALL=C

CONFIG=""
REFERENCE=""
FORGET=0
FIDDLE_BIN="${FIDDLE:-fiddle}"

usage() {
  cat <<'USAGE'
usage: local-toil-rerun.sh --config <deployment.toml> --ref <reference> [--forget] [--fiddle <path>]

This script decides nothing itself. It runs `fiddle inspect <reference>
--config <document> --json` and prints what fiddle reports about the change
record and its assessment. When fiddle refuses, the script prints fiddle's
error as it is and exits with fiddle's code.

With --forget it removes the change record fiddle named, at the [stub] root
`fiddle config check --config <document> --json` reports, when that record is
a regular file, a symlink or an empty directory. Anything else it leaves, and
exits 2.

The fiddle it runs is --fiddle, else $FIDDLE, else `fiddle` on the PATH. Run
the script from the directory you run fiddle from. For a jira: reference,
fiddle inspect makes a live request to the configured Jira site.

It prints the run for you to type. It does not run it.
USAGE
}

refuse() { echo "local-toil-rerun: $*" >&2; exit 2; }

value_of() { [ "$2" -ge 2 ] || refuse "$1 takes a value, and none follows it"; }

while [ $# -gt 0 ]; do
  case "$1" in
    --config) value_of --config $#; CONFIG="$2"; shift 2 ;;
    --ref) value_of --ref $#; REFERENCE="$2"; shift 2 ;;
    --fiddle) value_of --fiddle $#; FIDDLE_BIN="$2"; shift 2 ;;
    --forget) FORGET=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "local-toil-rerun: unknown argument $1" >&2; usage >&2; exit 2 ;;
  esac
done

[ -n "$CONFIG" ] || refuse "--config is required"
[ -n "$REFERENCE" ] || refuse "--ref is required"
[ -n "$FIDDLE_BIN" ] || refuse "--fiddle names no program"
command -v "$FIDDLE_BIN" >/dev/null 2>&1 \
  || refuse "cannot run fiddle: there is no fiddle at '$FIDDLE_BIN'; pass --fiddle <path> or set FIDDLE"
command -v jq >/dev/null 2>&1 || refuse "jq is not on the PATH, so fiddle's answer cannot be read"

ERRORS=$(mktemp "${TMPDIR:-/tmp}/local-toil-rerun-XXXXXX") || exit 2
trap 'rm -f "$ERRORS"' EXIT

ask() {
  "$FIDDLE_BIN" "$@" 2>"$ERRORS"
}

ANSWER=$(ask inspect "$REFERENCE" --config "$CONFIG" --json)
ASKED=$?
if [ "$ASKED" -ne 0 ]; then
  cat "$ERRORS" >&2
  exit "$ASKED"
fi
printf '%s' "$ANSWER" | jq -e '.schema == "fiddle.inspect.v0"' >/dev/null 2>&1 \
  || refuse "fiddle inspect answered in a shape this script does not read: $ANSWER"

CHANGES=$(printf '%s' "$ANSWER" | jq -c '.observations.changes')
ASSESSMENT=$(printf '%s' "$ANSWER" | jq -c '.assessment')
KIND=$(printf '%s' "$ANSWER" | jq -r '.observations.changes | if type == "object" and length == 1 then keys[0] else "" end')
SOURCE=$(printf '%s' "$ANSWER" | jq -r '.observations.changes | if type == "object" and length == 1 then (to_entries[0].value.source // "") else "" end')
REASON=$(printf '%s' "$ANSWER" | jq -r '.observations.changes.unavailable.reason // ""')

echo "local-toil-rerun: fiddle's change observation: $CHANGES"
echo "local-toil-rerun: fiddle's assessment: $ASSESSMENT"
case "$KIND" in
  available) echo "local-toil-rerun: fiddle reports the change record $SOURCE available" ;;
  unavailable) echo "local-toil-rerun: fiddle reports the change record $SOURCE unavailable: $REASON" ;;
  *) echo "local-toil-rerun: fiddle reports the change record ${KIND:-in no single state}" ;;
esac

if [ "$FORGET" -eq 1 ]; then
  if [ -z "$SOURCE" ]; then
    echo "local-toil-rerun: fiddle named no change record, so --forget removed nothing"
  else
    NAME="${SOURCE#stub:changes/}"
    [ "stub:changes/$NAME" = "$SOURCE" ] && [[ "$NAME" =~ ^[A-Za-z0-9._:+-]+\.json$ ]] \
      || refuse "fiddle named the change record '$SOURCE', which is not stub:changes/<name>.json with a name this script removes; nothing was removed"
    CHECKED=$(ask config check --config "$CONFIG" --json)
    CHECK_EXIT=$?
    if [ "$CHECK_EXIT" -ne 0 ]; then
      cat "$ERRORS" >&2
      refuse "fiddle config check exited $CHECK_EXIT, so the [stub] root is unknown; nothing was removed"
    fi
    ROOT_KIND=$(printf '%s' "$CHECKED" | jq -r 'if (.stub.root | type) != "string" then "none" elif (.stub.root | test("[\u0000-\u001f\u007f-\u009f]")) then "control" elif (.stub.root | startswith("/")) then "absolute" else "relative" end' 2>/dev/null)
    case "$ROOT_KIND" in
      absolute) ;;
      control) refuse "fiddle's [stub] root holds a control character, which this script cannot name safely; nothing was removed" ;;
      relative) refuse "fiddle's [stub] root is not absolute; nothing was removed" ;;
      *) refuse "fiddle config check reported no [stub] root; nothing was removed" ;;
    esac
    ROOT=$(printf '%s' "$CHECKED" | jq -r '.stub.root')
    RECORD="$ROOT/changes/$NAME"
    if [ -L "$RECORD" ] || [ -f "$RECORD" ]; then
      rm -f -- "$RECORD" || refuse "could not remove $RECORD"
    elif [ -d "$RECORD" ] && [ -z "$(ls -A -- "$RECORD" 2>/dev/null)" ] && rmdir -- "$RECORD" 2>/dev/null; then
      :
    else
      refuse "--forget removed nothing: $RECORD is not a regular file, a symlink or an empty directory this script can remove"
    fi
    if [ -e "$RECORD" ] || [ -L "$RECORD" ]; then
      refuse "$RECORD is still there after --forget"
    fi
    echo "local-toil-rerun: removed $RECORD"
  fi
fi

echo
echo "local-toil-rerun: type this yourself, from this directory; this script does not run it"
echo "  FIDDLE_TRANSCRIPT=1 fiddle run \"$REFERENCE\" --config $CONFIG"
