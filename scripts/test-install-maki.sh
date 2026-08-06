#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT
CONFIG="$TMPDIR/config"
INIT="$CONFIG/init.lua"

mkdir -p "$CONFIG"
printf '%s\n' 'maki.setup({ model = "test" })' > "$INIT"

MAKI_CONFIG_HOME="$CONFIG" "$SCRIPT_DIR/install-maki.sh" >/dev/null
first="$(cat "$INIT")"
MAKI_CONFIG_HOME="$CONFIG" "$SCRIPT_DIR/install-maki.sh" >/dev/null
second="$(cat "$INIT")"

[[ "$first" == "$second" ]]
[[ "$(grep -c '^-- BEGIN FIDDLE MANAGED BLOCK$' "$INIT")" -eq 1 ]]
[[ "$(grep -c '^-- END FIDDLE MANAGED BLOCK$' "$INIT")" -eq 1 ]]
[[ "$(grep -c 'name = "/fiddle:" .. name' "$INIT")" -eq 1 ]]
grep -q 'maki.setup({ model = "test" })' "$INIT"
grep -q '"orchestrate",' "$INIT"

printf '%s\n' 'local name = "personal"' >> "$INIT"
MAKI_CONFIG_HOME="$CONFIG" "$SCRIPT_DIR/install-maki.sh" >/dev/null
grep -q 'local name = "personal"' "$INIT"
[[ "$(grep -c '^-- BEGIN FIDDLE MANAGED BLOCK$' "$INIT")" -eq 1 ]]

OTHER="$TMPDIR/other"
mkdir -p "$OTHER"
printf '%s\n' 'maki.api.register_command({' '  name = "/fiddle:" .. name,' '})' > "$OTHER/init.lua"
if MAKI_CONFIG_HOME="$OTHER" "$SCRIPT_DIR/install-maki.sh" >/dev/null 2>&1; then
  echo "expected unmanaged registration to fail" >&2
  exit 1
fi
MAKI_CONFIG_HOME="$OTHER" "$SCRIPT_DIR/install-maki.sh" --replace-unmanaged >/dev/null
[[ -f "$OTHER/init.lua.pre-fiddle-installer.bak" ]]
[[ "$(grep -c '^-- BEGIN FIDDLE MANAGED BLOCK$' "$OTHER/init.lua")" -eq 1 ]]

printf '%s\n' "Maki installer tests passed"
