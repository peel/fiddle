#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

required_artifacts=(
  .version-bump.json
  package.json
  .claude-plugin/plugin.json
  .claude-plugin/marketplace.json
  .codex-plugin/plugin.json
  .codex/hooks.json
  .agents/plugins/marketplace.json
  maki/fiddle.lua
  scripts/install-maki.sh
  plugins/fiddle/skills
  plugins/fiddle/hooks
  plugins/fiddle/.codex
  plugins/fiddle/.codex-plugin
)
for artifact in "${required_artifacts[@]}"; do
  if [[ ! -e "$artifact" ]]; then
    echo "required portability artifact is missing: $artifact" >&2
    exit 1
  fi
done

for lifecycle_dir in docs/plans docs/specs; do
  if ! git check-ignore -q "$lifecycle_dir/.fiddle-ignore-probe"; then
    echo "local lifecycle directory is not ignored: $lifecycle_dir" >&2
    exit 1
  fi
  if [[ -n "$(git ls-files "$lifecycle_dir")" ]]; then
    echo "local lifecycle artifacts are tracked: $lifecycle_dir" >&2
    exit 1
  fi
done

expected="$(jq -r '.version' .version-bump.json)"
for file in package.json .claude-plugin/plugin.json .codex-plugin/plugin.json; do
  actual="$(jq -r '.version' "$file")"
  if [[ "$actual" != "$expected" ]]; then
    echo "version mismatch: $file has $actual, expected $expected" >&2
    exit 1
  fi
done

marketplace_version="$(jq -r '.plugins[] | select(.name == "fiddle") | .version' .claude-plugin/marketplace.json)"
if [[ "$marketplace_version" != "$expected" ]]; then
  echo "version mismatch: .claude-plugin/marketplace.json has $marketplace_version, expected $expected" >&2
  exit 1
fi

if ! jq -e '.plugins[] | select(.name == "fiddle" and .source.source == "local" and .source.path == "./plugins/fiddle")' .agents/plugins/marketplace.json >/dev/null; then
  echo "Codex local marketplace entry for fiddle is missing or has the wrong source path" >&2
  exit 1
fi

if rg -n '^\\s*(argument-hint|disable-model-invocation):' skills; then
  echo "unsupported skill frontmatter found" >&2
  exit 1
fi

found_skill=0
for file in skills/*/SKILL.md; do
  [[ -f "$file" ]] || continue
  found_skill=1
  dir_name="$(basename "$(dirname "$file")")"
  skill_name="$(awk -F': ' '/^name: / { print $2; exit }' "$file")"
  if [[ "$skill_name" != "$dir_name" ]]; then
    echo "skill name mismatch: $file has name '$skill_name', expected '$dir_name'" >&2
    exit 1
  fi
done
if [[ "$found_skill" -eq 0 ]]; then
  echo "no skills found" >&2
  exit 1
fi

if rg -n 'maki\.fs\.(read|glob|grep)' maki; then
  echo "Maki adapter must delegate repository reads to agent tools" >&2
  exit 1
fi

stale_prefix='^name: f-|skills/f-|f''iddle:f-|\bf-[a-z0-9]+(-[a-z0-9]+)*\b'
if rg -n "$stale_prefix" README.md AGENTS.md CLAUDE.md docs/README.md docs/technical skills scripts .codex .codex-plugin .claude-plugin --glob '!scripts/check-portability.sh' 2>/dev/null; then
  echo "stale f-* skill references found" >&2
  exit 1
fi

legacy_namespace='name: f''iddle:|Skill\\(.*f''iddle:'
legacy_root='\\$\\{CLAUDE''_PLUGIN_ROOT\\}'
if rg -n "${legacy_namespace}|${legacy_root}" skills scripts .codex .codex-plugin .claude-plugin --glob '!scripts/check-portability.sh' 2>/dev/null; then
  echo "stale Claude-only references found" >&2
  exit 1
fi

"$ROOT/scripts/audit-skills.sh"
"$ROOT/scripts/test-resolve-orchestrate-phase.sh"

echo "portable skill metadata ok"
