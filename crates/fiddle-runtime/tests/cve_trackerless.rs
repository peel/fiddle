mod support;

use fiddle_core::{
    AttemptId, DeploymentRule, EffectName, EvidenceRef, NextAction, RunDisposition,
    TreeObservation, CVE_MITIGATE,
};
use fiddle_runtime::agent::AgentBudget;
use fiddle_runtime::capability::{attempt_worktree, Capability, ExecutionGrant, ExecutionInput};
use fiddle_runtime::cve::verdict::Budget;
use fiddle_runtime::effect::{EffectContext, EffectTrace, ExecutionStep, Executor, ReadRetry};
use fiddle_runtime::evaluate::{Check, Success};
use fiddle_runtime::workspace::WorkspaceCommand;
use fiddle_runtime::{CveMitigate, GhCli, GitCli, MitigateConfig, Redaction};
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::cve::{ask_git, every_fixture_grade, scanner_with, wiz_stub};
use support::stub_jira::{client_for, StubJira};
use support::{Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const REPO: &str = "peel/r";

const OWNER: &str = "peel";

const BASE: &str = "main";

const TODAY: &str = "2026-08-26";

const TITLE: &str = "fiddle: mitigate {advisories} reported advisories";

const WORK_ID: &str = "cve-sweep";

const ATTEMPT: &str = "01JCVETRACKERLESS0000000M5";

const TICKET: &str = "IDENT-1";

const PATIENT: Duration = Duration::from_secs(120);

const SOURCE: &str = "main.go";

const SOURCE_BEFORE: &str =
    "package main\n\nfunc main() {\n\tlegacyName()\n}\n\nfunc legacyName() {}\n";

const SOURCE_AFTER: &str =
    "package main\n\nfunc main() {\n\trenamedName()\n}\n\nfunc renamedName() {}\n";

const MITIGATED: &str = "CVE-2026-0001";

const REPORTED: &str = r#"{"changed_files":["main.go"],"summary":"applied the rename the bump needs","claimed_complete":true,"findings":[{"cve":"CVE-2026-0001","attempted":true,"note":"the bump reached this call site"}]}"#;

const DECLINED: &str = r#"{"changed_files":[],"summary":"clearing this needs a major bump I am not confident in","claimed_complete":false,"findings":[{"cve":"CVE-2026-0001","attempted":false,"note":"I changed nothing"}]}"#;

const NO_PULL_REQUEST_WAS_OPENED: &str = "no pull request was opened on this forge";

const NO_BRANCH_WAS_PUSHED: &str = "no branch under security/ reached this forge";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WithJira {
    Absent,
    Configured,
    RefusingEveryRead,
}

#[derive(Debug, Eq, PartialEq)]
struct Outcome {
    evidence: Result<EvidenceRef, String>,
    disposition: Option<RunDisposition>,
    tree: Option<TreeObservation>,
    receipts: Vec<EvidenceRef>,
}

struct Ran {
    outcome: Outcome,
    pull_request_title: String,
    pull_request_body: String,
    diff: String,
    commit_messages: String,
    prompt: String,
}

struct Silent;

impl EffectTrace for Silent {
    fn step(&self, _kind: &EffectName, _step: ExecutionStep) {}
}

fn seed() -> TempDir {
    let held = TempDir::new().expect("a temporary directory for the seed repository");
    let repo = held.path().join("seed");
    std::fs::create_dir_all(&repo).expect("the seed repository's directory is creatable");
    ask_git(
        &repo,
        &["-c", "init.defaultBranch=main", "init", "--quiet", "."],
    );
    std::fs::write(repo.join(SOURCE), SOURCE_BEFORE).expect("the seed repository is writable");
    std::fs::write(
        repo.join("notes.txt"),
        "the repository this run mitigates\n",
    )
    .expect("the seed repository is writable");
    ask_git(&repo, &["add", "-A"]);
    ask_git(
        &repo,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "the call site a mitigation rewrites",
        ],
    );
    held
}

fn seed_repo(seed: &TempDir) -> PathBuf {
    seed.path().join("seed")
}

struct World {
    stub: TempDir,
    trees: TempDir,
    workspaces: TempDir,
    reports: TempDir,
    scratch: TempDir,
}

impl World {
    fn cloned_from(seed: &Path) -> Self {
        let world = World {
            stub: TempDir::new().expect("a temporary directory for the forge"),
            trees: TempDir::new().expect("a temporary directory for the checkout"),
            workspaces: TempDir::new().expect("a temporary directory for the worktrees"),
            reports: TempDir::new().expect("a temporary directory for the reports"),
            scratch: TempDir::new().expect("a temporary directory for the rescans"),
        };
        std::fs::create_dir_all(world.stub.path().join("config"))
            .expect("the forge's configuration directory is creatable");
        std::fs::create_dir_all(world.stub_root().join("changes"))
            .expect("the change set directory is creatable");
        ask_git(
            world.stub.path(),
            &[
                "clone",
                "--quiet",
                "--bare",
                &seed.display().to_string(),
                &world.remote().display().to_string(),
            ],
        );
        ask_git(
            world.trees.path(),
            &[
                "clone",
                "--quiet",
                &world.remote().display().to_string(),
                &world.tree().display().to_string(),
            ],
        );
        world
    }

    fn remote(&self) -> PathBuf {
        self.stub.path().join("remote.git")
    }

    fn tree(&self) -> PathBuf {
        self.trees.path().join("clone")
    }

    fn stub_root(&self) -> PathBuf {
        self.reports.path().join("stub")
    }

    fn workspace_root(&self) -> PathBuf {
        self.workspaces.path().to_path_buf()
    }

    fn gh(&self) -> GhCli {
        GhCli::new(
            PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
            vec![
                "--stub-dir".to_string(),
                self.stub.path().display().to_string(),
            ],
            "ghp_never_reaches_a_network".to_string(),
            "FIDDLE_GITHUB_TOKEN",
            self.stub.path().join("config"),
            PATIENT,
        )
    }

    fn config(&self) -> MitigateConfig {
        let rescan = wiz_stub("clean-image");
        MitigateConfig {
            repo: REPO.to_string(),
            head_owner: OWNER.to_string(),
            base: BASE.to_string(),
            title: TITLE.to_string(),
            project: PROJECT.to_string(),
            stub_root: self.stub_root(),
            tree: self.tree(),
            workspace_root: self.workspace_root(),
            image: support::cve::image(),
            severities: every_fixture_grade(),
            scratch: self.scratch.path().to_path_buf(),
            checks: vec![
                passing_check(),
                Check {
                    program: rescan.program,
                    args: rescan.args,
                    success: Success::ArtefactWritten,
                },
            ],
            check: passing_command(),
            commands: std::sync::Arc::new(Vec::new()),
            budget: AgentBudget {
                max_turns: 8,
                max_tokens: 4096,
                deadline: Duration::from_secs(300),
                max_changed_files: 16,
                tool_timeout: PATIENT,
            },
            redaction: Redaction::unknown(),
            transcripts: None,
            command_timeout: PATIENT,
            findings: Budget::of(5),
            max_attempts: 3,
            report_dir: self.reports.path().to_path_buf(),
            today: TODAY.to_string(),
            settle: Duration::ZERO,
            filing: None,
            cancel: CancellationToken::new(),
        }
    }

    fn created(&self) -> Option<serde_json::Value> {
        let key = format!("POST_repos_{}_pulls", REPO.replace('/', "_"));
        std::fs::read_to_string(self.stub.path().join("world"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|landed| landed["key"].as_str() == Some(key.as_str()))
            .and_then(|landed| {
                serde_json::from_str::<serde_json::Value>(landed["body"].as_str()?).ok()
            })
    }

    fn pull_request_title(&self) -> String {
        match self.created() {
            Some(created) => created["title"].as_str().unwrap_or_default().to_string(),
            None => NO_PULL_REQUEST_WAS_OPENED.to_string(),
        }
    }

    fn pull_request_body(&self) -> String {
        match self.created() {
            Some(created) => created["body"].as_str().unwrap_or_default().to_string(),
            None => NO_PULL_REQUEST_WAS_OPENED.to_string(),
        }
    }

    fn published_branch(&self) -> Option<String> {
        ask_git(
            &self.remote(),
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .find(|branch| branch.starts_with("security/"))
        .map(str::to_string)
    }

    fn diff(&self) -> String {
        match self.published_branch() {
            Some(branch) => ask_git(&self.remote(), &["diff", &format!("{BASE}...{branch}")]),
            None => NO_BRANCH_WAS_PUSHED.to_string(),
        }
    }

    fn commit_messages(&self) -> String {
        match self.published_branch() {
            Some(branch) => ask_git(
                &self.remote(),
                &["log", "--format=%B", &format!("{BASE}..{branch}")],
            ),
            None => NO_BRANCH_WAS_PUSHED.to_string(),
        }
    }
}

fn passing_check() -> Check {
    Check {
        program: "git".to_string(),
        args: vec!["--version".to_string()],
        success: Success::ExitZero,
    }
}

fn passing_command() -> WorkspaceCommand {
    WorkspaceCommand {
        program: "git".to_string(),
        args: vec!["--version".to_string()],
        timeout: PATIENT,
    }
}

fn migrates() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call("c1", "read_file", json!({ "path": SOURCE })),
        MockTurn::tool_call(
            "c2",
            "write_file",
            json!({ "path": SOURCE, "contents": SOURCE_AFTER }),
        ),
        MockTurn::tool_call("c3", "run_check", json!({})),
        MockTurn::text(REPORTED),
    ]
}

async fn tracker(jira: WithJira) -> Option<StubJira> {
    match jira {
        WithJira::Absent => None,
        WithJira::Configured => {
            let site = StubJira::start().await;
            site.holds_issue(TICKET, "10001", "Done", "Done", 7).await;
            let answered = read_the_ticket(&site).await;
            assert_eq!(
                answered.0, 200,
                "this world's tracker has to answer the run's own read, or `configured` \
                 stands for a site nothing could have learnt a state from"
            );
            assert_eq!(
                answered.1["fields"]["status"]["name"], "Done",
                "and it has to hold {TICKET} in a state a leak would act on"
            );
            Some(site)
        }
        WithJira::RefusingEveryRead => {
            let site = StubJira::start().await;
            site.refuses_with(403).await;
            let answered = read_the_ticket(&site).await;
            assert_eq!(
                answered.0, 403,
                "this world's tracker has to refuse the run's own read, or it is a \
                 reachable site rather than a refusing one"
            );
            Some(site)
        }
    }
}

async fn read_the_ticket(site: &StubJira) -> (u16, serde_json::Value) {
    let answered = reqwest::get(format!("{}/rest/api/3/issue/{TICKET}", site.base_url()))
        .await
        .expect("a loopback stub answers");
    let status = answered.status().as_u16();
    let body = answered.json().await.expect("the stub answers JSON");
    (status, body)
}

fn declines() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call("c1", "read_file", json!({ "path": SOURCE })),
        MockTurn::text(DECLINED),
    ]
}

async fn run_cve(seed: &Path, jira: WithJira) -> Ran {
    run_scripted(seed, jira, migrates()).await
}

async fn run_scripted(seed: &Path, jira: WithJira, script: Vec<MockTurn>) -> Ran {
    let world = World::cloned_from(seed);
    let site = tracker(jira).await;
    let cancel = CancellationToken::new();

    let held = EffectContext::new(
        world.gh(),
        GitCli::new(
            PathBuf::from("git"),
            "ghp_never_reaches_a_network".to_string(),
            "FIDDLE_GITHUB_TOKEN",
            PATIENT,
        ),
        attempt_worktree(&world.workspace_root(), PROJECT, INVOCATION_REF),
        cancel.clone(),
    );
    let ctx = match &site {
        Some(site) => held.with_jira(client_for(site)),
        None => held,
    };

    let deployment = Deployment(DeploymentRule::Allow);
    let trace = Silent;
    let executor = Executor::new(
        CVE_MITIGATE,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        &trace,
        ReadRetry::none(),
    );
    let model = MockCompletionModel::new(script);
    let capability = CveMitigate::new(
        executor,
        &ctx,
        scanner_with(wiz_stub("library-only")),
        model.clone(),
        world.config(),
    );
    let grant = ExecutionGrant::authorise(
        &NextAction::Execute {
            capability_id: CVE_MITIGATE,
        },
        &AttemptId(ATTEMPT.to_string()),
    )
    .expect("an execute action authorises the capability it names");

    let evidence = capability
        .execute(ExecutionInput::unobserved(grant, WORK_ID, INVOCATION_REF))
        .await;

    Ran {
        outcome: Outcome {
            evidence: evidence.map_err(|why| why.to_string()),
            disposition: capability.disposition(),
            tree: capability.tree_observation(),
            receipts: capability.receipts(),
        },
        pull_request_title: world.pull_request_title(),
        pull_request_body: world.pull_request_body(),
        diff: world.diff(),
        commit_messages: world.commit_messages(),
        prompt: serde_json::to_string(&model.requests()).expect("a completion request serializes"),
    }
}

fn a_mitigation_really_happened(ran: &Ran, world: &str) {
    assert!(
        ran.outcome.evidence.is_ok(),
        "the {world} run has to reach an outcome, or the comparisons below hold \
         two failures equal and prove nothing: {:?}",
        ran.outcome.evidence
    );
    let disposition = ran
        .outcome
        .disposition
        .as_ref()
        .unwrap_or_else(|| panic!("the {world} run recorded no disposition to compare"));
    assert!(
        disposition.pull_request.is_some(),
        "the {world} run has to have opened a pull request, or the title and body \
         compared below are two absences: {disposition:?}"
    );
    assert!(
        ran.pull_request_body.contains(MITIGATED),
        "the {world} run's pull request body has to name the advisory it \
         mitigated, or the body is not the mitigation's own words: {}",
        ran.pull_request_body
    );
    assert!(
        ran.diff.contains("renamedName"),
        "the {world} run has to have published the edit the attempt made, or the \
         diff compared below is empty on both sides: {}",
        ran.diff
    );
}

fn agree(with: &Ran, without: &Ran, why: &str) {
    assert_eq!(
        with.outcome, without.outcome,
        "{why}: the evidence, the typed disposition, the tree observed and every \
         receipt must be what they are with no tracker in reach"
    );
    assert_eq!(
        with.pull_request_title, without.pull_request_title,
        "{why}: nor may a tracker reach the title a reviewer reads"
    );
    assert_eq!(
        with.pull_request_body, without.pull_request_body,
        "{why}: nor the body, which is where a ticket key or a ticket status would \
         first show itself"
    );
    assert_eq!(
        with.diff, without.diff,
        "{why}: the patch must not depend on a tracker"
    );
    assert_eq!(
        with.commit_messages, without.commit_messages,
        "{why}: nor the commit the mitigation wrote"
    );
    assert_eq!(
        with.prompt, without.prompt,
        "{why}: nor what the model was asked, which is where a ticket fact would \
         reach the decision without changing this run's output"
    );
}

#[tokio::test]
async fn a_run_without_jira_produces_the_identical_pull_request_and_outcome() {
    let seed = seed();
    let with = run_cve(&seed_repo(&seed), WithJira::Configured).await;
    let without = run_cve(&seed_repo(&seed), WithJira::Absent).await;

    a_mitigation_really_happened(&with, "tracker-holding");
    a_mitigation_really_happened(&without, "trackerless");

    agree(
        &with,
        &without,
        "requirement 22 keeps its without-requiring-Jira, and the tracker this run \
         could reach holds IDENT-1 in Done",
    );
}

#[tokio::test]
async fn no_ticket_state_reaches_a_mitigation_decision() {
    let seed = seed();
    let unreachable = run_cve(&seed_repo(&seed), WithJira::RefusingEveryRead).await;
    let absent = run_cve(&seed_repo(&seed), WithJira::Absent).await;

    a_mitigation_really_happened(&unreachable, "refusing-tracker");
    a_mitigation_really_happened(&absent, "trackerless");

    agree(
        &unreachable,
        &absent,
        "a tracker that refuses every read is one whose state cannot be known, and \
         a mitigation that reads none cannot tell that world from a trackerless \
         one; a difference here means a tracker read is on the decision path and \
         its refusal is being acted on",
    );
}

#[tokio::test]
async fn every_value_this_file_compares_moves_when_the_mitigation_decision_moves() {
    let seed = seed();
    let mitigating = run_cve(&seed_repo(&seed), WithJira::Absent).await;
    let declining = run_scripted(&seed_repo(&seed), WithJira::Absent, declines()).await;

    a_mitigation_really_happened(&mitigating, "trackerless");

    assert_ne!(
        mitigating.outcome, declining.outcome,
        "a run that decided to mitigate and a run that decided not to must not \
         share a typed outcome, or the equalities above hold two constants equal"
    );
    assert_ne!(
        mitigating.pull_request_title, declining.pull_request_title,
        "nor a title"
    );
    assert_ne!(
        mitigating.pull_request_body, declining.pull_request_body,
        "nor a body"
    );
    assert_ne!(mitigating.diff, declining.diff, "nor a patch");
    assert_ne!(
        mitigating.commit_messages, declining.commit_messages,
        "nor a commit"
    );
    assert_ne!(
        mitigating.prompt, declining.prompt,
        "nor what the model was asked and answered"
    );
}
