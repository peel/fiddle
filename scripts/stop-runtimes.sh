#!/usr/bin/env bash
set -euo pipefail

STATE_FILE=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --state)
      [[ $# -ge 2 ]] || { echo '{"error":"--state requires a value"}' >&2; exit 2; }
      STATE_FILE="$2"; shift 2;;
    *) echo '{"error":"unknown arg: '"$1"'"}' >&2; exit 2;;
  esac
done

[[ -n "$STATE_FILE" ]] || { echo '{"error":"missing --state"}' >&2; exit 2; }
[[ -f "$STATE_FILE" ]] || { echo '{"error":"state file not found: '"$STATE_FILE"'"}' >&2; exit 2; }

if ! jq empty "$STATE_FILE" 2>/dev/null; then
  echo '{"error":"invalid JSON in state file"}' >&2
  exit 2
fi

PIDS=$(jq -r '.[].pid' "$STATE_FILE")

if [[ -z "$PIDS" ]]; then
  exit 0
fi

for pid in $PIDS; do
  if ! [[ "$pid" =~ ^[1-9][0-9]*$ ]]; then
    echo '{"error":"invalid PID: '"$pid"'"}' >&2
    exit 2
  fi
done

for pid in $PIDS; do
  if kill -0 "$pid" 2>/dev/null; then
    kill -TERM "$pid" 2>/dev/null || true
  fi
done

for pid in $PIDS; do
  DEADLINE=$(( $(date +%s) + 10 ))
  while kill -0 "$pid" 2>/dev/null; do
    if [[ $(date +%s) -ge $DEADLINE ]]; then
      kill -9 "$pid" 2>/dev/null || true
      break
    fi
    sleep 0.2
  done
done

exit 0
