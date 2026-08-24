use super::cve::{
    check_out, land, plan_shared_pull_request, plan_unproved_pull_request, publish_work,
    unproved_summary, Approved, ChangesRequested, Checkout, FailedCheck, Followed, Git,
    GroupMigration, GroupStatus, HumanSaid, InRepository, InWorktree, Landed, MigrationConfig,
    PlanError, Publication, SharedWork, Unproved, CVE_LABEL, UNPROVED_LABEL,
};
use super::{Capability, CapabilityError, ExecutionGrant};
use crate::agent::{AgentBudget, Transcripts};
use crate::cve::attempts;
use crate::cve::dedup::commit_log_dedup;
use crate::cve::project::{project, Projection};
use crate::cve::verdict::{
    disposition, Attempted, BoundReached, Budget, Disposition, InProgress, Run,
};
use crate::effect::{EffectContext, Executor, IntegrationOperation};
use crate::evaluate::{
    evaluate, Check, Contract, Evaluation, InWorkspace, Outcome, Repair, Rescan,
};
use crate::gateway::Redaction;
use crate::github::{
    observe_genuine_failure, read_pull_request_body, EnsurePullRequestBody, GenuineFailure,
};
use crate::scanner::{ScanReport, Scanner};
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

    pub checks: Vec<Check>,

    pub check: WorkspaceCommand,

    pub commands: std::sync::Arc<Vec<crate::workspace::DeclaredCommand>>,

    pub budget: AgentBudget,

    pub redaction: Redaction,

    pub transcripts: Option<Transcripts>,

    pub command_timeout: Duration,

    pub findings: Budget,

    pub max_attempts: u32,

    pub report_dir: PathBuf,

    pub today: String,

    pub settle: Duration,

    pub cancel: CancellationToken,
}

struct Counted {
    number: u64,
    held: String,
    spent: u32,
}

const SETTLE_POLL: Duration = Duration::from_secs(20);

enum Feedback {
    NoCandidate,
    Blaming(GenuineFailure),
    BlamingNothing,
    Unsettled { pending: usize, read: usize },
    Unreadable { why: String },
}

impl Feedback {
    fn attempts_afresh(&self) -> bool {
        !matches!(
            self,
            Feedback::BlamingNothing | Feedback::Unsettled { .. } | Feedback::Unreadable { .. }
        )
    }

    fn blamed(&self) -> Option<&GenuineFailure> {
        match self {
            Feedback::Blaming(failure) => Some(failure),
            Feedback::NoCandidate
            | Feedback::BlamingNothing
            | Feedback::Unsettled { .. }
            | Feedback::Unreadable { .. } => None,
        }
    }

    fn unreadable(&self) -> Option<&str> {
        match self {
            Feedback::Unreadable { why } => Some(why),
            Feedback::NoCandidate
            | Feedback::Blaming(_)
            | Feedback::BlamingNothing
            | Feedback::Unsettled { .. } => None,
        }
    }

    fn unsettled(&self) -> Option<String> {
        match self {
            Feedback::Unsettled { pending, read } => Some(format!(
                "{pending} of {read} checks on the open pull request had not settled, so \
                 this run did not read them as passing and made no fresh attempt from them"
            )),
            _ => None,
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
                commands: std::sync::Arc::clone(&config.commands),
                command_timeout: config.command_timeout,
                budget: config.budget.clone(),
                redaction: config.redaction.clone(),
                transcripts: config.transcripts.clone(),
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

    async fn sweep(
        &self,
        work_id: &str,
    ) -> Result<(Run, Option<crate::scanner::ScanError>), CapabilityError> {
        let approved = plan_shared_pull_request(
            &self.context.gh,
            &self.config.repo,
            &self.config.base,
            &stamped(&self.config.today, work_id),
            &self.config.cancel,
        )
        .await?;

        let unproved = plan_unproved_pull_request(
            &self.context.gh,
            &self.config.repo,
            &self.config.base,
            &stamped(&self.config.today, work_id),
            &self.config.cancel,
        )
        .await?;

        let counted = self.counted(&approved, &unproved).await?;

        if let Some(reached) = self.bound_reached(counted.as_ref()) {
            let mut run = Run::unusable(
                "the attempt bound was reached before anything was built or scanned".to_string(),
            );
            run.bound_reached = Some(reached);
            return Ok((run, None));
        }

        let feedback = self.settled_feedback(&approved).await;

        if let Some(why) = feedback.unreadable() {
            let mut run = Run::unusable(
                "the pull request's checks could not be read, so nothing was built or scanned"
                    .to_string(),
            );
            run.checks_unreadable = Some(why.to_string());
            return Ok((run, None));
        }

        let unsettled = feedback.unsettled();

        let checkout = check_out(
            &InRepository::new(
                &self.config.tree,
                self.executor.git(),
                self.config.cancel.clone(),
            ),
            &approved,
        )
        .await?;

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
        let git = InWorktree::new(
            &workspace,
            self.config.budget.tool_timeout,
            self.executor.git(),
        );

        let fixed = commit_log_dedup(workspace.root(), &self.config.base)?;

        let baseline = self.baseline_of(&workspace).await?;

        let report = match self.scanner.scan(&self.config.image).await {
            Ok(report) => report,
            Err(why) => {
                return Ok((Run::unusable(why.to_string()), Some(why)));
            }
        };
        self.observed.lock().unwrap().tree = Some(observed_tree(&checkout, &report));
        let projection = project(&report, &self.config.severities)?;

        let (taken, deferred) = self
            .config
            .findings
            .apply(projection.all().cloned().collect());

        let mut said = self.conversation(&approved).await;
        said.extend(self.conversation(&unproved).await);
        let (mut asked, reviewed) = self.reviews(&approved).await;
        said.extend(reviewed);
        let (also_asked, also_reviewed) = self.reviews(&unproved).await;
        asked.extend(also_asked);
        said.extend(also_reviewed);

        let spent = counted.as_ref().map_or(0, |it| it.spent);
        let mut ignored_citation: Option<String> = None;
        let mut settled: Vec<AdvisoryId> = Vec::new();
        let mut attempted: Vec<Attempted> = Vec::new();
        let mut judged = None;
        let answering = !asked.is_empty();
        if (feedback.attempts_afresh() && !taken.is_empty()) || answering {
            let attempt = self
                .migration
                .migrate(
                    &workspace,
                    &taken,
                    feedback.blamed(),
                    &baseline,
                    &said,
                    &asked,
                )
                .await?;
            let evaluation = self
                .judge(&workspace, &taken, &projection, &report, &baseline.failed)
                .await?;
            let cited = attempt.report.quoted_from_a_comment.as_deref();
            let followed = cited.and_then(|sentence| Followed::quoted(&said, sentence));
            if let (Some(claimed), None) = (cited, followed.as_ref()) {
                ignored_citation = Some(format!(
                    "the report cited direction that nobody who speaks for this project \
                     wrote, so it was ignored and the attempt stands on its own: {claimed:?}"
                ));
            }
            let status =
                GroupStatus::of(&evaluation, attempt.undeclared.as_ref(), followed.as_ref());
            let advisories = advisories_of(&taken);
            let group = Attempted {
                findings: taken,
                status,
                attempt,
            };
            match group.settled() {
                true => settled = advisories.clone(),
                false => {
                    let onto = unproved.reused_branch();
                    let landing = land(
                        &git,
                        &advisories,
                        &group.status,
                        &group.attempt.changed,
                        onto,
                    )
                    .await?;
                    if landing == Landed::CommittedForJudgement {
                        let summary = unproved_summary(&Unproved {
                            advisories: &advisories,
                            rationale: &refusal(&group.status).unwrap_or_default(),
                            check: failed_check(&evaluation).as_ref(),
                            declared: &group.attempt.report.changed_files,
                            notes: &group.attempt.report.findings,
                        });
                        judged = Some(
                            self.publish_for_judgement(
                                &git,
                                &unproved,
                                summary,
                                spent + 1,
                                advisories.len(),
                            )
                            .await?,
                        );
                    }
                }
            }
            attempted.push(group);
        }

        let landed = match attempted.iter().any(Attempted::committed) {
            false => None,
            true => Some(self.publish(&git, &approved, &attempted, spent + 1).await?),
        };
        let counted_number = counted.as_ref().map(|held| held.number);
        let count_is_written = judged
            .as_ref()
            .is_some_and(|it| Some(it.pull_request) == counted_number);
        if landed.is_none() && !count_is_written && !attempted.is_empty() {
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
        run.judged = judged;
        run.checks_unsettled = unsettled;
        run.ignored_citation = ignored_citation;
        Ok((run, None))
    }

    async fn counted(
        &self,
        approved: &Approved,
        unproved: &Approved,
    ) -> Result<Option<Counted>, CapabilityError> {
        let Some(number) = approved.reused().or_else(|| unproved.reused()) else {
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
            Observation::Available { value, .. } => match value.failure {
                Some(failure) => Feedback::Blaming(failure),
                None if value.has_settled() => Feedback::BlamingNothing,
                None => Feedback::Unsettled {
                    pending: value.pending(),
                    read: value.read,
                },
            },
            Observation::Unavailable { source, reason } => Feedback::Unreadable {
                why: format!("{}: {reason}", source.0),
            },
            Observation::NotApplicable { .. } => Feedback::BlamingNothing,
        }
    }

    async fn settled_feedback(&self, approved: &Approved) -> Feedback {
        let deadline = self.config.settle;
        if deadline.is_zero() {
            return self.feedback(approved).await;
        }
        let started = std::time::Instant::now();
        loop {
            let feedback = self.feedback(approved).await;
            if !matches!(feedback, Feedback::Unsettled { .. }) {
                return feedback;
            }
            if started.elapsed() >= deadline {
                return feedback;
            }
            tokio::select! {
                _ = self.config.cancel.cancelled() => return feedback,
                _ = tokio::time::sleep(SETTLE_POLL) => {}
            }
        }
    }

    async fn reviews(&self, approved: &Approved) -> (Vec<ChangesRequested>, Vec<HumanSaid>) {
        let Some(number) = approved.reused() else {
            return (Vec::new(), Vec::new());
        };
        let head = approved.pr_head().unwrap_or_default().to_string();
        let read = crate::github::read_reviews(
            &self.context.gh,
            &self.config.repo,
            number,
            crate::human::CONVERSATION_PAGES,
            &self.config.cancel,
        )
        .await;
        let Ok(reviews) = read else {
            return (Vec::new(), Vec::new());
        };
        let spoken: Vec<_> = reviews
            .into_iter()
            .filter(|it| !it.body.trim().is_empty())
            .collect();
        let asks = |it: &crate::github::Reviewed| {
            it.state
                .eq_ignore_ascii_case(crate::github::CHANGES_REQUESTED)
        };
        let asked = spoken
            .iter()
            .filter(|it| {
                asks(it)
                    && crate::capability::entitled(&it.author_association)
                    && it.commit_id == head
            })
            .map(|it| ChangesRequested {
                author: it.author.login.clone(),
                body: it.body.clone(),
            })
            .collect();
        let said = spoken
            .iter()
            .filter(|it| !asks(it))
            .map(|it| HumanSaid {
                author: it.author.login.clone(),
                body: it.body.clone(),
                entitled: crate::capability::entitled(&it.author_association),
            })
            .collect();
        (asked, said)
    }

    async fn conversation(&self, approved: &Approved) -> Vec<HumanSaid> {
        let Some(number) = approved.reused() else {
            return Vec::new();
        };
        let read = crate::github::read_conversation(
            &self.context.gh,
            &self.config.repo,
            number,
            crate::human::CONVERSATION_PAGES,
            &self.config.cancel,
        )
        .await;
        match read {
            Ok(conversation) => conversation
                .into_iter()
                .filter(|it| !it.is_bot)
                .map(|it| HumanSaid {
                    author: it.author.login,
                    entitled: crate::capability::entitled(&it.author_association),
                    body: it.body,
                })
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    async fn baseline_of(
        &self,
        workspace: &Workspace,
    ) -> Result<crate::evaluate::Baseline, CapabilityError> {
        let contract = Contract {
            checks: self.config.checks.clone(),
            severities: self.config.severities.clone(),
            repair: None,
            excused: Vec::new(),
        };
        let tree = InWorkspace::new(
            workspace,
            self.config.command_timeout,
            Rescan {
                scratch: self.config.scratch.clone(),
                image: self.config.image.clone(),
            },
        );
        crate::evaluate::baseline(&contract, &tree)
            .await
            .map_err(|_cancelled| CapabilityError::Workspace(WorkspaceError::Cancelled))
    }

    async fn judge(
        &self,
        workspace: &Workspace,
        shown: &[ProjectedFinding],
        projection: &Projection,
        report: &ScanReport,
        excused: &[String],
    ) -> Result<Evaluation, CapabilityError> {
        let contract = Contract {
            checks: self.config.checks.clone(),
            severities: self.config.severities.clone(),
            excused: excused.to_vec(),
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
        let shared = publish_work(
            &self.executor,
            self.id(),
            approved,
            &Publication {
                repo: self.config.repo.clone(),
                head_owner: self.config.head_owner.clone(),
                title: rendered_title(
                    &self.config.title,
                    &self.config.project,
                    counted_advisories(attempted),
                ),
                summary: summary_of(attempted),
                head_sha,
                attempts,
                label: CVE_LABEL,
                draft: false,
            },
        )
        .await?;
        Ok(self.receipted(shared))
    }

    async fn publish_for_judgement(
        &self,
        git: &InWorktree<'_>,
        unproved: &Approved,
        summary: String,
        attempts: u32,
        advisories: usize,
    ) -> Result<crate::cve::verdict::Landed, CapabilityError> {
        let head_sha = git.run(&["rev-parse", "HEAD"]).await?.trim().to_string();
        let published = publish_work(
            &self.executor,
            self.id(),
            unproved,
            &Publication {
                repo: self.config.repo.clone(),
                head_owner: self.config.head_owner.clone(),
                title: format!(
                    "{}, unproved",
                    rendered_title(&self.config.title, &self.config.project, advisories)
                ),
                summary,
                head_sha,
                attempts,
                label: UNPROVED_LABEL,
                draft: true,
            },
        )
        .await?;
        Ok(self.receipted(published))
    }

    fn receipted(&self, published: SharedWork) -> crate::cve::verdict::Landed {
        let mut observed = self.observed.lock().unwrap();
        observed.receipts.push(EvidenceRef(format!(
            "{CVE_ORIGIN}:{}/tree/{}",
            self.config.repo, published.branch
        )));
        observed.receipts.push(EvidenceRef(format!(
            "{CVE_ORIGIN}:{}/pull/{}",
            self.config.repo, published.pull_request
        )));
        crate::cve::verdict::Landed {
            branch: published.branch,
            pull_request: published.pull_request,
        }
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

        let (run, scanned) = self.sweep(work_id).await?;

        let concluded = disposition(&run);
        self.observed.lock().unwrap().disposition = Some(concluded.published());
        self.publish_reports(&concluded)?;
        if let Some(why) = scanned {
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

fn refusal(status: &GroupStatus) -> Option<String> {
    match status {
        GroupStatus::Clean | GroupStatus::Directed { .. } => None,
        GroupStatus::NeedsWork { reason } => Some(reason.to_string()),
    }
}

fn failed_check(evaluation: &Evaluation) -> Option<FailedCheck> {
    let refused = evaluation.first_failure()?;
    let (exit_code, log) = match &refused.outcome {
        Outcome::Finished(answered) => (
            Some(answered.exit_code),
            [answered.stdout.as_str(), answered.stderr.as_str()]
                .into_iter()
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        Outcome::Scanned(_) => (None, String::new()),
        Outcome::NoArtefact(why) | Outcome::NotRun(why) => (None, why.clone()),
    };
    Some(FailedCheck {
        name: refused.name.clone(),
        exit_code,
        log,
    })
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

fn stamped(today: &str, work_id: &str) -> String {
    let tail: String = work_id
        .chars()
        .filter(|it| it.is_ascii_alphanumeric())
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    match tail.is_empty() {
        true => today.to_string(),
        false => format!("{today}-{}", tail.to_ascii_lowercase()),
    }
}

fn rendered_title(template: &str, project: &str, advisories: usize) -> String {
    template
        .replace("{project}", project)
        .replace("{advisories}", &advisories.to_string())
}

fn counted_advisories(attempted: &[Attempted]) -> usize {
    attempted.iter().map(|it| it.findings.len()).sum()
}

fn summary_of(attempted: &[Attempted]) -> String {
    let advisories: usize = attempted.iter().map(|it| it.findings.len()).sum();
    let committed = match attempted.iter().any(Attempted::committed) {
        true => "committed what it changed",
        false => "committed nothing",
    };
    let mut body = format!(
        "fiddle attempted {advisories} {} for this repository's container image in \
         one bounded attempt and {committed}.",
        match advisories {
            1 => "advisory",
            _ => "advisories",
        },
    );

    body.push_str(&advisory_table(attempted));
    body.push_str(&agent_notes(attempted));
    body.push_str(&changed_files(attempted));
    body
}

fn advisory_table(attempted: &[Attempted]) -> String {
    if attempted.iter().all(|group| group.findings.is_empty()) {
        return String::new();
    }
    let mut table = String::from(
        "\n\n| advisory | package | in the project | the fix is in | severity | outcome |\n\
         | --- | --- | --- | --- | --- | --- |",
    );
    for group in attempted {
        let outcome = outcome_of(group);
        for finding in &group.findings {
            table.push_str(&format!(
                "\n| {} | `{}` | {} | {} | {} | {} |",
                finding.cve.as_str(),
                finding.package,
                finding.current,
                finding
                    .fixed_version
                    .as_deref()
                    .unwrap_or("no fix published"),
                finding.severity.as_str(),
                outcome,
            ));
        }
    }
    table
}

fn outcome_of(group: &Attempted) -> String {
    match &group.status {
        GroupStatus::Clean => match group.attempt.changed.is_empty() {
            true => "already clear, nothing changed".to_string(),
            false => "cleared by this change".to_string(),
        },
        GroupStatus::Directed { over, direction } => {
            format!("changed, and published over the failing check `{over}` because {direction}")
        }
        GroupStatus::NeedsWork { reason } => reason.to_string(),
    }
}

fn agent_notes(attempted: &[Attempted]) -> String {
    let mut notes = String::new();
    for group in attempted {
        for disposition in &group.attempt.report.findings {
            if disposition.note.trim().is_empty() {
                continue;
            }
            notes.push_str(&format!(
                "\n\n**{}** — {}",
                disposition.cve,
                disposition.note.trim()
            ));
        }
    }
    match notes.is_empty() {
        true => String::new(),
        false => format!("\n\n### What the agent reported{notes}"),
    }
}

fn changed_files(attempted: &[Attempted]) -> String {
    let mut files: Vec<String> = Vec::new();
    for group in attempted {
        for path in &group.attempt.changed {
            let named = path.as_str().to_string();
            if !files.contains(&named) {
                files.push(named);
            }
        }
    }
    match files.is_empty() {
        true => String::new(),
        false => format!(
            "\n\n### Files changed\n\n{}",
            files
                .iter()
                .map(|it| format!("- `{it}`"))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}

#[cfg(test)]
mod body {
    use super::*;
    use crate::agent::{FindingDisposition, RepairReport};
    use crate::capability::cve::{GroupStatus, MigrationAttempt, NeedsWork};
    use fiddle_core::{AdvisoryId, PackageType, ProjectedFinding, Severity};

    fn finding() -> ProjectedFinding {
        ProjectedFinding {
            cve: AdvisoryId::parse("CVE-2025-30204").expect("a canonical advisory id"),
            package: "github.com/golang-jwt/jwt/v4".to_string(),
            current: "4.5.0".to_string(),
            fixed_version: Some("4.5.2".to_string()),
            severity: Severity::High,
            package_type: PackageType::Library,
        }
    }

    fn attempted(status: GroupStatus, changed: &[&str]) -> Attempted {
        Attempted {
            findings: vec![finding()],
            status,
            attempt: MigrationAttempt {
                report: RepairReport {
                    changed_files: changed.iter().map(|it| it.to_string()).collect(),
                    summary: "a summary the body does not quote".to_string(),
                    claimed_complete: true,
                    findings: vec![FindingDisposition {
                        cve: "CVE-2025-30204".to_string(),
                        attempted: true,
                        note: "Upgraded jwt/v4 from v4.5.0 to v4.5.2.".to_string(),
                    }],
                    quoted_from_a_comment: None,
                },
                changed: changed
                    .iter()
                    .map(|it| crate::workspace::WorkspacePath::parse(it).expect("a workspace path"))
                    .collect(),
                undeclared: None,
            },
        }
    }

    #[test]
    fn a_fresh_branch_carries_the_runs_own_stamp_so_a_closed_pull_request_cannot_block_it() {
        let first = stamped("2026-08-23", "cve:0:01M0QNX18RQ0F18J842SF047JM");
        let second = stamped("2026-08-23", "cve:0:01M0QKZY061D2G9J7QGT4V07HB");

        assert_ne!(
            first, second,
            "two runs on one day must not choose one branch name, or the second \
             cannot push where a closed pull request left the first"
        );
        assert!(
            first.starts_with("2026-08-23-"),
            "the date still reads first, so a human scanning branches sees the day: {first}"
        );
        assert!(
            first
                .chars()
                .all(|it| it.is_ascii_lowercase() || it.is_ascii_digit() || it == '-'),
            "a branch name carries no character git or the forge would refuse: {first}"
        );
    }

    #[test]
    fn a_run_with_no_usable_work_id_still_names_a_branch() {
        assert_eq!(
            stamped("2026-08-23", "::"),
            "2026-08-23",
            "an id with nothing to take falls back to the date rather than a trailing dash"
        );
    }

    #[test]
    fn the_default_template_keeps_the_title_every_deployment_already_has() {
        assert_eq!(
            rendered_title("{project}: dependency advisories", "icecube", 1),
            "icecube: dependency advisories",
            "a deployment that configures nothing must not see its titles change"
        );
    }

    #[test]
    fn a_configured_template_reaches_the_title_with_the_counts_substituted() {
        assert_eq!(
            rendered_title(
                "fix(security): remediate {advisories} advisory in {project}",
                "icecube",
                3,
            ),
            "fix(security): remediate 3 advisory in icecube",
            "a repository that writes conventional commits says so once"
        );
    }

    #[test]
    fn a_directed_attempt_is_published_and_says_what_it_went_over() {
        let group = attempted(
            GroupStatus::Directed {
                over: "./policy.sh".to_string(),
                direction: crate::capability::cve::Followed {
                    author: "peel".to_string(),
                    sentence: "understood. publish it.".to_string(),
                },
            },
            &["go.mod", "go.sum"],
        );

        assert!(
            group.committed(),
            "a directed attempt changed the tree and stands, so it publishes like a clean \
             one — otherwise it commits and nothing carries it anywhere"
        );

        let body = summary_of(&[group]);
        assert!(
            body.contains("published over the failing check `./policy.sh`")
                && body.contains("peel wrote: understood. publish it."),
            "and the row names the check it went over and whose words did it: {body}"
        );
    }

    #[test]
    fn a_repaired_advisory_is_named_with_both_versions_and_what_the_agent_said() {
        let body = summary_of(&[attempted(GroupStatus::Clean, &["go.mod", "go.sum"])]);

        for expected in [
            "CVE-2025-30204",
            "github.com/golang-jwt/jwt/v4",
            "4.5.0",
            "4.5.2",
            "HIGH",
            "cleared by this change",
            "Upgraded jwt/v4 from v4.5.0 to v4.5.2.",
            "`go.mod`",
            "`go.sum`",
        ] {
            assert!(
                body.contains(expected),
                "a reader judges the change from the body alone, and {expected} is missing: \
                 {body}"
            );
        }
    }

    #[test]
    fn an_advisory_left_unproved_says_so_in_the_same_row() {
        let body = summary_of(&[attempted(
            GroupStatus::NeedsWork {
                reason: NeedsWork::CheckFailed {
                    check: "golangci-lint run".to_string(),
                    already: false,
                },
            },
            &["go.mod"],
        )]);

        assert!(
            body.contains("CVE-2025-30204"),
            "the advisory is named whatever the outcome: {body}"
        );
        assert!(
            body.contains("golangci-lint run"),
            "the check that stopped it is the whole news: {body}"
        );
    }
}
