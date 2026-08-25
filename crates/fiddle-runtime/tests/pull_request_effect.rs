mod support;

use fiddle_core::{
    effect_id, payload_hash, EffectName, ProposedEffect, ENSURE_PULL_REQUEST, FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    AdapterError, EffectContext, EffectError, EffectOutcome, EffectPhase, EffectReceipt,
    EffectTrace, ExecutionStep, Executor, IntegrationOperation, ReadRetry,
};
use fiddle_runtime::github::{branch_name, EnsurePullRequest, PullRequest};
use fiddle_runtime::{GhCli, GhError};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const REPO: &str = "peel/r";

const OWNER: &str = "peel";

const BASE: &str = "main";

const TITLE: &str = "fiddle: repair the fixture";

const CREATE: &str = "POST_repos_peel_r_pulls";

const PATIENT: Duration = Duration::from_secs(60);

fn head() -> String {
    branch_name(PROJECT, INVOCATION_REF)
}

fn head_label() -> String {
    format!("{OWNER}:{}", head())
}

struct Forge {
    dir: TempDir,
    steps: Mutex<Vec<&'static str>>,
}

impl EffectTrace for Forge {
    fn step(&self, _kind: &EffectName, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl Forge {
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

    fn open_pull_request(&self, head: &str, base: &str, title: &str) {
        let path = self.dir.path().join("pulls_seed");
        let mut seed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_default();
        seed.push(serde_json::json!({ "head": head, "base": base, "title": title }));
        std::fs::write(&path, serde_json::Value::Array(seed).to_string()).unwrap();
    }

    fn answer_without_filtering(&self) {
        std::fs::write(self.dir.path().join("pulls_unfiltered"), "yes").unwrap();
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

    fn recorded(&self) -> Vec<serde_json::Value> {
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

    fn requests(&self) -> Vec<Vec<String>> {
        self.recorded()
            .iter()
            .filter_map(|recorded| {
                Some(
                    recorded["argv"]
                        .as_array()?
                        .iter()
                        .filter_map(|a| a.as_str().map(str::to_string))
                        .collect(),
                )
            })
            .collect()
    }

    fn nth_request(&self, method: &str, nth: usize) -> serde_json::Value {
        self.recorded()
            .into_iter()
            .filter(|recorded| {
                let argv: Vec<&str> = recorded["argv"]
                    .as_array()
                    .map(|a| a.iter().filter_map(serde_json::Value::as_str).collect())
                    .unwrap_or_default();
                argv.iter()
                    .position(|a| *a == "--method")
                    .and_then(|at| argv.get(at + 1))
                    .copied()
                    == Some(method)
            })
            .nth(nth)
            .unwrap_or_else(|| panic!("no {method} request number {nth} was recorded"))
    }

    fn path_of(&self, method: &str, nth: usize) -> String {
        self.nth_request(method, nth)["argv"]
            .as_array()
            .and_then(|argv| {
                argv.iter()
                    .filter_map(serde_json::Value::as_str)
                    .find(|a| a.starts_with('/'))
                    .map(str::to_string)
            })
            .unwrap_or_else(|| panic!("the {method} request number {nth} named no path"))
    }

    fn body_of(&self, method: &str, nth: usize) -> serde_json::Value {
        let recorded = self.nth_request(method, nth);
        serde_json::from_str(recorded["body"].as_str().unwrap_or_default())
            .unwrap_or(serde_json::Value::Null)
    }

    fn creation_requests(&self) -> usize {
        self.requests()
            .iter()
            .filter(|argv| {
                argv.iter().any(|a| a == "POST") && argv.iter().any(|a| a.starts_with("/repos"))
            })
            .count()
    }

    fn landed_creations(&self) -> usize {
        self.creations_matching(|_| true)
    }

    fn creations_whose_answer_was_lost(&self) -> usize {
        self.creations_matching(|line| line.contains(r#""mode":"commit_then_"#))
    }

    fn creations_matching(&self, keep: impl Fn(&str) -> bool) -> usize {
        std::fs::read_to_string(self.dir.path().join("world"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains("POST_repos_peel_r_pulls") && keep(line))
            .count()
    }

    fn open_pull_requests(&self) -> usize {
        let seeded: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join("pulls_seed")).unwrap_or_default(),
        )
        .unwrap_or_default();
        seeded.len() + self.landed_creations()
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }
}

fn ensure_drafting_titled(title: &str, draft: bool) -> EnsurePullRequest {
    EnsurePullRequest::new(
        REPO.to_string(),
        OWNER.to_string(),
        head(),
        BASE.to_string(),
        title.to_string(),
        "opened by fiddle".to_string(),
        draft,
    )
}

fn ensure_titled(title: &str) -> EnsurePullRequest {
    ensure_drafting_titled(title, false)
}

fn ensure() -> EnsurePullRequest {
    ensure_titled(TITLE)
}

fn ensure_drafting() -> EnsurePullRequest {
    ensure_drafting_titled(TITLE, true)
}

async fn open_the_pull_request(
    forge: &Forge,
    ctx: &EffectContext,
    operation: EnsurePullRequest,
) -> Result<EffectReceipt<PullRequest>, EffectError> {
    let deployment = Deployment(fiddle_core::DeploymentRule::Allow);
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectName::shipped(ENSURE_PULL_REQUEST),
        target: operation.target(),
        payload: operation.payload(),
    };
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        ctx,
        forge,
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
}

#[test]
fn a_title_moves_the_payload_hash_and_never_the_identity() {
    let ours = ensure_titled("fiddle: repair the fixture");
    let reworded = ensure_titled("chore(deps): repair the fixture");

    assert_eq!(
        ours.target(),
        reworded.target(),
        "a reworded title is the same pull request"
    );
    assert_eq!(
        effect_id(PROJECT, INVOCATION_REF, ENSURE_PULL_REQUEST, &ours.target()),
        effect_id(
            PROJECT,
            INVOCATION_REF,
            ENSURE_PULL_REQUEST,
            &reworded.target()
        ),
        "so a fresh process recomputes the same identity for it"
    );
    assert!(
        !ours.target().contains("repair the fixture"),
        "and no part of the title reaches the identity: {}",
        ours.target()
    );

    assert_ne!(
        payload_hash(&ours.payload()),
        payload_hash(&reworded.payload()),
        "but the change is still detectable, which is what the payload hash is for"
    );
    assert_eq!(
        payload_hash(&ours.payload()),
        payload_hash(&ensure_titled("fiddle: repair the fixture").payload()),
        "and the payload is canonical: the same request hashes the same"
    );
}

#[tokio::test]
async fn a_pull_request_is_located_by_head_and_base_not_by_title() {
    let forge = Forge::empty();
    forge.open_pull_request(&head_label(), BASE, "something else entirely");
    let ctx = forge.context();

    let receipt = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect("the pull request is already open");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.external_ref.as_deref(), Some("7"));
    assert_eq!(
        receipt.value.title, "something else entirely",
        "the pull request that was found is the one the world already held"
    );
    assert_eq!(
        forge.creation_requests(),
        0,
        "a differing title is not a reason to create a second"
    );
    assert_eq!(forge.open_pull_requests(), 1);
    assert_eq!(
        forge.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition"
        ],
        "the walk stops at the inspection; nothing is even put to policy"
    );
}

#[tokio::test]
async fn the_lookup_qualifies_the_head_with_its_owner() {
    let forge = Forge::empty();
    let ctx = forge.context();

    let _ = open_the_pull_request(&forge, &ctx, ensure()).await;

    let lookup = forge.path_of("GET", 0);
    assert!(
        lookup.contains(&format!("head={OWNER}%3Afiddle%2F")),
        "the head must be owner-qualified: {lookup}"
    );
    assert!(lookup.contains(&format!("base={BASE}")), "{lookup}");
    assert!(lookup.contains("state=open"), "{lookup}");
    assert!(
        !lookup.contains("title") && !lookup.contains("repair"),
        "no part of the payload belongs in the lookup: {lookup}"
    );
}

#[tokio::test]
async fn a_head_under_another_owner_is_not_this_pull_request() {
    let forge = Forge::empty();
    forge.open_pull_request(&format!("someone-else:{}", head()), BASE, "theirs");
    let ctx = forge.context();

    let receipt = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect("nothing matched, so the pull request is opened");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("8"),
        "the effect settled on the one it opened, not on the other owner's"
    );
    assert_eq!(receipt.value.head, head_label());
    assert_eq!(receipt.value.title, TITLE);
    assert_eq!(forge.creation_requests(), 1);
}

#[tokio::test]
async fn a_pull_request_that_is_not_the_one_asked_for_is_never_settled_on() {
    let forge = Forge::empty();
    forge.open_pull_request(&format!("someone-else:{}", head()), BASE, "theirs");
    forge.answer_without_filtering();
    let ctx = forge.context();

    let error = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect_err("an object that was not asked for is not an answer");

    assert!(
        matches!(
            error,
            EffectError::Adapter {
                source: GhError::Malformed(_),
                ..
            }
        ),
        "expected the read to be refused, got {error:?}"
    );
    assert_eq!(
        forge.creation_requests(),
        0,
        "and an unreadable answer is never written over"
    );
}

#[tokio::test]
async fn two_open_pull_requests_are_reported_rather_than_chosen_between() {
    let forge = Forge::empty();
    forge.open_pull_request(&head_label(), BASE, "one");
    forge.open_pull_request(&head_label(), BASE, "two");
    let ctx = forge.context();

    let error = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect_err("two matching objects is not a postcondition");

    assert!(
        matches!(error, EffectError::DuplicateState { count: 2, .. }),
        "expected DuplicateState with the count, got {error:?}"
    );
    assert_eq!(forge.creation_requests(), 0);
}

#[tokio::test]
async fn a_422_for_a_pull_request_that_already_exists_is_not_a_false_failure() {
    let witness = Forge::empty();
    witness.script(CREATE, "422 1 conflict");
    let refusal = witness
        .context()
        .gh
        .api(
            "POST",
            &format!("/repos/{REPO}/pulls"),
            Some(&serde_json::json!({ "head": head_label(), "base": BASE })),
            &CancellationToken::new(),
        )
        .await
        .expect_err("the create must really be refused, or this proves nothing");
    assert!(
        matches!(
            refusal,
            GhError::Http {
                status: 422,
                ref message,
                ..
            } if message == "scripted 422"
        ),
        "expected a 422 whose message says nothing, got {refusal:?}"
    );
    assert_eq!(
        refusal.outcome(EffectPhase::Apply),
        EffectOutcome::Unknown,
        "a 422 is never classified on its face; being Unknown is what forces the read"
    );
    assert_eq!(
        witness.landed_creations(),
        0,
        "and the refused create must really have changed nothing"
    );

    let forge = Forge::empty();
    forge.script(CREATE, "422 1 conflict");
    let ctx = forge.context();

    let receipt = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect("the duplicate was prevented, which is not a failure");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("7"),
        "and the receipt carries the existing pull request's number"
    );
    assert_eq!(
        receipt.value.title, "opened by another run",
        "the object settled on is the one that made the create a duplicate"
    );
    assert_eq!(
        forge.open_pull_requests(),
        1,
        "exactly one pull request — the property, stated as an object count"
    );
    assert_eq!(forge.creation_requests(), 1, "dispatched exactly once");
    assert_eq!(
        forge.landed_creations(),
        0,
        "and this run created nothing: the 422 was a real refusal, resolved by \
         the read rather than by the response"
    );
}

#[tokio::test]
async fn a_422_with_no_matching_pull_request_stays_a_failure() {
    let forge = Forge::empty();
    forge.script(CREATE, "422 1 normal");
    let ctx = forge.context();

    let error = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect_err("nothing was observed, so nothing is confirmed");

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "expected Unresolved rather than a receipt, got {error:?}"
    );
    assert_eq!(
        forge.open_pull_requests(),
        0,
        "no pull request was created, and none was invented by reading"
    );
    assert_eq!(
        forge.creation_requests(),
        1,
        "an unresolved outcome is never resolved by dispatching again"
    );
}

#[tokio::test]
async fn a_lost_create_response_does_not_produce_a_second_pull_request() {
    let witness = Forge::empty();
    witness.script(CREATE, "201 0 commit_then_die");
    let lost = witness
        .context()
        .gh
        .api(
            "POST",
            &format!("/repos/{REPO}/pulls"),
            Some(&serde_json::json!({ "head": head_label(), "base": BASE })),
            &CancellationToken::new(),
        )
        .await
        .expect_err("the fixture must really lose the answer, or it proves nothing");
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
        witness.landed_creations(),
        1,
        "and the write must really have landed, or the answer was all that was lost"
    );

    let forge = Forge::empty();
    forge.script(CREATE, "201 0 commit_then_die");
    let ctx = forge.context();

    let receipt = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect("the answer was lost, not the write");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("7"),
        "the number comes from the read, since the create never reported one"
    );
    assert_eq!(receipt.value.head, head_label());
    assert_eq!(receipt.value.base, BASE);
    assert_eq!(
        forge.creations_whose_answer_was_lost(),
        1,
        "the ambiguity is real in *this* world: the create landed and the child \
         died before it could say so, which is the only route by which reaching \
         Committed means anything here"
    );
    assert_eq!(
        forge.open_pull_requests(),
        1,
        "exactly one pull request, which is the property"
    );
    assert_eq!(
        forge.creation_requests(),
        1,
        "and it was dispatched exactly once; an Unknown settled by retrying \
         instead of by reading would show up here as two"
    );
    assert_eq!(
        forge.steps().iter().filter(|s| **s == "apply").count(),
        1,
        "the executor agrees it dispatched once"
    );
}

#[test]
fn omitting_draft_leaves_the_canonical_payload_byte_identical() {
    let plain = EnsurePullRequest::new(
        "acme/r".to_string(),
        "acme".to_string(),
        "fiddle/x".to_string(),
        "main".to_string(),
        "t".to_string(),
        "b".to_string(),
        false,
    );

    assert_eq!(
        plain.payload(),
        r#"{"base":"main","body":"b","head":"acme:fiddle/x","repo":"acme/r","title":"t"}"#,
        "the payload of a run that is not drafting is the one it was before the \
         field existed, so nothing recorded against it has to be re-derived"
    );
}

#[test]
fn a_draft_is_a_different_payload_and_the_same_identity() {
    let plain = ensure();
    let draft = ensure_drafting();

    assert_ne!(plain.payload(), draft.payload());
    assert!(
        draft.payload().contains(r#""draft":true"#),
        "the drafting payload says so: {}",
        draft.payload()
    );
    assert_ne!(
        payload_hash(&plain.payload()),
        payload_hash(&draft.payload()),
        "so the difference is detectable against the payload that was approved"
    );
    assert_eq!(
        plain.target(),
        draft.target(),
        "but a draft is the same pull request, not a second one"
    );
}

#[tokio::test]
async fn a_draft_pull_request_is_created_as_a_draft() {
    let forge = Forge::empty();
    let ctx = forge.context();

    let receipt = open_the_pull_request(&forge, &ctx, ensure_drafting())
        .await
        .expect("nothing was open, so the pull request is opened");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(forge.creation_requests(), 1);
    assert_eq!(
        forge.body_of("POST", 0)["draft"],
        serde_json::json!(true),
        "the create asks for a draft"
    );
}

#[tokio::test]
async fn an_open_pull_request_is_found_by_the_unchanged_lookup_when_drafting() {
    let forge = Forge::empty();
    forge.open_pull_request(&head_label(), BASE, "opened by another run");
    let ctx = forge.context();

    let receipt = open_the_pull_request(&forge, &ctx, ensure_drafting())
        .await
        .expect("the pull request is already open");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.external_ref.as_deref(), Some("7"));
    assert_eq!(
        receipt.value.title, "opened by another run",
        "the object settled on is the one the world already held"
    );
    assert_eq!(
        forge.creation_requests(),
        0,
        "drafting is not a reason to open a second pull request"
    );
    assert_eq!(forge.open_pull_requests(), 1);

    let lookup = forge.path_of("GET", 0);
    assert!(
        lookup.contains("state=open") && !lookup.contains("draft"),
        "drafting adds nothing to the query and takes nothing out of it: {lookup}"
    );
}

#[test]
fn no_branch_depends_on_githubs_error_prose() {
    let source = include_str!("../src/github/pulls.rs");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    for phrase in [
        "already exists",
        "A pull request already",
        "No commits between",
        "Validation Failed",
    ] {
        assert!(
            !code.contains(phrase),
            "classification must not depend on GitHub's wording: {phrase:?}"
        );
    }
    assert!(
        !code.contains("message"),
        "the error envelope's message is prose; the status and the postcondition \
         read are the contract"
    );
}
