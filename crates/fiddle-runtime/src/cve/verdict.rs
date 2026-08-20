use crate::agent::FindingDisposition;
use crate::capability::cve::{GroupStatus, MigrationAttempt};
use crate::cve::group::GroupError;
use crate::cve::project::Projection;
use crate::evaluate::Reason;
use fiddle_core::{AdvisoryId, Published, RunOutcome, Severity};
use std::path::{Path, PathBuf};

pub const REPORT_FILE: &str = "verdicts.json";

#[derive(Debug)]
pub struct Run {
    scan: Result<Projection, String>,

    pub already_fixed: Vec<AdvisoryId>,

    pub in_progress: Option<InProgress>,

    pub blocked: Vec<Blocked>,

    pub attempted: Vec<Attempted>,

    pub deferred: Vec<Deferred>,

    pub landed: Option<Landed>,
}

impl Run {
    pub fn scanned(projection: Projection) -> Self {
        Run {
            scan: Ok(projection),
            already_fixed: Vec::new(),
            in_progress: None,
            blocked: Vec::new(),
            attempted: Vec::new(),
            deferred: Vec::new(),
            landed: None,
        }
    }

    pub fn unusable(why: impl Into<String>) -> Self {
        Run {
            scan: Err(why.into()),
            already_fixed: Vec::new(),
            in_progress: None,
            blocked: Vec::new(),
            attempted: Vec::new(),
            deferred: Vec::new(),
            landed: None,
        }
    }

    pub fn projection(&self) -> Option<&Projection> {
        self.scan.as_ref().ok()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InProgress {
    pub number: u64,

    pub covers: Vec<AdvisoryId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Blocked {
    pub findings: Vec<fiddle_core::ProjectedFinding>,

    pub error: GroupError,
}

#[derive(Debug)]
pub struct Attempted {
    pub findings: Vec<fiddle_core::ProjectedFinding>,

    pub status: GroupStatus,

    pub attempt: MigrationAttempt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Landed {
    pub branch: String,
    pub pull_request: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Deferred {
    pub cve: AdvisoryId,
    pub bound: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Budget {
    max_findings: usize,
}

impl Budget {
    pub const DEFAULT_MAX_FINDINGS: usize = 5;

    pub fn of(max_findings: usize) -> Self {
        Budget { max_findings }
    }

    pub fn max_findings(&self) -> usize {
        self.max_findings
    }

    pub fn apply(
        &self,
        fixable: Vec<fiddle_core::ProjectedFinding>,
    ) -> (Vec<fiddle_core::ProjectedFinding>, Vec<Deferred>) {
        let mut taken = fixable;
        let overflow = taken.split_off(taken.len().min(self.max_findings));
        let deferred = overflow
            .into_iter()
            .map(|finding| Deferred {
                cve: finding.cve,
                bound: self.max_findings,
            })
            .collect();
        (taken, deferred)
    }
}

impl Default for Budget {
    fn default() -> Self {
        Budget::of(Budget::DEFAULT_MAX_FINDINGS)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Verdict {
    pub cve: AdvisoryId,

    pub package: String,

    pub rationale: String,

    pub severity: Severity,

    pub verdict: Judgement,

    #[serde(flatten)]
    pub disposed: Option<Disposed>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Disposed {
    pub attempted: bool,

    pub note: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Judgement {
    UpstreamBlocked,

    NeedsWork,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttemptRecord {
    pub cves: Vec<AdvisoryId>,

    pub status: GroupStatus,

    pub claimed_complete: bool,

    pub forbidden: Vec<crate::capability::cve::ForbiddenShape>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Disposition {
    outcome: RunOutcome,
    reason: Reason,
    deferred: Vec<Deferred>,
    verdicts: Vec<Verdict>,
    already_fixed: Vec<AdvisoryId>,
    attempts: Vec<AttemptRecord>,
    branch: Option<String>,
    pull_request: Option<u64>,
}

impl Disposition {
    pub fn outcome(&self) -> &RunOutcome {
        &self.outcome
    }

    pub fn reason(&self) -> &Reason {
        &self.reason
    }

    pub fn deferred(&self) -> &[Deferred] {
        &self.deferred
    }

    pub fn verdicts(&self) -> &[Verdict] {
        &self.verdicts
    }

    pub fn already_fixed(&self) -> &[AdvisoryId] {
        &self.already_fixed
    }

    pub fn attempts(&self) -> &[AttemptRecord] {
        &self.attempts
    }

    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    pub fn pull_request(&self) -> Option<u64> {
        self.pull_request
    }

    pub fn report(&self) -> serde_json::Value {
        serde_json::to_value(&self.verdicts).expect("a verdict holds no value serde can refuse")
    }

    pub fn published(&self) -> fiddle_core::RunDisposition {
        fiddle_core::RunDisposition {
            reason: self.reason.row().to_string(),
            verdicts: self.verdicts.len(),
            already_fixed: self.already_fixed.clone(),
            deferred: self
                .deferred
                .iter()
                .map(|deferred| fiddle_core::DeferredFinding {
                    cve: deferred.cve.clone(),
                    bound: deferred.bound,
                })
                .collect(),
            attempts: self
                .attempts
                .iter()
                .map(|attempt| fiddle_core::AttemptOutcome {
                    cves: attempt.cves.clone(),
                    status: match attempt.status {
                        GroupStatus::Clean => "clean",
                        GroupStatus::NeedsWork { .. } => "needs_work",
                    }
                    .to_string(),
                    claimed_complete: attempt.claimed_complete,
                    forbidden: attempt.forbidden.iter().map(ToString::to_string).collect(),
                })
                .collect(),
            branch: self.branch.clone(),
            pull_request: self.pull_request,
        }
    }

    pub fn write_report(&self, dir: &Path) -> Result<PathBuf, std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(REPORT_FILE);
        let mut document = serde_json::to_vec_pretty(&self.verdicts)
            .expect("a verdict holds no value serde can refuse");
        document.push(b'\n');
        std::fs::write(&path, document)?;
        Ok(path)
    }
}

pub fn disposition(run: &Run) -> Disposition {
    let projection = match &run.scan {
        Err(why) => {
            return Disposition {
                outcome: RunOutcome::Retryable {
                    reason: Published::of(why),
                },
                reason: Reason::ScanUnusable { why: why.clone() },
                deferred: Vec::new(),
                verdicts: Vec::new(),
                already_fixed: Vec::new(),
                attempts: Vec::new(),
                branch: None,
                pull_request: None,
            };
        }
        Ok(projection) => projection,
    };

    let verdicts = verdicts_of(run, projection);
    let attempts = attempts_of(run);
    let landed = |reason: Reason| Disposition {
        outcome: RunOutcome::Completed,
        reason,
        deferred: run.deferred.clone(),
        verdicts: verdicts.clone(),
        already_fixed: run.already_fixed.clone(),
        attempts: attempts.clone(),
        branch: None,
        pull_request: None,
    };

    if run
        .attempted
        .iter()
        .any(|group| group.status == GroupStatus::Clean)
    {
        return Disposition {
            branch: run.landed.as_ref().map(|it| it.branch.clone()),
            pull_request: run.landed.as_ref().map(|it| it.pull_request),
            ..landed(Reason::PullRequest)
        };
    }

    if !run.attempted.is_empty() {
        return landed(Reason::UnsafeWithoutDirection);
    }

    if !verdicts.is_empty() {
        return landed(Reason::VerdictsOnly);
    }

    if let Some(in_progress) = &run.in_progress {
        if !in_progress.covers.is_empty() {
            return Disposition {
                pull_request: Some(in_progress.number),
                ..landed(Reason::AlreadyInProgress)
            };
        }
    }

    if !run.already_fixed.is_empty() {
        return landed(Reason::AlreadyFixed);
    }

    landed(Reason::NothingToDo)
}

pub fn report_of(run: &Run) -> serde_json::Value {
    disposition(run).report()
}

fn verdicts_of(run: &Run, projection: &Projection) -> Vec<Verdict> {
    let mut verdicts = Vec::new();

    let no_fix = GroupError::NoFixedVersion.to_string();
    for finding in projection.upstream_blocked() {
        verdicts.push(verdict(finding, no_fix.clone(), Judgement::UpstreamBlocked));
    }

    for group in &run.blocked {
        let rationale = group.error.to_string();
        for finding in &group.findings {
            verdicts.push(verdict(
                finding,
                rationale.clone(),
                Judgement::UpstreamBlocked,
            ));
        }
    }

    for group in &run.attempted {
        let reason = match &group.status {
            GroupStatus::Clean => continue,
            GroupStatus::NeedsWork { reason } => reason,
        };
        let rationale = reason.to_string();
        for finding in &group.findings {
            verdicts.push(Verdict {
                disposed: disposed_of(&group.attempt.report.findings, &finding.cve),
                ..verdict(finding, rationale.clone(), Judgement::NeedsWork)
            });
        }
    }

    verdicts
}

fn verdict(
    finding: &fiddle_core::ProjectedFinding,
    rationale: String,
    judgement: Judgement,
) -> Verdict {
    Verdict {
        cve: finding.cve.clone(),
        package: finding.package.clone(),
        rationale,
        severity: finding.severity,
        verdict: judgement,
        disposed: None,
    }
}

fn disposed_of(reported: &[FindingDisposition], cve: &AdvisoryId) -> Option<Disposed> {
    reported
        .iter()
        .find(|disposition| disposition.cve == cve.as_str())
        .map(|disposition| Disposed {
            attempted: disposition.attempted,
            note: disposition.note.clone(),
        })
}

fn attempts_of(run: &Run) -> Vec<AttemptRecord> {
    run.attempted
        .iter()
        .map(|group| {
            let report = &group.attempt.report;
            AttemptRecord {
                cves: group
                    .findings
                    .iter()
                    .map(|finding| finding.cve.clone())
                    .collect(),
                status: group.status.clone(),
                claimed_complete: report.claimed_complete,
                forbidden: group.attempt.forbidden.clone(),
            }
        })
        .collect()
}
