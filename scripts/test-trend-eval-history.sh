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

# --- helpers -----------------------------------------------------------------

# Set a bean's created_at frontmatter for deterministic oldest->newest ordering.
set_created() {
  local path="$1" ts="$2" f
  f=$(ls "$path"/${3}*.md)
  sed -i.bak "s/^created_at: .*/created_at: $ts/" "$f" && rm -f "$f.bak"
}

# Append an iteration whose scorecard has the given per-dimension scores.
# usage: append_iter <beans-path> <bean-id> <iter> <dispatches> <cq> <corr> <dsf> [disagreements-json]
append_iter() {
  local bp="$1" id="$2" iter="$3" disp="$4" cq="$5" corr="$6" dsf="$7" disag="${8:-}"
  local sc="$TMP/sc-$id-$iter.json"
  cat > "$sc" <<EOF
{"domains":{"general":{"dimensions":{"code_quality":{"score":$cq,"threshold":6},"correctness":{"score":$corr,"threshold":7},"domain_spec_fidelity":{"score":$dsf,"threshold":8}}}},"criteria":[]}
EOF
  local args=(--bean-id "$id" --iteration "$iter" --scorecard "$sc" --dispatches "$disp" --guidance "")
  if [[ -n "$disag" ]]; then
    local df="$TMP/dis-$id-$iter.json"
    echo "$disag" > "$df"
    args+=(--disagreements "$df")
  fi
  BEANS_PATH="$bp" "$SCRIPT_DIR/append-eval-log.sh" "${args[@]}"
}

mkbean() { # <beans-path> <title> <type> [parent] -> id
  local bp="$1" title="$2" typ="$3" parent="${4:-}"
  local pargs=()
  [[ -n "$parent" ]] && pargs=(--parent "$parent")
  beans create "$title" --beans-path "$bp" -t "$typ" -s completed "${pargs[@]}" --json 2>/dev/null | jq -r '.bean.id // .id'
}

# =============================================================================
echo "Scenario A: two epics, newest declines -> alarm true"
BP="$TMP/a"; mkdir -p "$BP"; beans init --beans-path "$BP" >/dev/null 2>&1

E1=$(mkbean "$BP" "Epic One" epic)
E2=$(mkbean "$BP" "Epic Two" epic)
T1=$(mkbean "$BP" "Task 1" task "$E1")
T2=$(mkbean "$BP" "Task 2" task "$E1")
T3=$(mkbean "$BP" "Task 3" task "$E2")
T4=$(mkbean "$BP" "Task 4" task "$E2")
# Deterministic ordering: E1 older than E2, tasks whenever.
set_created "$BP" "2026-01-01T00:00:00Z" "$E1"
set_created "$BP" "2026-02-01T00:00:00Z" "$E2"

for t in "$T1" "$T2" "$T3" "$T4"; do
  BEANS_PATH="$BP" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$t" --init --base-sha "sha_$t"
done
# E1: strong — few dispatches, high scores, no disagreements.
append_iter "$BP" "$T1" 1 2 9 9 9
append_iter "$BP" "$T2" 1 2 9 9 9
# E2: weaker — more dispatches, lower scores, disagreements present.
append_iter "$BP" "$T3" 1 3 7 7 7
append_iter "$BP" "$T3" 2 3 6 6 6 '[{"domain":"general","dimension":"correctness","spread":3,"scores":{"claude":9,"codex":6}}]'
append_iter "$BP" "$T4" 1 3 6 6 6 '[{"domain":"general","dimension":"code_quality","spread":2,"scores":{"claude":8,"codex":6}}]'

OUT=$("$SCRIPT_DIR/trend-eval-history.sh" --beans-path "$BP")
assert_eq "two epics aggregated" "2" "$(echo "$OUT" | jq '.epics | length')"
assert_eq "epics ordered oldest first" "$E1" "$(echo "$OUT" | jq -r '.epics[0].epic')"
assert_eq "epics ordered newest last" "$E2" "$(echo "$OUT" | jq -r '.epics[1].epic')"
assert_eq "E1 task_count" "2" "$(echo "$OUT" | jq '.epics[0].task_count')"
assert_eq "E1 mean dispatches" "2" "$(echo "$OUT" | jq '.epics[0].dispatches.mean')"
assert_eq "E2 mean dispatches higher" "4.5" "$(echo "$OUT" | jq '.epics[1].dispatches.mean')"
assert_eq "E1 correctness mean" "9" "$(echo "$OUT" | jq '.epics[0].dimensions.correctness')"
assert_eq "E2 correctness mean" "6" "$(echo "$OUT" | jq '.epics[1].dimensions.correctness')"
assert_eq "E2 disagreements counted" "2" "$(echo "$OUT" | jq '.epics[1].disagreements')"
assert_eq "E1 disagreements zero" "0" "$(echo "$OUT" | jq '.epics[0].disagreements')"
assert_eq "dispatches trend declining" "declining" "$(echo "$OUT" | jq -r '.trends[-1].dispatches.direction')"
assert_eq "correctness trend declining" "declining" "$(echo "$OUT" | jq -r '.trends[-1].dimensions.correctness.direction')"
assert_eq "disagreements trend declining" "declining" "$(echo "$OUT" | jq -r '.trends[-1].disagreements.direction')"
assert_eq "alarm raised" "true" "$(echo "$OUT" | jq '.alarm')"
assert_eq "alarm reasons non-empty" "true" "$(echo "$OUT" | jq '.alarm_reasons | length > 0')"

# =============================================================================
echo "Scenario B: improving epic -> no alarm"
BP="$TMP/b"; mkdir -p "$BP"; beans init --beans-path "$BP" >/dev/null 2>&1
E1=$(mkbean "$BP" "Epic One" epic)
E2=$(mkbean "$BP" "Epic Two" epic)
T1=$(mkbean "$BP" "Task 1" task "$E1")
T2=$(mkbean "$BP" "Task 2" task "$E2")
set_created "$BP" "2026-01-01T00:00:00Z" "$E1"
set_created "$BP" "2026-02-01T00:00:00Z" "$E2"
BEANS_PATH="$BP" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$T1" --init --base-sha s1
BEANS_PATH="$BP" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$T2" --init --base-sha s2
append_iter "$BP" "$T1" 1 4 6 6 6
append_iter "$BP" "$T2" 1 2 9 9 9
OUT=$("$SCRIPT_DIR/trend-eval-history.sh" --beans-path "$BP")
assert_eq "no alarm when improving" "false" "$(echo "$OUT" | jq '.alarm')"
assert_eq "dispatches trend improving" "improving" "$(echo "$OUT" | jq -r '.trends[-1].dispatches.direction')"
assert_eq "correctness trend improving" "improving" "$(echo "$OUT" | jq -r '.trends[-1].dimensions.correctness.direction')"

# =============================================================================
echo "Scenario C: fewer than 2 epics with data -> trends null, alarm false"
BP="$TMP/c"; mkdir -p "$BP"; beans init --beans-path "$BP" >/dev/null 2>&1
E1=$(mkbean "$BP" "Epic One" epic)
T1=$(mkbean "$BP" "Task 1" task "$E1")
BEANS_PATH="$BP" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$T1" --init --base-sha s1
append_iter "$BP" "$T1" 1 3 8 8 8
EXIT_CODE=0
OUT=$("$SCRIPT_DIR/trend-eval-history.sh" --beans-path "$BP") || EXIT_CODE=$?
assert_eq "single-epic exit 0" "0" "$EXIT_CODE"
assert_eq "single epic aggregated" "1" "$(echo "$OUT" | jq '.epics | length')"
assert_eq "trends null with one epic" "null" "$(echo "$OUT" | jq '.trends')"
assert_eq "alarm false with one epic" "false" "$(echo "$OUT" | jq '.alarm')"

# =============================================================================
echo "Scenario D: tasks without eval data are skipped"
BP="$TMP/d"; mkdir -p "$BP"; beans init --beans-path "$BP" >/dev/null 2>&1
E1=$(mkbean "$BP" "Epic One" epic)
E2=$(mkbean "$BP" "Epic Two" epic)
T1=$(mkbean "$BP" "Task 1" task "$E1")       # has data
T2=$(mkbean "$BP" "Task 2" task "$E1")       # no eval log at all
T3=$(mkbean "$BP" "Task 3" task "$E2")       # init only, no iterations
set_created "$BP" "2026-01-01T00:00:00Z" "$E1"
set_created "$BP" "2026-02-01T00:00:00Z" "$E2"
BEANS_PATH="$BP" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$T1" --init --base-sha s1
append_iter "$BP" "$T1" 1 3 8 8 8
BEANS_PATH="$BP" "$SCRIPT_DIR/append-eval-log.sh" --bean-id "$T3" --init --base-sha s3
OUT=$("$SCRIPT_DIR/trend-eval-history.sh" --beans-path "$BP")
assert_eq "only epic with iterations present" "1" "$(echo "$OUT" | jq '.epics | length')"
assert_eq "epic keeps only the task with data" "1" "$(echo "$OUT" | jq '.epics[0].task_count')"
assert_eq "no-data run trends null" "null" "$(echo "$OUT" | jq '.trends')"

# =============================================================================
echo "Scenario E: empty beans dir -> graceful empty output"
BP="$TMP/e"; mkdir -p "$BP"; beans init --beans-path "$BP" >/dev/null 2>&1
EXIT_CODE=0
OUT=$("$SCRIPT_DIR/trend-eval-history.sh" --beans-path "$BP") || EXIT_CODE=$?
assert_eq "empty dir exit 0" "0" "$EXIT_CODE"
assert_eq "empty dir zero epics" "0" "$(echo "$OUT" | jq '.epics | length')"
assert_eq "empty dir trends null" "null" "$(echo "$OUT" | jq '.trends')"
assert_eq "empty dir alarm false" "false" "$(echo "$OUT" | jq '.alarm')"

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ] || exit 1
