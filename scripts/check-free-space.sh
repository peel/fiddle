#!/usr/bin/env bash
set -uo pipefail

DIR="${1:-}"
TARGET="${2:-}"
FLOOR_KB="${FIDDLE_GATE_FLOOR_KB:-8388608}"

refuse() { echo "FREE SPACE: CANNOT RUN  ($*)"; exit 2; }

[ -n "$DIR" ] && [ -d "$DIR" ] || refuse "no directory to measure was named, or it does not exist: '${DIR}'"
case "$FLOOR_KB" in ''|*[!0-9]*) refuse "FIDDLE_GATE_FLOOR_KB is not a number of kilobytes: '$FLOOR_KB'" ;; esac

AVAILABLE_KB=$(df -Pk "$DIR" 2>/dev/null | awk 'NR == 2 { print $4 }')
case "$AVAILABLE_KB" in ''|*[!0-9]*) refuse "df did not report the free space under $DIR" ;; esac

NEEDED_KB="$FLOOR_KB"
WHY="the floor for a tree with no build output"
if [ -n "$TARGET" ] && [ -d "$TARGET" ]; then
  BUILT_KB=$(du -sk "$TARGET" 2>/dev/null | awk '{ print $1 }')
  case "$BUILT_KB" in ''|*[!0-9]*) refuse "du did not report the size of $TARGET" ;; esac
  if [ "$BUILT_KB" -gt "$NEEDED_KB" ]; then
    NEEDED_KB="$BUILT_KB"
    WHY="the size of $TARGET, which a full rebuild rewrites"
  else
    WHY="the floor, which is more than the $BUILT_KB KB $TARGET holds"
  fi
fi

if [ "$AVAILABLE_KB" -lt "$NEEDED_KB" ]; then
  refuse "$AVAILABLE_KB KB free under $DIR, and the build needs $NEEDED_KB KB, $WHY; reclaim the target directories of merged lanes first"
fi
echo "FREE SPACE: $AVAILABLE_KB KB free, $NEEDED_KB KB needed ($WHY)"
