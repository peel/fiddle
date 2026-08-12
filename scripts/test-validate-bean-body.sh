#!/usr/bin/env bash
set -euo pipefail
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

assert_json() {
  local desc="$1" field="$2" expected="$3" json="$4"
  local actual
  actual=$(echo "$json" | jq -r "$field")
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"
  fi
}

TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT
ERRFILE="$TMPDIR/err.json"

echo "Test 1: complete body → exit 0"
cat > "$TMPDIR/complete.md" << 'EOF'
## Context
Some context.

## Files
- Create: `scripts/foo.sh`
- Test: `scripts/test-foo.sh`

## Steps
- [ ] Write failing test
- [ ] Implement

## Evaluation
```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: complete-body-passes
      check: "exits 0"
```
EOF
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/complete.md" 2>/dev/null || EXIT_CODE=$?
assert_exit "complete body → exit 0" 0 "$EXIT_CODE"

echo "Test 2: missing eval block → exit 2, JSON error naming eval"
cat > "$TMPDIR/no-eval.md" << 'EOF'
## Files
- Create: `scripts/foo.sh`

## Steps
- [ ] Do the thing
EOF
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/no-eval.md" 2>"$ERRFILE" || EXIT_CODE=$?
ERR=$(cat "$ERRFILE")
assert_exit "missing eval block → exit 2" 2 "$EXIT_CODE"
assert_json "error array names eval" '[.[] | select(test("eval"))] | length > 0' "true" "$ERR"

echo "Test 3: eval block without criteria: → exit 2"
cat > "$TMPDIR/no-criteria.md" << 'EOF'
## Files
- Create: `scripts/foo.sh`

## Steps
- [ ] Do the thing

```eval
domains: [infrastructure]
thresholds: {}
```
EOF
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/no-criteria.md" 2>"$ERRFILE" || EXIT_CODE=$?
ERR=$(cat "$ERRFILE")
assert_exit "eval without criteria → exit 2" 2 "$EXIT_CODE"
assert_json "error array names eval" '[.[] | select(test("eval"))] | length > 0' "true" "$ERR"

echo "Test 4: missing files section → exit 2 naming files"
cat > "$TMPDIR/no-files.md" << 'EOF'
## Context
No files section here.

## Steps
- [ ] Do the thing

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: x
      check: "y"
```
EOF
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/no-files.md" 2>"$ERRFILE" || EXIT_CODE=$?
ERR=$(cat "$ERRFILE")
assert_exit "missing files → exit 2" 2 "$EXIT_CODE"
assert_json "error array names files" '[.[] | select(test("files"))] | length > 0' "true" "$ERR"

echo "Test 5: no checkbox steps → exit 2 naming steps"
cat > "$TMPDIR/no-steps.md" << 'EOF'
## Files
- Create: `scripts/foo.sh`

## Steps
Just prose, no checkboxes.

```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: x
      check: "y"
```
EOF
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/no-steps.md" 2>"$ERRFILE" || EXIT_CODE=$?
ERR=$(cat "$ERRFILE")
assert_exit "no checkbox steps → exit 2" 2 "$EXIT_CODE"
assert_json "error array names steps" '[.[] | select(test("steps"))] | length > 0' "true" "$ERR"

echo "Test 6: --container → exit 0 even on empty body"
: > "$TMPDIR/empty.md"
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/empty.md" --container 2>/dev/null || EXIT_CODE=$?
assert_exit "container exempt → exit 0" 0 "$EXIT_CODE"

echo "Test 7: missing --body file → exit 2"
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/does-not-exist.md" 2>/dev/null || EXIT_CODE=$?
assert_exit "missing body file → exit 2" 2 "$EXIT_CODE"

# A finished bean has every step ticked. The gate checks that a body carries a
# checklist at all — a thin body has none — so ticked steps satisfy it. See
# Test 5 for the property this must not weaken.
echo "Test 8: finished body, every step ticked → exit 0"
cat > "$TMPDIR/finished.md" << 'EOF'
---
# fiddle-0000
title: a finished task bean
status: completed
---

## Context
Shaped like a real converged bean: frontmatter, ticked steps, eval block.

## Files
- Modify: `scripts/foo.sh`

## Steps
- [x] Write failing test
- [x] Implement
- [x] Commit

## Evaluation
```eval
domains: [infrastructure]
criteria:
  infrastructure:
    - id: finished-body-passes
      check: "exits 0"
```
EOF
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/finished.md" 2>"$ERRFILE" || EXIT_CODE=$?
assert_exit "all steps ticked → exit 0" 0 "$EXIT_CODE"
assert_exit "all steps ticked → no gaps on stderr" "" "$(cat "$ERRFILE")"

echo "Test 9: in-progress body, some ticked and some not → exit 0"
sed 's/^- \[x\] Commit$/- [ ] Commit/' "$TMPDIR/finished.md" > "$TMPDIR/mixed.md"
grep -q '^- \[ \] Commit$' "$TMPDIR/mixed.md" || { echo "  FIXTURE BROKEN: mixed.md has no unchecked step"; exit 1; }
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/mixed.md" 2>/dev/null || EXIT_CODE=$?
assert_exit "mixed ticked/unchecked → exit 0" 0 "$EXIT_CODE"

echo "Test 10: uppercase - [X] counts as a checklist step → exit 0"
sed 's/^- \[x\]/- [X]/' "$TMPDIR/finished.md" > "$TMPDIR/upper.md"
grep -q '^- \[X\] Implement$' "$TMPDIR/upper.md" || { echo "  FIXTURE BROKEN: upper.md has no - [X] step"; exit 1; }
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/upper.md" 2>/dev/null || EXIT_CODE=$?
assert_exit "uppercase [X] → exit 0" 0 "$EXIT_CODE"

# The gate's reason for existing: a body with no checklist of any kind is thin
# and must not reach an implementer. Test 5 covers prose-only; this covers a
# body that is otherwise complete, so only the missing checklist can fail it.
echo "Test 11: otherwise-complete body with no checklist of any kind → exit 2"
grep -v '^- \[' "$TMPDIR/finished.md" > "$TMPDIR/stepless.md"
grep -qE '^[[:space:]]*-[[:space:]]+\[[ xX]\]' "$TMPDIR/stepless.md" && { echo "  FIXTURE BROKEN: stepless.md still has a checklist"; exit 1; }
EXIT_CODE=0
"$SCRIPT_DIR/validate-bean-body.sh" --body "$TMPDIR/stepless.md" 2>"$ERRFILE" || EXIT_CODE=$?
assert_exit "no checklist at all → exit 2" 2 "$EXIT_CODE"
assert_json "error array names steps" '[.[] | select(test("steps"))] | length > 0' "true" "$(cat "$ERRFILE")"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
