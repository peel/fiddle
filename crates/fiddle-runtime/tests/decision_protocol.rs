mod support;

use fiddle_core::decision::{
    decision_request_id, render_marker, DecisionBinding, DecisionRequestId,
    InterpretedHumanDecision,
};
use fiddle_core::{
    effect_id, payload_hash, EffectId, EffectName, PayloadHash, ENSURE_PULL_REQUEST_READY,
};
use fiddle_runtime::effect::{EffectContext, IntegrationOperation, ResolvedDecision};
use fiddle_runtime::github::EnsurePullRequestReady;
use fiddle_runtime::human::interpret::InterpretationBounds;
use fiddle_runtime::human::validate::{
    resolve, DecisionError, DecisionResolution, DecisionStep, DecisionTrace, DecisionWalk, Ignored,
};
use fiddle_runtime::GhCli;
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use support::{unreachable_git, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const REPO: &str = "acme/r";

const PR: u64 = 7;

const HEAD_SHA: &str = "3f9a1c2b4d6e8f0a1b2c3d4e5f60718293a4b5c6";

const MAX_PAGES: u32 = 10;

const PATIENT: Duration = Duration::from_secs(60);

const APPROVER: u64 = 505_401;

const STRANGER: u64 = 999_999;

const QUESTION: &str = "May fiddle mark pull request acme/r#7 ready for review?";

const STAMP: &str = "2026-08-10T12:00:00Z";

const APPROVES: &str = r#"{"decision":"approve","redirect":null,"evidence":"go ahead"}"#;

const APPROVES_WITH_NO_REDIRECT_KEY: &str = r#"{"decision":"approve","evidence":"go ahead"}"#;

const YES: &str = "yes, go ahead";

#[derive(Clone)]
struct Comment {
    id: u64,
    author: u64,
    login: String,
    body: String,
    kind: &'static str,
    app: bool,
    created_at: String,
    updated_at: String,
}

impl Comment {
    fn new(id: u64, author: u64, body: &str) -> Self {
        Self {
            id,
            author,
            login: format!("user-{author}"),
            body: body.to_string(),
            kind: "User",
            app: false,
            created_at: STAMP.to_string(),
            updated_at: STAMP.to_string(),
        }
    }

    fn spelled(mut self, login: &str) -> Self {
        self.login = login.to_string();
        self
    }

    fn a_bot(mut self) -> Self {
        self.kind = "Bot";
        self
    }

    fn via_an_app(mut self) -> Self {
        self.app = true;
        self
    }

    fn rewritten_at(mut self, when: &str) -> Self {
        self.updated_at = when.to_string();
        self
    }

    fn json(&self) -> Value {
        json!({
            "id": self.id,
            "body": self.body,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
            "author_association": "COLLABORATOR",
            "user": {"login": self.login, "id": self.author, "type": self.kind},
            "performed_via_github_app": match self.app {
                true => json!({"slug": "some-app"}),
                false => Value::Null,
            },
        })
    }
}

fn derived() -> (DecisionRequestId, EffectId, PayloadHash) {
    let effect = effect_id(
        PROJECT,
        INVOCATION_REF,
        ENSURE_PULL_REQUEST_READY,
        &operation().target(),
    );
    let request = decision_request_id(PROJECT, INVOCATION_REF, &effect);
    let payload = payload_hash(&operation().payload());
    (request, effect, payload)
}

fn operation() -> EnsurePullRequestReady {
    EnsurePullRequestReady::new(REPO.to_string(), PR, HEAD_SHA.to_string())
}

fn genuine_marker() -> String {
    let (request, effect, payload) = derived();
    render_marker(&DecisionBinding {
        request,
        effect,
        payload,
        head_sha: HEAD_SHA.to_string(),
    })
}

fn forged_marker() -> String {
    let (request, _, payload) = derived();
    render_marker(&DecisionBinding {
        request,
        effect: EffectId("0123456789abcdef".to_string()),
        payload,
        head_sha: HEAD_SHA.to_string(),
    })
}

fn request_comment(id: u64) -> Comment {
    Comment::new(id, APPROVER, &format!("{QUESTION}\n\n{}", genuine_marker()))
}

struct World {
    dir: TempDir,
    steps: Mutex<Vec<&'static str>>,
    model: MockCompletionModel,
    allowlist: Vec<u64>,
}

impl DecisionTrace for World {
    fn step(&self, step: DecisionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl World {
    fn new(scripted: &str) -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        let world = Self {
            dir,
            steps: Mutex::new(Vec::new()),
            model: MockCompletionModel::new([MockTurn::text(scripted)]),
            allowlist: vec![APPROVER],
        };
        world.pull(json!({
            "state": "open",
            "draft": true,
            "node_id": "PR_kwDOabcdef",
            "head": {"sha": HEAD_SHA},
        }));
        world
    }

    fn authorizing(mut self, ids: &[u64]) -> Self {
        self.allowlist = ids.to_vec();
        self
    }

    fn pull(&self, body: Value) {
        let dir = self.dir.path().join("pulls_by_number");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{PR}.json")), body.to_string()).unwrap();
    }

    fn converse(&self, comments: &[Comment]) {
        let dir = self.dir.path().join("issue-comments");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("page-1.json"),
            Value::Array(comments.iter().map(Comment::json).collect()).to_string(),
        )
        .unwrap();
        for comment in comments {
            self.by_id(comment);
        }
    }

    fn by_id(&self, comment: &Comment) {
        let dir = self.dir.path().join("issue-comments").join("by-id");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{}.json", comment.id)),
            comment.json().to_string(),
        )
        .unwrap();
    }

    fn unreadable(&self, status: u16) {
        std::fs::write(
            self.dir.path().join("issue-comments-unreadable"),
            status.to_string(),
        )
        .unwrap();
    }

    fn ctx(&self) -> EffectContext {
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

    async fn resolve(&self) -> Result<DecisionResolution, DecisionError> {
        let ctx = self.ctx();
        let operation = operation();
        let target = operation.target();
        let payload = operation.payload();
        let walk = DecisionWalk {
            repo: REPO,
            pr: PR,
            max_pages: MAX_PAGES,
            project: PROJECT,
            invocation_ref: INVOCATION_REF,
            kind: EffectName::shipped(ENSURE_PULL_REQUEST_READY),
            target: &target,
            payload: &payload,
            allowlist: &self.allowlist,
        };
        resolve(
            &ctx,
            &walk,
            QUESTION,
            self.model.clone(),
            &InterpretationBounds {
                max_reply_bytes: 4_096,
                max_tokens: 256,
                deadline: Duration::from_secs(30),
            },
            self,
        )
        .await
    }

    fn trace(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    fn model_calls(&self) -> usize {
        self.model.requests().len()
    }

    fn prompts(&self) -> String {
        self.model
            .requests()
            .iter()
            .map(|request| serde_json::to_string(request).expect("a request serializes"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn mutations(&self) -> usize {
        std::fs::read_to_string(self.dir.path().join("graphql_calls"))
            .ok()
            .and_then(|seen| seen.trim().parse().ok())
            .unwrap_or(0)
    }
}

const ASKED: u64 = 1_001;

type Build = fn() -> World;

fn approving() -> World {
    let world = World::new(APPROVES);
    world.converse(&[request_comment(ASKED), Comment::new(1_002, APPROVER, YES)]);
    world
}

fn without_request() -> World {
    let world = World::new(APPROVES);
    world.converse(&[Comment::new(1_002, APPROVER, YES)]);
    world
}

fn with_duplicate_request() -> World {
    let world = World::new(APPROVES);
    let quoted = Comment::new(1_002, APPROVER, &request_comment(ASKED).body);
    world.converse(&[request_comment(ASKED), quoted]);
    world
}

fn with_marker_for_another_effect() -> World {
    let world = World::new(APPROVES);
    let forged = Comment::new(
        ASKED,
        APPROVER,
        &format!("{QUESTION}\n\n{}", forged_marker()),
    );
    world.converse(&[forged, Comment::new(1_002, APPROVER, YES)]);
    world
}

fn with_edited_request() -> World {
    let world = World::new(APPROVES);
    let edited = request_comment(ASKED).rewritten_at("2026-08-10T13:00:00Z");
    world.converse(&[edited, Comment::new(1_002, APPROVER, YES)]);
    world
}

fn with_edited_approval() -> World {
    let world = approving();
    world
        .by_id(&Comment::new(1_002, APPROVER, "actually, no").rewritten_at("2026-08-10T13:00:00Z"));
    world
}

fn with_closed_pr() -> World {
    let world = approving();
    world.pull(json!({
        "state": "closed",
        "draft": true,
        "node_id": "PR_kwDOabcdef",
        "head": {"sha": HEAD_SHA},
    }));
    world
}

fn with_ready_pr() -> World {
    let world = approving();
    world.pull(json!({
        "state": "open",
        "draft": false,
        "node_id": "PR_kwDOabcdef",
        "head": {"sha": HEAD_SHA},
    }));
    world
}

fn with_moved_head() -> World {
    let world = approving();
    world.pull(json!({
        "state": "open",
        "draft": true,
        "node_id": "PR_kwDOabcdef",
        "head": {"sha": "0000000000000000000000000000000000000000"},
    }));
    world
}

fn with_only_unauthorized_replies() -> World {
    let world = World::new(APPROVES);
    world.converse(&[
        request_comment(ASKED),
        Comment::new(1_002, STRANGER, YES),
        Comment::new(1_003, STRANGER, "approve"),
    ]);
    world
}

fn with_nobody_authorized() -> World {
    approving().authorizing(&[])
}

fn with_a_widened_payload() -> World {
    let world = World::new(APPROVES);
    let (request, effect, _) = derived();
    let widened = render_marker(&DecisionBinding {
        request,
        effect,
        payload: payload_hash(r#"{"head":"*","pr":7,"repo":"acme/r"}"#),
        head_sha: HEAD_SHA.to_string(),
    });
    world.converse(&[
        Comment::new(ASKED, APPROVER, &format!("{QUESTION}\n\n{widened}")),
        Comment::new(1_002, APPROVER, YES),
    ]);
    world
}

fn with_an_unreadable_conversation() -> World {
    let world = approving();
    world.unreadable(500);
    world
}

fn with_only_the_request_comment() -> World {
    let world = World::new(APPROVES);
    world.converse(&[request_comment(ASKED)]);
    world
}

fn with_authorized_replies(scripted: &str, bodies: &[&str]) -> World {
    let world = World::new(scripted);
    let mut conversation = vec![request_comment(ASKED)];
    for (at, body) in bodies.iter().enumerate() {
        conversation.push(Comment::new(1_002 + at as u64, APPROVER, body));
    }
    world.converse(&conversation);
    world
}

#[tokio::test]
async fn the_order_is_announced_before_the_work_behind_each_step() {
    let world = approving();
    let decision = world.resolve().await.expect("an approving world resolves");
    assert_eq!(
        decision
            .answer
            .as_ref()
            .map(|answer| &answer.interpreted)
            .expect("somebody answered"),
        &InterpretedHumanDecision::Approve
    );
    assert_eq!(
        world.trace(),
        [
            "recompute_identity",
            "find_request",
            "parse_binding",
            "select_candidates",
            "re_read_candidates",
            "re_observe_state",
            "interpret",
            "compare_payload",
        ]
    );
}

#[tokio::test]
async fn a_walk_that_stops_announces_no_step_it_did_not_take() {
    let world = without_request();
    let _ = world.resolve().await;
    assert_eq!(world.trace(), ["recompute_identity", "find_request"]);
}

#[tokio::test]
async fn nothing_the_shell_refuses_reaches_the_model() {
    let worlds: [(&str, Build); 9] = [
        ("no request comment", without_request),
        ("two request comments", with_duplicate_request),
        ("foreign effect", with_marker_for_another_effect),
        ("edited request", with_edited_request),
        ("edited approval", with_edited_approval),
        ("closed pull request", with_closed_pr),
        ("already ready", with_ready_pr),
        ("head moved", with_moved_head),
        ("only unauthorized replies", with_only_unauthorized_replies),
    ];
    for (name, build) in worlds {
        let world = build();
        let _ = world.resolve().await;
        assert!(
            !world.trace().contains(&DecisionStep::Interpret.as_str()),
            "{name} reached the model"
        );
        assert_eq!(world.model_calls(), 0, "{name} spent a model call");
    }
}

#[tokio::test]
async fn every_refusal_names_what_actually_moved() {
    assert!(matches!(
        without_request().resolve().await,
        Err(DecisionError::RequestAbsent(_))
    ));
    assert!(matches!(
        with_duplicate_request().resolve().await,
        Err(DecisionError::DuplicateRequest { count: 2, .. })
    ));
    assert!(matches!(
        with_marker_for_another_effect().resolve().await,
        Err(DecisionError::ForeignEffect { .. })
    ));
    let edited_request = with_edited_request()
        .resolve()
        .await
        .expect_err("an edited request comment refuses");
    assert!(matches!(
        edited_request,
        DecisionError::RequestEdited { comment: ASKED }
    ));
    assert!(
        !edited_request.to_string().contains("since it was listed"),
        "the evidence is `created_at != updated_at`, which an edit made before the \
         listing fails too: {edited_request}"
    );
    assert!(matches!(
        with_edited_approval().resolve().await,
        Err(DecisionError::ReplyEdited { comment: 1_002 })
    ));
    assert!(matches!(
        with_closed_pr().resolve().await,
        Err(DecisionError::NotOpen)
    ));
    assert!(matches!(
        with_ready_pr().resolve().await,
        Err(DecisionError::AlreadyReady)
    ));
    assert!(matches!(
        with_moved_head().resolve().await,
        Err(DecisionError::HeadMoved { .. })
    ));
    assert!(matches!(
        with_a_widened_payload().resolve().await,
        Err(DecisionError::ForeignPayload { .. })
    ));
    assert!(matches!(
        with_an_unreadable_conversation().resolve().await,
        Err(DecisionError::Unreadable(_))
    ));
}

#[tokio::test]
async fn no_two_refusals_read_the_same_to_a_person() {
    let mut refusals = Vec::new();
    for world in [
        without_request(),
        with_duplicate_request(),
        with_marker_for_another_effect(),
        with_edited_request(),
        with_edited_approval(),
        with_closed_pr(),
        with_ready_pr(),
        with_moved_head(),
        with_a_widened_payload(),
        with_an_unreadable_conversation(),
    ] {
        refusals.push(
            world
                .resolve()
                .await
                .expect_err("each of these worlds refuses"),
        );
    }
    for (at, refusal) in refusals.iter().enumerate() {
        for other in &refusals[at + 1..] {
            assert_ne!(
                std::mem::discriminant(refusal),
                std::mem::discriminant(other),
                "{refusal:?} and {other:?} are two conditions sharing one variant"
            );
        }
    }
    assert_eq!(
        refusals.len(),
        DecisionError::VARIANT_COUNT,
        "the worlds above are written by hand and each reaches its own variant, so \
         this line is what makes them every refusal rather than the ones somebody \
         remembered; a refusal no world here provokes is read by nobody"
    );
    let messages: Vec<String> = refusals
        .iter()
        .map(|refusal| without_numbers(&refusal.to_string()))
        .collect();
    for (at, message) in messages.iter().enumerate() {
        assert!(
            !messages[at + 1..].contains(message),
            "{message:?} is two refusals told apart only by the numbers in them"
        );
    }
}

fn without_numbers(message: &str) -> String {
    message.chars().filter(|c| !c.is_ascii_digit()).collect()
}

#[tokio::test]
async fn an_unreadable_conversation_is_not_a_missing_request() {
    let world = with_an_unreadable_conversation();
    assert!(matches!(
        world.resolve().await,
        Err(DecisionError::Unreadable(_))
    ));
    assert_eq!(world.model_calls(), 0);
    assert_eq!(
        world.trace(),
        ["recompute_identity", "find_request"],
        "a step announced after its work says nothing about the work that failed"
    );
}

#[tokio::test]
async fn a_parse_is_not_an_authentication() {
    let world = with_marker_for_another_effect();
    let Err(DecisionError::ForeignEffect { found, derived }) = world.resolve().await else {
        panic!("a marker naming another effect must be refused on the recomputation");
    };
    assert_eq!(found, "0123456789abcdef", "the marker's own claim");
    assert_ne!(
        derived, found,
        "the recomputation is what disagrees with it"
    );
    assert_eq!(
        world.trace().last(),
        Some(&"parse_binding"),
        "the refusal belongs to step 3 and not to a later one"
    );
}

#[tokio::test]
async fn a_quoted_request_with_a_field_altered_is_a_second_request_and_not_a_choice() {
    let world = World::new(APPROVES);
    let quoted = Comment::new(
        1_002,
        APPROVER,
        &format!("> {QUESTION}\n\n{}", forged_marker()),
    );
    world.converse(&[request_comment(ASKED), quoted]);

    assert!(matches!(
        world.resolve().await,
        Err(DecisionError::DuplicateRequest { count: 2, .. })
    ));
    assert_eq!(world.model_calls(), 0);
}

#[tokio::test]
async fn an_unauthorized_reply_is_observed_ignored_and_recorded() {
    let world = World::new(APPROVES);
    world.converse(&[
        request_comment(ASKED),
        Comment::new(1_002, STRANGER, "approve"),
        Comment::new(1_003, APPROVER, YES),
    ]);

    let decision = world.resolve().await.expect("an authorized reply answered");
    let answer = decision.answer.as_ref().expect("somebody answered");
    assert_eq!(answer.interpreted, InterpretedHumanDecision::Approve);
    assert_eq!(answer.acted_on.comment, 1_003);
    assert!(
        decision
            .ignored
            .iter()
            .any(|i| i.comment == 1_002 && i.reason == Ignored::ActorNotAuthorized),
        "the ignored reply must be recorded: {:?}",
        decision.ignored
    );
    assert_eq!(
        Ignored::ActorNotAuthorized.as_str(),
        "actor not authorized",
        "the reason a person reads has one spelling"
    );
}

#[tokio::test]
async fn the_allowlist_matches_the_numeric_id_and_not_the_login() {
    let world = World::new(APPROVES);
    world.converse(&[
        request_comment(ASKED),
        Comment::new(1_002, STRANGER, YES).spelled(&format!("user-{APPROVER}")),
    ]);

    let decision = world
        .resolve()
        .await
        .expect("an unanswered question is not an error");
    assert!(
        decision.acted_on_nothing(),
        "a login collision must not authorize"
    );
    assert_eq!(world.model_calls(), 0);
}

#[tokio::test]
async fn neither_a_bot_nor_an_app_can_decide() {
    for reply in [
        Comment::new(1_002, APPROVER, YES).a_bot(),
        Comment::new(1_002, APPROVER, YES).via_an_app(),
    ] {
        let world = World::new(APPROVES);
        world.converse(&[request_comment(ASKED), reply]);

        let decision = world
            .resolve()
            .await
            .expect("an unanswered question is not an error");
        assert!(decision.acted_on_nothing());
        assert!(decision
            .ignored
            .iter()
            .any(|i| i.comment == 1_002 && i.reason == Ignored::NotAPerson));
        assert_eq!(world.model_calls(), 0);
    }
}

#[tokio::test]
async fn a_deployment_that_nominated_nobody_authorizes_nobody() {
    let world = with_nobody_authorized();
    let decision = world
        .resolve()
        .await
        .expect("an unanswered question is not an error");
    assert!(
        decision.acted_on_nothing(),
        "an empty allowlist cannot authorize the approval it received"
    );
    assert!(
        decision.considered.is_empty(),
        "there is no candidate to consider: {:?}",
        decision.considered
    );
    assert!(
        decision
            .ignored
            .iter()
            .any(|i| i.comment == 1_002 && i.reason == Ignored::ActorNotAuthorized),
        "the reply is recorded as declined rather than dropped: {:?}",
        decision.ignored
    );
    assert_eq!(world.model_calls(), 0);
}

#[tokio::test]
async fn the_request_comment_is_never_read_as_a_reply_to_itself() {
    let world = with_only_the_request_comment();
    let decision = world
        .resolve()
        .await
        .expect("an unanswered question is not an error");
    assert!(decision.acted_on_nothing());
    assert!(decision
        .ignored
        .iter()
        .any(|i| i.comment == ASKED && i.reason == Ignored::RequestComment));
    assert_eq!(world.model_calls(), 0);
}

const EARLIER: &str = "on-reflection-ignore-this-line";

const LATER: &str = "this-is-the-line-that-counts";

#[tokio::test]
async fn the_last_authorized_reply_decides_and_the_earlier_ones_are_evidence() {
    let rows: [(&str, Expect); 3] = [
        (
            r#"{"decision":"reject","redirect":null,"evidence":"counts"}"#,
            Expect::Reject,
        ),
        (
            r#"{"decision":"approve","redirect":null,"evidence":"counts"}"#,
            Expect::Approve,
        ),
        (
            r#"{"decision":"unclear","redirect":null,"evidence":"counts"}"#,
            Expect::Unclear,
        ),
    ];
    for (scripted, expected) in rows {
        let world = with_authorized_replies(scripted, &[EARLIER, LATER]);
        let decision = world.resolve().await.expect("an authorized reply answered");
        let answer = decision.answer.as_ref().expect("somebody answered");

        assert_eq!(Expect::of(&answer.interpreted), expected, "{scripted}");
        assert_eq!(answer.acted_on.comment, 1_003, "the greatest id decides");
        assert_eq!(
            decision.considered.len(),
            2,
            "the superseded reply is evidence, not a comment to forget"
        );

        let prompts = world.prompts();
        assert!(
            prompts.contains(LATER),
            "the later reply must be what was read"
        );
        assert!(
            !prompts.contains(EARLIER),
            "a superseded reply must not be handed to the model as well"
        );
    }
}

#[tokio::test]
async fn a_scrambled_listing_reaches_the_same_decision_as_a_sorted_one() {
    let world = World::new(r#"{"decision":"approve","redirect":null,"evidence":"counts"}"#);
    world.converse(&[
        Comment::new(1_003, APPROVER, LATER),
        Comment::new(1_002, APPROVER, EARLIER),
        request_comment(ASKED),
    ]);

    let decision = world
        .resolve()
        .await
        .expect("the order of a page is not a fact");
    let answer = decision.answer.as_ref().expect("somebody answered");
    assert_eq!(answer.interpreted, InterpretedHumanDecision::Approve);
    assert_eq!(
        answer.acted_on.comment, 1_003,
        "the greatest id decides whatever position it arrived in"
    );
    assert_eq!(
        decision
            .considered
            .iter()
            .map(|reply| reply.comment)
            .collect::<Vec<_>>(),
        [1_002, 1_003],
        "the superseded reply comes first however the page arrived"
    );
    assert!(!world.prompts().contains(EARLIER));
}

#[tokio::test]
async fn both_admissible_spellings_of_an_approval_reach_the_same_verdict() {
    for scripted in [APPROVES, APPROVES_WITH_NO_REDIRECT_KEY] {
        let world = World::new(scripted);
        world.converse(&[request_comment(ASKED), Comment::new(1_002, APPROVER, YES)]);
        let decision = world.resolve().await.expect("an authorized reply answered");
        assert_eq!(
            decision.answer.expect("somebody answered").interpreted,
            InterpretedHumanDecision::Approve,
            "{scripted}"
        );
    }
}

#[test]
fn a_verdict_that_is_not_an_approval_has_no_spelling_the_executor_would_spend() {
    let (request, effect, payload) = derived();
    let binding = || DecisionBinding {
        request: request.clone(),
        effect: effect.clone(),
        payload: payload.clone(),
        head_sha: HEAD_SHA.to_string(),
    };
    assert!(
        ResolvedDecision::approved(binding(), &InterpretedHumanDecision::Approve).is_some(),
        "reject-then-approve mutates"
    );
    for refused in [
        InterpretedHumanDecision::Reject {
            reason: fiddle_core::Published::of("no"),
        },
        InterpretedHumanDecision::Redirect {
            instruction: fiddle_core::Published::of("do it differently"),
        },
        InterpretedHumanDecision::Unclear,
    ] {
        assert!(
            ResolvedDecision::approved(binding(), &refused).is_none(),
            "{refused:?} must not reach step 4"
        );
    }
}

#[tokio::test]
async fn an_approval_for_a_different_payload_is_refused_before_the_executor() {
    let world = with_a_widened_payload();
    let error = world
        .resolve()
        .await
        .expect_err("a payload nobody approved is refused");
    assert!(error.to_string().contains("payload"), "got {error}");
    assert_eq!(
        world.trace().last(),
        Some(&"compare_payload"),
        "the refusal belongs to step 8"
    );
    assert_eq!(world.mutations(), 0);
}

#[tokio::test]
async fn no_identity_this_run_holds_appears_in_what_reached_the_model() {
    let world = approving();
    world.resolve().await.expect("an approving world resolves");
    assert_eq!(world.model_calls(), 1, "there must be one request to read");

    let prompts = world.prompts();
    let (request, effect, payload) = derived();
    for (what, identity) in [
        ("the effect id", effect.0.as_str()),
        ("the payload digest", payload.0.as_str()),
        ("the request id", request.0.as_str()),
        ("the head sha", HEAD_SHA),
        ("the effect's target", &operation().target()),
    ] {
        assert!(
            !prompts.contains(identity),
            "{what} ({identity}) reached the model in {prompts}"
        );
    }
    assert!(
        prompts.contains("go ahead"),
        "the reply must have been read"
    );
}

#[derive(Debug, Eq, PartialEq)]
enum Expect {
    Approve,
    Reject,
    Unclear,
}

impl Expect {
    fn of(decision: &InterpretedHumanDecision) -> Self {
        match decision {
            InterpretedHumanDecision::Approve => Expect::Approve,
            InterpretedHumanDecision::Reject { .. } => Expect::Reject,
            InterpretedHumanDecision::Redirect { .. } | InterpretedHumanDecision::Unclear => {
                Expect::Unclear
            }
        }
    }
}
