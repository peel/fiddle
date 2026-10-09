#!/usr/bin/env bash
set -uo pipefail
LC_ALL=C

CONFIG=""
REFERENCE=""
FORGET=0
FIDDLE_BIN="${FIDDLE:-fiddle}"

usage() {
  cat <<'USAGE'
usage: local-toil-rerun.sh --config <deployment.toml> --ref <jira:KEY | pr:OWNER/REPO#N> [--forget] [--fiddle <path>]

A runner starts with an empty filesystem, so it holds no record of earlier
work. A developer's machine keeps the change record a run wrote under the
[stub] root. This script asks fiddle where that root is, with `fiddle config
check --config <document> --json`, and names the record under it. When
anything is at the record's path, it asks fiddle what it makes of it, with
`fiddle inspect <ref> --config <document> --json`, and reports fiddle's answer.
With --forget it removes a record that is a file, a symlink or an empty
directory, and refuses a directory that holds anything.

The fiddle it asks is --fiddle, else $FIDDLE, else `fiddle` on the PATH. When
fiddle cannot answer either question, the script judges nothing and exits 2.
For a jira: reference, fiddle inspect makes a live request to the configured
Jira site.

A pr: reference names a pull request, which is not a work item, so a record
fiddle reads does not stop a run. Each run answers only the mentions not yet
answered. To run a mention again, delete fiddle's answer comment or post a new
mention. A record fiddle reports unreadable blocks a run over its reference
until it is removed, and --forget removes it.

Run it from the directory you run fiddle from. fiddle resolves a relative
[stub] root against its working directory (ADR 092), and this script asks
fiddle from the same directory.

It prints the run for you to type. It does not run it: a toil run writes to a
repository and a ticket, and that is yours to authorise.
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --config) CONFIG="${2:-}"; shift 2 || exit 2 ;;
    --ref) REFERENCE="${2:-}"; shift 2 || exit 2 ;;
    --forget) FORGET=1; shift ;;
    --fiddle) FIDDLE_BIN="${2:-}"; shift 2 || exit 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "local-toil-rerun: unknown argument $1" >&2; usage >&2; exit 2 ;;
  esac
done

fail() { echo "local-toil-rerun: FAIL: $*" >&2; exit 1; }

[ -n "$CONFIG" ] || fail "--config is required, and the completion record lives under the [stub] root that document names"
[ -f "$CONFIG" ] || fail "$CONFIG does not exist"
[ -n "$REFERENCE" ] || fail "--ref is required, for example jira:ISP-263 or pr:acme/icecube#7"

PULL=0
PR_FORM='^pr:([A-Za-z0-9._-]+)/([A-Za-z0-9._-]+)#([1-9][0-9]*)$'

case "$REFERENCE" in
  jira:*)
    TICKET="${REFERENCE#jira:}"
    [ -n "$TICKET" ] || fail "'$REFERENCE' names no ticket"
    [[ "$TICKET" =~ ^[A-Za-z0-9_:-]+$ ]] \
      || fail "'$REFERENCE' is not a jira reference fiddle accepts; a value is written with ASCII letters, digits, '-', '_' and ':' only"
    ;;
  pr:*)
    [[ "$REFERENCE" =~ $PR_FORM ]] \
      || fail "'$REFERENCE' does not name a pull request; write pr:<owner>/<repo>#<number>, where owner and repo hold ASCII letters, digits, '-', '_' and '.', and the number has no leading zero"
    NUMBER="${BASH_REMATCH[3]}"
    TICKET="${BASH_REMATCH[1]}+${BASH_REMATCH[2]}+$NUMBER"
    LARGEST=18446744073709551615
    if [ "${#NUMBER}" -gt "${#LARGEST}" ] || { [ "${#NUMBER}" -eq "${#LARGEST}" ] && [ "$(printf '%s\n%s\n' "$NUMBER" "$LARGEST" | sort | tail -1)" != "$LARGEST" ]; }; then
      fail "'$REFERENCE' does not name a pull request; write pr:<owner>/<repo>#<number>, and the number is at most $LARGEST"
    fi
    PULL=1
    ;;
  *) fail "--ref must name a jira ticket or a pull request, and this one is '$REFERENCE'" ;;
esac

cannot_locate() {
  echo "local-toil-rerun: cannot ask fiddle where its records are; nothing was judged: $*" >&2
  exit 2
}

[ -n "$FIDDLE_BIN" ] || cannot_locate "--fiddle names no program"
command -v "$FIDDLE_BIN" >/dev/null 2>&1 \
  || cannot_locate "there is no fiddle at '$FIDDLE_BIN'; pass --fiddle <path> or set FIDDLE"
command -v jq >/dev/null 2>&1 || cannot_locate "jq is not on the PATH, so fiddle's answer cannot be read"
CHECK_ERRORS=$(mktemp "${TMPDIR:-/tmp}/local-toil-rerun-XXXXXX") || exit 2
CHECKED_DOCUMENT=$("$FIDDLE_BIN" config check --config "$CONFIG" --json 2>"$CHECK_ERRORS")
CHECK_EXIT=$?
CHECK_SAID=$(head -c 2000 "$CHECK_ERRORS")
rm -f "$CHECK_ERRORS"
[ "$CHECK_EXIT" -eq 0 ] || cannot_locate "fiddle config check exited $CHECK_EXIT: $CHECK_SAID"
ROOT=$(printf '%s' "$CHECKED_DOCUMENT" | jq -r 'if (.stub.root | type) == "string" then .stub.root else empty end' 2>/dev/null)
[ -n "$ROOT" ] || cannot_locate "fiddle config check reported no [stub] root: $CHECKED_DOCUMENT"
case "$ROOT" in
  /*) ;;
  *) cannot_locate "fiddle config check reported the root '$ROOT', which is not absolute" ;;
esac

RECORD="$ROOT/changes/$TICKET.json"
echo "local-toil-rerun: completion record for $TICKET is $RECORD"

cannot_ask() {
  echo "local-toil-rerun: cannot ask fiddle whether it can read this record; nothing was judged: $*" >&2
  exit 2
}

EXISTS=0
{ [ -e "$RECORD" ] || [ -L "$RECORD" ]; } && EXISTS=1

if [ "$EXISTS" -eq 1 ]; then
  ERRORS=$(mktemp "${TMPDIR:-/tmp}/local-toil-rerun-XXXXXX") || exit 2
  ANSWER=$("$FIDDLE_BIN" inspect "$REFERENCE" --config "$CONFIG" --json 2>"$ERRORS")
  ASKED=$?
  SAID=$(head -c 2000 "$ERRORS")
  rm -f "$ERRORS"
  [ "$ASKED" -eq 0 ] || cannot_ask "fiddle inspect exited $ASKED: $SAID"
  field() { printf '%s' "$ANSWER" | jq -r "$1" 2>/dev/null; }
  [ "$(field '.schema')" = "fiddle.inspect.v0" ] || cannot_ask "fiddle inspect did not answer with fiddle.inspect.v0: $ANSWER"
  CHANGES=$(field '.observations.changes | keys | if length == 1 then .[0] else "" end')
  SOURCE=$(field '.observations.changes[]?.source // empty')
  [ "$SOURCE" = "stub:changes/$TICKET.json" ] \
    || cannot_ask "fiddle read the change record '${SOURCE:-none}', and this script names stub:changes/$TICKET.json"
  ASSESSMENT=$(field '.assessment | keys | if length == 1 then .[0] else "" end')
  BLOCKED_BY=$(field '.assessment.blocked.reason // empty')
  case "$CHANGES" in
    available) MARKER=$(field '.observations.changes.available.value.marker // empty') ;;
    unavailable) WHY=$(field '.observations.changes.unavailable.reason // empty') ;;
    *) cannot_ask "fiddle reported the change record as '${CHANGES:-nothing}'" ;;
  esac
  [ -n "$ASSESSMENT" ] || cannot_ask "fiddle reported no assessment"
fi

forget() {
  if [ -d "$RECORD" ] && [ ! -L "$RECORD" ]; then
    if [ -n "$(ls -A -- "$RECORD")" ]; then
      echo "local-toil-rerun: $RECORD is a directory that holds files, and --forget removes only a file, a symlink or an empty directory; decide what to do with what it holds, then remove it yourself" >&2
      exit 2
    fi
    rmdir -- "$RECORD" || fail "could not remove $RECORD"
  else
    rm -f -- "$RECORD" || fail "could not remove $RECORD"
  fi
  [ ! -e "$RECORD" ] && [ ! -L "$RECORD" ] || fail "$RECORD is still there after --forget"
}

if [ "$EXISTS" -eq 0 ]; then
  if [ "$PULL" -eq 1 ]; then
    echo "local-toil-rerun: it does not exist, and a record does not stop a pr: run; a pull request is not a work item, so a marker over it is not a completion"
    echo "local-toil-rerun: each run answers only the mentions not yet answered on the pull request"
    echo "local-toil-rerun: to run a mention again, delete fiddle's answer comment or post a new mention"
  else
    echo "local-toil-rerun: it does not exist, so this machine already behaves as a runner does"
  fi
elif [ "$CHANGES" = "unavailable" ]; then
  echo "local-toil-rerun: fiddle reports this record unreadable, so a run over this reference is blocked until it is removed; --forget removes it"
  echo "local-toil-rerun: fiddle says: $WHY"
  if [ "$FORGET" -eq 1 ]; then
    forget
    echo "local-toil-rerun: removed, so a run over this reference is no longer blocked by it"
  fi
else
  echo "local-toil-rerun: fiddle reads it, and it carries marker '${MARKER:-none}'"
  echo "local-toil-rerun: fiddle's assessment is $ASSESSMENT"
  if [ "$PULL" -eq 1 ] && [ "$ASSESSMENT" = "not_started" ]; then
    echo "local-toil-rerun: a record fiddle reads does not stop a pr: run; a pull request is not a work item, so a marker over it is not a completion"
    echo "local-toil-rerun: each run answers only the mentions not yet answered on the pull request"
    echo "local-toil-rerun: to run a mention again, delete fiddle's answer comment or post a new mention"
    if [ "$FORGET" -eq 1 ]; then
      forget
      echo "local-toil-rerun: removed; this changes nothing for a pr: run"
    fi
  else
    case "$ASSESSMENT" in
      satisfied) echo "local-toil-rerun: fiddle reports the work completed, so while the record exists a run reaches no step, which a runner never does" ;;
      blocked) echo "local-toil-rerun: fiddle reports a run over this reference blocked: $BLOCKED_BY" ;;
      not_started) echo "local-toil-rerun: fiddle reports the work not started, so the record does not stop a run" ;;
      *) echo "local-toil-rerun: fiddle reports $ASSESSMENT" ;;
    esac
    if [ "$FORGET" -eq 1 ]; then
      forget
      echo "local-toil-rerun: removed, so the next run reads the forge exactly as a runner does"
    else
      echo "local-toil-rerun: pass --forget to remove it"
    fi
  fi
fi

echo
echo "local-toil-rerun: type this yourself, from this directory; it writes to a repository and a ticket or a pull request"
echo "  FIDDLE_TRANSCRIPT=1 fiddle run \"$REFERENCE\" --config $CONFIG"
echo
echo "local-toil-rerun: the run reads the direction on its own pull request first."
echo "local-toil-rerun: nothing asked for means it completes without paying for the agent."
