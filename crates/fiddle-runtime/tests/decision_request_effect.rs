//! Publishing the question, which is a mutation and passes the executor like
//! every other one.
//!
//! `POST /repos/{repo}/issues/{pr}/comments` documents no idempotency key of any
//! kind, so a request comment whose answer was lost and which is then re-sent
//! makes a **second** comment. That is not a cosmetic duplicate: the validation
//! walk chooses candidate replies by their position relative to *the* request
//! comment, so two of those is a question with no answerable thread. The
//! executor's step 3 is exactly the inspect-before-write the endpoint does not
//! offer, and its step 8 is what settles a lost answer by reading rather than by
//! asking again.
//!
//! Driven through the product's `cli.program` seam against the scripted `gh` in
//! `tests/gh_stub/`, like `human_comments` and `ready_effect`. Nothing here
//! reaches GitHub. The `git` inside the context is a path that does not exist, so
//! an operation that grew a push behind the executor's back would fail loudly
//! rather than quietly acquire a second mutation channel.
//!
//! # Why the ids in here are what they are
//!
//! The stub assigns a posted comment `9000 + n`, and seeded comments are numbered
//! by the test. Both are far from zero and far from one on purpose: an assertion
//! that a receipt names comment `9000` cannot pass by accident against an index,
//! a count or a page number, which an assertion about comment `1` could.

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

/// A generous bound for a stub that answers immediately. No case here is about
/// the deadline; `github_cli` owns that one.
const PATIENT: Duration = Duration::from_secs(30);

/// Sentinel-shaped, like every other suite's, so nothing here can pass against
/// an empty string.
const TEST_TOKEN: &str = "ghp_decision_request_sentinel_must_not_appear";

const PROJECT: &str = "acme/widget";
const INVOCATION_REF: &str = "beans:w-1";
const REPO: &str = "acme/widget";
const PR: u64 = 7;
const HEAD: &str = "1111111111111111111111111111111111111111";
/// A second revision, for the question that is *not* this question.
const OTHER_HEAD: &str = "2222222222222222222222222222222222222222";

// ---------------------------------------------------------------------------
// The question under test
// ---------------------------------------------------------------------------

/// The effect a question gates: making this pull request ready at one revision.
///
/// Derived rather than written down, because the request id derives from it and a
/// hand-written pair would let the two disagree — which is the one thing a suite
/// about finding your own question again must not allow.
fn gated_effect(head: &str) -> EffectId {
    effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsurePullRequestReady,
        &format!("{REPO}#{PR}@{head}"),
    )
}

/// The binding a request comment's marker carries, for the question about `head`.
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

/// A question about a different revision, and therefore a different question:
/// the gated effect differs, so the request id differs.
fn other_binding() -> DecisionBinding {
    binding_for(OTHER_HEAD)
}

/// The question this suite publishes, with every prose field filled in.
fn request_with(
    question: &str,
    rationale: &str,
    risks: &[&str],
    alternatives: &[&str],
    evidence: &[&str],
) -> HumanDecisionRequest {
    let binding = binding();
    HumanDecisionRequest {
        request: binding.request.clone(),
        invocation_ref: INVOCATION_REF.to_string(),
        work_ref: Some(WorkRef("w-1".to_string())),
        capability: PUBLISH_CHANGE,
        binding,
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

/// The ordinary question, used by every case whose subject is not the rendering.
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

// ---------------------------------------------------------------------------
// Comments the world already holds
// ---------------------------------------------------------------------------

/// A comment carrying a request marker, in the shape the listing returns.
///
/// The body is the marker alone rather than a whole rendered question, and that
/// is deliberate: what makes a comment *this request* is the marker, so a fixture
/// whose seeded comments carried prose too would leave it unclear which half the
/// operation matched on.
fn comment_with_marker(id: u64, binding: &DecisionBinding) -> serde_json::Value {
    comment_with_body(id, &render_marker(binding))
}

/// A comment with a body written verbatim.
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

// ---------------------------------------------------------------------------
// The scripted world
// ---------------------------------------------------------------------------

/// What the deployment document says. `Allow` throughout: the combination rule is
/// exhaustively tested in `fiddle-core`, and this operation's minimum is
/// `Automatic`, so nothing here is about policy.
struct Deployment(DeploymentRule);

impl DeploymentPolicy for Deployment {
    fn rule_for(&self, _kind: EffectKind) -> DeploymentRule {
        self.0
    }
}

/// The executor writes down which step it is on, and the world keeps the list, so
/// the *order* is assertable rather than only the endpoints.
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

    /// A `GhCli` pointed at the scripted `gh`.
    ///
    /// The scratch directory arrives through `cli.args` rather than through the
    /// environment, for the reason the fixture's own header gives: the adapter
    /// clears the environment and sets exactly five names, so a sixth could not
    /// reach the child even if this test wanted one.
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

    /// One page of one collection.
    ///
    /// A conversation that really is empty is scripted by writing `page-1.json`
    /// holding `[]`, which says so on purpose — the fixture has no unscripted
    /// default here, because an empty answer is a legitimate conversation and a
    /// fixture that produced one for a file a test forgot to write would let that
    /// test assert "no request was found" against a world it never built.
    fn page(&self, collection: &str, page: u64, comments: &[serde_json::Value]) {
        let dir = self.dir.path().join(collection);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("page-{page}.json")),
            serde_json::Value::Array(comments.to_vec()).to_string(),
        )
        .unwrap();
    }

    /// How the `POST` to the conversation should *end*. The world it builds is
    /// never scripted — that is the stub's whole design.
    ///
    /// The key is derived from the same repo and number the operation addresses,
    /// so a script cannot come to name a path the operation stopped requesting.
    fn on_post(&self, spec: &str) {
        let script = self.dir.path().join("script");
        std::fs::create_dir_all(&script).unwrap();
        let key = format!(
            "POST_{}",
            format!("repos/{REPO}/issues/{PR}/comments").replace('/', "_")
        );
        std::fs::write(script.join(key), spec).unwrap();
    }

    /// The mutation lands and the answer is then really lost — the shape this
    /// whole fixture exists for. The stub mutates and *then* dies; a stub that
    /// exited first would be testing a failed write, which proves nothing.
    fn on_post_apply_then_die(&self) {
        self.on_post("201 0 commit_then_die");
    }

    /// Every comment this run actually posted, read out of the world log the stub
    /// appends to — so the count is what happened out there rather than what the
    /// executor believes about itself.
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

    /// How many comments were posted.
    fn posted_comments(&self) -> usize {
        self.posted().len()
    }

    /// The body of the nth posted comment, as the request carried it.
    fn posted_body(&self, n: usize) -> String {
        let posted = self.posted();
        let request: serde_json::Value =
            serde_json::from_str(posted[n]["body"].as_str().unwrap_or("{}")).unwrap();
        request["body"].as_str().unwrap_or_default().to_string()
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.0.lock().unwrap().clone()
    }

    /// A context whose `gh` is the scripted one and whose `git` cannot be run.
    fn ctx(&self) -> EffectContext {
        EffectContext::new(
            self.gh(),
            unreachable_git(),
            self.dir.path().to_path_buf(),
            CancellationToken::new(),
        )
    }

    /// Walk the authorization order for one question.
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
            // One read and no waiting, chosen rather than inherited: a case that
            // silently acquired a backoff would be asserting the postcondition
            // against a world that quietly retried under it.
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

/// A `git` that cannot be run. An operation that grew a push behind the
/// executor's back would fail loudly here instead of quietly acquiring a second
/// mutation channel.
fn unreachable_git() -> GitCli {
    GitCli::new(
        PathBuf::from("/nonexistent/git"),
        String::new(),
        "FIDDLE_GITHUB_TOKEN",
        Duration::from_secs(1),
    )
}

// ---------------------------------------------------------------------------
// Exactly once
// ---------------------------------------------------------------------------

/// The postcondition is "a comment carrying this request's marker exists here",
/// so a run that already asked posts nothing and returns what it found.
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
    // Settled at step 3, so the walk never reached the mutation. Asserted as the
    // *order* rather than only as a count, so somebody reordering the executor's
    // steps fails here rather than nothing failing.
    assert!(
        !world.steps().contains(&ExecutionStep::Apply.as_str()),
        "got {:?}",
        world.steps()
    );
}

/// The one mutation, once.
#[tokio::test]
async fn a_request_not_yet_published_is_posted_exactly_once() {
    let world = World::new();
    world.page("issue-comments", 1, &[]);

    let receipt = world.execute(operation()).await.unwrap();

    assert_eq!(world.posted_comments(), 1);
    assert_eq!(receipt.outcome, fiddle_runtime::EffectOutcome::Committed);
    // The stub numbers a posted comment from 9000, so this is the comment the
    // `POST` created and not a seeded one.
    assert!(
        matches!(
            receipt.value,
            InteractionRef::GitHubPullRequestComment { comment: 9000, .. }
        ),
        "got {:?}",
        receipt.value
    );
}

/// The lost answer, and the property M2 established. The stub applies the `POST`
/// and then dies, so the world really changed and the answer really was lost;
/// step 8 reads it back and the run does not post a second comment.
#[tokio::test]
async fn a_lost_answer_is_settled_by_reading_and_never_by_posting_again() {
    let world = World::new();
    world.page("issue-comments", 1, &[]);
    world.on_post_apply_then_die();

    let receipt = world.execute(operation()).await.unwrap();

    assert_eq!(world.posted_comments(), 1, "exactly one comment, not two");
    assert_eq!(receipt.outcome, fiddle_runtime::EffectOutcome::Committed);
    // The distinguishing half: it was settled by *looking*, not by asking again.
    // Without this the case would pass against an executor that re-dispatched and
    // happened to be believed, since the count above would then be the second
    // `POST`'s own success rather than the first one's read-back.
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

/// The revision moved, so the question moved with it — and the earlier question's
/// comment is not this one's postcondition.
///
/// Without this, one conversation could only ever hold one question: a second
/// question on the same pull request would find the first one's comment, conclude
/// it had already asked, and wait forever for a reply to a question nobody had
/// been asked.
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
    // And the comment it settled on is the one it posted, not the decoy.
    assert!(
        matches!(
            receipt.value,
            InteractionRef::GitHubPullRequestComment { comment: 9000, .. }
        ),
        "got {:?}",
        receipt.value
    );
}

// ---------------------------------------------------------------------------
// Two of them is a state to report
// ---------------------------------------------------------------------------

/// Two comments naming one request is a state to report, never a set to pick
/// from — the same rule `EnsurePullRequest` applies to two open pull requests.
///
/// Sharper here than there: the validation walk chooses candidate replies by
/// their position relative to *the* request comment, so a run that picked one of
/// two would be mining a thread it had guessed at.
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

/// The count is the actionable half and it is the real one, so three is reported
/// as three.
///
/// A `count` hard-coded to two, or derived from anything but the comments found,
/// would satisfy the case above and tell an operator to go and delete one comment
/// when there are two to delete.
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

/// A duplicate spread across pages is still a duplicate.
///
/// The one case a client that stopped at the first page would get wrong while
/// passing every other case here: it would find one comment, call it the
/// postcondition, and report a settled question against a conversation that holds
/// two.
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

// ---------------------------------------------------------------------------
// The question is decidable, and it is what was hashed
// ---------------------------------------------------------------------------

/// Automatic, and it must be: a question that needed a question would not
/// terminate. Asserted rather than left to the reader of the struct.
#[test]
fn publishing_a_question_never_requires_a_question() {
    assert_eq!(
        operation().minimum(),
        HumanDecisionRequirement::Automatic,
        "a question that required a question to ask would not terminate"
    );
}

/// The rendered body is the payload, so a question whose text changed is a
/// widened request and step 6 refuses it. And the marker is in the body that is
/// actually posted, not only in the one that was hashed.
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

/// A person has to be able to decide from this comment alone. Every field the
/// RFC requires is in it.
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

/// The prose comes first and the marker last.
///
/// Not cosmetic: the marker is an HTML comment, so a person reading the rendered
/// conversation sees nothing of it, and a rendering that opened with it would put
/// an invisible line where the question belongs. A reader scanning a notification
/// email sees the first line.
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

/// A section nobody filled in is absent rather than empty.
///
/// An empty **Risks** heading reads as a claim that there are none, which is a
/// different thing from a field the caller left blank — and it is the more
/// dangerous of the two to put in front of somebody about to approve something.
#[test]
fn an_empty_section_is_omitted_rather_than_rendered_empty() {
    let body = render_request(&request_with("Ready?", "Because.", &[], &[], &[]));
    for absent in ["Risks", "Alternatives considered", "Evidence"] {
        assert!(
            !body.contains(absent),
            "{absent:?} has no items and must not have a heading:\n{body}"
        );
    }
    // And the question and rationale are still there, so the case above is about
    // the empty sections rather than about an empty rendering.
    assert!(
        body.contains("Ready?") && body.contains("Because."),
        "{body}"
    );
}

/// Two questions differing only in prose are the same effect and different
/// requests.
///
/// This is the identity/payload split at this operation, and it is what makes
/// step 6 able to refuse a widened question at all. The target names the
/// conversation and the request id, neither of which the prose touches — so
/// rewording does not open a second question — while the payload *is* the prose,
/// so the reworded question does not pass as the one that was approved.
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

/// The question's own effect is not the effect it gates.
///
/// Worth pinning because the two are one keystroke apart in every derivation and
/// the consequence of confusing them is silent: a request published under the
/// gated effect's identity would collide with the approval's own binding, and the
/// question would appear to have been answered before it was asked.
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

// ---------------------------------------------------------------------------
// An unreadable conversation is never an empty one
// ---------------------------------------------------------------------------

/// A listing that could not be read is not a conversation with no question in it.
///
/// The consequence of getting this wrong is unbounded: reading a failed listing as
/// "not asked yet" posts a fresh question on every attempt for as long as the
/// listing stays broken, which is exactly the duplicate supply this operation goes
/// through the executor to prevent.
#[tokio::test]
async fn an_unreadable_conversation_posts_nothing() {
    let world = World::new();
    std::fs::write(world.dir.path().join("issue-comments-unreadable"), "500").unwrap();

    let err = world.execute(operation()).await.unwrap_err();

    assert!(matches!(err, EffectError::Adapter { .. }), "got {err:?}");
    assert_eq!(world.posted_comments(), 0);
}

/// A conversation longer than the bound is refused rather than truncated, and
/// nothing is posted.
///
/// "I read the whole conversation and found no question" and "I read as much as I
/// was allowed and found no question" are different facts, and only the first of
/// them may be acted on by posting.
#[tokio::test]
async fn a_conversation_past_the_bound_posts_nothing() {
    let world = World::new();
    // One more page than `human/mod.rs`'s bound, each holding a comment that is
    // not the request, so the read walks every page and runs out.
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

/// The conversation, and never the review comments.
///
/// A question posted to `/pulls/{n}/comments` would be a question about a line of
/// a diff, and — worse — nothing reads that collection, so the run would never
/// find its own request again and would ask on every attempt. Asserted as the
/// endpoint never being requested, which is the only form of the claim a filter
/// could not also satisfy.
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

// ---------------------------------------------------------------------------
// The target, spelled once
// ---------------------------------------------------------------------------

/// `{repo}#{pr}:{request_id}` — the conversation, and which question on it.
///
/// Pinned against a literal rather than against a second `format!`, because it is
/// hashed into the effect identity: a round trip through the same expression would
/// agree with any spelling at all, including one a later process could not
/// recompute.
#[test]
fn the_target_names_the_conversation_and_the_question() {
    let request = DecisionRequestId("0123456789abcdef".to_string());
    assert_eq!(
        fiddle_runtime::human::decision_request_target("acme/widget", 7, &request),
        "acme/widget#7:0123456789abcdef"
    );
}

/// And the operation's own target is that function's output, so the two cannot
/// drift.
#[test]
fn the_operation_proposes_under_the_canonical_target() {
    let op = operation();
    assert_eq!(
        op.target(),
        fiddle_runtime::human::decision_request_target(REPO, PR, &binding().request)
    );
}

/// A capability the executor is not bound to cannot propose this effect.
///
/// Not this operation's own rule — step 1 owns it — but asserted here because the
/// question is the effect a capability is most likely to propose on somebody
/// else's behalf, being the one effect that is *about* another effect.
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

// ---------------------------------------------------------------------------
// The port, and what the executor does with what it answers
// ---------------------------------------------------------------------------

/// The receipt names the comment the *world* holds, never the one the response
/// claimed.
///
/// `HumanInteractionPort::request` reads the created comment's id off the create's
/// own answer, because that is the only place it exists at that moment. This is
/// the case that makes reading it harmless: the stub answers `999999`, which is
/// not the id of anything in the world it describes, and the receipt has to carry
/// the id the listing shows. Without this, an implementation that built its
/// receipt from the response would pass every other case in this file — the
/// counts and the outcome would all be right — while reporting a comment nobody
/// can open.
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

/// A create whose answer carried no comment id still ends committed, exactly once.
///
/// The scripted mode answers 201 with a body that is a message rather than a
/// comment — and puts the credential in it, which is why the token assertion is
/// here: nothing built from that body reaches the receipt.
///
/// **What this case does not prove.** `HumanInteractionPort::request` refuses a
/// create that named no id rather than defaulting one, and that refusal is
/// *unobservable from here*: `apply` discards the port's answer, and the executor
/// reads the world back at step 8 whether the mutation reported success or
/// failure, so a port that returned comment `0` instead of erroring produces this
/// same receipt. An inversion replacing the check with `unwrap_or_default()`
/// breaks no test in this file, and no test can be written that it would break —
/// the port cannot be called from a test at all, because `AuthorizedEffect` has
/// no public constructor. The strictness stays because defaulting an id is a lie
/// about which comment this is; it is documented here as untested rather than
/// left looking covered.
///
/// What *is* pinned below is real and is the reason the case exists: a 201 the
/// client could not read a comment out of leaves exactly one comment in the
/// world, and the walk settles it by looking rather than by asking again.
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
    // Settled by looking, and the look came after the dispatch.
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

/// The port reads the replies back, and it reads the whole conversation.
///
/// Every page, and not the replies the port judges relevant: which comments are
/// candidate answers is `validate`'s decision, made against a run and an
/// allowlist, and a transport that pre-filtered would be making it somewhere with
/// neither. The seeded pages are three so that a port which stopped at the first
/// fails here.
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

/// And an unreadable conversation is not an empty one, on the way back as well as
/// on the way out.
///
/// The read the port performs is the same `read_conversation` the postcondition
/// uses, so this is the fail-closed rule holding at the reply end: a caller that
/// received `Ok(vec![])` from a broken listing would conclude nobody had answered
/// and wait forever.
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

// ---------------------------------------------------------------------------
// The request id the type carries twice
// ---------------------------------------------------------------------------

/// The question is identified by the id its **marker** carries, and by nothing
/// else it happens to hold.
///
/// `HumanDecisionRequest` carries the request id twice — as `request` and as
/// `binding.request` — and nothing makes the two agree. Only `binding.request`
/// reaches the marker, because that is what `render_marker` is given. So an
/// operation matching on the other field publishes a marker naming one id and
/// then searches for a different one: it finds nothing, concludes it has not asked
/// yet, and posts again on **every attempt, forever**. That is the unbounded
/// duplicate supply this operation exists to prevent, arriving through the one
/// door the executor cannot close — from step 3's view the postcondition really is
/// absent each time, so no amount of inspect-before-write helps.
///
/// This is the only case in the file that can notice. Every other one builds a
/// request whose two ids agree, so both readings behave identically and the bug is
/// invisible. Here they are made to disagree on purpose, which is why the request
/// is assembled by hand rather than through `request_with`.
#[tokio::test]
async fn the_question_is_identified_by_the_id_its_marker_carries() {
    let world = World::new();
    // The marker on the conversation names the *binding's* id, because that is
    // what a marker can name.
    world.page("issue-comments", 1, &[comment_with_marker(11, &binding())]);

    let mut divergent = request();
    // A well-formed id that is not this question's. If the operation reads this
    // field, it will not recognise the comment above.
    divergent.request = DecisionRequestId("dddddddddddddddd".to_string());
    assert_ne!(
        divergent.request, divergent.binding.request,
        "this case is only meaningful while the two disagree"
    );
    let op = PublishDecisionRequest::new(REPO.to_string(), PR, divergent);

    let receipt = world.execute(op).await.unwrap();

    assert_eq!(
        world.posted_comments(),
        0,
        "the request was already published; reading the wrong id posts forever"
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

/// And the target names that same id, so the effect identity a fresh process
/// recomputes is the one the marker can be found by.
///
/// Separate from the case above because they fail for different reasons: that one
/// is about the postcondition lookup, and this one is about the identity the
/// approval is bound to. A target built from the other field would derive an
/// effect id no continuation could match against the marker it read.
#[test]
fn the_target_names_the_id_the_marker_carries() {
    let mut divergent = request();
    let binding_id = divergent.binding.request.clone();
    divergent.request = DecisionRequestId("dddddddddddddddd".to_string());
    let op = PublishDecisionRequest::new(REPO.to_string(), PR, divergent);

    assert_eq!(
        op.target(),
        fiddle_runtime::human::decision_request_target(REPO, PR, &binding_id)
    );
    assert!(
        !op.target().contains("dddddddddddddddd"),
        "the target must not name the field the marker cannot carry: {}",
        op.target()
    );
}
