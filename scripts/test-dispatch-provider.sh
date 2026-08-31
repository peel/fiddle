#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PASS=0; FAIL=0

assert_contains() {
  local desc="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -qF -- "$needle"; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (missing '$needle')"
  fi
}

assert_not_contains() {
  local desc="$1" needle="$2" haystack="$3"
  if echo "$haystack" | grep -qF -- "$needle"; then
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (unexpected '$needle')"
  else
    PASS=$((PASS+1)); echo "  PASS: $desc"
  fi
}

assert_exit() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected exit $expected, got $actual)"
  fi
}

assert_eq() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"
  fi
}

assert_json() {
  local desc="$1" filter="$2" expected="$3" doc="$4"
  local actual
  actual=$(echo "$doc" | jq -r "$filter" 2>/dev/null) || actual="<unparseable>"
  assert_eq "$desc" "$expected" "$actual"
}

TMPDIR_T=$(mktemp -d)
trap "rm -rf $TMPDIR_T" EXIT

mkdir -p "$TMPDIR_T/hooks" "$TMPDIR_T/scripts" "$TMPDIR_T/skills/develop" "$TMPDIR_T/bin"
cp "$REPO_DIR/hooks/dispatch-provider.sh" "$TMPDIR_T/hooks/"
cp "$REPO_DIR/scripts/build-scorecard-schema.sh" "$TMPDIR_T/scripts/"
cp "$REPO_DIR/skills/develop/provider-context.md" "$TMPDIR_T/skills/develop/"
DISPATCH="$TMPDIR_T/hooks/dispatch-provider.sh"
BUILDER="$TMPDIR_T/scripts/build-scorecard-schema.sh"

ARGV_LOG="$TMPDIR_T/argv.log"
SCHEMA_COPY="$TMPDIR_T/schema-seen.json"

cat > "$TMPDIR_T/bin/fake-codex" << 'EOF'
#!/usr/bin/env bash
cat > /dev/null
: > "$FAKE_ARGV"
for a in "$@"; do printf '%s\n' "$a" >> "$FAKE_ARGV"; done
OUT=""; SCHEMA=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -o) OUT="${2:-}"; shift 2 ;;
    --output-schema) SCHEMA="${2:-}"; shift 2 ;;
    *) shift ;;
  esac
done
rm -f "$FAKE_SCHEMA_COPY"
[[ -n "$SCHEMA" ]] && cp "$SCHEMA" "$FAKE_SCHEMA_COPY"
printf '%s\n' "$FAKE_RAW_STREAM"
if [[ -n "$OUT" && "${FAKE_MESSAGE:-none}" != "none" ]]; then
  cp "$FAKE_MESSAGE" "$OUT"
fi
exit "${FAKE_EXIT:-0}"
EOF
chmod +x "$TMPDIR_T/bin/fake-codex"
export PATH="$TMPDIR_T/bin:$PATH"
export FAKE_ARGV="$ARGV_LOG"
export FAKE_SCHEMA_COPY="$SCHEMA_COPY"
export FAKE_RAW_STREAM="codex raw output line"

cat > "$TMPDIR_T/orchestrate.json" << 'EOF'
{"providers":{"codexlike":{"command":"fake-codex",
                           "flags":"-s read-only",
                           "extract":"codex-last-message",
                           "schema_roles":{"evaluator":"evaluator",
                                           "holistic-reviewer":"holistic"}}}}
EOF

CARD='{"provider":"codex","task_id":"fiddle-obpq","iteration":1,"timestamp":"2026-08-27T10:00:00Z","domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"gate.sh ran 72 of 72 binaries"}}}},"criteria":[{"id":"c1","pass":true,"evidence":"dispatch-provider.sh:118 passes -o"}],"antipatterns_detected":[],"spec_defect":null,"guidance":"","dispatch_count":1}'
printf '%s' "$CARD" > "$TMPDIR_T/card-whole.json"
printf '%s' "${CARD%\}}" > "$TMPDIR_T/card-truncated.json"
printf '   \n\t\n' > "$TMPDIR_T/card-blank.json"
printf 'The design reads well and the lane covers the truncation case.\n' > "$TMPDIR_T/prose.txt"

dispatch_capture() {
  local exit_var="$1" out_var="$2" err_var="$3"; shift 3
  local rc=0
  "$DISPATCH" "$@" > "$TMPDIR_T/stdout.txt" 2> "$TMPDIR_T/stderr.txt" || rc=$?
  printf -v "$exit_var" '%s' "$rc"
  printf -v "$out_var" '%s' "$(cat "$TMPDIR_T/stdout.txt")"
  printf -v "$err_var" '%s' "$(cat "$TMPDIR_T/stderr.txt")"
}

EVAL_ARGS=(codexlike --role evaluator --topic "Evaluate domain: general" --instructions i
           --domain general --dimensions correctness,domain_spec_fidelity,code_quality)

echo "Test 1: a whole scorecard in the last-message file reaches stdout"
export FAKE_MESSAGE="$TMPDIR_T/card-whole.json"
dispatch_capture EXIT_CODE OUTPUT STDERR "${EVAL_ARGS[@]}"
assert_exit "whole card -> exit 0" 0 "$EXIT_CODE"
assert_json "stdout is the scorecard" '.criteria[0].id' "c1" "$OUTPUT"
assert_json "stdout keeps the dimension score" '.domains.general.dimensions.correctness.score' "8" "$OUTPUT"

echo "Test 2: a scorecard one closing brace short is refused, not returned"
export FAKE_MESSAGE="$TMPDIR_T/card-truncated.json"
dispatch_capture EXIT_CODE OUTPUT STDERR "${EVAL_ARGS[@]}"
assert_exit "truncated card -> exit 2" 2 "$EXIT_CODE"
assert_eq "no partial answer reaches stdout" "" "$OUTPUT"
assert_contains "stderr names the schema and the role" "under a schema" "$STDERR"
assert_contains "stderr shows the answer for diagnosis" '"task_id":"fiddle-obpq"' "$STDERR"

echo "Test 3: the same bytes with the closing brace restored are returned"
assert_eq "the two fixtures differ by exactly one byte" 1 \
  "$(( $(wc -c < "$TMPDIR_T/card-whole.json") - $(wc -c < "$TMPDIR_T/card-truncated.json") ))"
export FAKE_MESSAGE="$TMPDIR_T/card-whole.json"
dispatch_capture EXIT_CODE OUTPUT STDERR "${EVAL_ARGS[@]}"
assert_exit "restored card -> exit 0" 0 "$EXIT_CODE"
assert_json "restored card parses" '.provider' "codex" "$OUTPUT"

echo "Test 4: a provider that writes no last-message file is refused"
export FAKE_MESSAGE="none"
dispatch_capture EXIT_CODE OUTPUT STDERR "${EVAL_ARGS[@]}"
assert_exit "absent message file -> exit 2" 2 "$EXIT_CODE"
assert_eq "nothing reaches stdout" "" "$OUTPUT"
assert_contains "stderr names the absent file" "wrote no last-message file" "$STDERR"
assert_contains "stderr carries the raw output for diagnosis" "codex raw output line" "$STDERR"

echo "Test 5: a whitespace-only last message is refused"
export FAKE_MESSAGE="$TMPDIR_T/card-blank.json"
dispatch_capture EXIT_CODE OUTPUT STDERR "${EVAL_ARGS[@]}"
assert_exit "blank message file -> exit 2" 2 "$EXIT_CODE"
assert_eq "nothing reaches stdout" "" "$OUTPUT"
assert_contains "stderr names the empty message" "wrote an empty last message" "$STDERR"

echo "Test 6: the evaluator dispatch asks for a file and a schema"
export FAKE_MESSAGE="$TMPDIR_T/card-whole.json"
dispatch_capture EXIT_CODE OUTPUT STDERR "${EVAL_ARGS[@]}"
ARGV=$(cat "$ARGV_LOG")
assert_contains "provider is invoked with -o" "-o" "$ARGV"
assert_contains "provider is invoked with --output-schema" "--output-schema" "$ARGV"
SEEN=$(cat "$SCHEMA_COPY")
assert_json "schema closes the general domain" '.properties.domains.required | join(",")' "general" "$SEEN"
assert_json "schema names the three dimensions" \
  '.properties.domains.properties.general.properties.dimensions.required | sort | join(",")' \
  "code_quality,correctness,domain_spec_fidelity" "$SEEN"
assert_json "schema closes the top-level object" '.additionalProperties' "false" "$SEEN"
assert_json "schema requires non-empty criterion evidence" \
  '.properties.criteria.items.properties.evidence.minLength' "1" "$SEEN"

echo "Test 7: the holistic-reviewer dispatch gets the holistic schema without domain flags"
dispatch_capture EXIT_CODE OUTPUT STDERR codexlike --role holistic-reviewer \
  --topic "Holistic review" --instructions i
assert_exit "holistic dispatch -> exit 0" 0 "$EXIT_CODE"
SEEN=$(cat "$SCHEMA_COPY")
assert_json "schema closes the holistic domain" '.properties.domains.required | join(",")' "holistic" "$SEEN"
assert_json "schema names the five holistic dimensions" \
  '.properties.domains.properties.holistic.properties.dimensions.required | sort | join(",")' \
  "coherence,holistic_spec_fidelity,integration,polish,runtime_health" "$SEEN"
assert_json "holistic schema carries the coverage matrix" \
  '.properties.spec_coverage_matrix.items.properties.coverage.enum | join(",")' "Full,Weak,Missing" "$SEEN"
assert_json "holistic schema carries remediation beans" \
  '.properties.remediation_beans.items.required | sort | join(",")' \
  "description,requirement,source,title" "$SEEN"

echo "Test 8: a role that returns prose gets no schema, and its prose is returned"
export FAKE_MESSAGE="$TMPDIR_T/prose.txt"
rm -f "$SCHEMA_COPY"
dispatch_capture EXIT_CODE OUTPUT STDERR codexlike --role "Research analyst" \
  --topic "Research options" --instructions i
assert_exit "prose role -> exit 0" 0 "$EXIT_CODE"
ARGV=$(cat "$ARGV_LOG")
assert_contains "prose role still reads its answer from a file" "-o" "$ARGV"
assert_not_contains "prose role gets no --output-schema" "--output-schema" "$ARGV"
if [ -f "$SCHEMA_COPY" ]; then
  FAIL=$((FAIL+1)); echo "  FAIL: no schema file is handed to a prose role"
else
  PASS=$((PASS+1)); echo "  PASS: no schema file is handed to a prose role"
fi
assert_contains "the prose answer is returned" "The design reads well" "$OUTPUT"

echo "Test 9: a schema role with no --dimensions refuses before the provider runs"
export FAKE_MESSAGE="$TMPDIR_T/card-whole.json"
rm -f "$ARGV_LOG"
dispatch_capture EXIT_CODE OUTPUT STDERR codexlike --role evaluator \
  --topic "Evaluate domain: general" --instructions i --domain general
assert_exit "missing --dimensions -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr names the missing flag" "needs --dimensions" "$STDERR"
if [ -f "$ARGV_LOG" ]; then
  FAIL=$((FAIL+1)); echo "  FAIL: the provider is not dispatched when the schema cannot be built"
else
  PASS=$((PASS+1)); echo "  PASS: the provider is not dispatched when the schema cannot be built"
fi

echo "Test 10: an empty --dimensions builds the evidence-only schema"
dispatch_capture EXIT_CODE OUTPUT STDERR codexlike --role evaluator \
  --topic "Evaluate domain: general" --instructions i --domain general --dimensions ""
assert_exit "evidence-only dispatch -> exit 0" 0 "$EXIT_CODE"
SEEN=$(cat "$SCHEMA_COPY")
assert_json "evidence-only schema scores no dimension" \
  '.properties.domains.properties.general.properties.dimensions.required | length' "0" "$SEEN"
assert_json "evidence-only schema forces the mode declaration" \
  '.properties.mode.enum | join(",")' "evidence-only" "$SEEN"
assert_json "a scored schema declares no mode" '.properties.mode' "null" \
  "$($BUILDER --profile evaluator --domain general --dimensions correctness)"

echo "Test 11: the answer comes from the message file, not from the raw stream"
export FAKE_RAW_STREAM='{"type":"item.completed","item":{"type":"agent_message","text":"{\"provider\":\"stream-scrape\"}"}}'
export FAKE_MESSAGE="$TMPDIR_T/card-whole.json"
dispatch_capture EXIT_CODE OUTPUT STDERR "${EVAL_ARGS[@]}"
assert_exit "stream and file disagree -> exit 0" 0 "$EXIT_CODE"
assert_json "the file's answer wins" '.provider' "codex" "$OUTPUT"
assert_not_contains "no scraped stream answer reaches stdout" "stream-scrape" "$OUTPUT"
export FAKE_RAW_STREAM="codex raw output line"

echo "Test 12: a provider exit code survives a good answer"
export FAKE_EXIT=7
dispatch_capture EXIT_CODE OUTPUT STDERR "${EVAL_ARGS[@]}"
assert_exit "provider exit 7 is reported" 7 "$EXIT_CODE"
assert_json "the answer is still returned" '.provider' "codex" "$OUTPUT"
unset FAKE_EXIT

echo "Test 13: the schema builder refuses what it cannot express"
EXIT_CODE=0; ERR=$("$BUILDER" --profile reviewer 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "unknown profile -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr names the profile" "unknown profile 'reviewer'" "$ERR"

EXIT_CODE=0; ERR=$("$BUILDER" --profile holistic --dimensions integration 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "holistic with --dimensions -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr says the holistic dimensions are fixed" "are fixed" "$ERR"

EXIT_CODE=0; ERR=$("$BUILDER" --profile evaluator --dimensions correctness 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "evaluator with no --domain -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr names the missing domain" "--domain is required" "$ERR"

EXIT_CODE=0; ERR=$("$BUILDER" --profile evaluator --domain general --dimensions "a, b" 2>&1 >/dev/null) || EXIT_CODE=$?
assert_exit "a spaced dimension list -> exit 2" 2 "$EXIT_CODE"
assert_contains "stderr names the spaces" "must not contain spaces" "$ERR"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
