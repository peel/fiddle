mod support;

use fiddle_core::{
    AttemptId, DeploymentRule, EffectName, EvidenceRef, NextAction, CVE_MITIGATE, JIRA_ISSUE_FILED,
};
use fiddle_runtime::agent::AgentBudget;
use fiddle_runtime::capability::{
    attempt_worktree, Capability, Executed, ExecutionGrant, ExecutionInput,
};
use fiddle_runtime::cve::verdict::{Budget, TicketFiling, FILINGS_FILE};
use fiddle_runtime::effect::{EffectContext, EffectTrace, ExecutionStep, Executor, ReadRetry};
use fiddle_runtime::evaluate::{Check, Success};
use fiddle_runtime::workspace::WorkspaceCommand;
use fiddle_runtime::{CveMitigate, GhCli, GitCli, MitigateConfig, Redaction};
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use support::cve::{ask_git, every_fixture_grade, scanner_with, wiz_stub};
use support::stub_jira::{client_for, StubJira, TOKEN};
use support::{Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const REPO: &str = "peel/r";

const OWNER: &str = "peel";

const BASE: &str = "main";

const TODAY: &str = "2026-08-29";

const TITLE: &str = "fiddle: mitigate {advisories} reported advisories";

const WORK_ID: &str = "cve-sweep";

const ATTEMPT: &str = "01JCVEFILING000000000000M5";

const PATIENT: Duration = Duration::from_secs(120);

const FILING_PROJECT: &str = "SEC";

const LEDGER: &str = "SEC-1";

const ISSUE_TYPE: &str = "Task";

const REPORTED_CVE: &str = "CVE-2026-0001";

const LEGACY_LABEL: &str = "cve-upstream-blocked";

const SOURCE: &str = "main.go";

const SOURCE_BEFORE: &str =
    "package main\n\nfunc main() {\n\tlegacyName()\n}\n\nfunc legacyName() {}\n";

const SOURCE_AFTER: &str =
    "package main\n\nfunc main() {\n\trenamedName()\n}\n\nfunc renamedName() {}\n";

const REPORTED: &str = r#"{"changed_files":["main.go"],"summary":"applied the rename the bump needs","claimed_complete":true,"findings":[{"cve":"CVE-2026-0001","attempted":true,"note":"the bump reached this call site"}]}"#;

const NO_PULL_REQUEST_WAS_OPENED: &str = "no pull request was opened on this forge";

const NO_BRANCH_WAS_PUSHED: &str = "no branch under security/ reached this forge";

struct Silent;

impl EffectTrace for Silent {
    fn step(&self, _kind: &EffectName, _step: ExecutionStep) {}
}

struct Ran {
    evidence: Result<Executed, String>,
    receipts: Vec<EvidenceRef>,
    filings: Value,
    verdicts: Value,
    pull_request_body: String,
    diff: String,
}

impl Ran {
    fn tickets(&self) -> &[Value] {
        self.filings["tickets"]
            .as_array()
            .unwrap_or_else(|| panic!("this run attempted no filing: {}", self.filings))
    }

    fn only_ticket(&self) -> &Value {
        let tickets = self.tickets();
        assert_eq!(
            tickets.len(),
            1,
            "one advisory carrying a legacy label proposes one ticket, and this run wrote \
             {} of them: {}",
            tickets.len(),
            self.filings
        );
        &tickets[0]
    }
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

    fn config(&self, filing: Option<TicketFiling>) -> MitigateConfig {
        let rescan = wiz_stub("library-only");
        MitigateConfig {
            repo: REPO.to_string(),
            head_owner: OWNER.to_string(),
            base: BASE.to_string(),
            title: TITLE.to_string(),
            project: PROJECT.to_string(),
            stub_root: self.stub_root(),
            tree: self.tree(),
            workspace_root: self.workspaces.path().to_path_buf(),
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
            filing,
            cancel: CancellationToken::new(),
        }
    }

    fn report(&self, file: &str) -> Value {
        let path = self.reports.path().join(file);
        let held = std::fs::read_to_string(&path)
            .unwrap_or_else(|why| panic!("the run wrote no {}: {why}", path.display()));
        serde_json::from_str(&held)
            .unwrap_or_else(|why| panic!("{} is not JSON: {why}", path.display()))
    }

    fn created(&self) -> Option<Value> {
        let key = format!("POST_repos_{}_pulls", REPO.replace('/', "_"));
        std::fs::read_to_string(self.stub.path().join("world"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|landed| landed["key"].as_str() == Some(key.as_str()))
            .and_then(|landed| serde_json::from_str::<Value>(landed["body"].as_str()?).ok())
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

fn files_into_the_ledger() -> TicketFiling {
    TicketFiling {
        project_key: FILING_PROJECT.to_string(),
        issue_type: ISSUE_TYPE.to_string(),
        ledger_issue: LEDGER.to_string(),
    }
}

async fn a_site_holding_the_ledger() -> StubJira {
    let site = StubJira::start().await;
    site.holds_anchor_issue(LEDGER).await;
    assert_eq!(
        site.issue_keys().await,
        vec![LEDGER.to_string()],
        "the ledger is the only issue this site holds before a run, so every later key is \
         one a run created"
    );
    site
}

async fn run(seed: &Path, site: Option<&StubJira>, filing: Option<TicketFiling>) -> Ran {
    let world = World::cloned_from(seed);
    let cancel = CancellationToken::new();

    let held = EffectContext::new(
        world.gh(),
        GitCli::new(
            PathBuf::from("git"),
            "ghp_never_reaches_a_network".to_string(),
            "FIDDLE_GITHUB_TOKEN",
            PATIENT,
        ),
        attempt_worktree(world.workspaces.path(), PROJECT, INVOCATION_REF),
        cancel.clone(),
    );
    let ctx = match site {
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
    let capability = CveMitigate::new(
        executor,
        &ctx,
        scanner_with(wiz_stub("library-only")),
        MockCompletionModel::new(migrates()),
        world.config(filing),
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
        evidence: evidence.map_err(|why| why.to_string()),
        receipts: capability.receipts(),
        filings: world.report(FILINGS_FILE),
        verdicts: world.report(fiddle_runtime::cve::verdict::REPORT_FILE),
        pull_request_body: world.pull_request_body(),
        diff: world.diff(),
    }
}

fn a_repair_really_landed(ran: &Ran, world: &str) {
    assert!(
        ran.evidence.is_ok(),
        "the {world} run has to reach an outcome, or every count below is taken from a run \
         that stopped before it filed anything: {:?}",
        ran.evidence
    );
    assert!(
        ran.diff.contains("renamedName"),
        "the {world} run has to have published the edit the attempt made, or it never \
         reached the point where a verdict is written: {}",
        ran.diff
    );
    assert!(
        ran.pull_request_body.contains(REPORTED_CVE),
        "the {world} run has to have opened a pull request naming the advisory: {}",
        ran.pull_request_body
    );
    assert_eq!(
        ran.verdicts[0]["legacy_label"], "upstream-blocked",
        "the ticket this bean files comes from a verdict row carrying a legacy label, and a \
         run whose verdicts carry none proposes nothing at all: {}",
        ran.verdicts
    );
}

async fn tickets_created(site: &StubJira) -> Vec<String> {
    site.issue_keys()
        .await
        .into_iter()
        .filter(|key| key != LEDGER)
        .collect()
}

#[tokio::test]
async fn a_verdict_carrying_a_legacy_label_files_one_ticket_and_a_second_run_files_none() {
    let seed = seed();
    let site = a_site_holding_the_ledger().await;

    let first = run(
        &seed_repo(&seed),
        Some(&site),
        Some(files_into_the_ledger()),
    )
    .await;
    a_repair_really_landed(&first, "first");

    let filed = first.only_ticket();
    assert_eq!(filed["state"], "filed", "the first run files the ticket");
    assert_eq!(filed["cve"], REPORTED_CVE);
    assert_eq!(filed["issue"], "SEC-2");
    assert_eq!(
        tickets_created(&site).await,
        vec!["SEC-2".to_string()],
        "one create reached the site"
    );
    assert_eq!(
        site.create_requests().await,
        1,
        "and it was sent once, not retried"
    );

    let created = site.last_create().await;
    assert_eq!(
        created["fields"]["project"]["key"], FILING_PROJECT,
        "the ticket is filed in the project the filing configuration names, which is not the \
         project the deployment observes work items in: {created}"
    );
    assert_eq!(
        created["fields"]["issuetype"]["name"], ISSUE_TYPE,
        "and it carries the issue type that configuration names, which the site requires: \
         {created}"
    );
    assert_eq!(
        created["fields"]["labels"],
        json!([LEGACY_LABEL, filed["marker"]]),
        "the label a person reads comes from the verdict row's legacy label, and the marker \
         beside it is the identity the next run recognises: {created}"
    );

    let second = run(
        &seed_repo(&seed),
        Some(&site),
        Some(files_into_the_ledger()),
    )
    .await;
    a_repair_really_landed(&second, "second");

    let again = second.only_ticket();
    assert_eq!(
        again["marker"], filed["marker"],
        "the marker is derived from the project, the invocation reference and the advisory, \
         so two runs over one verdict propose one identity"
    );
    assert_eq!(
        again["issue"], "SEC-2",
        "the second run answers with the ticket the first one filed"
    );
    assert_eq!(
        tickets_created(&site).await,
        vec!["SEC-2".to_string()],
        "and the site still holds exactly one ticket beside the ledger"
    );
    assert_eq!(
        site.create_requests().await,
        1,
        "the second run sent no create at all: a create the site refused would count here, \
         so this separates `filed nothing` from `tried and was turned away`"
    );
    assert!(
        second
            .receipts
            .contains(&EvidenceRef(format!("cve:{JIRA_ISSUE_FILED}:SEC-2"))),
        "both runs report the same ticket as evidence: {:?}",
        second.receipts
    );
}

#[tokio::test]
async fn the_claim_is_the_only_thing_standing_between_one_ticket_and_two() {
    let seed = seed();
    let site = a_site_holding_the_ledger().await;
    site.withholds_new_issues_from_search().await;

    let first = run(
        &seed_repo(&seed),
        Some(&site),
        Some(files_into_the_ledger()),
    )
    .await;
    a_repair_really_landed(&first, "first");
    let marker = first.only_ticket()["marker"]
        .as_str()
        .expect("a filed ticket names its marker")
        .to_string();
    assert_eq!(tickets_created(&site).await, vec!["SEC-2".to_string()]);
    assert_eq!(
        site.all_search_matches(&format!("project = {FILING_PROJECT} AND labels = {marker}"))
            .await
            .len(),
        0,
        "this world is inside the indexing lag: the ticket exists and no search can find it,          so the claim on the ledger is the only reader of what the first run did"
    );

    let held = run(
        &seed_repo(&seed),
        Some(&site),
        Some(files_into_the_ledger()),
    )
    .await;
    a_repair_really_landed(&held, "claim-holding");
    assert_eq!(
        held.only_ticket()["issue"],
        "SEC-2",
        "a second run reads the claim and answers with the ticket the first one filed"
    );
    assert_eq!(
        tickets_created(&site).await,
        vec!["SEC-2".to_string()],
        "one ticket"
    );

    let removed = site.delete_issue_property(LEDGER, &marker).await;
    assert_eq!(
        removed.status, 204,
        "this control only measures anything if the claim really left the ledger"
    );
    assert_eq!(
        site.get_issue_property(LEDGER, &marker).await.status,
        404,
        "and the ledger must now answer as though no run had ever claimed the marker"
    );

    let bare = run(
        &seed_repo(&seed),
        Some(&site),
        Some(files_into_the_ledger()),
    )
    .await;
    a_repair_really_landed(&bare, "claimless");
    assert_eq!(
        bare.only_ticket()["issue"],
        "SEC-3",
        "the claim was the only thing removed between this run and the one above, and the          count moved. Without this case `a_verdict_carrying_a_legacy_label_files_one_ticket_\
         and_a_second_run_files_none` could be counting something no run can move"
    );
    assert_eq!(
        tickets_created(&site).await,
        vec!["SEC-2".to_string(), "SEC-3".to_string()],
        "two tickets"
    );
    assert_eq!(site.create_requests().await, 2);
}

#[tokio::test]
async fn a_deployment_with_no_jira_table_completes_the_run_and_files_nothing() {
    let seed = seed();
    let site = a_site_holding_the_ledger().await;

    let ran = run(&seed_repo(&seed), None, None).await;
    a_repair_really_landed(&ran, "trackerless");

    assert_eq!(
        ran.filings,
        json!({"filing": "not_configured"}),
        "a deployment that configures no filing records that it configured none, which is a \
         different fact from `it filed nothing this time`"
    );
    assert!(
        ran.filings["tickets"].is_null(),
        "and it offers no ticket list for a reader to count as zero: {}",
        ran.filings
    );
    assert_eq!(
        site.request_lines().await,
        Vec::<String>::new(),
        "no request of any kind reached a site during this run"
    );
    assert_eq!(site.create_requests().await, 0);
    assert_eq!(tickets_created(&site).await, Vec::<String>::new());
}

#[tokio::test]
async fn a_reachable_site_with_no_filing_configured_files_nothing() {
    let seed = seed();
    let site = a_site_holding_the_ledger().await;

    let ran = run(&seed_repo(&seed), Some(&site), None).await;
    a_repair_really_landed(&ran, "reading");

    assert_eq!(
        ran.filings,
        json!({"filing": "not_configured"}),
        "reaching a site is not being told where to file, so a `[jira]` table naming no \
         filing files nothing"
    );
    assert_eq!(site.create_requests().await, 0);
    assert_eq!(tickets_created(&site).await, Vec::<String>::new());
}

#[tokio::test]
async fn a_create_the_site_refuses_is_named_as_a_filing_refusal_and_the_repair_still_lands() {
    let seed = seed();
    let site = a_site_holding_the_ledger().await;
    site.refuses_the_create_with(403).await;

    let ran = run(
        &seed_repo(&seed),
        Some(&site),
        Some(files_into_the_ledger()),
    )
    .await;
    a_repair_really_landed(&ran, "refused");

    let refused = ran.only_ticket();
    assert_eq!(
        refused["state"], "refused",
        "a filing the site refused is its own state: {}",
        ran.filings
    );
    assert_eq!(refused["cve"], REPORTED_CVE);
    let why = refused["why"].as_str().expect("a refusal says why");
    assert!(
        why.contains("403"),
        "and it carries the site's own refusal rather than a write error: {why}"
    );
    assert!(
        !why.contains(FILINGS_FILE) && !why.contains("verdicts.json"),
        "a Jira refusal has no path and no io::Error, so it must never read as a failure to \
         write a report file: {why}"
    );
    assert_eq!(
        site.create_requests().await,
        1,
        "the run reached the create and was turned away; a run that never proposed one would \
         count 0 here, and the two must not read alike"
    );
    assert_eq!(
        tickets_created(&site).await,
        Vec::<String>::new(),
        "and nothing was filed"
    );
    assert!(
        ran.evidence.is_ok(),
        "a tracker that will not take the ticket does not undo a repair that landed: the \
         pull request is open and the verdicts are written, and failing the run here would \
         report both as not done: {:?}",
        ran.evidence
    );
    assert!(
        !ran.receipts
            .iter()
            .any(|receipt| receipt.0.contains(JIRA_ISSUE_FILED)),
        "a refused filing earns no receipt: {:?}",
        ran.receipts
    );
    assert!(
        ran.receipts
            .contains(&EvidenceRef(format!("cve:{FILINGS_FILE}"))),
        "the record of the refusal is itself evidence: {:?}",
        ran.receipts
    );
}

#[tokio::test]
async fn a_ledger_the_site_does_not_hold_is_named_and_no_create_is_sent() {
    let seed = seed();
    let site = StubJira::start().await;
    assert_eq!(
        site.issue_keys().await,
        Vec::<String>::new(),
        "this site holds no ledger issue at all"
    );

    let ran = run(
        &seed_repo(&seed),
        Some(&site),
        Some(files_into_the_ledger()),
    )
    .await;
    a_repair_really_landed(&ran, "ledgerless");

    let refused = ran.only_ticket();
    assert_eq!(refused["state"], "refused");
    let why = refused["why"].as_str().expect("a refusal says why");
    assert!(
        why.contains(LEDGER),
        "the refusal names the ledger issue a person has to create: {why}"
    );
    assert_eq!(
        site.create_requests().await,
        0,
        "no create was sent, which is what tells this refusal from the one a refused create \
         produces"
    );
}

#[tokio::test]
async fn the_credential_never_reaches_the_filing_report_through_a_quoted_refusal() {
    let seed = seed();
    let site = a_site_holding_the_ledger().await;
    site.refuses_with_body(
        400,
        &json!({"errorMessages": [format!("the site echoed the credential {TOKEN} back")]})
            .to_string(),
    )
    .await;

    let ran = run(
        &seed_repo(&seed),
        Some(&site),
        Some(files_into_the_ledger()),
    )
    .await;
    a_repair_really_landed(&ran, "echoing");

    let why = ran.only_ticket()["why"]
        .as_str()
        .expect("a refusal says why")
        .to_string();
    assert!(
        why.contains("[redacted]"),
        "the adapter quotes what the site said, so the report on the disk is a surface \
         remote text reaches: {why}"
    );
    assert!(
        !why.contains(TOKEN),
        "and the credential must not be one of the things it carries: {why}"
    );
    assert!(
        !ran.filings.to_string().contains(TOKEN),
        "nor anywhere else in the document: {}",
        ran.filings
    );
}
