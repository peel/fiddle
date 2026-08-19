#!/usr/bin/env bash
set -euo pipefail


: "${FIDDLE_GITHUB_TOKEN:?verify-graphql-ready.sh needs FIDDLE_GITHUB_TOKEN — a fine-grained token scoped to the disposable repository alone (see .env.example). This probe fails rather than skips, because a silently-skipped probe is indistinguishable from a passing one.}"

REPO="${FIDDLE_EFFECTS_REPO:-peel/fiddle-effects-acceptance}"
BASE=main
REF_NAMESPACE="fiddle/"

command -v gh >/dev/null 2>&1 || { echo "graphql-probe: FAIL: gh must be on PATH" >&2; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "graphql-probe: FAIL: jq must be on PATH" >&2; exit 1; }

SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/fiddle-graphql-XXXXXX")
export GH_TOKEN="$FIDDLE_GITHUB_TOKEN"
export GH_CONFIG_DIR="$SCRATCH/gh"
export GH_PROMPT_DISABLED=1
export NO_COLOR=1
unset GITHUB_TOKEN || true
mkdir -p "$GH_CONFIG_DIR"

refuse_target() {
  echo "graphql-probe: FAIL: $*" >&2
  echo "graphql-probe: refused before arming cleanup and before any mutation; nothing was created, nothing was deleted" >&2
  rm -rf "$SCRATCH"
  exit 2
}

[[ "$REPO" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*/[A-Za-z0-9][A-Za-z0-9._-]*$ ]] \
  || refuse_target "$REPO is not a bare owner/name"

gh api "repos/$REPO/collaborators" >/dev/null 2>&1 \
  || refuse_target "$REPO is not in this credential's repository selection (/collaborators is not 200); this probe only arms a sweep against a repository its credential was deliberately given"

meta=$(gh api "repos/$REPO" --jq '"\(.visibility) \(.default_branch)"')
[[ "$meta" == "public $BASE" ]] \
  || refuse_target "$REPO is visibility/default-branch '$meta', wanted 'public $BASE'"

foreign=$(gh api "repos/$REPO/branches?per_page=100" --jq '.[].name' \
  | grep -v -e "^$BASE\$" -e "^$REF_NAMESPACE" || true)
[[ -z "$foreign" ]] \
  || refuse_target "$REPO holds a branch outside $BASE and $REF_NAMESPACE…: $(tr '\n' ' ' <<<"$foreign")"

echo "graphql-probe: target $REPO: bare name, selected by the credential, $meta, no foreign branch"

OUT="${FIDDLE_GRAPHQL_PROBE_OUT:-target/graphql-probe.log}"
mkdir -p "$(dirname "$OUT")"
exec > >(tee "$OUT") 2>&1

fail() { echo "FAIL: $*"; exit 1; }


echo "== 0. gh passes -f fields as GraphQL variables (verified, not assumed) =="
login=$(gh api graphql -f query='query($login: String!) { user(login: $login) { login } }' \
  -f login=peel --jq .data.user.login)
echo "\$login resolved to: $login"
[[ "$login" == peel ]] || fail "a \$-parameterised query did not resolve its variable"
echo "OK: a \$-parameterised operation is expressible; -f fields arrive as variables"


echo
echo "== 1. a refusal arrives as HTTP 200 with errors[] =="
set +e
out=$(gh api graphql -i \
  -f query='mutation($id: ID!) { markPullRequestReadyForReview(input: {pullRequestId: $id}) { clientMutationId } }' \
  -f id=PR_kwDOnosuchnode 2>"$SCRATCH/refusal.err")
code=$?
set -e
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
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

sha=$(gh api "repos/$REPO/git/ref/heads/$BASE" --jq .object.sha)
gh api "repos/$REPO/git/refs" -f ref="refs/heads/$BR" -f sha="$sha" >/dev/null
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
[[ "$(gh api "repos/$REPO/pulls/$N" --jq .draft)" == false ]] \
  || fail "the pull request is still a draft after the mutation"
echo "OK: draft -> ready, observed by reading the pull request back over REST"


echo
echo "== 4. cleanup, and no residue =="
if grep -qF "$FIDDLE_GITHUB_TOKEN" "$OUT" 2>/dev/null; then
  fail "the transcript holds the credential"
fi
echo "OK: the transcript holds no credential"
echo "graphql-probe: PASS"
