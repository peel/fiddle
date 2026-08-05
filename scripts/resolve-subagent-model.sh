#!/usr/bin/env bash
set -euo pipefail

CONFIG="orchestrate.json"
PHASE=""
ROLE=""

error() {
  jq -n --arg code "$1" --arg message "$2" '{error:{code:$code,message:$message}}' >&2
  exit 2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --config) CONFIG="${2:-}"; shift 2 ;;
    --phase) PHASE="${2:-}"; shift 2 ;;
    --role) ROLE="${2:-}"; shift 2 ;;
    *) error "invalid-argument" "unknown argument: $1" ;;
  esac
done

[[ -n "$PHASE" ]] || error "missing-phase" "--phase is required"
[[ -n "$ROLE" ]] || error "missing-role" "--role is required"
[[ -f "$CONFIG" ]] || error "missing-config" "config file not found: $CONFIG"
jq empty "$CONFIG" 2>/dev/null || error "invalid-config" "invalid JSON: $CONFIG"

if ! jq -e '
  if has("models") then
    if (.models | type) != "object" then false
    elif (.models | has("roles")) and (.models.roles | type) != "object" then false
    elif (.models | has("phases")) and (.models.phases | type) != "object" then false
    else true
    end
  else true
  end
' "$CONFIG" >/dev/null; then
  error "invalid-config" "models, models.roles, and models.phases must be objects when present"
fi

role_model=$(jq -r --arg role "$ROLE" '.models.roles[$role] // empty' "$CONFIG")
phase_model=$(jq -r --arg phase "$PHASE" '.models.phases[$phase] // empty' "$CONFIG")
if [[ -n "$role_model" ]]; then
  model="$role_model"
  source="role"
elif [[ -n "$phase_model" ]]; then
  model="$phase_model"
  source="phase"
else
  model="default"
  source="default"
fi

case "$model" in
  default)
    jq -n --arg source "$source" '{source:$source}'
    ;;
  smol|slow)
    jq -n --arg source "$source" --arg model "$model" '{source:$source,model:$model}'
    ;;
  *)
    error "invalid-model" "models values must be default, smol, or slow; got $model"
    ;;
esac
