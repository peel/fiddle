#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONFIG_DIR="${MAKI_CONFIG_HOME:-${XDG_CONFIG_HOME:-$HOME/.config}/maki}"
INIT="$CONFIG_DIR/init.lua"
START="-- BEGIN FIDDLE MANAGED BLOCK"
END="-- END FIDDLE MANAGED BLOCK"
REPLACE_UNMANAGED=false

if [[ "${1:-}" == "--replace-unmanaged" ]]; then
  REPLACE_UNMANAGED=true
  shift
fi
if [[ $# -gt 0 ]]; then
  printf 'unknown argument: %s\n' "$1" >&2
  exit 2
fi

mkdir -p "$CONFIG_DIR"
touch "$INIT"

if grep -q 'name = "/fiddle:" .. name' "$INIT" && ! grep -qF -- "$START" "$INIT"; then
  if [[ "$REPLACE_UNMANAGED" == true ]]; then
    cp "$INIT" "$INIT.pre-fiddle-installer.bak"
    : > "$INIT"
  else
    printf '%s\n' "unmanaged Fiddle commands already exist in $INIT" >&2
    printf '%s\n' "rerun with --replace-unmanaged to back up and replace the existing init.lua" >&2
    exit 2
  fi
fi

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT

awk -v start="$START" -v end="$END" '
  $0 == start { managed = 1; next }
  $0 == end { managed = 0; next }
  !managed { lines[++count] = $0 }
  END {
    while (count > 0 && lines[count] == "") count--
    for (i = 1; i <= count; i++) print lines[i]
  }
' "$INIT" > "$TMP"

if [[ -s "$TMP" ]]; then
  printf '\n' >> "$TMP"
fi

lua_string() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  printf '"%s"' "$value"
}

{
  cat "$TMP"
  printf '%s\n' "$START"
  printf 'local fiddle_root = '
  lua_string "$ROOT"
  printf '\n'
  printf '%s\n' 'local fiddle_skills = {'
  for skill_file in "$ROOT"/skills/*/SKILL.md; do
    skill="$(basename "$(dirname "$skill_file")")"
    printf '  '
    lua_string "$skill"
    printf ',\n'
  done
  printf '%s\n' '}' ''
  cat "$ROOT/maki/fiddle.lua"
  printf '%s\n' '' 'register_fiddle(fiddle_root, fiddle_skills)' "$END"
} > "$INIT"

printf 'Installed Fiddle commands in %s\n' "$INIT"
printf '%s\n' 'Run /reload in Maki to activate them.'
