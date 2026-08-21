use super::cve::{
    check_out, land, plan_shared_pull_request, publish_shared_work, Approved, Checkout, Git,
    GroupMigration, GroupStatus, InRepository, InWorktree, MigrationConfig, PlanError,
    SharedPublication,
};
use super::{Capability, CapabilityError, ExecutionGrant};
use crate::agent::AgentBudget;
use crate::cve::attempts;
use crate::cve::dedup::commit_log_dedup;
use crate::cve::project::{project, Projection};
use crate::cve::verdict::{
    disposition, Attempted, BoundReached, Budget, Disposition, InProgress, Run,
};
use crate::effect::{EffectContext, Executor, IntegrationOperation};
use crate::evaluate::{evaluate, Check, Contract, Evaluation, InWorkspace, Repair, Rescan};
use crate::github::{
    observe_genuine_failure, read_pull_request_body, EnsurePullRequestBody, GenuineFailure,
};
use crate::scanner::{ScanReport, Scanner, WizCredential, WizLogin};
use crate::workspace::{Workspace, WorkspaceCommand, WorkspaceError};
use fiddle_core::{
    correlation_key, AdvisoryId, AttemptId, CapabilityId, ChangeSetState, EffectKind, EvidenceRef,
    Observation, ProjectedFinding, ProposedEffect, RunDisposition, Severities, TreeObservation,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const CVE_ORIGIN: &str = "cve";

pub struct MitigateConfig {
    pub repo: String,

    pub head_owner: String,

    pub base: String,

    pub title: String,

    pub project: String,

    pub stub_root: PathBuf,

    pub tree: PathBuf,

    pub workspace_root: PathBuf,

    pub image: String,

    pub severities: Severities,

    pub scratch: PathBuf,

    pub rescan_login: WizLogin,

    pub rescan_credential: WizCredential,

    pub checks: Vec<Check>,

    pub check: WorkspaceCommand,

    pub budget: AgentBudget,

    pub command_timeout: Duration,

    pub findings: Budget,

    pub max_attempts: u32,

    pub report_dir: PathBuf,

    pub today: String,

    pub cancel: CancellationToken,
}

struct Counted {
    number: u64,
    held: String,
    spent: u32,
}

enum Feedback {
    NoCandidate,
    Blaming(GenuineFailure),
    BlamingNothing,
    Unreadable { why: String },
}

impl Feedback {
    fn attempts_afresh(&self) -> bool {
        !matches!(self, Feedback::BlamingNothing | Feedback::Unreadable { .. })
    }

    fn blamed(&self) -> Option<&GenuineFailure> {
        match self {
            Feedback::Blaming(failure) => Some(failure),
            Feedback::NoCandidate | Feedback::BlamingNothing | Feedback::Unreadable { .. } => None,
        }
    }

    fn unreadable(&self) -> Option<&str> {
        match self {
            Feedback::Unreadable { why } => Some(why),
            Feedback::NoCandidate | Feedback::Blaming(_) | Feedback::BlamingNothing => None,
        }
    }
}

#[derive(Default)]
struct Observed {
    receipts: Vec<EvidenceRef>,
    tree: Option<TreeObservation>,
    disposition: Option<RunDisposition>,
}

pub struct CveMitigate<'a, M, S> {
    executor: Executor<'a>,
    context: &'a EffectContext,
    scanner: S,
    config: MitigateConfig,
    observed: Mutex<Observed>,
    migration: GroupMigration<M>,
}

impl<'a, M, S> CveMitigate<'a, M, S>
where
    M: rig_core::completion::CompletionModel + 'static,
    S: Scanner + Send + Sync,
{
    pub fn new(
        executor: Executor<'a>,
        context: &'a EffectContext,
        scanner: S,
        model: M,
        config: MitigateConfig,
    ) -> Self {
        let migration = GroupMigration::new(
            model.clone(),
            MigrationConfig {
                check: config.check.clone(),
                budget: config.budget.clone(),
                cancel: config.cancel.clone(),
            },
        );
        CveMitigate {
            executor,
            context,
            scanner,
            config,
            observed: Mutex::new(Observed::default()),
            migration,
        }
    }

    fn worktree(&self) -> PathBuf {
        super::attempt_worktree(
            &self.config.workspace_root,
            &self.config.project,
            self.executor.invocation_ref(),
        )
    }

    async fn sweep(&self, report: &ScanReport) -> Result<Run, CapabilityError> {
        let projection = project(report, &self.config.severities)?;

        let approved = plan_shared_pull_request(
            &self.context.gh,
            &self.config.repo,
            &self.config.base,
            &self.config.today,
            &self.config.cancel,
        )
        .await?;

        let counted = self.counted(&approved).await?;

        if let Some(reached) = self.bound_reached(counted.as_ref()) {
            let mut run = Run::scanned(projection);
            run.bound_reached = Some(reached);
            return Ok(run);
        }

        let feedback = self.feedback(&approved).await;

        if let Some(why) = feedback.unreadable() {
            let mut run = Run::scanned(projection);
            run.checks_unreadable = Some(why.to_string());
            return Ok(run);
        }

        let checkout = check_out(&InRepository::new(&self.config.tree), &approved).await?;
        self.observed.lock().unwrap().tree = Some(observed_tree(&checkout, report));

        let worktree = self.worktree();
        let root = worktree
            .parent()
            .unwrap_or(&self.config.workspace_root)
            .to_path_buf();
        let name = AttemptId(
            worktree
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
        );
        let workspace = Arc::new(Workspace::create_at(
            &self.config.tree,
            &root,
            &name,
            checkout.revision(),
            self.config.cancel.clone(),
        )?);
        let git = InWorktree::new(&workspace, self.config.budget.tool_timeout);

        let fixed = commit_log_dedup(workspace.root(), &self.config.base)?;

        let (taken, deferred) = self
            .config
            .findings
            .apply(projection.all().cloned().collect());

        let mut settled: Vec<AdvisoryId> = Vec::new();
        let mut attempted: Vec<Attempted> = Vec::new();
        if feedback.attempts_afresh() && !taken.is_empty() {
            let attempt = self
                .migration
                .migrate(&workspace, &taken, feedback.blamed())
                .await?;
            let evaluation = self.judge(&workspace, &taken, &projection, report).await?;
            let status = GroupStatus::of(&evaluation, attempt.undeclared.as_ref());
            let advisories = advisories_of(&taken);
            let group = Attempted {
                findings: taken,
                status,
                attempt,
            };
            match group.settled() {
                true => settled = advisories,
                false => {
                    land(&git, &advisories, &group.status, &group.attempt.changed).await?;
                }
            }
            attempted.push(group);
        }

        let spent = counted.as_ref().map_or(0, |it| it.spent);
        let landed = match attempted.iter().any(Attempted::committed) {
            false => None,
            true => Some(self.publish(&git, &approved, &attempted, spent + 1).await?),
        };
        if landed.is_none() && !attempted.is_empty() {
            self.record_attempt(counted.as_ref()).await?;
        }

        let in_progress = approved.reused().map(|number| InProgress {
            number,
            covers: projection
                .all()
                .filter(|finding| fixed.names(finding.cve.as_str()))
                .map(|finding| finding.cve.clone())
                .collect(),
        });

        let mut run = Run::scanned(projection);
        run.already_fixed = settled;
        run.in_progress = in_progress;
        run.attempted = attempted;
        run.deferred = deferred;
        run.landed = landed;
        Ok(run)
    }

    async fn counted(&self, approved: &Approved) -> Result<Option<Counted>, CapabilityError> {
        let Some(number) = approved.reused() else {
            return Ok(None);
        };
        let held = read_pull_request_body(self.context, &self.config.repo, number)
            .await
            .map_err(PlanError::Read)?;
        let spent = attempts::read(&held)?;
        Ok(Some(Counted {
            number,
            held,
            spent,
        }))
    }

    fn bound_reached(&self, counted: Option<&Counted>) -> Option<BoundReached> {
        counted
            .filter(|it| it.spent >= self.config.max_attempts)
            .map(|it| BoundReached {
                number: it.number,
                spent: it.spent,
                bound: self.config.max_attempts,
            })
    }

    async fn record_attempt(&self, counted: Option<&Counted>) -> Result<(), CapabilityError> {
        let Some(counted) = counted else {
            return Ok(());
        };
        let body = attempts::write(&counted.held, counted.spent + 1)?;
        let describe = EnsurePullRequestBody::new(self.config.repo.clone(), counted.number, body);
        self.executor
            .execute(
                ProposedEffect {
                    capability: self.id(),
                    kind: EffectKind::EnsurePullRequestBody,
                    target: describe.target(),
                    payload: describe.payload(),
                },
                describe,
            )
            .await?;
        Ok(())
    }

    async fn feedback(&self, approved: &Approved) -> Feedback {
        let Some(candidate) = approved.pr_head() else {
            return Feedback::NoCandidate;
        };
        let observed = observe_genuine_failure(
            &self.context.gh,
            &self.config.repo,
            candidate,
            &self.config.cancel,
        )
        .await;
        match observed {
            Observation::Available {
                value: Some(failure),
                ..
            } => Feedback::Blaming(failure),
            Observation::Unavailable { source, reason } => Feedback::Unreadable {
                why: format!("{}: {reason}", source.0),
            },
            Observation::Available { value: None, .. } | Observation::NotApplicable { .. } => {
                Feedback::BlamingNothing
            }
        }
    }

    async fn judge(
        &self,
        workspace: &Workspace,
        shown: &[ProjectedFinding],
        projection: &Projection,
        report: &ScanReport,
    ) -> Result<Evaluation, CapabilityError> {
        let contract = Contract {
            checks: self.config.checks.clone(),
            severities: self.config.severities.clone(),
            repair: Some(Repair {
                must_clear: advisories_of(shown),
                input: projection
                    .all()
                    .map(|finding| finding.cve.clone())
                    .collect(),
                scanned_at: report.scanner_version.clone(),
            }),
        };
        let tree = InWorkspace::new(
            workspace,
            self.config.command_timeout,
            Rescan {
                scratch: self.config.scratch.clone(),
                login: self.config.rescan_login.clone(),
                credential: self.config.rescan_credential.clone(),
                image: self.config.image.clone(),
            },
        );
        evaluate(&contract, &tree)
            .await
            .map_err(|_cancelled| CapabilityError::Workspace(WorkspaceError::Cancelled))
    }

    async fn publish(
        &self,
        git: &InWorktree<'_>,
        approved: &Approved,
        attempted: &[Attempted],
        attempts: u32,
    ) -> Result<crate::cve::verdict::Landed, CapabilityError> {
        let head_sha = git.run(&["rev-parse", "HEAD"]).await?.trim().to_string();
        let shared = publish_shared_work(
            &self.executor,
            self.id(),
            approved,
            &SharedPublication {
                repo: self.config.repo.clone(),
                head_owner: self.config.head_owner.clone(),
                title: self.config.title.clone(),
                summary: summary_of(attempted),
                head_sha,
                attempts,
            },
        )
        .await?;

        let mut observed = self.observed.lock().unwrap();
        observed.receipts.push(EvidenceRef(format!(
            "{CVE_ORIGIN}:{}/tree/{}",
            self.config.repo, shared.branch
        )));
        observed.receipts.push(EvidenceRef(format!(
            "{CVE_ORIGIN}:{}/pull/{}",
            self.config.repo, shared.pull_request
        )));
        Ok(crate::cve::verdict::Landed {
            branch: shared.branch,
            pull_request: shared.pull_request,
        })
    }

    fn record_change_set(&self, work_id: &str) -> Result<(), CapabilityError> {
        let state = ChangeSetState {
            marker: Some(correlation_key(
                &self.config.project,
                self.executor.invocation_ref(),
            )),
        };
        let destination = self
            .config
            .stub_root
            .join(format!("changes/{work_id}.json"));
        super::stub::write_atomically(&destination, &state).map_err(|source| {
            CapabilityError::Write {
                path: destination.clone(),
                source,
            }
        })
    }
}

#[async_trait::async_trait]
impl<M, S> Capability for CveMitigate<'_, M, S>
where
    M: rig_core::completion::CompletionModel + 'static,
    S: Scanner + Send + Sync,
{
    fn id(&self) -> CapabilityId {
        fiddle_core::CVE_MITIGATE
    }

    fn stage(&self) -> &'static str {
        "mitigate"
    }

    async fn execute(
        &self,
        grant: ExecutionGrant,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        if grant.capability_id() != self.id() {
            return Err(CapabilityError::NotAuthorised {
                granted: grant.capability_id(),
                requested: self.id(),
            });
        }
        if invocation_ref != self.executor.invocation_ref()
            || self.config.project != self.executor.project()
        {
            return Err(CapabilityError::Misbound {
                bound: format!(
                    "{}/{}",
                    self.executor.project(),
                    self.executor.invocation_ref()
                ),
                asked: format!("{}/{invocation_ref}", self.config.project),
            });
        }
        let worktree = self.worktree();
        if self.context.work != worktree {
            return Err(CapabilityError::PublishesElsewhere {
                publishing: self.context.work.clone(),
                working: worktree,
            });
        }

        let scanned = self.scanner.scan(&self.config.image).await;
        let run = match &scanned {
            Ok(report) => self.sweep(report).await?,
            Err(why) => Run::unusable(why.to_string()),
        };

        let concluded = disposition(&run);
        self.observed.lock().unwrap().disposition = Some(concluded.published());
        self.publish_reports(&concluded)?;
        if let Err(why) = scanned {
            return Err(CapabilityError::Scan(why));
        }
        if let Some(why) = &run.checks_unreadable {
            return Err(CapabilityError::ChecksUnreadable(why.clone()));
        }

        self.record_change_set(work_id)?;
        Ok(EvidenceRef(format!(
            "{CVE_ORIGIN}:{}:{}",
            concluded.verdicts().len(),
            grant.attempt_id().0
        )))
    }

    fn receipts(&self) -> Vec<EvidenceRef> {
        self.observed.lock().unwrap().receipts.clone()
    }

    fn tree_observation(&self) -> Option<TreeObservation> {
        self.observed.lock().unwrap().tree.clone()
    }

    fn disposition(&self) -> Option<RunDisposition> {
        self.observed.lock().unwrap().disposition.clone()
    }
}

impl<M, S> CveMitigate<'_, M, S> {
    fn publish_reports(&self, concluded: &Disposition) -> Result<(), CapabilityError> {
        self.write_verdicts(concluded)?;
        self.write_findings(concluded)
    }

    fn write_verdicts(&self, concluded: &Disposition) -> Result<(), CapabilityError> {
        concluded
            .write_report(&self.config.report_dir)
            .map_err(|source| self.refuse(crate::cve::verdict::REPORT_FILE, source))?;
        self.receipt(crate::cve::verdict::REPORT_FILE);
        Ok(())
    }

    fn write_findings(&self, concluded: &Disposition) -> Result<(), CapabilityError> {
        concluded
            .write_findings(&self.config.report_dir)
            .map_err(|source| self.refuse(crate::cve::verdict::FINDINGS_FILE, source))?;
        self.receipt(crate::cve::verdict::FINDINGS_FILE);
        Ok(())
    }

    fn refuse(&self, file: &str, source: std::io::Error) -> CapabilityError {
        CapabilityError::Write {
            path: self.config.report_dir.join(file),
            source,
        }
    }

    fn receipt(&self, file: &str) {
        self.observed
            .lock()
            .unwrap()
            .receipts
            .push(EvidenceRef(format!("{CVE_ORIGIN}:{file}")));
    }
}

fn observed_tree(checkout: &Checkout, report: &ScanReport) -> TreeObservation {
    TreeObservation {
        base_revision: checkout.base_revision().to_string(),
        pr_head: checkout.pr_head().map(str::to_string),
        attempt_tree: checkout.attempt_tree().as_str().to_string(),
        scanned_image_digest: report.image_digest.clone(),
    }
}

fn advisories_of(findings: &[ProjectedFinding]) -> Vec<AdvisoryId> {
    let mut advisories: Vec<AdvisoryId> = Vec::new();
    for finding in findings {
        if !advisories.contains(&finding.cve) {
            advisories.push(finding.cve.clone());
        }
    }
    advisories
}

fn summary_of(attempted: &[Attempted]) -> String {
    let advisories: usize = attempted.iter().map(|it| it.findings.len()).sum();
    let committed = match attempted.iter().any(Attempted::committed) {
        true => "committed what it changed",
        false => "committed nothing",
    };
    format!(
        "fiddle attempted {advisories} {} for this repository's container image in \
         one bounded attempt and {committed}.\n\nEvery advisory this run did not \
         fix is in the verdict report published beside this run's bundle, with the \
         sentence that decided it.",
        match advisories {
            1 => "advisory",
            _ => "advisories",
        },
    )
}
