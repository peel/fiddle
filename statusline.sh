#!/bin/bash
# Claude Code statusline
#  personal/work ·  repo/branch ·   model · ███░░ 12:34 · ███░░ Wed 1:34
input=$(cat)

# ── colors ──
RST='\033[0m'
BOLD='\033[1m'
DIM='\033[2m'
RED='\033[31m'
GREEN='\033[32m'
YELLOW='\033[33m'
GRAY='\033[90m'
SEP="${DIM}${GRAY} · ${RST}"

# ── icons ──
ICON_USER=""
ICON_GIT=""
ICON_MODEL=""

# ── helpers ──
progress_bar() {
  local pct=${1:-0} width=${2:-5}
  local filled=$(( pct * width / 100 ))
  [ "$filled" -gt "$width" ] && filled=$width
  local empty=$(( width - filled ))
  local bar="" i
  for ((i=0; i<filled; i++)); do bar="${bar}█"; done
  for ((i=0; i<empty; i++)); do bar="${bar}░"; done
  printf '%s' "$bar"
}

# Use /bin/date explicitly for BSD date (macOS) - avoids nix/GNU coreutils shadow
DATE=/bin/date

format_reset_5h() {
  local iso="$1"
  [ -z "$iso" ] && return
  local clean
  clean=$(printf '%s' "$iso" | sed 's/\.[0-9]*//' | sed 's/[+-][0-9][0-9]:[0-9][0-9]$//' | sed 's/Z$//')
  local epoch
  epoch=$(TZ=UTC $DATE -j -f "%Y-%m-%dT%H:%M:%S" "$clean" "+%s" 2>/dev/null)
  [ -z "$epoch" ] && return
  $DATE -j -f "%s" "$epoch" "+%-H:%M" 2>/dev/null
}

format_reset_7d() {
  local iso="$1"
  [ -z "$iso" ] && return
  local clean
  clean=$(printf '%s' "$iso" | sed 's/\.[0-9]*//' | sed 's/[+-][0-9][0-9]:[0-9][0-9]$//' | sed 's/Z$//')
  local epoch
  epoch=$(TZ=UTC $DATE -j -f "%Y-%m-%dT%H:%M:%S" "$clean" "+%s" 2>/dev/null)
  [ -z "$epoch" ] && return
  $DATE -j -f "%s" "$epoch" "+%a %-H:%M" 2>/dev/null
}

# ── 1. account ──
config_dir="${CLAUDE_CONFIG_DIR:-$HOME/.claude}"
# Resolve symlinks so account detection works with symlinked config dirs
[ -L "$config_dir" ] && config_dir=$(readlink "$config_dir")
if printf '%s' "$config_dir" | grep -qi "personal"; then
  account="personal"
  acct_color="$RED"
elif printf '%s' "$config_dir" | grep -qi "work"; then
  account="work"
  acct_color="$GREEN"
else
  account=$(basename "$config_dir")
  acct_color="$GRAY"
fi

# ── 2. repo/branch ──
dir=$(printf '%s' "$input" | jq -r '.workspace.current_dir // .cwd // ""')
repo_name=$(basename "$dir")
branch=""
if git -C "$dir" rev-parse --git-dir > /dev/null 2>&1; then
  branch=$(git -C "$dir" symbolic-ref --short HEAD 2>/dev/null \
    || git -C "$dir" rev-parse --short HEAD 2>/dev/null)
fi

# ── 3. model ──
model=$(printf '%s' "$input" | jq -r '.model.display_name // ""')
model_lower=$(printf '%s' "$model" | tr '[:upper:]' '[:lower:]')
if printf '%s' "$model_lower" | grep -q "opus"; then
  model_color="$RED"
elif printf '%s' "$model_lower" | grep -q "sonnet"; then
  model_color="$YELLOW"
elif printf '%s' "$model_lower" | grep -q "haiku"; then
  model_color="$GREEN"
else
  model_color="$GRAY"
fi

# ── 4 & 5. usage from cache ──
CACHE_FILE="/tmp/.claude_usage_cache"
five_h="" seven_d="" five_h_reset="" seven_d_reset=""
if [ -f "$CACHE_FILE" ]; then
  five_h=$(sed -n '1p' "$CACHE_FILE")
  seven_d=$(sed -n '2p' "$CACHE_FILE")
  five_h_reset=$(sed -n '3p' "$CACHE_FILE")
  seven_d_reset=$(sed -n '4p' "$CACHE_FILE")
else
  bash ~/.claude-base/fetch-usage.sh > /dev/null 2>&1 &
fi

# ── render ──

# section 1: account
printf '%b%s %s%b' "$acct_color" "$ICON_USER" "$account" "$RST"

# section 2: repo/branch
printf '%b' "$SEP"
printf '%s' "$ICON_GIT"
if [ -n "$branch" ]; then
  printf ' %s/%s' "$repo_name" "$branch"
else
  printf ' %s' "$repo_name"
fi

# section 3: model
printf '%b' "$SEP"
printf '%b%s %s%b' "$model_color" "$ICON_MODEL" "$model" "$RST"

# section 4: 5h window
if [ -n "$five_h" ]; then
  bar=$(progress_bar "$five_h" 5)
  reset=$(format_reset_5h "$five_h_reset")
  printf '%b' "$SEP"
  if [ "$five_h" -gt 80 ] 2>/dev/null; then
    printf '%b%s %s%b' "$RED" "$bar" "${reset:-?}" "$RST"
  else
    printf '%s %s' "$bar" "${reset:-?}"
  fi
fi

# section 5: 7d window
if [ -n "$seven_d" ]; then
  bar=$(progress_bar "$seven_d" 5)
  reset=$(format_reset_7d "$seven_d_reset")
  printf '%b' "$SEP"
  if [ "$seven_d" -gt 80 ] 2>/dev/null; then
    printf '%b%s %s%b' "$RED" "$bar" "${reset:-?}" "$RST"
  else
    printf '%s %s' "$bar" "${reset:-?}"
  fi
fi
