#!/usr/bin/env bash
set -euo pipefail

ROOT=""
REQUIRE_ROUTER=false
MAX_PRIMARY_LINES=300

usage() {
  cat <<'EOF'
Usage: audit-skills.sh [--root <path>] [--require-router] [--max-primary-lines <count>]

Exit 0 when no violations are found. Exit 2 with {"errors":[...]} on violations
or invalid arguments.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root)
      ROOT="${2:-}"
      shift 2
      ;;
    --require-router)
      REQUIRE_ROUTER=true
      shift
      ;;
    --max-primary-lines)
      MAX_PRIMARY_LINES="${2:-}"
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      jq -n --arg error "unknown argument: $1" '{errors:[{code:"invalid-argument",error:$error}]}' >&2
      exit 2
      ;;
  esac
done

ROOT="${ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
if [[ ! -d "$ROOT/skills" ]]; then
  jq -n --arg root "$ROOT" '{errors:[{code:"missing-skills-directory",root:$root}]}' >&2
  exit 2
fi
if ! [[ "$MAX_PRIMARY_LINES" =~ ^[1-9][0-9]*$ ]]; then
  jq -n --arg value "$MAX_PRIMARY_LINES" '{errors:[{code:"invalid-max-primary-lines",value:$value}]}' >&2
  exit 2
fi

errors='[]'
add_error() {
  local code="$1" file="$2" detail="$3"
  errors=$(jq --arg code "$code" --arg file "$file" --arg detail "$detail" \
    '. + [{code:$code,file:$file,detail:$detail}]' <<<"$errors")
}

referenced_files=$'\n'
validate_reference() {
  local source="$1" reference="$2" target="$3"
  if [[ ! -f "$target" ]]; then
    add_error "missing-reference" "$source" "$reference"
    return
  fi

  referenced_files+="$(cd "$(dirname "$target")" && pwd)/$(basename "$target")"$'\n'
}

relative_to_root() {
  local path="$1"
  printf '%s' "${path#"$ROOT/"}"
}

is_documented_dynamic_template() {
  local relative="$1"
  [[ "$relative" =~ ^skills/evaluate/evaluator-[A-Za-z0-9_-]+\.md$ ]]
}

for skill_file in "$ROOT"/skills/*/SKILL.md; do
  [[ -f "$skill_file" ]] || continue
  relative=$(relative_to_root "$skill_file")
  skill_dir=$(dirname "$skill_file")
  expected_name=$(basename "$skill_dir")

  first_line=$(sed -n '1p' "$skill_file")
  closing_line=$(sed -n '2,/^---$/=' "$skill_file" | tail -n 1)
  if [[ "$first_line" != '---' || -z "$closing_line" ]]; then
    add_error "malformed-frontmatter" "$relative" "frontmatter must start and end with ---"
    continue
  fi

  frontmatter=$(sed -n "2,$((closing_line - 1))p" "$skill_file")
  name=$(awk -F': ' '$1 == "name" { print substr($0, 7); exit }' <<<"$frontmatter")
  description=$(awk -F': ' '$1 == "description" { print substr($0, 14); exit }' <<<"$frontmatter")
  if [[ -z "$name" || -z "$description" ]]; then
    add_error "malformed-frontmatter" "$relative" "frontmatter requires name and description"
  elif [[ "$name" != "$expected_name" ]]; then
    add_error "skill-name-mismatch" "$relative" "name $name does not match directory $expected_name"
  elif [[ "$REQUIRE_ROUTER" == true && ! "$description" =~ ^Use\ (when|to|after|before)\  ]]; then
    add_error "non-router-description" "$relative" "description must begin with Use when, Use to, Use after, or Use before"
  fi

  line_count=$(wc -l < "$skill_file" | tr -d ' ')
  if (( line_count > MAX_PRIMARY_LINES )); then
    add_error "oversized-primary-skill" "$relative" "$line_count lines exceeds $MAX_PRIMARY_LINES"
  fi

done

while IFS= read -r markdown_file; do
  markdown_relative=$(relative_to_root "$markdown_file")
  markdown_dir=$(dirname "$markdown_file")
  while IFS= read -r reference; do
    target="$markdown_dir/$reference"
    validate_reference "$markdown_relative" "$reference" "$target"
  done < <(grep -Eo '\]\(([A-Za-z0-9_./-]+\.md)\)' "$markdown_file" | sed -E 's/^\]\((.*)\)$/\1/' | sort -u)
  while IFS= read -r reference; do
    [[ "$reference" == *'<'* || "$reference" == *'>'* ]] && continue
    case "$reference" in
      references/*)
        target="$markdown_dir/$reference"
        ;;
      skills/*|scripts/*)
        target="$ROOT/$reference"
        ;;
      @*)
        target="$markdown_dir/${reference#@}"
        ;;
      *)
        continue
        ;;
    esac
    validate_reference "$markdown_relative" "$reference" "$target"
  done < <(grep -Eo '(@[A-Za-z0-9_./-]+\.md|references/[A-Za-z0-9_./-]+\.md|skills/[A-Za-z0-9_./-]+\.md|scripts/[A-Za-z0-9_./-]+\.md)' "$markdown_file" | sort -u)
done < <(find "$ROOT/skills" -type f -name '*.md' -print)

while IFS= read -r companion; do
  companion_relative=$(relative_to_root "$companion")
  [[ "$(basename "$companion")" == 'SKILL.md' ]] && continue
  is_documented_dynamic_template "$companion_relative" && continue
  normalized_companion="$(cd "$(dirname "$companion")" && pwd)/$(basename "$companion")"
  if [[ "$referenced_files" != *$'\n'"$normalized_companion"$'\n'* ]]; then
    add_error "orphaned-companion" "$companion_relative" "companion is not referenced by another skill document"
  fi
done < <(find "$ROOT/skills" -type f -name '*.md' -print)

if [[ "$errors" != '[]' ]]; then
  jq -n --argjson errors "$errors" '{errors:$errors}' >&2
  exit 2
fi
