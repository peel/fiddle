#!/usr/bin/env bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PASS=0; FAIL=0

assert_eq() {
  local desc="$1" expected="$2" actual="$3"
  if [ "$expected" = "$actual" ]; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc (expected '$expected', got '$actual')"
  fi
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
OUT="$TMP/out.json"

converge() {
  local exit_code=0
  "$SCRIPT_DIR/check-convergence.sh" --current "$TMP/current.json" --history "$TMP/history.json" \
    --max-dispatches 60 --current-dispatches 4 > "$OUT" 2>/dev/null || exit_code=$?
  echo "$exit_code"
}

status_of_out() { jq -r '.status' "$OUT"; }

verdict_pass() {
  local tree="$1" fidelity="$2" findings="${3:-[]}"
  jq -n --arg tree "$tree" --argjson fidelity "$fidelity" --argjson findings "$findings" '{
    verdict: "PASS",
    tree_sha: $tree,
    failing_dimensions: [],
    failing_criteria: [],
    passing_dimensions: [
      {domain: "general", dimension: "correctness", score: 8, threshold: 7},
      {domain: "general", dimension: "domain_spec_fidelity", score: $fidelity, threshold: 8}
    ],
    dimensions: {"general.correctness": 8, "general.domain_spec_fidelity": $fidelity},
    findings: $findings
  } | if .tree_sha == "" then del(.tree_sha) else . end'
}

verdict_fail_criterion() {
  local tree="$1" criterion="$2"
  jq -n --arg tree "$tree" --arg criterion "$criterion" '{
    verdict: "FAIL",
    tree_sha: $tree,
    failing_dimensions: [],
    failing_criteria: [$criterion],
    passing_dimensions: [{domain: "general", dimension: "correctness", score: 8, threshold: 7}],
    dimensions: {"general.correctness": 8},
    findings: []
  } | if .tree_sha == "" then del(.tree_sha) else . end'
}

verdict_fail_dimension() {
  local tree="$1" score="$2"
  jq -n --arg tree "$tree" --argjson score "$score" '{
    verdict: "FAIL",
    tree_sha: $tree,
    failing_dimensions: [{domain: "general", dimension: "domain_spec_fidelity", score: $score, threshold: 8}],
    failing_criteria: [],
    passing_dimensions: [{domain: "general", dimension: "correctness", score: 8, threshold: 7}],
    dimensions: {"general.correctness": 8, "general.domain_spec_fidelity": $score},
    findings: []
  }'
}

echo "Case 1: the 5swi pair — one evaluator scores a byte-identical tree lower"
verdict_pass 8c5dc5a 9 > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass 8c5dc5a 8 > "$TMP/current.json"
assert_eq "same tree, lower score → exit 0" "0" "$(converge)"
assert_eq "same tree, lower score → CONVERGED" "CONVERGED" "$(status_of_out)"
assert_eq "same tree reported as unchanged" "unchanged" "$(jq -r '.tree_comparison' "$OUT")"
assert_eq "the score drop is recorded, not acted on" "general.domain_spec_fidelity" \
  "$(jq -r '.ignored_score_deltas[0].dimension' "$OUT")"
assert_eq "the recorded delta keeps both numbers" "9 8" \
  "$(jq -r '.ignored_score_deltas[0] | "\(.previous) \(.current)"' "$OUT")"

echo "Case 2: the same score drop across a changed tree still blocks"
verdict_pass 8c5dc5a 9 > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass 46b1b98 8 > "$TMP/current.json"
assert_eq "changed tree, lower score → exit 1" "1" "$(converge)"
assert_eq "changed tree, lower score → PASS_REGRESSED" "PASS_REGRESSED" "$(status_of_out)"
assert_eq "the regressed dimension is named" "general.domain_spec_fidelity" "$(jq -r '.regressions[0]' "$OUT")"

echo "Case 3: a tree that cannot be compared keeps the guard on"
verdict_pass "" 9 > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass "" 8 > "$TMP/current.json"
assert_eq "no shas recorded → PASS_REGRESSED" "PASS_REGRESSED" "$(converge >/dev/null; status_of_out)"
assert_eq "no shas recorded → tree_comparison unknown" "unknown" "$(jq -r '.tree_comparison' "$OUT")"

verdict_pass "" 9 > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass 8c5dc5a 8 > "$TMP/current.json"
assert_eq "history predating the sha → PASS_REGRESSED" "PASS_REGRESSED" "$(converge >/dev/null; status_of_out)"

echo "Case 4: a criterion that passed and now fails on the same tree is contested"
verdict_pass 8c5dc5a 9 > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_fail_criterion 8c5dc5a the_excused_set_is_empty_in_production > "$TMP/current.json"
assert_eq "same tree, criterion flipped → exit 2" "2" "$(converge)"
assert_eq "same tree, criterion flipped → CONTESTED" "CONTESTED" "$(status_of_out)"
assert_eq "the contested criterion is named" "the_excused_set_is_empty_in_production" \
  "$(jq -r '.contested_criteria[0]' "$OUT")"

echo "Case 5: a dimension the previous pass cleared, now below threshold on the same tree"
verdict_pass 8c5dc5a 9 > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_fail_dimension 8c5dc5a 6 > "$TMP/current.json"
assert_eq "same tree, dimension flipped → CONTESTED" "CONTESTED" "$(converge >/dev/null; status_of_out)"
assert_eq "the contested dimension is named" "general.domain_spec_fidelity" \
  "$(jq -r '.contested_dimensions[0]' "$OUT")"

echo "Case 6: a criterion failing on a changed tree is ordinary remediation"
verdict_pass 8c5dc5a 9 > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_fail_criterion 46b1b98 the_excused_set_is_empty_in_production > "$TMP/current.json"
assert_eq "changed tree, criterion fails → exit 1" "1" "$(converge)"
assert_eq "changed tree, criterion fails → FAIL" "FAIL" "$(status_of_out)"

echo "Case 7: two failures in a row on one tree contradict nothing"
verdict_fail_criterion 8c5dc5a the_excused_set_is_empty_in_production > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_fail_criterion 8c5dc5a the_excused_set_is_empty_in_production > "$TMP/current.json"
assert_eq "same tree, failure repeated → FAIL" "FAIL" "$(converge >/dev/null; status_of_out)"

echo "Case 8: a new finding above low on the same tree is contested"
verdict_pass 8c5dc5a 9 '[]' > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass 8c5dc5a 9 '[{"id":"stub_implementation","severity":"high"}]' > "$TMP/current.json"
assert_eq "same tree, new finding → exit 2" "2" "$(converge)"
assert_eq "same tree, new finding → CONTESTED" "CONTESTED" "$(status_of_out)"
assert_eq "the new finding is named" "stub_implementation" "$(jq -r '.new_findings[0].id' "$OUT")"

echo "Case 9: a new finding of low severity does not contest a pass"
verdict_pass 8c5dc5a 9 '[]' > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass 8c5dc5a 8 '[{"id":"naming_nit","severity":"low"}]' > "$TMP/current.json"
assert_eq "same tree, low finding → CONVERGED" "CONVERGED" "$(converge >/dev/null; status_of_out)"

echo "Case 10: a finding of unstated severity counts until it is stated low"
verdict_pass 8c5dc5a 9 '[]' > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass 8c5dc5a 9 '[{"id":"unspecified_finding"}]' > "$TMP/current.json"
assert_eq "same tree, unstated severity → CONTESTED" "CONTESTED" "$(converge >/dev/null; status_of_out)"

echo "Case 11: a finding both evaluators already reported is not new"
verdict_pass 8c5dc5a 9 '[{"id":"stub_implementation","severity":"high"}]' > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass 8c5dc5a 8 '[{"id":"stub_implementation","severity":"high"}]' > "$TMP/current.json"
assert_eq "same tree, finding repeated → CONVERGED" "CONVERGED" "$(converge >/dev/null; status_of_out)"

echo "Case 12: two clean passes still converge, on either tree comparison"
verdict_pass 8c5dc5a 9 > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass 8c5dc5a 9 > "$TMP/current.json"
assert_eq "same tree, agreeing passes → exit 0" "0" "$(converge)"
assert_eq "same tree, agreeing passes → CONVERGED" "CONVERGED" "$(status_of_out)"

verdict_pass 8c5dc5a 9 > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass 46b1b98 9 > "$TMP/current.json"
assert_eq "changed tree, agreeing passes → exit 0" "0" "$(converge)"
assert_eq "changed tree, agreeing passes → CONVERGED" "CONVERGED" "$(status_of_out)"
assert_eq "changed tree reported as changed" "changed" "$(jq -r '.tree_comparison' "$OUT")"

verdict_pass 8c5dc5a 9 > "$TMP/prior.json"
jq -s '.' "$TMP/prior.json" > "$TMP/history.json"
verdict_pass 46b1b98 10 > "$TMP/current.json"
assert_eq "changed tree, improved score → CONVERGED" "CONVERGED" "$(converge >/dev/null; status_of_out)"

echo "Case 13: check-thresholds stamps the tree it graded"
cat > "$TMP/scorecard.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"e"}}}},
 "criteria":[{"id":"c1","pass":true,"evidence":"e"}],
 "antipatterns_detected":[{"id":"stub_implementation","severity":"high"}]}
EOF
jq -c '.criteria' "$TMP/scorecard.json" > "$TMP/criteria.json"
STAMPED=$("$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMP/scorecard.json" \
  --criteria "$TMP/criteria.json" --tree-sha 8c5dc5a)
assert_eq "the verdict carries the tree sha" "8c5dc5a" "$(echo "$STAMPED" | jq -r '.tree_sha')"
assert_eq "the verdict carries the scorecard findings" "stub_implementation" \
  "$(echo "$STAMPED" | jq -r '.findings[0].id')"
assert_eq "the finding keeps its severity" "high" "$(echo "$STAMPED" | jq -r '.findings[0].severity')"
UNSTAMPED=$("$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMP/scorecard.json" --criteria "$TMP/criteria.json")
assert_eq "no sha given, no sha claimed" "false" "$(echo "$UNSTAMPED" | jq -c 'has("tree_sha")')"

echo "Case 14: a bare string antipattern becomes a finding of unstated severity"
cat > "$TMP/bare.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7,"evidence":"e"}}}},
 "criteria":[{"id":"c1","pass":true,"evidence":"e"}],
 "antipatterns_detected":["stub_implementation"]}
EOF
jq -c '.criteria' "$TMP/bare.json" > "$TMP/bare-criteria.json"
BARE=$("$SCRIPT_DIR/check-thresholds.sh" --scorecard "$TMP/bare.json" --criteria "$TMP/bare-criteria.json")
assert_eq "the bare id survives" "stub_implementation" "$(echo "$BARE" | jq -r '.findings[0].id')"
assert_eq "its severity is unspecified" "unspecified" "$(echo "$BARE" | jq -r '.findings[0].severity')"

echo "Case 15: the eval log shows every dispatch, its tree and its verdict"
LOGDIR="$TMP/beans"
beans init --beans-path "$LOGDIR" >/dev/null 2>&1
LOG_BEAN=$(beans create "Tree identity log" --beans-path "$LOGDIR" -t task -s in-progress --json 2>/dev/null | jq -r '.bean.id // .id')
cat > "$TMP/log-scorecard.json" << 'EOF'
{"domains":{"general":{"dimensions":{"correctness":{"score":8,"threshold":7}}}},"criteria":[]}
EOF
log_iteration() {
  BEANS_PATH="$LOGDIR" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$LOG_BEAN" \
    --iteration "$1" --scorecard "$TMP/log-scorecard.json" --dispatches "$2" \
    --tree-sha "$3" --convergence "$4"
}
BEANS_PATH="$LOGDIR" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$LOG_BEAN" --init --base-sha 8c5dc5a
log_iteration 1 2 8c5dc5a PASS_PENDING
log_iteration 2 1 8c5dc5a CONVERGED
LOG_BODY=$(beans show "$LOG_BEAN" --beans-path "$LOGDIR" --json 2>/dev/null | jq -r '.body')
assert_eq "the tree is on the iteration entry" "2" "$(echo "$LOG_BODY" | grep -c '^tree: 8c5dc5a')"
assert_eq "the convergence verdict is on the entry" "1" "$(echo "$LOG_BODY" | grep -c '^convergence: PASS_PENDING')"
PARSED=$(BEANS_PATH="$LOGDIR" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$LOG_BEAN")
assert_eq "each dispatch count is readable per iteration" "2 1" \
  "$(echo "$PARSED" | jq -r '[.iterations[].dispatches] | join(" ")')"
assert_eq "each verdict is readable per iteration" "PASS_PENDING CONVERGED" \
  "$(echo "$PARSED" | jq -r '[.iterations[].convergence] | join(" ")')"
assert_eq "re-evaluating an unchanged tree is counted" "1" \
  "$(echo "$PARSED" | jq -r '.unchanged_tree_reevaluations')"

echo "Case 16: an eval log without trees reports no unchanged re-evaluations"
PLAINDIR="$TMP/beans-plain"
beans init --beans-path "$PLAINDIR" >/dev/null 2>&1
PLAIN_BEAN=$(beans create "Tree-free log" --beans-path "$PLAINDIR" -t task -s in-progress --json 2>/dev/null | jq -r '.bean.id // .id')
BEANS_PATH="$PLAINDIR" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$PLAIN_BEAN" --init --base-sha abc1234
BEANS_PATH="$PLAINDIR" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$PLAIN_BEAN" \
  --iteration 1 --scorecard "$TMP/log-scorecard.json" --dispatches 2
BEANS_PATH="$PLAINDIR" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$PLAIN_BEAN" \
  --iteration 2 --scorecard "$TMP/log-scorecard.json" --dispatches 2
PLAIN_PARSED=$(BEANS_PATH="$PLAINDIR" "$SCRIPT_DIR/parse-eval-log.sh" --bean-id "$PLAIN_BEAN")
assert_eq "two iterations are still listed" "2" "$(echo "$PLAIN_PARSED" | jq -r '.iterations | length')"
assert_eq "absent trees are not counted as identical" "0" \
  "$(echo "$PLAIN_PARSED" | jq -r '.unchanged_tree_reevaluations')"

echo "Case 17: the protocol the scripts implement is the one the skills describe"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
assert_contains() {
  local desc="$1" needle="$2" file="$3"
  if grep -qF -- "$needle" "$ROOT/$file"; then
    PASS=$((PASS+1)); echo "  PASS: $desc"
  else
    FAIL=$((FAIL+1)); echo "  FAIL: $desc ($needle absent from $file)"
  fi
}
CONVERGENCE_DOC="skills/develop-loop/convergence-and-recovery.md"
assert_contains "the loop doc passes the tree sha" "--tree-sha" "$CONVERGENCE_DOC"
assert_contains "the loop doc names the tree, not the commit" 'git rev-parse HEAD^{tree}' "$CONVERGENCE_DOC"
assert_contains "the loop doc lists CONTESTED among the results" "**CONTESTED** (exit 2)" "$CONVERGENCE_DOC"
assert_contains "the loop doc routes CONTESTED" "| **CONTESTED** |" "$CONVERGENCE_DOC"
assert_contains "the loop doc names the ignored deltas" "ignored_score_deltas" "$CONVERGENCE_DOC"
assert_contains "the loop doc logs the tree and the verdict" "--convergence {status}" "$CONVERGENCE_DOC"
assert_contains "the loop doc names the re-evaluation count" "unchanged_tree_reevaluations" "$CONVERGENCE_DOC"
assert_contains "holistic passes the tree sha too" "--tree-sha" "skills/develop-holistic/SKILL.md"
assert_contains "holistic routes CONTESTED" "| **CONTESTED** |" "skills/develop-holistic/SKILL.md"
assert_contains "CONTESTED is a terminal verdict for the gate" "CONTESTED / DISPATCHES_EXCEEDED" "skills/develop/SKILL.md"
assert_contains "the envelope documents the stamped tree" "tree_sha" "skills/develop/scorecard-envelope.md"
assert_contains "the envelope ties antipatterns to findings" "findings" "skills/develop/scorecard-envelope.md"

echo
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
