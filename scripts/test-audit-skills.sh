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

assert_file_excludes() {
  local description="$1" pattern="$2" file="$3"
  if grep -F --quiet "$pattern" "$file"; then
    FAIL=$((FAIL + 1))
    echo "  FAIL: $description (found $pattern)"
  else
    PASS=$((PASS + 1))
    echo "  PASS: $description"
  fi
}

assert_path_missing() {
  local description="$1" path="$2"
  if [[ -e "$path" ]]; then
    FAIL=$((FAIL + 1))
    echo "  FAIL: $description (found $path)"
  else
    PASS=$((PASS + 1))
    echo "  PASS: $description"
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
assert_exit "valid tree" 0 "$(run_audit "$VALID" "$ERR" --max-primary-lines 80)"

echo "Test 2: malformed frontmatter fails"
MALFORMED="$TMPDIR/malformed"
mkdir -p "$MALFORMED/skills/bad"
printf 'name: bad\ndescription: missing delimiters\n' > "$MALFORMED/skills/bad/SKILL.md"
ERR="$TMPDIR/malformed.json"
assert_exit "malformed frontmatter" 2 "$(run_audit "$MALFORMED" "$ERR")"
assert_error_code "malformed frontmatter code" "malformed-frontmatter" "$ERR"

echo "Test 3: non-router description always fails"
NONROUTER="$TMPDIR/nonrouter"
make_skill "$NONROUTER" "bad" "Creates a useful artifact for the project."
ERR="$TMPDIR/nonrouter.json"
assert_exit "non-router description" 2 "$(run_audit "$NONROUTER" "$ERR")"
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

SELF_REFERENCE="$TMPDIR/self-reference"
mkdir -p "$SELF_REFERENCE/skills/self-reference"
cat > "$SELF_REFERENCE/skills/self-reference/SKILL.md" <<'EOF'
---
name: self-reference
description: Use when checking that companion files require an entrypoint reference.
---

# Self reference
EOF
printf '# Companion\n\nRead [this file](companion.md).\n' > "$SELF_REFERENCE/skills/self-reference/companion.md"
ERR="$TMPDIR/self-reference.json"
assert_exit "self-reference is not reachability" 2 "$(run_audit "$SELF_REFERENCE" "$ERR")"
assert_error_code "self-reference orphan code" "orphaned-companion" "$ERR"

echo "Test 8: oversized primary skill fails"
OVERSIZE="$TMPDIR/oversize"
make_skill "$OVERSIZE" "oversize" "Use when checking primary skill size constraints before publishing."
for _ in $(seq 1 81); do printf 'extra line\n' >> "$OVERSIZE/skills/oversize/SKILL.md"; done
ERR="$TMPDIR/oversize.json"
assert_exit "oversized primary skill" 2 "$(run_audit "$OVERSIZE" "$ERR" --max-primary-lines 80)"
assert_error_code "oversized primary skill code" "oversized-primary-skill" "$ERR"

echo "Test 9: optional agent-empathy prompts are documented"
assert_file_contains "capability prompt" "What can the available tools and context do" "$SCRIPT_DIR/../skills/discover-docs/SKILL.md"
assert_file_contains "needed context prompt" "What missing context, access, or tool" "$SCRIPT_DIR/../skills/brainstorm/SKILL.md"
assert_file_contains "prior-run prompt" "What did the previous run reveal" "$SCRIPT_DIR/../skills/brainstorm/SKILL.md"
assert_file_contains "optional prompt guard" "optional diagnostic, not a required questionnaire" "$SCRIPT_DIR/../skills/discover-docs/SKILL.md"

echo "Test 10: extracted protocols retain executable contracts"
assert_file_contains "plan header contract" "For agentic workers" "$SCRIPT_DIR/../skills/write-plan/plan-format.md"
assert_file_contains "plan critique dispatch" "hooks/dispatch-provider.sh <provider>" "$SCRIPT_DIR/../skills/write-plan/plan-format.md"
assert_file_contains "bean creation contract" "beans create --json" "$SCRIPT_DIR/../skills/write-plan/bean-materialization.md"
assert_file_contains "evaluation log initialization" 'BASE_SHA=$(git rev-parse HEAD)' "$SCRIPT_DIR/../skills/develop-loop/dispatch-and-evidence.md"
assert_file_contains "domain resolution command" "scripts/resolve-domains.sh" "$SCRIPT_DIR/../skills/develop-loop/dispatch-and-evidence.md"
assert_file_contains "scorecard validation command" "scripts/validate-scorecard.sh" "$SCRIPT_DIR/../skills/develop-loop/dispatch-and-evidence.md"
assert_file_contains "threshold command" "scripts/check-thresholds.sh" "$SCRIPT_DIR/../skills/develop-loop/convergence-and-recovery.md"
assert_file_contains "evaluation log command" "scripts/append-eval-log.sh" "$SCRIPT_DIR/../skills/develop-loop/convergence-and-recovery.md"

echo "Test 11: plans and specs remain local lifecycle artifacts"
assert_file_contains "plan local policy" "Plans are local lifecycle artifacts and are not committed." "$SCRIPT_DIR/../skills/write-plan/plan-format.md"
assert_file_contains "spec local policy" "local lifecycle artifact; do not commit it" "$SCRIPT_DIR/../skills/brainstorm/SKILL.md"
assert_file_contains "challenge reads specs" "docs/specs/" "$SCRIPT_DIR/../skills/challenge/SKILL.md"
assert_file_excludes "brainstorm does not commit specs" "and commit it" "$SCRIPT_DIR/../skills/brainstorm/SKILL.md"
assert_file_excludes "define does not commit specs" "commit the changes before proceeding" "$SCRIPT_DIR/../skills/define/SKILL.md"

echo "Test 12: evaluator provider state and runtime roles are unambiguous"
assert_file_contains "dispatch provider state is domain-specific" "selected-provider-{domain}.json" "$SCRIPT_DIR/../skills/develop-loop/dispatch-and-evidence.md"
assert_file_contains "convergence provider state is domain-specific" "selected-provider-{domain}.json" "$SCRIPT_DIR/../skills/develop-loop/convergence-and-recovery.md"
assert_file_excludes "evaluator does not interact with app" "evaluator can interact with the app" "$SCRIPT_DIR/../skills/develop-loop/dispatch-and-evidence.md"

echo "Test 13: delivery closes the epic without obsolete lifecycle wrappers"
assert_file_contains "deliver closes named epic" 'beans update <epic-id> --status completed' "$SCRIPT_DIR/../skills/deliver/SKILL.md"
assert_file_excludes "deliver does not auto-archive" "fiddle:archive" "$SCRIPT_DIR/../skills/deliver/SKILL.md"
assert_file_excludes "routing has no archive mode" "docs/archive" "$SCRIPT_DIR/../skills/using-fiddle/SKILL.md"
assert_path_missing "status poller removed" "$SCRIPT_DIR/orchestrate-status.sh"
assert_path_missing "archive wrapper removed" "$SCRIPT_DIR/archive.sh"
assert_path_missing "archive skill removed" "$SCRIPT_DIR/../skills/archive/SKILL.md"
assert_file_contains "archive guard retained" "archive directories contain stale artifacts" "$SCRIPT_DIR/../hooks/archive-guard.sh"

echo "Test 14: portability owns the complete CI audit path"
assert_file_contains "portability runs complete audit" '"$ROOT/scripts/audit-skills.sh"' "$SCRIPT_DIR/check-portability.sh"
assert_file_excludes "audit has no optional router flag" "require-router" "$SCRIPT_DIR/audit-skills.sh"
assert_file_excludes "workflow has no duplicate direct audit" "scripts/audit-skills.sh" "$SCRIPT_DIR/../.github/workflows/skill-quality.yml"


echo
printf 'Results: %d passed, %d failed\n' "$PASS" "$FAIL"
[[ "$FAIL" -eq 0 ]]
