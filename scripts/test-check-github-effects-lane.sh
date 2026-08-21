#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CHECKER="$SCRIPT_DIR/check-github-effects-lane.sh"
WORKFLOWS="$SCRIPT_DIR/../.github/workflows"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

PASS=0
FAIL=0
ok()  { PASS=$((PASS + 1)); echo "  PASS: $1"; }
bad() { FAIL=$((FAIL + 1)); echo "  FAIL: $1"; }

PREFLIGHT_MARKER='- name: Require a Cargo workspace on the dispatched ref'

insert_after() {
  awk -v marker="$1" -v text="$2" '
    { print }
    !done && index($0, marker) { print text; done = 1 }
  ' "$3" > "$4"
}

expect_reject() {
  local description="$1" file="$2" guard="$3" want="$4" out code=0
  out=$("$CHECKER" "$file" "$guard" 2>&1) || code=$?
  if [[ "$code" -eq 0 ]]; then
    bad "$description (checker accepted the broken file)"
  elif [[ -n "$want" ]] && ! grep -qF -- "$want" <<<"$out"; then
    bad "$description (rejected, but the reason never mentions '$want')"
  else
    ok "$description"
  fi
}

extract_run_block() {
  awk -v marker="      - name: $1" '
    index($0, marker) == 1 { instep = 1; next }
    instep && $0 == "        run: |" { inrun = 1; next }
    inrun {
      if ($0 == "") { print ""; next }
      if (substr($0, 1, 10) == "          ") { print substr($0, 11); next }
      exit
    }
  ' "$2"
}

rejects_each_defect() {
  local lane="$1" guard="$2" job="$3"
  local workflow="$WORKFLOWS/$lane"
  local guard_marker="- name: $guard"
  local work="$WORK/$lane"
  mkdir -p "$work"

  echo
  echo "A. $lane — the checker rejects each defect it exists to catch"

  if "$CHECKER" "$workflow" "$guard" >/dev/null 2>&1; then
    ok "the shipped workflow passes"
  else
    bad "the shipped workflow passes"
  fi

  insert_after "$guard_marker" "        if: \${{ github.event_name == 'workflow_dispatch' }}" \
    "$workflow" "$work/with-if.yml"
  expect_reject "an 'if:' on the credential guard is rejected" "$work/with-if.yml" "$guard" "'if:' appears"

  insert_after "$guard_marker" "        continue-on-error: true" \
    "$workflow" "$work/with-coe.yml"
  expect_reject "a 'continue-on-error:' is rejected" "$work/with-coe.yml" "$guard" "'continue-on-error:' appears"

  awk -v job="^  $job:" '{ print } $0 ~ job { print "    if: false" }' "$workflow" > "$work/job-if.yml"
  expect_reject "a job-level 'if:' is rejected" "$work/job-if.yml" "$guard" "'if:' appears"

  awk -v start="$PREFLIGHT_MARKER" '
    index($0, start) { dropping = 1 }
    dropping && /^      - uses: dtolnay\/rust-toolchain/ { dropping = 0 }
    !dropping { print }
  ' "$workflow" > "$work/no-preflight.yml"
  expect_reject "removing the workspace preflight is rejected" "$work/no-preflight.yml" "$guard" "no workspace preflight step"

  awk -v marker="$PREFLIGHT_MARKER" '
    /^      - run: cargo build --release$/ { next }
    index($0, marker) { print "      - run: cargo build --release" }
    { print }
  ' "$workflow" > "$work/late-preflight.yml"
  expect_reject "a preflight that runs after the build is rejected" "$work/late-preflight.yml" "$guard" "runs after"

  awk '/^    steps:$/ { print; print "      - uses: actions/checkout@v4"; next } { print }' \
    "$workflow" > "$work/late-guard.yml"
  expect_reject "a credential guard that is not the first step is rejected" "$work/late-guard.yml" "$guard" "first step"

  sed 's/^            exit 1$/            exit 0/' "$workflow" > "$work/soft-guard.yml"
  expect_reject "a credential guard that exits 0 is rejected" "$work/soft-guard.yml" "$guard" "no 'exit 1'"

  grep -v -- '--ref' "$workflow" > "$work/no-remedy.yml"
  expect_reject "a preflight that never names '--ref' is rejected" "$work/no-remedy.yml" "$guard" "what to pass instead"

  insert_after "        env:" "          FIDDLE_UNTESTED_TOKEN: \${{ secrets.FIDDLE_UNTESTED_TOKEN }}" \
    "$workflow" "$work/untested-secret.yml"
  expect_reject "a secret the guard receives and never tests is rejected" "$work/untested-secret.yml" "$guard" "FIDDLE_UNTESTED_TOKEN"

  grep -v 'secrets\.' "$workflow" > "$work/no-secret.yml"
  expect_reject "a guard that receives no secret at all is rejected" "$work/no-secret.yml" "$guard" "maps no secret"
}

runs_the_shipped_guards() {
  local lane="$1" guard="$2"
  shift 2
  local credentials=("$@")
  local workflow="$WORKFLOWS/$lane"
  local work="$WORK/$lane"
  mkdir -p "$work"

  echo
  echo "B. $lane — the shipped guards' own shell fires, run rather than grepped"

  extract_run_block "Require a Cargo workspace on the dispatched ref" "$workflow" > "$work/preflight.sh"
  extract_run_block "$guard" "$workflow" > "$work/guard.sh"

  [[ -s "$work/preflight.sh" ]] || bad "the preflight run: block could be extracted"
  [[ -s "$work/guard.sh" ]]     || bad "the credential guard run: block could be extracted"

  mkdir -p "$work/no-workspace"
  local out code
  out=$(cd "$work/no-workspace" && \
    GITHUB_REF=refs/heads/main GITHUB_SHA=aa86c60 \
    GITHUB_WORKSPACE="$work/no-workspace" GITHUB_REPOSITORY=peel/fiddle \
    bash "$work/preflight.sh" 2>&1)
  code=$?
  if [[ "$code" -eq 0 ]]; then
    bad "the preflight fails on a checkout with no Cargo.toml"
  elif ! grep -qF -- '--ref' <<<"$out" || ! grep -qF 'no Cargo.toml' <<<"$out"; then
    bad "the preflight's failure names the reason and the remedy (got: ${out:0:120})"
  else
    ok "the preflight fails on a checkout with no Cargo.toml, naming the reason and '--ref'"
  fi

  mkdir -p "$work/workspace"
  : > "$work/workspace/Cargo.toml"
  if (cd "$work/workspace" && \
      GITHUB_REF=refs/heads/plan/agentic-factory-m2 GITHUB_SHA=deadbee \
      GITHUB_WORKSPACE="$work/workspace" GITHUB_REPOSITORY=peel/fiddle \
      bash "$work/preflight.sh" >/dev/null 2>&1); then
    ok "the preflight passes on a checkout that has a Cargo.toml"
  else
    bad "the preflight passes on a checkout that has a Cargo.toml"
  fi

  local name
  for name in "${credentials[@]}"; do
    local present=()
    local other
    for other in "${credentials[@]}"; do
      [[ "$other" == "$name" ]] || present+=("$other=a-value")
    done
    out=$(env "${present[@]}" "$name=" GITHUB_REPOSITORY=peel/fiddle bash "$work/guard.sh" 2>&1)
    code=$?
    if [[ "$code" -eq 0 ]]; then
      bad "the credential guard fails when $name alone renders empty"
    elif ! grep -qF "$name" <<<"$(head -1 <<<"$out")"; then
      bad "the credential guard's annotation names $name (got: ${out:0:120})"
    else
      ok "the credential guard fails when $name alone renders empty, naming it"
    fi
  done

  local all=()
  for name in "${credentials[@]}"; do
    all+=("$name=not-a-real-token")
  done
  if env "${all[@]}" GITHUB_REPOSITORY=peel/fiddle bash "$work/guard.sh" >/dev/null 2>&1; then
    ok "the credential guard passes when every secret is present"
  else
    bad "the credential guard passes when every secret is present"
  fi
}

rejects_each_defect github-effects.yml "Require FIDDLE_EFFECTS_TOKEN" effects
runs_the_shipped_guards github-effects.yml "Require FIDDLE_EFFECTS_TOKEN" FIDDLE_EFFECTS_TOKEN

rejects_each_defect cve-live.yml "Require the live credentials" live
runs_the_shipped_guards cve-live.yml "Require the live credentials" \
  FIDDLE_CVE_TOKEN WIZ_CLIENT_ID WIZ_CLIENT_SECRET LITELLM_API_KEY

echo
printf 'Results: %d passed, %d failed\n' "$PASS" "$FAIL"
[[ "$FAIL" -eq 0 ]]
