//! The validation order, and every way it refuses.
//!
//! Eight steps, of which the first six are deterministic and run before the one
//! model call. Every case below is offline and free: the world is the scripted
//! `gh` in `tests/gh_stub/`, reached through the product's own `cli.program`
//! seam, and the model is `MockCompletionModel`, reached through the generic
//! parameter `interpret` already carries. There is no credential and no socket in
//! this file.
//!
//! # The two claims that carry the weight
//!
//! **Nothing the shell would refuse costs a model call.** Nine refusals are
//! reachable without asking anybody anything — no request comment, two of them, a
//! marker naming another effect, an edited request, an edited approval, a closed
//! pull request, one already out of draft, a moved head, and a conversation whose
//! only replies are unauthorized. Each is asserted twice: that
//! `DecisionStep::Interpret` was never announced, and that the scripted model
//! recorded zero requests. The second is what makes the first more than a claim
//! about a log.
//!
//! **No identity reaches the model.** `interpret` takes the question as text so
//! that it cannot receive an `EffectId`, a `PayloadHash` or a head sha, and that
//! guarantee is total inside it and conditional on whoever composes the string.
//! `resolve` is that caller, and
//! [`no_identity_this_run_holds_appears_in_what_reached_the_model`] asserts it
//! against the *serialized outbound request* rather than against the builder —
//! the arrangement `binary_repair`'s
//! `the_serialized_request_offers_four_tools_and_carries_no_host_fact`
//! established.
//!
//! # Order, not position
//!
//! `read_conversation`'s query pins no sort order, so every rule under test is
//! stated as a comparison of comment ids.
//! [`a_scrambled_listing_reaches_the_same_decision_as_a_sorted_one`] returns the
//! conversation with the approval first and the question last, and requires the
//! same answer. Against a sorted fixture "higher id than the request comment" and
//! "after the request comment in the vector" are indistinguishable; this is the
//! case that tells them apart.

mod support;

use fiddle_core::decision::{
    decision_request_id, render_marker, DecisionBinding, DecisionRequestId,
    InterpretedHumanDecision,
};
use fiddle_core::{effect_id, payload_hash, EffectId, EffectKind, PayloadHash};
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

/// The repository the conversation lives in.
const REPO: &str = "acme/r";

/// The pull request. Seven rather than one, so an assertion cannot pass by
/// accident against an index or a count.
const PR: u64 = 7;

/// The revision the question was asked about, spelled the way GitHub spells one.
///
/// Forty hex characters rather than a short sentinel because it is one of the
/// values [`no_identity_this_run_holds_appears_in_what_reached_the_model`]
/// searches a prompt for, and a four-character stand-in could appear inside an
/// unrelated digest and make that test pass or fail for the wrong reason.
const HEAD_SHA: &str = "3f9a1c2b4d6e8f0a1b2c3d4e5f60718293a4b5c6";

/// A generous page bound. No case here is about pagination; `human_comments`
/// owns that one.
const MAX_PAGES: u32 = 10;

/// A generous process bound for a stub that answers immediately.
const PATIENT: Duration = Duration::from_secs(60);

/// The numeric id this deployment nominated. Ids and not logins, which is the
/// whole subject of two cases below.
const APPROVER: u64 = 505_401;

/// A numeric id nobody nominated.
const STRANGER: u64 = 999_999;

/// fiddle's own question. It is the shell's text, and it contains the word an
/// interpreter is looking for, which is why `interpret` keeps it in a labelled
/// block of its own.
const QUESTION: &str = "May fiddle mark pull request acme/r#7 ready for review?";

/// The timestamp a comment nobody has touched carries in both its fields.
const STAMP: &str = "2026-08-10T12:00:00Z";

/// The one shape the RFC's own documentation describes.
const APPROVES: &str = r#"{"decision":"approve","redirect":null,"evidence":"go ahead"}"#;

/// The second shape that approves, and the one nothing enumerates.
///
/// Recorded empirically against the real `Reply` shape rather than reasoned
/// about: `redirect` is an `Option`, so serde gives an absent field an implicit
/// `None` and this document reaches `Approve` exactly as the one above does. The
/// set of approving documents has two members, not one, and
/// [`both_admissible_spellings_of_an_approval_reach_the_same_verdict`] is here so
/// that a later change to the schema which silently narrows or widens that set
/// does not pass.
const APPROVES_WITH_NO_REDIRECT_KEY: &str = r#"{"decision":"approve","evidence":"go ahead"}"#;

/// The body of a reply that approves. The evidence span above is a substring of
/// it, which `interpret` requires.
const YES: &str = "yes, go ahead";

// ---------------------------------------------------------------------------
// The conversation a walk reads
// ---------------------------------------------------------------------------

/// One comment, as the listing returns it.
///
/// A builder rather than nine positional arguments because most cases vary one
/// field: this author is a bot, this one's id is not on the allowlist, this one's
/// `updated_at` moved. The defaults are a person on the allowlist whose comment
/// nobody has touched.
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

    /// The same comment under somebody else's name, with its id untouched. The
    /// login-collision case.
    fn spelled(mut self, login: &str) -> Self {
        self.login = login.to_string();
        self
    }

    /// An account whose type is `Bot`, which is one of GitHub's two ways of not
    /// being a person.
    fn a_bot(mut self) -> Self {
        self.kind = "Bot";
        self
    }

    /// A comment an app posted through somebody's credential, which is the other.
    fn via_an_app(mut self) -> Self {
        self.app = true;
        self
    }

    /// A comment somebody has rewritten since it was created.
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
            // `null` is GitHub's way of saying no app was involved, and the field
            // is always present — which is why the adapter reads it as a `Value`.
            "performed_via_github_app": match self.app {
                true => json!({"slug": "some-app"}),
                false => Value::Null,
            },
        })
    }
}

/// The identity a fresh process recomputes for the gated effect.
///
/// Derived here from the same four canonical inputs `resolve` derives them from,
/// which is what makes a marker built out of these values a marker the walk
/// authenticates rather than one it merely parses.
fn derived() -> (DecisionRequestId, EffectId, PayloadHash) {
    let effect = effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsurePullRequestReady,
        &operation().target(),
    );
    let request = decision_request_id(PROJECT, INVOCATION_REF, &effect);
    let payload = payload_hash(&operation().payload());
    (request, effect, payload)
}

/// The operation a continuation rebuilds, and the source of both the target and
/// the payload.
fn operation() -> EnsurePullRequestReady {
    EnsurePullRequestReady::new(REPO.to_string(), PR, HEAD_SHA.to_string())
}

/// The marker fiddle's own request comment carries.
fn genuine_marker() -> String {
    let (request, effect, payload) = derived();
    render_marker(&DecisionBinding {
        request,
        effect,
        payload,
        head_sha: HEAD_SHA.to_string(),
    })
}

/// A marker naming this run's request and something else's effect.
///
/// Every field is well formed, so [`parse_marker`](fiddle_core::parse_marker)
/// accepts it: the request id is copied off the visible conversation, which is
/// all anybody needs in order to type one. What cannot be typed is an effect id
/// that agrees with the recomputation.
fn forged_marker() -> String {
    let (request, _, payload) = derived();
    render_marker(&DecisionBinding {
        request,
        effect: EffectId("0123456789abcdef".to_string()),
        payload,
        head_sha: HEAD_SHA.to_string(),
    })
}

/// The request comment, as fiddle posted it.
fn request_comment(id: u64) -> Comment {
    Comment::new(id, APPROVER, &format!("{QUESTION}\n\n{}", genuine_marker()))
}

// ---------------------------------------------------------------------------
// The world one walk runs against
// ---------------------------------------------------------------------------

/// The scripted `gh`'s scratch directory, the scripted model, and the steps the
/// walk announced.
struct World {
    dir: TempDir,
    steps: Mutex<Vec<&'static str>>,
    model: MockCompletionModel,
}

/// The walk writes down which step it is on, and the world keeps the list. This
/// is what makes the *order* assertable rather than only the outcome.
impl DecisionTrace for World {
    fn step(&self, step: DecisionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl World {
    /// An open draft pull request at [`HEAD_SHA`], and a model that will answer
    /// `scripted` if it is ever asked.
    ///
    /// The model is built for every world, including the ones whose whole claim is
    /// that it is never called: a world holding no model could not tell "the walk
    /// declined to ask" from "there was nothing to ask".
    fn new(scripted: &str) -> Self {
        let dir = TempDir::new().unwrap();
        // Empty and it stays empty: it is what a real `gh` would be pinned to, and
        // beside an absent `HOME` it is what makes an operator's keyring
        // unreachable.
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        let world = Self {
            dir,
            steps: Mutex::new(Vec::new()),
            model: MockCompletionModel::new([MockTurn::text(scripted)]),
        };
        world.pull(json!({
            "state": "open",
            "draft": true,
            "node_id": "PR_kwDOabcdef",
            "head": {"sha": HEAD_SHA},
        }));
        world
    }

    /// Put one pull request in the world, as `GET /repos/{repo}/pulls/{n}`
    /// answers it.
    fn pull(&self, body: Value) {
        let dir = self.dir.path().join("pulls_by_number");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{PR}.json")), body.to_string()).unwrap();
    }

    /// The whole conversation, as one page and as the by-id route that re-reads
    /// it.
    ///
    /// Both are written from one list so that the default world is *consistent* —
    /// a re-read finds the comment that was listed. A case whose subject is an
    /// edit overrides one by-id entry afterwards, which is what makes that case
    /// about the edit rather than about a fixture that disagreed with itself.
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

    /// What the by-id route answers for one comment, replacing whatever
    /// [`World::converse`] wrote.
    fn by_id(&self, comment: &Comment) {
        let dir = self.dir.path().join("issue-comments").join("by-id");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{}.json", comment.id)),
            comment.json().to_string(),
        )
        .unwrap();
    }

    /// Make the conversation unreadable, with the status GitHub answered.
    fn unreadable(&self, status: u16) {
        std::fs::write(
            self.dir.path().join("issue-comments-unreadable"),
            status.to_string(),
        )
        .unwrap();
    }

    /// A context whose `gh` is the scripted one and whose `git` cannot be run.
    fn ctx(&self) -> EffectContext {
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

    /// Walk the eight steps.
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
            kind: EffectKind::EnsurePullRequestReady,
            target: &target,
            payload: &payload,
            allowlist: &[APPROVER],
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

    /// Which steps were announced, in order.
    fn trace(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    /// How many times the model was asked anything.
    ///
    /// Read off the recorder the scripted model keeps, so this is what the walk
    /// really sent rather than what anything in this file believes it sent.
    fn model_calls(&self) -> usize {
        self.model.requests().len()
    }

    /// Every request that reached the model, serialized.
    ///
    /// `CompletionRequest` is `Serialize`, so this is the document a provider
    /// integration renders rather than a summary of the builders that assembled
    /// it. A field a leak lived in that this serialization skipped would be
    /// invisible to an assertion against the builder and visible here.
    fn prompts(&self) -> String {
        self.model
            .requests()
            .iter()
            .map(|request| serde_json::to_string(request).expect("a request serializes"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// How many mutations the world received.
    ///
    /// `resolve` has no mutation channel at all, so this is zero on every path
    /// through it; the number is asserted anyway in the payload case, because
    /// what that case claims is that the refusal happened *before* the executor
    /// — and the counter the stub keeps is the only place that claim is a fact
    /// about the world rather than about this file's control flow.
    fn mutations(&self) -> usize {
        std::fs::read_to_string(self.dir.path().join("graphql_calls"))
            .ok()
            .and_then(|seen| seen.trim().parse().ok())
            .unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// The worlds each case is about
// ---------------------------------------------------------------------------

/// The request comment's id. Below every reply's, because a candidate is a
/// comment whose id is greater than this one.
const ASKED: u64 = 1_001;

/// One scenario, named by the function that arranges it.
///
/// A named type because the table in
/// [`nothing_the_shell_refuses_reaches_the_model`] pairs a label with a builder,
/// and the builders have to be values rather than calls: a world is arranged and
/// then resolved once, so a table holding `World`s would have resolved none of
/// them and a table holding results would have lost the label.
type Build = fn() -> World;

/// A conversation in which one authorized person said yes.
fn approving() -> World {
    let world = World::new(APPROVES);
    world.converse(&[request_comment(ASKED), Comment::new(1_002, APPROVER, YES)]);
    world
}

/// A conversation carrying no marker at all.
fn without_request() -> World {
    let world = World::new(APPROVES);
    world.converse(&[Comment::new(1_002, APPROVER, YES)]);
    world
}

/// fiddle's question, and somebody quoting it back verbatim.
fn with_duplicate_request() -> World {
    let world = World::new(APPROVES);
    let quoted = Comment::new(1_002, APPROVER, &request_comment(ASKED).body);
    world.converse(&[request_comment(ASKED), quoted]);
    world
}

/// One request-shaped comment, naming this run's request and another effect.
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

/// A request comment somebody has rewritten. fiddle wrote it and has no path
/// that edits one, so `created_at` and `updated_at` disagreeing is somebody
/// else's edit.
fn with_edited_request() -> World {
    let world = World::new(APPROVES);
    let edited = request_comment(ASKED).rewritten_at("2026-08-10T13:00:00Z");
    world.converse(&[edited, Comment::new(1_002, APPROVER, YES)]);
    world
}

/// An approval that changed between the listing and the re-read.
///
/// The listing carries an untouched comment and the by-id route carries the same
/// comment with a later `updated_at`, which is the only difference — so nothing
/// but the re-read comparison can refuse this world.
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

/// The pull request force-pushed since the question was asked.
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

/// Somebody nobody nominated, answering.
fn with_only_unauthorized_replies() -> World {
    let world = World::new(APPROVES);
    world.converse(&[
        request_comment(ASKED),
        Comment::new(1_002, STRANGER, YES),
        Comment::new(1_003, STRANGER, "approve"),
    ]);
    world
}

/// fiddle's question and nothing else.
fn with_only_the_request_comment() -> World {
    let world = World::new(APPROVES);
    world.converse(&[request_comment(ASKED)]);
    world
}

/// Replies from authorized people, in the order their ids put them.
fn with_authorized_replies(scripted: &str, bodies: &[&str]) -> World {
    let world = World::new(scripted);
    let mut conversation = vec![request_comment(ASKED)];
    for (at, body) in bodies.iter().enumerate() {
        conversation.push(Comment::new(1_002 + at as u64, APPROVER, body));
    }
    world.converse(&conversation);
    world
}

// ---------------------------------------------------------------------------
// The order is a contract, so it is observable
// ---------------------------------------------------------------------------

/// Each step is announced before the work behind it, exactly as `ExecutionStep`
/// is.
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

/// A walk that stops announces the steps it took and no more.
///
/// The complement of the case above, and the reason a trace is worth reading: a
/// walk that announced all eight whatever happened would say nothing about where
/// it got to. This one refuses at step 2, so step 3 was never entered.
#[tokio::test]
async fn a_walk_that_stops_announces_no_step_it_did_not_take() {
    let world = without_request();
    let _ = world.resolve().await;
    assert_eq!(world.trace(), ["recompute_identity", "find_request"]);
}

/// The whole point of the ordering: nothing the shell would refuse costs a model
/// call.
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

// ---------------------------------------------------------------------------
// Each refusal names what actually moved
// ---------------------------------------------------------------------------

/// "Stale" with no antecedent sends its reader back to the conversation to guess,
/// so each of these is its own error naming the thing that moved.
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
    assert!(matches!(
        with_edited_request().resolve().await,
        Err(DecisionError::Edited { comment: ASKED })
    ));
    assert!(matches!(
        with_edited_approval().resolve().await,
        Err(DecisionError::Edited { comment: 1_002 })
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
}

/// Every message says which check failed, and no two of them say the same thing.
///
/// The variants above are what code matches on; these are what a person reads,
/// and a distinct variant carrying a shared message would satisfy the first claim
/// while defeating the second.
#[tokio::test]
async fn no_two_refusals_read_the_same_to_a_person() {
    let mut messages = Vec::new();
    for world in [
        without_request(),
        with_duplicate_request(),
        with_marker_for_another_effect(),
        with_edited_request(),
        with_closed_pr(),
        with_ready_pr(),
        with_moved_head(),
    ] {
        messages.push(
            world
                .resolve()
                .await
                .expect_err("each of these worlds refuses")
                .to_string(),
        );
    }
    for (at, message) in messages.iter().enumerate() {
        assert!(
            !messages[at + 1..].contains(message),
            "{message:?} is two different refusals"
        );
    }
}

/// A conversation that could not be read is never an empty one.
///
/// The distinction the whole adapter is built around, restated at this boundary:
/// "nobody has answered" is a fact this system acts on by continuing to wait, and
/// a failed read is not that fact. Reporting it as
/// [`DecisionError::RequestAbsent`] would make an outage look like a question
/// nobody had asked.
#[tokio::test]
async fn an_unreadable_conversation_is_not_a_missing_request() {
    let world = approving();
    world.unreadable(500);
    assert!(matches!(
        world.resolve().await,
        Err(DecisionError::Unreadable(_))
    ));
    assert_eq!(world.model_calls(), 0);
    // And the step was announced *before* the read it names, which is the whole
    // value of a trace: this is the only arrangement in which the two orders are
    // distinguishable, because a step announced afterwards is announced only when
    // its work succeeded — and here it did not.
    assert_eq!(
        world.trace(),
        ["recompute_identity", "find_request"],
        "a step announced after its work says nothing about the work that failed"
    );
}

// ---------------------------------------------------------------------------
// A parse is not an authentication
// ---------------------------------------------------------------------------

/// A body that parses has proven only that somebody can type.
///
/// The marker below is well formed in every dimension `parse_marker` checks, and
/// its request id is correct because a request id is visible to anyone who can
/// read the conversation. What it cannot do is name an effect that agrees with a
/// recomputation over `(project, invocation_ref, kind, target)` — values the
/// conversation does not carry — and that recomputation is step 3.
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

/// A quotation of the request comment is a second request, and two requests are
/// reported rather than chosen between.
///
/// The sharper half of the same problem: an edited quote whose four fields are
/// still well formed parses and yields a *different* binding, so a walk that
/// picked one would be picking between a question fiddle asked and a question
/// somebody typed. First is not more authoritative than last.
#[tokio::test]
async fn a_quoted_request_with_a_field_altered_is_a_second_request_and_not_a_choice() {
    let world = World::new(APPROVES);
    // The quote names this run's request and another effect — well formed, and a
    // plausible-looking question about something nobody proposed.
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

// ---------------------------------------------------------------------------
// Who may decide
// ---------------------------------------------------------------------------

/// An unauthorized reply is observed, ignored, and recorded — not dropped.
///
/// Dropping it would make "nobody has replied" and "somebody tried and was not
/// allowed" the same observation, and only one of those is a state an operator
/// can do something about.
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

/// A login is not an identity.
///
/// Matching on one would let a renamed-and-reclaimed account inherit an
/// approver's authority: the reply below is spelled exactly like the authorized
/// account and carries a different immutable id.
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

/// Bots and apps are not people, and both spellings are excluded.
///
/// Both authors below carry an id that *is* on the allowlist, so the allowlist
/// cannot be what refuses them.
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

/// The request comment is never read as a reply to itself.
///
/// Without this, fiddle's own question — which contains the word the interpreter
/// is looking for — would be the first thing resembling an answer, and the run
/// would approve itself.
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

// ---------------------------------------------------------------------------
// The last authorized reply decides
// ---------------------------------------------------------------------------

/// The body of the reply that has been superseded.
const EARLIER: &str = "on-reflection-ignore-this-line";

/// The body of the reply with the greatest id, and the evidence span every
/// scripted answer below quotes out of it.
const LATER: &str = "this-is-the-line-that-counts";

/// The last authorized reply decides, and the earlier ones are kept as evidence.
///
/// The reason is the approve-then-reject row: the mutation has not happened yet,
/// so acting on an approval already known to be superseded is a choice rather
/// than an oversight. Each row asserts three things — the verdict, that the
/// superseded reply is still in `considered`, and that the text which reached the
/// model was the later reply's and not the earlier one's.
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

/// The rule is an id comparison, so a listing that arrives out of order reaches
/// the same decision.
///
/// This is the case that distinguishes "everything with a higher id than the
/// request comment" from "everything after the request comment in the vector".
/// Both readings pass against the sorted fixture every other case here uses;
/// only the first passes against this one, where the approval comes back *first*
/// and the question comes back last.
#[tokio::test]
async fn a_scrambled_listing_reaches_the_same_decision_as_a_sorted_one() {
    // The evidence span is quoted out of `LATER` rather than out of `YES`,
    // because `interpret` requires the span to be a substring of the reply it was
    // handed — which is itself the assertion that the later reply is what
    // travelled.
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
    // The evidence is handed to a reader in conversation order, which is a second
    // and separate claim from the one above: this one is about the order of a
    // list, that one about which of its members decided. Held by different lines,
    // so that breaking either is visible here.
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

/// Both documents that approve reach the same verdict.
///
/// `redirect` is an `Option`, so serde gives an absent field an implicit `None`
/// and the shorter document below approves exactly as the longer one does. The
/// set has two members and `interpretation.rs`'s module doc describes one, which
/// makes this a coverage item rather than a defect — but a later schema change
/// that silently narrowed or widened the approving set would pass unnoticed
/// without it.
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

/// Only an approval becomes the thing the executor's step 4 will spend.
///
/// The rule above is about which reply is read; this is what makes it matter.
/// `ResolvedDecision::approved` is the only door past the verdict, so
/// approve-then-reject produces nothing to hand the executor while
/// reject-then-approve does — which is the difference between a mutation and no
/// mutation, stated over the type that gates it.
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

// ---------------------------------------------------------------------------
// The payload is compared twice, independently
// ---------------------------------------------------------------------------

/// An approval minted for another payload is refused before the executor.
///
/// Step 8, and a second *independent* comparison rather than a repeat of the
/// executor's step 6: this one is against what the conversation recorded, and the
/// executor's is against what the proposal carried. Deleting either as redundant
/// deletes one of the two claims.
#[tokio::test]
async fn an_approval_for_a_different_payload_is_refused_before_the_executor() {
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

// ---------------------------------------------------------------------------
// What reached the model
// ---------------------------------------------------------------------------

/// No identity this run holds appears anywhere in what reached the model.
///
/// `interpret`'s parameter list is what makes the leak impossible *inside* it;
/// this is what stops the caller handing it the same information by another
/// route. Asserted against the serialized outbound request rather than against
/// the prompt this file assembled, because the claim is about the bytes a
/// provider would receive.
///
/// The repository and the pull request number are deliberately *not* on the list:
/// they are in fiddle's own question, which a person reads and a model has to see
/// in order to know what it is reading a reply to. What must not travel is the
/// bookkeeping — the effect id, the payload digest, the request id and the
/// revision — because those are what a wrong reading would need in order to widen
/// what was approved.
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
    // And the reply itself did, so the assertions above are not passing because
    // nothing was sent.
    assert!(
        prompts.contains("go ahead"),
        "the reply must have been read"
    );
}

// ---------------------------------------------------------------------------
// Which branch a model chose, with the payloads dropped
// ---------------------------------------------------------------------------

/// The branch and nothing else, because the branch is the whole of what a model
/// decides.
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
            // A redirect and an unclear both produce a follow-up rather than an
            // action, and no row below scripts one, so they collapse here rather
            // than adding a variant nothing constructs.
            InterpretedHumanDecision::Redirect { .. } | InterpretedHumanDecision::Unclear => {
                Expect::Unclear
            }
        }
    }
}
