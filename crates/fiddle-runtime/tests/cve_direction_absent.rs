mod support;

use fiddle_core::{DeploymentRule, EffectName, CVE_MITIGATE};
use fiddle_runtime::agent::AgentBudget;
use fiddle_runtime::capability::cve::Approved;
use fiddle_runtime::capability::{ChangesRequested, HumanSaid};
use fiddle_runtime::cve::verdict::Budget;
use fiddle_runtime::effect::{EffectContext, EffectTrace, ExecutionStep, Executor, ReadRetry};
use fiddle_runtime::evaluate::{Check, Success};
use fiddle_runtime::scanner::{ScanError, ScanReport, Scanner};
use fiddle_runtime::workspace::WorkspaceCommand;
use fiddle_runtime::{
    CapabilityError, CveMitigate, GhCli, GhError, GitCli, JiraHttp, MitigateConfig, Redaction,
};
use rig_core::test_utils::MockCompletionModel;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use support::{Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const REPO: &str = "peel/r";

const BASE: &str = "main";

const BRANCH: &str = "security/2026-08-26-cve-sweep";

const NUMBER: u64 = 41;

const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const PATIENT: Duration = Duration::from_secs(60);

const ASKED_FOR: &str = "this bump skips the call site in main.go";

const REMARKED: &str = "the rescan reads clean on my machine too";

const NAMES_THE_ABSENT_FORGE: &str =
    "this deployment holds no `[github]` configuration, so no request was sent";

struct Silent;

impl EffectTrace for Silent {
    fn step(&self, _kind: &EffectName, _step: ExecutionStep) {}
}

struct NeverScans;

#[async_trait::async_trait]
impl Scanner for NeverScans {
    async fn scan(&self, _image: &str) -> Result<ScanReport, ScanError> {
        panic!("reading the direction on a pull request scans nothing")
    }
}

struct Forge {
    dir: TempDir,
}

impl Forge {
    fn answering(reviews: serde_json::Value, conversation: serde_json::Value) -> Self {
        let dir = TempDir::new().expect("a temporary directory for the forge");
        std::fs::create_dir_all(dir.path().join("config")).expect("a gh configuration directory");
        for (collection, page) in [("reviews", reviews), ("issue-comments", conversation)] {
            let held = dir.path().join(collection);
            std::fs::create_dir_all(&held).expect("a collection directory");
            std::fs::write(held.join("page-1.json"), page.to_string())
                .expect("the collection's one page is written");
        }
        Forge { dir }
    }

    fn gh(&self) -> GhCli {
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
        )
    }

    fn requests(&self) -> Vec<String> {
        let held = self.dir.path().join("requests");
        let Ok(entries) = std::fs::read_dir(&held) else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = entries.map(|entry| entry.unwrap().path()).collect();
        paths.sort();
        paths
            .into_iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .collect()
    }
}

fn review(state: &str, association: &str, body: &str) -> serde_json::Value {
    json!({
        "state": state,
        "body": body,
        "author_association": association,
        "commit_id": HEAD,
        "user": { "login": "sofia", "id": 11, "type": "User" },
    })
}

fn comment(association: &str, body: &str) -> serde_json::Value {
    json!({
        "id": 7001,
        "body": body,
        "created_at": "2026-08-26T09:00:00Z",
        "updated_at": "2026-08-26T09:00:00Z",
        "author_association": association,
        "performed_via_github_app": serde_json::Value::Null,
        "user": { "login": "sofia", "id": 11, "type": "User" },
    })
}

fn tracker_only() -> EffectContext {
    EffectContext::tracking(
        JiraHttp::new(
            "http://127.0.0.1:1",
            "bot@example.com",
            "s3cr3t",
            Duration::from_secs(1),
        )
        .expect("a client is built without reaching the site"),
        PathBuf::from("/nonexistent"),
        CancellationToken::new(),
    )
}

fn forge_context(forge: &Forge) -> EffectContext {
    EffectContext::new(
        forge.gh(),
        GitCli::new(
            PathBuf::from("git"),
            "ghp_never_reaches_a_network".to_string(),
            "FIDDLE_GITHUB_TOKEN",
            PATIENT,
        ),
        PathBuf::from("/nonexistent"),
        CancellationToken::new(),
    )
}

struct Reading {
    held: TempDir,
}

impl Reading {
    fn new() -> Self {
        Reading {
            held: TempDir::new().expect("a temporary directory for the sweep's paths"),
        }
    }

    fn config(&self) -> MitigateConfig {
        MitigateConfig {
            repo: REPO.to_string(),
            head_owner: "peel".to_string(),
            base: BASE.to_string(),
            title: "fiddle: mitigate {advisories} reported advisories".to_string(),
            project: PROJECT.to_string(),
            stub_root: self.held.path().join("stub"),
            tree: self.held.path().join("tree"),
            workspace_root: self.held.path().join("workspaces"),
            image: "example.invalid/app@sha256:0000".to_string(),
            severities: Default::default(),
            scratch: self.held.path().join("scratch"),
            checks: vec![Check {
                program: "git".to_string(),
                args: vec!["--version".to_string()],
                success: Success::ExitZero,
            }],
            check: WorkspaceCommand {
                program: "git".to_string(),
                args: vec!["--version".to_string()],
                timeout: PATIENT,
            },
            commands: std::sync::Arc::new(Vec::new()),
            budget: AgentBudget {
                max_turns: 1,
                max_tokens: 16,
                deadline: PATIENT,
                max_changed_files: 1,
                tool_timeout: PATIENT,
            },
            redaction: Redaction::unknown(),
            transcripts: None,
            command_timeout: PATIENT,
            findings: Budget::of(1),
            max_attempts: 1,
            report_dir: self.held.path().join("reports"),
            today: "2026-08-26".to_string(),
            settle: Duration::ZERO,
            filing: None,
            cancel: CancellationToken::new(),
        }
    }
}

fn reused() -> Approved {
    Approved::Reuse {
        number: NUMBER,
        branch: BRANCH.to_string(),
        head_sha: HEAD.to_string(),
        base: BASE.to_string(),
        duplicates: Vec::new(),
    }
}

fn fresh() -> Approved {
    Approved::Fresh {
        branch: BRANCH.to_string(),
        base: BASE.to_string(),
    }
}

struct Asked {
    reviews: Result<(Vec<ChangesRequested>, Vec<HumanSaid>), CapabilityError>,
    conversation: Result<Vec<HumanSaid>, CapabilityError>,
}

async fn ask(ctx: &EffectContext, approved: &Approved) -> Asked {
    let deployment = Deployment(DeploymentRule::Allow);
    let trace = Silent;
    let paths = Reading::new();
    let executor = Executor::new(
        CVE_MITIGATE,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        ctx,
        &trace,
        ReadRetry::none(),
    );
    let capability = CveMitigate::new(
        executor,
        ctx,
        NeverScans,
        MockCompletionModel::new(Vec::new()),
        paths.config(),
    );
    Asked {
        reviews: capability.reviews(approved).await,
        conversation: capability.conversation(approved, &[]).await,
    }
}

fn refusal(answered: &Result<impl std::fmt::Debug, CapabilityError>, what: &str) -> String {
    let error = answered.as_ref().err().unwrap_or_else(|| {
        panic!("a forge nothing can reach must not answer {what}: {answered:?}")
    });
    assert!(
        matches!(error, CapabilityError::Forge(GhError::Unconfigured)),
        "and it must refuse with the accessor's own error rather than any failure that \
         happens to be handy: {error:?}"
    );
    error.to_string()
}

#[tokio::test]
async fn a_deployment_with_no_forge_refuses_the_direction_it_cannot_read() {
    let ctx = tracker_only();
    assert!(
        matches!(ctx.gh_client(), Err(GhError::Unconfigured)),
        "this world's premise: the context holds no forge"
    );

    let asked = ask(&ctx, &reused()).await;

    let for_reviews = refusal(&asked.reviews, "a list of reviews");
    let for_conversation = refusal(&asked.conversation, "a conversation");

    for (arm, said) in [
        ("reviews", &for_reviews),
        ("conversation", &for_conversation),
    ] {
        assert!(
            said.contains(NAMES_THE_ABSENT_FORGE),
            "the {arm} arm names the client the deployment does not hold, which is what \
             tells a reader this is an absent forge and not a quiet one: {said}"
        );
    }
}

#[tokio::test]
async fn a_forge_that_answers_nothing_is_not_a_forge_that_is_absent() {
    let forge = Forge::answering(json!([]), json!([]));
    let ctx = forge_context(&forge);

    let asked = ask(&ctx, &reused()).await;

    let (requested, said_in_reviews) = asked
        .reviews
        .expect("a forge that answered an empty list answered, so the run reads it");
    let said = asked
        .conversation
        .expect("and the same for the conversation it answered nothing in");

    assert!(
        requested.is_empty() && said_in_reviews.is_empty() && said.is_empty(),
        "nobody asked for changes and nobody spoke: {requested:?} {said_in_reviews:?} {said:?}"
    );

    let absent = ask(&tracker_only(), &reused()).await;
    assert!(
        absent.reviews.is_err() && absent.conversation.is_err(),
        "and the run that could reach no forge at all does not reach this same answer, \
         or an operator reading `no reviews` cannot tell which world produced it"
    );

    let asked_for = forge.requests().join("\n");
    assert!(
        asked_for.contains(&format!("/repos/{REPO}/pulls/{NUMBER}/reviews")),
        "the empty answer above was the forge's, read over the reviews route: {asked_for}"
    );
    assert!(
        asked_for.contains(&format!("/repos/{REPO}/issues/{NUMBER}/comments")),
        "and over the conversation route: {asked_for}"
    );
}

#[tokio::test]
async fn the_same_routes_carry_direction_when_the_forge_holds_some() {
    let forge = Forge::answering(
        json!([review("CHANGES_REQUESTED", "OWNER", ASKED_FOR)]),
        json!([comment("MEMBER", REMARKED)]),
    );
    let ctx = forge_context(&forge);

    let asked = ask(&ctx, &reused()).await;

    let (requested, _) = asked.reviews.expect("the forge answered its reviews");
    let said = asked.conversation.expect("and its conversation");

    assert_eq!(
        requested
            .iter()
            .map(|it| it.body.as_str())
            .collect::<Vec<_>>(),
        [ASKED_FOR],
        "a review asking for changes reaches the run, so the empty answer in the row \
         above is an empty forge and not a route this stub cannot serve"
    );
    assert_eq!(
        said.iter().map(|it| it.body.as_str()).collect::<Vec<_>>(),
        [REMARKED],
        "and so does a comment"
    );
}

#[tokio::test]
async fn a_plan_that_opens_a_fresh_pull_request_asks_no_forge_and_refuses_nothing() {
    let asked = ask(&tracker_only(), &fresh()).await;

    let (requested, said_in_reviews) = asked.reviews.expect(
        "a plan with no pull request behind it has no reviews to read and sends \
                 no request, so there is nothing for an absent forge to refuse",
    );
    let said = asked
        .conversation
        .expect("and no conversation to read either");

    assert!(
        requested.is_empty() && said_in_reviews.is_empty() && said.is_empty(),
        "and what it reads is nothing: {requested:?} {said_in_reviews:?} {said:?}"
    );
}
