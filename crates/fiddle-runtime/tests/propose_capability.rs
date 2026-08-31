mod fixture;
mod support;

use fiddle_core::{
    effect_id, parse_marker, payload_hash, DecisionBinding, DeploymentRule, EffectName,
    EvidenceRef, NextAction, Observation, ProposedEffect, WorkItemState, ENSURE_BRANCH_PUBLISHED,
    ENSURE_PULL_REQUEST, ENSURE_PULL_REQUEST_READY, JIRA_COMMENT_ADDED, PROPOSE_CHANGE,
    PUBLISH_CHANGE, PUBLISH_DECISION_REQUEST,
};
use fiddle_runtime::agent::AgentBudget;
use fiddle_runtime::capability::{
    attempt_worktree, Capability, CapabilityError, Executed, ExecutionGrant, ExecutionInput,
    ProposeChange, ProposeConfig,
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
use fiddle_runtime::Redaction;
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use support::stub_jira::{client_for, StubJira};
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const REPO: &str = "acme/r";

const HEAD_OWNER: &str = "acme";

const BASE: &str = "main";

const WORK_ID: &str = "w-1";

const ATTEMPT: &str = "01JQZX0000000000000000000";

const PR: u64 = 7;

const ISSUE: &str = "IDENT-1";

const JIRA_INVOCATION: &str = "jira:IDENT-1";

const PATIENT: Duration = Duration::from_secs(180);

const APPROVER: u64 = 505_401;

const STRANGER: u64 = 999_999;

const FIDDLE_BOT: u64 = 1_000_001;

const QUESTION: &str = "May fiddle mark pull request acme/r#7 ready for review?";

const YES: &str = "yes, go ahead";

const APPROVES: &str = r#"{"decision":"approve","redirect":null,"evidence":"go ahead"}"#;
const REJECTS: &str = r#"{"decision":"reject","redirect":null,"evidence":"drop it"}"#;
const REDIRECTS: &str = r#"{"decision":"redirect","redirect":"use a bounded loop instead","evidence":"do it differently"}"#;
const UNCLEAR: &str = r#"{"decision":"unclear","redirect":null,"evidence":"what does this do"}"#;

fn patient_interpretation() -> InterpretationBounds {
    InterpretationBounds {
        max_reply_bytes: 4_096,
        max_tokens: 256,
        deadline: Duration::from_secs(30),
    }
}

fn readied() -> Value {
    json!({"data": {"markPullRequestReadyForReview": {"pullRequest": {"isDraft": false}}}})
}

struct World {
    dir: TempDir,
    remote: PathBuf,
    fixture: PathBuf,
    steps: Mutex<Vec<(EffectName, &'static str)>>,
    decisions: Mutex<Vec<&'static str>>,
    jira: Option<StubJira>,
}

impl EffectTrace for World {
    fn step(&self, kind: &EffectName, step: ExecutionStep) {
        self.steps
            .lock()
            .unwrap()
            .push((kind.clone(), step.as_str()));
    }
}

impl DecisionTrace for World {
    fn step(&self, step: DecisionStep) {
        self.decisions.lock().unwrap().push(step.as_str());
    }
}

impl World {
    fn fresh() -> Self {
        let dir = TempDir::new().unwrap();
        let remote = dir.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        fixture::git(&remote, &["init", "-q", "--bare", "."]);
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
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
            jira: None,
        }
    }

    async fn reachable_jira_site() -> Self {
        let jira = StubJira::start().await;
        jira.holds_issue_labelled(ISSUE, &[]).await;
        World {
            jira: Some(jira),
            ..World::fresh()
        }
    }

    fn jira(&self) -> &StubJira {
        self.jira
            .as_ref()
            .expect("this world was built with a jira site")
    }

    async fn jira_comments_on(&self, issue: &str) -> usize {
        self.jira().comment_requests_on(issue).await
    }

    async fn revision_the_site_holds(&self) -> String {
        self.jira().get_issue(ISSUE).await.body["fields"]["updated"]
            .as_str()
            .expect("the stub holds a `fields.updated`")
            .to_string()
    }

    fn workspace_root(&self) -> PathBuf {
        self.dir.path().join("workspaces")
    }

    fn stub_root(&self) -> PathBuf {
        self.dir.path().join("stub-state")
    }

    fn marker(&self, work_id: &str) -> Option<String> {
        let path = self.stub_root().join(format!("changes/{work_id}.json"));
        let text = std::fs::read_to_string(path).ok()?;
        let value: Value = serde_json::from_str(&text).unwrap();
        value["marker"].as_str().map(str::to_string)
    }

    fn change_sets(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.stub_root().join("changes"))
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn work(&self) -> PathBuf {
        self.work_for(INVOCATION_REF)
    }

    fn work_for(&self, invocation_ref: &str) -> PathBuf {
        attempt_worktree(&self.workspace_root(), PROJECT, invocation_ref)
    }

    fn ctx(&self) -> EffectContext {
        self.ctx_publishing_from(self.work())
    }

    fn ctx_publishing_from(&self, work: PathBuf) -> EffectContext {
        self.ctx_with(
            work,
            GitCli::new(
                PathBuf::from("git"),
                "ghp_never_used_by_a_path_remote".to_string(),
                "FIDDLE_GITHUB_TOKEN",
                PATIENT,
            ),
        )
    }

    fn ctx_without_git(&self) -> EffectContext {
        self.ctx_with(self.work(), unreachable_git())
    }

    fn ctx_with(&self, work: PathBuf, git: GitCli) -> EffectContext {
        let ctx = self.ctx_reaching_github_only(work, git);
        match &self.jira {
            Some(site) => ctx.with_jira(client_for(site)),
            None => ctx,
        }
    }

    fn ctx_reaching_github_only(&self, work: PathBuf, git: GitCli) -> EffectContext {
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
            git,
            work,
            CancellationToken::new(),
        )
    }

    fn branch(&self) -> String {
        branch_name(PROJECT, INVOCATION_REF)
    }

    fn branches(&self) -> Vec<String> {
        self.git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    fn published_sha(&self) -> String {
        self.git_says(
            &self.remote,
            &["rev-parse", &format!("refs/heads/{}", self.branch())],
        )
    }

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

    fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        std::process::Command::new("git")
            .args(["merge-base", "--is-ancestor", ancestor, descendant])
            .current_dir(&self.remote)
            .status()
            .unwrap()
            .success()
    }

    fn git_says(&self, dir: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

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

    fn pull_request_creates(&self) -> Vec<serde_json::Value> {
        self.posts_to("/pulls")
    }

    fn posted_comments(&self) -> Vec<String> {
        self.posts_to("/comments")
            .iter()
            .map(|body| body["body"].as_str().unwrap_or_default().to_string())
            .collect()
    }

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
            "user": {"login": "fiddle[bot]", "id": FIDDLE_BOT, "type": "Bot"},
            "performed_via_github_app": Value::Null,
        }));
        ids
    }

    fn by_id(&self, comment: &Value) {
        let dir = self.dir.path().join("issue-comments/by-id");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{}.json", comment["id"].as_u64().unwrap())),
            comment.to_string(),
        )
        .unwrap();
    }

    fn script_graphql(&self, n: usize, status: u16, body: Value) {
        let dir = self.dir.path().join("graphql");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{n}.json")),
            json!({"status": status, "body": body}).to_string(),
        )
        .unwrap();
    }

    fn graphql_calls(&self) -> usize {
        std::fs::read_to_string(self.dir.path().join("graphql_calls"))
            .ok()
            .and_then(|count| count.trim().parse().ok())
            .unwrap_or(0)
    }

    fn with_no_workspace_available(&self) {
        let root = self.workspace_root();
        let _ = std::fs::remove_dir_all(&root);
        std::fs::write(&root, b"not a directory").unwrap();
    }

    fn effects_performed(&self) -> Vec<EffectName> {
        self.steps_matching(ExecutionStep::Apply)
    }

    fn effects_proposed(&self) -> Vec<EffectName> {
        self.steps_matching(ExecutionStep::ValidateCapability)
    }

    fn steps_matching(&self, step: ExecutionStep) -> Vec<EffectName> {
        self.steps
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, entered)| *entered == step.as_str())
            .map(|(kind, _)| kind.clone())
            .collect()
    }

    fn steps(&self) -> Vec<(EffectName, &'static str)> {
        self.steps.lock().unwrap().clone()
    }

    fn decision_steps(&self) -> Vec<&'static str> {
        self.decisions.lock().unwrap().clone()
    }

    fn comments_naming(&self, request: &fiddle_core::DecisionRequestId) -> Vec<String> {
        self.posted_comments()
            .into_iter()
            .filter(|body| parse_marker(body).is_ok_and(|binding| &binding.request == request))
            .collect()
    }
}

const POSTED_AT: &str = "2026-08-11T00:00:00Z";

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
        stub_root: world.stub_root(),
        check,
        commands: std::sync::Arc::new(Vec::new()),
        command_timeout: PATIENT,
        budget: AgentBudget {
            max_turns: 8,
            max_tokens: 4096,
            deadline: Duration::from_secs(300),
            max_changed_files: 16,
            tool_timeout: PATIENT,
        },
        redaction: Redaction::unknown(),
        transcripts: None,
        deciders: vec![APPROVER],
        interpretation: patient_interpretation(),
        cancel: CancellationToken::new(),
    }
}

fn the_projects_own_check() -> WorkspaceCommand {
    WorkspaceCommand {
        program: "cargo".to_string(),
        args: vec!["test".to_string(), "--offline".to_string()],
        timeout: PATIENT,
    }
}

fn a_check_that_always_passes() -> WorkspaceCommand {
    WorkspaceCommand {
        program: "git".to_string(),
        args: vec!["status".to_string(), "--porcelain".to_string()],
        timeout: PATIENT,
    }
}

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

fn claims_success() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call("c1", "read_file", json!({"path": "src/lib.rs"})),
        MockTurn::text(r#"{"changed_files":[],"summary":"all good","claimed_complete":true}"#),
    ]
}

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

async fn run(
    world: &World,
    script: Vec<MockTurn>,
    check: WorkspaceCommand,
) -> (
    Result<Executed, CapabilityError>,
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

async fn run_with(
    world: &World,
    model: MockCompletionModel,
    check: WorkspaceCommand,
    bound_to: fiddle_core::CapabilityId,
    publishing_from: Option<PathBuf>,
) -> (
    Result<Executed, CapabilityError>,
    Vec<EvidenceRef>,
    Option<fiddle_core::Publication>,
) {
    let ctx = match publishing_from {
        Some(work) => world.ctx_publishing_from(work),
        None => world.ctx(),
    };
    execute_against(world, &ctx, model, check, bound_to).await
}

async fn continue_in(
    world: &World,
    model: MockCompletionModel,
) -> (
    Result<Executed, CapabilityError>,
    Vec<EvidenceRef>,
    Option<fiddle_core::Publication>,
) {
    world.with_no_workspace_available();
    let ctx = world.ctx_without_git();
    execute_against(world, &ctx, model, the_projects_own_check(), PROPOSE_CHANGE).await
}

async fn continue_in_a_process_that_can_attempt(
    world: &World,
    model: MockCompletionModel,
) -> (
    Result<Executed, CapabilityError>,
    Vec<EvidenceRef>,
    Option<fiddle_core::Publication>,
) {
    let _ = std::fs::remove_dir_all(world.workspace_root());
    let ctx = world.ctx();
    execute_against(world, &ctx, model, the_projects_own_check(), PROPOSE_CHANGE).await
}

async fn execute_against(
    world: &World,
    ctx: &EffectContext,
    model: MockCompletionModel,
    check: WorkspaceCommand,
    bound_to: fiddle_core::CapabilityId,
) -> (
    Result<Executed, CapabilityError>,
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
        .execute(ExecutionInput::unobserved(
            grant_for(PROPOSE_CHANGE),
            WORK_ID,
            INVOCATION_REF,
        ))
        .await;
    (outcome, capability.receipts(), capability.publication())
}

struct Suspension {
    comment: u64,
    head_sha: String,
}

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
    world.pull_request_at(PR, &head_sha, true);
    Suspension { comment, head_sha }
}

fn walk_at<'a>(target: &'a str, payload: &'a str, allowlist: &'a [u64]) -> DecisionWalk<'a> {
    DecisionWalk {
        repo: REPO,
        pr: PR,
        max_pages: 10,
        project: PROJECT,
        invocation_ref: INVOCATION_REF,
        kind: EffectName::shipped(ENSURE_PULL_REQUEST_READY),
        target,
        payload,
        allowlist,
    }
}

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
        ENSURE_PULL_REQUEST_READY,
        &pull_request_ready_target(REPO, PR, head_sha),
    );
    let request = fiddle_core::decision_request_id(PROJECT, INVOCATION_REF, &effect);
    let payload = payload_hash(
        &EnsurePullRequestReady::new(REPO.to_string(), PR, head_sha.to_string()).payload(),
    );
    (request, effect, payload)
}

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
            "propose_change",
            "cve_mitigate"
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
            EffectName::shipped(ENSURE_BRANCH_PUBLISHED),
            EffectName::shipped(ENSURE_PULL_REQUEST),
            EffectName::shipped(PUBLISH_DECISION_REQUEST),
        ],
        "{:?}",
        world.steps()
    );

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

#[tokio::test]
async fn the_gated_effect_is_not_proposed_before_there_is_an_answer() {
    let world = World::fresh();
    let _ = run(&world, repairs(), the_projects_own_check()).await;

    assert!(!world
        .effects_performed()
        .contains(&EffectName::shipped(ENSURE_PULL_REQUEST_READY)));
    assert!(
        !world
            .effects_proposed()
            .contains(&EffectName::shipped(ENSURE_PULL_REQUEST_READY)),
        "the effect must not reach the executor at all: {:?}",
        world.effects_proposed()
    );
    assert!(
        !world
            .argvs()
            .iter()
            .any(|argv| argv.iter().any(|arg| arg == "graphql")),
        "no GraphQL call may be made before there is an answer: {:?}",
        world.argvs()
    );
}

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
    assert_eq!(fixture::changed_files(&world.fixture), Vec::<String>::new());
}

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
    let review = publication
        .expect("a publication is reported on every arm")
        .review;
    assert!(
        matches!(review, Observation::Unavailable { .. }),
        "an unpublished run has read no forge and must not claim to: {review:?}"
    );
}

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

#[test]
fn the_capability_holds_no_credential_and_accounts_for_work_in_one_place() {
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
    assert_eq!(
        source.matches("self.record_change_set(").count(),
        1,
        "accounting has one call site, on the arm that concluded the transition; \
         a second is a path claiming work it has not completed"
    );
    assert_eq!(
        source.matches("fn record_change_set(").count(),
        1,
        "and the helper the count above is about has one definition; a second would \
         make that count a count of one of two writers, not of the writes. Neither \
         line sees a write spelled inline — `only_an_approval_marks_the_pull_request_\
         ready` is what covers that"
    );
}

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

    assert!(comments[0].contains(&question), "{}", comments[0]);
    assert!(
        comments[0].contains("ready for review?"),
        "yes and no both have to mean something: {}",
        comments[0]
    );
    match interaction {
        InteractionRef::GitHubPullRequestComment { repo, pr, comment } => {
            assert_eq!(repo, REPO);
            assert_eq!(pr, PR);
            assert_ne!(comment, 0, "a comment id nobody sent names no comment");
        }
        named => panic!(
            "this capability publishes to the github channel it was configured with and to no \
             other; exactly one channel is authoritative for one request, and this run named \
             {named}"
        ),
    }
}

#[tokio::test]
async fn a_second_process_finds_its_own_question_and_does_not_ask_twice() {
    let world = World::fresh();
    let suspension = suspended(&world).await;
    let published = suspension.head_sha.clone();
    world.answered_by(suspension.comment, &[]);

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
    assert_eq!(
        world.effects_performed(),
        [
            EffectName::shipped(ENSURE_BRANCH_PUBLISHED),
            EffectName::shipped(ENSURE_PULL_REQUEST),
            EffectName::shipped(PUBLISH_DECISION_REQUEST),
        ],
        "{:?}",
        world.steps()
    );
}

fn a_change_already_published(world: &World, draft: bool) -> String {
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
    world.pull_request_at(PR, &published, draft);
    published
}

#[tokio::test]
async fn a_published_change_nobody_has_been_asked_about_is_resumed_by_asking() {
    let world = World::fresh();
    let published = a_change_already_published(&world, true);

    let (outcome, _, _) = run(&world, repairs_differently(), the_projects_own_check()).await;

    let error = outcome.expect_err("the question is what was missing, so the run asks it");
    assert!(
        matches!(error, CapabilityError::AwaitingDecision { .. }),
        "got {error:?}"
    );
    assert_eq!(
        world.effects_performed(),
        [EffectName::shipped(PUBLISH_DECISION_REQUEST)],
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

#[tokio::test]
async fn a_readied_pull_request_is_not_re_drafted() {
    let world = World::fresh();
    let published = a_change_already_published(&world, false);

    let already_ready = EnsurePullRequestReady::new(REPO.to_string(), PR, published.clone())
        .inspect(&world.ctx())
        .await
        .expect("the world answers a by-number read");
    assert!(
        already_ready.is_some(),
        "the fixture must really hold a readied pull request, or nothing below is \
         about one"
    );

    let (outcome, _, _) = run(&world, repairs_differently(), the_projects_own_check()).await;

    assert!(
        world.pull_request_creates().is_empty(),
        "a readied pull request was re-drafted: {:?}",
        world.pull_request_creates()
    );
    assert_eq!(
        world.effects_performed(),
        [EffectName::shipped(PUBLISH_DECISION_REQUEST)],
        "only the question: the change was already published and already ready: {:?}",
        world.steps()
    );
    let error = outcome.expect_err("the question is what was missing, so the run asks it");
    assert!(
        matches!(error, CapabilityError::AwaitingDecision { .. }),
        "got {error:?}"
    );
    assert_eq!(world.posted_comments().len(), 1);
}

struct Answered {
    outcome: Result<Executed, CapabilityError>,
    continuation_receipts: Vec<EvidenceRef>,
    model: MockCompletionModel,
    request: fiddle_core::DecisionRequestId,
    head_sha: String,
    question_comment: u64,
    reply_comment: u64,
    posted_before: usize,
    published_before: Option<String>,
    requests_before: usize,
}

enum ThenWhat {
    Nothing,

    OneMoreAttempt(Vec<MockTurn>),
}

async fn answered(
    world: &World,
    author: u64,
    reply: &str,
    document: &str,
    then: ThenWhat,
) -> Answered {
    let suspension = suspended(world).await;
    let seeded = world.answered_by(suspension.comment, &[(author, reply)]);
    let [reply_comment] = seeded.as_slice() else {
        panic!("one reply was seeded, and the ids are {seeded:?}");
    };
    world.script_graphql(0, 200, readied());

    let posted_before = world.posted_comments().len();
    let published_before = world.published_file("src/lib.rs");
    let requests_before = world.requests().len();
    let mut turns = vec![MockTurn::text(document)];
    let attempting = match then {
        ThenWhat::Nothing => false,
        ThenWhat::OneMoreAttempt(more) => {
            turns.extend(more);
            true
        }
    };
    let model = MockCompletionModel::new(turns);
    let (outcome, receipts, _) = match attempting {
        false => continue_in(world, model.clone()).await,
        true => continue_in_a_process_that_can_attempt(world, model.clone()).await,
    };
    Answered {
        outcome,
        continuation_receipts: receipts,
        model,
        request: identity_at(&suspension.head_sha).0,
        head_sha: suspension.head_sha,
        question_comment: suspension.comment,
        reply_comment: *reply_comment,
        posted_before,
        published_before,
        requests_before,
    }
}

#[tokio::test]
async fn only_an_approval_marks_the_pull_request_ready() {
    for (reply, document, should_mutate, then, turns) in [
        (YES, APPROVES, true, ThenWhat::Nothing, 1),
        ("no, drop it", REJECTS, false, ThenWhat::Nothing, 1),
        (
            "do it differently",
            REDIRECTS,
            false,
            ThenWhat::OneMoreAttempt(repairs_differently()),
            3,
        ),
        ("what does this do?", UNCLEAR, false, ThenWhat::Nothing, 1),
    ] {
        let world = World::fresh();
        let answered = answered(&world, APPROVER, reply, document, then).await;

        assert_eq!(
            world
                .effects_performed()
                .contains(&EffectName::shipped(ENSURE_PULL_REQUEST_READY)),
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
        assert_eq!(
            answered.model.requests().len(),
            turns,
            "{reply:?} spent {} model calls",
            answered.model.requests().len()
        );
        assert_eq!(
            world.marker(WORK_ID).is_some(),
            should_mutate,
            "{reply:?} left change sets {:?}",
            world.change_sets()
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Traffic {
    Read,
    Write,
    GraphQl,
}

struct Call<'a> {
    method: &'a str,
    endpoint: &'a str,
}

fn call_of(argv: &[String]) -> Call<'_> {
    let mut method = "GET";
    let mut endpoint = "";
    let mut rest = argv.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--method" | "-X" => method = rest.next().map(String::as_str).unwrap_or_default(),
            "-f" | "-F" | "--input" => {
                rest.next();
            }
            "api" => {}
            flag if flag.starts_with('-') => {}
            bare if endpoint.is_empty() => endpoint = bare,
            _ => {}
        }
    }
    Call { method, endpoint }
}

impl Call<'_> {
    fn traffic(&self) -> Traffic {
        match (self.endpoint, self.method) {
            ("graphql", _) => Traffic::GraphQl,
            (_, "GET" | "HEAD") => Traffic::Read,
            _ => Traffic::Write,
        }
    }

    fn line(&self) -> String {
        format!("{} {}", self.method, self.endpoint)
    }
}

#[test]
fn a_recorded_call_is_sorted_by_what_it_did() {
    for (argv, expected) in [
        (
            vec!["api", "-i", "--method", "GET", "/repos/o/r/pulls/7"],
            Traffic::Read,
        ),
        (vec!["api", "-i", "/repos/o/r/pulls/7"], Traffic::Read),
        (
            vec![
                "api",
                "-i",
                "--method",
                "POST",
                "/repos/o/r/issues/7/comments",
                "--input",
                "-",
            ],
            Traffic::Write,
        ),
        (
            vec![
                "api",
                "-i",
                "--method",
                "PATCH",
                "/repos/o/r/pulls/7",
                "--input",
                "-",
            ],
            Traffic::Write,
        ),
        (
            vec![
                "api",
                "-i",
                "-X",
                "PATCH",
                "/repos/o/r/pulls/7",
                "--input",
                "-",
            ],
            Traffic::Write,
        ),
        (
            vec!["api", "-i", "--method", "PUT", "/repos/o/r/pulls/7/merge"],
            Traffic::Write,
        ),
        (
            vec![
                "api",
                "-i",
                "--method",
                "DELETE",
                "/repos/o/r/git/refs/heads/fiddle/1234",
            ],
            Traffic::Write,
        ),
        (
            vec![
                "api",
                "-i",
                "graphql",
                "-f",
                "query=mutation { closeIssue { clientMutationId } }",
            ],
            Traffic::GraphQl,
        ),
        (
            vec![
                "api",
                "-i",
                "--method",
                "POST",
                "graphql",
                "-f",
                "query=mutation { markPullRequestReadyForReview { clientMutationId } }",
            ],
            Traffic::GraphQl,
        ),
    ] {
        let argv: Vec<String> = argv.into_iter().map(str::to_string).collect();
        assert_eq!(call_of(&argv).traffic(), expected, "{argv:?}");
    }
}

#[tokio::test]
async fn a_redirect_writes_one_comment_and_asks_for_nothing_else() {
    let world = World::fresh();
    let answered = answered(
        &world,
        APPROVER,
        "do it differently",
        REDIRECTS,
        ThenWhat::OneMoreAttempt(repairs_differently()),
    )
    .await;

    let calls = world.argvs();
    let continuation = &calls[answered.requests_before..];
    let mut reads = 0;
    let mut graphql = 0;
    let mut writes: Vec<String> = Vec::new();
    for argv in continuation {
        let call = call_of(argv);
        match call.traffic() {
            Traffic::Read => reads += 1,
            Traffic::GraphQl => graphql += 1,
            Traffic::Write => writes.push(call.line()),
        }
    }

    assert!(
        reads > 0,
        "the denominator: the continuation made {} calls of which {reads} were \
         reads, and an empty write inventory over a continuation that asked the \
         forge nothing would be empty for the wrong reason",
        continuation.len()
    );
    assert_eq!(
        writes,
        [format!("POST /repos/{REPO}/issues/{PR}/comments")],
        "a redirect's only write is the question it asks about the new change, and \
         the whole continuation asked for {continuation:#?}"
    );
    assert_eq!(
        graphql, 0,
        "a redirect dispatches no GraphQL: {continuation:#?}"
    );
}

#[tokio::test]
async fn a_redirect_names_the_instruction_it_received_where_an_operator_reads_it() {
    for instruction in ["use a bounded loop instead", "add a regression test first"] {
        let world = World::fresh();
        let document = json!({
            "decision": "redirect",
            "redirect": instruction,
            "evidence": "do it differently",
        })
        .to_string();
        let answered = answered(
            &world,
            APPROVER,
            "do it differently",
            &document,
            ThenWhat::OneMoreAttempt(repairs_differently()),
        )
        .await;

        let named: Vec<&str> = answered
            .continuation_receipts
            .iter()
            .map(|receipt| receipt.0.as_str())
            .filter(|receipt| receipt.starts_with("redirect:"))
            .collect();
        assert_eq!(
            named.len(),
            1,
            "one redirect receipt, whatever else the run recorded: {:?}",
            answered.continuation_receipts
        );
        assert!(
            named[0].contains(instruction),
            "the receipt names the instruction the run was given: {}",
            named[0]
        );
        let origin = named[0]
            .strip_prefix("redirect:")
            .and_then(|rest| rest.split_once(':'))
            .map(|(origin, _)| origin)
            .unwrap_or_else(|| panic!("a redirect receipt names its origin: {}", named[0]));
        assert_eq!(
            origin.parse::<u64>().ok(),
            Some(answered.reply_comment),
            "the receipt must name the comment the instruction was read from, and the \
             person replied on comment {}: {}",
            answered.reply_comment,
            named[0]
        );
        assert_ne!(
            answered.reply_comment, answered.question_comment,
            "the reply and the question must be different comments, or naming either \
             one would satisfy the assertion above"
        );
        assert!(
            named[0].contains(&format!(
                "; 1 comment was read and not counted: comment {} by {} (the request \
                 comment is not a reply to itself)",
                answered.question_comment, FIDDLE_BOT
            )),
            "the receipt must say who else the walk read, and name them: {}",
            named[0]
        );

        let posted = world.posted_comments();
        assert_eq!(
            posted.len(),
            answered.posted_before + 1,
            "the redirect asked about the new change exactly once, so the comment \
             read below is its own and not the first run's: {posted:#?}"
        );
        let asked = posted.last().expect("a redirect asks a fresh question");
        assert!(
            asked.contains(instruction),
            "and the question an operator reads carries it too: {asked}"
        );
    }
}

#[tokio::test]
async fn the_transition_is_performed_through_the_decided_entry_point() {
    let world = World::fresh();
    let answered = answered(&world, APPROVER, YES, APPROVES, ThenWhat::Nothing).await;

    let concluded = answered
        .outcome
        .expect("an approved transition earns evidence");
    let evidence = concluded
        .earned()
        .expect("an approved transition earns evidence rather than refusing the change");
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
            EffectName::shipped(ENSURE_PULL_REQUEST_READY),
            ExecutionStep::ResolveDecision.as_str()
        )),
        "the gated effect was authorized by a decision, which only \
         `execute_decided` can announce: {:?}",
        world.steps()
    );
    assert_eq!(
        world.effects_performed(),
        [
            EffectName::shipped(ENSURE_BRANCH_PUBLISHED),
            EffectName::shipped(ENSURE_PULL_REQUEST),
            EffectName::shipped(PUBLISH_DECISION_REQUEST),
            EffectName::shipped(ENSURE_PULL_REQUEST_READY),
        ],
        "{:?}",
        world.steps()
    );
    let kinds: Vec<&str> = answered
        .continuation_receipts
        .iter()
        .filter(|entry| entry.0.starts_with("effect:"))
        .map(|entry| entry.0.split(':').nth(1).unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["ensure_pull_request_ready"],
        "a continuation's receipts are its own: {:?}",
        answered.continuation_receipts
    );
}

#[tokio::test]
async fn the_approve_path_accounts_for_the_work_it_completed() {
    let world = World::fresh();
    let suspension = suspended(&world).await;
    assert_eq!(
        world.marker(WORK_ID),
        None,
        "a suspended run has not earned a marker: {:?}",
        world.change_sets()
    );

    world.answered_by(suspension.comment, &[(APPROVER, YES)]);
    world.script_graphql(0, 200, readied());
    let model = MockCompletionModel::new([MockTurn::text(APPROVES)]);
    let (outcome, _, _) = continue_in(&world, model).await;
    outcome.expect("an approved transition completes");

    assert_eq!(
        world.marker(WORK_ID).as_deref(),
        Some(fiddle_core::correlation_key(PROJECT, INVOCATION_REF).as_str()),
        "the marker is this run's own, derived from its two canonical inputs"
    );
    assert_ne!(
        world.marker(WORK_ID).as_deref(),
        Some(fiddle_core::correlation_key(PROJECT, "beans:some-other-run").as_str()),
        "and it is a function of them: another run's inputs give another marker"
    );
    assert_eq!(
        world.change_sets(),
        [format!("{WORK_ID}.json")],
        "one change set, filed under the work item the run was asked about"
    );
}

#[tokio::test]
async fn a_rejection_concludes_the_run_rather_than_suspending_it_again() {
    let world = World::fresh();
    let answered = answered(&world, APPROVER, "no, drop it", REJECTS, ThenWhat::Nothing).await;

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

#[tokio::test]
async fn an_unclear_reply_waits_on_the_same_request_and_posts_nothing_further() {
    let world = World::fresh();
    let answered = answered(
        &world,
        APPROVER,
        "what does this do?",
        UNCLEAR,
        ThenWhat::Nothing,
    )
    .await;

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

#[tokio::test]
async fn a_redirect_produces_a_different_change_and_asks_again_about_it() {
    let world = World::fresh();
    let answered = answered(
        &world,
        APPROVER,
        "do it differently",
        REDIRECTS,
        ThenWhat::OneMoreAttempt(repairs_differently()),
    )
    .await;
    let first_sha = answered.head_sha.clone();
    let first_file = answered
        .published_before
        .clone()
        .expect("the first run published a tree");

    let error = answered
        .outcome
        .expect_err("a redirect has earned no transition");
    assert!(
        matches!(error, CapabilityError::AwaitingDecision { .. }),
        "got {error:?}"
    );
    assert_eq!(error.recurrence(), Recurrence::Awaiting);
    assert_eq!(world.graphql_calls(), 0, "no approval was spent");
    assert!(
        world.marker(WORK_ID).is_none(),
        "a waiting run accounts for nothing: {:?}",
        world.change_sets()
    );

    let second_file = world
        .published_file("src/lib.rs")
        .expect("the redirected attempt published a tree");
    assert_ne!(
        second_file, first_file,
        "the published tree must differ, and it is the tree that is the deliverable"
    );
    let second_sha = world.published_sha();
    assert_ne!(second_sha, first_sha, "the head moved");
    assert!(
        world.is_ancestor(&first_sha, &second_sha),
        "the new commit must descend from the published one, or the push was a \
         rewrite: {first_sha} -> {second_sha}"
    );
    assert!(
        !world.is_ancestor(&second_sha, &first_sha),
        "and strictly forward, which is the denominator for the line above"
    );

    assert_eq!(world.branches(), [world.branch()], "one branch");
    assert_eq!(
        world.pull_request_creates().len(),
        1,
        "no second pull request was opened: {:?}",
        world.pull_request_creates()
    );

    let posted = world.posted_comments();
    assert_eq!(
        posted.len(),
        answered.posted_before + 1,
        "exactly one further question: {posted:?}"
    );
    let second = parse_marker(posted.last().unwrap()).expect("the new question carries a marker");
    assert_eq!(
        second.head_sha, second_sha,
        "and it is about the change that was just published"
    );
    assert_ne!(
        second.request, answered.request,
        "a new head is a new question"
    );
    assert_ne!(second.effect, identity_at(&first_sha).1, "and a new effect");
    assert_eq!(
        world.comments_naming(&answered.request).len(),
        1,
        "the old question is neither deleted nor edited: {posted:?}"
    );

    assert_eq!(
        answered.model.requests().len(),
        3,
        "one interpretation and one two-turn attempt"
    );
}

#[tokio::test]
async fn the_approve_path_invokes_git_not_at_all() {
    let world = World::fresh();
    let answered = answered(&world, APPROVER, YES, APPROVES, ThenWhat::Nothing).await;

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
            .continuation_receipts
            .contains(&EvidenceRef("tools:0".to_string())),
        "a continuation calls no tool, because it runs no attempt: {:?}",
        answered.continuation_receipts
    );
    assert_eq!(world.graphql_calls(), 1, "one approval, one mutation");
}

#[tokio::test]
async fn the_capability_delegates_the_whole_validation_order() {
    let world = World::fresh();
    let answered = answered(&world, STRANGER, "approve", APPROVES, ThenWhat::Nothing).await;

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
            .contains(&EffectName::shipped(ENSURE_PULL_REQUEST_READY)),
        "{:?}",
        world.effects_performed()
    );
    assert_eq!(world.graphql_calls(), 0);
}

#[tokio::test]
async fn the_second_payload_comparison_catches_what_the_first_could_not_see() {
    let world = World::fresh();
    let suspension = suspended(&world).await;
    world.answered_by(suspension.comment, &[(APPROVER, YES)]);
    world.script_graphql(0, 200, readied());

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
            EffectName::shipped(ENSURE_PULL_REQUEST_READY),
            ExecutionStep::Authorize.as_str()
        )),
        "refused at step 4 by the decision, not at step 6 by the envelope: {:?}",
        world.steps()
    );

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

#[tokio::test]
async fn a_second_invocation_after_an_approval_accounts_for_the_work_and_does_not_mutate_again() {
    let world = World::fresh();
    let first = answered(&world, APPROVER, YES, APPROVES, ThenWhat::Nothing).await;
    first.outcome.expect("the approved transition landed");
    let accounted = world
        .marker(WORK_ID)
        .expect("the approving invocation accounted for its work");

    std::fs::remove_file(world.stub_root().join(format!("changes/{WORK_ID}.json"))).unwrap();
    assert_eq!(
        world.marker(WORK_ID),
        None,
        "the denominator for the marker below"
    );

    let model = MockCompletionModel::new([MockTurn::text(APPROVES)]);
    let (outcome, receipts, _) = continue_in(&world, model.clone()).await;

    let concluded = outcome.expect("the transition this run was about has happened");
    let evidence = concluded
        .earned()
        .expect("the transition earns evidence rather than refusing the change");
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
            .any(|kind| *kind == EffectName::shipped(ENSURE_PULL_REQUEST_READY)),
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
    assert_eq!(
        world.marker(WORK_ID).as_deref(),
        Some(accounted.as_str()),
        "it accounts for the work it found already done, under the same marker the \
         invocation that performed the transition wrote"
    );
}

fn proposal(target: &str, payload: &str) -> ProposedEffect {
    ProposedEffect {
        capability: PROPOSE_CHANGE,
        kind: EffectName::shipped(ENSURE_PULL_REQUEST_READY),
        target: target.to_string(),
        payload: payload.to_string(),
    }
}

fn widened(payload: &str) -> String {
    let mut asked: serde_json::Map<String, Value> =
        serde_json::from_str(payload).expect("the payload is an object");
    asked.insert("merge".to_string(), json!(true));
    Value::Object(asked).to_string()
}

fn observed_issue(revision: Option<&str>) -> WorkItemState {
    WorkItemState {
        id: ISSUE.to_string(),
        status: "In Progress".to_string(),
        projected_status: None,
        revision: revision.map(str::to_string),
    }
}

async fn steered_by(
    world: &World,
    invocation_ref: &str,
    work_item: Option<&WorkItemState>,
) -> (Result<Executed, CapabilityError>, Vec<EvidenceRef>) {
    let ctx = world.ctx_publishing_from(world.work_for(invocation_ref));
    let deployment = Deployment(DeploymentRule::Allow);
    let executor = Executor::new(
        PROPOSE_CHANGE,
        PROJECT.to_string(),
        invocation_ref.to_string(),
        &deployment,
        &ctx,
        world,
        ReadRetry::none(),
    );
    let capability = ProposeChange::new(
        executor,
        &ctx,
        world,
        MockCompletionModel::new(repairs()),
        config(world, the_projects_own_check()),
    );
    let outcome = capability
        .execute(ExecutionInput::observed(
            grant_for(PROPOSE_CHANGE),
            WORK_ID,
            invocation_ref,
            work_item,
        ))
        .await;
    (outcome, capability.receipts())
}

#[tokio::test]
async fn a_jira_run_asks_on_the_issue_and_leaves_the_pull_request_unwritten() {
    let world = World::reachable_jira_site().await;
    let held = world.revision_the_site_holds().await;
    let observed = observed_issue(Some(&held));

    let (outcome, receipts) = steered_by(&world, JIRA_INVOCATION, Some(&observed)).await;

    let error = outcome.expect_err("a run that asked a question earned no evidence");
    match &error {
        CapabilityError::AwaitingDecision {
            interaction: InteractionRef::JiraIssueComment { issue, .. },
            ..
        } => assert_eq!(issue, ISSUE),
        other => panic!("a jira run waits on a jira comment, got {other:?}"),
    }
    assert_eq!(
        world.jira_comments_on(ISSUE).await,
        1,
        "the question reached the issue the run was invoked for"
    );
    assert_eq!(
        world.posted_comments().len(),
        0,
        "and the pull request carries none; this zero is the counter-case that keeps the count          above from passing on a run that writes to both"
    );
    assert!(
        world
            .effects_performed()
            .contains(&EffectName::shipped(JIRA_COMMENT_ADDED)),
        "{:?}",
        world.effects_performed()
    );
    assert!(
        !world
            .effects_performed()
            .contains(&EffectName::shipped(PUBLISH_DECISION_REQUEST)),
        "{:?}",
        world.effects_performed()
    );
    assert_eq!(
        effect_kinds(&receipts),
        [
            "ensure_branch_published",
            "ensure_pull_request",
            JIRA_COMMENT_ADDED
        ],
        "the evidence line spells the effect the chosen channel performed, and the selector \
         does not hide it: {receipts:?}"
    );
}

#[tokio::test]
async fn a_pull_request_run_asks_on_the_pull_request_and_leaves_the_issue_unwritten() {
    let world = World::reachable_jira_site().await;
    let held = world.revision_the_site_holds().await;
    let observed = observed_issue(Some(&held));

    let (outcome, receipts) = steered_by(&world, INVOCATION_REF, Some(&observed)).await;

    let error = outcome.expect_err("a run that asked a question earned no evidence");
    match &error {
        CapabilityError::AwaitingDecision {
            interaction: InteractionRef::GitHubPullRequestComment { repo, .. },
            ..
        } => assert_eq!(repo, REPO),
        other => panic!("a pull-request run waits on a github comment, got {other:?}"),
    }
    assert_eq!(
        world.posted_comments().len(),
        1,
        "the question reached the pull request the run opened"
    );
    assert_eq!(
        world.jira_comments_on(ISSUE).await,
        0,
        "and the issue carries none, although this run observed that issue; the channel          follows the invocation and not the observation"
    );
    assert!(
        world
            .effects_performed()
            .contains(&EffectName::shipped(PUBLISH_DECISION_REQUEST)),
        "{:?}",
        world.effects_performed()
    );
    assert!(
        !world
            .effects_performed()
            .contains(&EffectName::shipped(JIRA_COMMENT_ADDED)),
        "{:?}",
        world.effects_performed()
    );
    assert_eq!(
        effect_kinds(&receipts),
        [
            "ensure_branch_published",
            "ensure_pull_request",
            PUBLISH_DECISION_REQUEST
        ],
        "the github evidence line still spells the name it spelled before the selector \
         carried the question: {receipts:?}"
    );
}

#[tokio::test]
async fn a_jira_run_that_observed_no_revision_asks_nobody_and_names_the_rule() {
    let world = World::reachable_jira_site().await;
    let observed = observed_issue(None);

    let (outcome, _) = steered_by(&world, JIRA_INVOCATION, Some(&observed)).await;

    let error = outcome.expect_err("a run that named no channel asked nobody");
    assert!(
        matches!(&error, CapabilityError::Unasked(_)),
        "got {error:?}"
    );
    assert_eq!(
        error.recurrence(),
        Recurrence::Permanent,
        "a run that observed no revision observes none on a retry either"
    );
    let said = error.to_string();
    assert!(
        said.contains("no channel is named"),
        "the refusal names the rule it holds: {said}"
    );
    assert_eq!(world.jira_comments_on(ISSUE).await, 0);
    assert_eq!(world.posted_comments().len(), 0);
}

#[tokio::test]
async fn a_jira_run_whose_revision_is_not_a_time_asks_nobody_and_names_the_issue() {
    let world = World::reachable_jira_site().await;
    let observed = observed_issue(Some("yesterday"));

    let (outcome, _) = steered_by(&world, JIRA_INVOCATION, Some(&observed)).await;

    let error = outcome.expect_err("a run that could build no identity asked nobody");
    assert!(
        matches!(&error, CapabilityError::Unasked(_)),
        "got {error:?}"
    );
    let said = error.to_string();
    assert!(
        said.contains("carries no revision this run can build an identity from"),
        "the refusal says why the issue could not be addressed: {said}"
    );
    assert!(
        !said.contains("no channel is named"),
        "and it is not the refusal a run that named nothing gets: {said}"
    );
    assert_eq!(world.jira_comments_on(ISSUE).await, 0);
    assert_eq!(world.posted_comments().len(), 0);
}

fn effect_kinds(receipts: &[EvidenceRef]) -> Vec<String> {
    receipts
        .iter()
        .map(|entry| entry.0.as_str())
        .filter(|entry| entry.starts_with("effect:"))
        .map(|entry| entry.split(':').nth(1).unwrap_or_default().to_string())
        .collect()
}
