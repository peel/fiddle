#!/usr/bin/env bash
# test-check-github-effects-lane.sh — proof that the lane checker discriminates,
# and that the lane's own shell does what its checker claims.
#
# Two halves, because the checker and the thing checked can both be wrong:
#
#   A. THE CHECKER. Every property in check-github-effects-lane.sh is broken in
#      turn, in a copy of the real workflow, and the checker must exit non-zero.
#      A check nobody has ever seen fail is a check nobody has evidence for: an
#      absence is only evidence when something would notice its return.
#   B. THE LANE. The two guards' `run:` blocks are extracted from the shipped
#      workflow and *executed* — with and without the condition each refuses. A
#      grep proving `exit 1` is present is weaker than a run proving it fires.
#
# Exit 0 if every case behaves; 1 otherwise.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHECKER="$SCRIPT_DIR/check-github-effects-lane.sh"
WORKFLOW="$SCRIPT_DIR/../.github/workflows/github-effects.yml"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

PASS=0
FAIL=0
ok()  { PASS=$((PASS + 1)); echo "  PASS: $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL: $1"; }

GUARD_MARKER='- name: Require FIDDLE_EFFECTS_TOKEN'
PREFLIGHT_MARKER='- name: Require a Cargo workspace on the dispatched ref'

# Insert a line immediately after the first line containing a marker. awk rather
# than `sed a\`, whose syntax differs between GNU and BSD sed and so would pass
# on one of the two machines this runs on and not the other.
insert_after() {
  awk -v marker="$1" -v text="$2" '
    { print }
    !done && index($0, marker) { print text; done = 1 }
  ' "$3" > "$4"
}

expect_reject() {
  local description="$1" file="$2" want="$3" out code=0
  out=$("$CHECKER" "$file" 2>&1) || code=$?
  if [[ "$code" -eq 0 ]]; then
    bad "$description (checker accepted the broken file)"
  elif [[ -n "$want" ]] && ! grep -qF -- "$want" <<<"$out"; then
    bad "$description (rejected, but the reason never mentions '$want')"
  else
    ok "$description"
  fi
}

echo "A. the checker rejects each defect it exists to catch"

# The unmutated file must pass, or every rejection below proves nothing.
if "$CHECKER" "$WORKFLOW" >/dev/null 2>&1; then
  ok "the shipped workflow passes"
else
  bad "the shipped workflow passes"
fi

insert_after "$GUARD_MARKER" "        if: \${{ github.event_name == 'workflow_dispatch' }}" \
  "$WORKFLOW" "$WORK/with-if.yml"
expect_reject "an 'if:' on the credential guard is rejected" "$WORK/with-if.yml" "'if:' appears"

insert_after "$GUARD_MARKER" "        continue-on-error: true" \
  "$WORKFLOW" "$WORK/with-coe.yml"
expect_reject "a 'continue-on-error:' is rejected" "$WORK/with-coe.yml" "'continue-on-error:' appears"

# A job-level `if:` is the version that goes green most quietly, so it gets its
# own case rather than relying on the step-level one above.
awk '{ print } /^  effects:/ { print "    if: false" }' "$WORKFLOW" > "$WORK/job-if.yml"
expect_reject "a job-level 'if:' is rejected" "$WORK/job-if.yml" "'if:' appears"

# Delete the whole preflight step.
awk -v start="$PREFLIGHT_MARKER" '
  index($0, start) { dropping = 1 }
  dropping && /^      - uses: dtolnay\/rust-toolchain/ { dropping = 0 }
  !dropping { print }
' "$WORKFLOW" > "$WORK/no-preflight.yml"
expect_reject "removing the workspace preflight is rejected" "$WORK/no-preflight.yml" "no workspace preflight step"

# Move `cargo build --release` ahead of the preflight: the step still exists, it
# just no longer pre-empts anything. This is the ordering-only regression, and
# nothing but an ordering assertion catches it.
awk -v marker="$PREFLIGHT_MARKER" '
  /^      - run: cargo build --release$/ { next }
  index($0, marker) { print "      - run: cargo build --release" }
  { print }
' "$WORKFLOW" > "$WORK/late-preflight.yml"
expect_reject "a preflight that runs after the build is rejected" "$WORK/late-preflight.yml" "runs after"

# Demote the credential guard out of first position.
awk -v marker="$GUARD_MARKER" '
  /^    steps:$/ { print; print "      - uses: actions/checkout@v4"; next }
  { print }
' "$WORKFLOW" > "$WORK/late-guard.yml"
expect_reject "a credential guard that is not the first step is rejected" "$WORK/late-guard.yml" "first step"

# Neuter the guard's exit. The guard's `exit 1` is indented 12 spaces (it sits
# inside its own `if`), the preflight's 10, so this hits exactly one of them.
sed 's/^            exit 1$/            exit 0/' "$WORKFLOW" > "$WORK/soft-guard.yml"
expect_reject "a credential guard that exits 0 is rejected" "$WORK/soft-guard.yml" "no 'exit 1'"

# Strip the remedy from the preflight's diagnostics: naming the fault without
# naming the fix is the failure mode the criterion is about.
grep -v -- '--ref' "$WORKFLOW" > "$WORK/no-remedy.yml"
expect_reject "a preflight that never names '--ref' is rejected" "$WORK/no-remedy.yml" "what to pass instead"

echo
echo "B. the shipped guards' own shell fires, run rather than grepped"

extract_run_block() {
  awk -v marker="      - name: $1" '
    index($0, marker) == 1 { instep = 1; next }
    instep && $0 == "        run: |" { inrun = 1; next }
    inrun {
      if ($0 == "") { print ""; next }
      if (substr($0, 1, 10) == "          ") { print substr($0, 11); next }
      exit
    }
  ' "$WORKFLOW"
}

extract_run_block "Require a Cargo workspace on the dispatched ref" > "$WORK/preflight.sh"
extract_run_block "Require FIDDLE_EFFECTS_TOKEN" > "$WORK/guard.sh"

[[ -s "$WORK/preflight.sh" ]] || bad "the preflight run: block could be extracted"
[[ -s "$WORK/guard.sh" ]]     || bad "the credential guard run: block could be extracted"

# The counterfactual: the exact case that produced run 31374051935 — a checkout
# with no Cargo.toml at its root.
mkdir -p "$WORK/no-workspace"
out=$(cd "$WORK/no-workspace" && \
  GITHUB_REF=refs/heads/main GITHUB_SHA=aa86c60 \
  GITHUB_WORKSPACE="$WORK/no-workspace" GITHUB_REPOSITORY=peel/fiddle \
  bash "$WORK/preflight.sh" 2>&1)
code=$?
if [[ "$code" -eq 0 ]]; then
  bad "the preflight fails on a checkout with no Cargo.toml"
elif ! grep -qF -- '--ref' <<<"$out" || ! grep -qF 'no Cargo.toml' <<<"$out"; then
  bad "the preflight's failure names the reason and the remedy (got: ${out:0:120})"
else
  ok "the preflight fails on a checkout with no Cargo.toml, naming the reason and '--ref'"
fi

# And the case it must not obstruct.
mkdir -p "$WORK/workspace"
: > "$WORK/workspace/Cargo.toml"
if (cd "$WORK/workspace" && \
    GITHUB_REF=refs/heads/plan/agentic-factory-m2 GITHUB_SHA=deadbee \
    GITHUB_WORKSPACE="$WORK/workspace" GITHUB_REPOSITORY=peel/fiddle \
    bash "$WORK/preflight.sh" >/dev/null 2>&1); then
  ok "the preflight passes on a checkout that has a Cargo.toml"
else
  bad "the preflight passes on a checkout that has a Cargo.toml"
fi

# `${{ secrets.X }}` renders as the empty string when X does not exist, so an
# absent secret reaches the guard as an empty variable — this is that.
out=$(FIDDLE_EFFECTS_TOKEN="" GITHUB_REPOSITORY=peel/fiddle bash "$WORK/guard.sh" 2>&1)
code=$?
if [[ "$code" -eq 0 ]]; then
  bad "the credential guard fails when the secret renders empty"
elif ! grep -qF 'FIDDLE_EFFECTS_TOKEN' <<<"$out"; then
  bad "the credential guard's failure names the secret (got: ${out:0:120})"
else
  ok "the credential guard fails when the secret renders empty, naming it"
fi

if FIDDLE_EFFECTS_TOKEN=not-a-real-token GITHUB_REPOSITORY=peel/fiddle \
   bash "$WORK/guard.sh" >/dev/null 2>&1; then
  ok "the credential guard passes when the secret is present"
else
  bad "the credential guard passes when the secret is present"
fi

echo
printf 'Results: %d passed, %d failed\n' "$PASS" "$FAIL"
[[ "$FAIL" -eq 0 ]]
