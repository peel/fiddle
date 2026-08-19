#!/usr/bin/env bash
set -euo pipefail

BEAN_ID="" INIT=false SPOT_CHECK=false BASE_SHA="" ITERATION="" SCORECARD="" DISPATCHES="" GUIDANCE="" DISAGREEMENTS="" CORRECTIONS="" ANTIPATTERNS=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bean-id) BEAN_ID="$2"; shift 2;;
    --init) INIT=true; shift;;
    --spot-check) SPOT_CHECK=true; shift;;
    --base-sha) BASE_SHA="$2"; shift 2;;
    --iteration) ITERATION="$2"; shift 2;;
    --scorecard) SCORECARD="$2"; shift 2;;
    --dispatches) DISPATCHES="$2"; shift 2;;
    --guidance) GUIDANCE="$2"; shift 2;;
    --disagreements) DISAGREEMENTS="$2"; shift 2;;
    --corrections) CORRECTIONS="$2"; shift 2;;
    --antipatterns) ANTIPATTERNS="$2"; shift 2;;
    *) echo "Unknown arg: $1" >&2; exit 2;;
  esac
done

[[ -n "$BEAN_ID" ]] || { echo "Missing --bean-id" >&2; exit 2; }

if $INIT; then
  [[ -n "$BASE_SHA" ]] || { echo "Missing --base-sha for --init" >&2; exit 2; }
  beans update "$BEAN_ID" --body-append "$(cat <<EOF

## Evaluation Log
BASE_SHA: $BASE_SHA
total_dispatches: 0
EOF
)" >/dev/null 2>&1 || { echo "Bean $BEAN_ID not found" >&2; exit 1; }
  exit 0
fi

if $SPOT_CHECK; then
  DISPATCHES="${DISPATCHES:-0}"
else
  [[ -n "$ITERATION" ]] || { echo "Missing --iteration" >&2; exit 2; }
  [[ -n "$DISPATCHES" ]] || { echo "Missing --dispatches" >&2; exit 2; }
fi
[[ -n "$SCORECARD" && -f "$SCORECARD" ]] || { echo "Missing --scorecard file" >&2; exit 2; }

TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")

if $SPOT_CHECK; then
  HEADING="### Spot-Check ($TIMESTAMP)"
else
  HEADING="### Iteration $ITERATION ($TIMESTAMP)"
fi

ENTRY=$(jq -r --arg heading "$HEADING" --arg disp "$DISPATCHES" --arg guide "$GUIDANCE" '
  def ungraded($why): " (UNGRADED, \($why))";
  def dim_note($d):
    if ($d | type) != "object" then
      ungraded("dimension is \($d | type), not an object")
    elif ($d.score | type) != "number" then
      ungraded(if $d.score == null then "no score recorded"
               else "score is \($d.score | type), not a number" end)
    elif ($d.threshold | type) != "number" then
      ungraded(if $d.threshold == null then "no threshold recorded"
               else "threshold is \($d.threshold | type), not a number" end)
    elif $d.score < $d.threshold then " (FAIL, threshold \($d.threshold))"
    else "" end;

  "\($heading)\ndispatches: \($disp)" +
  (if (.domains | type) != "object" then
     "\n**scorecard:**\n- domains" +
     ungraded(if .domains == null then "no `domains` recorded"
              else "`domains` is \(.domains | type), not an object" end)
   else
     (.domains | to_entries | map(
       "\n**\(.key):**" +
       ((.value | if type == "object" then .dimensions else null end) as $dims |
        if ($dims | type) != "object" then
          "\n- dimensions" +
          ungraded(if $dims == null then "no `dimensions` recorded"
                   else "`dimensions` is \($dims | type), not an object" end)
        else
          ($dims | to_entries | map(
            .value as $d |
            "\n- \(.key): \(if ($d | type) == "object" then $d.score else $d end)/10" +
            dim_note($d)
          ) | join(""))
        end)
     ) | join(""))
   end) +
  (if $guide != "" then "\n**Guidance:** \"\($guide)\"" else "" end)
' "$SCORECARD") || ENTRY=""

if [[ -z "$ENTRY" ]]; then
  echo "Warning: could not read scorecard $SCORECARD — logging the iteration without dimensions" >&2
  ENTRY=$(printf '%s\ndispatches: %s\n**scorecard:** (UNGRADED, could not be read: %s)' \
    "$HEADING" "$DISPATCHES" "$SCORECARD")
  if [[ -n "$GUIDANCE" ]]; then
    ENTRY=$(printf '%s\n**Guidance:** "%s"' "$ENTRY" "$GUIDANCE")
  fi
fi

if [[ -n "$DISAGREEMENTS" && -f "$DISAGREEMENTS" ]]; then
  DISAGREE_SECTION=$(jq -r '
    if length == 0 then "" else
      "\n**Disagreements:**" +
      (map("\n- \(.domain).\(.dimension): spread \(.spread) (" +
        ([.scores | to_entries[] | "\(.key): \(.value)"] | join(", ")) +
      ")") | join(""))
    end
  ' "$DISAGREEMENTS" 2>/dev/null || true)
  if [[ -n "$DISAGREE_SECTION" ]]; then
    ENTRY="${ENTRY}${DISAGREE_SECTION}"
  fi
fi

if [[ -n "$CORRECTIONS" && -f "$CORRECTIONS" ]]; then
  CORRECT_SECTION=$(jq -r '
    if length == 0 then "" else
      "\n**Human Corrections:**" +
      (map("\n- \(.domain).\(.dimension): evaluator \(.evaluator_score) → human \(.human_score)" +
        (if .reason then " (\(.reason))" else "" end)
      ) | join(""))
    end
  ' "$CORRECTIONS" 2>/dev/null || true)
  if [[ -n "$CORRECT_SECTION" ]]; then
    ENTRY="${ENTRY}${CORRECT_SECTION}"
  fi
fi

if [[ -n "$ANTIPATTERNS" && -f "$ANTIPATTERNS" ]]; then
  ANTIPATTERN_SECTION=$(jq -r '
    if length == 0 then "" else
      "\n**Antipatterns detected:**" +
      (map(
        "\n- " +
        (if type == "string" then . else (.id // .antipattern // .antipattern_id // "unknown") end) +
        (if type == "object" and (.evidence // "") != "" then ": \(.evidence)" else "" end)
      ) | join(""))
    end
  ' "$ANTIPATTERNS" 2>/dev/null || true)
  if [[ -n "$ANTIPATTERN_SECTION" ]]; then
    ENTRY="${ENTRY}${ANTIPATTERN_SECTION}"
  fi
fi

CURRENT_BODY=$(beans show "$BEAN_ID" --json 2>/dev/null | jq -r '.body') || { echo "Bean $BEAN_ID not found" >&2; exit 1; }
OLD_TOTAL=$(echo "$CURRENT_BODY" | sed -n 's/^total_dispatches: \([0-9][0-9]*\)/\1/p' | tail -1)
OLD_TOTAL="${OLD_TOTAL:-0}"
NEW_TOTAL=$((OLD_TOTAL + DISPATCHES))

beans update "$BEAN_ID" \
  --body-replace-old "total_dispatches: $OLD_TOTAL" \
  --body-replace-new "total_dispatches: $NEW_TOTAL" >/dev/null 2>&1 || true

beans update "$BEAN_ID" --body-append "$ENTRY" >/dev/null 2>&1 || { echo "Bean $BEAN_ID not found" >&2; exit 1; }
exit 0
