#!/usr/bin/env bash
set -uo pipefail

# This suite gates. It does not only report.
#
# The sweep measures where a wrong type enters scripts/merge-scorecards.sh and what
# comes back. Most positions come back silently: the merge answers exit 0 with a
# merged card that looks complete. That silence is the standing hazard fiddle-cveg
# was opened on, and docs/BACKLOG.md names four live instances of it.
#
# A harness that printed those counts and always exited 0 would be
# assertion-weaker-than-its-message: the numbers would be believed and nothing
# would hold them. So every probe carries the partition it is expected to land in,
# and a probe that moves fails the suite. Moving a probe is allowed; moving it
# without editing this table and docs/BACKLOG.md together is not.
#
# The denominator is derived, never written down. The probe set is every position
# in the fixture card that jq `paths` reaches, plus the two positions at the root
# of the input document. Adding a field to the fixture adds a probe. A field
# merge-scorecards.sh reads that the fixture does not carry makes the suite refuse
# before it counts anything.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
MERGE="$SCRIPT_DIR/merge-scorecards.sh"
VALIDATE="$SCRIPT_DIR/validate-scorecard.sh"

refuse() { echo "SWEEP: CANNOT RUN  ($1)" >&2; exit 2; }

command -v jq >/dev/null 2>&1 || refuse "jq is not on the PATH, so the sweep cannot classify anything"
[ -x "$MERGE" ]    || refuse "$MERGE is not executable"
[ -x "$VALIDATE" ] || refuse "$VALIDATE is not executable"

WORK=$(mktemp -d) || refuse "cannot make a working directory"
trap 'rm -rf "$WORK"' EXIT INT TERM

BASE="$WORK/base.json"
cat > "$BASE" <<'FIXTURE'
[
  {
    "task_id": "bean-sweep",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "claude",
    "mode": "evidence-only",
    "domains": {"general": {"dimensions": {"correctness": {"score": 8, "threshold": 7, "evidence": "e"}}}},
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "antipatterns_detected": ["ap"],
    "spec_defect": {"detected": true, "reason": "r"},
    "spec_coverage_matrix": [{"requirement": "r1", "coverage": "Full"}],
    "remediation_beans": [{"requirement": "r1", "description": "d"}],
    "guidance": "g",
    "dispatch_count": 1
  },
  {
    "task_id": "bean-sweep",
    "iteration": 1,
    "timestamp": "2026-01-01T00:00:00Z",
    "provider": "codex",
    "mode": "evidence-only",
    "domains": {"general": {"dimensions": {"correctness": {"score": 9, "threshold": 7, "evidence": "e"}}}},
    "criteria": [{"id": "c1", "pass": true, "evidence": "e"}],
    "antipatterns_detected": ["ap"],
    "spec_defect": {"detected": true, "reason": "r"},
    "spec_coverage_matrix": [{"requirement": "r1", "coverage": "Full"}],
    "remediation_beans": [{"requirement": "r1", "description": "d"}],
    "guidance": "g",
    "dispatch_count": 1
  }
]
FIXTURE

# Precondition: the fixture is clean at both gates the sweep measures against.
# A fixture that already fails one of them would make every probe read as that
# failure rather than as the injected type.
"$MERGE" < "$BASE" > "$WORK/base-merged.json" 2>/dev/null \
  || refuse "the fixture does not merge, so no probe measures the injection"
for i in 0 1; do
  jq ".[$i]" "$BASE" > "$WORK/base-card.json"
  "$VALIDATE" --scorecard "$WORK/base-card.json" --criteria-ids c1 >/dev/null 2>&1 \
    || refuse "fixture card $i does not validate, so the upstream split measures the fixture"
done

# Precondition: the fixture carries every field name merge-scorecards.sh reads.
# Three of the names the merge dereferences are its own and never come off a card:
# `domain` and `source_provider` are keys the merge builds, and `value` is jq's
# to_entries output. Everything else must be probeable.
DEREFERENCED=$(grep -oE '\.[a-z_][a-z_0-9]*|has\("[a-z_]+"\)' "$MERGE" \
  | sed 's/^\.//; s/has("//; s/")//' | sort -u \
  | grep -vxE 'domain|source_provider|value')
FIXTURE_KEYS=$(jq -r '.[0] | [paths | .[] | select(type == "string")] | unique[]' "$BASE" | sort -u)
UNPROBED=$(comm -23 <(echo "$DEREFERENCED") <(echo "$FIXTURE_KEYS"))
[ -z "$UNPROBED" ] || refuse "merge-scorecards.sh reads fields the fixture does not carry: $(echo "$UNPROBED" | tr '\n' ' ')"

# Every probe's expected partition. Five partitions, one line each:
#   refuses    the merge exits 2 and says why
#   aborts     the merge dies on a jq type error, exit 5
#   in_band    the merge exits 0 and names the card in spec_defect.missing_from
#   silent_refused_upstream   the merge absorbs it; validate-scorecard.sh exits 2
#   silent_accepted           the merge absorbs it; validate-scorecard.sh exits 0
EXPECTED=$(cat <<'TABLE'
refuses	<root>
refuses	<root>[]
refuses	criteria
aborts	provider
aborts	domains
aborts	domains.general
aborts	domains.general.dimensions
aborts	domains.general.dimensions.correctness
aborts	domains.general.dimensions.correctness.score
aborts	criteria.0
aborts	spec_coverage_matrix.0
aborts	remediation_beans.0
aborts	dispatch_count
in_band	spec_defect
in_band	spec_defect.detected
silent_refused_upstream	mode
silent_refused_upstream	domains.general.dimensions.correctness.threshold
silent_refused_upstream	domains.general.dimensions.correctness.evidence
silent_refused_upstream	criteria.0.id
silent_refused_upstream	criteria.0.pass
silent_refused_upstream	criteria.0.evidence
silent_refused_upstream	spec_defect.reason
silent_accepted	task_id
silent_accepted	iteration
silent_accepted	timestamp
silent_accepted	antipatterns_detected
silent_accepted	antipatterns_detected.0
silent_accepted	spec_coverage_matrix
silent_accepted	spec_coverage_matrix.0.requirement
silent_accepted	spec_coverage_matrix.0.coverage
silent_accepted	remediation_beans
silent_accepted	remediation_beans.0.requirement
silent_accepted	remediation_beans.0.description
silent_accepted	guidance
TABLE
)

expected_for() { awk -F'\t' -v p="$1" '$2 == p {print $1}' <<<"$EXPECTED"; }

REFUSES=""; ABORTS=""; IN_BAND=""; SILENT_REFUSED=""; SILENT_ACCEPTED=""
TOTAL=0; DRIFT=0

classify() {
  local label="$1" input="$2"
  local out rc observed vrc
  out=$("$MERGE" <<<"$input" 2>/dev/null); rc=$?
  if [ "$rc" -eq 2 ]; then
    observed=refuses
  elif [ "$rc" -eq 5 ]; then
    observed=aborts
  elif [ "$rc" -ne 0 ]; then
    observed="exited_$rc"
  else
    if jq -e '.spec_defect.missing_from | index("claude")' <<<"$out" >/dev/null 2>&1; then
      observed=in_band
    else
      jq '.[0]' <<<"$input" > "$WORK/probe-card.json" 2>/dev/null
      "$VALIDATE" --scorecard "$WORK/probe-card.json" --criteria-ids c1 >/dev/null 2>&1; vrc=$?
      if [ "$vrc" -eq 0 ]; then observed=silent_accepted; else observed=silent_refused_upstream; fi
    fi
  fi

  TOTAL=$((TOTAL + 1))
  case "$observed" in
    refuses)                 REFUSES="$REFUSES $label";;
    aborts)                  ABORTS="$ABORTS $label";;
    in_band)                 IN_BAND="$IN_BAND $label";;
    silent_refused_upstream) SILENT_REFUSED="$SILENT_REFUSED $label";;
    silent_accepted)         SILENT_ACCEPTED="$SILENT_ACCEPTED $label";;
    *) echo "  DRIFT: $label left the merge at $observed, which is none of the five partitions";;
  esac

  local want
  want=$(expected_for "$label")
  if [ -z "$want" ]; then
    echo "  DRIFT: $label is probed and the expected table does not name it (observed $observed)"
    DRIFT=$((DRIFT + 1))
  elif [ "$want" != "$observed" ]; then
    echo "  DRIFT: $label expected $want, observed $observed"
    DRIFT=$((DRIFT + 1))
  fi
}

echo "=== wrong-type sweep of scripts/merge-scorecards.sh ==="

classify '<root>'   '"wrong"'
classify '<root>[]' "$(jq -c '[.[0], "wrong"]' "$BASE")"

PROBED_LABELS=""
while IFS= read -r p; do
  label=$(jq -r 'map(tostring) | join(".")' <<<"$p")
  PROBED_LABELS="$PROBED_LABELS$label"$'\n'
  # One wrong type per position, chosen by what is there: a string takes a number,
  # everything else takes a string. Neither is ever the right type for its slot.
  input=$(jq -c --argjson p "$p" '
    ([0] + $p) as $at |
    setpath($at; if (getpath($at) | type) == "string" then 42 else "wrong" end)
  ' "$BASE")
  classify "$label" "$input"
done < <(jq -c '.[0] | paths' "$BASE")

# The table must not name a probe that no longer runs, or a retired hole would keep
# a count alive with nothing behind it.
STALE=$(comm -23 \
  <(awk -F'\t' '{print $2}' <<<"$EXPECTED" | grep -vxE '<root>|<root>\[\]' | sort) \
  <(printf '%s' "$PROBED_LABELS" | grep -v '^$' | sort))
if [ -n "$STALE" ]; then
  echo "  DRIFT: the expected table names positions the sweep no longer probes: $(echo "$STALE" | tr '\n' ' ')"
  DRIFT=$((DRIFT + 1))
fi

count() { set -- $1; echo $#; }
N_REFUSES=$(count "$REFUSES")
N_ABORTS=$(count "$ABORTS")
N_IN_BAND=$(count "$IN_BAND")
N_SILENT_REFUSED=$(count "$SILENT_REFUSED")
N_SILENT_ACCEPTED=$(count "$SILENT_ACCEPTED")
N_SILENT=$((N_SILENT_REFUSED + N_SILENT_ACCEPTED))
SUM=$((N_REFUSES + N_ABORTS + N_IN_BAND + N_SILENT))

echo ""
printf '  refuses with a reason, exit 2          %2d :%s\n' "$N_REFUSES" "$REFUSES"
printf '  aborts on a jq type error, exit 5      %2d :%s\n' "$N_ABORTS" "$ABORTS"
printf '  classified in band as not_reported     %2d :%s\n' "$N_IN_BAND" "$IN_BAND"
printf '  admitted silently                      %2d\n' "$N_SILENT"
printf '    of which validate-scorecard refuses  %2d :%s\n' "$N_SILENT_REFUSED" "$SILENT_REFUSED"
printf '    of which validate-scorecard accepts  %2d :%s\n' "$N_SILENT_ACCEPTED" "$SILENT_ACCEPTED"
echo ""
printf '  SWEEP: %d probes, %d classified of %d positions\n' "$TOTAL" "$SUM" "$TOTAL"

if [ "$SUM" -ne "$TOTAL" ]; then
  echo "  REPORT UNRELIABLE: the partitions do not sum to the probe count"
  exit 1
fi
if [ "$DRIFT" -ne 0 ]; then
  echo ""
  echo "  FAIL: $DRIFT of $TOTAL probes disagree with the expected table."
  echo "  A probe that moves is a real change in what the merge admits. Correct the"
  echo "  table above and the counts in docs/BACKLOG.md in the same commit."
  exit 1
fi

# The two documents that state this partition in prose state it once in a fixed line
# as well, composed from the same counts. A partition that moves without the prose
# moving fails here, so neither document can carry a number the sweep contradicts.
CANONICAL=$(printf 'sweep: %d probes = %d refuses + %d aborts + %d in-band + %d silent (%d refused upstream + %d accepted)' \
  "$TOTAL" "$N_REFUSES" "$N_ABORTS" "$N_IN_BAND" "$N_SILENT" "$N_SILENT_REFUSED" "$N_SILENT_ACCEPTED")
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
STATED=0
for DOC in docs/BACKLOG.md skills/develop/scorecard-envelope.md; do
  if [ ! -f "$REPO_ROOT/$DOC" ]; then
    echo "  DRIFT: $DOC is missing, so its statement of this partition cannot be checked"
    STATED=$((STATED + 1))
  elif ! grep -qF "$CANONICAL" "$REPO_ROOT/$DOC"; then
    echo "  DRIFT: $DOC does not state the measured partition"
    echo "         expected line: $CANONICAL"
    STATED=$((STATED + 1))
  fi
done
if [ "$STATED" -ne 0 ]; then
  echo ""
  echo "  FAIL: $STATED of 2 documents state a partition the sweep did not measure."
  exit 1
fi

echo "  PASS: all $TOTAL probes landed in the partition the table names,"
echo "        and both documents state the measured partition"
exit 0
