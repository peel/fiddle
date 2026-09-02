#!/usr/bin/env bash
set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0; FAIL=0

assert_exit() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected exit $expected, got $actual)"
  fi
}

assert_contains() {
  local desc="$1" needle="$2" haystack="$3"
  if printf '%s' "$haystack" | grep -qF "$needle"; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected to contain '$needle' in: $haystack)"
  fi
}

assert_excludes() {
  local desc="$1" needle="$2" haystack="$3"
  if printf '%s' "$haystack" | grep -qF "$needle"; then
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected NOT to contain '$needle' in: $haystack)"
  else
    PASS=$((PASS+1)); echo "  PASS: $desc"
  fi
}

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

DOCUMENT_REL="workflows/toil.toml"
SYSTEM_REL="docs/technical/SYSTEM.md"
ADR_REL="docs/technical/decisions/083-toil-is-a-document-and-a-gate.md"
COMMIT_ADR_REL="docs/technical/decisions/082-a-step-earns-the-commit-the-branch-step-publishes.md"
MAIN_REL="crates/fiddle-cli/src/main.rs"

ORDERING='after `selected_workflow` loads the document'

step() { printf '[[steps]]\nkind = "%s"\n\n' "$1"; }
effect() { printf '[[steps]]\nkind = "effect"\nname = "%s"\n\n' "$1"; }

document() {
  {
    printf 'version = 1\nname = "toil"\nstage = "toil"\n\n'
    printf '# The gate runs in Rust %s and before the workflow runs.\n\n' "$ORDERING"
    step agent
    step evaluate
    step commit
    effect ensure_branch_published
    effect ensure_pull_request
    effect jira.pull_request_linked
    effect jira.issue_transitioned
  } > "$WORK/tree/$DOCUMENT_REL"
}

EFFECT_LIST='`ensure_branch_published`, `ensure_pull_request`, `jira.pull_request_linked` and `jira.issue_transitioned`'

system() {
  {
    printf '| toil gate | `fiddle-runtime/src/toil` | thirteen rules | the gate runs in Rust %s and before the workflow runs (ADR 083) |\n' "$ORDERING"
    case "${1:-both}" in
      both) printf 'The shipped document names seven steps: an agent step, an evaluation, a commit, and the four effect steps %s.\n' "$EFFECT_LIST" ;;
      steps) printf 'The shipped document names seven steps: an agent step, an evaluation, a commit, %s.\n' "$EFFECT_LIST" ;;
      effects) printf 'The shipped document names these steps: an agent step, an evaluation, a commit, and the four effect steps %s.\n' "$EFFECT_LIST" ;;
    esac
  } > "$WORK/tree/$SYSTEM_REL"
}

adr() {
  {
    printf '# 083\n\n'
    printf '`toil::qualify` runs in `qualified`, %s and before the workflow runs.\n' "$ORDERING"
    case "${1:-both}" in
      both) printf 'The shipped document names seven steps. A run reaches the four effect steps — %s — in the order the document names them.\n' "$EFFECT_LIST" ;;
      steps) printf 'The shipped document names seven steps. A run reaches them, the tail being %s, in the order the document names them.\n' "$EFFECT_LIST" ;;
      effects) printf 'A run reaches the four effect steps — %s — in the order the shipped document names them.\n' "$EFFECT_LIST" ;;
    esac
  } > "$WORK/tree/$ADR_REL"
}

commit_adr() {
  {
    printf '# 082\n\n'
    case "${1:-both}" in
      both) printf 'The shipped `workflows/toil.toml` names seven steps: agent, evaluate, commit, and the four effect steps %s.\n' "$EFFECT_LIST" ;;
      steps) printf 'The shipped `workflows/toil.toml` names seven steps: agent, evaluate, commit, %s.\n' "$EFFECT_LIST" ;;
      effects) printf 'The shipped `workflows/toil.toml` names these steps: agent, evaluate, commit, and the four effect steps %s.\n' "$EFFECT_LIST" ;;
    esac
  } > "$WORK/tree/$COMMIT_ADR_REL"
}

record_path() {
  case "$1" in
    system) printf '%s' "$SYSTEM_REL" ;;
    adr) printf '%s' "$ADR_REL" ;;
    commit_adr) printf '%s' "$COMMIT_ADR_REL" ;;
  esac
}

fresh() {
  rm -rf "$WORK/tree"
  mkdir -p "$WORK/tree/docs/technical/decisions" "$WORK/tree/crates/fiddle-cli/src" "$WORK/tree/workflows"
  document
  system
  adr
  commit_adr
  printf 'fn dispatch() {\n    let document = selected_workflow(selection, &cli.config)?;\n    let qualification = qualified(\n        &config,\n    );\n}\n' \
    > "$WORK/tree/$MAIN_REL"
}

run() { "$SCRIPT_DIR/check-toil-gate-order.sh" --root "$WORK/tree" 2>&1; }

echo "the four records agreeing is the passing case"
fresh
OUT=$(run); RC=$?
assert_exit "a tree whose document, records and call order agree passes" 0 "$RC"
assert_contains "and it prints the two line numbers it compared" "selected_workflow at 2" "$OUT"
assert_contains "and it prints the denominators it derived" "names 7 steps and 4 effect steps" "$OUT"

echo "the row asserting the retired ordering fails"
fresh
printf '| toil gate | `fiddle-runtime/src/toil` | thirteen rules | the gate runs in Rust before the document loads (ADR 083) |\nThe shipped document names seven steps, of which the four effect steps are %s.\n' "$EFFECT_LIST" \
  > "$WORK/tree/$SYSTEM_REL"
OUT=$(run); RC=$?
assert_exit "the row this bean found fails" 1 "$RC"
assert_contains "and the reason quotes the retired clause" "before the document loads" "$OUT"

echo "the shipped document asserting the retired ordering fails, which is the copy that shipped with it"
fresh
{
  printf 'version = 1\nname = "toil"\nstage = "toil"\n\n'
  printf '# Eligibility is decided before this document is loaded. A run that reaches\n# these steps has already been qualified.\n\n'
  step agent
  step evaluate
  step commit
  effect ensure_branch_published
  effect ensure_pull_request
  effect jira.pull_request_linked
  effect jira.issue_transitioned
} > "$WORK/tree/$DOCUMENT_REL"
OUT=$(run); RC=$?
assert_exit "the operator-facing document is inside this check's scope" 1 "$RC"
assert_contains "and the reason names the document" "workflows/toil.toml" "$OUT"
assert_contains "and it quotes the clause it found there" "Eligibility is decided before" "$OUT"

echo "a row that states no ordering at all fails"
fresh
printf '| toil gate | `fiddle-runtime/src/toil` | thirteen rules | a refusal is published on the ticket (ADR 083) |\nThe shipped document names seven steps, of which the four effect steps are %s.\n' "$EFFECT_LIST" \
  > "$WORK/tree/$SYSTEM_REL"
OUT=$(run); RC=$?
assert_exit "silence is not agreement" 1 "$RC"
assert_contains "and the reason names the ordering it wanted" "$ORDERING" "$OUT"

echo "the ADR dropping the ordering fails, so the pair is pinned in both directions"
fresh
printf '# 083\n\nEligibility is an outer Rust gate.\nThe shipped document names seven steps. A run reaches the four effect steps — %s — in order.\n' "$EFFECT_LIST" \
  > "$WORK/tree/$ADR_REL"
OUT=$(run); RC=$?
assert_exit "an ADR that no longer states the ordering fails" 1 "$RC"
assert_contains "and the reason names the ADR" "083-toil-is-a-document-and-a-gate.md" "$OUT"

echo "the source moving under both records fails"
fresh
printf 'fn dispatch() {\n    let qualification = qualified(\n        &config,\n    );\n    let document = selected_workflow(selection, &cli.config)?;\n}\n' \
  > "$WORK/tree/$MAIN_REL"
OUT=$(run); RC=$?
assert_exit "a binary that gates before it loads fails, whatever the records say" 1 "$RC"
assert_contains "and the reason names both line numbers" "selected_workflow at line 5 and qualified at line 2" "$OUT"

echo "an eighth step reds every record that states the count"
fresh
step check >> "$WORK/tree/$DOCUMENT_REL"
OUT=$(run); RC=$?
assert_exit "a document that grew a step fails until the records say so" 1 "$RC"
assert_contains "and the reason names the count the records now have to state" 'names eight steps' "$OUT"
assert_contains "and it names both records that state a step count" "082-a-step-earns-the-commit-the-branch-step-publishes.md" "$OUT"
assert_contains "and it names the other one" "SYSTEM.md" "$OUT"

echo "a fifth effect step reds the effect count on its own, with the step count unmoved"
fresh
{
  printf 'version = 1\nname = "toil"\nstage = "toil"\n\n'
  printf '# The gate runs in Rust %s and before the workflow runs.\n\n' "$ORDERING"
  step agent
  step evaluate
  effect jira.comment_added
  effect ensure_branch_published
  effect ensure_pull_request
  effect jira.pull_request_linked
  effect jira.issue_transitioned
} > "$WORK/tree/$DOCUMENT_REL"
OUT=$(run); RC=$?
assert_exit "seven steps of which five are effects fails" 1 "$RC"
assert_contains "and the reason names the effect count the ADR has to state" '`five effect steps`' "$OUT"
assert_excludes "and it does not blame the step count, which did not move" 'names eight steps' "$OUT"

echo "reordering the document's effect steps reds the order pin alone"
fresh
{
  printf 'version = 1\nname = "toil"\nstage = "toil"\n\n'
  printf '# The gate runs in Rust %s and before the workflow runs.\n\n' "$ORDERING"
  step agent
  step evaluate
  step commit
  effect ensure_branch_published
  effect jira.pull_request_linked
  effect ensure_pull_request
  effect jira.issue_transitioned
} > "$WORK/tree/$DOCUMENT_REL"
OUT=$(run); RC=$?
assert_exit "a link step moved before the pull request step fails" 1 "$RC"
assert_contains "and the reason names the order it read off the document" "ensure_branch_published jira.pull_request_linked ensure_pull_request" "$OUT"
assert_excludes "and the counts are not blamed, because neither moved" 'effect steps` and' "$OUT"

echo "a record left on the old count fails, and the reason names that record"
fresh
printf '# 082\n\nThe shipped `workflows/toil.toml` names six steps: agent, evaluate, commit, `ensure_branch_published`, `ensure_pull_request` and `jira.pull_request_linked`.\nIts four effect steps are %s.\n' "$EFFECT_LIST" \
  > "$WORK/tree/$COMMIT_ADR_REL"
OUT=$(run); RC=$?
assert_exit "the sentence fiddle-46a0 left behind fails" 1 "$RC"
assert_contains "and the reason quotes the stale count" '`six steps`' "$OUT"

echo "a stale count reds even when it is not spelt \`names N steps\`, so the canonical form is not the whole pin"
fresh
printf '# 082\n\nThe shipped `workflows/toil.toml` names seven steps: agent, evaluate, commit, and the four effect steps %s.\nOf the six steps that document held before `fiddle-46a0`, the last was the link.\n' "$EFFECT_LIST" \
  > "$WORK/tree/$COMMIT_ADR_REL"
OUT=$(run); RC=$?
assert_exit "a record carrying both the right canonical form and a stale count fails" 1 "$RC"
assert_contains "and the reason quotes the stale count it found beside it" '`six steps`' "$OUT"

echo "a record left on the old effect count fails"
fresh
printf '# 083\n\n`toil::qualify` runs in `qualified`, %s and before the workflow runs.\nThe shipped document names seven steps. It reaches the three effect steps — %s — in order.\n' "$ORDERING" "$EFFECT_LIST" \
  > "$WORK/tree/$ADR_REL"
OUT=$(run); RC=$?
assert_exit "the sentence this bean found in ADR 083 fails" 1 "$RC"
assert_contains "and the reason quotes the stale effect count" '`three effect steps`' "$OUT"

echo "a record that states the step count and not the effect count fails, once per record"
for RECORD in system adr commit_adr; do
  fresh
  "$RECORD" steps
  OUT=$(run); RC=$?
  assert_exit "$RECORD stating only the step count fails" 1 "$RC"
  assert_contains "and the reason names the count it is missing" '`four effect steps`' "$OUT"
  assert_contains "and the reason names the record missing it" "$(record_path "$RECORD")" "$OUT"
  assert_excludes "and the step count it does state is not blamed" 'does not say `names seven steps`' "$OUT"
done

echo "a record that states the effect count and not the step count fails, once per record"
for RECORD in system adr commit_adr; do
  fresh
  "$RECORD" effects
  OUT=$(run); RC=$?
  assert_exit "$RECORD stating only the effect count fails" 1 "$RC"
  assert_contains "and the reason names the count it is missing" '`names seven steps`' "$OUT"
  assert_contains "and the reason names the record missing it" "$(record_path "$RECORD")" "$OUT"
  assert_excludes "and the effect count it does state is not blamed" 'does not say `four effect steps`' "$OUT"
done

echo "a precondition that cannot be read refuses rather than answering"
fresh
rm "$WORK/tree/$SYSTEM_REL"
OUT=$(run); RC=$?
assert_exit "a missing SYSTEM.md is exit 2 and not a pass" 2 "$RC"
assert_contains "and it says it measured nothing" "measured nothing" "$OUT"

fresh
rm "$WORK/tree/$DOCUMENT_REL"
OUT=$(run); RC=$?
assert_exit "a missing workflows/toil.toml is exit 2 and not a pass" 2 "$RC"
assert_contains "and it names the file it could not read" "workflows/toil.toml" "$OUT"

fresh
rm "$WORK/tree/$COMMIT_ADR_REL"
OUT=$(run); RC=$?
assert_exit "a missing ADR 082 is exit 2 and not a pass" 2 "$RC"
assert_contains "and it names that file" "082-a-step-earns-the-commit-the-branch-step-publishes.md" "$OUT"

fresh
printf 'version = 1\nname = "toil"\nstage = "toil"\n\n# The gate runs in Rust %s and before the workflow runs.\n' "$ORDERING" \
  > "$WORK/tree/$DOCUMENT_REL"
OUT=$(run); RC=$?
assert_exit "a document naming no steps is exit 2, not a pass on counts read off nothing" 2 "$RC"
assert_contains "and it says why" 'names no `[[steps]]`' "$OUT"

fresh
{
  printf 'version = 1\nname = "toil"\nstage = "toil"\n\n'
  printf '# The gate runs in Rust %s and before the workflow runs.\n\n' "$ORDERING"
  step agent
  step evaluate
  step commit
} > "$WORK/tree/$DOCUMENT_REL"
OUT=$(run); RC=$?
assert_exit "a document naming no effect step is exit 2" 2 "$RC"
assert_contains "and it says the effect order would be read off nothing" "read off nothing" "$OUT"

fresh
printf '[[steps]]\nkind = "effect"\n\n' >> "$WORK/tree/$DOCUMENT_REL"
OUT=$(run); RC=$?
assert_exit "an effect step carrying no name is exit 2" 2 "$RC"
assert_contains "and it says which fact it could not read" 'carries no `name`' "$OUT"

fresh
{
  printf 'version = 1\nname = "toil"\nstage = "toil"\n\n'
  printf '# The gate runs in Rust %s and before the workflow runs.\n\n' "$ORDERING"
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12; do step agent; done
  effect ensure_branch_published
  effect ensure_pull_request
  effect jira.pull_request_linked
  effect jira.issue_transitioned
} > "$WORK/tree/$DOCUMENT_REL"
OUT=$(run); RC=$?
assert_exit "a step count this check has no word for is exit 2 and not a pass" 2 "$RC"
assert_contains "and it says it carries no number word for it" "no number word" "$OUT"

fresh
system
printf '| toil gate | a second row | thirteen rules | the gate runs in Rust %s |\n' "$ORDERING" \
  >> "$WORK/tree/$SYSTEM_REL"
OUT=$(run); RC=$?
assert_exit "two rows spelt the same way is exit 2, because the check reads one" 2 "$RC"
assert_contains "and it says how many it found" "holds 2 rows" "$OUT"

fresh
printf 'fn dispatch() {\n    let document = load(selection)?;\n    let gate = gate();\n}\n' > "$WORK/tree/$MAIN_REL"
OUT=$(run); RC=$?
assert_exit "a main.rs whose two calls this check cannot find is exit 2 and not a pass" 2 "$RC"
assert_contains "and it names what it could not find" "measured nothing" "$OUT"

OUT=$("$SCRIPT_DIR/check-toil-gate-order.sh" --root "$WORK/nosuchtree" 2>&1); RC=$?
assert_exit "an absent root is exit 2" 2 "$RC"

OUT=$("$SCRIPT_DIR/check-toil-gate-order.sh" --wat 2>&1); RC=$?
assert_exit "an unknown argument is exit 2" 2 "$RC"

echo "and the real tree passes"
OUT=$("$SCRIPT_DIR/check-toil-gate-order.sh" --root "$SCRIPT_DIR/.." 2>&1); RC=$?
assert_exit "this repository's own document and four records agree" 0 "$RC"
assert_contains "and the pass names the ordering it compared" "TOIL GATE ORDER: ok" "$OUT"
assert_contains "and it names this document's own denominators" "names 7 steps and 4 effect steps" "$OUT"

echo
echo "  $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
