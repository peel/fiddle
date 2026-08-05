#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0
FAIL=0
TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

assert_exit() {
  local description="$1" expected="$2" actual="$3"
  if [[ "$expected" == "$actual" ]]; then
    PASS=$((PASS + 1))
    echo "  PASS: $description"
  else
    FAIL=$((FAIL + 1))
    echo "  FAIL: $description (expected exit $expected, got $actual)"
  fi
}

assert_error_code() {
  local description="$1" expected="$2" error_file="$3"
  local actual
  actual=$(jq -r '.errors[] | select(.code == "'"$expected"'") | .code' "$error_file" 2>/dev/null | head -n 1 || true)
  if [[ "$actual" == "$expected" ]]; then
    PASS=$((PASS + 1))
    echo "  PASS: $description"
  else
    FAIL=$((FAIL + 1))
    echo "  FAIL: $description (expected error $expected, got ${actual:-none})"
  fi
}

assert_file_contains() {
  local description="$1" pattern="$2" file="$3"
  if grep -F --quiet "$pattern" "$file"; then
    PASS=$((PASS + 1))
    echo "  PASS: $description"
  else
    FAIL=$((FAIL + 1))
    echo "  FAIL: $description (missing $pattern)"
  fi
}

make_skill() {
  local root="$1" name="$2" description="$3"
  mkdir -p "$root/skills/$name"
  cat > "$root/skills/$name/SKILL.md" <<EOF
---
name: $name
description: $description
---

# $name

Use references/details.md when deeper detail is needed.
EOF
  mkdir -p "$root/skills/$name/references"
  printf '# Details\n' > "$root/skills/$name/references/details.md"
}

run_audit() {
  local root="$1" error_file="$2"
  shift 2
  local exit_code=0
  "$SCRIPT_DIR/audit-skills.sh" --root "$root" "$@" 2>"$error_file" || exit_code=$?
  printf '%s' "$exit_code"
}

VALID="$TMPDIR/valid"
make_skill "$VALID" "example" "Use when reviewing examples to load concise guidance and reference details."
mkdir -p "$VALID/skills/evaluate"
printf '# dynamic template\n' > "$VALID/skills/evaluate/evaluator-general.md"
printf '\nThe evaluator template is selected dynamically from `skills/evaluate/evaluator-<domain>.md`.\n' >> "$VALID/skills/example/SKILL.md"

echo "Test 1: valid skill tree passes"
ERR="$TMPDIR/valid.json"
assert_exit "valid tree" 0 "$(run_audit "$VALID" "$ERR" --require-router --max-primary-lines 80)"

echo "Test 2: malformed frontmatter fails"
MALFORMED="$TMPDIR/malformed"
mkdir -p "$MALFORMED/skills/bad"
printf 'name: bad\ndescription: missing delimiters\n' > "$MALFORMED/skills/bad/SKILL.md"
ERR="$TMPDIR/malformed.json"
assert_exit "malformed frontmatter" 2 "$(run_audit "$MALFORMED" "$ERR")"
assert_error_code "malformed frontmatter code" "malformed-frontmatter" "$ERR"

echo "Test 3: non-router description fails when required"
NONROUTER="$TMPDIR/nonrouter"
make_skill "$NONROUTER" "bad" "Creates a useful artifact for the project."
ERR="$TMPDIR/nonrouter.json"
assert_exit "non-router description" 2 "$(run_audit "$NONROUTER" "$ERR" --require-router)"
assert_error_code "non-router code" "non-router-description" "$ERR"

echo "Test 4: missing reference fails"
MISSING="$TMPDIR/missing"
make_skill "$MISSING" "missing" "Use when validating references to locate missing skill files."
rm "$MISSING/skills/missing/references/details.md"
ERR="$TMPDIR/missing.json"
assert_exit "missing reference" 2 "$(run_audit "$MISSING" "$ERR")"
assert_error_code "missing reference code" "missing-reference" "$ERR"

echo "Test 5: orphaned companion fails"
ORPHAN="$TMPDIR/orphan"
make_skill "$ORPHAN" "orphan" "Use when checking companion reachability in skill documentation."
printf '# Orphan\n' > "$ORPHAN/skills/orphan/unused.md"
ERR="$TMPDIR/orphan.json"
assert_exit "orphaned companion" 2 "$(run_audit "$ORPHAN" "$ERR")"
assert_error_code "orphaned companion code" "orphaned-companion" "$ERR"

echo "Test 6: Markdown link targets are validated"
MARKDOWN_LINK="$TMPDIR/markdown-link"
make_skill "$MARKDOWN_LINK" "markdown-link" "Use when validating local Markdown link targets."
printf '\nRead [missing guidance](missing.md).\n' >> "$MARKDOWN_LINK/skills/markdown-link/SKILL.md"
ERR="$TMPDIR/markdown-link.json"
assert_exit "missing Markdown link" 2 "$(run_audit "$MARKDOWN_LINK" "$ERR")"
assert_error_code "missing Markdown link code" "missing-reference" "$ERR"

MARKDOWN_LINK_VALID="$TMPDIR/markdown-link-valid"
make_skill "$MARKDOWN_LINK_VALID" "markdown-link-valid" "Use when validating existing local Markdown link targets."
printf '# Guidance\n' > "$MARKDOWN_LINK_VALID/skills/markdown-link-valid/guidance.md"
printf '\nRead [guidance](guidance.md), [site](https://example.com/guide.md), and [section](#details).\n' >> "$MARKDOWN_LINK_VALID/skills/markdown-link-valid/SKILL.md"
ERR="$TMPDIR/markdown-link-valid.json"
assert_exit "valid Markdown link" 0 "$(run_audit "$MARKDOWN_LINK_VALID" "$ERR")"

echo "Test 7: companion reachability uses exact paths"
DUPLICATE_BASENAME="$TMPDIR/duplicate-basename"
make_skill "$DUPLICATE_BASENAME" "duplicate-basename" "Use when checking exact companion reachability."
mkdir -p "$DUPLICATE_BASENAME/skills/duplicate-basename/other"
printf '# Unlinked details\n' > "$DUPLICATE_BASENAME/skills/duplicate-basename/other/details.md"
ERR="$TMPDIR/duplicate-basename.json"
assert_exit "unlinked duplicate basename" 2 "$(run_audit "$DUPLICATE_BASENAME" "$ERR")"
assert_error_code "unlinked duplicate basename code" "orphaned-companion" "$ERR"

echo "Test 6: oversized primary skill fails"
OVERSIZE="$TMPDIR/oversize"
make_skill "$OVERSIZE" "oversize" "Use when checking primary skill size constraints before publishing."
for _ in $(seq 1 81); do printf 'extra line\n' >> "$OVERSIZE/skills/oversize/SKILL.md"; done
ERR="$TMPDIR/oversize.json"
echo "Test 7: optional agent-empathy prompts are documented"
assert_file_contains "capability prompt" "What can the available tools and context do" "$SCRIPT_DIR/../skills/discover-docs/SKILL.md"
assert_file_contains "needed context prompt" "What missing context, access, or tool" "$SCRIPT_DIR/../skills/brainstorm/SKILL.md"
assert_file_contains "prior-run prompt" "What did the previous run reveal" "$SCRIPT_DIR/../skills/brainstorm/SKILL.md"
assert_file_contains "optional prompt guard" "optional diagnostic, not a required questionnaire" "$SCRIPT_DIR/../skills/discover-docs/SKILL.md"

assert_exit "oversized primary skill" 2 "$(run_audit "$OVERSIZE" "$ERR" --max-primary-lines 80)"
assert_error_code "oversized primary skill code" "oversized-primary-skill" "$ERR"

echo
printf 'Results: %d passed, %d failed\n' "$PASS" "$FAIL"
[[ "$FAIL" -eq 0 ]]
