//! The whole walk: produce a change, publish a draft, ask, stop — and come back.
//!
//! Everything here is offline, credential-free and always runs. The forge is the
//! scripted `gh` in `tests/gh_stub/`, reached through the product's own
//! `cli.program` seam; the remote is a bare repository on a path, pushed to by the
//! real `git` the product spawns; the model is `MockCompletionModel`, reached
//! through the generic parameter the capability already carries. Nothing fake
//! enters the product to make any of that possible.
//!
//! # Why the assertions are about the world and not about the return value
//!
//! The claim this capability makes is *what happened out there and in what
//! order*: a branch, then a draft, then a question, and then nothing. So the
//! branch is read out of the bare repository, the pull request create and its
//! `draft` flag are read out of the request the scripted `gh` recorded, the
//! comment is read out of the collection it was posted to, and the published tree
//! is read out of the remote with `git show`. The step order is read off the
//! [`EffectTrace`] the executor announces to, which is what makes the *order*
//! assertable rather than only the endpoints — a suite that inferred it from the
//! objects would pass on an implementation that asked first and published
//! afterwards.
//!
//! # The two halves this file has to keep apart
//!
//! [`a_first_run_publishes_a_branch_a_draft_and_a_question_then_waits`] and
//! [`an_attempt_whose_check_failed_publishes_nothing_and_asks_nothing`] are the
//! same fixture, the same check and the same tool: only the *content* the script
//! writes differs. So a `write_file` that silently did nothing could not make the
//! first pass, and a shell that believed the model's `claimed_complete` could not
//! make the second fail. Both scripts claim completion, which is the point — the
//! claim is evidence beside the exit code, never instead of it.
//!
//! # Every continuation here runs in a process that could not have produced the change
//!
//! [`continue_in`] is the second half of this file, and it takes two things away
//! before running: the `git` becomes one that cannot be executed, and the
//! workspace root becomes a *file*, so no worktree can be created under it. Both
//! are removed for **every** continuation case rather than only for the one whose
//! criterion is about it, because that is the difference between a property and a
//! sample: a continuation that grew a push or a second attempt fails loudly in all
//! of them instead of quietly acquiring a second mutation channel. It is
//! `support::unreachable_git`'s own reasoning, applied to a capability that has a
//! path which legitimately pushes and a path which must not.
//!
//! # Where the two payload comparisons finally meet
//!
//! [`the_second_payload_comparison_catches_what_the_first_could_not_see`] is the
//! only test in the workspace that runs both of them in one walk:
//! [`resolve`] compares the marker's digest against the operation this run
//! rebuilds, and [`Executor::execute_decided`] compares the *approval's* digest
//! against the payload the proposal carries. Until a caller existed that did both,
//! the bridge between them was asserted at the type level only — that
//! [`ResolvedDecision::approved`] answers `None` for the three non-approvals — and
//! the end-to-end claim was unproven. It widens the proposal after `resolve` has
//! already passed, which is the one disagreement the first comparison structurally
//! cannot see.

mod fixture;
mod support;

use fiddle_core::{
    effect_id, parse_marker, payload_hash, DecisionBinding, DeploymentRule, EffectKind,
    EvidenceRef, NextAction, Observation, ProposedEffect, PROPOSE_CHANGE, PUBLISH_CHANGE,
};
use fiddle_runtime::agent::AgentBudget;
use fiddle_runtime::capability::{
    attempt_worktree, Capability, CapabilityError, ExecutionGrant, ProposeChange, ProposeConfig,
};
use fiddle_runtime::effect::{
    EffectContext, EffectError, EffectOutcome, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ReadRetry, Recurrence, ResolvedDecision,
};
use fiddle_runtime::git::GitCli;
use fiddle_runtime::github::{branch_name, pull_request_ready_target, EnsurePullRequestReady};
use fiddle_runtime::human::interpret::InterpretationBounds;
use fiddle_runtime::human::validate::{resolve, DecisionStep, DecisionTrace, DecisionWalk};
use fiddle_runtime::human::InteractionRef;
use fiddle_runtime::workspace::WorkspaceCommand;
use fiddle_runtime::GhCli;
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// The repository the question is asked in.
const REPO: &str = "acme/r";

/// The owner the head branch lives under, which is [`REPO`]'s own owner here.
const HEAD_OWNER: &str = "acme";

/// The branch a publication is proposed into.
const BASE: &str = "main";

/// The work item every run here is about.
const WORK_ID: &str = "w-1";

/// The attempt every scenario executes under. Fixed rather than minted, so the
/// evidence reference these tests assert on is a function of the run rather than
/// of the clock.
const ATTEMPT: &str = "01JQZX0000000000000000000";

/// The number the scripted forge assigns the first pull request in a world.
/// Seven rather than one, so an assertion cannot pass by accident against an
/// index or a count.
const PR: u64 = 7;

/// A generous process bound for a stub that answers immediately, and for a check
/// that compiles a crate with no dependencies.
const PATIENT: Duration = Duration::from_secs(180);

/// The immutable numeric id this deployment nominated as able to decide.
///
/// An id and not a login, which is the allowlist's whole design: a login can be
/// changed and the vacated name reclaimed, and a numeric id cannot.
const APPROVER: u64 = 505_401;

/// A numeric id nobody nominated. Whatever this account writes, it is not an
/// answer this run may act on.
const STRANGER: u64 = 999_999;

/// fiddle's own question, as [`ProposeChange`] composes it.
///
/// Written down so that the interpretation's prompt and the marker's comment can
/// be asserted against one string — and checked against the question the world
/// really received in [`suspended`], so it cannot drift from the capability's.
const QUESTION: &str = "May fiddle mark pull request acme/r#7 ready for review?";

/// The reply of somebody who agrees, and the evidence span every approving
/// document below quotes out of it.
const YES: &str = "yes, go ahead";

/// What the interpreting model answers for each of the four verdicts.
///
/// Each `evidence` span is a substring of the reply it accompanies, because
/// `interpret` refuses a document that cannot quote the comment it read — so
/// these pairs are the fixture's way of saying *the model read this reply*, and a
/// mismatched pair would land on `Unclear` rather than on the verdict the case is
/// about.
const APPROVES: &str = r#"{"decision":"approve","redirect":null,"evidence":"go ahead"}"#;
const REJECTS: &str = r#"{"decision":"reject","redirect":null,"evidence":"drop it"}"#;
const REDIRECTS: &str = r#"{"decision":"redirect","redirect":"use a bounded loop instead","evidence":"do it differently"}"#;
const UNCLEAR: &str = r#"{"decision":"unclear","redirect":null,"evidence":"what does this do"}"#;

/// The bounds one interpretation runs inside. Generous, because no case here is
/// about a bound; `interpretation.rs` owns those.
fn patient_interpretation() -> InterpretationBounds {
    InterpretationBounds {
        max_reply_bytes: 4_096,
        max_tokens: 256,
        deadline: Duration::from_secs(30),
    }
}

/// The answer a successful `markPullRequestReadyForReview` comes back with.
fn readied() -> Value {
    json!({"data": {"markPullRequestReadyForReview": {"pullRequest": {"isDraft": false}}}})
}

// ---------------------------------------------------------------------------
// The world one proposal runs against
// ---------------------------------------------------------------------------

/// The forge, the remote, the fixture and the recorded step order.
///
/// The two adapters see the *same* remote through different doors — `git` writes
/// to it over a path, the scripted `gh` reads its ref files — which is what makes
/// "the postcondition was read back rather than assumed" a real claim here.
struct World {
    dir: TempDir,
    remote: PathBuf,
    fixture: PathBuf,
    steps: Mutex<Vec<(EffectKind, &'static str)>>,
    /// The validation order, as the walk announced it.
    ///
    /// A second list rather than a widening of the first, because the two traits
    /// are separate for a reason `validate::DecisionTrace` states: the
    /// authorization order repeats once per effect and carries an
    /// [`EffectKind`], while the decision order runs once for the single effect a
    /// question gates. Keeping them apart is also what makes
    /// [`the_capability_delegates_the_whole_validation_order`] an assertion about
    /// the shared walk rather than about this capability's own control flow.
    decisions: Mutex<Vec<&'static str>>,
}

impl EffectTrace for World {
    fn step(&self, kind: EffectKind, step: ExecutionStep) {
        self.steps.lock().unwrap().push((kind, step.as_str()));
    }
}

impl DecisionTrace for World {
    fn step(&self, step: DecisionStep) {
        self.decisions.lock().unwrap().push(step.as_str());
    }
}

impl World {
    /// An empty remote, an unrepaired fixture pointing at it, and a forge holding
    /// nothing.
    fn fresh() -> Self {
        let dir = TempDir::new().unwrap();
        // `remote.git` is the name the scripted `gh` looks for beside its own
        // scratch directory; see `tests/gh_stub/gh_stub.rs`.
        let remote = dir.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        fixture::git(&remote, &["init", "-q", "--bare", "."]);
        // Empty, and stays empty: it is what a real `gh` would be pinned to, and
        // beside an absent `HOME` it is what makes an operator's keyring
        // unreachable.
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        // A conversation that really is empty, said on purpose. The scripted `gh`
        // panics on an unscripted page rather than answering an empty one — an
        // absent file is an oversight, and a fixture that defaulted it would let a
        // test assert "no question has been asked" against a world it never
        // built. The comments this run posts are appended to this page by the
        // stub, which is how the same read sees them afterwards.
        std::fs::create_dir_all(dir.path().join("issue-comments")).unwrap();
        std::fs::write(dir.path().join("issue-comments/page-1.json"), "[]").unwrap();

        let fixture = fixture::broken_crate(dir.path());
        fixture::git(
            &fixture,
            &["remote", "add", "origin", &remote.display().to_string()],
        );

        World {
            dir,
            remote,
            fixture,
            steps: Mutex::new(Vec::new()),
            decisions: Mutex::new(Vec::new()),
        }
    }

    fn workspace_root(&self) -> PathBuf {
        self.dir.path().join("workspaces")
    }

    /// The tree the push publishes, derived the way the capability derives it —
    /// through the product's own function, so this fixture cannot agree with a
    /// capability that computed a different path.
    fn work(&self) -> PathBuf {
        attempt_worktree(&self.workspace_root(), PROJECT, INVOCATION_REF)
    }

    /// A context whose `gh` is the scripted one, whose `git` is the real one, and
    /// whose worktree is the one this run's attempt will work in.
    fn ctx(&self) -> EffectContext {
        self.ctx_publishing_from(self.work())
    }

    fn ctx_publishing_from(&self, work: PathBuf) -> EffectContext {
        self.ctx_with(
            work,
            GitCli::new(
                PathBuf::from("git"),
                // Never used: a path remote authenticates nobody, which is what
                // keeps this lane credential-free while still running the exact
                // environment the product builds.
                "ghp_never_used_by_a_path_remote".to_string(),
                "FIDDLE_GITHUB_TOKEN",
                PATIENT,
            ),
        )
    }

    /// The context a continuation runs against: the same scripted forge, and a
    /// `git` that cannot be executed at all.
    ///
    /// See this module's documentation. The approve, reject, redirect and unclear
    /// paths propose no operation that reads [`EffectContext::work`], so a `git`
    /// nothing can run is not a limitation of these cases — it is the assertion.
    fn ctx_without_git(&self) -> EffectContext {
        self.ctx_with(self.work(), unreachable_git())
    }

    fn ctx_with(&self, work: PathBuf, git: GitCli) -> EffectContext {
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
            git,
            work,
            CancellationToken::new(),
        )
    }

    /// The branch this run's identity produces, recomputed rather than read back.
    fn branch(&self) -> String {
        branch_name(PROJECT, INVOCATION_REF)
    }

    /// Every branch the remote holds, in ref order.
    fn branches(&self) -> Vec<String> {
        self.git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    /// What the published branch points at, according to the remote.
    fn published_sha(&self) -> String {
        self.git_says(
            &self.remote,
            &["rev-parse", &format!("refs/heads/{}", self.branch())],
        )
    }

    /// One file of the published commit, as the remote holds it.
    ///
    /// Read out of the bare repository rather than out of the worktree the attempt
    /// worked in — the worktree is gone by then, and a tree read from it would be
    /// a claim about a directory rather than about what was published.
    fn published_file(&self, path: &str) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["show", &format!("refs/heads/{}:{path}", self.branch())])
            .current_dir(&self.remote)
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn git_says(&self, dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Every request the scripted `gh` recorded, in arrival order.
    fn requests(&self) -> Vec<serde_json::Value> {
        let dir = self.dir.path().join("requests");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        files
            .iter()
            .filter_map(|file| serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok())
            .collect()
    }

    /// The `argv` of every recorded request, flattened.
    fn argvs(&self) -> Vec<Vec<String>> {
        self.requests()
            .iter()
            .map(|request| {
                request["argv"]
                    .as_array()
                    .map(|argv| {
                        argv.iter()
                            .filter_map(|a| a.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .collect()
    }

    /// The bodies of the `POST`s to one path suffix, in arrival order.
    ///
    /// Counted from the requests rather than from the objects, because that is the
    /// number a sequence that failed to stop would move and the object count might
    /// not.
    fn posts_to(&self, suffix: &str) -> Vec<serde_json::Value> {
        self.requests()
            .iter()
            .filter(|request| {
                let argv: Vec<String> = request["argv"]
                    .as_array()
                    .map(|argv| {
                        argv.iter()
                            .filter_map(|a| a.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                argv.iter().any(|a| a == "POST")
                    && argv.iter().any(|a| a.trim_end().ends_with(suffix))
            })
            .map(|request| {
                serde_json::from_str(request["body"].as_str().unwrap_or("{}"))
                    .unwrap_or(serde_json::Value::Null)
            })
            .collect()
    }

    /// The pull requests this world was asked to create.
    fn pull_request_creates(&self) -> Vec<serde_json::Value> {
        self.posts_to("/pulls")
    }

    /// The comments this world was asked to post, oldest first.
    fn posted_comments(&self) -> Vec<String> {
        self.posts_to("/comments")
            .iter()
            .map(|body| body["body"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    /// Put the pull request the world already holds where a by-number read can
    /// find it.
    ///
    /// The scripted `gh` answers `GET /repos/{o}/{r}/pulls/{n}` from a file and
    /// panics when there is none, deliberately — an unscripted read is an
    /// oversight and not a scenario. A *continuing* process reads the pull request
    /// for its head, so a world that is to be continued in has to hold one.
    fn pull_request_at(&self, number: u64, head_sha: &str, draft: bool) {
        let dir = self.dir.path().join("pulls_by_number");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{number}.json")),
            json!({
                "number": number,
                "draft": draft,
                "state": "open",
                "node_id": "PR_kwDOabcdef",
                "head": { "sha": head_sha },
            })
            .to_string(),
        )
        .unwrap();
    }

    /// Put the replies somebody wrote after fiddle's question on the
    /// conversation, and make every comment on it answerable by its own id.
    ///
    /// Two things are being arranged, and both are the fixture's rather than the
    /// code's:
    ///
    /// - **The listing.** The scripted `gh` merges the comments this world's own
    ///   `POST`s created onto the *last* page, so the replies are written to page
    ///   one and fiddle's question arrives after them. That order is deliberately
    ///   the wrong way round: `validate::select_candidates` decides what is a
    ///   reply by comparing ids, and a fixture that also happened to be sorted
    ///   would make "after the question" and "later in the vector"
    ///   indistinguishable.
    /// - **The re-read.** The by-id route has no merge and panics on a comment
    ///   nothing scripted, so a continuation has to script one for fiddle's own
    ///   question too. It is built from the body the world *really received*,
    ///   read back out of the recorded `POST`, so a re-read cannot agree with a
    ///   marker the capability never published.
    ///
    /// Ids are assigned above `request_comment`'s and derived from it, so this
    /// cannot drift from whatever numbering the fixture chose.
    fn answered_by(&self, request_comment: u64, replies: &[(u64, &str)]) -> Vec<u64> {
        let ids: Vec<u64> = (1..=replies.len() as u64)
            .map(|offset| request_comment + offset)
            .collect();
        let listed: Vec<Value> = ids
            .iter()
            .zip(replies)
            .map(|(id, (author, body))| comment(*id, *author, body))
            .collect();
        let page = self.dir.path().join("issue-comments/page-1.json");
        std::fs::write(&page, Value::Array(listed.clone()).to_string()).unwrap();

        for reply in &listed {
            self.by_id(reply);
        }
        // Timestamps equal, because fiddle has no path that edits its own
        // question and `validate` refuses one whose two stamps differ. The author
        // is the bot the stub lists it under, so the walk declines it as a
        // candidate for its own answer.
        let question = self
            .posted_comments()
            .first()
            .expect("a suspended world has posted its question")
            .clone();
        self.by_id(&json!({
            "id": request_comment,
            "body": question,
            "created_at": POSTED_AT,
            "updated_at": POSTED_AT,
            "author_association": "OWNER",
            "user": {"login": "fiddle[bot]", "id": 1_000_001, "type": "Bot"},
            "performed_via_github_app": Value::Null,
        }));
        ids
    }

    /// What the by-id route answers for one comment.
    fn by_id(&self, comment: &Value) {
        let dir = self.dir.path().join("issue-comments/by-id");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{}.json", comment["id"].as_u64().unwrap())),
            comment.to_string(),
        )
        .unwrap();
    }

    /// Script the answer to GraphQL call `n`, status and body separately.
    ///
    /// Separate arguments because for GraphQL they are separate facts: a refusal
    /// arrives as **200** carrying an `errors[]`. `ready_effect.rs`'s fixture and
    /// its reasoning.
    fn script_graphql(&self, n: usize, status: u16, body: Value) {
        let dir = self.dir.path().join("graphql");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{n}.json")),
            json!({"status": status, "body": body}).to_string(),
        )
        .unwrap();
    }

    /// How many GraphQL calls this world was asked to answer.
    ///
    /// The mutation has exactly one spelling in this build, so this is the count
    /// of ready transitions *attempted* — which is what "an approval buys one
    /// mutation and a repeat buys none" is stated in.
    fn graphql_calls(&self) -> usize {
        std::fs::read_to_string(self.dir.path().join("graphql_calls"))
            .ok()
            .and_then(|count| count.trim().parse().ok())
            .unwrap_or(0)
    }

    /// Take the workspace away, so that no attempt could run here even if
    /// something tried.
    ///
    /// A *file* where the root directory was, rather than a missing directory:
    /// `Workspace::create` would create a missing one. The derived worktree path
    /// is unchanged, so the capability's own published-from check still agrees —
    /// which is the point. A continuation is a fresh process that never held a
    /// checkout, and this is the nearest a test can get to one.
    fn with_no_workspace_available(&self) {
        let root = self.workspace_root();
        let _ = std::fs::remove_dir_all(&root);
        std::fs::write(&root, b"not a directory").unwrap();
    }

    /// Which effects the executor entered `apply` for, in order.
    ///
    /// The mutation, and never the proposal: this is the list "the walk performed
    /// exactly these three things, in this order" is stated in.
    fn effects_performed(&self) -> Vec<EffectKind> {
        self.steps_matching(ExecutionStep::Apply)
    }

    /// Which effects were put to the executor at all, in order.
    ///
    /// Step 1 is announced before anything else happens, so an effect that was
    /// proposed and refused appears here and not in
    /// [`World::effects_performed`] — which is the distinction the gated effect's
    /// absence has to be stated in.
    fn effects_proposed(&self) -> Vec<EffectKind> {
        self.steps_matching(ExecutionStep::ValidateCapability)
    }

    fn steps_matching(&self, step: ExecutionStep) -> Vec<EffectKind> {
        self.steps
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, entered)| *entered == step.as_str())
            .map(|(kind, _)| *kind)
            .collect()
    }

    fn steps(&self) -> Vec<(EffectKind, &'static str)> {
        self.steps.lock().unwrap().clone()
    }

    /// Which steps of the validation order were announced, in order.
    fn decision_steps(&self) -> Vec<&'static str> {
        self.decisions.lock().unwrap().clone()
    }

    /// The comments whose marker names one request.
    ///
    /// Counted over the bodies the world was really asked to post, so "the run did
    /// not ask a second time" is a claim about the conversation rather than about
    /// an internal flag.
    fn comments_naming(&self, request: &fiddle_core::DecisionRequestId) -> Vec<String> {
        self.posted_comments()
            .into_iter()
            .filter(|body| parse_marker(body).is_ok_and(|binding| &binding.request == request))
            .collect()
    }
}

/// The timestamp the scripted `gh` lists a comment this run posted under, in both
/// of its fields — which is what says nobody has edited it.
///
/// Duplicated from `gh_stub`'s own constant rather than shared, because the stub
/// is a binary and its constants are private to it. It is load-bearing in one
/// direction only: a value that disagreed would make the request comment's
/// re-read report an edit, which is a loud failure and not a silent pass.
const POSTED_AT: &str = "2026-08-11T00:00:00Z";

/// One comment somebody wrote, as the listing returns it.
fn comment(id: u64, author: u64, body: &str) -> Value {
    json!({
        "id": id,
        "body": body,
        "created_at": POSTED_AT,
        "updated_at": POSTED_AT,
        "author_association": "COLLABORATOR",
        "user": {"login": format!("user-{author}"), "id": author, "type": "User"},
        "performed_via_github_app": Value::Null,
    })
}

// ---------------------------------------------------------------------------
// The capability under test
// ---------------------------------------------------------------------------

/// The configuration every scenario below runs under, with `check` and
/// `workspace_root` left for the scenario to vary.
fn config(world: &World, check: WorkspaceCommand) -> ProposeConfig {
    ProposeConfig {
        repo: REPO.to_string(),
        head_owner: HEAD_OWNER.to_string(),
        base: BASE.to_string(),
        title: "propose the change".to_string(),
        body: "opened by fiddle".to_string(),
        project: PROJECT.to_string(),
        fixture: world.fixture.clone(),
        workspace_root: world.workspace_root(),
        check,
        budget: AgentBudget {
            max_turns: 8,
            max_tokens: 4096,
            deadline: Duration::from_secs(300),
            max_changed_files: 16,
            tool_timeout: PATIENT,
        },
        deciders: vec![APPROVER],
        interpretation: patient_interpretation(),
        cancel: CancellationToken::new(),
    }
}

/// M1's own check, unchanged: the package's test suite, offline.
///
/// The fixture genuinely fails it until `src/lib.rs` is edited, which is what
/// makes every verdict below a verdict about the tree rather than about the model.
fn the_projects_own_check() -> WorkspaceCommand {
    WorkspaceCommand {
        program: "cargo".to_string(),
        args: vec!["test".to_string(), "--offline".to_string()],
        timeout: PATIENT,
    }
}

/// A check that passes over any tree at all, for the one case that is about a
/// tree nobody changed.
fn a_check_that_always_passes() -> WorkspaceCommand {
    WorkspaceCommand {
        program: "git".to_string(),
        args: vec!["status".to_string(), "--porcelain".to_string()],
        timeout: PATIENT,
    }
}

/// The model writes the fix, runs the check itself, and reports.
fn repairs() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call(
            "c1",
            "write_file",
            json!({"path": "src/lib.rs", "contents": fixture::REPAIRED}),
        ),
        MockTurn::tool_call("c2", "run_check", json!({})),
        MockTurn::text(
            r#"{"changed_files":["src/lib.rs"],"summary":"fixed","claimed_complete":true}"#,
        ),
    ]
}

/// The model reads one file, changes nothing, and says it is done.
fn claims_success() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call("c1", "read_file", json!({"path": "src/lib.rs"})),
        MockTurn::text(r#"{"changed_files":[],"summary":"all good","claimed_complete":true}"#),
    ]
}

/// The model writes something *else*, so a run that attempted twice would publish
/// a different commit and be visible in the tree.
fn repairs_differently() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call(
            "c1",
            "write_file",
            json!({"path": "src/lib.rs", "contents": "pub fn last_index(len: usize) -> usize { len - 1 } // again\n"}),
        ),
        MockTurn::text(
            r#"{"changed_files":["src/lib.rs"],"summary":"again","claimed_complete":true}"#,
        ),
    ]
}

fn grant_for(capability: fiddle_core::CapabilityId) -> ExecutionGrant {
    ExecutionGrant::authorise(
        &NextAction::Execute {
            capability_id: capability,
        },
        &fiddle_core::AttemptId(ATTEMPT.to_string()),
    )
    .expect("an Execute derivation authorises")
}

/// One whole execution, and the capability afterwards so its receipts and its
/// publication can be read.
///
/// The capability is built here rather than by each test because four things have
/// to agree for a run to be legal at all — the executor's binding, the
/// configuration's project, the context's worktree and the grant — and a test
/// that assembled them itself would be free to get one wrong for a reason it did
/// not intend.
async fn run(
    world: &World,
    script: Vec<MockTurn>,
    check: WorkspaceCommand,
) -> (
    Result<EvidenceRef, CapabilityError>,
    Vec<EvidenceRef>,
    Option<fiddle_core::Publication>,
) {
    run_with(
        world,
        MockCompletionModel::new(script),
        check,
        PROPOSE_CHANGE,
        None,
    )
    .await
}

/// The same, with the executor's binding and the published-from tree open to a
/// test that is about one of them.
///
/// The model arrives already built rather than as a script, because a case about
/// the continuation has to be able to read what was asked of it afterwards —
/// `MockCompletionModel` is cloned into the walk and the clone shares its record,
/// which is how "an unauthorized reply reaches no model" is asserted against the
/// requests that were made rather than against a log.
async fn run_with(
    world: &World,
    model: MockCompletionModel,
    check: WorkspaceCommand,
    bound_to: fiddle_core::CapabilityId,
    publishing_from: Option<PathBuf>,
) -> (
    Result<EvidenceRef, CapabilityError>,
    Vec<EvidenceRef>,
    Option<fiddle_core::Publication>,
) {
    let ctx = match publishing_from {
        Some(work) => world.ctx_publishing_from(work),
        None => world.ctx(),
    };
    execute_against(world, &ctx, model, check, bound_to).await
}

/// A continuation: the same capability, over the world a suspended run left, in a
/// process that holds neither a workspace nor a usable `git`.
///
/// See this module's documentation for why both are taken away from every case
/// here rather than from one.
async fn continue_in(
    world: &World,
    model: MockCompletionModel,
) -> (
    Result<EvidenceRef, CapabilityError>,
    Vec<EvidenceRef>,
    Option<fiddle_core::Publication>,
) {
    world.with_no_workspace_available();
    let ctx = world.ctx_without_git();
    execute_against(world, &ctx, model, the_projects_own_check(), PROPOSE_CHANGE).await
}

async fn execute_against(
    world: &World,
    ctx: &EffectContext,
    model: MockCompletionModel,
    check: WorkspaceCommand,
    bound_to: fiddle_core::CapabilityId,
) -> (
    Result<EvidenceRef, CapabilityError>,
    Vec<EvidenceRef>,
    Option<fiddle_core::Publication>,
) {
    let deployment = Deployment(DeploymentRule::Allow);
    let executor = Executor::new(
        bound_to,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        ctx,
        world,
        ReadRetry::none(),
    );
    let capability = ProposeChange::new(executor, ctx, world, model, config(world, check));

    let outcome = capability
        .execute(grant_for(PROPOSE_CHANGE), WORK_ID, INVOCATION_REF)
        .await;
    (outcome, capability.receipts(), capability.publication())
}

/// What a first run left behind, for the continuation that reads it.
struct Suspension {
    /// The comment fiddle's question really landed on, taken off the interaction
    /// the run named rather than assumed from the fixture's numbering.
    comment: u64,
    /// The revision the question was asked about.
    head_sha: String,
}

/// Run once, so that the world holds a published change and an unanswered
/// question.
///
/// The whole first half of the walk really runs — the attempt, the push, the
/// draft, the comment — so every continuation below reads a question fiddle
/// actually asked, at a revision it actually published. A hand-seeded marker
/// would be a test of the fixture's arithmetic.
async fn suspended(world: &World) -> Suspension {
    let (outcome, _, _) = run(world, repairs(), the_projects_own_check()).await;
    let (request, comment, question) = match outcome {
        Err(CapabilityError::AwaitingDecision {
            request,
            interaction: InteractionRef::GitHubPullRequestComment { comment, .. },
            question,
        }) => (request, comment, question),
        other => panic!("a first run suspends, got {other:?}"),
    };
    let head_sha = world.published_sha();
    assert_eq!(
        request,
        identity_at(&head_sha).0,
        "the question is the one a fresh process derives"
    );
    assert_eq!(
        question, QUESTION,
        "and it is the text this file asserts on"
    );
    // The world the second process reads: the pull request answerable by number,
    // as a real forge would answer it.
    world.pull_request_at(PR, &head_sha, true);
    Suspension { comment, head_sha }
}

/// The walk a continuation performs, rebuilt here from canonical inputs.
///
/// Used by the bridge case, which drives `resolve` and the executor directly
/// rather than through the capability — so it needs the same inputs the
/// capability derives, derived the same way.
fn walk_at<'a>(target: &'a str, payload: &'a str, allowlist: &'a [u64]) -> DecisionWalk<'a> {
    DecisionWalk {
        repo: REPO,
        pr: PR,
        max_pages: 10,
        project: PROJECT,
        invocation_ref: INVOCATION_REF,
        kind: EffectKind::EnsurePullRequestReady,
        target,
        payload,
        allowlist,
    }
}

/// The request id, the gated effect id and the payload digest a *fresh* process
/// would derive for one pull request at one revision.
///
/// Recomputed from the canonical inputs rather than read back out of the marker,
/// so an assertion about the marker cannot pass on a build that invented an
/// identity and wrote it down consistently.
fn identity_at(
    head_sha: &str,
) -> (
    fiddle_core::DecisionRequestId,
    fiddle_core::EffectId,
    fiddle_core::PayloadHash,
) {
    let effect = effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsurePullRequestReady,
        &pull_request_ready_target(REPO, PR, head_sha),
    );
    let request = fiddle_core::decision_request_id(PROJECT, INVOCATION_REF, &effect);
    let payload = payload_hash(
        &EnsurePullRequestReady::new(REPO.to_string(), PR, head_sha.to_string()).payload(),
    );
    (request, effect, payload)
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// It is registered, it names its own stage, and the stage is not another
/// capability's vocabulary — the defect `Capability::stage` exists because of.
#[tokio::test]
async fn the_fourth_capability_is_registered_and_names_its_own_stage() {
    let ids: Vec<&str> = fiddle_runtime::CAPABILITIES
        .iter()
        .map(|capability| capability.0)
        .collect();
    assert_eq!(
        ids,
        [
            "stub_mark",
            "fixture_repair",
            "publish_change",
            "propose_change"
        ]
    );

    let world = World::fresh();
    let ctx = world.ctx();
    let deployment = Deployment(DeploymentRule::Allow);
    let executor = Executor::new(
        PROPOSE_CHANGE,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        &world,
        ReadRetry::none(),
    );
    let capability = ProposeChange::new(
        executor,
        &ctx,
        &world,
        MockCompletionModel::new(repairs()),
        config(&world, the_projects_own_check()),
    );
    assert_eq!(capability.id(), PROPOSE_CHANGE);
    assert_eq!(capability.stage(), "propose");
    assert!(
        capability.publication().is_none(),
        "a capability that has not run has reached no forge"
    );
}

// ---------------------------------------------------------------------------
// The walk
// ---------------------------------------------------------------------------

/// The walk, in order, and nothing out of it. The first run performs three
/// automatic effects and then stops.
#[tokio::test]
async fn a_first_run_publishes_a_branch_a_draft_and_a_question_then_waits() {
    let world = World::fresh();
    let (outcome, _, _) = run(&world, repairs(), the_projects_own_check()).await;

    let error = outcome.expect_err("a run that asked a question produced no evidence");
    assert!(
        matches!(error, CapabilityError::AwaitingDecision { .. }),
        "got {error:?}"
    );
    assert_eq!(
        error.recurrence(),
        Recurrence::Awaiting,
        "waiting is not failing, and exit 10 is what says so"
    );

    assert_eq!(
        world.effects_performed(),
        [
            EffectKind::EnsureBranchPublished,
            EffectKind::EnsurePullRequest,
            EffectKind::PublishDecisionRequest,
        ],
        "{:?}",
        world.steps()
    );

    // And each of the three really reached the world, read from three different
    // doors: the remote's refs, the create the forge recorded, and the collection
    // the comment was posted to.
    assert_eq!(world.branches(), [world.branch()]);
    let creates = world.pull_request_creates();
    assert_eq!(creates.len(), 1, "{creates:?}");
    assert_eq!(
        creates[0]["draft"],
        json!(true),
        "the pull request is opened as a draft, because the transition out of \
         draft is the gated act: {}",
        creates[0]
    );
    assert_eq!(
        creates[0]["head"],
        json!(format!("{HEAD_OWNER}:{}", world.branch()))
    );
    assert_eq!(world.posted_comments().len(), 1);
}

/// The gated effect is not proposed on the first run at all. Proposing it and
/// catching the refusal would work, and would also mean a deployment document
/// that said `deny` produced a denial rather than a question.
#[tokio::test]
async fn the_gated_effect_is_not_proposed_before_there_is_an_answer() {
    let world = World::fresh();
    let _ = run(&world, repairs(), the_projects_own_check()).await;

    assert!(!world
        .effects_performed()
        .contains(&EffectKind::EnsurePullRequestReady));
    assert!(
        !world
            .effects_proposed()
            .contains(&EffectKind::EnsurePullRequestReady),
        "the effect must not reach the executor at all: {:?}",
        world.effects_proposed()
    );
    // The mutation has exactly one spelling — `markPullRequestReadyForReview`
    // through GraphQL — so a run that had performed it some other way is visible
    // here rather than only in the trace this same run wrote.
    assert!(
        !world
            .argvs()
            .iter()
            .any(|argv| argv.iter().any(|arg| arg == "graphql")),
        "no GraphQL call may be made before there is an answer: {:?}",
        world.argvs()
    );
}

/// The hybrid half: the change comes from a bounded attempt, and the commit that
/// is published is the one the attempt produced.
///
/// Read out of the remote, so this is a claim about what a reviewer would open. It
/// is also what makes the derived worktree path load-bearing rather than
/// incidental: the branch effect pushes `HEAD` out of the context's tree, so a
/// capability whose attempt worked anywhere else would publish the fixture's own
/// commit — which the second assertion below rules out.
#[tokio::test]
async fn the_published_commit_is_what_the_attempt_left_behind() {
    let world = World::fresh();
    let (_, _, _) = run(&world, repairs(), the_projects_own_check()).await;

    assert_eq!(
        world.published_file("src/lib.rs").as_deref(),
        Some(fixture::REPAIRED),
        "the published tree carries the file the attempt wrote"
    );
    assert_ne!(
        world.published_sha(),
        world.git_says(&world.fixture, &["rev-parse", "HEAD"]),
        "and it is a new commit, not the tree the attempt started from"
    );
    // The fixture itself is untouched: the attempt lived and died in a worktree.
    assert_eq!(fixture::changed_files(&world.fixture), Vec::<String>::new());
}

/// M1's rule, unmoved: the capability's outcome is decided by the check it runs
/// itself, over the tree the attempt actually left. A model's claim is evidence
/// beside the exit code that overruled it, never instead of it.
#[tokio::test]
async fn an_attempt_whose_check_failed_publishes_nothing_and_asks_nothing() {
    let world = World::fresh();
    let (outcome, _, publication) = run(&world, claims_success(), the_projects_own_check()).await;

    let error = outcome.expect_err("a failing check earns nothing");
    assert!(
        !matches!(error, CapabilityError::AwaitingDecision { .. }),
        "must not ask about a failure: {error:?}"
    );
    match &error {
        CapabilityError::CheckFailed {
            claimed, exit_code, ..
        } => {
            assert!(
                *claimed,
                "the claim is carried as evidence, so it must be recorded"
            );
            assert_ne!(*exit_code, 0, "the check is what decided this");
        }
        other => panic!("a failing check must be reported as such, got {other:?}"),
    }
    assert_eq!(error.recurrence(), Recurrence::Correctable);

    assert_eq!(world.effects_performed(), []);
    assert_eq!(
        world.effects_proposed(),
        [],
        "nothing was even proposed: {:?}",
        world.steps()
    );
    assert_eq!(world.branches(), Vec::<String>::new());
    assert_eq!(world.posted_comments(), Vec::<String>::new());
    // And the run still says what it saw of the forge, which is nothing.
    let review = publication
        .expect("a publication is reported on every arm")
        .review;
    assert!(
        matches!(review, Observation::Unavailable { .. }),
        "an unpublished run has read no forge and must not claim to: {review:?}"
    );
}

/// A passing check over a tree nobody changed is not a change to propose.
///
/// The one arm where the check and the model agree and there is still nothing to
/// do. Publishing an empty commit would ask a person to approve a change that
/// does not exist.
#[tokio::test]
async fn an_attempt_that_changed_nothing_publishes_nothing_and_asks_nothing() {
    let world = World::fresh();
    let (outcome, _, _) = run(&world, claims_success(), a_check_that_always_passes()).await;

    let error = outcome.expect_err("there is nothing to propose");
    assert!(
        matches!(error, CapabilityError::NothingProposed),
        "got {error:?}"
    );
    assert_eq!(
        error.recurrence(),
        Recurrence::Correctable,
        "a later attempt may still produce something"
    );
    assert_eq!(world.effects_performed(), []);
    assert_eq!(world.branches(), Vec::<String>::new());
    assert_eq!(world.posted_comments(), Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// What the capability holds, and what it cannot do
// ---------------------------------------------------------------------------

/// It holds no credential and constructs no client — it receives an executor
/// already bound to its own id, and step 1 refuses a proposal under any other.
#[tokio::test]
async fn the_capability_cannot_propose_under_another_capabilitys_name() {
    let world = World::fresh();
    let (outcome, _, _) = run_with(
        &world,
        MockCompletionModel::new(repairs()),
        the_projects_own_check(),
        PUBLISH_CHANGE,
        None,
    )
    .await;

    let error = outcome.expect_err("a proposal under another name is refused");
    assert!(
        error.to_string().contains("cannot propose for"),
        "got {error}"
    );
    assert_eq!(
        world.effects_performed(),
        [],
        "a refusal at step 1 reaches nothing"
    );
    assert_eq!(world.branches(), Vec::<String>::new());
    assert_eq!(world.posted_comments(), Vec::<String>::new());
}

/// A context that publishes from anywhere other than the tree this run's attempt
/// works in is refused before anything is read or written.
///
/// The hazard is specific: `EnsureBranchPublished` pushes `HEAD` out of the
/// context's worktree, so the two disagreeing means publishing a commit this run
/// never made — with a payload naming the commit it *did* make, and a
/// postcondition read that then disagrees with both. Checked rather than assumed,
/// and checked before the attempt, which is why the assertions below are that
/// nothing at all happened.
#[tokio::test]
async fn a_context_publishing_from_another_tree_is_refused_before_anything_runs() {
    let world = World::fresh();
    let elsewhere = world.dir.path().join("somewhere-else");
    let (outcome, _, _) = run_with(
        &world,
        MockCompletionModel::new(repairs()),
        the_projects_own_check(),
        PROPOSE_CHANGE,
        Some(elsewhere.clone()),
    )
    .await;

    let error = outcome.expect_err("the two trees have to be one tree");
    assert!(
        matches!(error, CapabilityError::PublishesElsewhere { .. }),
        "got {error:?}"
    );
    assert_eq!(error.recurrence(), Recurrence::Permanent);
    assert!(
        error.to_string().contains("somewhere-else"),
        "the diagnostic names the tree it was pointed at: {error}"
    );
    assert!(
        !world.workspace_root().exists(),
        "a refused run must not even prepare a workspace"
    );
    assert!(
        world.requests().is_empty(),
        "and must not read the forge: {:?}",
        world.argvs()
    );
}

/// The capability names no credential and constructs no client.
///
/// A source-level assertion because that is the level the property is at: the type
/// system cannot express "names no secret", and the alternative — asserting that
/// no token *reached* GitHub — would pass on a capability that held one and
/// happened not to use it this time. `publish_change` is asserted the same way in
/// `effect_protocol.rs`, and this is the same list.
///
/// The last pair is this capability's own: it writes **no change set**, on any
/// path. A correlation marker says *this invocation accounts for this work*, and
/// the next invocation completes on it without executing — so a marker written by
/// a run that is waiting for an answer would stop the very process that was
/// supposed to read one.
#[test]
fn the_capability_holds_no_credential_and_accounts_for_no_work() {
    let source = include_str!("../src/capability/propose.rs");
    for named in ["GH_TOKEN", "FIDDLE_GITHUB_TOKEN", "token"] {
        assert!(
            !source.contains(named),
            "the capability names no credential, and it names `{named}`"
        );
    }
    for constructed in ["GhCli", "GitCli", "EffectContext::new"] {
        assert!(
            !source.contains(constructed),
            "the capability constructs no client, and it constructs `{constructed}`"
        );
    }
    for written in ["ChangeSetState", "write_atomically", "correlation_key"] {
        assert!(
            !source.contains(written),
            "a suspended run accounts for no work, and this names `{written}`"
        );
    }
}

// ---------------------------------------------------------------------------
// What a suspended run leaves behind
// ---------------------------------------------------------------------------

/// Receipts and publication survive the suspending arm, which is M1's and M2's
/// rule and is exactly what an operator needs from a run that stopped part-way.
#[tokio::test]
async fn a_suspended_run_still_reports_what_it_did_reach() {
    let world = World::fresh();
    let (_, receipts, publication) = run(&world, repairs(), the_projects_own_check()).await;

    let publication = publication.expect("a publication is reported on every arm");
    match &publication.review {
        Observation::Available {
            value, revision, ..
        } => {
            assert_eq!(value.pull_request, Some(PR));
            assert_eq!(value.branch.as_deref(), Some(world.branch().as_str()));
            assert_eq!(value.state.as_deref(), Some("open"));
            assert_eq!(
                revision.as_deref(),
                Some(world.published_sha().as_str()),
                "the revision is the one the remote was observed to hold"
            );
        }
        other => panic!("a published run describes its review, got {other:?}"),
    }
    assert!(
        matches!(publication.verification, Observation::NotApplicable { .. }),
        "this capability requests no check, so it makes no claim about CI: {:?}",
        publication.verification
    );

    // The tool summary, the attempt's own reference, and one per effect.
    let rendered: Vec<&str> = receipts.iter().map(|entry| entry.0.as_str()).collect();
    assert!(receipts.len() >= 3, "{rendered:?}");
    assert!(
        rendered.contains(&"tools:2"),
        "an attempt's tool calls are counted even when nothing went wrong: {rendered:?}"
    );
    assert!(
        rendered
            .iter()
            .any(|entry| *entry == format!("propose:1:{ATTEMPT}")),
        "the evidence names what git saw change and the attempt it was granted: {rendered:?}"
    );
    let kinds: Vec<&str> = rendered
        .iter()
        .filter(|entry| entry.starts_with("effect:"))
        .map(|entry| entry.split(':').nth(1).unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "ensure_branch_published",
            "ensure_pull_request",
            "publish_decision_request"
        ],
        "{rendered:?}"
    );
}

/// The question a person reads and the question this run says it is waiting on
/// are the same question.
///
/// **This is the post-forever hazard, stated from the outside.**
/// Only `binding.request` is rendered into the marker, so it is the only id a later
/// process can find the question by. A producer that derived the id twice, or a
/// consumer that looked the comment up by anything the marker does not carry, would
/// publish a marker naming one question and then look for another: it would find
/// nothing, conclude it had not asked yet, and post again on every attempt forever.
/// So the marker is parsed back out of the comment the world really received and
/// required to name the id the run is waiting on — and every field of it is
/// recomputed here from canonical inputs, so this cannot pass on a build that
/// invented an identity and then wrote it down consistently.
///
/// `HumanDecisionRequest` also carried the id a second time, as its own field, which
/// is what made this a hazard rather than a convention; `fiddle-11vj` deleted that
/// field. The half of the hazard this case still watches is the one deletion cannot
/// reach: two *derivations* of the id, here and in the fresh process below.
///
/// **Why agreement is the assertion.** A single derivation has no direct
/// observable — nothing outside the capability can see how many times an id was
/// computed. What it *does* have is a consequence: two values built from one
/// derivation cannot disagree, and two built from two derivations have no reason
/// to agree. So the only honest way to test "the id came from one place" is to
/// take the two places it surfaces — the marker on the conversation and the
/// request the error names — and require them to be the same string. When the
/// duplicated field still existed, twenty-four tests passed over it because every
/// one of them built its two ids agreeing; this one takes them from the world
/// instead.
#[tokio::test]
async fn the_suspended_run_waits_on_the_question_the_comment_carries() {
    let world = World::fresh();
    let (outcome, _, _) = run(&world, repairs(), the_projects_own_check()).await;

    let (request, effect, payload) = identity_at(&world.published_sha());
    let (waiting_on, interaction, question) = match outcome {
        Err(CapabilityError::AwaitingDecision {
            request,
            interaction,
            question,
        }) => (request, interaction, question),
        other => panic!("a first run suspends, got {other:?}"),
    };
    assert_eq!(waiting_on, request);

    let comments = world.posted_comments();
    assert_eq!(comments.len(), 1, "{comments:?}");
    let binding = parse_marker(&comments[0]).expect("the comment carries a marker");
    assert_eq!(
        binding.request, waiting_on,
        "the marker names this question"
    );
    assert_eq!(
        binding.effect, effect,
        "and the effect an approval would gate"
    );
    assert_eq!(binding.payload, payload, "and the payload it was shown");
    assert_eq!(binding.head_sha, world.published_sha());

    // The question a person reads is a question, and the body carries it.
    assert!(comments[0].contains(&question), "{}", comments[0]);
    assert!(
        comments[0].contains("ready for review?"),
        "yes and no both have to mean something: {}",
        comments[0]
    );
    // And the conversation the run names is the comment that was created.
    match interaction {
        InteractionRef::GitHubPullRequestComment { repo, pr, comment } => {
            assert_eq!(repo, REPO);
            assert_eq!(pr, PR);
            assert_ne!(comment, 0, "a comment id nobody sent names no comment");
        }
    }
}

/// A second process, with no memory of the first, finds its own question rather
/// than asking it again.
///
/// Nothing local survives between the two runs — the worktree was removed when the
/// first capability's execution ended, and the second holds a different
/// capability value with empty receipts — so the only thing the second run can
/// recognise its own work from is the world. It is given a model that would write
/// something *different*, which is what makes "it did not attempt again" an
/// assertion about the published tree rather than about an internal counter.
///
/// The conversation is given **no replies at all**, which is the case that
/// distinguishes *nobody has answered* from *somebody has*: the walk reads six of
/// its eight steps, finds no candidate reply, and the run goes on waiting without
/// a model call. The by-id route has to be scripted for fiddle's own question even
/// so, because the walk re-reads what it is about to rely on — and that re-read is
/// the reason this case reaches further than it did before the continuation
/// existed.
#[tokio::test]
async fn a_second_process_finds_its_own_question_and_does_not_ask_twice() {
    let world = World::fresh();
    let suspension = suspended(&world).await;
    let published = suspension.head_sha.clone();
    world.answered_by(suspension.comment, &[]);

    // A second capability value, with empty receipts and no worktree, over the
    // same forge — which is all a fresh process has. Its `git` and its workspace
    // are the working ones, deliberately: "the attempt did not run again" is a
    // claim about a run that *could* have run one.
    let (outcome, receipts, _) = run(&world, repairs_differently(), the_projects_own_check()).await;

    let error = outcome.expect_err("the question stands, so the run is still waiting");
    match &error {
        CapabilityError::AwaitingDecision { request, .. } => {
            assert_eq!(*request, identity_at(&published).0, "the same question");
        }
        other => panic!("a run whose question is unanswered waits, got {other:?}"),
    }
    assert_eq!(
        world.decision_steps(),
        [
            DecisionStep::RecomputeIdentity.as_str(),
            DecisionStep::FindRequest.as_str(),
            DecisionStep::ParseBinding.as_str(),
            DecisionStep::SelectCandidates.as_str(),
            DecisionStep::ReReadCandidates.as_str(),
            DecisionStep::ReObserveState.as_str(),
        ],
        "an unanswered question announces six steps and stops"
    );
    assert_eq!(
        world.posted_comments().len(),
        1,
        "no second question was posted"
    );
    assert_eq!(
        world.published_sha(),
        published,
        "and no second commit was published, so the attempt did not run again"
    );
    assert_eq!(
        world.published_file("src/lib.rs").as_deref(),
        Some(fixture::REPAIRED),
        "the tree is still the first attempt's"
    );
    assert!(
        receipts.contains(&EvidenceRef("tools:0".to_string())),
        "a continuation calls no tool, because it runs no attempt: {receipts:?}"
    );
    // The three effects of the *first* run only. The second proposed nothing: it
    // read, recognised its own question, and stopped.
    assert_eq!(
        world.effects_performed(),
        [
            EffectKind::EnsureBranchPublished,
            EffectKind::EnsurePullRequest,
            EffectKind::PublishDecisionRequest,
        ],
        "{:?}",
        world.steps()
    );
}

/// A pull request with no question on it is resumed by asking, not by attempting
/// again.
///
/// The state a process interrupted between the create and the comment leaves
/// behind, which is a third answer to "is this a first run" that neither of the
/// other two covers. A second attempt would produce a different commit, the push
/// would then be a refused non-fast-forward, and the run would be stuck for good;
/// the change is already out there, and what is missing is the question.
#[tokio::test]
async fn a_published_change_nobody_has_been_asked_about_is_resumed_by_asking() {
    let world = World::fresh();
    // Arrange the world the interrupted run left: the branch pushed and the pull
    // request open, with nothing on its conversation. Both are put there with the
    // test's own tools rather than by the code under test.
    let fixed = world.dir.path().join("fixed");
    fixture::git(
        &world.fixture,
        &[
            "worktree",
            "add",
            "--detach",
            "-q",
            &fixed.display().to_string(),
            "HEAD",
        ],
    );
    std::fs::write(fixed.join("src/lib.rs"), fixture::REPAIRED).unwrap();
    fixture::git(&fixed, &["add", "src/lib.rs"]);
    fixture::git(
        &fixed,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "the interrupted run's commit",
        ],
    );
    let published = world.git_says(&fixed, &["rev-parse", "HEAD"]);
    fixture::git(
        &fixed,
        &[
            "push",
            "-q",
            "origin",
            &format!("HEAD:refs/heads/{}", world.branch()),
        ],
    );
    std::fs::write(
        world.dir.path().join("pulls_seed"),
        json!([{
            "head": format!("{HEAD_OWNER}:{}", world.branch()),
            "base": BASE,
            "title": "opened before the interruption",
        }])
        .to_string(),
    )
    .unwrap();
    world.pull_request_at(PR, &published, true);

    let (outcome, _, _) = run(&world, repairs_differently(), the_projects_own_check()).await;

    let error = outcome.expect_err("the question is what was missing, so the run asks it");
    assert!(
        matches!(error, CapabilityError::AwaitingDecision { .. }),
        "got {error:?}"
    );
    assert_eq!(
        world.effects_performed(),
        [EffectKind::PublishDecisionRequest],
        "only the question: the change was already published: {:?}",
        world.steps()
    );
    assert_eq!(world.posted_comments().len(), 1);
    assert_eq!(
        world.published_sha(),
        published,
        "the commit that was already out there is the one the question is about"
    );
    assert_eq!(
        parse_marker(&world.posted_comments()[0])
            .expect("a marker")
            .head_sha,
        published
    );
}

// ---------------------------------------------------------------------------
// The continuation: resolve the answer, and act only on an approval
// ---------------------------------------------------------------------------

/// One continuation over the world a first run left, answered by one person.
struct Answered {
    outcome: Result<EvidenceRef, CapabilityError>,
    receipts: Vec<EvidenceRef>,
    /// The model the walk was given, so what was asked of it can be read back.
    model: MockCompletionModel,
    /// The question this run is about, as a fresh process derives it.
    request: fiddle_core::DecisionRequestId,
    /// The revision it was asked about.
    head_sha: String,
    /// How many comments this world had been asked to post *before* the
    /// continuation ran — the baseline "and it posted nothing further" is stated
    /// against.
    posted_before: usize,
}

/// Suspend a run, have `author` answer it with `reply`, and read that reply as
/// `document`.
///
/// The GraphQL answer is scripted on every path, including the three where no
/// mutation may happen: a fixture that could only answer a mutation it expected
/// would make "nothing was dispatched" indistinguishable from "the fixture had no
/// answer for it".
async fn answered(world: &World, author: u64, reply: &str, document: &str) -> Answered {
    let suspension = suspended(world).await;
    world.answered_by(suspension.comment, &[(author, reply)]);
    world.script_graphql(0, 200, readied());

    let posted_before = world.posted_comments().len();
    let model = MockCompletionModel::new([MockTurn::text(document)]);
    let (outcome, receipts, _) = continue_in(world, model.clone()).await;
    Answered {
        outcome,
        receipts,
        model,
        request: identity_at(&suspension.head_sha).0,
        head_sha: suspension.head_sha,
        posted_before,
    }
}

/// **Approve is the only decision that mutates.**
///
/// The four verdicts against one property, over four worlds rather than one,
/// because a world in which the transition has happened cannot be asked the
/// question again — which is itself the subject of
/// [`a_second_invocation_after_an_approval_completes_without_a_second_mutation`].
///
/// The mutation has exactly one spelling in this build, so the assertion is made
/// twice from two doors: the `apply` the executor announced, and the count of
/// GraphQL calls the forge was asked for. A capability that performed the
/// transition some other way would still be visible in the second.
#[tokio::test]
async fn only_an_approval_marks_the_pull_request_ready() {
    for (reply, document, should_mutate) in [
        (YES, APPROVES, true),
        ("no, drop it", REJECTS, false),
        ("do it differently", REDIRECTS, false),
        ("what does this do?", UNCLEAR, false),
    ] {
        let world = World::fresh();
        let answered = answered(&world, APPROVER, reply, document).await;

        assert_eq!(
            world
                .effects_performed()
                .contains(&EffectKind::EnsurePullRequestReady),
            should_mutate,
            "{reply:?} performed {:?}",
            world.effects_performed()
        );
        assert_eq!(
            world.graphql_calls(),
            usize::from(should_mutate),
            "{reply:?} asked the forge for {} GraphQL calls",
            world.graphql_calls()
        );
        assert_eq!(
            answered.outcome.is_ok(),
            should_mutate,
            "{reply:?} produced {:?}",
            answered.outcome
        );
        // Every one of them read the reply, which is what makes the differences
        // above differences of verdict rather than of whether anybody looked.
        assert_eq!(answered.model.requests().len(), 1, "{reply:?}");
    }
}

/// The transition goes through the executor's **decided** entry point, and that is
/// observable rather than inferred.
///
/// `ExecutionStep::ResolveDecision` is announced in exactly one place — inside the
/// arm of step 4 that has a [`ResolvedDecision`] in hand — so a capability that
/// had called [`Executor::execute`] instead would have been refused with
/// `HumanDecisionRequired` and this step would never appear. It is the difference
/// between *this milestone has a decided path* and *this milestone has a decided
/// path something walks*, which is the criticism M2's `RequireHumanDecision` drew.
///
/// The whole four-effect order is asserted, across the two processes: three
/// automatic effects from the run that asked, and the one `Human` effect from the
/// run that read the answer.
#[tokio::test]
async fn the_transition_is_performed_through_the_decided_entry_point() {
    let world = World::fresh();
    let answered = answered(&world, APPROVER, YES, APPROVES).await;

    let evidence = answered
        .outcome
        .expect("an approved transition earns evidence");
    assert!(
        evidence.0.starts_with("effect:ensure_pull_request_ready:"),
        "the run's evidence is the transition it performed: {evidence:?}"
    );
    assert!(
        evidence.0.contains(":committed:"),
        "and the postcondition was read back: {evidence:?}"
    );

    assert!(
        world.steps().contains(&(
            EffectKind::EnsurePullRequestReady,
            ExecutionStep::ResolveDecision.as_str()
        )),
        "the gated effect was authorized by a decision, which only \
         `execute_decided` can announce: {:?}",
        world.steps()
    );
    assert_eq!(
        world.effects_performed(),
        [
            EffectKind::EnsureBranchPublished,
            EffectKind::EnsurePullRequest,
            EffectKind::PublishDecisionRequest,
            EffectKind::EnsurePullRequestReady,
        ],
        "{:?}",
        world.steps()
    );
    // And the receipt reached the bundle beside the first run's, rather than only
    // the return value.
    let kinds: Vec<&str> = answered
        .receipts
        .iter()
        .filter(|entry| entry.0.starts_with("effect:"))
        .map(|entry| entry.0.split(':').nth(1).unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["ensure_pull_request_ready"],
        "a continuation's receipts are its own: {:?}",
        answered.receipts
    );
}

/// A rejection is a conclusion, not a wait and not a failure to retry.
///
/// Exit 20, which is what `Recurrence::Permanent` means: repeating the invocation
/// re-derives the same verdict, because the same person's same comment is still
/// the last authorized reply. Inviting a retry (11) would ask again; suspending
/// (10) would leave a run waiting for an answer it has already been given.
#[tokio::test]
async fn a_rejection_concludes_the_run_rather_than_suspending_it_again() {
    let world = World::fresh();
    let answered = answered(&world, APPROVER, "no, drop it", REJECTS).await;

    let error = answered.outcome.expect_err("a refusal earns nothing");
    match &error {
        CapabilityError::DecisionRejected { request, reason } => {
            assert_eq!(*request, answered.request, "the question that was refused");
            assert!(
                reason.as_str().contains("drop it"),
                "the reason is the person's own words: {reason}"
            );
        }
        other => panic!("a refusal is reported as one, got {other:?}"),
    }
    assert_eq!(error.recurrence(), Recurrence::Permanent);
    assert!(
        !matches!(error, CapabilityError::AwaitingDecision { .. }),
        "a run that has its answer is not waiting for one"
    );
    assert_eq!(
        world.posted_comments().len(),
        answered.posted_before,
        "and nothing further was said out there"
    );
    assert_eq!(world.graphql_calls(), 0);
}

/// An unclear reply waits on the *same* request and posts nothing at all.
///
/// Publishing a follow-up comment is deliberately not done, and the reason is a
/// property rather than a preference: the effect has not moved, so the request
/// identity has not moved, so `PublishDecisionRequest::inspect` finds the existing
/// marker and correctly suppresses a second post. Making a follow-up possible
/// would mean a second identity for the same question — an effect kind M3 does not
/// have — and inventing one to send a courtesy message is not worth a new external
/// mutation. Recorded in the design's §8 as a known gap.
///
/// Both halves are asserted, because they are different claims: that exactly one
/// comment names this question, and that **this run posted nothing** — the first
/// would also hold of a run that posted a second comment naming a *different*
/// question.
#[tokio::test]
async fn an_unclear_reply_waits_on_the_same_request_and_posts_nothing_further() {
    let world = World::fresh();
    let answered = answered(&world, APPROVER, "what does this do?", UNCLEAR).await;

    let error = answered
        .outcome
        .expect_err("an unread answer is not evidence");
    match &error {
        CapabilityError::AwaitingDecision { request, .. } => {
            assert_eq!(
                *request, answered.request,
                "the same question, not a new one"
            );
        }
        other => panic!("an unclear reply leaves the run waiting, got {other:?}"),
    }
    assert_eq!(error.recurrence(), Recurrence::Awaiting);
    assert!(
        error.to_string().contains("could not be read as"),
        "the diagnostic says why the run is still waiting: {error}"
    );
    assert_eq!(
        world.comments_naming(&answered.request).len(),
        1,
        "no second question"
    );
    assert_eq!(
        world.posted_comments().len(),
        answered.posted_before,
        "an unclear reply posts nothing at all"
    );
    assert_eq!(world.graphql_calls(), 0);
}

/// A redirect changes nothing out there, and says what it was told.
///
/// Attempting the change again is Task 15's; until then the honest outcome is that
/// the run is waiting, with the instruction in the diagnostic so an operator can
/// see what was asked for rather than only that something was.
#[tokio::test]
async fn a_redirect_waits_and_names_the_instruction_it_received() {
    let world = World::fresh();
    let answered = answered(&world, APPROVER, "do it differently", REDIRECTS).await;

    let error = answered.outcome.expect_err("a redirect earns nothing yet");
    assert!(
        matches!(error, CapabilityError::AwaitingDecision { .. }),
        "got {error:?}"
    );
    assert_eq!(error.recurrence(), Recurrence::Awaiting);
    assert!(
        error.to_string().contains("use a bounded loop instead"),
        "the instruction is named: {error}"
    );
    assert_eq!(
        world.posted_comments().len(),
        answered.posted_before,
        "a redirect mutates nothing, including the conversation"
    );
    assert_eq!(world.graphql_calls(), 0);
}

/// **A continuation needs no worktree, and this one could not have had one.**
///
/// The approve path publishes nothing through git, so a process that cannot create
/// a workspace at all still completes it — which is the property that makes the
/// deleted-workspace lane in Task 13 possible.
///
/// # Three witnesses rather than a count, and why a count is not available here
///
/// A program seam does exist: `Workspace::run` spawns
/// `Command::new(&cmd.program)`, which is how the scripted `gh` and the recording
/// `git_stub` are reached elsewhere in this crate. What is missing is an
/// *interception point a fixture can reach*. `Workspace` is a concrete struct with
/// no trait over its runner, and `ProposeChange` takes `&Workspace` concretely, so
/// there is no implementation to substitute; the one call site that goes through
/// the seam hardcodes `program: "git"`; and `Workspace::create` and
/// `changed_files` do not go through it at all, spawning `Command::new("git")`
/// directly. So a count is unavailable to this test rather than absent from the
/// product, and making it available is a port question — a trait on the workspace
/// runner — rather than a change to this suite.
///
/// What is asserted instead makes an invocation impossible or loud:
///
/// - the workspace root is a **file**, so `Workspace::create` could not have
///   produced a worktree under it, and this run reports success anyway;
/// - the `git` the adapter would push with is `/nonexistent/git`, so any push
///   would have failed loudly rather than silently succeeding;
/// - the remote still holds exactly the branch and the commit the *first* run
///   published, so nothing was pushed to it a second time.
#[tokio::test]
async fn the_approve_path_invokes_git_not_at_all() {
    let world = World::fresh();
    let answered = answered(&world, APPROVER, YES, APPROVES).await;

    answered
        .outcome
        .expect("a continuation with no workspace still completes");
    assert!(
        !world.workspace_root().is_dir(),
        "no worktree could be created under a file, and none was needed"
    );
    assert_eq!(world.branches(), [world.branch()]);
    assert_eq!(
        world.published_sha(),
        answered.head_sha,
        "the remote is where the first run left it"
    );
    assert!(
        answered
            .receipts
            .contains(&EvidenceRef("tools:0".to_string())),
        "a continuation calls no tool, because it runs no attempt: {:?}",
        answered.receipts
    );
    assert_eq!(world.graphql_calls(), 1, "one approval, one mutation");
}

/// The validation order is the shell's, and the capability neither re-implements
/// nor partially bypasses it.
///
/// An unauthorized reply reaches no model *here* either — not because this
/// capability checks an allowlist, but because `validate::resolve` owns that check
/// and the capability calls it. The evidence is the order the walk announced: six
/// deterministic steps and no `interpret`, from the trace the shared walk writes
/// to rather than from anything this capability could have written itself.
#[tokio::test]
async fn the_capability_delegates_the_whole_validation_order() {
    let world = World::fresh();
    let answered = answered(&world, STRANGER, "approve", APPROVES).await;

    let error = answered
        .outcome
        .expect_err("nobody who may decide has decided");
    match &error {
        CapabilityError::AwaitingDecision { request, .. } => {
            assert_eq!(*request, answered.request);
        }
        other => panic!("an unauthorized reply leaves the run waiting, got {other:?}"),
    }
    assert_eq!(
        answered.model.requests().len(),
        0,
        "a reply nobody authorized must not cost a model call"
    );
    assert_eq!(
        world.decision_steps(),
        [
            DecisionStep::RecomputeIdentity.as_str(),
            DecisionStep::FindRequest.as_str(),
            DecisionStep::ParseBinding.as_str(),
            DecisionStep::SelectCandidates.as_str(),
            DecisionStep::ReReadCandidates.as_str(),
            DecisionStep::ReObserveState.as_str(),
        ],
        "the whole deterministic order ran, and stopped where there was nothing \
         to interpret"
    );
    assert!(
        !world
            .effects_performed()
            .contains(&EffectKind::EnsurePullRequestReady),
        "{:?}",
        world.effects_performed()
    );
    assert_eq!(world.graphql_calls(), 0);
}

/// **The two payload comparisons, in one walk, with only the second able to see
/// the disagreement.**
///
/// `resolve` compares the marker's digest against the payload *this run rebuilds*;
/// `execute_decided` compares the **approval's** digest against the payload the
/// proposal carries. Between the two there is a gap nothing else covers: a
/// proposal widened after the walk passed. The identity cannot catch it, because
/// the identity is derived over the target and deliberately never over the
/// payload — so the widened request arrives looking like the same work.
///
/// The positive control runs second and against the same world, which is what
/// makes the refusal attributable to the widening rather than to anything else
/// about this fixture: the same decision, the same operation and the same forge
/// commit when the payload is the one the person was shown.
#[tokio::test]
async fn the_second_payload_comparison_catches_what_the_first_could_not_see() {
    let world = World::fresh();
    let suspension = suspended(&world).await;
    world.answered_by(suspension.comment, &[(APPROVER, YES)]);
    world.script_graphql(0, 200, readied());

    // Step 8's comparison: the walk rebuilds the operation and agrees with the
    // marker, so it resolves an approval.
    let ctx = world.ctx_without_git();
    let ready = EnsurePullRequestReady::new(REPO.to_string(), PR, suspension.head_sha.clone());
    let target = ready.target();
    let payload = ready.payload();
    let resolution = resolve(
        &ctx,
        &walk_at(&target, &payload, &[APPROVER]),
        QUESTION,
        MockCompletionModel::new([MockTurn::text(APPROVES)]),
        &patient_interpretation(),
        &world,
    )
    .await
    .expect("the walk resolves");
    let answer = resolution.answer.expect("an authorized approval");
    let (request, effect, digest) = identity_at(&suspension.head_sha);
    let decision = ResolvedDecision::approved(
        DecisionBinding {
            request,
            effect,
            payload: digest,
            head_sha: suspension.head_sha.clone(),
        },
        &answer.interpreted,
    )
    .expect("an approval is the one verdict that converts");

    let deployment = Deployment(DeploymentRule::Allow);
    let executor = Executor::new(
        PROPOSE_CHANGE,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        &world,
        ReadRetry::none(),
    );

    // Step 4's comparison: the proposal now asks for one thing more than the
    // question carried, and the approval is not addressed to it.
    let refused = executor
        .execute_decided(
            proposal(&target, &widened(&payload)),
            EnsurePullRequestReady::new(REPO.to_string(), PR, suspension.head_sha.clone()),
            &decision,
        )
        .await
        .expect_err("an approval given for another request buys nothing");
    match &refused {
        EffectError::PayloadDiverged { approved, .. } => {
            assert_eq!(
                approved,
                &decision.binding().payload,
                "the digest the person was shown is the one that refused it"
            );
        }
        other => panic!("a widened proposal diverges, got {other:?}"),
    }
    assert_eq!(
        refused.recurrence(),
        Recurrence::Permanent,
        "nothing here a repeat gets past"
    );
    assert_eq!(
        world.graphql_calls(),
        0,
        "and it was refused before the mutation"
    );
    assert!(
        !world.steps().contains(&(
            EffectKind::EnsurePullRequestReady,
            ExecutionStep::Authorize.as_str()
        )),
        "refused at step 4 by the decision, not at step 6 by the envelope: {:?}",
        world.steps()
    );

    // The control: the same approval, the same operation, the payload the marker
    // named.
    let receipt = executor
        .execute_decided(
            proposal(&target, &payload),
            EnsurePullRequestReady::new(REPO.to_string(), PR, suspension.head_sha.clone()),
            &decision,
        )
        .await
        .expect("the request that was approved commits");
    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(world.graphql_calls(), 1, "one approval, one mutation");
}

/// A repeat of an invocation whose transition already landed completes, and buys
/// no second mutation.
///
/// This capability records no change set on any path, so nothing stops a later
/// invocation of the same run from walking the whole thing again: it finds its own
/// pull request, derives the same question, finds the same comment — and the
/// validation order then refuses, because the pull request it was asked about is
/// no longer a draft. The honest answer at that point is not a failure. The gated
/// effect's postcondition holds, and the executor's step 3 is what says so: it
/// inspects before it combines policy, so an already-ready pull request settles
/// there and no decision is needed to *observe* a completed effect. `ready.rs`
/// documents that ordering, and `an_already_ready_pull_request_is_the_postcondition`
/// pins it.
///
/// So the run completes on the transition that really happened, and the assertion
/// that it performed nothing is the GraphQL count: still one, from the invocation
/// that had the approval.
#[tokio::test]
async fn a_second_invocation_after_an_approval_completes_without_a_second_mutation() {
    let world = World::fresh();
    let first = answered(&world, APPROVER, YES, APPROVES).await;
    first.outcome.expect("the approved transition landed");

    let model = MockCompletionModel::new([MockTurn::text(APPROVES)]);
    let (outcome, receipts, _) = continue_in(&world, model.clone()).await;

    let evidence = outcome.expect("the transition this run was about has happened");
    assert!(
        evidence.0.contains("ensure_pull_request_ready") && evidence.0.contains(":committed:"),
        "it completes on the effect the world already satisfies: {evidence:?}"
    );
    assert_eq!(
        world.graphql_calls(),
        1,
        "the mutation was dispatched once, by the invocation that had the approval"
    );
    assert!(
        !world
            .effects_performed()
            .iter()
            .skip(4)
            .any(|kind| *kind == EffectKind::EnsurePullRequestReady),
        "and nothing was applied a second time: {:?}",
        world.effects_performed()
    );
    assert_eq!(
        model.requests().len(),
        0,
        "no reply was interpreted: the walk refused before the model, because the \
         state it re-observed had moved"
    );
    assert!(
        receipts.contains(&EvidenceRef("tools:0".to_string())),
        "{receipts:?}"
    );
}

/// One proposal of the gated effect, under this capability's own name.
fn proposal(target: &str, payload: &str) -> ProposedEffect {
    ProposedEffect {
        capability: PROPOSE_CHANGE,
        kind: EffectKind::EnsurePullRequestReady,
        target: target.to_string(),
        payload: payload.to_string(),
    }
}

/// The gated effect's payload with one more key in it than the question carried.
///
/// A *widening* and not a corruption: still well formed, still naming the same
/// pull request at the same revision, and asking for one thing more than the
/// person was shown. That is the disagreement the identity cannot see, which is
/// why it is the one worth testing.
fn widened(payload: &str) -> String {
    let mut asked: serde_json::Map<String, Value> =
        serde_json::from_str(payload).expect("the payload is an object");
    asked.insert("merge".to_string(), json!(true));
    Value::Object(asked).to_string()
}
