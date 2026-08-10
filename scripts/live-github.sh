#!/usr/bin/env bash
# live-github.sh — the opt-in live half of M2's proof.
#
# `crates/fiddle-acceptance/tests/exactly_once.rs` proves exactly-once **offline**
# and **gates**. This repeats the same walk against real GitHub and **never
# gates**, so its job is not to re-prove the property — it is to show the property
# survives contact with the real thing: a real `git push` over HTTPS, GitHub's own
# refusal of a second pull request for the same head and base, a real
# `workflow_dispatch` that answers 204 with no run id, and the `run-name` echo
# that is the only channel the effect identity comes back through.
#
# It costs nothing and leaves nothing.
#
# # Why this is a script and not a test
#
# The deterministic gate is offline and credential-free, and stays that way: this
# lane is reached by running this file, never by `cargo test`, and nothing in
# `.github/workflows` invokes it. What it must not do is **skip**. A lane that
# quietly no-ops when its credential is absent looks exactly like a passing one,
# which is why the credential check below is `:?` and not `if`. M1's Tier 1 lane
# established that rule; this one inherits it.
#
# # Why every count is read from the remote
#
# fiddle's own report saying "one pull request" is fiddle's opinion. Every number
# asserted here is read back out of GitHub with `gh`, across separate `fiddle`
# processes, and a run's report is consulted only to cross-check the *identity* it
# derived against the name GitHub gave the object — which is the round trip, and
# the one thing the remote cannot tell us on its own.
#
# # Why the preconditions are asserted before anything is published
#
# The sharpest failure mode available to a lane like this is to do nothing at all
# and then assert "exactly main, no open pull requests" — which passes. So the
# lane asserts the remote holds **zero** of each object it is about to create
# *before* it creates them, and **one** of each afterwards. Zero-then-one cannot
# be satisfied by a walk that never happened.
#
# # Why a process may legitimately not complete, and what that costs
#
# The three-tier model's second rule is that a live lane never asserts GitHub
# cooperated. This lane found out why the hard way, twice, on its first real run:
#
#   * `GET /git/ref/heads/<branch>` immediately after the push that created it
#     answered **404** once, and `EnsureBranchPublished` correctly reported
#     `Unresolved` rather than believing the push's own answer;
#   * and — reliably, not once — `GET .../actions/workflows/<f>/runs` immediately
#     after a `workflow_dispatch` does **not yet list the run**, so
#     `EnsureCheckRequested` reports `Unresolved` too.
#
# Both are exactly the ambiguity `exactly_once.rs` arranges with a fixture that
# mutates and then dies: the write landed and its answer is not available. Here
# GitHub produces it for free. `Unresolved` reaches `RunOutcome::Retryable`, exit
# **11**, whose whole meaning is "run me again" — so this lane runs fresh
# processes until one completes, which is what a caller reading exit 11 is
# supposed to do.
#
# That is not a weakening. Each extra process is another chance to duplicate an
# object, and the assertion is unchanged: **however many fresh processes ran, the
# remote holds exactly one branch, one pull request and one requested check, and
# they are the same three objects throughout.** Exit **20** — a settled failure —
# is never retried, and a bounded number of attempts that never completes is a
# loud failure rather than a skip.
#
# # Usage
#
#   nix develop -c cargo build --release
#   set -a; . ./.env; set +a          # or export FIDDLE_GITHUB_TOKEN yourself
#   FIDDLE_BIN="$PWD/target/release/fiddle" scripts/live-github.sh
#
# See `docs/technical/effects-repository.md` for what the target repository is,
# why it is public, and the standing rules it holds.
set -euo pipefail

# ---------------------------------------------------------------------------
# What this lane is about
# ---------------------------------------------------------------------------

# Overridable so the lane can be pointed at a different disposable repository,
# never so it can be pointed at a repository somebody works in: cleanup below is
# scoped to the `fiddle/` namespace precisely because this value is a knob.
REPO="${FIDDLE_EFFECTS_REPO:-peel/fiddle-effects-acceptance}"
WORKFLOW="fiddle-check.yml"
BASE="main"

# The project name and the invocation reference are the canonical inputs the
# branch name, the pull request and the run name are all derived from. They are
# fixed, because the whole point is that a fresh process recomputes the same names
# from them with nothing local left to read.
PROJECT="icecube"
WORK_ID="fiddle-live-publish"
INVOCATION_REF="beans:$WORK_ID"

# The namespaces cleanup is allowed to touch. `fiddle/` for refs and pull request
# heads (`fiddle-runtime::github::refs::NAMESPACE`), `fiddle-` for workflow run
# names (`fiddle-runtime::github::checks::run_name`).
REF_NAMESPACE="fiddle/"
RUN_NAMESPACE="fiddle-"

# `RunOutcome::Retryable`, the code whose meaning is "run me again" — see
# `fiddle-cli::exit_code_for`.
RETRYABLE=11
# How many fresh processes one phase may spend reaching a completed run, and how
# long to wait between them. Six attempts four seconds apart is ~20s of grace for
# GitHub to list a dispatched run, which is an order of magnitude more than the
# ~1–2s observed. Exhausting it is a failure, never a skip.
MAX_ATTEMPTS=6
BACKOFF=4

fail() {
  echo "live-github: FAIL: $*" >&2
  exit 1
}

note() { echo "live-github: $*"; }

# ---------------------------------------------------------------------------
# Fail loudly, never skip
# ---------------------------------------------------------------------------

: "${FIDDLE_GITHUB_TOKEN:?live-github.sh needs FIDDLE_GITHUB_TOKEN — a fine-grained token scoped to the disposable repository alone. This lane fails rather than skips, because a silently-skipped lane is indistinguishable from a passing one.}"
: "${FIDDLE_BIN:?live-github.sh needs FIDDLE_BIN — the path to the compiled fiddle, as in FIDDLE_BIN=\"\$PWD/target/release/fiddle\"}"

[ -x "$FIDDLE_BIN" ] || fail "FIDDLE_BIN is not an executable file: $FIDDLE_BIN"
command -v gh >/dev/null 2>&1 || fail "gh must be on PATH"
command -v jq >/dev/null 2>&1 || fail "jq must be on PATH"
command -v git >/dev/null 2>&1 || fail "git must be on PATH"

# ---------------------------------------------------------------------------
# The disposable project
# ---------------------------------------------------------------------------

TMP=$(mktemp -d "${TMPDIR:-/tmp}/fiddle-live-XXXXXX")

# The harness's own `gh` authenticates through the environment, never `argv`, and
# is pointed at an empty configuration directory so the credential it uses is
# provably the one this lane was handed rather than whatever is in the operator's
# keychain — a probe in this milestone read a real `gho_` token out of the
# operator's keyring into a transcript, and this is the line that prevents it.
# `GITHUB_TOKEN` is removed so an ambient token cannot answer instead.
export GH_TOKEN="$FIDDLE_GITHUB_TOKEN"
export GH_CONFIG_DIR="$TMP/gh-harness"
export GH_PROMPT_DISABLED=1
export NO_COLOR=1
unset GITHUB_TOKEN || true
mkdir -p "$GH_CONFIG_DIR"

# ---------------------------------------------------------------------------
# What the remote holds — read out of GitHub, never out of a report
# ---------------------------------------------------------------------------

all_branches() { gh api "repos/$REPO/branches?per_page=100" --jq '.[].name'; }

our_branches() { all_branches | { grep "^$REF_NAMESPACE" || true; }; }

# Open pull requests whose head is in our namespace, as bare numbers.
our_open_prs() {
  gh pr list --repo "$REPO" --state open --limit 100 \
    --json number,headRefName \
    --jq ".[] | select(.headRefName | startswith(\"$REF_NAMESPACE\")) | .number"
}

# Workflow runs of the dispatch target whose `run-name` is one of ours, as
# `<id> <name>` pairs. `.name` is guarded against null because a run predating the
# `run-name:` key would have none, and `startswith` on null is an error rather
# than a miss.
our_runs() {
  gh api "repos/$REPO/actions/workflows/$WORKFLOW/runs?per_page=100" \
    --jq ".workflow_runs[] | select(.name != null and (.name | startswith(\"$RUN_NAMESPACE\"))) | \"\(.id) \(.name)\""
}

# How many non-empty lines arrived. `grep -c` exits 1 on no match, which `set -e`
# would take for a failure, so the count is printed and the status discarded.
lines() { grep -c . || true; }

# ---------------------------------------------------------------------------
# Cleanup, on every exit path, scoped to what this lane made
# ---------------------------------------------------------------------------
#
# Scoped deliberately. A blanket "delete every branch that is not main" would
# destroy a colleague's branch the first time this repository stopped being
# disposable, and `fiddle/` is exactly the namespace the deterministic branch name
# uses — so the scope is not a convention, it is the same string the product
# derives its names under.
#
# Workflow runs are deleted too. Deleting a branch does not delete the runs
# dispatched against it, so a lane that cleaned up only refs would accumulate
# residue invisibly and its own `exactly one run` assertion would start failing on
# the next invocation.
#
# A **closed** pull request is not residue this lane can remove: GitHub has no
# API that deletes one, so the record and the number it consumed are permanent.
# That is a property of the forge rather than of this lane, and it is a large part
# of why the target repository is disposable — see
# docs/technical/effects-repository.md.
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

  # A run in flight cannot be deleted, so it is cancelled first and the delete is
  # retried against a bounded wait. The bound is reported rather than swallowed:
  # a run that survives is residue, and the assertion below is what says so.
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

  # The residue assertion. It runs on every exit path, so a failing run is held to
  # the same standard as a passing one.
  #
  # Read *before* the scratch directory goes, because `GH_CONFIG_DIR` lives inside
  # it: a residue read made against a `gh` whose configuration directory had just
  # been deleted would be answering under conditions this lane never arranged.
  local left_branches left_prs left_runs every
  left_branches=$(our_branches | lines)
  left_prs=$(our_open_prs | lines)
  left_runs=$(our_runs | lines)
  every=$(all_branches | paste -sd, -)

  rm -rf "$TMP"

  echo "live-github: residue after cleanup: ${REF_NAMESPACE}branches=$left_branches open-prs=$left_prs ${RUN_NAMESPACE}runs=$left_runs"
  echo "live-github: branches at the remote: $every"

  if [ "$left_branches" != 0 ] || [ "$left_prs" != 0 ] || [ "$left_runs" != 0 ]; then
    echo "live-github: FAIL: cleanup left residue behind" >&2
    exit 1
  fi
  # This repository's standing rule is that `main` is its only permanent branch —
  # see docs/technical/effects-repository.md. Asserted, not merely reported, so a
  # scoped cleanup that missed something outside its namespace is still caught.
  if [ "$every" != "$BASE" ]; then
    echo "live-github: FAIL: branches must be exactly $BASE, got $every" >&2
    exit 1
  fi
  if [ "$status" != 0 ]; then
    echo "live-github: did not pass (exit $status)" >&2
  fi
  exit "$status"
}

# Cleanup hangs off EXIT, and the two signals turn into an `exit` that reaches it.
#
# Not `trap cleanup EXIT INT TERM`, which is the obvious spelling and is wrong in
# the one direction that matters: on a signal, `$?` at the top of the handler is
# the *interrupted command's* status, which for a `sleep` that finished is **0** —
# so a lane killed halfway through would clean up and then exit 0, and a killed
# lane that looks like a passing one is the same failure as a skipped one.
interrupted() {
  echo "live-github: interrupted by SIG$1" >&2
  exit "$2"
}
trap cleanup EXIT
trap 'interrupted INT 130' INT
trap 'interrupted TERM 143' TERM

# ---------------------------------------------------------------------------
# The target repository and the identity channel it carries
# ---------------------------------------------------------------------------

note "target repository: $REPO"
gh api "repos/$REPO" --jq '"visibility=\(.visibility) default_branch=\(.default_branch)"' \
  | sed 's/^/live-github: /'

WORKFLOW_YAML=$(gh api "repos/$REPO/contents/.github/workflows/$WORKFLOW" \
  -H "Accept: application/vnd.github.raw") \
  || fail "the dispatch target workflow .github/workflows/$WORKFLOW is not installed in $REPO"

# The echo is the whole identity channel: `POST .../dispatches` answers 204 with
# no run id and the runs listing carries no `inputs`, so `run-name` is the only
# place the effect id can come back from. Checked in the file here, and *observed*
# in the runs listing further down.
grep -q 'run-name: fiddle-${{ inputs.fiddle_effect_id }}' <<<"$WORKFLOW_YAML" \
  || fail "$WORKFLOW must echo the effect id through run-name; got:
$WORKFLOW_YAML"
note "run-name echo present in $WORKFLOW"

# The repository holds no secret this credential could read: the token has no
# `Secrets` permission at all, which is stronger evidence than a zero count would
# be — a credential that cannot enumerate secrets cannot leak one either.
if gh api "repos/$REPO/actions/secrets" >/dev/null 2>&1; then
  fail "the lane's credential can enumerate $REPO's secrets; it is scoped too broadly"
fi
note "the lane's credential cannot enumerate secrets (403), so it cannot leak one"

# ---------------------------------------------------------------------------
# Preconditions: zero of each, before anything is published
# ---------------------------------------------------------------------------

before_branches=$(our_branches | lines)
before_prs=$(our_open_prs | lines)
before_runs=$(our_runs | lines)
note "before: ${REF_NAMESPACE}branches=$before_branches open-prs=$before_prs ${RUN_NAMESPACE}runs=$before_runs"
[ "$before_branches" = 0 ] || fail "the remote already holds a ${REF_NAMESPACE} branch; a zero-then-one walk cannot start here"
[ "$before_prs" = 0 ] || fail "the remote already holds an open ${REF_NAMESPACE} pull request"
[ "$before_runs" = 0 ] || fail "the remote already holds a ${RUN_NAMESPACE} workflow run"

# ---------------------------------------------------------------------------
# The project every process in this lane shares
# ---------------------------------------------------------------------------

mkdir -p "$TMP/stub-state/work" "$TMP/stub-state/changes" "$TMP/gh-config"

# The worktree whose HEAD is published is a real clone of the target repository,
# taken with no credential helper of any kind — which is both what makes the push
# a fast-forward on top of the real `main` and a demonstration of the standing
# rule that reading this repository needs no credential.
git -c credential.helper= clone --quiet "https://github.com/$REPO.git" "$TMP/work" \
  || fail "could not clone $REPO without a credential; it is meant to be public"

# The commit is deliberately *not* reproducible across invocations. A stale branch
# left by an earlier interrupted run would then be a non-fast-forward — refused
# loudly by git, which owns ancestry — rather than a ref that silently already
# matched and made this walk assert nothing.
printf 'the change fiddle publishes, made at %s\n' "$(date -u +%FT%TZ)" \
  > "$TMP/work/live-probe.txt"
git -C "$TMP/work" add -A
git -C "$TMP/work" \
  -c user.email=live-github@fiddle.invalid -c user.name="fiddle live lane" \
  commit -qm "the change fiddle publishes"
HEAD_SHA=$(git -C "$TMP/work" rev-parse HEAD)
note "worktree HEAD is $HEAD_SHA, one commit ahead of $BASE"

# One open work item, and no change-set marker: the assessment that reaches
# `publish_change`.
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

# ---------------------------------------------------------------------------
# Running fiddle, and refusing to print anything that holds the credential
# ---------------------------------------------------------------------------

# The token is matched from a file rather than passed as a pattern argument, so it
# never reaches any `argv` — /proc/<pid>/cmdline is world-readable on Linux, which
# is the same reason `git push` carries it in the environment.
printf '%s\n' "$FIDDLE_GITHUB_TOKEN" > "$TMP/needle"
chmod 600 "$TMP/needle"

PAYLOAD=""   # the JSON payload of the last completed process
ATTEMPTS=0   # every fiddle process this lane has spawned, across both phases

# Take away everything the runs so far recorded locally: the published bundles,
# the attempt journals under `<report.dir>/.attempts`, and the change-set marker.
#
# This is how "the identity is recomputed, not remembered" becomes a claim about
# the binary. With the marker left in place the next process would decline to
# execute the capability at all, and a process that does nothing proves nothing
# about read-before-write.
forget_everything_local() {
  rm -rf "$TMP/reports"
  rm -f "$TMP"/stub-state/changes/*
  [ ! -e "$TMP/reports" ] || fail "no local record of an earlier attempt may survive"
}

# One fresh process. Prints what it reported and hands back its exit code.
publish_once() {
  local label="$1" code=0
  "$FIDDLE_BIN" run "$INVOCATION_REF" \
    --capability publish_change --json \
    --config "$TMP/fiddle.toml" \
    > "$TMP/$label.json" 2> "$TMP/$label.err" || code=$?

  # Redaction by construction: nothing fiddle wrote is echoed until it has been
  # checked for the credential, and a hit is a failure rather than a redaction.
  if grep -F -q -f "$TMP/needle" "$TMP/$label.json" "$TMP/$label.err"; then
    fail "$label: the credential reached fiddle's own output; not printing it"
  fi

  jq -c '{outcome, capability: .capability_executions[0].status,
          evidence: .progress[0].evidence}' "$TMP/$label.json" \
    | sed "s/^/live-github: $label (exit $code): /" \
    || fail "$label: stdout is not the JSON payload:
$(cat "$TMP/$label.json")"
  PAYLOAD="$TMP/$label.json"
  return "$code"
}

# Fresh processes, each with nothing local left to read, until one completes.
#
# Exit 0 stops the loop. Exit 11 is `Retryable`, whose whole meaning is "run me
# again", and is the code GitHub's own dispatch-to-listing latency produces — so
# it is retried, which is what a caller reading it is told to do. Every other
# code is settled and is never retried.
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

# One field of one effect receipt, out of the last completed run's evidence.
#
# An evidence reference is `effect:<kind>:<id>:<outcome>:<external_ref>:<postcondition>`
# and the postcondition itself carries colons, so it is the tail rather than a
# field. Field 2 is the identity, 4 the external reference — the sha, the pull
# request number, the run id.
#
# This is the only thing read out of fiddle's report, and it is read in order to
# be checked *against* the remote rather than believed.
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

# ---------------------------------------------------------------------------
# Phase one: publish
# ---------------------------------------------------------------------------

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

# **The identity round trip, closed against real GitHub rather than against a
# fixture.** `POST .../dispatches` answers 204 with no run id and the runs listing
# carries no `inputs`, so the workflow's `run-name` is the only channel the effect
# id comes back through — and here the name GitHub gave the run is checked against
# the identity fiddle derived for the check effect. If this held only against the
# scripted `gh`, the live check path would not work at all.
[ "$run_name" = "${RUN_NAMESPACE}${first_check_id}" ] \
  || fail "the run GitHub lists must be titled ${RUN_NAMESPACE}${first_check_id}, got $run_name"

# The branch's name is *not* the branch effect's id: `refs::branch_name` derives
# it from `(project, invocation_ref)` with the project in the target slot, because
# a name derived from its own target would be circular — while the effect's
# identity is derived over `refs/heads/<branch>`. So the ref is checked against the
# ref the receipt claims to have observed, which is the pair that must agree.
[ "$(receipt ensure_branch_published postcondition)" = "refs/heads/$branch points at $HEAD_SHA" ] \
  || fail "the branch receipt must be about refs/heads/$branch at $HEAD_SHA, got \"$(receipt ensure_branch_published postcondition)\""

# Each external reference is the remote's own identifier for the object, so it is
# checked against what the remote answers rather than trusted.
[ "$(receipt ensure_pull_request external_ref)" = "$pr" ] \
  || fail "the pull request receipt names #$(receipt ensure_pull_request external_ref), the remote holds #$pr"
[ "$(receipt ensure_check_requested external_ref)" = "$run_id" ] \
  || fail "the check receipt names run $(receipt ensure_check_requested external_ref), the remote lists $run_id"
note "round trip: ref $branch, pull request #$pr, run $run_id titled $run_name"

# The push landed on the commit the run proposed, read back from the remote rather
# than taken from the push's own answer.
remote_sha=$(gh api "repos/$REPO/git/ref/heads/$branch" --jq '.object.sha')
[ "$remote_sha" = "$HEAD_SHA" ] \
  || fail "the remote ref must point at the published commit $HEAD_SHA, got $remote_sha"

# The pull request is the one this walk is about, not one that happened to be
# open: head, base and state are read from GitHub.
gh pr view --repo "$REPO" "$pr" --json headRefName,baseRefName,state \
  --jq '"live-github: pull request #'"$pr"': \(.state) \(.headRefName) -> \(.baseRefName)"'
pr_head=$(gh pr view --repo "$REPO" "$pr" --json headRefName --jq '.headRefName')
pr_base=$(gh pr view --repo "$REPO" "$pr" --json baseRefName --jq '.baseRefName')
[ "$pr_head" = "$branch" ] || fail "pull request #$pr is opened from $pr_head, not $branch"
[ "$pr_base" = "$BASE" ] || fail "pull request #$pr is opened against $pr_base, not $BASE"

# ---------------------------------------------------------------------------
# Phase two: the same work, by processes with nothing local left to read
# ---------------------------------------------------------------------------

note "--- republishing the same work, nothing local left to read ---"
converge republish

[ "$(identity ensure_branch_published)" = "$first_branch_id" ] \
  || fail "the retry must derive the branch identity the first process derived"
[ "$(identity ensure_pull_request)" = "$first_pr_id" ] \
  || fail "the retry must derive the pull request identity the first process derived"
[ "$(identity ensure_check_requested)" = "$first_check_id" ] \
  || fail "the retry must derive the check identity the first process derived"

# And it must have *recognised* the very objects that were already there rather
# than made new ones: same sha, same pull request number, same run id.
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

# The milestone's assertion, made against the remote across every process this
# lane ran.
[ "$(lines <<<"$final_branches")" = 1 ] || fail "exactly one ${REF_NAMESPACE} branch expected, got [$final_branches]"
[ "$(lines <<<"$final_prs")" = 1 ] || fail "exactly one open pull request expected, got [$final_prs]"
[ "$(lines <<<"$final_runs")" = 1 ] || fail "exactly one requested check expected, got [$final_runs]"

# Stronger than the counts, and the reason the counts alone are not enough: a
# close-and-reopen, or a delete-and-redispatch, would leave every count at one.
# The objects must be the *same* objects.
[ "$(tr -d ' ' <<<"$final_branches")" = "$branch" ] || fail "the branch must still be $branch, got [$final_branches]"
[ "$(tr -d ' ' <<<"$final_prs")" = "$pr" ] || fail "the pull request must still be #$pr, got [$final_prs]"
[ "$(awk '{print $1}' <<<"$final_runs")" = "$run_id" ] || fail "the workflow run must still be $run_id, got [$final_runs]"
[ "$(gh api "repos/$REPO/git/ref/heads/$branch" --jq '.object.sha')" = "$HEAD_SHA" ] \
  || fail "the branch must still point at $HEAD_SHA"

[ "$ATTEMPTS" -ge 2 ] || fail "at least two fresh processes must have run, got $ATTEMPTS"
note "$ATTEMPTS fresh fiddle processes ran over the same work; the remote holds exactly \
one branch, one pull request and one requested check, and the same three throughout"
note "PASS"
