#!/usr/bin/env bash
set -euo pipefail

BEANS_PATH=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --beans-path) BEANS_PATH="$2"; shift 2;;
    -h|--help)
      echo "Usage: trend-eval-history.sh [--beans-path <path>]" >&2
      exit 0;;
    *) echo "Unknown arg: $1" >&2; exit 2;;
  esac
done

BEANS_ARGS=()
[[ -n "$BEANS_PATH" ]] && BEANS_ARGS=(--beans-path "$BEANS_PATH")

command -v beans >/dev/null 2>&1 || { echo "beans CLI not found" >&2; exit 2; }
command -v jq >/dev/null 2>&1 || { echo "jq not found" >&2; exit 2; }

ALL=$(beans list ${BEANS_ARGS[@]+"${BEANS_ARGS[@]}"} --json --full 2>/dev/null || echo '[]')

EPICMAP=$(echo "$ALL" | jq -r '
  (reduce .[] as $b ({}; . + {($b.id): {type: $b.type, parent: ($b.parent // "")}})) as $m
  | def resolve($id):
      {cur: $id, depth: 0, epic: ""}
      | until(.epic != "" or .cur == "" or .depth >= 32;
          ($m[.cur]) as $n
          | if $n == null then .cur = ""
            elif $n.type == "epic" then .epic = .cur
            else .cur = $n.parent | .depth += 1 end)
      | .epic;
  .[] | [.id, resolve(.id)] | @tsv')

resolve_epic() {
  printf '%s\n' "$EPICMAP" | awk -F'\t' -v id="$1" '$1 == id {print $2; exit}'
}

parse_eval_body() {
  local body iters disp disagree reevals lastsec dims
  body="$(cat)"
  echo "$body" | grep -q '## Evaluation Log' || return 0
  iters=$(echo "$body" | grep -c '### Iteration ' || true)
  [[ "$iters" -ge 1 ]] || return 0
  disp=$(echo "$body" | sed -n 's/^total_dispatches: \([0-9][0-9]*\).*/\1/p' | tail -1)
  disp="${disp:-0}"
  disagree=$(echo "$body" | grep -cE '^- .*: spread [0-9]' || true)
  reevals=$(echo "$body" | sed -n 's/^unchanged_tree_reevaluations: \([0-9][0-9]*\).*/\1/p' | tail -1)
  reevals="${reevals:-0}"
  lastsec=$(echo "$body" | awk '/^### Iteration /{buf=$0"\n";cap=1;next} /^### /{cap=0;next} cap{buf=buf $0"\n"} END{printf "%s", buf}')
  dims=$(echo "$lastsec" | sed -n 's/^- \([A-Za-z_][A-Za-z0-9_]*\): \([0-9][0-9]*\)\/10.*/{"\1":\2}/p' | jq -s 'add // {}')
  [[ -n "$dims" ]] || dims='{}'
  jq -n \
    --argjson disp "$disp" \
    --argjson iters "$iters" \
    --argjson disagree "$disagree" \
    --argjson reevals "$reevals" \
    --argjson dims "$dims" \
    '{dispatches:$disp, iterations:$iters, disagreements:$disagree, unchanged_tree_reevaluations:$reevals, dimensions:$dims}'
}

PERTASK=$(mktemp)
trap 'rm -f "$PERTASK"' EXIT

while read -r tid; do
  [[ -n "$tid" ]] || continue
  body=$(echo "$ALL" | jq -r --arg id "$tid" '.[] | select(.id==$id) | .body // ""')
  parsed=$(printf '%s' "$body" | parse_eval_body)
  [[ -n "$parsed" ]] || continue
  epic=$(resolve_epic "$tid")
  [[ -n "$epic" ]] || continue
  echo "$parsed" | jq -c --arg epic "$epic" '. + {epic:$epic}'
done < <(echo "$ALL" | jq -r '.[] | select(.type=="task") | .id') >>"$PERTASK"

EPICMETA=$(echo "$ALL" | jq '[.[] | select(.type=="epic") | {key:.id, value:{created_at:(.created_at // ""), title:.title}}] | from_entries')

jq -s \
  --argjson epicmeta "$EPICMETA" \
  '
  def round2: (. * 100 | round) / 100;
  def dir_count(a; b):
    (if (0.05 * a) < 0.5 then 0.5 else (0.05 * a) end) as $t
    | if (b - a) > $t then "declining"
      elif (a - b) > $t then "improving"
      else "stable" end;
  def dir_score(a; b):
    if (a - b) > 0.2 then "declining"
    elif (b - a) > 0.2 then "improving"
    else "stable" end;

  (group_by(.epic) | map(
    . as $tasks
    | ([$tasks[].dimensions] ) as $ds
    | ([$ds[] | keys[]] | unique) as $dimkeys
    | {
        epic: $tasks[0].epic,
        title: ($epicmeta[$tasks[0].epic].title // null),
        created_at: ($epicmeta[$tasks[0].epic].created_at // ""),
        task_count: ($tasks | length),
        dispatches: {
          mean: (([$tasks[].dispatches] | add) / ($tasks | length) | round2),
          max: ([$tasks[].dispatches] | max)
        },
        iterations: {
          mean: (([$tasks[].iterations] | add) / ($tasks | length) | round2)
        },
        disagreements: ([$tasks[].disagreements] | add),
        unchanged_tree_reevaluations: ([$tasks[].unchanged_tree_reevaluations] | add),
        dimensions: (
          reduce $dimkeys[] as $k ({};
            ([$ds[] | .[$k] // empty]) as $vals
            | if ($vals | length) > 0
              then . + {($k): (($vals | add) / ($vals | length) | round2)}
              else . end)
        )
      }
  ) | sort_by(.created_at)) as $epics_raw
  | ($epics_raw | map(del(.created_at))) as $epics
  | (if ($epics | length) < 2 then null else
      [ range(1; ($epics | length)) as $i
        | $epics[$i-1] as $p
        | $epics[$i] as $c
        | {
            from_epic: $p.epic,
            to_epic: $c.epic,
            dispatches: {
              from: $p.dispatches.mean, to: $c.dispatches.mean,
              direction: dir_count($p.dispatches.mean; $c.dispatches.mean)
            },
            iterations: {
              from: $p.iterations.mean, to: $c.iterations.mean,
              direction: dir_count($p.iterations.mean; $c.iterations.mean)
            },
            disagreements: {
              from: $p.disagreements, to: $c.disagreements,
              direction: dir_count($p.disagreements; $c.disagreements)
            },
            unchanged_tree_reevaluations: {
              from: $p.unchanged_tree_reevaluations, to: $c.unchanged_tree_reevaluations,
              direction: dir_count($p.unchanged_tree_reevaluations; $c.unchanged_tree_reevaluations)
            },
            dimensions: (
              ([($p.dimensions | keys[]), ($c.dimensions | keys[])] | unique) as $keys
              | reduce $keys[] as $k ({};
                  ($p.dimensions[$k]) as $pv | ($c.dimensions[$k]) as $cv
                  | if ($pv != null and $cv != null)
                    then . + {($k): {from: $pv, to: $cv, direction: dir_score($pv; $cv)}}
                    else . end)
            )
          }
      ]
    end) as $trends
  | (if $trends == null then false else
      ($trends[-1]) as $last
      | ($last.dispatches.direction == "declining")
        or ($last.disagreements.direction == "declining")
        or ($last.unchanged_tree_reevaluations.direction == "declining")
        or ([$last.dimensions[] | .direction] | any(. == "declining"))
    end) as $alarm
  | (if $alarm then
      ($trends[-1]) as $last
      | [ (if $last.dispatches.direction == "declining"
            then "dispatches-to-convergence rose from \($last.dispatches.from) to \($last.dispatches.to)" else empty end),
          (if $last.disagreements.direction == "declining"
            then "provider disagreements rose from \($last.disagreements.from) to \($last.disagreements.to)" else empty end),
          (if $last.unchanged_tree_reevaluations.direction == "declining"
            then "re-evaluations of an unchanged tree rose from \($last.unchanged_tree_reevaluations.from) to \($last.unchanged_tree_reevaluations.to)" else empty end),
          ($last.dimensions | to_entries[] | select(.value.direction == "declining")
            | "\(.key) score fell from \(.value.from) to \(.value.to)") ]
    else [] end) as $alarm_reasons
  | {epics: $epics, trends: $trends, alarm: $alarm, alarm_reasons: $alarm_reasons}
  ' "$PERTASK"
