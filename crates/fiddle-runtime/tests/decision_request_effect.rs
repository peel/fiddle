use fiddle_core::{
    decision_request_id, effect_id, parse_marker, payload_hash, render_marker, CapabilityId,
    DecisionBinding, DecisionRequestId, DeploymentRule, EffectId, EffectKind, EvidenceRef,
    HumanDecisionRequest, HumanDecisionRequirement, ProposedEffect, WorkRef, PUBLISH_CHANGE,
};
use fiddle_runtime::effect::{
    DeploymentPolicy, EffectContext, EffectError, EffectReceipt, EffectTrace, ExecutionStep,
    Executor, IntegrationOperation, ReadRetry,
};
use fiddle_runtime::git::GitCli;
use fiddle_runtime::human::{
    render_request, GitHubConversation, HumanInteractionPort, InteractionRef,
    PublishDecisionRequest,
};
use fiddle_runtime::GhCli;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const PATIENT: Duration = Duration::from_secs(30);

const TEST_TOKEN: &str = "ghp_decision_request_sentinel_must_not_appear";

const PROJECT: &str = "acme/widget";
const INVOCATION_REF: &str = "beans:w-1";
const REPO: &str = "acme/widget";
const PR: u64 = 7;
const HEAD: &str = "1111111111111111111111111111111111111111";
const OTHER_HEAD: &str = "2222222222222222222222222222222222222222";

fn gated_effect(head: &str) -> EffectId {
    effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsurePullRequestReady,
        &format!("{REPO}#{PR}@{head}"),
    )
}

fn binding_for(head: &str) -> DecisionBinding {
    let effect = gated_effect(head);
    DecisionBinding {
        request: decision_request_id(PROJECT, INVOCATION_REF, &effect),
        effect,
        payload: payload_hash(r#"{"pr":7,"repo":"acme/widget"}"#),
        head_sha: head.to_string(),
    }
}

fn binding() -> DecisionBinding {
    binding_for(HEAD)
}

fn other_binding() -> DecisionBinding {
    binding_for(OTHER_HEAD)
}

fn request_with(
    question: &str,
    rationale: &str,
    risks: &[&str],
    alternatives: &[&str],
    evidence: &[&str],
) -> HumanDecisionRequest {
    HumanDecisionRequest {
        invocation_ref: INVOCATION_REF.to_string(),
        work_ref: Some(WorkRef("w-1".to_string())),
        capability: PUBLISH_CHANGE,
        binding: binding(),
        question: question.to_string(),
        rationale: rationale.to_string(),
        risks: risks.iter().map(|r| r.to_string()).collect(),
        alternatives: alternatives.iter().map(|a| a.to_string()).collect(),
        evidence: evidence
            .iter()
            .map(|e| EvidenceRef(e.to_string()))
            .collect(),
    }
}

fn request() -> HumanDecisionRequest {
    request_with(
        "Mark this ready for review?",
        "The check passed at this revision.",
        &["review notifications reach the team"],
        &["leave it a draft and revisit"],
        &["check=pass"],
    )
}

fn operation() -> PublishDecisionRequest {
    PublishDecisionRequest::new(REPO.to_string(), PR, request())
}

fn comment_with_marker(id: u64, binding: &DecisionBinding) -> serde_json::Value {
    comment_with_body(id, &render_marker(binding))
}

fn comment_with_body(id: u64, body: &str) -> serde_json::Value {
    json!({
        "id": id,
        "body": body,
        "created_at": "2026-08-11T00:00:00Z",
        "updated_at": "2026-08-11T00:00:00Z",
        "author_association": "OWNER",
        "user": { "login": "peel", "id": 505_401, "type": "User" },
        "performed_via_github_app": null,
    })
}

struct Deployment(DeploymentRule);

impl DeploymentPolicy for Deployment {
    fn rule_for(&self, _kind: EffectKind) -> DeploymentRule {
        self.0
    }
}

#[derive(Default)]
struct Steps(Mutex<Vec<&'static str>>);

impl EffectTrace for Steps {
    fn step(&self, _kind: EffectKind, step: ExecutionStep) {
        self.0.lock().unwrap().push(step.as_str());
    }
}

struct World {
    dir: TempDir,
    steps: Steps,
}

impl World {
    fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
            steps: Steps::default(),
        }
    }

    fn gh(&self) -> GhCli {
        let config = self.dir.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        GhCli::new(
            PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
            vec![
                "--stub-dir".to_string(),
                self.dir.path().display().to_string(),
            ],
            TEST_TOKEN.to_string(),
            "FIDDLE_GITHUB_TOKEN",
            config,
            PATIENT,
        )
    }

    fn page(&self, collection: &str, page: u64, comments: &[serde_json::Value]) {
        let dir = self.dir.path().join(collection);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("page-{page}.json")),
            serde_json::Value::Array(comments.to_vec()).to_string(),
        )
        .unwrap();
    }

    fn on_post(&self, spec: &str) {
        let script = self.dir.path().join("script");
        std::fs::create_dir_all(&script).unwrap();
        let key = format!(
            "POST_{}",
            format!("repos/{REPO}/issues/{PR}/comments").replace('/', "_")
        );
        std::fs::write(script.join(key), spec).unwrap();
    }

    fn on_post_apply_then_die(&self) {
        self.on_post("201 0 commit_then_die");
    }

    fn posted(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.dir.path().join("world"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|w| {
                let key = w["key"].as_str().unwrap_or_default();
                key.starts_with("POST") && key.contains("issues") && key.ends_with("comments")
            })
            .collect()
    }

    fn posted_comments(&self) -> usize {
        self.posted().len()
    }

    fn posted_body(&self, n: usize) -> String {
        let posted = self.posted();
        let request: serde_json::Value =
            serde_json::from_str(posted[n]["body"].as_str().unwrap_or("{}")).unwrap();
        request["body"].as_str().unwrap_or_default().to_string()
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.0.lock().unwrap().clone()
    }

    fn ctx(&self) -> EffectContext {
        EffectContext::new(
            self.gh(),
            unreachable_git(),
            self.dir.path().to_path_buf(),
            CancellationToken::new(),
        )
    }

    async fn execute(
        &self,
        operation: PublishDecisionRequest,
    ) -> Result<EffectReceipt<InteractionRef>, EffectError> {
        let ctx = self.ctx();
        let deployment = Deployment(DeploymentRule::Allow);
        let executor = Executor::new(
            PUBLISH_CHANGE,
            PROJECT.to_string(),
            INVOCATION_REF.to_string(),
            &deployment,
            &ctx,
            &self.steps,
            ReadRetry::none(),
        );
        let proposed = ProposedEffect {
            capability: PUBLISH_CHANGE,
            kind: EffectKind::PublishDecisionRequest,
            target: operation.target(),
            payload: operation.payload(),
        };
        executor.execute(proposed, operation).await
    }
}

fn unreachable_git() -> GitCli {
    GitCli::new(
        PathBuf::from("/nonexistent/git"),
        String::new(),
        "FIDDLE_GITHUB_TOKEN",
        Duration::from_secs(1),
    )
}

#[tokio::test]
async fn a_request_already_published_is_recognised_and_not_posted_again() {
    let world = World::new();
    world.page("issue-comments", 1, &[comment_with_marker(11, &binding())]);

    let receipt = world.execute(operation()).await.unwrap();

    assert_eq!(receipt.outcome, fiddle_runtime::EffectOutcome::Committed);
    assert_eq!(world.posted_comments(), 0, "step 3 must have settled it");
    assert!(
        matches!(
            receipt.value,
            InteractionRef::GitHubPullRequestComment { comment: 11, .. }
        ),
        "got {:?}",
        receipt.value
    );
    assert!(
        !world.steps().contains(&ExecutionStep::Apply.as_str()),
        "got {:?}",
        world.steps()
    );
}

#[tokio::test]
async fn a_request_not_yet_published_is_posted_exactly_once() {
    let world = World::new();
    world.page("issue-comments", 1, &[]);

    let receipt = world.execute(operation()).await.unwrap();

    assert_eq!(world.posted_comments(), 1);
    assert_eq!(receipt.outcome, fiddle_runtime::EffectOutcome::Committed);
    assert!(
        matches!(
            receipt.value,
            InteractionRef::GitHubPullRequestComment { comment: 9000, .. }
        ),
        "got {:?}",
        receipt.value
    );
}

#[tokio::test]
async fn a_lost_answer_is_settled_by_reading_and_never_by_posting_again() {
    let world = World::new();
    world.page("issue-comments", 1, &[]);
    world.on_post_apply_then_die();

    let receipt = world.execute(operation()).await.unwrap();

    assert_eq!(world.posted_comments(), 1, "exactly one comment, not two");
    assert_eq!(receipt.outcome, fiddle_runtime::EffectOutcome::Committed);
    let steps = world.steps();
    let applied = steps
        .iter()
        .position(|s| *s == ExecutionStep::Apply.as_str())
        .expect("the mutation was dispatched");
    assert!(
        steps[applied + 1..].contains(&ExecutionStep::ObservePostcondition.as_str()),
        "the executor must have looked after the answer was lost: {steps:?}"
    );
}

#[tokio::test]
async fn a_marker_for_another_request_is_not_the_postcondition() {
    let world = World::new();
    world.page(
        "issue-comments",
        1,
        &[comment_with_marker(11, &other_binding())],
    );

    let receipt = world.execute(operation()).await.unwrap();

    assert_eq!(world.posted_comments(), 1);
    assert!(
        matches!(
            receipt.value,
            InteractionRef::GitHubPullRequestComment { comment: 9000, .. }
        ),
        "got {:?}",
        receipt.value
    );
}

#[tokio::test]
async fn two_comments_naming_one_request_are_a_duplicate_state() {
    let world = World::new();
    let b = binding();
    world.page(
        "issue-comments",
        1,
        &[comment_with_marker(11, &b), comment_with_marker(12, &b)],
    );

    let err = world.execute(operation()).await.unwrap_err();

    assert!(
        matches!(err, EffectError::DuplicateState { count: 2, .. }),
        "got {err:?}"
    );
    assert_eq!(world.posted_comments(), 0);
}

#[tokio::test]
async fn the_duplicate_count_is_how_many_there_actually_are() {
    let world = World::new();
    let b = binding();
    world.page(
        "issue-comments",
        1,
        &[
            comment_with_marker(11, &b),
            comment_with_marker(12, &b),
            comment_with_marker(13, &b),
        ],
    );

    let err = world.execute(operation()).await.unwrap_err();

    assert!(
        matches!(err, EffectError::DuplicateState { count: 3, .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn a_duplicate_is_found_across_pages() {
    let world = World::new();
    let b = binding();
    world.page("issue-comments", 1, &[comment_with_marker(11, &b)]);
    world.page("issue-comments", 2, &[comment_with_marker(12, &b)]);

    let err = world.execute(operation()).await.unwrap_err();

    assert!(
        matches!(err, EffectError::DuplicateState { count: 2, .. }),
        "got {err:?}"
    );
    assert_eq!(world.posted_comments(), 0);
}

#[test]
fn publishing_a_question_never_requires_a_question() {
    assert_eq!(
        operation().minimum(),
        HumanDecisionRequirement::Automatic,
        "a question that required a question to ask would not terminate"
    );
}

#[tokio::test]
async fn the_posted_body_carries_the_marker_and_is_the_hashed_payload() {
    let world = World::new();
    world.page("issue-comments", 1, &[]);

    let op = operation();
    let payload = op.payload();
    world.execute(op).await.unwrap();

    let posted = world.posted_body(0);
    assert_eq!(posted, payload, "the posted body is not the hashed payload");
    assert_eq!(parse_marker(&posted).unwrap(), binding());
}

#[test]
fn the_rendered_question_carries_what_a_person_needs_to_decide() {
    let body = render_request(&request_with(
        "Mark this ready for review?",
        "The check passed at this revision.",
        &["review notifications reach the team"],
        &["leave it a draft and revisit"],
        &["check=pass"],
    ));
    for expected in [
        "Mark this ready for review?",
        "The check passed at this revision.",
        "review notifications reach the team",
        "leave it a draft and revisit",
        "check=pass",
    ] {
        assert!(
            body.contains(expected),
            "missing {expected:?} from:\n{body}"
        );
    }
}

#[test]
fn the_question_reads_before_the_bookkeeping() {
    let request = request();
    let body = render_request(&request);
    let marker = render_marker(&request.binding);
    assert!(
        body.ends_with(&marker),
        "the marker must come last:\n{body}"
    );
    assert!(
        body.find(&request.question).unwrap() < body.find(&marker).unwrap(),
        "the question must come before the marker:\n{body}"
    );
}

#[test]
fn an_empty_section_is_omitted_rather_than_rendered_empty() {
    let body = render_request(&request_with("Ready?", "Because.", &[], &[], &[]));
    for absent in ["Risks", "Alternatives considered", "Evidence"] {
        assert!(
            !body.contains(absent),
            "{absent:?} has no items and must not have a heading:\n{body}"
        );
    }
    assert!(
        body.contains("Ready?") && body.contains("Because."),
        "{body}"
    );
}

#[test]
fn rewording_a_question_keeps_its_identity_and_changes_its_payload() {
    let asked = PublishDecisionRequest::new(REPO.to_string(), PR, request());
    let reworded = PublishDecisionRequest::new(
        REPO.to_string(),
        PR,
        request_with(
            "Actually, may I merge this straight to main?",
            "The check passed at this revision.",
            &["review notifications reach the team"],
            &["leave it a draft and revisit"],
            &["check=pass"],
        ),
    );
    assert_eq!(asked.target(), reworded.target());
    assert_ne!(asked.payload(), reworded.payload());
}

#[test]
fn the_question_is_a_different_effect_from_the_one_it_gates() {
    let publishing = effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::PublishDecisionRequest,
        &operation().target(),
    );
    assert_ne!(publishing, gated_effect(HEAD));
}

#[tokio::test]
async fn an_unreadable_conversation_posts_nothing() {
    let world = World::new();
    std::fs::write(world.dir.path().join("issue-comments-unreadable"), "500").unwrap();

    let err = world.execute(operation()).await.unwrap_err();

    assert!(matches!(err, EffectError::Adapter { .. }), "got {err:?}");
    assert_eq!(world.posted_comments(), 0);
}

#[tokio::test]
async fn a_conversation_past_the_bound_posts_nothing() {
    let world = World::new();
    for page in 1..=11 {
        world.page(
            "issue-comments",
            page,
            &[comment_with_body(page, "chatter")],
        );
    }

    let err = world.execute(operation()).await.unwrap_err();

    assert!(matches!(err, EffectError::Adapter { .. }), "got {err:?}");
    assert_eq!(world.posted_comments(), 0);
}

#[tokio::test]
async fn the_review_comment_collection_is_never_touched() {
    let world = World::new();
    world.page("issue-comments", 1, &[]);

    world.execute(operation()).await.unwrap();

    let requests = world.dir.path().join("requests");
    let paths: Vec<String> = std::fs::read_dir(&requests)
        .unwrap()
        .map(|entry| {
            let recorded: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(entry.unwrap().path()).unwrap())
                    .unwrap();
            recorded["argv"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|arg| arg.as_str())
                .find(|arg| arg.starts_with('/'))
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    assert!(
        !paths.iter().any(|path| path.contains("/pulls/")),
        "the review-comment collection must never be asked for: {paths:?}"
    );
    assert!(
        paths.iter().any(|path| path.contains("/issues/")),
        "the conversation must have been read: {paths:?}"
    );
}

#[test]
fn the_target_names_the_conversation_and_the_question() {
    let request = DecisionRequestId("0123456789abcdef".to_string());
    assert_eq!(
        fiddle_runtime::human::decision_request_target("acme/widget", 7, &request),
        "acme/widget#7:0123456789abcdef"
    );
}

#[test]
fn the_operation_proposes_under_the_canonical_target() {
    let op = operation();
    assert_eq!(
        op.target(),
        fiddle_runtime::human::decision_request_target(REPO, PR, &binding().request)
    );
}

#[tokio::test]
async fn a_question_proposed_under_another_capability_is_refused() {
    let world = World::new();
    world.page("issue-comments", 1, &[]);

    let ctx = EffectContext::new(
        world.gh(),
        unreachable_git(),
        world.dir.path().to_path_buf(),
        CancellationToken::new(),
    );
    let deployment = Deployment(DeploymentRule::Allow);
    let executor = Executor::new(
        PUBLISH_CHANGE,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        &world.steps,
        ReadRetry::none(),
    );
    let op = operation();
    let proposed = ProposedEffect {
        capability: CapabilityId("someone-else"),
        kind: EffectKind::PublishDecisionRequest,
        target: op.target(),
        payload: op.payload(),
    };

    let err = executor.execute(proposed, op).await.unwrap_err();

    assert!(
        matches!(err, EffectError::PolicyDenied { .. }),
        "got {err:?}"
    );
    assert_eq!(world.posted_comments(), 0);
}

#[tokio::test]
async fn the_receipt_names_the_comment_the_world_holds_and_not_the_one_the_response_claimed() {
    let world = World::new();
    world.page("issue-comments", 1, &[]);
    world.on_post("201 0 answers_a_run_id");

    let receipt = world.execute(operation()).await.unwrap();

    assert_eq!(world.posted_comments(), 1);
    assert!(
        matches!(
            receipt.value,
            InteractionRef::GitHubPullRequestComment { comment: 9000, .. }
        ),
        "the receipt must name the observed comment, got {:?}",
        receipt.value
    );
    assert_eq!(receipt.external_ref.as_deref(), Some("9000"));
}

#[tokio::test]
async fn a_create_that_answers_without_a_comment_id_is_settled_by_the_read() {
    let world = World::new();
    world.page("issue-comments", 1, &[]);
    world.on_post("201 0 echo_token");

    let receipt = world.execute(operation()).await.unwrap();

    assert_eq!(world.posted_comments(), 1, "exactly one comment, not two");
    assert_eq!(receipt.outcome, fiddle_runtime::EffectOutcome::Committed);
    assert!(
        matches!(
            receipt.value,
            InteractionRef::GitHubPullRequestComment { comment: 9000, .. }
        ),
        "got {:?}",
        receipt.value
    );
    let steps = world.steps();
    let applied = steps
        .iter()
        .position(|s| *s == ExecutionStep::Apply.as_str())
        .expect("the mutation was dispatched");
    assert!(
        steps[applied + 1..].contains(&ExecutionStep::ObservePostcondition.as_str()),
        "the executor must have looked after the answer was lost: {steps:?}"
    );
    assert!(
        !format!("{receipt:?}").contains(TEST_TOKEN),
        "the credential must not reach a receipt"
    );
}

#[tokio::test]
async fn the_port_reads_every_reply_on_the_conversation() {
    let world = World::new();
    world.page("issue-comments", 1, &[comment_with_body(21, "first")]);
    world.page("issue-comments", 2, &[comment_with_body(22, "second")]);
    world.page("issue-comments", 3, &[comment_with_body(23, "third")]);
    let interaction = InteractionRef::GitHubPullRequestComment {
        repo: REPO.to_string(),
        pr: PR,
        comment: 21,
    };

    let replies = GitHubConversation
        .responses(&world.ctx(), &interaction)
        .await
        .unwrap();

    assert_eq!(
        replies.iter().map(|r| r.comment).collect::<Vec<_>>(),
        [21, 22, 23],
        "the whole conversation, oldest first"
    );
}

#[tokio::test]
async fn the_port_refuses_an_unreadable_conversation_rather_than_reporting_no_replies() {
    let world = World::new();
    std::fs::write(world.dir.path().join("issue-comments-unreadable"), "500").unwrap();
    let interaction = InteractionRef::GitHubPullRequestComment {
        repo: REPO.to_string(),
        pr: PR,
        comment: 21,
    };

    let err = GitHubConversation
        .responses(&world.ctx(), &interaction)
        .await
        .unwrap_err();

    assert!(err.to_string().contains("500"), "got {err}");
}

#[tokio::test]
async fn the_operation_recognises_the_question_its_own_body_names() {
    let world = World::new();
    let already_asked = operation().payload();
    world.page(
        "issue-comments",
        1,
        &[comment_with_body(11, &already_asked)],
    );

    let receipt = world.execute(operation()).await.unwrap();

    assert_eq!(
        world.posted_comments(),
        0,
        "the question is already published; failing to recognise it posts forever"
    );
    assert!(
        matches!(
            receipt.value,
            InteractionRef::GitHubPullRequestComment { comment: 11, .. }
        ),
        "got {:?}",
        receipt.value
    );
}

#[test]
fn the_target_names_the_id_the_rendered_marker_carries() {
    let op = operation();
    let published = parse_marker(&op.payload()).expect("the rendered body carries a marker");
    assert_eq!(
        op.target(),
        fiddle_runtime::human::decision_request_target(REPO, PR, &published.request)
    );
}
