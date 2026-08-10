#!/usr/bin/env bash
#
# Tier 2: realistic fixtures against a real model.
#
# On demand and at milestone completion — never automatic, never a gate. Its
# output is judgment material, not an assertion: this script exits 0 whenever it
# managed to *run* the fixtures, whatever the model made of them. A run in which
# every fixture came back unrepaired is a finding, and findings are read, not
# thrown at a build.
#
# That is the whole difference between this and the two lanes either side of it:
#
#   deterministic suite   always, free, offline    MockCompletionModel   GATES
#   tier 1 (smoke.rs)     opt-in, #[ignore]d       real, cheap           no
#   tier 2 (this)         opt-in, on demand        real                  no
#
# Neither real-model tier ever runs unless somebody asks for it by name. Tier 1
# is one #[ignore]d test, reachable only through `--ignored`; this script is
# reachable only by being invoked. Nothing in .github/workflows invokes either,
# and nothing should: they need a credential and they cost money, so a workflow
# that ran one would make both a condition of merging. ADR 012 records the
# decision and what it gives up.
#
# Usage
#   ( set -a; . .env; set +a; ./scripts/tier2.sh )
#   FIDDLE_TIER2_MODEL=claude-opus-5 ./scripts/tier2.sh
#   FIDDLE_TIER2_OUT=/tmp/run-7 ./scripts/tier2.sh
#
# It writes one JSON artifact per fixture plus a `summary.json` over all of
# them, so a later reading is done against a record rather than against a
# terminal scrollback.
#
# NEVER print the credential. It is read from the environment by the binary
# itself, through the variable `fiddle.toml` names; this script never expands
# LITELLM_API_KEY into a command line, a log line, or a generated document.

set -euo pipefail

: "${LITELLM_API_KEY:?tier 2 requires a credential}"

# Defaulted to a model measured to drive the whole loop rather than to the
# largest one available. Through this gateway both Claude-family models finalise
# after a single tool call, while kimi, deepseek and glm-5 all list, read, write,
# check and earn the marker — see the table at `DEFAULT_MODEL` in
# crates/fiddle-cli/tests/smoke.rs. Comparing models is what FIDDLE_TIER2_MODEL
# is for, and it is a good use of Tier 2.
MODEL="${FIDDLE_TIER2_MODEL:-bedrock/moonshotai.kimi-k2.5}"
BASE_URL="${FIDDLE_TIER2_BASE_URL:-https://litellm.firn.snplow.net/v1}"
CREDENTIAL_VAR="LITELLM_API_KEY"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${FIDDLE_TIER2_OUT:-$REPO_ROOT/target/tier2/$(date -u +%Y%m%dT%H%M%SZ)}"
WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

mkdir -p "$OUT_DIR"

# ---------------------------------------------------------------------------
# The binary under test
#
# Built here rather than assumed present, for the reason the acceptance
# harness's `fiddle_binary` spells out at length: a path under target/ holds
# whatever the last build left, which may predate the change being judged, and a
# Tier 2 report about last week's binary is a report about nothing. `--release`
# because Tier 2 is meant to resemble a deployment.
# ---------------------------------------------------------------------------
echo "building the fiddle binary…"
cargo build --release --bin fiddle --manifest-path "$REPO_ROOT/Cargo.toml" >&2
FIDDLE="$REPO_ROOT/target/release/fiddle"
[ -x "$FIDDLE" ] || { echo "no fiddle binary at $FIDDLE" >&2; exit 1; }

# ---------------------------------------------------------------------------
# The fixtures
#
# Each is a real cargo package whose own test suite fails, and each fails for a
# different *kind* of reason — which is the point of Tier 2 over Tier 1. Tier 1
# uses one trivial fixture so that a failure means the plumbing broke; these are
# meant to be hard enough that what comes back is worth reading.
#
#   off_by_one    one wrong operator, the defect named by a failing assertion
#   wrong_branch  a condition inverted, where reading the test is not enough
#                 and the model has to reason about which branch is which
#   missing_case  an unhandled input, where the repair is an addition rather
#                 than an edit
# ---------------------------------------------------------------------------
FIXTURES=(off_by_one wrong_branch missing_case)

write_fixture() {
  local name="$1" repo="$2"
  mkdir -p "$repo/src" "$repo/tests"
  cat > "$repo/Cargo.toml" <<TOML
[package]
name = "fixture"
version = "0.0.0"
edition = "2021"

[dependencies]
TOML
  printf 'target/\nCargo.lock\n' > "$repo/.gitignore"

  case "$name" in
    off_by_one)
      cat > "$repo/src/lib.rs" <<'RS'
/// The index of the last element of a slice of length `len`.
pub fn last_index(len: usize) -> usize {
    len
}
RS
      cat > "$repo/tests/repair.rs" <<'RS'
#[test]
fn the_last_index_is_one_before_the_length() {
    assert_eq!(fixture::last_index(3), 2);
    assert_eq!(fixture::last_index(1), 0);
}
RS
      ;;
    wrong_branch)
      cat > "$repo/src/lib.rs" <<'RS'
/// Clamp `value` into `[low, high]`.
pub fn clamp(value: i64, low: i64, high: i64) -> i64 {
    if value < low {
        high
    } else if value > high {
        low
    } else {
        value
    }
}
RS
      cat > "$repo/tests/repair.rs" <<'RS'
#[test]
fn a_value_below_the_floor_becomes_the_floor() {
    assert_eq!(fixture::clamp(-5, 0, 10), 0);
}

#[test]
fn a_value_above_the_ceiling_becomes_the_ceiling() {
    assert_eq!(fixture::clamp(50, 0, 10), 10);
}

#[test]
fn a_value_inside_the_range_is_unchanged() {
    assert_eq!(fixture::clamp(4, 0, 10), 4);
}
RS
      ;;
    missing_case)
      cat > "$repo/src/lib.rs" <<'RS'
/// Render a byte count as a short human-readable string.
pub fn human_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{} KiB", bytes / 1024)
    }
}
RS
      cat > "$repo/tests/repair.rs" <<'RS'
#[test]
fn bytes_are_rendered_plainly() {
    assert_eq!(fixture::human_bytes(512), "512 B");
}

#[test]
fn kibibytes_are_rendered_as_such() {
    assert_eq!(fixture::human_bytes(2048), "2 KiB");
}

#[test]
fn mebibytes_get_their_own_unit() {
    assert_eq!(fixture::human_bytes(5 * 1024 * 1024), "5 MiB");
}
RS
      ;;
    *)
      echo "unknown fixture $name" >&2
      exit 1
      ;;
  esac

  git -C "$repo" init -q .
  git -C "$repo" add -A
  git -C "$repo" -c user.email=t@t -c user.name=t commit -qm "the broken fixture" >/dev/null
}

# ---------------------------------------------------------------------------
# One fixture, one run
# ---------------------------------------------------------------------------
run_fixture() {
  local name="$1"
  local root="$WORK_DIR/$name"
  local ref="beans:tier2-$name"

  mkdir -p "$root/stub-state/work" "$root/stub-state/changes"
  printf '{"id":"tier2-%s","status":"open"}' "$name" \
    > "$root/stub-state/work/tier2-$name.json"
  write_fixture "$name" "$root/fixture"

  # The document names the *variable*, never the value — the same discipline the
  # schema enforces, kept here so a generated document is as safe as a written
  # one.
  cat > "$root/fiddle.toml" <<TOML
[project]
name = "icecube"

[stub]
root = "$root/stub-state"

[report]
dir = "$root/reports"

[agent]
model = "$MODEL"
base_url = "$BASE_URL"
api_key = { env = "$CREDENTIAL_VAR" }
max_turns = 40
max_tokens = 8192
deadline = "15m"
tool_timeout = "10m"

[workspace]
root = "$root/workspaces"
fixture = "$root/fixture"
check = { program = "cargo", args = ["test", "--offline"] }
command_timeout = "10m"
TOML

  echo "── $name ──────────────────────────────────────────────" >&2
  local started ended code
  started=$(date +%s)
  set +e
  "$FIDDLE" run "$ref" \
    --config "$root/fiddle.toml" \
    --capability fixture_repair \
    --json > "$root/stdout.json" 2> "$root/stderr.txt"
  code=$?
  set -e
  ended=$(date +%s)

  # The marker on disk, read the way the stub change port reads it. Present
  # exactly when the check passed, so it is the one unambiguous answer to "did
  # the repair land".
  local marker="null"
  if [ -f "$root/stub-state/changes/tier2-$name.json" ]; then
    marker=$(python3 -c "
import json,sys
print(json.dumps(json.load(open(sys.argv[1])).get('marker')))
" "$root/stub-state/changes/tier2-$name.json")
  fi

  MODEL="$MODEL" BASE_URL="$BASE_URL" FIXTURE="$name" REF="$ref" \
  CODE="$code" ELAPSED="$((ended - started))" MARKER="$marker" \
  python3 - "$root/stdout.json" "$root/stderr.txt" "$OUT_DIR/$name.json" <<'PY'
import json, os, sys

stdout_path, stderr_path, out_path = sys.argv[1:4]
try:
    payload = json.load(open(stdout_path))
except Exception as e:
    payload = {"_unparseable": str(e), "_raw": open(stdout_path).read()[:4000]}

outcome = payload.get("outcome")
if isinstance(outcome, str):
    kind, reason = outcome, None
elif isinstance(outcome, dict) and outcome:
    kind = next(iter(outcome))
    reason = outcome[kind].get("reason") or outcome[kind].get("error")
else:
    kind, reason = None, None

record = {
    "fixture": os.environ["FIXTURE"],
    "invocation_ref": os.environ["REF"],
    "model": os.environ["MODEL"],
    "gateway": os.environ["BASE_URL"],
    "exit_code": int(os.environ["CODE"]),
    "elapsed_seconds": int(os.environ["ELAPSED"]),
    "outcome": kind,
    "reason": reason,
    "repair_landed": kind == "completed",
    "marker": json.loads(os.environ["MARKER"]),
    "capability_executions": payload.get("capability_executions"),
    "next_action": payload.get("next_action"),
    "report": payload.get("report"),
    "stderr": open(stderr_path).read()[:4000],
}
json.dump(record, open(out_path, "w"), indent=2)
print(f"  outcome        = {kind}")
print(f"  repair landed  = {record['repair_landed']}")
print(f"  exit code      = {record['exit_code']}")
print(f"  elapsed        = {record['elapsed_seconds']}s")
if reason:
    print(f"  reason         = {reason[:300]}")
PY
}

echo "tier 2 — model=$MODEL gateway=$BASE_URL"
echo "artifacts → $OUT_DIR"
echo

for fixture in "${FIXTURES[@]}"; do
  run_fixture "$fixture"
done

python3 - "$OUT_DIR" <<'PY'
import glob, json, os, sys

out_dir = sys.argv[1]
records = [
    json.load(open(p))
    for p in sorted(glob.glob(os.path.join(out_dir, "*.json")))
    if os.path.basename(p) != "summary.json"
]
landed = [r for r in records if r["repair_landed"]]
summary = {
    "model": records[0]["model"] if records else None,
    "gateway": records[0]["gateway"] if records else None,
    "fixtures": len(records),
    "repairs_landed": len(landed),
    "total_seconds": sum(r["elapsed_seconds"] for r in records),
    "by_fixture": {
        r["fixture"]: {
            "outcome": r["outcome"],
            "repair_landed": r["repair_landed"],
            "elapsed_seconds": r["elapsed_seconds"],
        }
        for r in records
    },
}
json.dump(summary, open(os.path.join(out_dir, "summary.json"), "w"), indent=2)
print()
print("─── tier 2 summary ────────────────────────────────────────")
print(f"  model          = {summary['model']}")
print(f"  repairs landed = {summary['repairs_landed']} / {summary['fixtures']}")
print(f"  total          = {summary['total_seconds']}s")
print(f"  artifacts      = {out_dir}")
print("───────────────────────────────────────────────────────────")
print("  This is judgment material, not a gate. A low score is a finding to")
print("  read, not a build to fail — so this script exits 0 either way.")
PY
