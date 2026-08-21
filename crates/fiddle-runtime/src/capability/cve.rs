use super::propose::COMMITTER;
use super::CapabilityError;
use crate::agent::{
    attempt_briefed, unaccounted, AgentBudget, Brief, RepairReport, ToolHost, ToolReceipts,
};
use crate::cve::attempts;
use crate::cve::dedup::{Local, Spawn};
use crate::effect::{Executor, IntegrationOperation};
use crate::evaluate::{Evaluation, RescanVerdict};
use crate::github::{
    find_labelled_pull_request, BlamedCheck, EnsureBranchPublished, EnsurePullRequest,
    EnsurePullRequestBody, GenuineFailure, SharedPullRequest,
};
use crate::workspace::{
    DeclaredCommand, FileEdit, Workspace, WorkspaceCommand, WorkspaceError, WorkspacePath,
};
use crate::{GhCli, GhError};
use async_trait::async_trait;
use fiddle_core::{AdvisoryId, CapabilityId, EffectKind, ProjectedFinding, ProposedEffect};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MIGRATION_PREAMBLE: &str = "\
You are making one change to one project. You can read its files, list them, \
replace a file's contents, and run the project's check. You cannot do anything \
else, and there is nothing outside the project you can reach.\n\
\n\
Work in small steps: read before you write, and run the check after you write. \
Change as few files as you can. When you are done — or when you are certain you \
cannot finish — reply with the structured report and nothing else. Report what \
you actually changed, whether or not it worked.";

const FINDINGS_FRAME: &str = "\
A dependency bump has already been applied to this project to clear the \
advisories below. It may have broken the build, and it may not have been enough. \
Here they are as the scanner reported them — what it found, the version that was \
in the project, and the version it says the fix is in. Those are its words, not \
ours.";

const SCOPE_RULES: &str = "\
Name every file you change, spelling each path the way the project spells \
it, and change every file you name. A file changed without being named, or \
named without being changed, refuses the whole attempt — so work in as few \
files as you can and keep the list exact. The bump above is already in the \
tree and is not yours to declare: if it needs no follow-up, change nothing \
and name no files.\n\
\n\
You are not asked to fix everything. If you cannot see what would clear an \
advisory, or clearing it would take a change you are not confident in, \
change nothing for it and report it as not attempted, saying what stopped \
you. That is an answer this run can use. A change made on a guess is not, \
and neither is a report that claims more than you did.";

const TASK: &str = "\
Read the project and work out what would clear each advisory above: whether the \
bump already did, and what else has to change if it did not. Make those changes, \
run the check, and then report — the files you changed, and one entry for every \
advisory you were shown.";

const FEEDBACK_FRAME: &str = "\
An earlier attempt on this project is already open, and the forge reports that \
its checks failed on the commit named below. Here is what the forge reports, \
check by check, with the log each one published. Those are its words, not ours. \
Read them before you change anything, because the change you make has to \
answer them.";

fn render(finding: &ProjectedFinding) -> String {
    let ProjectedFinding {
        cve,
        package,
        current,
        fixed_version,
        severity,
        package_type,
    } = finding;
    let fixed = fixed_version
        .as_deref()
        .filter(|version| !version.trim().is_empty())
        .unwrap_or("no published fix");
    format!(
        "- {} in {package} {current}, fixed in {fixed} ({severity:?}, {package_type:?} package)",
        cve.as_str()
    )
}

fn render_blame(check: &BlamedCheck) -> String {
    match &check.details_url {
        Some(url) => format!("- `{}` failed. Its log is at {url}", check.name),
        None => format!("- `{}` failed, and the forge published no log", check.name),
    }
}

fn feedback_task(failure: &GenuineFailure) -> String {
    let blamed: Vec<String> = failure.blamed.iter().map(render_blame).collect();
    format!(
        "{FEEDBACK_FRAME}\n\nThe checks ran against commit {}.\n\n{}",
        failure.head_sha,
        blamed.join("\n")
    )
}

fn migration_task(findings: &[&ProjectedFinding], failure: Option<&GenuineFailure>) -> String {
    let rendered: Vec<String> = findings.iter().map(|finding| render(finding)).collect();
    let mut sections = vec![FINDINGS_FRAME.to_string(), rendered.join("\n")];
    if let Some(failure) = failure {
        sections.push(feedback_task(failure));
    }
    sections.push(SCOPE_RULES.to_string());
    sections.push(TASK.to_string());
    sections.join("\n\n")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationBreach {
    pub unannounced: Vec<String>,

    pub unmet: Vec<String>,
}

impl std::fmt::Display for DeclarationBreach {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.unannounced.is_empty() {
            write!(
                f,
                "changed without declaring: {}",
                self.unannounced.join(", ")
            )?;
        }
        if !self.unmet.is_empty() {
            if !self.unannounced.is_empty() {
                write!(f, "; ")?;
            }
            write!(f, "declared without changing: {}", self.unmet.join(", "))?;
        }
        Ok(())
    }
}

pub fn undeclared(declared: &[String], touched: &[FileEdit]) -> Option<DeclarationBreach> {
    let declared: BTreeSet<&str> = declared.iter().map(String::as_str).collect();
    let touched_paths: BTreeSet<&str> = touched.iter().map(|edit| edit.path.as_str()).collect();

    let unannounced: Vec<String> = touched_paths
        .difference(&declared)
        .map(|path| path.to_string())
        .collect();
    let unmet: Vec<String> = declared
        .difference(&touched_paths)
        .map(|path| path.to_string())
        .collect();

    if unannounced.is_empty() && unmet.is_empty() {
        None
    } else {
        Some(DeclarationBreach { unannounced, unmet })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GroupStatus {
    Clean,

    NeedsWork { reason: NeedsWork },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NeedsWork {
    Undeclared(DeclarationBreach),

    CheckFailed { check: String },

    Unproved(RescanVerdict),
}

impl std::fmt::Display for NeedsWork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NeedsWork::Undeclared(breach) => write!(f, "{breach}"),
            NeedsWork::CheckFailed { check } => {
                write!(f, "`{check}` did not pass over the tree the attempt left")
            }
            NeedsWork::Unproved(verdict) => write!(f, "{}", unproved_sentence(verdict)),
        }
    }
}

fn unproved_sentence(verdict: &RescanVerdict) -> String {
    match verdict {
        RescanVerdict::Cleared => {
            "the rescan proved this group clean, so there is nothing to report".to_string()
        }
        RescanVerdict::NotCompared => {
            "no rescan was compared, so the repair is unproved".to_string()
        }
        RescanVerdict::Provisional(_) => {
            "the rescan ran at a different scanner version, so the comparison is provisional"
                .to_string()
        }
        RescanVerdict::NotObserved { array } => {
            format!("the rescan reported no `{array}` array at all, so it proved nothing about it")
        }
        RescanVerdict::StillReported(cves) => format!(
            "still reported after the bump: {}",
            cves.iter()
                .map(|cve| cve.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RescanVerdict::NewFinding(_) => {
            "the bump introduced a finding the input scan did not report".to_string()
        }
        RescanVerdict::Unreadable(why) => {
            format!("the rescan wrote a document this build cannot read: {why}")
        }
    }
}

impl GroupStatus {
    pub fn of(evaluation: &Evaluation, undeclared: Option<&DeclarationBreach>) -> GroupStatus {
        if let Some(breach) = undeclared {
            return GroupStatus::NeedsWork {
                reason: NeedsWork::Undeclared(breach.clone()),
            };
        }

        if let Some(failed) = evaluation.first_failure() {
            return GroupStatus::NeedsWork {
                reason: NeedsWork::CheckFailed {
                    check: failed.name.clone(),
                },
            };
        }

        match evaluation.rescan() {
            RescanVerdict::Cleared => GroupStatus::Clean,
            unproved => GroupStatus::NeedsWork {
                reason: NeedsWork::Unproved(unproved.clone()),
            },
        }
    }
}

pub struct MigrationConfig {
    pub check: WorkspaceCommand,

    pub commands: Arc<Vec<DeclaredCommand>>,

    pub command_timeout: Duration,

    pub budget: AgentBudget,

    pub cancel: CancellationToken,
}

#[derive(Debug)]
pub struct MigrationAttempt {
    pub report: RepairReport,

    pub changed: Vec<WorkspacePath>,

    pub undeclared: Option<DeclarationBreach>,
}

pub struct GroupMigration<M> {
    model: M,
    config: MigrationConfig,
    receipts: Arc<Mutex<ToolReceipts>>,
}

impl<M> GroupMigration<M> {
    pub fn new(model: M, config: MigrationConfig) -> Self {
        GroupMigration {
            model,
            config,
            receipts: Arc::new(Mutex::new(ToolReceipts::default())),
        }
    }

    pub fn receipts(&self) -> ToolReceipts {
        self.receipts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl<M> GroupMigration<M>
where
    M: rig_core::completion::CompletionModel + 'static,
{
    pub async fn migrate(
        &self,
        workspace: &Arc<Workspace>,
        findings: &[ProjectedFinding],
        failure: Option<&GenuineFailure>,
    ) -> Result<MigrationAttempt, CapabilityError> {
        let findings: Vec<&ProjectedFinding> = findings.iter().collect();
        let task = migration_task(&findings, failure);

        let bumped: Vec<String> = workspace
            .changed_files()?
            .into_iter()
            .map(|path| path.as_str().to_string())
            .collect();

        let host = ToolHost {
            workspace: Arc::clone(workspace),
            cancel: self.config.cancel.clone(),
            check: self.config.check.clone(),
            commands: Arc::clone(&self.config.commands),
            command_timeout: self.config.command_timeout,
            receipts: Arc::clone(&self.receipts),
        };

        let report = attempt_briefed(
            self.model.clone(),
            host,
            self.config.budget.clone(),
            Brief {
                preamble: MIGRATION_PREAMBLE,
                task: &task,
            },
        )
        .await?;

        let shown: Vec<&str> = findings
            .iter()
            .map(|finding| finding.cve.as_str())
            .collect();
        if let Some(failure) = unaccounted(&shown, &report.findings) {
            return Err(failure.into());
        }

        let edits = workspace.edits()?;
        let changed = edits.iter().map(|edit| edit.path.clone()).collect();
        let declared_by_the_run: Vec<String> =
            report.changed_files.iter().cloned().chain(bumped).collect();
        let breach = undeclared(&declared_by_the_run, &edits);

        Ok(MigrationAttempt {
            report,
            changed,
            undeclared: breach,
        })
    }
}

#[async_trait]
pub trait Git: Sync {
    async fn run(&self, args: &[&str]) -> Result<String, CapabilityError>;
}

pub struct InWorktree<'a> {
    workspace: &'a Workspace,
    timeout: Duration,
}

impl<'a> InWorktree<'a> {
    pub fn new(workspace: &'a Workspace, timeout: Duration) -> Self {
        InWorktree { workspace, timeout }
    }
}

#[async_trait]
impl Git for InWorktree<'_> {
    async fn run(&self, args: &[&str]) -> Result<String, CapabilityError> {
        let command = WorkspaceCommand {
            program: "git".to_string(),
            args: args.iter().map(|argument| argument.to_string()).collect(),
            timeout: self.timeout,
        };
        let result = self.workspace.run(&command).await?;
        match result.exit_code {
            0 => Ok(result.stdout),
            _ => Err(CapabilityError::Workspace(WorkspaceError::Git {
                command: args.join(" "),
                stderr: result.stderr,
            })),
        }
    }
}

pub struct InRepository {
    repository: PathBuf,
}

impl InRepository {
    pub fn new(repository: impl Into<PathBuf>) -> Self {
        InRepository {
            repository: repository.into(),
        }
    }
}

#[async_trait]
impl Git for InRepository {
    async fn run(&self, args: &[&str]) -> Result<String, CapabilityError> {
        let repository = self.repository.clone();
        let owned: Vec<String> = args.iter().map(|argument| argument.to_string()).collect();
        let spelled = owned.join(" ");
        let ran = tokio::task::spawn_blocking(move || {
            let borrowed: Vec<&str> = owned.iter().map(String::as_str).collect();
            Local.run("git", &borrowed, &repository)
        })
        .await
        .map_err(|joined| {
            CapabilityError::Workspace(WorkspaceError::Git {
                command: spelled.clone(),
                stderr: joined.to_string(),
            })
        })??;

        match ran.ok {
            true => Ok(ran.stdout),
            false => Err(CapabilityError::Workspace(WorkspaceError::Git {
                command: spelled,
                stderr: ran.stderr,
            })),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landed {
    Committed,

    Reverted,
}

pub async fn land<G>(
    git: &G,
    advisories: &[AdvisoryId],
    status: &GroupStatus,
    changed: &[WorkspacePath],
) -> Result<Landed, CapabilityError>
where
    G: Git + ?Sized,
{
    match lands_as(status) {
        Landed::Committed => {
            stage_and_commit(git, advisories, changed).await?;
            Ok(Landed::Committed)
        }
        Landed::Reverted => {
            revert(git, changed).await?;
            Ok(Landed::Reverted)
        }
    }
}

fn lands_as(status: &GroupStatus) -> Landed {
    match status {
        GroupStatus::Clean => Landed::Committed,
        GroupStatus::NeedsWork { .. } => Landed::Reverted,
    }
}

async fn stage_and_commit<G>(
    git: &G,
    advisories: &[AdvisoryId],
    changed: &[WorkspacePath],
) -> Result<(), CapabilityError>
where
    G: Git + ?Sized,
{
    if changed.is_empty() {
        return Err(CapabilityError::NothingProposed);
    }

    let paths: Vec<&str> = changed.iter().map(|path| path.as_str()).collect();
    let mut add = vec!["add", "-f", "--"];
    add.extend_from_slice(&paths);
    git.run(&add).await?;

    let subject = commit_subject(advisories);
    let body = commit_body(advisories);
    let mut commit: Vec<&str> = COMMITTER
        .iter()
        .flat_map(|setting| ["-c", setting])
        .collect();
    commit.extend(["commit", "-q", "-m", subject.as_str(), "-m", body.as_str()]);
    git.run(&commit).await?;
    Ok(())
}

async fn revert<G>(git: &G, changed: &[WorkspacePath]) -> Result<(), CapabilityError>
where
    G: Git + ?Sized,
{
    if changed.is_empty() {
        return Ok(());
    }
    let paths: Vec<&str> = changed.iter().map(|path| path.as_str()).collect();

    let mut probe = vec!["ls-tree", "-r", "--name-only", "-z", "HEAD", "--"];
    probe.extend_from_slice(&paths);
    let listed = git.run(&probe).await?;
    let committed: BTreeSet<&str> = listed.split('\0').filter(|it| !it.is_empty()).collect();

    let (edited, created): (Vec<&str>, Vec<&str>) = paths
        .iter()
        .copied()
        .partition(|path| committed.contains(path));

    if !edited.is_empty() {
        let mut checkout = vec!["checkout", "HEAD", "--"];
        checkout.extend_from_slice(&edited);
        git.run(&checkout).await?;
    }
    if !created.is_empty() {
        let mut clean = vec!["clean", "-f", "-q", "--"];
        clean.extend_from_slice(&created);
        git.run(&clean).await?;
    }
    Ok(())
}

fn commit_subject(advisories: &[AdvisoryId]) -> String {
    match advisories.len() {
        1 => "fix: mitigate 1 advisory".to_string(),
        many => format!("fix: mitigate {many} advisories"),
    }
}

fn commit_body(advisories: &[AdvisoryId]) -> String {
    advisories
        .iter()
        .map(|cve| format!("Fixes: {}", cve.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

pub const CVE_LABEL: &str = "security/cve";

pub const PUSHABLE_PREFIX: &str = "security/";

pub const BRANCH_STEM: &str = "security/cve-remediation-";

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Refusal {
    #[error(
        "pull request #{number} carries the shared label and its head branch \
         `{head}` is not under `{prefix}`, which is the only namespace this \
         capability may push to"
    )]
    HeadOutsideThePushablePrefix {
        number: u64,
        head: String,
        prefix: &'static str,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("the shared pull request could not be looked up: {0}")]
    Read(#[from] GhError),

    #[error("{0}")]
    Refused(#[from] Refusal),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Approved {
    Reuse {
        number: u64,
        branch: String,
        head_sha: String,
        base: String,
        duplicates: Vec<u64>,
    },

    Fresh {
        branch: String,
        base: String,
    },
}

impl Approved {
    pub fn branch(&self) -> &str {
        match self {
            Approved::Reuse { branch, .. } | Approved::Fresh { branch, .. } => branch,
        }
    }

    pub fn from(&self) -> String {
        match self {
            Approved::Reuse { branch, .. } => origin_ref(branch),
            Approved::Fresh { base, .. } => origin_ref(base),
        }
    }

    pub fn base(&self) -> &str {
        match self {
            Approved::Reuse { base, .. } | Approved::Fresh { base, .. } => base,
        }
    }

    pub fn pr_head(&self) -> Option<&str> {
        match self {
            Approved::Reuse { head_sha, .. } => Some(head_sha),
            Approved::Fresh { .. } => None,
        }
    }

    pub fn reused(&self) -> Option<u64> {
        match self {
            Approved::Reuse { number, .. } => Some(*number),
            Approved::Fresh { .. } => None,
        }
    }

    pub fn duplicates(&self) -> &[u64] {
        match self {
            Approved::Reuse { duplicates, .. } => duplicates,
            Approved::Fresh { .. } => &[],
        }
    }

    pub fn note(&self) -> Option<String> {
        duplicate_note(self.duplicates())
    }
}

fn duplicate_note(duplicates: &[u64]) -> Option<String> {
    if duplicates.is_empty() {
        return None;
    }
    let listed: Vec<String> = duplicates.iter().map(|it| format!("#{it}")).collect();
    Some(format!(
        "More than one open pull request carries `{CVE_LABEL}`. This run added to \
         the lowest-numbered one and opened nothing new; {} {} still open and \
         should be closed by hand.",
        listed.join(", "),
        match duplicates.len() {
            1 => "is",
            _ => "are",
        }
    ))
}

fn dated_branch(today: &str) -> String {
    format!("{BRANCH_STEM}{today}")
}

pub fn today_utc() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = match month_prime < 10 {
        true => month_prime + 3,
        false => month_prime - 9,
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

pub fn plan(
    found: Option<SharedPullRequest>,
    base: &str,
    today: &str,
) -> Result<Approved, Refusal> {
    let approved = match found {
        Some(shared) => Approved::Reuse {
            number: shared.number,
            branch: shared.head,
            head_sha: shared.head_sha,
            base: shared.base,
            duplicates: shared.duplicates,
        },
        None => Approved::Fresh {
            branch: dated_branch(today),
            base: base.to_string(),
        },
    };

    if !approved.branch().starts_with(PUSHABLE_PREFIX) {
        return Err(Refusal::HeadOutsideThePushablePrefix {
            number: approved.reused().unwrap_or_default(),
            head: approved.branch().to_string(),
            prefix: PUSHABLE_PREFIX,
        });
    }
    Ok(approved)
}

pub async fn plan_shared_pull_request(
    gh: &GhCli,
    repo: &str,
    base: &str,
    today: &str,
    cancel: &CancellationToken,
) -> Result<Approved, PlanError> {
    let found = find_labelled_pull_request(gh, repo, CVE_LABEL, cancel).await?;
    Ok(plan(found, base, today)?)
}

const REMOTE: &str = "origin";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttemptTree {
    BaseRevision,
    PrHead,
}

impl AttemptTree {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttemptTree::BaseRevision => "base_revision",
            AttemptTree::PrHead => "pr_head",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Checkout {
    AtBaseRevision {
        base_revision: String,
    },

    AtPullRequestHead {
        base_revision: String,
        pr_head: String,
    },
}

impl Checkout {
    pub fn revision(&self) -> &str {
        match self {
            Checkout::AtBaseRevision { base_revision } => base_revision,
            Checkout::AtPullRequestHead { pr_head, .. } => pr_head,
        }
    }

    pub fn base_revision(&self) -> &str {
        match self {
            Checkout::AtBaseRevision { base_revision }
            | Checkout::AtPullRequestHead { base_revision, .. } => base_revision,
        }
    }

    pub fn pr_head(&self) -> Option<&str> {
        match self {
            Checkout::AtBaseRevision { .. } => None,
            Checkout::AtPullRequestHead { pr_head, .. } => Some(pr_head),
        }
    }

    pub fn attempt_tree(&self) -> AttemptTree {
        match self {
            Checkout::AtBaseRevision { .. } => AttemptTree::BaseRevision,
            Checkout::AtPullRequestHead { .. } => AttemptTree::PrHead,
        }
    }

    pub fn observed(&self) -> serde_json::Value {
        serde_json::json!({
            "base_revision": self.base_revision(),
            "pr_head": self.pr_head(),
            "attempt_tree": self.attempt_tree().as_str(),
        })
    }
}

pub async fn check_out<G>(git: &G, approved: &Approved) -> Result<Checkout, CapabilityError>
where
    G: Git + ?Sized,
{
    fetch(git, approved.base()).await?;

    let Some(pr_head) = approved.pr_head() else {
        return Ok(Checkout::AtBaseRevision {
            base_revision: resolve(git, &approved.from()).await?,
        });
    };

    let base_revision = resolve(git, &origin_ref(approved.base())).await?;

    fetch(git, approved.branch()).await?;
    let pr_head = resolve(git, &format!("{pr_head}^{{commit}}")).await?;

    Ok(Checkout::AtPullRequestHead {
        base_revision,
        pr_head,
    })
}

fn origin_ref(branch: &str) -> String {
    format!("{REMOTE}/{branch}")
}

async fn fetch<G>(git: &G, branch: &str) -> Result<(), CapabilityError>
where
    G: Git + ?Sized,
{
    git.run(&[
        "fetch",
        "--no-tags",
        "--quiet",
        REMOTE,
        &format!("+refs/heads/{branch}:refs/remotes/{REMOTE}/{branch}"),
    ])
    .await
    .map(|_output| ())
}

async fn resolve<G>(git: &G, revision: &str) -> Result<String, CapabilityError>
where
    G: Git + ?Sized,
{
    let printed = git
        .run(&["rev-parse", "--verify", "--quiet", revision])
        .await?;
    Ok(printed.trim().to_string())
}

pub struct SharedPublication {
    pub repo: String,
    pub head_owner: String,
    pub title: String,
    pub summary: String,
    pub head_sha: String,
    pub attempts: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedWork {
    pub branch: String,
    pub head_sha: String,
    pub pull_request: u64,
}

pub async fn publish_shared_work(
    executor: &Executor<'_>,
    capability: CapabilityId,
    approved: &Approved,
    config: &SharedPublication,
) -> Result<SharedWork, CapabilityError> {
    let publish_branch = EnsureBranchPublished::new(
        config.repo.clone(),
        approved.branch().to_string(),
        config.head_sha.clone(),
    );
    let published = executor
        .execute(
            ProposedEffect {
                capability,
                kind: EffectKind::EnsureBranchPublished,
                target: publish_branch.target(),
                payload: publish_branch.payload(),
            },
            publish_branch,
        )
        .await?;

    let body = attempts::write(&shared_body(&config.summary, approved), config.attempts)?;

    let open = EnsurePullRequest::new(
        config.repo.clone(),
        config.head_owner.clone(),
        approved.branch().to_string(),
        approved.base().to_string(),
        config.title.clone(),
        body.clone(),
        false,
    )
    .labelled(vec![CVE_LABEL.to_string()]);
    let opened = executor
        .execute(
            ProposedEffect {
                capability,
                kind: EffectKind::EnsurePullRequest,
                target: open.target(),
                payload: open.payload(),
            },
            open,
        )
        .await?;

    let describe = EnsurePullRequestBody::new(config.repo.clone(), opened.value.number, body);
    executor
        .execute(
            ProposedEffect {
                capability,
                kind: EffectKind::EnsurePullRequestBody,
                target: describe.target(),
                payload: describe.payload(),
            },
            describe,
        )
        .await?;

    Ok(SharedWork {
        branch: published.value.branch,
        head_sha: published.value.sha,
        pull_request: opened.value.number,
    })
}

pub fn shared_body(summary: &str, approved: &Approved) -> String {
    match approved.note() {
        Some(note) => format!("{summary}\n\n{note}"),
        None => summary.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::{PackageType, Severity};

    #[test]
    fn the_calendar_arithmetic_agrees_with_the_calendar() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(59), (1970, 3, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(20_683), (2026, 8, 18));
    }

    #[test]
    fn today_renders_zero_padded_and_the_branch_is_under_the_pushable_prefix() {
        let today = today_utc();
        assert_eq!(today.len(), 10, "{today}");
        assert!(
            today.chars().enumerate().all(|(at, character)| match at {
                4 | 7 => character == '-',
                _ => character.is_ascii_digit(),
            }),
            "{today}"
        );
        assert!(dated_branch(&today).starts_with(PUSHABLE_PREFIX));
    }

    fn finding() -> ProjectedFinding {
        ProjectedFinding {
            cve: serde_json::from_value::<AdvisoryId>(serde_json::json!("CVE-2026-4242"))
                .expect("a canonical advisory id"),
            package: "golang.org/x/text".to_string(),
            current: "v0.3.7".to_string(),
            fixed_version: Some("v0.3.8".to_string()),
            severity: Severity::High,
            package_type: PackageType::Library,
        }
    }

    #[test]
    fn the_rendering_carries_all_six_fields_of_a_finding() {
        let task = migration_task(&[&finding()], None);
        for expected in [
            "CVE-2026-4242",
            "golang.org/x/text",
            "v0.3.7",
            "v0.3.8",
            "High",
            "Library",
        ] {
            assert!(
                task.contains(expected),
                "the projection's `{expected}` did not reach the prompt: {task}"
            );
        }
    }

    #[test]
    fn a_finding_with_no_published_fix_is_named_as_such() {
        let mut unfixed = finding();
        unfixed.fixed_version = None;
        let mut blank = finding();
        blank.fixed_version = Some("  ".to_string());

        for finding in [unfixed, blank] {
            let task = migration_task(&[&finding], None);
            assert!(
                task.contains("no published fix"),
                "an unfixed finding must not render an empty version: {task}"
            );
        }
    }

    #[test]
    fn what_is_fetched_and_what_is_resolved_are_one_ref() {
        let fresh = plan(None, "main", "20260817").expect("nothing open is not a refusal");

        assert_eq!(
            format!("refs/remotes/{}", origin_ref(fresh.base())),
            format!("refs/remotes/{REMOTE}/main")
        );
        assert_eq!(fresh.from(), origin_ref(fresh.base()));
        assert_eq!(fresh.from(), "origin/main");
    }

    #[test]
    fn the_composition_carries_the_scope_rules_and_no_mechanical_rule() {
        let task = migration_task(&[&finding()], None);
        for rule in ["refuses the whole attempt", "report it as not attempted"] {
            assert!(task.contains(rule), "`{rule}` is a scope rule: {task}");
        }
        for mechanical in ["go list -m", "go mod why", "at_least", "dedup", "fold"] {
            assert!(
                !task.contains(mechanical),
                "`{mechanical}` is decided in Rust and must not be in the prompt: {task}"
            );
        }
    }

    #[test]
    fn the_prompt_names_no_ecosystem_and_no_chosen_version() {
        let elsewhere = ProjectedFinding {
            cve: serde_json::from_value::<AdvisoryId>(serde_json::json!("CVE-2026-1234"))
                .expect("a canonical advisory id"),
            package: "urllib3".to_string(),
            current: "2.0.0".to_string(),
            fixed_version: Some("2.2.2".to_string()),
            severity: Severity::High,
            package_type: PackageType::Library,
        };
        let prompt = format!(
            "{MIGRATION_PREAMBLE}\n\n{}",
            migration_task(&[&elsewhere], None)
        );

        for word in [
            "Go",
            "go.mod",
            "go.sum",
            "golang",
            "module",
            "_test.go",
            "t.Skip",
            "Rust",
            "Cargo.toml",
            "requirements.txt",
            "package.json",
        ] {
            assert!(
                !prompt.contains(word),
                "the prompt must not name an ecosystem; found {word:?} in:\n{prompt}"
            );
        }
        for shown in ["urllib3", "2.0.0", "2.2.2"] {
            assert!(
                prompt.contains(shown),
                "the scanner's own words about the finding are still shown; \
                 {shown:?} missing from:\n{prompt}"
            );
        }
    }
}
