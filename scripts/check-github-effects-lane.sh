#!/usr/bin/env bash
# check-github-effects-lane.sh — the credentialed lane's shape, asserted.
#
# `.github/workflows/github-effects.yml` is the one workflow in this repository
# that holds a credential, and two of its properties were held by prose in its
# own header plus whoever remembered to grep:
#
#   1. NEVER SKIPS. No `if:` and no `continue-on-error:` anywhere in the file, so
#      an absent secret makes the job red rather than green-by-not-running. A
#      silently skipped lane is indistinguishable from a passing one, and the
#      difference is invisible in the runs listing.
#   2. FAILS EARLY AND BY NAME. The credential guard is the first step, and the
#      workspace preflight precedes the toolchain install and the release build,
#      so a dispatch that cannot succeed costs seconds and says why.
#
# A comment cannot fail. This can. It is deliberately a shape check over the
# YAML text and not a semantic one over a parsed document, because the defect it
# guards against is someone *adding a line* — and a line is what it looks for.
#
# Comment lines are blanked before matching (line numbers preserved), so the
# header's own discussion of `if:` and `continue-on-error:` does not trip it and
# does not have to be written around.
#
# Usage: check-github-effects-lane.sh [path-to-workflow]
# Exit:  0 ok, 1 a property does not hold, 2 the file is unreadable.
#
# scripts/test-check-github-effects-lane.sh proves every check below
# discriminates, by injecting the corresponding defect and requiring a non-zero
# exit — an absence is only evidence when something would notice its return.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FILE="${1:-$ROOT/.github/workflows/github-effects.yml}"

if [[ ! -r "$FILE" ]]; then
  echo "check-github-effects-lane: cannot read $FILE" >&2
  exit 2
fi

WORK=$(mktemp -d "${TMPDIR:-/tmp}/check-effects-lane-XXXXXX") || exit 2
trap 'rm -rf "$WORK"' EXIT INT TERM

# Blank full-line comments, keeping the line count identical so reported numbers
# match the real file.
sed 's/^[[:space:]]*#.*$//' "$FILE" > "$WORK/stripped"
STRIPPED="$WORK/stripped"

FAILED=0
fail() {
  FAILED=1
  echo "check-github-effects-lane: $*" >&2
}

line_of() { grep -nF -- "$1" "$STRIPPED" | head -1 | cut -d: -f1; }

# The run: block of a named step, dedented, so it can be grepped or executed.
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

GUARD_NAME="Require FIDDLE_EFFECTS_TOKEN"
PREFLIGHT_NAME="Require a Cargo workspace on the dispatched ref"

# ---------------------------------------------------------------- property 1
# No key that could turn a missing credential into a green run.
for key in if continue-on-error; do
  hits=$(grep -nE "^[[:space:]]*(-[[:space:]]+)?${key}[[:space:]]*:" "$STRIPPED" || true)
  if [[ -n "$hits" ]]; then
    fail "'${key}:' appears in $FILE, so this lane can go green without running:"
    printf '%s\n' "$hits" | sed 's/^/    /' >&2
  fi
done

# ---------------------------------------------------------------- property 2
# The two guards exist, the credential guard is the first step, and the
# workspace preflight precedes everything expensive.
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

# ---------------------------------------------------------------- property 3
# Each guard's own shell must be capable of failing, and the preflight must name
# what to pass instead rather than only what is wrong.
guard_block=$(extract_run_block "$GUARD_NAME")
preflight_block=$(extract_run_block "$PREFLIGHT_NAME")

if [[ -z "$guard_block" ]]; then
  fail "could not read the credential guard's run: block"
else
  grep -q 'exit 1' <<<"$guard_block" ||
    fail "the credential guard's run: block has no 'exit 1'; it would report success with no credential"
  grep -q 'FIDDLE_EFFECTS_TOKEN' <<<"$guard_block" ||
    fail "the credential guard's run: block does not name FIDDLE_EFFECTS_TOKEN"
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

echo "credentialed lane ok: never skips, guards first, preflight before the build"
