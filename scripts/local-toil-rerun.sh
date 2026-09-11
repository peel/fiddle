#!/usr/bin/env bash
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2

CONFIG=""
REFERENCE=""
FORGET=0

usage() {
  cat <<'USAGE'
usage: local-toil-rerun.sh --config <deployment.toml> --ref <jira:KEY> [--forget]

A runner starts with an empty filesystem, so it holds no record that this work
completed and it runs the workflow again. A developer's machine keeps that
record, so a second run here reports `completed` and never reaches a step. This
script shows that record and, with --forget, removes it, which is the only
difference between a local rerun and a runner's.

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
[ -n "$REFERENCE" ] || fail "--ref is required, for example jira:ISP-263"

case "$REFERENCE" in
  jira:*) TICKET="${REFERENCE#jira:}" ;;
  *) fail "--ref must name a jira ticket, and this one is '$REFERENCE'" ;;
esac
[ -n "$TICKET" ] || fail "'$REFERENCE' names no ticket"

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
  *) ROOT="$(cd "$(dirname "$CONFIG")" && pwd)/$ROOT" ;;
esac

RECORD="$ROOT/changes/$TICKET.json"
echo "local-toil-rerun: completion record for $TICKET is $RECORD"

if [ -f "$RECORD" ]; then
  MARKER=$(sed -n 's/.*"marker"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$RECORD" | head -1)
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
echo "local-toil-rerun: type this yourself; it writes to a repository and a ticket"
echo "  FIDDLE_TRANSCRIPT=1 fiddle run $REFERENCE --config $CONFIG"
echo
echo "local-toil-rerun: the run reads the direction on its own pull request first."
echo "local-toil-rerun: nothing asked for means it completes without paying for the agent."
