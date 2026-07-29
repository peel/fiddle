#!/usr/bin/env bash
# start-runtimes.sh — Start runtime processes for evaluation with ready-check polling.
# Exit 0 = started and ready, 1 = app failed to start, 2 = invalid input, 3 = harness failure.
set -euo pipefail

DOMAINS_FILE=""
SLOT_INDEX=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --domains) DOMAINS_FILE="$2"; shift 2;;
    --slot-index) SLOT_INDEX="$2"; shift 2;;
    *) echo "Unknown arg: $1" >&2; exit 2;;
  esac
done

[[ -n "$DOMAINS_FILE" ]] || { echo '{"error":"missing --domains"}' >&2; exit 2; }
[[ -f "$DOMAINS_FILE" ]] || { echo '{"error":"domains file not found: '"$DOMAINS_FILE"'"}' >&2; exit 2; }

# Check dependencies
command -v jq >/dev/null 2>&1 || { echo '{"error":"jq not found"}' >&2; exit 3; }
command -v python3 >/dev/null 2>&1 || { echo '{"error":"python3 not found"}' >&2; exit 3; }
command -v curl >/dev/null 2>&1 || { echo '{"error":"curl not found"}' >&2; exit 3; }
command -v bc >/dev/null 2>&1 || { echo '{"error":"bc not found"}' >&2; exit 3; }

now_ms() {
  python3 -c 'import time; print(int(time.time() * 1000))'
}

# Validate JSON
if ! jq empty "$DOMAINS_FILE" 2>/dev/null; then
  echo '{"error":"invalid JSON in domains file"}' >&2
  exit 2
fi

# Build ordered list of domain names
# If runtime_order is present, use it; otherwise use key order from .domains
RUNTIME_ORDER=$(jq -r '
  if .runtime_order then
    .runtime_order[]
  else
    .domains | keys[]
  end
' "$DOMAINS_FILE")

RESULTS="[]"

cleanup_on_failure() {
  for pid in $(echo "$RESULTS" | jq -r '.[].pid' 2>/dev/null); do
    kill "$pid" 2>/dev/null || true
  done
}

poll_tcp() {
  local port="$1" timeout_ms="$2" retry_ms="${3:-1000}"
  local deadline=$(( $(now_ms) + timeout_ms ))
  while true; do
    if python3 -c "import socket; s=socket.socket(); s.settimeout(1); s.connect(('localhost', $port)); s.close()" 2>/dev/null; then
      return 0
    fi
    local now
    now=$(now_ms)
    if [[ $now -ge $deadline ]]; then
      return 1
    fi
    local remaining=$(( deadline - now ))
    local sleep_ms=$retry_ms
    if [[ $sleep_ms -gt $remaining ]]; then
      sleep_ms=$remaining
    fi
    sleep "$(echo "scale=3; $sleep_ms / 1000" | bc)"
  done
}

poll_http() {
  local url="$1" timeout_ms="$2" retry_ms="${3:-1000}" expect_status="${4:-200}"
  local deadline=$(( $(now_ms) + timeout_ms ))
  while true; do
    local status
    status=$(curl -s -o /dev/null -w "%{http_code}" --connect-timeout 1 --max-time 2 "$url" 2>/dev/null || echo "0")
    if [[ "$status" = "$expect_status" ]] || [[ "$expect_status" = "200" && "$status" =~ ^[23] ]]; then
      return 0
    fi
    local now
    now=$(now_ms)
    if [[ $now -ge $deadline ]]; then
      return 1
    fi
    local remaining=$(( deadline - now ))
    local sleep_ms=$retry_ms
    if [[ $sleep_ms -gt $remaining ]]; then
      sleep_ms=$remaining
    fi
    sleep "$(echo "scale=3; $sleep_ms / 1000" | bc)"
  done
}

poll_command() {
  local cmd="$1" timeout_ms="$2" retry_ms="${3:-1000}"
  local deadline=$(( $(now_ms) + timeout_ms ))
  while true; do
    if bash -c "$cmd" 2>/dev/null; then
      return 0
    fi
    local now
    now=$(now_ms)
    if [[ $now -ge $deadline ]]; then
      return 1
    fi
    local remaining=$(( deadline - now ))
    local sleep_ms=$retry_ms
    if [[ $sleep_ms -gt $remaining ]]; then
      sleep_ms=$remaining
    fi
    sleep "$(echo "scale=3; $sleep_ms / 1000" | bc)"
  done
}

while IFS= read -r DOMAIN; do
  # Check if this domain has a runtime configured
  HAS_RUNTIME=$(jq -r --arg d "$DOMAIN" '.domains[$d].runtime // empty | length > 0' "$DOMAINS_FILE" 2>/dev/null || echo "false")
  if [[ "$HAS_RUNTIME" != "true" ]]; then
    continue
  fi

  # Get runtime command for this slot
  RUNTIME_CMD=$(jq -r --arg d "$DOMAIN" --argjson idx "$SLOT_INDEX" \
    '.domains[$d].runtime[$idx] // .domains[$d].runtime[0]' "$DOMAINS_FILE")

  if [[ -z "$RUNTIME_CMD" || "$RUNTIME_CMD" = "null" ]]; then
    echo '{"error":"no runtime command for domain '"$DOMAIN"' at slot '"$SLOT_INDEX"'"}' >&2
    exit 2
  fi

  # Get ready_check config
  CHECK_TYPE=$(jq -r --arg d "$DOMAIN" '.domains[$d].ready_check.type // "tcp"' "$DOMAINS_FILE")
  TIMEOUT_MS=$(jq -r --arg d "$DOMAIN" '.domains[$d].ready_check.timeout_ms // 15000' "$DOMAINS_FILE")
  RETRY_MS=$(jq -r --arg d "$DOMAIN" '.domains[$d].ready_check.retry_interval_ms // 1000' "$DOMAINS_FILE")

  # Start the runtime in background
  # Use a temp file to capture the PID reliably
  TMPLOG=$(mktemp)
  bash -c "$RUNTIME_CMD" >"$TMPLOG" 2>&1 &
  RUNTIME_PID=$!

  # Brief wait to detect immediate crashes
  sleep 0.3
  if ! kill -0 "$RUNTIME_PID" 2>/dev/null; then
    rm -f "$TMPLOG"
    cleanup_on_failure
    echo '{"error":"runtime process for domain '"$DOMAIN"' exited immediately"}' >&2
    exit 1
  fi

  # Extract port from ready_check or command for output
  RUNTIME_PORT=0

  # Poll readiness
  READY=false
  case "$CHECK_TYPE" in
    tcp)
      CHECK_PORT=$(jq -r --arg d "$DOMAIN" '.domains[$d].ready_check.port // 8080' "$DOMAINS_FILE")
      RUNTIME_PORT=$CHECK_PORT
      if poll_tcp "$CHECK_PORT" "$TIMEOUT_MS" "$RETRY_MS"; then
        READY=true
      fi
      ;;
    http)
      CHECK_URL=$(jq -r --arg d "$DOMAIN" '.domains[$d].ready_check.url // "http://localhost:8080"' "$DOMAINS_FILE")
      EXPECT_STATUS=$(jq -r --arg d "$DOMAIN" '.domains[$d].ready_check.expect_status // 200' "$DOMAINS_FILE")
      # Extract port from URL for output
      RUNTIME_PORT=$(echo "$CHECK_URL" | python3 -c "
import sys, re
url = sys.stdin.read().strip()
m = re.search(r':(\d+)', url)
print(m.group(1) if m else '8080')
" 2>/dev/null || echo "8080")
      if poll_http "$CHECK_URL" "$TIMEOUT_MS" "$RETRY_MS" "$EXPECT_STATUS"; then
        READY=true
      fi
      ;;
    command)
      CHECK_CMD=$(jq -r --arg d "$DOMAIN" '.domains[$d].ready_check.command' "$DOMAINS_FILE")
      if poll_command "$CHECK_CMD" "$TIMEOUT_MS" "$RETRY_MS"; then
        READY=true
      fi
      ;;
    *)
      echo '{"error":"unknown ready_check type: '"$CHECK_TYPE"'"}' >&2
      kill "$RUNTIME_PID" 2>/dev/null || true
      rm -f "$TMPLOG"
      exit 2
      ;;
  esac

  rm -f "$TMPLOG"

  if [[ "$READY" != "true" ]]; then
    # Check if process died (app failure) vs timeout (could be either)
    if ! kill -0 "$RUNTIME_PID" 2>/dev/null; then
      cleanup_on_failure
      echo '{"error":"runtime for domain '"$DOMAIN"' crashed during startup"}' >&2
      exit 1
    else
      # Process still running but never became ready — treat as app failure
      kill "$RUNTIME_PID" 2>/dev/null || true
      cleanup_on_failure
      echo '{"error":"runtime for domain '"$DOMAIN"' timed out waiting for ready_check"}' >&2
      exit 1
    fi
  fi

  # Append to results
  RESULTS=$(echo "$RESULTS" | jq --arg domain "$DOMAIN" \
    --argjson pid "$RUNTIME_PID" \
    --argjson port "$RUNTIME_PORT" \
    --arg command "$RUNTIME_CMD" \
    '. + [{"domain": $domain, "pid": $pid, "port": $port, "command": $command}]')

done <<< "$RUNTIME_ORDER"

echo "$RESULTS"
exit 0
