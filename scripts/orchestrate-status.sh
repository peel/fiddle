#!/usr/bin/env bash
set -euo pipefail

EPIC_ID="${1:?Usage: orchestrate-status.sh <epic-id>}"
EVENT_LOG=".claude/orchestrate-events.log"
POLL_INTERVAL=5
EVENT_TAIL_LINES=8

# Terminal colors
RESET="\033[0m"
BOLD="\033[1m"
DIM="\033[2m"
GREEN="\033[32m"
YELLOW="\033[33m"
RED="\033[31m"
CYAN="\033[36m"
WHITE="\033[37m"

get_phase() {
  if [[ -f "$EVENT_LOG" ]]; then
    grep "^PHASE:" "$EVENT_LOG" | tail -1 | cut -d: -f2
  else
    echo "SETUP"
  fi
}

get_symbol() {
  local status="$1"
  local tags="$2"
  local blocked_by="$3"

  if echo "$tags" | grep -q "needs-attention"; then
    printf "${RED}!${RESET}"
    return
  fi

  case "$status" in
    completed)
      printf "${GREEN}✓${RESET}"
      ;;
    in-progress)
      if echo "$tags" | grep -q "role:review"; then
        printf "${CYAN}◐${RESET}"
      else
        printf "${YELLOW}●${RESET}"
      fi
      ;;
    todo)
      if [[ -n "$blocked_by" && "$blocked_by" != "null" && "$blocked_by" != "[]" ]]; then
        printf "${DIM}◌${RESET}"
      else
        printf "${WHITE}○${RESET}"
      fi
      ;;
    *)
      printf "${DIM}?${RESET}"
      ;;
  esac
}

get_state_label() {
  local status="$1"
  local tags="$2"
  local blocked_by="$3"

  if echo "$tags" | grep -q "needs-attention"; then
    echo "attention"
    return
  fi

  case "$status" in
    completed) echo "done" ;;
    in-progress)
      if echo "$tags" | grep -q "role:review"; then
        echo "review"
      else
        echo "impl"
      fi
      ;;
    todo)
      if [[ -n "$blocked_by" && "$blocked_by" != "null" && "$blocked_by" != "[]" ]]; then
        echo "blocked"
      else
        echo "todo"
      fi
      ;;
    *) echo "$status" ;;
  esac
}

progress_bar() {
  local done="$1"
  local total="$2"
  local width=20

  if [[ "$total" -eq 0 ]]; then
    printf "0/0  %s  0%%" "$(printf '░%.0s' $(seq 1 $width))"
    return
  fi

  local pct=$(( done * 100 / total ))
  local filled=$(( done * width / total ))
  local empty=$(( width - filled ))

  printf "%d/%d  " "$done" "$total"
  printf "${GREEN}"
  for ((i=0; i<filled; i++)); do printf "█"; done
  printf "${DIM}"
  for ((i=0; i<empty; i++)); do printf "░"; done
  printf "${RESET}"
  printf "  %d%%" "$pct"
}

render() {
  clear

  local epic_title
  epic_title=$(beans show "$EPIC_ID" --json 2>/dev/null | jq -r '.title // "unknown"')
  local phase
  phase=$(get_phase)

  # Header
  printf "${BOLD}epic: %s — %s" "$EPIC_ID" "$epic_title"
  printf "%*s${YELLOW}● %s${RESET}\n" 4 "" "$phase"
  echo ""

  # Bean list
  local beans_json
  beans_json=$(beans list --parent "$EPIC_ID" --json 2>/dev/null || echo "[]")
  local count
  count=$(echo "$beans_json" | jq 'length')
  local completed=0

  if [[ "$count" -eq 0 ]]; then
    printf "${DIM}  No beans yet${RESET}\n"
  else
    for i in $(seq 0 $((count - 1))); do
      local bean
      bean=$(echo "$beans_json" | jq ".[$i]")
      local id status title tags blocked_by
      id=$(echo "$bean" | jq -r '.id')
      status=$(echo "$bean" | jq -r '.status')
      title=$(echo "$bean" | jq -r '.title')
      tags=$(echo "$bean" | jq -r '(.tags // []) | join(",")')
      blocked_by=$(echo "$bean" | jq -r '.blocked_by // [] | join(",")')

      if [[ "$status" == "completed" ]]; then
        completed=$((completed + 1))
      fi

      local short_id="${id:(-4)}"
      local short_title="${title:0:40}"
      local symbol state_label
      symbol=$(get_symbol "$status" "$tags" "$blocked_by")
      state_label=$(get_state_label "$status" "$tags" "$blocked_by")

      printf "  %b %-5s %-42s %s\n" "$symbol" "$short_id" "$short_title" "$state_label"
    done
  fi

  echo ""

  # Progress bar
  printf "  "
  progress_bar "$completed" "$count"
  echo ""
  echo ""

  # Event log tail
  if [[ -f "$EVENT_LOG" ]]; then
    grep -v "^PHASE:" "$EVENT_LOG" | tail -n "$EVENT_TAIL_LINES" | while IFS= read -r line; do
      printf "  ${DIM}%s${RESET}\n" "$line"
    done
  fi
}

# Main loop
while true; do
  render
  sleep "$POLL_INTERVAL"
done
