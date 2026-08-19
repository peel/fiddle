//! The capability M4 adds, and the one place every other M4 module is called
//! from.
//!
//! Everything this file does was written somewhere else and had no caller.
//! [`crate::scanner`] runs the scan, [`crate::cve::project`] reads it,
//! [`crate::cve::dedup`] drops what is already dealt with, [`Budget`] bounds what
//! is left, [`super::cve::GroupMigration`] runs the one step a model is consulted
//! for, [`crate::evaluate`] judges what it left, [`super::cve::GroupStatus`] rules
//! on it, [`super::cve::land`] puts it on the branch or takes it back,
//! [`super::cve::publish_shared_work`] pushes and opens the one pull request, and
//! [`crate::cve::verdict::disposition`] says what the run came to. Each of those
//! is tested against its own seam; **none of them was reachable from a `fiddle
//! run` until this existed**, and a module with no production caller is a module
//! whose contract with its neighbours has never been checked.
//!
//! # The order is the design, and three places in it are load-bearing
//!
//! 1. **The branch is decided before anything touches a tree.**
//!    [`super::cve::plan_shared_pull_request`] is the only step that can refuse
//!    the run outright, and its whole argument is that a refusal must arrive
//!    before a commit — a run that checked a branch out, committed a bump onto it
//!    and only then found it could not push has written to a branch somebody else
//!    owns.
//! 2. **Dedup runs before the bound.** A finding this repository already carries
//!    the fix for must not spend the budget, which is what
//!    [`Budget::apply`]'s own header says the order is for.
//! 3. **One worktree, one attempt, one push.** Every finding the bound left is
//!    shown to **one** bounded attempt, in one worktree, and what it leaves is
//!    committed once or put back once. That is why
//!    [`super::cve::GroupMigration::migrate`] does not make its own worktree, and
//!    it is what earns [`super::cve::land`]'s revert: the worktree outlives the
//!    attempt, so putting a needs-work edit back is what stops it riding out on
//!    the push.
//!
//! # There is no grouping here, and that is the design rather than a simplification
//!
//! Until M4c this file grouped its findings by the bump target four mechanical
//! rules elected and ran one attempt per group. Both halves are gone, and the
//! reason is one fact: **a group cannot be formed without knowing which file
//! fixes a finding**, and that judgement is the agent's now — Rust cannot make it
//! for a `requirements.txt` or a `pom.xml`, and the version arithmetic that
//! picked what each group moved to was Go's minimal version selection wearing a
//! general name. So nothing below batches findings by any ecosystem meaning, no
//! bump is applied before the model is briefed, and the whole of what this file
//! decides about *which* edit clears an advisory is nothing.
//!
//! What that costs is stated where it is paid: `docs/specs/`'s M4c design §6 is
//! the list, and the one that lands here is that a run's landing is now
//! **all-or-nothing** over its findings rather than per group. A cleared advisory
//! is discarded when an unrelated one is not cleared, which was raised as a cost
//! and accepted in favour of the simpler core.
//!
//! # What this capability does not decide
//!
//! Any of it. Every branch below is a `match` on a value some other module
//! computed, and the arithmetic is all upstream — which is the same discipline
//! [`super::cve`] states about the prompt, applied to the orchestration. The one
//! thing decided here is the *order*, and the three sentences above are the whole
//! of it.

use super::cve::{
    check_out, land, plan_shared_pull_request, publish_shared_work, Approved, Checkout, Git,
    GroupMigration, GroupStatus, InRepository, InWorktree, MigrationConfig, SharedPublication,
};
use super::{Capability, CapabilityError, ExecutionGrant};
use crate::agent::AgentBudget;
use crate::cve::dedup::{already_fixed, commit_log_dedup};
use crate::cve::go::Go;
use crate::cve::project::{project, Projection};
use crate::cve::verdict::{disposition, Attempted, Budget, Disposition, InProgress, Run};
use crate::effect::{EffectContext, Executor};
use crate::evaluate::{evaluate, Check, Contract, Evaluation, InWorkspace, Repair, Rescan};
use crate::scanner::{ScanReport, Scanner, WizCredential};
use crate::workspace::{Workspace, WorkspaceCommand, WorkspaceError};
use fiddle_core::{
    correlation_key, AdvisoryId, AttemptId, CapabilityId, ChangeSetState, EvidenceRef,
    ProjectedFinding, RunDisposition, Severities, TreeObservation,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// The origin every evidence reference this capability publishes is namespaced
/// by.
const CVE_ORIGIN: &str = "cve";

/// Everything a sweep needs that is not the model, the scanner or the executor.
///
/// One struct rather than twenty arguments, for the reason
/// [`super::PublishConfig`] and [`super::ProposeConfig`] are each one: every
/// field is a deployment decision an operator configured, and none is derivable
/// from the others.
///
/// **No credential is in here except the scanner's**, and that one is here
/// because there is nowhere else it can be: the forge credential lives behind the
/// executor, the model's behind the model, and the scanner is constructed by the
/// caller and handed in whole — see [`CveMitigate::new`]. What this struct holds
/// is the *rescan's* credential, because a rescan's scanner is built per
/// evaluation by [`InWorkspace`] rather than once by the caller.
pub struct MitigateConfig {
    /// `owner/name`, as an API path spells it.
    pub repo: String,

    /// The owner the shared head branch lives under.
    pub head_owner: String,

    /// The branch the shared pull request is proposed into, and the branch
    /// [`commit_log_dedup`] measures this one against.
    ///
    /// **One value for both, and that is the point.** The dedup range is
    /// `origin/<base>..HEAD`, so a `base` guessed here and a `base` configured
    /// there would make the run read a range that is not the branch's own.
    pub base: String,

    /// The shared pull request's title, naming no advisory. It outlives any one
    /// run's findings, which is the reason a commit subject names none either.
    pub title: String,

    /// The project half of the run's identity, held for the reason
    /// [`super::ProposeConfig::project`] is: `execute` refuses a configuration
    /// whose project differs from the executor's rather than letting a run
    /// publish under a name its own effects were not derived from.
    pub project: String,

    /// Where the correlation marker is recorded, the same place every other
    /// capability writes one.
    pub stub_root: PathBuf,

    /// The repository being mitigated. The run branches one worktree from it and
    /// never writes to it.
    pub tree: PathBuf,

    /// Where that worktree is created.
    pub workspace_root: PathBuf,

    /// What is scanned, in whatever spelling the scanner accepts.
    pub image: String,

    /// The grades this deployment acts on by grade alone — `[orchestration.cve]
    /// severities`, and the second half of that table's two documented
    /// preferences beside [`MitigateConfig::findings`].
    ///
    /// Carried here rather than defaulted at the projection, for the reason every
    /// other field in this struct is here: it is a deployment decision an
    /// operator wrote down. It reaches two readers and has to be the same value
    /// at both — the input scan's projection and the rescan's contract — because
    /// a repair judged against a different set than the scan that opened it is
    /// answering about another deployment.
    pub severities: Severities,

    /// Where a rescan's report is written and stays. Owned by the caller for
    /// [`Rescan::scratch`]'s reason: the artefact has to outlive the scan long
    /// enough to be published as evidence.
    pub scratch: PathBuf,

    /// The tenant credential every *rescan* authenticates with. See this
    /// struct's header for why the input scan's is not here.
    pub rescan_credential: WizCredential,

    /// The `go` this run asks about the module graph, as the operator seam
    /// spells it: a program, its leading arguments, and the bound any one of its
    /// children runs under.
    ///
    /// A [`WorkspaceCommand`] because that is exactly the triple, not because
    /// anything runs it through [`Workspace::run`] — [`Go`] owns its own
    /// environment, and reusing the type here rather than declaring a fourth
    /// program-args-timeout struct is the same economy `[github] cli` makes.
    pub go: WorkspaceCommand,

    /// The five checks of Design §2.6, in the order they run, as the document
    /// declared them.
    pub checks: Vec<Check>,

    /// The one command the `run_check` tool offers a model. **Not the check that
    /// decides anything** — see [`MigrationConfig::check`].
    pub check: WorkspaceCommand,

    /// What one bounded attempt runs inside.
    pub budget: AgentBudget,

    /// The bound a single check may run for.
    pub command_timeout: Duration,

    /// How many findings this run may take. See [`Budget`].
    pub findings: Budget,

    /// Where `verdicts.json` is written, beside the report bundle.
    pub report_dir: PathBuf,

    /// The date a fresh branch is named after, supplied rather than read here —
    /// see [`super::cve::today_utc`], which is what the binary supplies it from.
    pub today: String,

    /// Stops the scan, the attempt, the tools, the checks and the git together.
    pub cancel: CancellationToken,
}

/// What the run observed about itself, read after the execution on both arms.
#[derive(Default)]
struct Observed {
    /// One reference per artefact this run produced, in the order it produced
    /// them.
    receipts: Vec<EvidenceRef>,
    /// Which revision the worktree was made at, once [`check_out`] has answered.
    tree: Option<TreeObservation>,
    /// Which row of Design §3's table the run reached, once [`disposition`] has
    /// answered.
    ///
    /// Recorded here rather than returned from
    /// [`Capability::execute`](crate::capability::Capability::execute) because
    /// one of the seven rows *is* an error return —
    /// [`Reason::ScanUnusable`](crate::evaluate::Reason::ScanUnusable) — and a
    /// row published only on the successful arm would be missing exactly the
    /// row Design §3 calls the milestone most likely to get wrong.
    disposition: Option<RunDisposition>,
}

/// One sweep of a repository's container image: scan, bump, judge, land,
/// publish, report.
///
/// Generic over the model for [`GroupMigration`]'s reason and over the scanner
/// for [`Scanner`]'s: a test substitutes a scripted model and a scripted scanner
/// and drives the real tools, the real worktree, the real checks and the real
/// effect executor without a credential or a socket.
pub struct CveMitigate<'a, M, S> {
    executor: Executor<'a>,
    /// The same context the executor holds. Borrowed for one thing the executor
    /// does not expose: the *read* that discovers the shared pull request, which
    /// is not an effect and must not be journaled as one.
    context: &'a EffectContext,
    scanner: S,
    config: MitigateConfig,
    observed: Mutex<Observed>,
    /// The tools' record, shared with the one [`GroupMigration`] every group in
    /// this run is attempted through, so the receipts survive a group that
    /// failed.
    migration: GroupMigration<M>,
}

impl<'a, M, S> CveMitigate<'a, M, S>
where
    M: rig_core::completion::CompletionModel + 'static,
    S: Scanner + Send + Sync,
{
    /// A sweep that will run `scanner` and `model` under `config`, publishing
    /// through `executor`.
    ///
    /// The executor is expected to be bound to [`fiddle_core::CVE_MITIGATE`]; one
    /// that is not is refused by the executor's own step 1 on the first
    /// proposal, which is the check that belongs to the executor rather than to
    /// its callers.
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

    /// The one tree this run works in and publishes from, derived rather than
    /// configured.
    ///
    /// Through [`super::attempt_worktree`] and not a spelling of its own: the
    /// binary calls the same function to build the [`EffectContext`] this
    /// capability is handed, and two derivations of one path is exactly the
    /// drift that function exists to make impossible.
    fn worktree(&self) -> PathBuf {
        super::attempt_worktree(
            &self.config.workspace_root,
            &self.config.project,
            self.executor.invocation_ref(),
        )
    }

    /// The whole sweep, from a scan that produced a document to the branch it
    /// left behind.
    ///
    /// Separated from [`Capability::execute`] so that the one thing `execute`
    /// does around it — write the report *whatever* this answered — is visible
    /// in one screen.
    async fn sweep(&self, report: &ScanReport) -> Result<Run, CapabilityError> {
        let projection = project(report, &self.config.severities)?;

        // 1. Which branch. Before a tree is touched, so a refusal precedes any
        //    commit. See this module's header.
        let approved = plan_shared_pull_request(
            &self.context.gh,
            &self.config.repo,
            &self.config.base,
            &self.config.today,
            &self.config.cancel,
        )
        .await?;

        // 2. Which revision, and the fetch that makes it resolvable. Run in the
        //    repository the worktree will be branched from, because there is not
        //    one yet — choosing its revision is what this answers.
        let checkout = check_out(&InRepository::new(&self.config.tree), &approved).await?;
        // And this line is the only place in the build where the scanned image
        // and the remediated revision are both in hand. See [`observed_tree`]:
        // the scan happened in `execute`, before there was a tree to speak of,
        // and `Checkout` never sees a scanner — so the pair can be made here or
        // nowhere.
        self.observed.lock().unwrap().tree = Some(observed_tree(&checkout, report));

        // 3. One worktree, at that revision, for every group in this run.
        let worktree = self.worktree();
        let root = worktree
            .parent()
            .unwrap_or(&self.config.workspace_root)
            .to_path_buf();
        // A directory name and not an attempt id: the attempt this execution
        // belongs to is the one on the grant, and it is what the evidence below
        // quotes. The same split `ProposeChange::produce_from` makes, and for its
        // reason — `Workspace::create_at` puts the tree at `<root>/<name>`.
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
        let graph = Go::new(
            PathBuf::from(&self.config.go.program),
            self.config.go.args.clone(),
            workspace.root().to_path_buf(),
            workspace.home().to_path_buf(),
            self.config.go.timeout,
            self.config.cancel.clone(),
        );

        // 4. What this branch already says it has fixed. Read in the worktree,
        //    over `origin/<base>..HEAD`, and read *before* this run commits
        //    anything — so what comes back is what was already there.
        let fixed = commit_log_dedup(workspace.root(), &self.config.base)?;

        let mut settled: Vec<AdvisoryId> = Vec::new();
        let mut open: Vec<ProjectedFinding> = Vec::new();
        for finding in projection.fixable().cloned().collect::<Vec<_>>() {
            match already_fixed(&finding, &graph, &fixed).await? {
                true => settled.push(finding.cve),
                false => open.push(finding),
            }
        }

        // 5. The bound, applied after deduplication rather than as a filter
        //    before it — see [`Budget::apply`] for why the order is the design.
        let (taken, deferred) = self.config.findings.apply(open);

        // 6. **One bounded attempt, every finding the bound left, one worktree.**
        //    No grouping and no bump: which file clears which advisory, and what
        //    version it moves to, is the attempt's own judgement — see this
        //    module's header. What Rust holds it to afterwards is mechanical and
        //    it is all downstream of this line: the declaration against the diff,
        //    the deployment's checks, and the rescan.
        //
        //    Guarded on emptiness rather than run over nothing, because an attempt
        //    shown no advisory is a prompt with no findings in it — a model asked
        //    to fix nothing, a turn spent, and a report that would then have
        //    nothing to account for. A run with nothing taken has one honest
        //    outcome and [`disposition`] already names it from the sets below.
        let mut attempted: Vec<Attempted> = Vec::new();
        if !taken.is_empty() {
            let attempt_outcome = self.migration.migrate(&workspace, &taken).await?;
            let evaluation = self.judge(&workspace, &taken, &projection, report).await?;
            let status = GroupStatus::of(
                &evaluation,
                &attempt_outcome.forbidden,
                attempt_outcome.undeclared.as_ref(),
            );
            // One landing, all of it or none of it. `land` commits exactly what
            // git saw the attempt change, or puts exactly that back — and with one
            // attempt in the run there is no second edit for a reverted one to
            // ride out on, which is what the revert used to be guarding against.
            land(
                &git,
                &advisories_of(&taken),
                &status,
                &attempt_outcome.changed,
            )
            .await?;

            attempted.push(Attempted {
                findings: taken,
                status,
                attempt: attempt_outcome,
            });
        }

        // 7. One push and one pull request, and only where something landed.
        let landed = match attempted
            .iter()
            .any(|group| group.status == GroupStatus::Clean)
        {
            false => None,
            true => Some(self.publish(&git, &approved, &attempted).await?),
        };

        // 8. The record. Every field is something that already happened; nothing
        //    below reads a conclusion out of them — see [`Run`].
        let in_progress = approved.reused().map(|number| InProgress {
            number,
            // **The branch's own commit bodies, never the pull request's body.**
            // A body lists what a scan found when the pull request was opened, so
            // a mention there is evidence a CVE was seen and not that it was
            // fixed. `dedup`'s 2026-08-12 incident is what settled that, and
            // `fixed` is the log read rather than a second reading of the forge.
            covers: projection
                .all()
                .filter(|finding| fixed.names(finding.cve.as_str()))
                .map(|finding| finding.cve.clone())
                .collect(),
        });

        let mut run = Run::scanned(projection);
        run.already_fixed = settled;
        run.in_progress = in_progress;
        // **Nothing sets `blocked`, and no run reaches it any more.** It was the
        // set of findings four mechanical rules could place nowhere, or could
        // pick no version for — both of them Go's judgements, both gone, and
        // there is nothing left in this build that refuses a finding *before*
        // showing it to an attempt. A finding this run cannot fix is now either
        // one the scanner published no fix for, which the projection reports
        // without ever offering it here, or one the attempt was shown and could
        // not clear, which is an `Attempted` row. The field stays because
        // `Run` is the record's shape and a reader of `verdicts.json` sees the
        // same document either way.
        run.attempted = attempted;
        run.deferred = deferred;
        run.landed = landed;
        Ok(run)
    }

    /// The five checks and the rescan, over the tree the attempt left behind.
    ///
    /// The [`Repair`] premise is the attempt's and the input scan's together,
    /// which is what [`Contract::repair`] is for: `must_clear` is every advisory
    /// the attempt was **shown**, because those are the ones it was asked to
    /// clear, and `input` is the whole scan, because condition (b) asks whether a
    /// finding is *new* and a baseline of the shown set alone would read every
    /// deferred finding as one that just appeared.
    ///
    /// `must_clear` holding the whole shown set is what makes the landing
    /// all-or-nothing: one advisory the rescan still reports is one the contract
    /// asked for, so the evaluation is not accepted and [`land`] reverts —
    /// including the edits that did clear their own findings. That is M4c design
    /// §3, and it is a property of *this* argument rather than of a rule
    /// somewhere downstream.
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
                credential: self.config.rescan_credential.clone(),
                image: self.config.image.clone(),
            },
        );
        evaluate(&contract, &tree)
            .await
            .map_err(|_cancelled| CapabilityError::Workspace(WorkspaceError::Cancelled))
    }

    /// Push the branch and make sure the one shared pull request exists.
    ///
    /// The head sha is read from the worktree rather than resolved by the
    /// operation, for [`crate::github::EnsureBranchPublished`]'s reason: an
    /// operation that read `HEAD` for itself could publish a commit its own
    /// proposal never named, with the payload hash still matching because the
    /// payload would never have carried it.
    async fn publish(
        &self,
        git: &InWorktree<'_>,
        approved: &Approved,
        attempted: &[Attempted],
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

    /// Record this invocation's correlation key as the change set for the work.
    ///
    /// Deliberately identical to what every other capability writes, through the
    /// same atomic write, for [`super::PublishChange`]'s reason: the assessment
    /// that reads it does not know or care which capability produced it.
    ///
    /// For a trackerless sweep `work_id` is the reference's slug — `cve` — which
    /// is [`crate::orchestration::Addressed`]'s answer and not this capability's
    /// invention.
    ///
    /// **What it is written for here is a record, not a gate.** A sweep's
    /// reference names no work item and so has no completion state, so the marker
    /// this writes will not make the next sweep `Complete` and is not meant to:
    /// the next sweep scans again, and design §4's dedup is what keeps it from
    /// opening a second pull request (ADR 023). It is still written, and
    /// identically, because it is the local record that a run happened at all —
    /// and because a capability that decided for itself which references deserve
    /// a marker would be answering a question the assessment owns.
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

    /// The vocabulary this capability's progress is filed under.
    ///
    /// Its own word and not a neighbour's, which is what
    /// [`Capability::stage`] refuses to default for: a `cve_mitigate` run filed
    /// under `mark` would be a bundle labelled with M0's one step.
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
        // The executor was built for a run, and this is that run — or this
        // capability publishes under one name while the bundle, the journal and
        // the marker are filed under another. Checked before the scanner is
        // started, so a misbound executor provably scans nothing and reaches no
        // forge.
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
        // And the tree this run publishes from is the tree its groups work in.
        // `EnsureBranchPublished` pushes the `HEAD` of the context's worktree, so
        // a context built for somewhere else would publish a commit this run
        // never made, with a payload hash naming the commit it did make. Both
        // paths are derived — one by the binary before the context existed, one
        // by [`CveMitigate::sweep`] — so a disagreement is a fact about the
        // caller, refused here rather than discovered after a push.
        let worktree = self.worktree();
        if self.context.work != worktree {
            return Err(CapabilityError::PublishesElsewhere {
                publishing: self.context.work.clone(),
                working: worktree,
            });
        }

        // **The scan's failure is held rather than propagated**, because the
        // disposition has a row for it — [`Reason::ScanUnusable`] — and that row
        // still deserves a report. The error is returned below, after the report
        // is on disk, and it is returned *whole* so that
        // [`CapabilityError::recurrence`] can delegate to the six-row table
        // `ScanError` owns.
        //
        // [`Reason::ScanUnusable`]: crate::evaluate::Reason::ScanUnusable
        let scanned = self.scanner.scan(&self.config.image).await;
        let run = match &scanned {
            Ok(report) => self.sweep(report).await?,
            Err(why) => Run::unusable(why.to_string()),
        };

        let concluded = disposition(&run);
        // Recorded before the scan error is returned, so the row a failed scan
        // reached is published on the arm that reaches it. See [`Observed`].
        self.observed.lock().unwrap().disposition = Some(concluded.published());
        self.write_report(&concluded)?;
        if let Err(why) = scanned {
            return Err(CapabilityError::Scan(why));
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
    /// Write the verdict report, and leave a receipt naming it.
    ///
    /// **On every path that reached a disposition, including the empty one.**
    /// [`Disposition::write_report`] states why: a consumer that had to tell *the
    /// file is absent* from *there was nothing to report* would be distinguishing
    /// a failed run from a clean one by a missing file, and absence reads as
    /// success.
    fn write_report(&self, concluded: &Disposition) -> Result<(), CapabilityError> {
        concluded
            .write_report(&self.config.report_dir)
            .map_err(|source| CapabilityError::Write {
                path: self
                    .config
                    .report_dir
                    .join(crate::cve::verdict::REPORT_FILE),
                source,
            })?;
        // **The file name, never the path it was written to.** The path
        // `write_report` hands back is a host absolute one —
        // `/var/folders/…/reports/verdicts.json` on the machine that ran — and a
        // published receipt quoting it says nothing to a reader on any other
        // machine while leaking the layout of the one that did. It is therefore
        // dropped rather than bound. Every other receipt this capability publishes is
        // logical (`cve:acme/r/pull/7`), and `<report.dir>` is the prefix a
        // bundle's own `published` path is already stripped against, for this
        // reason: a caller's payload stays the same whatever absolute prefix the
        // configuration happens to name.
        self.observed
            .lock()
            .unwrap()
            .receipts
            .push(EvidenceRef(format!(
                "{CVE_ORIGIN}:{}",
                crate::cve::verdict::REPORT_FILE
            )));
        Ok(())
    }
}

/// The four keys a run records about which tree its attempt worked in and which
/// image its verdicts were measured against.
///
/// Built from [`Checkout`] rather than from three fields the capability tracked,
/// so a bundle saying `attempt_tree: pr_head` provably has a pull request head in
/// it — the invariant that enum exists for.
///
/// # Fiddle does not build the image it scans, and this is where that is visible
///
/// Design §2.1's Prepare is *a detached worktree at the observed revision, then
/// `docker build`*. Only the first half is here, and the second half is not
/// missing — it is the **host workflow's**, decided 2026-08-18 and recorded in
/// ADR 020. Building inside this capability would put `docker build` in the
/// offline, credential-free gate, and a build that pulls base layers is not
/// something the scripted-stub approach carrying `wizcli`, `gh`, `git` and `go`
/// extends to: a stubbed build yields an image whose digest means nothing, which
/// is precisely the correspondence at issue.
///
/// So nothing in this build connects the two, and the ordering is why it cannot.
/// [`Capability::execute`] scans the statically configured `[orchestration.cve]
/// image` **before** [`CveMitigate::sweep`] is entered, so the document every
/// verdict is measured against is chosen before a worktree exists — and it
/// describes whatever image currently carries that tag.
/// `an_unusable_scanner_exits_eleven_and_reaches_no_forge` holds that order from
/// outside the process, by the worktree root not existing when the scan produced
/// nothing.
///
/// # What the pair does instead, which is not nothing
///
/// It makes the correspondence **checkable** rather than assumed. A bundle that
/// says *these verdicts are about digest `sha256:…` and I remediated revision
/// `abc…`* can be checked by the workflow that did the build, or by a person; one
/// that says neither cannot be, and until this the digest was parsed by
/// [`crate::scanner::wizcli`] and read by nothing at all.
///
/// The pairing is structural: `report` is a parameter rather than a field this
/// function could be called without, and [`TreeObservation`] has no `Default` —
/// so no run records a revision whose image is unknown. The other direction holds
/// by the ordering above: `sweep` is only entered with a document, and it is the
/// only producer of this value, so a run that scanned and made no tree publishes
/// **no `tree` key at all** rather than a half-pair a reader could mistake for a
/// correspondence.
///
/// What it still does not do is *verify* the pair, and ADR 020's consequences
/// name what would: the builder declaring the revision it built at, checked here
/// against `checkout.revision()`. Nothing populates such a field today, which is
/// why this records rather than refuses.
fn observed_tree(checkout: &Checkout, report: &ScanReport) -> TreeObservation {
    TreeObservation {
        base_revision: checkout.base_revision().to_string(),
        pr_head: checkout.pr_head().map(str::to_string),
        attempt_tree: checkout.attempt_tree().as_str().to_string(),
        // The scanner's own resolution, not `self.config.image`. The tag is the
        // name that was handed to the scan; the digest is what it turned out to
        // be, and recording the tag would record the very thing that can move.
        scanned_image_digest: report.image_digest.clone(),
    }
}

/// Every advisory in `findings`, in the order the projection produced them and
/// without repetition.
///
/// The one list the commit body, the repair contract and the record are all built
/// from, so a body naming an advisory the contract did not ask about is not a
/// thing this file can produce. Deduplicated because a projection may name one
/// advisory against two packages and a body that said `Fixes:` twice would be one
/// claim written twice.
fn advisories_of(findings: &[ProjectedFinding]) -> Vec<AdvisoryId> {
    let mut advisories: Vec<AdvisoryId> = Vec::new();
    for finding in findings {
        if !advisories.contains(&finding.cve) {
            advisories.push(finding.cve.clone());
        }
    }
    advisories
}

/// What the shared pull request's body says this run did.
///
/// One sentence about the one attempt and nothing else. The per-advisory rows are
/// the verdict report's, which is a document with a consumer of its own; a body
/// that duplicated them would be a second rendering to keep in step.
///
/// It counts **advisories** where it used to count groups, because a run has one
/// attempt now and *1 of 1* would be a sentence that never varied. What varies —
/// and what a person opening the pull request wants — is how many advisories were
/// in it and whether the tree it left is on the branch.
fn summary_of(attempted: &[Attempted]) -> String {
    let advisories: usize = attempted.iter().map(|it| it.findings.len()).sum();
    let committed = match attempted
        .iter()
        .any(|attempt| attempt.status == GroupStatus::Clean)
    {
        true => "committed what it changed",
        // All-or-nothing: one advisory the rescan still reported takes the whole
        // attempt back, including the edits that did clear their own findings. See
        // [`CveMitigate::judge`].
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
