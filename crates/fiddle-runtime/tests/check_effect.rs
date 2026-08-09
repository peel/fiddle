//! Checks observed by exact head, and the one request GitHub deduplicates
//! nothing for.
//!
//! The executor's protocol is not re-argued here — `effect_protocol.rs` owns it
//! against a scripted operation that never reaches a process, and
//! `pull_request_effect.rs` owns what a 422 means. What this file asks is the
//! pair of questions specific to CI.
//!
//! **A check belongs to a commit.** The first half is therefore about the head
//! sha and about names: a green result for a head the branch has moved past is a
//! green result about something else, and an unrelated green check satisfies no
//! requirement at all. Fiddle never *authors* a check run — only GitHub Apps
//! may, and this credential is not one — so everything here observes.
//!
//! **A `workflow_dispatch` protects nothing.** This is the milestone's real
//! duplicate-prevention case. `git push` to a named ref is idempotent and GitHub
//! refuses a second pull request for the same head and base; a retried dispatch
//! simply starts a second run. The dispatch answers `204` with no body and no
//! run id, and the runs listing does not carry the inputs a dispatch was made
//! with, so the identity has to leave as an input and come back through the
//! run's *name* — and the second half of this file is about that being the only
//! thing between a lost response and two workflow runs.
//!
//! Everything runs against `tests/gh_stub/`, whose world is stateful: the runs
//! it lists are the ones that were really dispatched or really seeded, so an
//! assertion about how many exist is an assertion about the world rather than
//! about what a fixture was told to say. Offline and credential-free throughout;
//! the `git` in every context is a path that does not exist, so an operation
//! that grew a second mutation channel would fail loudly.

mod support;

use fiddle_core::{
    effect_id, EffectKind, Observation, ProposedEffect, VerificationState, FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    EffectContext, EffectError, EffectOutcome, EffectReceipt, EffectTrace, ExecutionStep, Executor,
};
use fiddle_runtime::github::{
    branch_name, check_request_target, classify, observe_checks, run_name, CheckState,
    EnsureCheckRequested, WorkflowRun,
};
use fiddle_runtime::{GhCli, GhError};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// The repository the scripted `gh` answers for.
const REPO: &str = "peel/r";

/// The workflow this run asks for, spelled as the API path spells it. The same
/// file name the live verification used.
const WORKFLOW: &str = "fiddle-check.yml";

/// The head this run's work is at, and the head everything green in this file is
/// green about unless it says otherwise.
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A head the branch has moved on to. Distinct from [`HEAD`] in every character,
/// so nothing can match by prefix.
const MOVED_ON: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// The script key every dispatch in this file is scripted under.
const DISPATCH: &str = "POST_repos_peel_r_actions_workflows_fiddle-check.yml_dispatches";

/// A generous bound for children that answer immediately. Nothing here is about
/// the deadline; `github_cli` owns the process bounds.
const PATIENT: Duration = Duration::from_secs(60);

/// The ref this run's workflow is dispatched against, recomputed the way a fresh
/// process would.
fn git_ref() -> String {
    branch_name(PROJECT, INVOCATION_REF)
}

/// The operation under test.
fn ensure() -> EnsureCheckRequested {
    EnsureCheckRequested::new(
        REPO.to_string(),
        WORKFLOW.to_string(),
        git_ref(),
        PROJECT,
        INVOCATION_REF,
    )
}

/// The run title this effect's dispatch must produce, derived the way a fresh
/// process derives it — from the canonical inputs and nothing local.
fn expected_run_name() -> String {
    run_name(&effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsureCheckRequested,
        &check_request_target(REPO, WORKFLOW, &git_ref()),
    ))
}

fn required(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

// ---------------------------------------------------------------------------
// The world one check effect runs against
// ---------------------------------------------------------------------------

/// The scripted `gh`'s scratch directory, and everything a test needs to arrange
/// a world in it or read one back out.
///
/// The counts below are deliberately separate, because a duplicate hides between
/// them: [`Ci::dispatch_requests`] is what was *asked for*, counted from the
/// requests the stub recorded; [`Ci::landed_dispatches`] is what actually changed
/// the world; and [`Ci::workflow_runs`] is the object count, which is the
/// property the milestone is stated in.
///
/// It is a sibling of `pull_request_effect.rs`'s `Forge` rather than a shared
/// fixture, because the world it arranges is a different one — check runs and
/// workflow runs, where that one holds pull requests. What the two share is the
/// transport plumbing; `support::World` is the fixture that *is* shared, and it
/// models the executor's protocol rather than a repository.
struct Ci {
    dir: TempDir,
    steps: Mutex<Vec<&'static str>>,
}

impl EffectTrace for Ci {
    fn step(&self, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl Ci {
    /// A repository with no checks and no workflow runs.
    fn empty() -> Self {
        let dir = TempDir::new().unwrap();
        // Empty, and stays empty: it is what a real `gh` would be pinned to, and
        // it is what makes the operator's keyring unreachable.
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        Self {
            dir,
            steps: Mutex::new(Vec::new()),
        }
    }

    /// How one scripted write ends. `<status> <exit> <mode>`.
    fn script(&self, key: &str, spec: &str) {
        let script = self.dir.path().join("script");
        std::fs::create_dir_all(&script).unwrap();
        std::fs::write(script.join(key), spec).unwrap();
    }

    fn append(&self, file: &str, value: serde_json::Value) {
        let path = self.dir.path().join(file);
        let mut seed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_default();
        seed.push(value);
        std::fs::write(&path, serde_json::Value::Array(seed).to_string()).unwrap();
    }

    /// Put a check run in the world, at one exact head.
    fn check(&self, name: &str, status: &str, conclusion: Option<&str>, head_sha: &str) {
        self.append(
            "checks_seed",
            serde_json::json!({
                "name": name,
                "status": status,
                "conclusion": conclusion,
                "head_sha": head_sha,
            }),
        );
    }

    /// Make the check-runs endpoint ignore the head it was asked about.
    ///
    /// A stand-in for anything between this client and GitHub that answers a
    /// commit-scoped read with something wider — a proxy, a cached page, a
    /// future parameter GitHub stops honouring. The client's own check is what
    /// has to hold then, and this is how it is asked.
    fn answer_checks_for_any_head(&self) {
        std::fs::write(self.dir.path().join("checks_unfiltered"), "yes").unwrap();
    }

    /// Make a read fail, with the status it fails with.
    fn unreadable(&self, marker: &str, status: u16) {
        std::fs::write(self.dir.path().join(marker), status.to_string()).unwrap();
    }

    /// Put a workflow run in the world before the effect runs, on this run's own
    /// ref so that it is a real hazard rather than one the query would exclude.
    fn workflow_run(&self, name: &str) {
        self.append(
            "runs_seed",
            serde_json::json!({ "name": name, "status": "queued", "head_branch": git_ref() }),
        );
    }

    /// A context whose `gh` is the scripted one and whose `git` cannot be run.
    fn context(&self) -> EffectContext {
        EffectContext::new(
            GhCli::new(
                PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
                // The scratch directory arrives in `argv` because the adapter's
                // environment has room for exactly five names.
                vec![
                    "--stub-dir".to_string(),
                    self.dir.path().display().to_string(),
                ],
                "ghp_never_reaches_a_network".to_string(),
                "FIDDLE_GITHUB_TOKEN",
                self.dir.path().join("config"),
                PATIENT,
            ),
            unreachable_git(),
            self.dir.path().to_path_buf(),
            CancellationToken::new(),
        )
    }

    /// Every request the scripted `gh` recorded, in arrival order.
    fn requests(&self) -> Vec<serde_json::Value> {
        let dir = self.dir.path().join("requests");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_else(|_| Vec::new());
        files.sort();
        files
            .iter()
            .filter_map(|file| serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok())
            .collect()
    }

    fn argv(request: &serde_json::Value) -> Vec<String> {
        request["argv"]
            .as_array()
            .map(|argv| {
                argv.iter()
                    .filter_map(|a| a.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every dispatch that was *asked for*, landed or not. The number a retry
    /// would move and a postcondition read would not.
    fn dispatches(&self) -> Vec<serde_json::Value> {
        self.requests()
            .into_iter()
            .filter(|request| {
                let argv = Self::argv(request);
                argv.iter().any(|a| a == "POST") && argv.iter().any(|a| a.ends_with("/dispatches"))
            })
            .collect()
    }

    fn dispatch_requests(&self) -> usize {
        self.dispatches().len()
    }

    /// The body of the *n*th dispatch, as it was sent.
    fn dispatch_body(&self, nth: usize) -> serde_json::Value {
        let request = self
            .dispatches()
            .into_iter()
            .nth(nth)
            .unwrap_or_else(|| panic!("no dispatch number {nth} was recorded"));
        serde_json::from_str(request["body"].as_str().unwrap_or_default())
            .expect("a dispatch must carry a JSON body")
    }

    /// The API path of the *n*th request made with `method`.
    fn path_of(&self, method: &str, nth: usize) -> String {
        self.requests()
            .iter()
            .map(Self::argv)
            .filter(|argv| {
                argv.iter()
                    .position(|a| a == "--method")
                    .and_then(|at| argv.get(at + 1))
                    .map(String::as_str)
                    == Some(method)
            })
            .nth(nth)
            .and_then(|argv| argv.iter().find(|a| a.starts_with('/')).cloned())
            .unwrap_or_else(|| panic!("no {method} request number {nth} was recorded"))
    }

    /// How many dispatches actually changed the world, read out of the log the
    /// stub writes when a mutation lands.
    fn landed_dispatches(&self) -> usize {
        self.landed_matching(|_| true)
    }

    /// How many dispatches landed and then took their answer with them.
    ///
    /// Asserted of the world the test is making its claims about, rather than
    /// demonstrated on some other directory and assumed to have happened here
    /// too. Without it, an exactly-once assertion would pass on a dispatch that
    /// simply succeeded — which is a different property, and a much easier one.
    fn dispatches_whose_answer_was_lost(&self) -> usize {
        self.landed_matching(|line| line.contains(r#""mode":"commit_then_"#))
    }

    fn landed_matching(&self, keep: impl Fn(&str) -> bool) -> usize {
        std::fs::read_to_string(self.dir.path().join("world"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains("dispatches") && keep(line))
            .count()
    }

    /// The object count: every workflow run this world holds, however it came to
    /// exist.
    fn workflow_runs(&self) -> usize {
        let seeded: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join("runs_seed")).unwrap_or_default(),
        )
        .unwrap_or_default();
        seeded.len() + self.landed_dispatches()
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }
}

/// Walk the authorization order for one check request.
async fn request_the_check(
    ci: &Ci,
    ctx: &EffectContext,
    operation: EnsureCheckRequested,
) -> Result<EffectReceipt<WorkflowRun>, EffectError> {
    let deployment = Deployment(fiddle_core::DeploymentRule::Allow);
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectKind::EnsureCheckRequested,
        target: operation.target(),
        payload: operation.payload(),
    };
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        ctx,
    )
    .observed_by(ci)
    .execute(proposed, operation)
    .await
}

/// Observe this world's checks at one head.
async fn observe(ci: &Ci, head: &str, names: &[&str]) -> Observation<VerificationState> {
    let ctx = ci.context();
    observe_checks(&ctx.gh, REPO, head, &required(names), &ctx.cancel).await
}

// ---------------------------------------------------------------------------
// A check belongs to a commit
// ---------------------------------------------------------------------------

/// A green result for a superseded head is not a green result.
///
/// Both halves are asserted, and the second is what makes the first a test. A
/// world where the check-runs endpoint answered nothing at all would also report
/// `ci` missing at the new head — for the entirely different reason that the
/// fixture holds nothing — so the same world is asked about the head the check
/// really is at, and there it is satisfied.
#[tokio::test]
async fn checks_are_observed_by_exact_head_sha() {
    let ci = Ci::empty();
    ci.check("ci", "completed", Some("success"), HEAD);

    let moved_on = observe(&ci, MOVED_ON, &["ci"]).await;
    let state = moved_on.value().expect("the read succeeded");
    assert_eq!(state.head_sha, MOVED_ON);
    assert_eq!(
        state.required_missing,
        ["ci"],
        "the branch moved to a head CI has said nothing about"
    );

    let at_the_head = observe(&ci, HEAD, &["ci"]).await;
    let state = at_the_head.value().expect("the read succeeded");
    assert!(
        state.required_missing.is_empty() && state.failed.is_empty() && state.pending.is_empty(),
        "the same world really does hold a green ci — at the head it is green about: {state:?}"
    );
}

/// What comes back is checked against the head that was asked for, rather than
/// trusted because it arrived.
///
/// Here the endpoint answers without honouring the commit it was addressed by —
/// a proxy, a cache, a parameter GitHub stopped supporting. Reporting the
/// superseded result would be reporting a verification of a tree nobody built,
/// and *dropping* it silently would be indistinguishable from the head having no
/// such check. So it fails closed, which is the same rule an unreadable CI gets.
#[tokio::test]
async fn a_result_for_another_head_is_never_settled_on() {
    let ci = Ci::empty();
    ci.check("ci", "completed", Some("success"), HEAD);
    ci.answer_checks_for_any_head();

    let observed = observe(&ci, MOVED_ON, &["ci"]).await;

    assert!(
        observed.is_unavailable(),
        "expected the answer to be refused, got {observed:?}"
    );
    assert!(observed.value().is_none());
}

/// Absent, queued, in-progress and every conclusion stay distinct: collapsing
/// any two of them is how "has not started" becomes "passed".
///
/// The table is the bean's, extended to the conclusions GitHub actually reports.
/// The pairwise assertion underneath it is the one that would survive a rewrite:
/// a `classify` that answered `Passed` for everything would satisfy each row's
/// *name* but not the requirement that no two of these states are the same.
#[test]
fn every_check_lifecycle_state_stays_distinct() {
    let table = [
        ("queued", None, CheckState::Queued),
        ("in_progress", None, CheckState::InProgress),
        ("completed", Some("success"), CheckState::Passed),
        ("completed", Some("failure"), CheckState::Failed),
        ("completed", Some("cancelled"), CheckState::Cancelled),
        ("completed", Some("timed_out"), CheckState::TimedOut),
        (
            "completed",
            Some("action_required"),
            CheckState::ActionRequired,
        ),
        ("completed", Some("neutral"), CheckState::Neutral),
        ("completed", Some("skipped"), CheckState::Skipped),
        ("completed", Some("stale"), CheckState::Stale),
    ];
    for (status, conclusion, expected) in table {
        assert_eq!(
            classify(status, conclusion),
            expected,
            "{status}/{conclusion:?}"
        );
    }

    let states: Vec<CheckState> = table.iter().map(|(_, _, state)| *state).collect();
    for (i, one) in states.iter().enumerate() {
        for other in &states[i + 1..] {
            assert_ne!(one, other, "two lifecycle states collapsed into one");
        }
    }

    // Absent is never something GitHub said. A status this client does not know
    // is its own state, and specifically not `Absent` — a check that exists and
    // reports something unreadable is not a check that is not there — and
    // specifically not `Passed`.
    for unknown in [
        classify("completed", None),
        classify("completed", Some("something_new")),
        classify("who_knows", None),
    ] {
        assert_eq!(unknown, CheckState::Unrecognized);
        assert!(!unknown.is_passed());
    }
    assert!(!states.contains(&CheckState::Absent));
    assert!(!CheckState::Absent.is_passed());
    assert!(!CheckState::Neutral.is_passed() && !CheckState::Skipped.is_passed());
}

/// Named checks, never "any green result".
///
/// The witness is the second half again: the same world, asked for the check it
/// actually holds, is satisfied — so the first half fails because `ci` is
/// required and unmatched, not because the fixture served nothing.
#[tokio::test]
async fn an_unrelated_green_check_does_not_satisfy_a_required_one() {
    let ci = Ci::empty();
    ci.check("some-other-job", "completed", Some("success"), HEAD);

    let observed = observe(&ci, HEAD, &["ci"]).await;
    let state = observed.value().expect("the read succeeded");
    assert_eq!(state.required_missing, ["ci"]);
    assert!(
        state.failed.is_empty() && state.pending.is_empty(),
        "a check nobody required is not reported at all: {state:?}"
    );

    let observed = observe(&ci, HEAD, &["some-other-job"]).await;
    let state = observed.value().expect("the read succeeded");
    assert!(
        state.required_missing.is_empty(),
        "the green check really is there, under its own name: {state:?}"
    );
}

/// The three lists are three answers, and each check lands in the one that says
/// what is true of it.
#[tokio::test]
async fn each_unsatisfied_check_is_reported_by_why_it_is_unsatisfied() {
    let ci = Ci::empty();
    ci.check("waiting", "queued", None, HEAD);
    ci.check("running", "in_progress", None, HEAD);
    ci.check("broken", "completed", Some("failure"), HEAD);
    ci.check("green", "completed", Some("success"), HEAD);
    ci.check("declined", "completed", Some("skipped"), HEAD);

    let observed = observe(
        &ci,
        HEAD,
        &[
            "waiting", "running", "broken", "green", "declined", "absent",
        ],
    )
    .await;
    let state = observed.value().expect("the read succeeded");

    assert_eq!(state.pending, ["waiting", "running"]);
    assert_eq!(
        state.failed,
        ["broken", "declined"],
        "a skipped check is not a verification of anything"
    );
    assert_eq!(state.required_missing, ["absent"]);
}

/// An unreadable CI is not a CI with nothing in it.
///
/// M0's rule — `Unavailable` is never equivalent to empty — at the check
/// boundary. An empty [`VerificationState`] reads as "nothing is missing,
/// nothing is failing", which is the report a run makes when it is verified; a
/// source that could not be read must never produce it. The witness underneath
/// is what stops this passing for the wrong reason: the same seeded world, read
/// successfully, is `Available`.
#[tokio::test]
async fn an_unreadable_ci_is_never_an_empty_verification() {
    let ci = Ci::empty();
    ci.check("ci", "completed", Some("success"), HEAD);
    ci.unreadable("checks_unreadable", 500);

    let observed = observe(&ci, HEAD, &["ci"]).await;

    assert!(
        matches!(observed, Observation::Unavailable { .. }),
        "expected Unavailable, got {observed:?}"
    );
    assert!(
        observed.value().is_none(),
        "and no value at all, empty or otherwise"
    );
    match &observed {
        Observation::Unavailable { source, reason } => {
            assert_eq!(source.0, format!("github:{REPO}/commits/{HEAD}/check-runs"));
            assert!(
                reason.contains("500"),
                "the reason names what happened: {reason}"
            );
        }
        other => panic!("expected Unavailable, got {other:?}"),
    }

    let readable = Ci::empty();
    readable.check("ci", "completed", Some("success"), HEAD);
    assert!(
        observe(&readable, HEAD, &["ci"]).await.value().is_some(),
        "the same world without the failure is readable, so the refusal above is \
         the read failing and not the world being empty"
    );
}

// ---------------------------------------------------------------------------
// The one request GitHub deduplicates nothing for
// ---------------------------------------------------------------------------

/// The dispatch carries the effect id, which is the only thing that makes the
/// run findable afterwards.
#[tokio::test]
async fn the_dispatch_input_carries_the_effect_id_and_the_ref() {
    let ci = Ci::empty();
    let ctx = ci.context();

    let _ = request_the_check(&ci, &ctx, ensure()).await;

    assert_eq!(ci.dispatch_requests(), 1);
    assert_eq!(
        ci.path_of("POST", 0),
        format!("/repos/{REPO}/actions/workflows/{WORKFLOW}/dispatches")
    );
    let body = ci.dispatch_body(0);
    assert_eq!(
        body["inputs"]["fiddle_effect_id"].as_str(),
        Some(expected_run_name().trim_start_matches("fiddle-")),
        "the id that goes out is the identity a fresh process recomputes"
    );
    assert_eq!(body["ref"].as_str(), Some(git_ref().as_str()));

    // And the read that locates it is filtered on this run's own ref, not on
    // whatever ran last.
    let lookup = ci.path_of("GET", 0);
    assert!(lookup.contains("/runs?"), "{lookup}");
    assert!(lookup.contains("event=workflow_dispatch"), "{lookup}");
    assert!(
        lookup.contains("branch=fiddle%2F"),
        "the ref must be encoded, or its slash is read as structure: {lookup}"
    );
}

/// The load-bearing case for this whole task: a dispatch whose answer was lost
/// must not become two runs.
///
/// GitHub protects nothing here. There is no 422 to interpret and no idempotent
/// ref to re-push — a second dispatch is simply a second run — so `Committed`
/// being reached by *reading* rather than by retrying is the entire mechanism.
///
/// The witness at the top is what makes this a test rather than a coincidence.
/// A dispatch that simply succeeded would also reach `Committed` with one run,
/// so both halves of the ambiguity are shown to be real in this very world: the
/// request landed, and the child died before it could say so.
#[tokio::test]
async fn a_lost_dispatch_response_does_not_start_a_second_run() {
    let witness = Ci::empty();
    witness.script(DISPATCH, "204 0 commit_then_die");
    let lost = witness
        .context()
        .gh
        .api(
            "POST",
            &format!("/repos/{REPO}/actions/workflows/{WORKFLOW}/dispatches"),
            Some(&serde_json::json!({ "ref": git_ref() })),
            &CancellationToken::new(),
        )
        .await
        .expect_err("the fixture must really lose the answer, or this proves nothing");
    assert!(
        matches!(lost, GhError::Killed(_)),
        "expected a child that died without answering, got {lost:?}"
    );
    assert_eq!(
        lost.outcome(),
        EffectOutcome::Unknown,
        "and it must classify Unknown, or the executor would never go and look"
    );
    assert_eq!(
        witness.landed_dispatches(),
        1,
        "and the dispatch must really have landed, or the answer was all that was lost"
    );

    let ci = Ci::empty();
    ci.script(DISPATCH, "204 0 commit_then_die");
    let ctx = ci.context();

    let receipt = request_the_check(&ci, &ctx, ensure())
        .await
        .expect("the answer was lost, not the request");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.value.name, expected_run_name());
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("4200"),
        "the run id comes from the listing, since the dispatch never reported one"
    );
    assert_eq!(
        ci.dispatches_whose_answer_was_lost(),
        1,
        "the ambiguity is real in *this* world: the dispatch landed and the child \
         died before it could say so, which is the only route by which reaching \
         Committed means anything here"
    );
    assert_eq!(
        ci.dispatch_requests(),
        1,
        "exactly one dispatch request; an Unknown settled by retrying instead of \
         by reading would show up here as two"
    );
    assert_eq!(
        ci.workflow_runs(),
        1,
        "and exactly one workflow run — GitHub deduplicates nothing here, so this \
         count is ours to hold"
    );
    assert_eq!(
        ci.steps().iter().filter(|s| **s == "apply").count(),
        1,
        "the executor agrees it dispatched once"
    );
}

/// A run that already exists for this effect *is* the postcondition, so nothing
/// is dispatched at all — the recovery a fresh process performs when the
/// previous one died between the dispatch and its receipt.
///
/// Ours is seeded between two of somebody else's, deliberately. An
/// implementation that took the first listed run would settle on the one before
/// it and an implementation that took the most recent would settle on the one
/// after, so neither can pass by reaching for position instead of for the name.
#[tokio::test]
async fn an_existing_run_for_this_effect_id_is_the_postcondition() {
    let ci = Ci::empty();
    ci.workflow_run("fiddle-0000000000000000");
    ci.workflow_run(&expected_run_name());
    ci.workflow_run("fiddle-ffffffffffffffff");
    let ctx = ci.context();

    let receipt = request_the_check(&ci, &ctx, ensure())
        .await
        .expect("the run is already there");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.value.name, expected_run_name());
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("4201"),
        "the run settled on is ours — neither the first listed nor the last"
    );
    assert_eq!(
        ci.dispatch_requests(),
        0,
        "an effect the world already satisfies is never dispatched again"
    );
    assert_eq!(ci.workflow_runs(), 3);
    assert_eq!(
        ci.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition"
        ],
        "the walk stops at the inspection; nothing is even put to policy"
    );
}

/// Somebody else's run is not this effect's, however recent.
///
/// This is the other direction of the same rule, and the one that keeps the test
/// above from being satisfied by "there is a run on this branch". A dispatch for
/// a different effect is on the same ref, from the same event, and is the only
/// run in the listing — a locator built on recency would report the check as
/// already requested and this run's workflow would never start.
#[tokio::test]
async fn a_run_for_another_effect_is_not_this_ones() {
    let ci = Ci::empty();
    ci.workflow_run("fiddle-0123456789abcdef");
    let ctx = ci.context();

    let receipt = request_the_check(&ci, &ctx, ensure())
        .await
        .expect("nothing matched, so the workflow is dispatched");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.value.name,
        expected_run_name(),
        "the effect settled on the run it started, not on the one already there"
    );
    assert_eq!(receipt.external_ref.as_deref(), Some("4201"));
    assert_eq!(ci.dispatch_requests(), 1);
    assert_eq!(ci.workflow_runs(), 2);
}

/// The dispatch response is never the source of anything.
///
/// The real endpoint answers `204` with no body and no run id — there is nothing
/// in it to believe. Here it answers with an id anyway, which is the shape of a
/// client that read one: `999999` is not the id of anything in the world the
/// listing describes, so a receipt carrying it would be a receipt for a response
/// rather than for an observation.
#[tokio::test]
async fn the_dispatch_response_is_never_the_source_of_the_run_id() {
    let ci = Ci::empty();
    ci.script(DISPATCH, "200 0 answers_a_run_id");
    let ctx = ci.context();

    let receipt = request_the_check(&ci, &ctx, ensure())
        .await
        .expect("the dispatch landed");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("4200"),
        "the reference is read back out of the world, never taken from the response"
    );
    assert_ne!(receipt.external_ref.as_deref(), Some("999999"));
    assert_eq!(ci.workflow_runs(), 1);
}

/// Two runs for one effect is a state to report, not a set to pick from.
///
/// Nothing in GitHub prevents it — that is the premise of this whole task — so
/// arriving here means a dispatch really was sent twice, by an older build or by
/// a person, and choosing one of them silently would hide exactly the failure
/// this milestone exists to surface.
#[tokio::test]
async fn two_runs_for_one_effect_are_reported_rather_than_chosen_between() {
    let ci = Ci::empty();
    ci.workflow_run(&expected_run_name());
    ci.workflow_run(&expected_run_name());
    let ctx = ci.context();

    let error = request_the_check(&ci, &ctx, ensure())
        .await
        .expect_err("two matching runs is not a postcondition");

    assert!(
        matches!(error, EffectError::DuplicateState { count: 2, .. }),
        "expected DuplicateState with the count, got {error:?}"
    );
    assert_eq!(ci.dispatch_requests(), 0);
}

/// An unreadable runs listing is not a listing with no runs in it.
///
/// The same rule as `pulls`, arriving at the same treatment for the same reason:
/// this endpoint says absence with `200` and an empty array, so an error is the
/// source being unreadable — and reading an outage as "no run yet" is precisely
/// how the second dispatch gets sent.
#[tokio::test]
async fn an_unreadable_runs_listing_is_never_an_absent_run() {
    let ci = Ci::empty();
    ci.unreadable("runs_unreadable", 500);
    let ctx = ci.context();

    let error = request_the_check(&ci, &ctx, ensure())
        .await
        .expect_err("a listing that could not be read settles nothing");

    assert!(
        matches!(
            error,
            EffectError::Adapter {
                source: GhError::Http { status: 500, .. },
                ..
            }
        ),
        "expected the read to be reported, got {error:?}"
    );
    assert_eq!(
        ci.dispatch_requests(),
        0,
        "and an unreadable answer is never dispatched over"
    );
    assert_eq!(ci.workflow_runs(), 0);
}

/// Nothing is dispatched under an identity the lookup would not find.
///
/// The operation derives the run's name at construction, because the lookup
/// happens before the executor mints the envelope; the executor derives the
/// envelope's identity from the run it is bound to. They agree only if both were
/// given the same project and invocation ref. If they ever disagreed the run
/// would be *named* by one and *looked up* by the other, so every attempt would
/// find nothing and dispatch again — an unbounded supply of workflow runs, which
/// is the worst version of the failure this task exists to prevent. It is
/// refused before the request rather than after it.
#[tokio::test]
async fn a_dispatch_whose_identity_would_not_round_trip_is_refused() {
    let ci = Ci::empty();
    let ctx = ci.context();
    let mismatched = EnsureCheckRequested::new(
        REPO.to_string(),
        WORKFLOW.to_string(),
        git_ref(),
        "another/project",
        INVOCATION_REF,
    );
    assert_ne!(
        mismatched.run_name(),
        expected_run_name(),
        "the premise: these two identities really do differ"
    );

    let error = request_the_check(&ci, &ctx, mismatched)
        .await
        .expect_err("a run nobody could find again is not dispatched");

    assert!(
        matches!(
            error,
            EffectError::Adapter {
                source: GhError::Malformed(_),
                ..
            }
        ),
        "expected the dispatch to be refused, got {error:?}"
    );
    assert_eq!(
        ci.dispatch_requests(),
        0,
        "and nothing reached GitHub, so there is no run to find"
    );
    assert_eq!(ci.workflow_runs(), 0);
}
