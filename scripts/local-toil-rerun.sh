#!/usr/bin/env bash
set -uo pipefail
LC_ALL=C

CONFIG=""
REFERENCE=""
FORGET=0

usage() {
  cat <<'USAGE'
usage: local-toil-rerun.sh --config <deployment.toml> --ref <jira:KEY | pr:OWNER/REPO#N> [--forget]

A runner starts with an empty filesystem, so it holds no record that this work
completed and it runs the workflow again. A developer's machine keeps that
record, so a second run here reports `completed` and never reaches a step. This
script shows that record and, with --forget, removes it, which is the only
difference between a local rerun and a runner's.

A pr: reference names a pull request, which is not a work item, so a record
fiddle wrote does not stop a run. Each run answers only the mentions not yet
answered. To run a mention again, delete fiddle's answer comment or post a new
mention. A record fiddle cannot read blocks a run over its reference, whatever
the reference, until it is removed, and --forget removes it.

Run it from the directory you run fiddle from. fiddle resolves a relative
[stub] root against its working directory (ADR 092), and so does this script.

It prints the run for you to type. It does not run it: a toil run writes to a
repository and a ticket, and that is yours to authorise.
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    --config) CONFIG="${2:-}"; shift 2 || exit 2 ;;
    --ref) REFERENCE="${2:-}"; shift 2 || exit 2 ;;
    --forget) FORGET=1; shift ;;
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

ROOT=$(awk '
  /^[[:space:]]*\[/ { in_stub = ($0 ~ /^[[:space:]]*\[stub\][[:space:]]*$/) ; next }
  in_stub && /^[[:space:]]*root[[:space:]]*=/ {
    line = $0
    sub(/^[^=]*=[[:space:]]*/, "", line)
    gsub(/^"|"$/, "", line)
    print line
    exit
  }
' "$CONFIG")

[ -n "$ROOT" ] || fail "$CONFIG names no [stub] root, so the completion record cannot be found"

case "$ROOT" in
  /*) ;;
  *) HERE="$(pwd)" || fail "the working directory cannot be read, so the relative root '$ROOT' cannot be resolved"
     ROOT="$HERE/$ROOT" ;;
esac

RECORD="$ROOT/changes/$TICKET.json"
echo "local-toil-rerun: completion record for $TICKET is $RECORD"

read_record() {
  python3 - "$1" <<'PYTHON'
import json
import sys


class Pairs(dict):
    markers = 0


def pairs(items):
    held = Pairs(items)
    held.markers = sum(1 for key, _ in items if key == "marker")
    return held


def constant(name):
    raise ValueError(f"{name} is not JSON")


def depth(value):
    if isinstance(value, dict):
        return 1 + max((depth(item) for item in value.values()), default=0)
    if isinstance(value, list):
        return 1 + max((depth(item) for item in value), default=0)
    return 0


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for key, item in value.items():
            yield key
            yield from strings(item)
    elif isinstance(value, list):
        for item in value:
            yield from strings(item)


def unreadable(reason):
    print(f"unreadable\t{reason}")
    sys.exit(0)


try:
    text = open(sys.argv[1], "rb").read().decode("utf-8")
    value = json.loads(text, parse_constant=constant, object_pairs_hook=pairs)
    for string in strings(value):
        string.encode("utf-8")
except (UnicodeError, ValueError) as error:
    unreadable(f"it is not JSON: {error}")
if depth(value) >= 128:
    unreadable("it nests deeper than fiddle reads")
if isinstance(value, dict):
    if value.markers > 1:
        unreadable("it names marker twice")
    marker = value.get("marker")
elif isinstance(value, list) and len(value) == 1:
    marker = value[0]
else:
    unreadable("it is not an object holding a marker")
if marker is not None and not isinstance(marker, str):
    unreadable("its marker is not a string")
print(f"readable\t{marker or ''}")
PYTHON
}

READABLE=1
if [ -f "$RECORD" ]; then
  command -v python3 >/dev/null 2>&1 \
    || { echo "local-toil-rerun: python3 is not on the PATH, so whether fiddle can read $RECORD cannot be said" >&2; exit 2; }
  VERDICT=$(read_record "$RECORD") || fail "could not read $RECORD"
  case "$VERDICT" in
    readable$'\t'*) MARKER="${VERDICT#readable$'\t'}" ;;
    unreadable$'\t'*) READABLE=0; WHY="${VERDICT#unreadable$'\t'}" ;;
    *) fail "the reading of $RECORD said neither readable nor unreadable: $VERDICT" ;;
  esac
fi

if [ -f "$RECORD" ] && [ "$READABLE" -eq 0 ]; then
  echo "local-toil-rerun: it exists, and $WHY"
  echo "local-toil-rerun: fiddle cannot read this record, so a run over this reference is blocked until it is removed; --forget removes it"
  if [ "$FORGET" -eq 1 ]; then
    rm -f "$RECORD" || fail "could not remove $RECORD"
    echo "local-toil-rerun: removed, so a run over this reference is no longer blocked by it"
  fi
elif [ "$PULL" -eq 1 ]; then
  if [ -f "$RECORD" ]; then
    echo "local-toil-rerun: it exists and carries marker '${MARKER:-none}'"
    echo "local-toil-rerun: fiddle can read this record, and a record fiddle can read does not stop a pr: run; a pull request is not a work item, so a marker over it is not a completion"
  else
    echo "local-toil-rerun: it does not exist, and a record does not stop a pr: run; a pull request is not a work item, so a marker over it is not a completion"
  fi
  echo "local-toil-rerun: each run answers only the mentions not yet answered on the pull request"
  echo "local-toil-rerun: to run a mention again, delete fiddle's answer comment or post a new mention"
  if [ "$FORGET" -eq 1 ] && [ -f "$RECORD" ]; then
    rm -f "$RECORD" || fail "could not remove $RECORD"
    echo "local-toil-rerun: removed; this changes nothing for a pr: run"
  fi
elif [ -f "$RECORD" ]; then
  echo "local-toil-rerun: it exists and carries marker '${MARKER:-none}'"
  echo "local-toil-rerun: while it exists this run reports completed and reaches no step, which a runner never does"
  if [ "$FORGET" -eq 1 ]; then
    rm -f "$RECORD" || fail "could not remove $RECORD"
    echo "local-toil-rerun: removed, so the next run reads the forge exactly as a runner does"
  else
    echo "local-toil-rerun: pass --forget to remove it"
  fi
else
  echo "local-toil-rerun: it does not exist, so this machine already behaves as a runner does"
fi

echo
echo "local-toil-rerun: type this yourself, from this directory; it writes to a repository and a ticket or a pull request"
echo "  FIDDLE_TRANSCRIPT=1 fiddle run \"$REFERENCE\" --config $CONFIG"
echo
echo "local-toil-rerun: the run reads the direction on its own pull request first."
echo "local-toil-rerun: nothing asked for means it completes without paying for the agent."
