#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FILE="${1:-$ROOT/.github/workflows/github-effects.yml}"
GUARD_NAME="${2:-Require FIDDLE_EFFECTS_TOKEN}"

if [[ ! -r "$FILE" ]]; then
  echo "check-github-effects-lane: cannot read $FILE" >&2
  exit 2
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/check-effects-lane-XXXXXX") || exit 2
trap 'rm -rf "$WORK"' EXIT INT TERM

sed 's/^[[:space:]]*#.*$//' "$FILE" > "$WORK/stripped"
STRIPPED="$WORK/stripped"

FAILED=0
fail() {
  FAILED=1
  echo "check-github-effects-lane: $*" >&2
}

line_of() { grep -nF -- "$1" "$STRIPPED" | head -1 | cut -d: -f1; }

extract_run_block() {
  awk -v marker="      - name: $1" '
    index($0, marker) == 1 { instep = 1; next }
    instep && $0 == "        run: |" { inrun = 1; next }
    inrun {
      if ($0 == "") { print ""; next }
      if (substr($0, 1, 10) == "          ") { print substr($0, 11); next }
      exit
    }
  ' "$FILE"
}

PREFLIGHT_NAME="Require a Cargo workspace on the dispatched ref"

guard_secrets() {
  awk -v marker="      - name: $GUARD_NAME" '
    index($0, marker) == 1 { instep = 1; next }
    instep && $0 == "        env:" { inenv = 1; next }
    inenv {
      if ($0 ~ /^          [A-Za-z_][A-Za-z0-9_]*:[[:space:]]*\$\{\{[[:space:]]*secrets\./) {
        name = $1
        sub(/:$/, "", name)
        print name
        next
      }
      exit
    }
  ' "$FILE"
}

for key in if continue-on-error; do
  hits=$(grep -nE "^[[:space:]]*(-[[:space:]]+)?${key}[[:space:]]*:" "$STRIPPED" || true)
  if [[ -n "$hits" ]]; then
    fail "'${key}:' appears in $FILE, so this lane can go green without running:"
    printf '%s\n' "$hits" | sed 's/^/    /' >&2
  fi
done

guard_line=$(line_of "- name: $GUARD_NAME")
preflight_line=$(line_of "- name: $PREFLIGHT_NAME")
toolchain_line=$(line_of "uses: dtolnay/rust-toolchain")
build_line=$(line_of "run: cargo build --release")
first_step_line=$(grep -nE '^      - ' "$STRIPPED" | head -1 | cut -d: -f1)

[[ -n "$guard_line"     ]] || fail "no credential guard step named '$GUARD_NAME'"
[[ -n "$preflight_line" ]] || fail "no workspace preflight step named '$PREFLIGHT_NAME'"
[[ -n "$toolchain_line" ]] || fail "no dtolnay/rust-toolchain step to order the preflight against"
[[ -n "$build_line"     ]] || fail "no 'cargo build --release' step to order the preflight against"

if [[ -n "$guard_line" && -n "$first_step_line" && "$guard_line" != "$first_step_line" ]]; then
  fail "the credential guard is at line $guard_line but the first step is at line $first_step_line; an absent credential must cost seconds, not a build"
fi

if [[ -n "$preflight_line" && -n "$toolchain_line" ]] && (( preflight_line > toolchain_line )); then
  fail "the workspace preflight (line $preflight_line) runs after the toolchain install (line $toolchain_line); it must precede it"
fi

if [[ -n "$preflight_line" && -n "$build_line" ]] && (( preflight_line > build_line )); then
  fail "the workspace preflight (line $preflight_line) runs after 'cargo build --release' (line $build_line), which is the error it exists to pre-empt"
fi

guard_block=$(extract_run_block "$GUARD_NAME")
preflight_block=$(extract_run_block "$PREFLIGHT_NAME")

if [[ -z "$guard_block" ]]; then
  fail "could not read the credential guard's run: block"
else
  grep -q 'exit 1' <<<"$guard_block" ||
    fail "the credential guard's run: block has no 'exit 1'; it would report success with no credential"

  tested=$(grep -v '^[[:space:]]*echo' <<<"$guard_block" || true)
  credentials=$(guard_secrets)
  if [[ -z "$credentials" ]]; then
    fail "the credential guard of $FILE maps no secret into its environment, so it tests nothing and the lane runs uncredentialled"
  else
    while read -r name; do
      grep -q -- "$name" <<<"$tested" ||
        fail "the credential guard receives $name and no line of its run: block outside an echo names it; a guard that only writes the name into its diagnostic tests nothing, and the lane fails deep inside a run or passes over an empty value"
    done <<<"$credentials"
  fi
fi

if [[ -z "$preflight_block" ]]; then
  fail "could not read the workspace preflight's run: block"
else
  grep -q 'exit 1' <<<"$preflight_block" ||
    fail "the workspace preflight's run: block has no 'exit 1'; it would wave a workspace-less ref through to cargo"
  grep -q 'Cargo.toml' <<<"$preflight_block" ||
    fail "the workspace preflight's run: block does not name Cargo.toml, which is the thing it checks for"
  grep -q -- '--ref' <<<"$preflight_block" ||
    fail "the workspace preflight's run: block does not name '--ref', so it says what is wrong without saying what to pass instead"
fi

if [[ "$FAILED" -ne 0 ]]; then
  echo "check-github-effects-lane: FAIL ($FILE)" >&2
  exit 1
fi

echo "credentialed lane ok ($FILE): never skips, guards first, preflight before the build, guard tests every secret it receives"
