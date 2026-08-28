#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DOC="$ROOT/skills/develop-loop/scorecard-merge.md"
MARKER='```bash cross-domain-merge'
OPENER="jq -s '"

PASS=0; FAIL=0

TMPDIR=$(mktemp -d) || { echo "CANNOT RUN: mktemp -d failed, so this lane has nowhere to write its fixtures" >&2; exit 2; }
trap 'rm -rf "$TMPDIR"' EXIT

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
  actual=$(printf '%s' "$json" | jq -r "$field" 2>&1)
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"
  fi
}

assert_contains() {
  local desc="$1" needle="$2" haystack="$3"
  if printf '%s' "$haystack" | grep -qF -- "$needle"; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (text does not contain '$needle': $haystack)"
  fi
}

assert_equal() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"
  fi
}

extract_block() {
  awk -v marker="$MARKER" '
    $0 == marker { seen++; if (seen == 1) { inblock = 1; next } }
    inblock && $0 == "```" { inblock = 0; next }
    inblock { print }
    END {
      if (seen == 0) exit 3
      if (seen > 1) exit 4
      if (inblock) exit 5
    }
  ' "$1"
}

extract_program() {
  awk -v opener="$OPENER" -v q="'" '
    $0 == opener { opens++; if (opens == 1) { collecting = 1; next } }
    collecting && substr($0, 1, 1) == q { collecting = 0; closed = 1; next }
    collecting { print }
    END {
      if (opens == 0) exit 3
      if (opens > 1) exit 4
      if (!closed) exit 5
    }
  '
}

merge_program_from() {
  local doc="$1"
  local block program rc

  if [ ! -f "$doc" ]; then
    echo "REFUSE: $doc does not exist, so the documented merge cannot be read" >&2
    return 2
  fi

  rc=0
  block=$(extract_block "$doc") || rc=$?
  case "$rc" in
    0) ;;
    3) echo "REFUSE: $doc carries no fenced block marked \`$MARKER\`, so there is no documented merge to execute" >&2; return 2 ;;
    4) echo "REFUSE: $doc carries more than one fenced block marked \`$MARKER\`, so the block to execute is ambiguous" >&2; return 2 ;;
    5) echo "REFUSE: the block marked \`$MARKER\` in $doc has no closing fence" >&2; return 2 ;;
    *) echo "REFUSE: reading the block marked \`$MARKER\` in $doc failed with status $rc" >&2; return 2 ;;
  esac

  rc=0
  program=$(printf '%s\n' "$block" | extract_program) || rc=$?
  case "$rc" in
    0) ;;
    3) echo "REFUSE: the block marked \`$MARKER\` has no line \`$OPENER\`, so no jq program was extracted" >&2; return 2 ;;
    4) echo "REFUSE: the block marked \`$MARKER\` has more than one line \`$OPENER\`, so the program to execute is ambiguous" >&2; return 2 ;;
    5) echo "REFUSE: the program after \`$OPENER\` has no closing single quote, so its end is unknown" >&2; return 2 ;;
    *) echo "REFUSE: extracting the program after \`$OPENER\` failed with status $rc" >&2; return 2 ;;
  esac

  if [ -z "$(printf '%s' "$program" | tr -d '[:space:]')" ]; then
    echo "REFUSE: the program extracted from the block marked \`$MARKER\` is empty, and an empty program grades nothing" >&2
    return 2
  fi

  printf '%s\n' "$program"
}

RC=0
PROGRAM=$(merge_program_from "$DOC" 2>"$TMPDIR/extract-err.txt") || RC=$?
if [ "$RC" -ne 0 ]; then
  cat "$TMPDIR/extract-err.txt" >&2
  echo "CANNOT RUN: this lane executes the merge documented in skills/develop-loop/scorecard-merge.md, and it did not extract one. It reports no result rather than a pass." >&2
  exit 2
fi
PROGRAM_LINES=$(printf '%s\n' "$PROGRAM" | grep -c '[^[:space:]]')
echo "subject: skills/develop-loop/scorecard-merge.md, block \`$MARKER\`, $PROGRAM_LINES non-blank lines of jq extracted and executed"

line_of() {
  local needle="$1" file="$2" hits
  hits=$(grep -n -F -x -- "$needle" "$file" | cut -d: -f1)
  if [ "$(printf '%s\n' "$hits" | grep -c '[0-9]')" -ne 1 ]; then
    echo "CANNOT RUN: \`$needle\` appears $(printf '%s\n' "$hits" | grep -c '[0-9]') times in $file, so the mutation cases below cannot name one line" >&2
    exit 2
  fi
  printf '%s' "$hits"
}

rewrite_line() {
  local file="$1" number="$2" replacement="$3" out="$4"
  awk -v n="$number" -v repl="$replacement" 'NR == n { print repl; next } { print }' "$file" > "$out"
}

MARKER_LINE=$(line_of "$MARKER" "$DOC") || exit 2
OPENER_LINE=$(line_of "$OPENER" "$DOC") || exit 2

MERGED=""
merge_cards() {
  local rc=0
  MERGED=$(jq -s "$PROGRAM" "$@" 2>"$TMPDIR/merge-err.txt") || rc=$?
  if [ "$rc" -ne 0 ]; then
    echo "CANNOT RUN: the jq program taken from the document failed to run (exit $rc), so no assertion below has an input" >&2
    cat "$TMPDIR/merge-err.txt" >&2
    exit 2
  fi
}

write_card() {
  local path="$1" domain="$2" criterion="$3" declares="$4"
  jq -n \
    --arg domain "$domain" \
    --arg criterion "$criterion" \
    --argjson declares "$declares" \
    '{
       task_id: "bean-x4z8",
       iteration: 1,
       timestamp: "2026-01-01T00:00:00Z",
       domains: { ($domain): { dimensions: {} } },
       criteria: [{ id: $criterion, pass: true, evidence: "the evaluator recorded evidence and scored no dimension" }]
     }
     | if $declares then .mode = "evidence-only" else . end' > "$path"
}

echo "=== Test 1: the documented merge unions the domains and carries an all-declared evidence-only mode ==="
write_card "$TMPDIR/scorecard-general.json" general the-document-is-executed true
write_card "$TMPDIR/scorecard-frontend.json" frontend an-empty-extraction-refuses true
merge_cards "$TMPDIR/scorecard-general.json" "$TMPDIR/scorecard-frontend.json"
assert_json "the merged card carries every input domain, because the document says the union is across domains" \
  '.domains | keys | join(",")' "frontend,general" "$MERGED"
assert_json "the merged card keeps each domain's dimensions object, which check-thresholds.sh requires" \
  '[.domains[] | has("dimensions")] | all' "true" "$MERGED"
assert_json "the merged card declares evidence-only when every domain declared it, which is the line the document says carries the declaration" \
  '.mode' "evidence-only" "$MERGED"
assert_json "the merged card carries one criterion from each domain, because criteria are concatenated" \
  '.criteria | length' "2" "$MERGED"

printf '%s' "$MERGED" > "$TMPDIR/merged-1.json"
printf '%s' "$MERGED" | jq -c '.criteria' > "$TMPDIR/criteria-1.json"
RC=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/merged-1.json" --criteria "$TMPDIR/criteria-1.json" > "$TMPDIR/verdict-1.json" 2>/dev/null || RC=$?
assert_exit "the grader accepts the merged card the document produces, so the documented command reaches a verdict" 0 "$RC"
assert_json "the verdict repeats the declaration, so the evidence-only path stays visible after grading" \
  '.mode' "evidence-only" "$(cat "$TMPDIR/verdict-1.json")"

echo "=== Test 2: one domain that does not declare evidence-only drops the declaration for the whole merge ==="
write_card "$TMPDIR/scorecard-general.json" general the-document-is-executed true
write_card "$TMPDIR/scorecard-frontend.json" frontend an-empty-extraction-refuses false
merge_cards "$TMPDIR/scorecard-general.json" "$TMPDIR/scorecard-frontend.json"
assert_json "the merged card claims no declaration when one domain did not declare, so a merge that declared for every input would fail here" \
  '.mode' "null" "$MERGED"
assert_json "the domain union is unchanged by the missing declaration, so this case differs from Test 1 in the declaration alone" \
  '.domains | keys | join(",")' "frontend,general" "$MERGED"

printf '%s' "$MERGED" > "$TMPDIR/merged-2.json"
printf '%s' "$MERGED" | jq -c '.criteria' > "$TMPDIR/criteria-2.json"
RC=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/merged-2.json" --criteria "$TMPDIR/criteria-2.json" > "$TMPDIR/verdict-2.json" 2>"$TMPDIR/verdict-2.err" || RC=$?
assert_exit "the grader refuses the undeclared card that scored no dimensions, which is the consequence the document states" 2 "$RC"
assert_contains "the refusal names the missing declaration rather than reporting a threshold failure" \
  "does not declare evidence-only" "$(cat "$TMPDIR/verdict-2.err")"

echo "=== Test 3: two domains that graded the same criterion id produce that id twice ==="
write_card "$TMPDIR/scorecard-general.json" general shared-criterion-id true
write_card "$TMPDIR/scorecard-frontend.json" frontend shared-criterion-id true
merge_cards "$TMPDIR/scorecard-general.json" "$TMPDIR/scorecard-frontend.json"
assert_json "the merge concatenates criteria and does not deduplicate them, so the repeated id survives twice" \
  '.criteria | length' "2" "$MERGED"
assert_json "the two surviving criteria carry one distinct id, which is the duplicate the document warns about" \
  '[.criteria[].id] | unique | length' "1" "$MERGED"

printf '%s' "$MERGED" > "$TMPDIR/merged-3.json"
printf '%s' "$MERGED" | jq -c '.criteria' > "$TMPDIR/criteria-3.json"
RC=0
"$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMPDIR/merged-3.json" --criteria "$TMPDIR/criteria-3.json" > /dev/null 2>"$TMPDIR/verdict-3.err" || RC=$?
assert_exit "the grader refuses the duplicated id rather than counting one verdict twice, as the document states" 2 "$RC"
assert_contains "the refusal names the duplicated criterion id" \
  "duplicate criterion id: \`shared-criterion-id\`" "$(cat "$TMPDIR/verdict-3.err")"

echo "=== Test 4: an extraction that finds no marked block refuses and names the marker ==="
BROKEN_MARKER="$TMPDIR/no-marker.md"
rewrite_line "$DOC" "$MARKER_LINE" '```bash' "$BROKEN_MARKER"
RC=0
OUT=$(merge_program_from "$BROKEN_MARKER" 2>"$TMPDIR/err-4.txt") || RC=$?
assert_exit "a document without the fence marker refuses, because a lane that extracts nothing must not report a pass" 2 "$RC"
assert_equal "the refusal prints no program, so no assertion can run against an empty extraction" "" "$OUT"
assert_contains "the refusal names the marker it looked for" "$MARKER" "$(cat "$TMPDIR/err-4.txt")"

RESTORED_MARKER="$TMPDIR/marker-restored.md"
rewrite_line "$BROKEN_MARKER" "$MARKER_LINE" "$MARKER" "$RESTORED_MARKER"
RC=0
OUT=$(merge_program_from "$RESTORED_MARKER" 2>/dev/null) || RC=$?
assert_exit "restoring only the marker makes the same document extract again, so Test 4 refused for the marker and not for another fault" 0 "$RC"

echo "=== Test 5: a marked block without the jq opening line refuses and names that line ==="
NO_OPENER="$TMPDIR/no-opener.md"
rewrite_line "$DOC" "$OPENER_LINE" "jq -n '" "$NO_OPENER"
RC=0
OUT=$(merge_program_from "$NO_OPENER" 2>"$TMPDIR/err-5.txt") || RC=$?
assert_exit "a marked block whose program opener changed refuses, because the lane cannot tell which lines are the program" 2 "$RC"
assert_equal "the refusal prints no program, so the changed opener cannot be graded as a pass" "" "$OUT"
assert_contains "the refusal names the opening line it looked for" "$OPENER" "$(cat "$TMPDIR/err-5.txt")"

RESTORED_OPENER="$TMPDIR/opener-restored.md"
rewrite_line "$NO_OPENER" "$OPENER_LINE" "$OPENER" "$RESTORED_OPENER"
RC=0
OUT=$(merge_program_from "$RESTORED_OPENER" 2>/dev/null) || RC=$?
assert_exit "restoring only the opener makes the same document extract again, so Test 5 refused for the opener and not for another fault" 0 "$RC"

echo "=== Test 6: a second block with the same marker refuses instead of merging two programs ==="
DUPLICATE_BLOCK="$TMPDIR/duplicate-block.md"
{
  cat "$DOC"
  printf '\n%s\n' "$MARKER"
  printf "%s\n" "$OPENER" '  .' "' other.json > other.json" '```'
} > "$DUPLICATE_BLOCK"
RC=0
OUT=$(merge_program_from "$DUPLICATE_BLOCK" 2>"$TMPDIR/err-6.txt") || RC=$?
assert_exit "two blocks with one marker refuse, because a lane that silently took the first would execute a program the reader did not choose" 2 "$RC"
assert_equal "the refusal prints no program, so neither block is executed" "" "$OUT"
assert_contains "the refusal says the block is ambiguous" "ambiguous" "$(cat "$TMPDIR/err-6.txt")"

echo "=== Test 7: an empty marked block refuses rather than executing an empty program ==="
EMPTY_BLOCK="$TMPDIR/empty-block.md"
printf '%s\n%s\n' "$MARKER" '```' > "$EMPTY_BLOCK"
RC=0
OUT=$(merge_program_from "$EMPTY_BLOCK" 2>"$TMPDIR/err-7.txt") || RC=$?
assert_exit "an empty marked block refuses, because an empty jq program would return the input unchanged and pass" 2 "$RC"
assert_contains "the refusal says no program was extracted" "no jq program was extracted" "$(cat "$TMPDIR/err-7.txt")"

echo "=== Test 8: a line the mutation cases must name once refuses when the document carries it twice ==="
TWO_MARKERS="$TMPDIR/two-markers.md"
printf '%s\n%s\n%s\n%s\n' "$MARKER" '```' "$MARKER" '```' > "$TWO_MARKERS"
RC=0
OUT=$(line_of "$MARKER" "$TWO_MARKERS" 2>"$TMPDIR/err-8.txt") || RC=$?
assert_exit "a marker that appears twice refuses, because a mutation case that names the wrong line would red for a reason it did not test" 2 "$RC"
assert_equal "the refusal prints no line number, so no mutation case runs against a guessed line" "" "$OUT"
assert_contains "the refusal reports how many times the line was found" "appears 2 times" "$(cat "$TMPDIR/err-8.txt")"

ONE_MARKER="$TMPDIR/one-marker.md"
printf '%s\n%s\n' "$MARKER" '```' > "$ONE_MARKER"
RC=0
OUT=$(line_of "$MARKER" "$ONE_MARKER" 2>/dev/null) || RC=$?
assert_exit "removing only the second marker makes the same lookup succeed, so Test 8 refused for the count and not for another fault" 0 "$RC"
assert_equal "the successful lookup returns the line the marker is on" "1" "$OUT"

echo ""
echo "Results: $PASS passed, $FAIL failed of $((PASS + FAIL))"
[ "$FAIL" -eq 0 ] || exit 1
