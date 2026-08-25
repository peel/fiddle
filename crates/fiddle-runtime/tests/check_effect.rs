mod support;

use fiddle_core::{
    effect_id, EffectName, Observation, ProposedEffect, VerificationState, ENSURE_CHECK_REQUESTED,
    FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    AdapterError, EffectContext, EffectError, EffectOutcome, EffectPhase, EffectReceipt,
    EffectTrace, ExecutionStep, Executor, IntegrationOperation, ReadRetry,
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

const REPO: &str = "peel/r";

const WORKFLOW: &str = "fiddle-check.yml";

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const MOVED_ON: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

const DISPATCH: &str = "POST_repos_peel_r_actions_workflows_fiddle-check.yml_dispatches";

const PATIENT: Duration = Duration::from_secs(60);

fn git_ref() -> String {
    branch_name(PROJECT, INVOCATION_REF)
}

fn ensure() -> EnsureCheckRequested {
    EnsureCheckRequested::new(
        REPO.to_string(),
        WORKFLOW.to_string(),
        git_ref(),
        PROJECT,
        INVOCATION_REF,
    )
}

fn expected_run_name() -> String {
    run_name(&effect_id(
        PROJECT,
        INVOCATION_REF,
        ENSURE_CHECK_REQUESTED,
        &check_request_target(REPO, WORKFLOW, &git_ref()),
    ))
}

fn required(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

struct Ci {
    dir: TempDir,
    steps: Mutex<Vec<&'static str>>,
}

impl EffectTrace for Ci {
    fn step(&self, _kind: &EffectName, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl Ci {
    fn empty() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        Self {
            dir,
            steps: Mutex::new(Vec::new()),
        }
    }

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

    fn answer_checks_for_any_head(&self) {
        std::fs::write(self.dir.path().join("checks_unfiltered"), "yes").unwrap();
    }

    fn unreadable(&self, marker: &str, status: u16) {
        std::fs::write(self.dir.path().join(marker), status.to_string()).unwrap();
    }

    fn workflow_run(&self, name: &str) {
        self.append(
            "runs_seed",
            serde_json::json!({ "name": name, "status": "queued", "head_branch": git_ref() }),
        );
    }

    fn context(&self) -> EffectContext {
        EffectContext::new(
            GhCli::new(
                PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
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

    fn dispatch_body(&self, nth: usize) -> serde_json::Value {
        let request = self
            .dispatches()
            .into_iter()
            .nth(nth)
            .unwrap_or_else(|| panic!("no dispatch number {nth} was recorded"));
        serde_json::from_str(request["body"].as_str().unwrap_or_default())
            .expect("a dispatch must carry a JSON body")
    }

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

    fn landed_dispatches(&self) -> usize {
        self.landed_matching(|_| true)
    }

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

async fn request_the_check(
    ci: &Ci,
    ctx: &EffectContext,
    operation: EnsureCheckRequested,
) -> Result<EffectReceipt<WorkflowRun>, EffectError> {
    let deployment = Deployment(fiddle_core::DeploymentRule::Allow);
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectName::shipped(ENSURE_CHECK_REQUESTED),
        target: operation.target(),
        payload: operation.payload(),
    };
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        ctx,
        ci,
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
}

async fn observe(ci: &Ci, head: &str, names: &[&str]) -> Observation<VerificationState> {
    let ctx = ci.context();
    observe_checks(&ctx.gh, REPO, head, &required(names), &ctx.cancel).await
}

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

    let lookup = ci.path_of("GET", 0);
    assert!(lookup.contains("/runs?"), "{lookup}");
    assert!(lookup.contains("event=workflow_dispatch"), "{lookup}");
    assert!(
        lookup.contains("branch=fiddle%2F"),
        "the ref must be encoded, or its slash is read as structure: {lookup}"
    );
}

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
        lost.outcome(EffectPhase::Apply),
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
            error.adapter_source::<GhError>(),
            Some(GhError::Http { status: 500, .. })
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
        matches!(error.adapter_source::<GhError>(), Some(GhError::NotSent(_))),
        "expected the dispatch to be refused, got {error:?}"
    );
    assert_eq!(
        ci.dispatch_requests(),
        0,
        "and nothing reached GitHub, so there is no run to find"
    );
    assert_eq!(ci.workflow_runs(), 0);
}
