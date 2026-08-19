#!/usr/bin/env bash
set -euo pipefail


REPO="${FIDDLE_EFFECTS_REPO:-peel/fiddle-effects-acceptance}"
WORKFLOW="fiddle-check.yml"
BASE="main"

PROJECT="icecube"
WORK_ID="fiddle-live-publish"
INVOCATION_REF="beans:$WORK_ID"

DECISION_WORK_ID="fiddle-live-decide"
DECISION_REF="beans:$DECISION_WORK_ID"

DECISION="${FIDDLE_LIVE_DECISION:-}"

DECISION_MODEL="${FIDDLE_LIVE_MODEL:-bedrock/moonshotai.kimi-k2.5}"
DECISION_BASE_URL="${FIDDLE_LIVE_BASE_URL:-https://litellm.firn.snplow.net/v1}"
DECISION_CREDENTIAL="${FIDDLE_LIVE_MODEL_CREDENTIAL:-LITELLM_API_KEY}"

DECISION_REPLY="${FIDDLE_LIVE_DECISION_REPLY:-Approved. Please mark it ready for review.}"

DECISION_MARKER="<!-- fiddle:decision "

REF_NAMESPACE="fiddle/"
RUN_NAMESPACE="fiddle-"

RETRYABLE=11
MAX_ATTEMPTS=6
BACKOFF=4

fail() {
  echo "live-github: FAIL: $*" >&2
  exit 1
}

note() { echo "live-github: $*"; }


: "${FIDDLE_GITHUB_TOKEN:?live-github.sh needs FIDDLE_GITHUB_TOKEN — a fine-grained token scoped to the disposable repository alone. This lane fails rather than skips, because a silently-skipped lane is indistinguishable from a passing one.}"
: "${FIDDLE_BIN:?live-github.sh needs FIDDLE_BIN — the path to the compiled fiddle, as in FIDDLE_BIN=\"\$PWD/target/release/fiddle\"}"

[ -x "$FIDDLE_BIN" ] || fail "FIDDLE_BIN is not an executable file: $FIDDLE_BIN"

case "$DECISION" in
  "" | 1) ;;
  *) fail "FIDDLE_LIVE_DECISION is \"$DECISION\"; the decision phase is requested with exactly 1, and any other value is refused rather than read as a decline" ;;
esac
if [ "$DECISION" = 1 ] && [ -z "${!DECISION_CREDENTIAL:-}" ]; then
  fail "the decision phase needs $DECISION_CREDENTIAL — the credential for the model gateway at $DECISION_BASE_URL, which the attempt and the interpretation both call. It is requested, not optional: a phase that no-opped for want of a key would look exactly like one that passed"
fi
command -v gh >/dev/null 2>&1 || fail "gh must be on PATH"
command -v jq >/dev/null 2>&1 || fail "jq must be on PATH"
command -v git >/dev/null 2>&1 || fail "git must be on PATH"


TMP=$(mktemp -d "${TMPDIR:-/tmp}/fiddle-live-XXXXXX")

export GH_TOKEN="$FIDDLE_GITHUB_TOKEN"
export GH_CONFIG_DIR="$TMP/gh-harness"
export GH_PROMPT_DISABLED=1
export NO_COLOR=1
unset GITHUB_TOKEN || true
mkdir -p "$GH_CONFIG_DIR"


all_branches() { gh api "repos/$REPO/branches?per_page=100" --jq '.[].name'; }

our_branches() { all_branches | { grep "^$REF_NAMESPACE" || true; }; }

our_open_prs() {
  gh pr list --repo "$REPO" --state open --limit 100 \
    --json number,headRefName \
    --jq ".[] | select(.headRefName | startswith(\"$REF_NAMESPACE\")) | .number"
}

our_runs() {
  gh api "repos/$REPO/actions/workflows/$WORKFLOW/runs?per_page=100" \
    --jq ".workflow_runs[] | select(.name != null and (.name | startswith(\"$RUN_NAMESPACE\"))) | \"\(.id) \(.name)\""
}

lines() { grep -c . || true; }

counted() {
  local out
  if ! out=$("$@"); then
    printf 'unreadable\n'
    return 0
  fi
  printf '%s\n' "$out" | lines
}


refuse_target() {
  echo "live-github: FAIL: $*" >&2
  echo "live-github: refused before arming cleanup and before any mutation; nothing was created, nothing was deleted" >&2
  rm -rf "$TMP"
  exit 1
}

note "target repository: $REPO"

[[ "$REPO" =~ ^[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9][A-Za-z0-9._-]*$ ]] \
  || refuse_target "FIDDLE_EFFECTS_REPO must be a bare owner/name, and this is not one: $REPO"

target_facts=$(gh api "repos/$REPO" --jq '"\(.visibility) \(.default_branch)"' 2>/dev/null) \
  || refuse_target "$REPO cannot be read; it does not exist or this credential cannot see it"
read -r target_visibility target_default <<<"$target_facts"
note "visibility=$target_visibility default_branch=$target_default"

[ "$target_visibility" = public ] \
  || refuse_target "$REPO is $target_visibility; this lane only runs against a public disposable repository, because reading its target must need no credential"
[ "$target_default" = "$BASE" ] \
  || refuse_target "$REPO's default branch is $target_default, not $BASE; this lane publishes on top of $BASE"

foreign_branches=$(all_branches | { grep -v -e "^$BASE\$" -e "^$REF_NAMESPACE" || true; } | paste -sd, -) \
  || refuse_target "$REPO's branches cannot be read"
[ -z "$foreign_branches" ] \
  || refuse_target "$REPO holds branches that are neither $BASE nor ${REF_NAMESPACE}…: $foreign_branches — somebody works here, and this lane will not arm a ref-DELETE sweep in a repository that is not disposable"
note "no branch outside $BASE and ${REF_NAMESPACE}… at the remote"

gh api "repos/$REPO/collaborators" >/dev/null 2>&1 \
  || refuse_target "$REPO is not in this credential's repository selection (/collaborators is not 200); this lane only sweeps a repository its credential was deliberately given"
note "$REPO is in the credential's repository selection (200 on /collaborators)"

if gh api "repos/$REPO/actions/secrets" >/dev/null 2>&1; then
  refuse_target "the lane's credential can enumerate $REPO's secrets; it is scoped too broadly"
fi
note "the lane's credential cannot enumerate secrets (403), so it cannot leak one"

WORKFLOW_YAML=$(gh api "repos/$REPO/contents/.github/workflows/$WORKFLOW" \
  -H "Accept: application/vnd.github.raw" 2>/dev/null) \
  || refuse_target "the dispatch target workflow .github/workflows/$WORKFLOW is not installed in $REPO; this is not a repository this lane was built for"

grep -q 'run-name: fiddle-${{ inputs.fiddle_effect_id }}' <<<"$WORKFLOW_YAML" \
  || refuse_target "$WORKFLOW must echo the effect id through run-name; got:
$WORKFLOW_YAML"
note "run-name echo present in $WORKFLOW"

note "target accepted; arming cleanup"


COMMENT_RESIDUE=n/a
SWEPT_CONVERSATIONS=n/a

REVIEW_COMMENT_RESIDUE=n/a

decision_cleanup() {
  set +e
  local number ids deleted left review
  local prs status=0
  prs=$(our_open_prs) || status=$?
  if [ "$status" != 0 ]; then
    COMMENT_RESIDUE=unreadable
    SWEPT_CONVERSATIONS=unreadable
    REVIEW_COMMENT_RESIDUE=unreadable
    echo "live-github: could not list open pull requests to clear conversations from" >&2
    return 0
  fi
  SWEPT_CONVERSATIONS=$(printf '%s\n' "$prs" | lines)
  COMMENT_RESIDUE=0
  REVIEW_COMMENT_RESIDUE=0
  while read -r number; do
    [ -n "$number" ] || continue
    if ! ids=$(gh api "repos/$REPO/issues/$number/comments?per_page=100" --jq '.[].id'); then
      COMMENT_RESIDUE=unreadable
      continue
    fi
    deleted=0
    for id in $ids; do
      gh api "repos/$REPO/issues/comments/$id" --method DELETE >/dev/null 2>&1 \
        && deleted=$((deleted + 1))
    done
    if ! left=$(gh api "repos/$REPO/issues/$number/comments?per_page=100" --jq 'length'); then
      COMMENT_RESIDUE=unreadable
      continue
    fi
    if ! review=$(gh api "repos/$REPO/pulls/$number/comments?per_page=100" --jq 'length'); then
      REVIEW_COMMENT_RESIDUE=unreadable
    elif [ "$REVIEW_COMMENT_RESIDUE" != unreadable ]; then
      REVIEW_COMMENT_RESIDUE=$((REVIEW_COMMENT_RESIDUE + review))
    fi
    echo "live-github: cleared #$number's conversation: found $(printf '%s\n' "$ids" | lines), deleted $deleted, left $left"
    [ "$COMMENT_RESIDUE" = unreadable ] || COMMENT_RESIDUE=$((COMMENT_RESIDUE + left))
  done <<<"$prs"
}

cleanup() {
  local status=$?
  set +e
  trap - EXIT INT TERM

  echo "live-github: cleaning up (exit $status)"
  local number branch run id tries
  while read -r number; do
    [ -n "$number" ] || continue
    gh pr close --repo "$REPO" "$number" --delete-branch >/dev/null 2>&1
  done < <(our_open_prs)

  while read -r branch; do
    [ -n "$branch" ] || continue
    gh api "repos/$REPO/git/refs/heads/$branch" --method DELETE >/dev/null 2>&1
  done < <(our_branches)

  while read -r id run; do
    [ -n "$id" ] || continue
    gh api "repos/$REPO/actions/runs/$id" --method DELETE >/dev/null 2>&1 && continue
    gh api "repos/$REPO/actions/runs/$id/cancel" --method POST >/dev/null 2>&1
    tries=0
    until gh api "repos/$REPO/actions/runs/$id" --method DELETE >/dev/null 2>&1; do
      tries=$((tries + 1))
      [ "$tries" -ge 30 ] && break
      sleep 4
    done
  done < <(our_runs)

  local left_branches left_prs left_runs every
  left_branches=$(counted our_branches)
  left_prs=$(counted our_open_prs)
  left_runs=$(counted our_runs)
  if ! every=$(all_branches | paste -sd, -); then
    every=unreadable
  fi

  rm -rf "$TMP"

  echo "live-github: residue after cleanup: ${REF_NAMESPACE}branches=$left_branches open-prs=$left_prs ${RUN_NAMESPACE}runs=$left_runs comments=$COMMENT_RESIDUE review-comments=$REVIEW_COMMENT_RESIDUE over $SWEPT_CONVERSATIONS conversation(s)"
  echo "live-github: branches at the remote: $every"

  if [ "$left_branches" = unreadable ] || [ "$left_prs" = unreadable ] \
    || [ "$left_runs" = unreadable ] || [ "$every" = unreadable ]; then
    echo "live-github: FAIL: could not read the remote to check for residue (branches=$left_branches open-prs=$left_prs runs=$left_runs every=$every); nothing was established about what this lane left behind" >&2
    exit 1
  fi
  if [ "$left_branches" != 0 ] || [ "$left_prs" != 0 ] || [ "$left_runs" != 0 ]; then
    echo "live-github: FAIL: cleanup left residue behind" >&2
    exit 1
  fi
  if [ "$COMMENT_RESIDUE" != 0 ] && [ "$COMMENT_RESIDUE" != n/a ]; then
    echo "live-github: FAIL: $COMMENT_RESIDUE comment(s) survived the conversation sweep over $SWEPT_CONVERSATIONS conversation(s); a closed pull request keeps them for ever" >&2
    exit 1
  fi
  if [ "$REVIEW_COMMENT_RESIDUE" != 0 ] && [ "$REVIEW_COMMENT_RESIDUE" != n/a ]; then
    echo "live-github: FAIL: $REVIEW_COMMENT_RESIDUE review comment(s) on pulls/*/comments, a collection this sweep reads and does not delete; nothing in the walk posts one, so either something changed or somebody reviewed by hand — remove them and say which" >&2
    exit 1
  fi
  if [ "$every" != "$BASE" ]; then
    echo "live-github: FAIL: branches must be exactly $BASE, got $every" >&2
    exit 1
  fi
  if [ "$status" != 0 ]; then
    echo "live-github: did not pass (exit $status)" >&2
  fi
  exit "$status"
}

interrupted() {
  echo "live-github: interrupted by SIG$1" >&2
  exit "$2"
}
trap cleanup EXIT
trap 'interrupted INT 130' INT
trap 'interrupted TERM 143' TERM


before_branches=$(our_branches | lines)
before_prs=$(our_open_prs | lines)
before_runs=$(our_runs | lines)
note "before: ${REF_NAMESPACE}branches=$before_branches open-prs=$before_prs ${RUN_NAMESPACE}runs=$before_runs"
[ "$before_branches" = 0 ] || fail "the remote already holds a ${REF_NAMESPACE} branch; a zero-then-one walk cannot start here"
[ "$before_prs" = 0 ] || fail "the remote already holds an open ${REF_NAMESPACE} pull request"
[ "$before_runs" = 0 ] || fail "the remote already holds a ${RUN_NAMESPACE} workflow run"


mkdir -p "$TMP/stub-state/work" "$TMP/stub-state/changes" "$TMP/gh-config"

git -c credential.helper= clone --quiet "https://github.com/$REPO.git" "$TMP/work" \
  || fail "could not clone $REPO without a credential; it is meant to be public"

printf 'the change fiddle publishes, made at %s\n' "$(date -u +%FT%TZ)" \
  > "$TMP/work/live-probe.txt"
git -C "$TMP/work" add -A
git -C "$TMP/work" \
  -c user.email=live-github@fiddle.invalid -c user.name="fiddle live lane" \
  commit -qm "the change fiddle publishes"
HEAD_SHA=$(git -C "$TMP/work" rev-parse HEAD)
note "worktree HEAD is $HEAD_SHA, one commit ahead of $BASE"

printf '{"id":"%s","status":"open"}\n' "$WORK_ID" > "$TMP/stub-state/work/$WORK_ID.json"

cat > "$TMP/fiddle.toml" <<TOML
[project]
name = "$PROJECT"

[stub]
root = "$TMP/stub-state"

[report]
dir = "$TMP/reports"

[github]
repo = "$REPO"
base = "$BASE"
token = { env = "FIDDLE_GITHUB_TOKEN" }
work = "$TMP/work"
workflow = "$WORKFLOW"
config_dir = "$TMP/gh-config"
timeout = "300s"
TOML


printf '%s\n' "$FIDDLE_GITHUB_TOKEN" > "$TMP/needle"
chmod 600 "$TMP/needle"

PAYLOAD=""
ATTEMPTS=0

forget_everything_local() {
  rm -rf "$TMP/reports"
  rm -f "$TMP"/stub-state/changes/*
  [ ! -e "$TMP/reports" ] || fail "no local record of an earlier attempt may survive"
}

summarize_run() {
  local label="$1" code="$2"
  if grep -F -q -f "$TMP/needle" "$TMP/$label.json" "$TMP/$label.err"; then
    fail "$label: a credential reached fiddle's own output; not printing it"
  fi

  jq -c '{outcome, capability: .capability_executions[0].status,
          evidence: .progress[0].evidence}' "$TMP/$label.json" \
    | sed "s/^/live-github: $label (exit $code): /" \
    || fail "$label: stdout is not the JSON payload:
$(cat "$TMP/$label.json")"
  PAYLOAD="$TMP/$label.json"
}

publish_once() {
  local label="$1" code=0
  "$FIDDLE_BIN" run "$INVOCATION_REF" \
    --capability publish_change --json \
    --config "$TMP/fiddle.toml" \
    > "$TMP/$label.json" 2> "$TMP/$label.err" || code=$?

  summarize_run "$label" "$code"
  return "$code"
}

converge() {
  local phase="$1" attempt=0 code
  while :; do
    attempt=$((attempt + 1))
    ATTEMPTS=$((ATTEMPTS + 1))
    forget_everything_local
    code=0
    publish_once "$phase-$attempt" || code=$?
    if [ "$code" = 0 ]; then
      note "$phase: completed after $attempt fresh process(es)"
      return 0
    fi
    [ "$code" = "$RETRYABLE" ] || fail "$phase-$attempt: fiddle exited $code, which is settled and not retryable
$(cat "$TMP/$phase-$attempt.err")"
    [ "$attempt" -lt "$MAX_ATTEMPTS" ] \
      || fail "$phase: $MAX_ATTEMPTS fresh processes all ended retryable; the world never settled"
    note "$phase-$attempt: retryable, and that is what exit $RETRYABLE means — running again in ${BACKOFF}s"
    sleep "$BACKOFF"
  done
}

receipt() {
  local kind="$1" field="$2"
  jq -r --arg kind "$kind" --arg field "$field" \
    '[.progress[0].evidence[]? | select(startswith("effect:" + $kind + ":"))] | first // ""
     | split(":")
     | if $field == "postcondition" then (.[5:] | join(":"))
       elif $field == "external_ref" then (.[4] // "")
       else (.[2] // "") end' "$PAYLOAD"
}

identity() { receipt "$1" identity; }


note "--- publishing ---"
converge publish

first_branch_id=$(identity ensure_branch_published)
first_pr_id=$(identity ensure_pull_request)
first_check_id=$(identity ensure_check_requested)
for pair in "branch:$first_branch_id" "pull request:$first_pr_id" "check:$first_check_id"; do
  [ -n "${pair#*:}" ] || fail "the ${pair%%:*} receipt must reach the bundle of a completed run"
done

after_branches=$(our_branches)
after_prs=$(our_open_prs)
after_runs=$(our_runs)
note "after publishing: branches=[$(paste -sd, - <<<"$after_branches")] open-prs=[$(paste -sd, - <<<"$after_prs")] runs=[$(paste -sd';' - <<<"$after_runs")]"

[ "$(lines <<<"$after_branches")" = 1 ] || fail "exactly one ${REF_NAMESPACE} branch expected, got [$after_branches]"
[ "$(lines <<<"$after_prs")" = 1 ] || fail "exactly one open pull request expected, got [$after_prs]"
[ "$(lines <<<"$after_runs")" = 1 ] || fail "exactly one requested check expected, got [$after_runs]"

branch=$(tr -d ' ' <<<"$after_branches")
pr=$(tr -d ' ' <<<"$after_prs")
run_id=$(awk '{print $1}' <<<"$after_runs")
run_name=$(awk '{print $2}' <<<"$after_runs")

[ "$run_name" = "${RUN_NAMESPACE}${first_check_id}" ] \
  || fail "the run GitHub lists must be titled ${RUN_NAMESPACE}${first_check_id}, got $run_name"

[ "$(receipt ensure_branch_published postcondition)" = "refs/heads/$branch points at $HEAD_SHA" ] \
  || fail "the branch receipt must be about refs/heads/$branch at $HEAD_SHA, got \"$(receipt ensure_branch_published postcondition)\""

[ "$(receipt ensure_pull_request external_ref)" = "$pr" ] \
  || fail "the pull request receipt names #$(receipt ensure_pull_request external_ref), the remote holds #$pr"
[ "$(receipt ensure_check_requested external_ref)" = "$run_id" ] \
  || fail "the check receipt names run $(receipt ensure_check_requested external_ref), the remote lists $run_id"
note "round trip: ref $branch, pull request #$pr, run $run_id titled $run_name"

remote_sha=$(gh api "repos/$REPO/git/ref/heads/$branch" --jq '.object.sha')
[ "$remote_sha" = "$HEAD_SHA" ] \
  || fail "the remote ref must point at the published commit $HEAD_SHA, got $remote_sha"

gh pr view --repo "$REPO" "$pr" --json headRefName,baseRefName,state \
  --jq '"live-github: pull request #'"$pr"': \(.state) \(.headRefName) -> \(.baseRefName)"'
pr_head=$(gh pr view --repo "$REPO" "$pr" --json headRefName --jq '.headRefName')
pr_base=$(gh pr view --repo "$REPO" "$pr" --json baseRefName --jq '.baseRefName')
[ "$pr_head" = "$branch" ] || fail "pull request #$pr is opened from $pr_head, not $branch"
[ "$pr_base" = "$BASE" ] || fail "pull request #$pr is opened against $pr_base, not $BASE"


note "--- republishing the same work, nothing local left to read ---"
converge republish

[ "$(identity ensure_branch_published)" = "$first_branch_id" ] \
  || fail "the retry must derive the branch identity the first process derived"
[ "$(identity ensure_pull_request)" = "$first_pr_id" ] \
  || fail "the retry must derive the pull request identity the first process derived"
[ "$(identity ensure_check_requested)" = "$first_check_id" ] \
  || fail "the retry must derive the check identity the first process derived"

[ "$(receipt ensure_branch_published external_ref)" = "$HEAD_SHA" ] \
  || fail "the retry must have observed the branch at $HEAD_SHA"
[ "$(receipt ensure_pull_request external_ref)" = "$pr" ] \
  || fail "the retry must have observed pull request #$pr"
[ "$(receipt ensure_check_requested external_ref)" = "$run_id" ] \
  || fail "the retry must have observed workflow run $run_id"

final_branches=$(our_branches)
final_prs=$(our_open_prs)
final_runs=$(our_runs)
note "after republishing: branches=[$(paste -sd, - <<<"$final_branches")] open-prs=[$(paste -sd, - <<<"$final_prs")] runs=[$(paste -sd';' - <<<"$final_runs")]"

[ "$(lines <<<"$final_branches")" = 1 ] || fail "exactly one ${REF_NAMESPACE} branch expected, got [$final_branches]"
[ "$(lines <<<"$final_prs")" = 1 ] || fail "exactly one open pull request expected, got [$final_prs]"
[ "$(lines <<<"$final_runs")" = 1 ] || fail "exactly one requested check expected, got [$final_runs]"

[ "$(tr -d ' ' <<<"$final_branches")" = "$branch" ] || fail "the branch must still be $branch, got [$final_branches]"
[ "$(tr -d ' ' <<<"$final_prs")" = "$pr" ] || fail "the pull request must still be #$pr, got [$final_prs]"
[ "$(awk '{print $1}' <<<"$final_runs")" = "$run_id" ] || fail "the workflow run must still be $run_id, got [$final_runs]"
[ "$(gh api "repos/$REPO/git/ref/heads/$branch" --jq '.object.sha')" = "$HEAD_SHA" ] \
  || fail "the branch must still point at $HEAD_SHA"

[ "$ATTEMPTS" -ge 2 ] || fail "at least two fresh processes must have run, got $ATTEMPTS"
note "$ATTEMPTS fresh fiddle processes ran over the same work; the remote holds exactly \
one branch, one pull request and one requested check, and the same three throughout"


if [ "$DECISION" != 1 ]; then
  note "decision phase: NOT REQUESTED (FIDDLE_LIVE_DECISION is unset), so nothing \
below the publish walk was exercised and this transcript says nothing about the \
decision channel"
  note "PASS (publish, republish)"
  exit 0
fi

trap 'status=$?; decision_cleanup; (exit $status); cleanup' EXIT
note "--- the decision channel: one question, one answer, one ready transition ---"
note "model $DECISION_MODEL at $DECISION_BASE_URL, credential from \$$DECISION_CREDENTIAL"

printf '%s\n' "${!DECISION_CREDENTIAL}" >> "$TMP/needle"

DECISION_FIXTURE="$TMP/decision-fixture"
git -c credential.helper= clone --quiet "https://github.com/$REPO.git" "$DECISION_FIXTURE" \
  || fail "could not clone $REPO without a credential; it is meant to be public"
mkdir -p "$DECISION_FIXTURE/src"
printf 'pub fn last_index(len: usize) -> usize { len }\n' > "$DECISION_FIXTURE/src/lib.rs"
git -C "$DECISION_FIXTURE" add -A
git -C "$DECISION_FIXTURE" \
  -c user.email=live-github@fiddle.invalid -c user.name="fiddle live lane" \
  commit -qm "a fixture that fails its own check"
DECISION_FIXTURE_SHA=$(git -C "$DECISION_FIXTURE" rev-parse HEAD)
note "fixture at $DECISION_FIXTURE_SHA, one broken commit on top of $BASE"

DECISION_STATE="$TMP/decision-state"
DECISION_REPORTS="$TMP/decision-reports"
DECISION_WORKSPACES="$TMP/decision-workspaces"
mkdir -p "$DECISION_STATE/work" "$DECISION_STATE/changes" "$TMP/decision-gh-config"
printf '{"id":"%s","status":"open"}\n' "$DECISION_WORK_ID" \
  > "$DECISION_STATE/work/$DECISION_WORK_ID.json"

ACTOR_ID=$(gh api user --jq '.id') \
  || fail "could not read the credential's own user id from GET /user; the document below has to nominate somebody, and inventing an id would nominate nobody"
ACTOR_LOGIN=$(gh api user --jq '.login')
note "nominated approver: user id $ACTOR_ID ($ACTOR_LOGIN), read from GET /user"

cat > "$TMP/decision.toml" <<TOML
[project]
name = "$PROJECT"

[stub]
root = "$DECISION_STATE"

[report]
dir = "$DECISION_REPORTS"

[github]
repo = "$REPO"
base = "$BASE"
token = { env = "FIDDLE_GITHUB_TOKEN" }
config_dir = "$TMP/decision-gh-config"
timeout = "300s"

[github.decision]
authorized = [$ACTOR_ID]

[agent]
model = "$DECISION_MODEL"
base_url = "$DECISION_BASE_URL"
api_key = { env = "$DECISION_CREDENTIAL" }
max_turns = 16
max_tokens = 4096
deadline = "5m"
tool_timeout = "4m"

[workspace]
root = "$DECISION_WORKSPACES"
fixture = "$DECISION_FIXTURE"
check = { program = "bash", args = ["-c", "grep -q 'len - 1' src/lib.rs || { echo 'src/lib.rs: last_index must return len - 1, not len'; exit 1; }"] }
command_timeout = "4m"
TOML

propose_once() {
  local label="$1" code=0
  "$FIDDLE_BIN" run "$DECISION_REF" \
    --capability propose_change --json \
    --config "$TMP/decision.toml" \
    > "$TMP/$label.json" 2> "$TMP/$label.err" || code=$?

  summarize_run "$label" "$code"
  return "$code"
}

no_local_record() {
  local where="$1" markers
  rm -rf "$DECISION_REPORTS" "$DECISION_WORKSPACES"
  [ ! -e "$DECISION_REPORTS" ] || fail "$where: no local record of an earlier attempt may survive"
  [ ! -e "$DECISION_WORKSPACES" ] || fail "$where: no worktree of an earlier attempt may survive"
  [ -d "$DECISION_STATE/changes" ] \
    || fail "$where: $DECISION_STATE/changes is not a directory, so the count below would be a check that could not run"
  markers=$(find "$DECISION_STATE/changes" -type f | lines)
  [ "$markers" = 0 ] \
    || fail "$where: $markers change-set marker(s) under $DECISION_STATE/changes; a run that suspended must not have recorded the work as accounted for"
  note "$where: no bundle, no journal, no worktree, and 0 change-set markers in $DECISION_STATE/changes"
}

decide_until() {
  local step="$1" want="$2" otherwise="$3" attempt=0 code
  while :; do
    attempt=$((attempt + 1))
    ATTEMPTS=$((ATTEMPTS + 1))
    no_local_record "$step-$attempt"
    code=0
    LAST_RUN="$step-$attempt"
    propose_once "$step-$attempt" || code=$?
    if [ "$code" = "$want" ]; then
      note "$step: exit $want after $attempt fresh process(es)"
      return 0
    fi
    [ "$code" = "$RETRYABLE" ] || fail "$step-$attempt: fiddle exited $code, and this step is waiting for $want. $otherwise
$(cat "$TMP/$step-$attempt.err")"
    [ "$attempt" -lt "$MAX_ATTEMPTS" ] \
      || fail "$step: $MAX_ATTEMPTS fresh processes all ended retryable; the world never settled"
    note "$step-$attempt: retryable, and that is what exit $RETRYABLE means — running again in ${BACKOFF}s"
    sleep "$BACKOFF"
  done
}


decide_until propose 10 "Exit 20 here is most often CheckFailed: the attempt did \
not earn the check, which is a finding about $DECISION_MODEL and not about GitHub. \
Nothing is published on that path, so there is no forge residue to look for."

open_now=$(our_open_prs)
decision_candidates=$(grep -v "^$pr\$" <<<"$open_now" || true)
note "open ${REF_NAMESPACE} pull requests now: [$(paste -sd, - <<<"$open_now")] \
($(lines <<<"$open_now") of them), of which the publish phase's is #$pr"
[ "$(lines <<<"$decision_candidates")" = 1 ] \
  || fail "exactly one open ${REF_NAMESPACE} pull request other than the publish phase's #$pr expected, got [$(paste -sd, - <<<"$decision_candidates")]"
PRN=$(tr -d ' ' <<<"$decision_candidates")

[ "$(receipt ensure_pull_request external_ref)" = "$PRN" ] \
  || fail "the proposal's pull request receipt names #$(receipt ensure_pull_request external_ref), the remote holds #$PRN"

DECISION_BRANCH=$(gh api "repos/$REPO/pulls/$PRN" --jq '.head.ref')
DECISION_HEAD=$(gh api "repos/$REPO/pulls/$PRN" --jq '.head.sha')
draft_before=$(gh api "repos/$REPO/pulls/$PRN" --jq '.draft')
note "pull request #$PRN from $DECISION_BRANCH at $DECISION_HEAD, draft=$draft_before"
[ "$draft_before" = true ] \
  || fail "#$PRN must have been opened as a draft, because the transition out of one is the gated act; draft=$draft_before"
[ "$DECISION_BRANCH" != "$branch" ] \
  || fail "the decision phase must have published its own branch, not the publish phase's $branch"

conversation=$(gh api "repos/$REPO/issues/$PRN/comments?per_page=100") \
  || fail "the conversation of #$PRN cannot be read; a count taken from a failed read would be indistinguishable from an empty conversation"
comments_total=$(jq 'length' <<<"$conversation")
asked=$(jq --arg marker "$DECISION_MARKER" '[.[] | select(.body | contains($marker))] | length' <<<"$conversation")
note "conversation of #$PRN: $asked of $comments_total comment(s) carry $DECISION_MARKER"
[ "$asked" = 1 ] \
  || fail "exactly one request comment expected on #$PRN, found $asked of $comments_total"

REQUEST_COMMENT=$(jq -r --arg marker "$DECISION_MARKER" \
  'first(.[] | select(.body | contains($marker))) | .id' <<<"$conversation")
REQUEST_AUTHOR=$(jq -r --arg marker "$DECISION_MARKER" \
  'first(.[] | select(.body | contains($marker))) | .user.id' <<<"$conversation")
note "request comment id $REQUEST_COMMENT, written by user id $REQUEST_AUTHOR"

[ "$REQUEST_COMMENT" != "$REQUEST_AUTHOR" ] \
  || fail "the request comment's id and its author's user id are both $REQUEST_COMMENT, which is the field confusion this selection exists to avoid"
[ "$REQUEST_AUTHOR" = "$ACTOR_ID" ] \
  || fail "fiddle posted the question as user $REQUEST_AUTHOR, but this lane's credential is user $ACTOR_ID"

[ "$(receipt publish_decision_request external_ref)" = "$REQUEST_COMMENT" ] \
  || fail "the request receipt names comment $(receipt publish_decision_request external_ref), the remote holds $REQUEST_COMMENT"


REPLY_COMMENT=$(gh api "repos/$REPO/issues/$PRN/comments" -f body="$DECISION_REPLY" --jq '.id') \
  || fail "POST repos/$REPO/issues/$PRN/comments was refused. If this is a 403, the conversation-comment grant proven during Task 1 has been narrowed, rotated or re-scoped since — that is a regression to investigate and the exact response above is the finding. Do not widen the credential to make this pass"
reply_author=$(gh api "repos/$REPO/issues/comments/$REPLY_COMMENT" --jq '.user.id')
note "reply comment $REPLY_COMMENT by user $reply_author: \"$DECISION_REPLY\""
[ "$reply_author" = "$ACTOR_ID" ] \
  || fail "the reply must come from the user this document nominated ($ACTOR_ID), and it came from $reply_author"
[ "$REPLY_COMMENT" -gt "$REQUEST_COMMENT" ] \
  || fail "the reply's id ($REPLY_COMMENT) must be above the question's ($REQUEST_COMMENT), because a candidate reply is chosen by id and not by position"


note "the continuation below is invoked BY THIS SCRIPT, not by a comment event \
waking a runner: no issue_comment trigger is installed, and §5.3 records that \
wiring as blocked. This phase is not evidence for it."

decide_until continue 0 "Exit 10 here means the continuation suspended again \
rather than acting: either the interpretation did not read \"$DECISION_REPLY\" as \
an approval — which is what a deliberately-broken FIDDLE_LIVE_DECISION_REPLY \
produces, and is otherwise a finding about $DECISION_MODEL — or nobody the \
document nominated had answered. #$PRN is still a draft either way, and the \
residue counts below are asserted on this exit path."

draft_after=$(gh api "repos/$REPO/pulls/$PRN" --jq '.draft')
state_after=$(gh api "repos/$REPO/pulls/$PRN" --jq '.state')
note "#$PRN read back from the remote: draft $draft_before -> $draft_after, state=$state_after"
if [ "$draft_after" != false ]; then
  fail "#$PRN is still a draft after a continuation that exited 0, which means the run reported completing something the forge does not show. The run's diagnostic:
$(cat "$TMP/$LAST_RUN.err")"
fi
[ "$state_after" = open ] \
  || fail "#$PRN must still be open after being marked ready, and it is $state_after"
[ "$(receipt ensure_pull_request_ready external_ref)" = "$PRN" ] \
  || fail "the ready receipt names #$(receipt ensure_pull_request_ready external_ref), the remote holds #$PRN"

[ "$(gh api "repos/$REPO/pulls/$PRN" --jq '.head.sha')" = "$DECISION_HEAD" ] \
  || fail "#$PRN's head moved from $DECISION_HEAD under the approval"

final_open=$(our_open_prs)
final_ours=$(our_branches)
note "after the decision phase: open ${REF_NAMESPACE} pull requests \
[$(paste -sd, - <<<"$final_open")] ($(lines <<<"$final_open")), ${REF_NAMESPACE} branches \
[$(paste -sd, - <<<"$final_ours")] ($(lines <<<"$final_ours"))"
[ "$(lines <<<"$final_open")" = 2 ] \
  || fail "two open ${REF_NAMESPACE} pull requests expected — the publish phase's #$pr and this phase's #$PRN — got [$(paste -sd, - <<<"$final_open")]"
[ "$(lines <<<"$final_ours")" = 2 ] \
  || fail "two ${REF_NAMESPACE} branches expected — $branch and $DECISION_BRANCH — got [$(paste -sd, - <<<"$final_ours")]"
grep -q "^$PRN\$" <<<"$final_open" \
  || fail "#$PRN must still be the open pull request this phase opened, and the namespace holds [$(paste -sd, - <<<"$final_open")]"

final_conversation=$(gh api "repos/$REPO/issues/$PRN/comments?per_page=100") \
  || fail "the conversation of #$PRN cannot be read; a count taken from a failed read would be indistinguishable from an empty conversation"
final_total=$(jq 'length' <<<"$final_conversation")
final_marked=$(jq --arg marker "$DECISION_MARKER" '[.[] | select(.body | contains($marker))] | length' <<<"$final_conversation")
note "conversation of #$PRN at the end: $final_marked of $final_total comment(s) carry the request marker"
[ "$final_total" = 2 ] \
  || fail "#$PRN's conversation must hold exactly the question and the answer, and it holds $final_total comment(s)"
[ "$final_marked" = 1 ] \
  || fail "exactly one of #$PRN's $final_total comments must carry the request marker, and $final_marked do"

note "OK: draft -> question -> answer -> ready, one of each, every assertion read \
back from the remote"
note "PASS (publish, republish, decision)"
