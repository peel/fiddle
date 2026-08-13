#!/usr/bin/env bash
# verify-graphql-ready.sh — proves the one external contract M3 rests on, before
# anything in the build depends on it.
#
# M3 has to move a pull request from draft to ready for review.
# `PATCH /repos/{owner}/{repo}/pulls/{number}` does not accept `draft`, so that
# transition exists only as the GraphQL mutation `markPullRequestReadyForReview`.
# GraphQL answers a *refused* mutation with **HTTP 200** and a non-empty
# `errors[]`, and `crates/fiddle-runtime/src/github/cli.rs` decides failure at
# `if response.status >= 400` — so a refusal would arrive in the runtime as
# `Ok(GhResponse)`. ADR 018 is the decision that follows from what this script
# measures, and this script is the measurement it is written from.
#
# Five things are proved here and each is printed:
#
#   0. `gh` passes `-f` fields to `api graphql` as GraphQL **variables**, so a
#      `$`-parameterised mutation is expressible at all. Without this the
#      adapter would have to interpolate a node id into a query string, and
#      nothing further is worth measuring.
#   1. A refusal arrives as HTTP 200 with `errors[]` carrying a `type`, and `gh`
#      exits non-zero on that 200 — the same code a REST 404 produces, which is
#      why the exit code discriminates nothing.
#   2. `UNPROCESSABLE` is GraphQL's spelling of REST **422**, measured against
#      its own twin: the same refusal issued both ways in the same run. This is
#      what places `UNPROCESSABLE` on ADR 018's `Unknown` row.
#   3. The mutation lands, observed by reading the pull request back rather than
#      by believing the mutation's own answer — and the REST `PATCH` that looks
#      like it should do the same job answers 200 and changes nothing.
#   4. Nothing is left behind.
#
# # Why this is a script and not a test
#
# It needs a credential and real GitHub, and the gate is offline and
# credential-free. Nothing in `.github/workflows` invokes it and it is not a
# `cargo test`, so `scripts/gate.sh` cannot reach it. It is the sibling of
# `scripts/live-github.sh` and inherits that lane's rules, the first of which is
# that it **never skips**: the credential check below is `:?` and not `if`,
# because a probe that quietly no-ops when its credential is absent is
# indistinguishable from a passing one.
#
# # Why the target is guarded before anything is armed
#
# This script closes a pull request and deletes a ref on the way out.
# `scripts/live-github.sh` learned the hard way that arming that sweep before
# noticing a wrong repository leaves a destructive trap pointed at a repository
# nobody has checked — see *The target guard* in
# docs/technical/effects-repository.md. So the guard below runs before the traps
# exist and before the first byte this script writes anywhere, and its predicate
# is positive: *is this a repository built to be dirtied, and one this credential
# was deliberately given?* A denylist would be worth nothing, because the next
# dangerous value is one nobody has thought of yet.
#
# # Running it
#
#   set -a; . ./.env; set +a
#   scripts/verify-graphql-ready.sh
#
# On success it prints four `OK:` lines and exits 0, and the whole transcript is
# at `target/graphql-probe.log` (`FIDDLE_GRAPHQL_PROBE_OUT` overrides).
set -euo pipefail

# ---------------------------------------------------------------------------
# Fail loudly, never skip
# ---------------------------------------------------------------------------

: "${FIDDLE_GITHUB_TOKEN:?verify-graphql-ready.sh needs FIDDLE_GITHUB_TOKEN — a fine-grained token scoped to the disposable repository alone (see .env.example). This probe fails rather than skips, because a silently-skipped probe is indistinguishable from a passing one.}"

REPO="${FIDDLE_EFFECTS_REPO:-peel/fiddle-effects-acceptance}"
BASE=main
REF_NAMESPACE="fiddle/"

command -v gh >/dev/null 2>&1 || { echo "graphql-probe: FAIL: gh must be on PATH" >&2; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "graphql-probe: FAIL: jq must be on PATH" >&2; exit 1; }

# The probe's `gh` authenticates through the environment, never `argv`, and is
# pointed at an empty configuration directory so the credential it uses is
# provably the one this script was handed rather than whatever is in the
# operator's keychain — a probe earlier in this project read a real `gho_` token
# out of the operator's keyring into a transcript, and this is the line that
# prevents it. `GITHUB_TOKEN` is removed so an ambient token cannot answer
# instead.
SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/fiddle-graphql-XXXXXX")
export GH_TOKEN="$FIDDLE_GITHUB_TOKEN"
export GH_CONFIG_DIR="$SCRATCH/gh"
export GH_PROMPT_DISABLED=1
export NO_COLOR=1
unset GITHUB_TOKEN || true
mkdir -p "$GH_CONFIG_DIR"

# ---------------------------------------------------------------------------
# The target this probe was built for — refused before anything is armed
# ---------------------------------------------------------------------------
#
# Every line here is a read. A refusal removes the scratch directory itself,
# because the trap that would have removed it does not exist yet, and says which
# side of the trap it is on so the transcript cannot be misread as a sweep that
# ran.
refuse_target() {
  echo "graphql-probe: FAIL: $*" >&2
  echo "graphql-probe: refused before arming cleanup and before any mutation; nothing was created, nothing was deleted" >&2
  rm -rf "$SCRATCH"
  exit 2
}

# 1. A bare `owner/name`, each half beginning with an alphanumeric. The value is
#    interpolated into URL paths, so `a/b/../../c` addresses a repository other
#    than the one the operator wrote and the sweep would follow it there; the
#    leading-alphanumeric rule is what refuses `..` as a path component.
[[ "$REPO" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*/[A-Za-z0-9][A-Za-z0-9._-]*$ ]] \
  || refuse_target "$REPO is not a bare owner/name"

# 2. The credential's repository selection includes it, read as 200 on
#    `/collaborators`. This is the probe that discriminates by measurement: on
#    the credential this script is run with it answers 200 for
#    peel/fiddle-effects-acceptance and 403 for both peel/fiddle-acceptance and
#    peel/fiddle — see the probe table in docs/technical/effects-repository.md.
#    A public-repository `GET` of the repository would answer 200 regardless of
#    selection, and a probe that cannot discriminate is not evidence.
gh api "repos/$REPO/collaborators" >/dev/null 2>&1 \
  || refuse_target "$REPO is not in this credential's repository selection (/collaborators is not 200); this probe only arms a sweep against a repository its credential was deliberately given"

# 3. It is public and its default branch is `$BASE`. The committed argument for
#    both sibling lanes is that reading their target needs no credential, and a
#    private repository is by construction one somebody was trusted with.
meta=$(gh api "repos/$REPO" --jq '"\(.visibility) \(.default_branch)"')
[[ "$meta" == "public $BASE" ]] \
  || refuse_target "$REPO is visibility/default-branch '$meta', wanted 'public $BASE'"

# 4. It holds no branch that is neither `$BASE` nor ours. The target's standing
#    rule is that `main` is its only permanent branch, and that rule is the whole
#    reason deleting a ref here is defensible. Branches under `$REF_NAMESPACE`
#    are deliberately tolerated: they are this probe's own residue from an
#    interrupted run, and refusing them here would strand them forever because
#    cleanup would never arm to sweep them.
foreign=$(gh api "repos/$REPO/branches?per_page=100" --jq '.[].name' \
  | grep -v -e "^$BASE\$" -e "^$REF_NAMESPACE" || true)
[[ -z "$foreign" ]] \
  || refuse_target "$REPO holds a branch outside $BASE and $REF_NAMESPACE…: $(tr '\n' ' ' <<<"$foreign")"

echo "graphql-probe: target $REPO: bare name, selected by the credential, $meta, no foreign branch"

# ---------------------------------------------------------------------------
# The transcript
# ---------------------------------------------------------------------------
#
# Everything below is teed to a file, so the bean records a measurement rather
# than a recollection.
OUT="${FIDDLE_GRAPHQL_PROBE_OUT:-target/graphql-probe.log}"
mkdir -p "$(dirname "$OUT")"
exec > >(tee "$OUT") 2>&1

fail() { echo "FAIL: $*"; exit 1; }

# ---------------------------------------------------------------------------
# 0. `gh` passes `-f` fields to `api graphql` as GraphQL variables
# ---------------------------------------------------------------------------

echo "== 0. gh passes -f fields as GraphQL variables (verified, not assumed) =="
login=$(gh api graphql -f query='query($login: String!) { user(login: $login) { login } }' \
  -f login=peel --jq .data.user.login)
echo "\$login resolved to: $login"
[[ "$login" == peel ]] || fail "a \$-parameterised query did not resolve its variable"
echo "OK: a \$-parameterised operation is expressible; -f fields arrive as variables"

# ---------------------------------------------------------------------------
# 1. A refusal arrives as HTTP 200 with errors[], and gh exits non-zero
# ---------------------------------------------------------------------------
#
# `PR_kwDOnosuchnode` is a well-formed node id that resolves to nothing, so this
# exercises the refusal path without needing a pull request somebody else owns.
# `-i` is what makes the status line readable at all, which is ADR 015's rule and
# the only reason the 200 below is observable rather than inferred.

echo
echo "== 1. a refusal arrives as HTTP 200 with errors[] =="
set +e
out=$(gh api graphql -i \
  -f query='mutation($id: ID!) { markPullRequestReadyForReview(input: {pullRequestId: $id}) { clientMutationId } }' \
  -f id=PR_kwDOnosuchnode 2>"$SCRATCH/refusal.err")
code=$?
set -e
# Status line and body only. The response headers are dropped because they are
# noise here, not because anything in them is secret.
head -1 <<<"$out"
echo "body: $(awk 'BEGIN{b=0} b{print} /^\r?$/{b=1}' <<<"$out" | tr -d '\r' | tr -s '[:space:]' ' ')"
echo "gh stderr: $(tr -d '\r' < "$SCRATCH/refusal.err" | tr '\n' ' ')"
echo "gh exit: $code"
grep -q '^HTTP/2.0 200' <<<"$out" || fail "the refusal was not HTTP 200"
grep -q '"errors"' <<<"$out" || fail "the refusal carried no errors array"
grep -q '"type": *"NOT_FOUND"' <<<"$out" || fail "the refusal carried no error type"
grep -q '"markPullRequestReadyForReview": *null' <<<"$out" \
  || fail "the refusal did not null the mutation's own field"
[[ $code -ne 0 ]] || fail "gh exited 0 on a refused mutation"
echo "OK: 200 + data:null + errors[].type, and gh exit $code — status >= 400 would have read this as a success"

# ---------------------------------------------------------------------------
# 2. UNPROCESSABLE is GraphQL's spelling of REST 422
# ---------------------------------------------------------------------------
#
# `NOT_FOUND` alone would not settle ADR 018's classification table, because the
# row that matters most is the ambiguous one. So the same refusal — creating a
# ref that already exists — is issued both ways in the same run against the same
# repository. `refs/heads/main` is chosen because the call is refused and, were
# it somehow permitted, would create a ref that is already there: it cannot
# leave residue either way.

echo
echo "== 2. UNPROCESSABLE is GraphQL's spelling of REST 422 =="
repo_id=$(gh api graphql -f query='query($o: String!, $n: String!) { repository(owner: $o, name: $n) { id } }' \
  -f o="${REPO%%/*}" -f n="${REPO##*/}" --jq .data.repository.id)
base_sha=$(gh api "repos/$REPO/git/ref/heads/$BASE" --jq .object.sha)
set +e
gql=$(gh api graphql -i \
  -f query='mutation($id: ID!, $sha: GitObjectID!) { createRef(input: {repositoryId: $id, name: "refs/heads/'"$BASE"'", oid: $sha}) { ref { name } } }' \
  -f id="$repo_id" -f sha="$base_sha" 2>/dev/null)
gql_code=$?
rest=$(gh api -i "repos/$REPO/git/refs" -f ref="refs/heads/$BASE" -f sha="$base_sha" 2>/dev/null)
rest_code=$?
set -e
echo "graphql: $(head -1 <<<"$gql") / exit $gql_code"
echo "graphql body: $(awk 'BEGIN{b=0} b{print} /^\r?$/{b=1}' <<<"$gql" | tr -d '\r' | tr -s '[:space:]' ' ')"
echo "rest:    $(head -1 <<<"$rest") / exit $rest_code"
echo "rest body: $(awk 'BEGIN{b=0} b{print} /^\r?$/{b=1}' <<<"$rest" | tr -d '\r' | tr -s '[:space:]' ' ')"
grep -q '^HTTP/2.0 200' <<<"$gql" || fail "the GraphQL 'already exists' refusal was not HTTP 200"
grep -q '"type": *"UNPROCESSABLE"' <<<"$gql" || fail "the GraphQL 'already exists' refusal was not typed UNPROCESSABLE"
grep -q '^HTTP/2.0 422' <<<"$rest" || fail "the REST 'already exists' refusal was not HTTP 422"
echo "OK: one cause, two spellings — GraphQL 200 + UNPROCESSABLE, REST 422. Both exit $gql_code."

# ---------------------------------------------------------------------------
# 3. The mutation lands on a real draft pull request
# ---------------------------------------------------------------------------
#
# Cleanup closes the pull request as well as deleting the branch, and is armed
# before the first mutation. A cleanup that removed only the ref would leave an
# open pull request behind on any failure between the create and the end, which
# is the residue the target's standing rules forbid.

echo
echo "== 3. the mutation lands on a real draft pull request =="
BR="${REF_NAMESPACE}graphql-probe-$$"
PRN=""
cleaned=0
cleanup() {
  local status=$?
  [[ $cleaned -eq 0 ]] || return 0
  cleaned=1
  trap - EXIT INT TERM
  echo "graphql-probe: cleaning up (exit $status)"
  [[ -z "$PRN" ]] \
    || gh api -X PATCH "repos/$REPO/pulls/$PRN" -f state=closed >/dev/null 2>&1 || true
  gh api -X DELETE "repos/$REPO/git/refs/heads/$BR" >/dev/null 2>&1 || true
  local open branches
  open=$(gh api "repos/$REPO/pulls?state=open" --jq 'length')
  branches=$(gh api "repos/$REPO/branches?per_page=100" --jq '.[].name' | tr '\n' ' ')
  echo "graphql-probe: residue after cleanup: open-prs=$open branches=${branches% }"
  rm -rf "$SCRATCH"
  if [[ "$open" != 0 ]]; then
    echo "FAIL: cleanup left an open pull request behind"
    exit 1
  fi
  exit "$status"
}
# Registered on EXIT, with INT and TERM turned into an `exit` that reaches it,
# rather than as `trap cleanup EXIT INT TERM`: on a signal, `$?` at the top of a
# handler is the interrupted command's status, and an interrupted probe that
# exits 0 is the same failure as a skipped one.
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

sha=$(gh api "repos/$REPO/git/ref/heads/$BASE" --jq .object.sha)
gh api "repos/$REPO/git/refs" -f ref="refs/heads/$BR" -f sha="$sha" >/dev/null
# A pull request needs a commit between base and head — a ref created at `main`'s
# own sha is refused with 422 "No commits between …" — so the branch gets one
# file. It goes when the branch does.
gh api -X PUT "repos/$REPO/contents/graphql-probe-$$.txt" \
  -f message="graphql probe" \
  -f content="$(printf 'graphql probe\n' | base64 | tr -d '\n')" \
  -f branch="$BR" >/dev/null
echo "created $BR at ${sha:0:12} with one commit"

pr=$(gh api "repos/$REPO/pulls" -f title="graphql ready-for-review probe" \
  -f head="$BR" -f base="$BASE" -F draft=true --jq '"\(.number) \(.node_id)"')
N=${pr%% *}
ID=${pr##* }
PRN="$N"
echo "opened pull request #$N as node $ID"

[[ "$(gh api "repos/$REPO/pulls/$N" --jq .draft)" == true ]] \
  || fail "the pull request was not created as a draft, so there is nothing to mark ready"
echo "read back: draft=true"

# The REST call a first draft of M3's design had here, measured. `PATCH
# /repos/{o}/{r}/pulls/{n}` does not list `draft` among its body parameters, and
# what that means in practice is worse than a refusal: the field is accepted,
# ignored, and reported **200 OK**. A REST implementation would have believed a
# success that moved nothing, which is the same failure ADR 018 is about arriving
# from the other side.
# Piping `gh` into `head` would close its stdout and, under `pipefail`, report the
# resulting SIGPIPE as this script's own failure. The whole response is captured
# and the first line taken from the variable.
rest_patch=$(gh api -i -X PATCH "repos/$REPO/pulls/$N" -F draft=false 2>/dev/null)
rest_patch=$(head -1 <<<"$rest_patch")
echo "REST PATCH draft=false: $rest_patch"
still=$(gh api "repos/$REPO/pulls/$N" --jq .draft)
echo "draft after that PATCH: $still"
grep -q '^HTTP/2.0 200' <<<"$rest_patch" || fail "the REST PATCH did not answer 200, so this measurement needs redoing"
[[ "$still" == true ]] \
  || fail "REST PATCH draft=false actually worked; the GraphQL-only premise is refuted and ADR 018 needs rewriting"
echo "OK: REST answered 200 and the pull request is still a draft — the transition is GraphQL-only"

mutated=$(gh api graphql \
  -f query='mutation($id: ID!) { markPullRequestReadyForReview(input: {pullRequestId: $id}) { pullRequest { isDraft } } }' \
  -f id="$ID")
echo "mutation answered: $mutated"
# That answer is not the verdict. The pull request is read back
# over REST, which is the postcondition read the effect protocol performs on
# every path.
[[ "$(gh api "repos/$REPO/pulls/$N" --jq .draft)" == false ]] \
  || fail "the pull request is still a draft after the mutation"
echo "OK: draft -> ready, observed by reading the pull request back over REST"

# ---------------------------------------------------------------------------
# 4. No residue
# ---------------------------------------------------------------------------

echo
echo "== 4. cleanup, and no residue =="
# The transcript must not carry the credential. Checked before the pass line, so
# a leak is a failure rather than a redaction.
if grep -qF "$FIDDLE_GITHUB_TOKEN" "$OUT" 2>/dev/null; then
  fail "the transcript holds the credential"
fi
echo "OK: the transcript holds no credential"
echo "graphql-probe: PASS"
# `cleanup` runs from the EXIT trap and asserts zero open pull requests itself,
# so the assertion is made on the failing paths too and not only on this one.
