mod in_workspace;

pub use in_workspace::{InWorkspace, Rescan};

use crate::cve::project::{project, Arm};
use crate::scanner::{ScanError, ScanReport};
use async_trait::async_trait;
use fiddle_core::{AdvisoryId, Severities, Severity};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Success {
    ExitZero,

    ExitZeroAndNoOutput,

    ArtefactWritten,
}

#[derive(Clone, Debug)]
pub struct Check {
    pub program: String,

    pub args: Vec<String>,

    pub success: Success,
}

impl Check {
    pub fn name(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Clone, Debug)]
pub struct Contract {
    pub checks: Vec<Check>,

    pub severities: Severities,

    pub repair: Option<Repair>,
}

impl Contract {
    pub fn of(checks: Vec<Check>) -> Self {
        Self {
            checks,
            severities: Severities::default(),
            repair: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Repair {
    pub must_clear: Vec<AdvisoryId>,

    pub input: Vec<AdvisoryId>,

    pub scanned_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reason {
    NewFindingAppeared {
        cve: AdvisoryId,
        severity: Severity,
    },

    Provisional {
        scanned_at: String,
        rescanned_at: String,
    },

    NothingToDo,

    VerdictsOnly,

    AlreadyInProgress,

    AlreadyFixed,

    PullRequest,

    UnsafeWithoutDirection,

    ScanUnusable {
        why: String,
    },
}

impl Reason {
    pub fn row(&self) -> &'static str {
        match self {
            Reason::NewFindingAppeared { .. } => "new_finding_appeared",
            Reason::Provisional { .. } => "provisional",
            Reason::NothingToDo => "nothing_to_do",
            Reason::VerdictsOnly => "verdicts_only",
            Reason::AlreadyInProgress => "already_in_progress",
            Reason::AlreadyFixed => "already_fixed",
            Reason::PullRequest => "pull_request",
            Reason::UnsafeWithoutDirection => "unsafe_without_direction",
            Reason::ScanUnusable { .. } => "scan_unusable",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RescanVerdict {
    NotCompared,

    Cleared,

    Provisional(Reason),

    NotObserved { array: &'static str },

    StillReported(Vec<AdvisoryId>),

    NewFinding(Reason),

    Unreadable(String),
}

#[derive(Clone, Debug)]
pub struct Answered {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum Unanswered {
    NotStarted {
        program: String,
        source: std::io::Error,
    },

    TimedOut {
        program: String,
        timeout: std::time::Duration,
    },

    Cancelled,
}

#[async_trait]
pub trait Tree: Sync {
    async fn run(&self, check: &Check) -> Result<Answered, Unanswered>;

    async fn scan(&self, check: &Check) -> Result<ScanReport, ScanError>;
}

#[derive(Debug)]
pub enum Outcome {
    Finished(Answered),

    Scanned(ScanReport),

    NoArtefact(String),

    NotRun(String),
}

#[derive(Debug)]
pub struct CheckResult {
    pub name: String,

    pub passed: bool,

    pub outcome: Outcome,
}

#[derive(Debug)]
pub struct Evaluation {
    checks: Vec<CheckResult>,
    rescan: RescanVerdict,
}

impl Evaluation {
    pub fn checks(&self) -> &[CheckResult] {
        &self.checks
    }

    pub fn first_failure(&self) -> Option<&CheckResult> {
        self.checks.iter().find(|check| !check.passed)
    }

    pub fn rescan(&self) -> &RescanVerdict {
        &self.rescan
    }

    pub fn reason(&self) -> Option<&Reason> {
        match &self.rescan {
            RescanVerdict::Provisional(reason) | RescanVerdict::NewFinding(reason) => Some(reason),
            RescanVerdict::NotCompared
            | RescanVerdict::Cleared
            | RescanVerdict::NotObserved { .. }
            | RescanVerdict::StillReported(_)
            | RescanVerdict::Unreadable(_) => None,
        }
    }

    pub fn accepted(&self) -> bool {
        self.first_failure().is_none() && matches!(self.rescan, RescanVerdict::Cleared)
    }

    pub fn rejected(&self) -> bool {
        self.first_failure().is_some()
            || matches!(
                self.rescan,
                RescanVerdict::StillReported(_)
                    | RescanVerdict::NewFinding(_)
                    | RescanVerdict::Unreadable(_)
            )
    }
}

#[derive(Debug, thiserror::Error)]
#[error("the evaluation was cancelled, so this tree was neither accepted nor rejected")]
pub struct Cancelled;

pub async fn evaluate(contract: &Contract, tree: &impl Tree) -> Result<Evaluation, Cancelled> {
    let mut checks = Vec::with_capacity(contract.checks.len());
    for check in &contract.checks {
        let (outcome, passed) = match check.success {
            Success::ExitZero | Success::ExitZeroAndNoOutput => match tree.run(check).await {
                Ok(ran) => {
                    let output_is_the_complaint = check.success == Success::ExitZeroAndNoOutput;
                    let quiet = ran.stdout.is_empty() && ran.stderr.is_empty();
                    let passed = ran.exit_code == 0 && (!output_is_the_complaint || quiet);
                    (Outcome::Finished(ran), passed)
                }
                Err(Unanswered::NotStarted { program, source }) => {
                    (Outcome::NotRun(not_started(&program, &source)), false)
                }
                Err(Unanswered::TimedOut { program, timeout }) => (
                    Outcome::NotRun(format!(
                        "{program} did not finish within {timeout:?} and was killed"
                    )),
                    false,
                ),
                Err(Unanswered::Cancelled) => return Err(Cancelled),
            },

            Success::ArtefactWritten => match tree.scan(check).await {
                Ok(report) => (Outcome::Scanned(report), true),
                Err(error @ ScanError::Missing { .. }) => {
                    (Outcome::NotRun(error.to_string()), false)
                }
                Err(error) => (Outcome::NoArtefact(error.to_string()), false),
            },
        };

        checks.push(CheckResult {
            name: check.name(),
            passed,
            outcome,
        });
    }

    let report = checks.iter().rev().find_map(|check| match &check.outcome {
        Outcome::Scanned(report) => Some(report),
        Outcome::Finished(_) | Outcome::NoArtefact(_) | Outcome::NotRun(_) => None,
    });

    let rescan = match (&contract.repair, report) {
        (Some(repair), Some(report)) => judge(repair, report, &contract.severities),
        _ => RescanVerdict::NotCompared,
    };

    Ok(Evaluation { checks, rescan })
}

fn judge(repair: &Repair, report: &ScanReport, acted_on: &Severities) -> RescanVerdict {
    let projection = match project(report, acted_on) {
        Ok(projection) => projection,
        Err(why) => return RescanVerdict::Unreadable(why.to_string()),
    };

    let still_reported: Vec<AdvisoryId> = repair
        .must_clear
        .iter()
        .filter(|&cve| projection.all().any(|finding| &finding.cve == cve))
        .cloned()
        .collect();
    if !still_reported.is_empty() {
        return RescanVerdict::StillReported(still_reported);
    }

    if let Some(appeared) = projection
        .all()
        .find(|finding| acted_on.contains(finding.severity) && !repair.input.contains(&finding.cve))
    {
        return RescanVerdict::NewFinding(Reason::NewFindingAppeared {
            cve: appeared.cve.clone(),
            severity: appeared.severity,
        });
    }

    for (array, arm) in [
        ("libraries", projection.library_arm()),
        ("osPackages", projection.os_arm()),
    ] {
        if arm == Arm::Absent {
            return RescanVerdict::NotObserved { array };
        }
    }

    if report.scanner_version != repair.scanned_at {
        return RescanVerdict::Provisional(Reason::Provisional {
            scanned_at: repair.scanned_at.clone(),
            rescanned_at: report.scanner_version.clone(),
        });
    }

    RescanVerdict::Cleared
}

fn not_started(program: &str, source: &std::io::Error) -> String {
    match source.kind() {
        std::io::ErrorKind::NotFound => format!("no such program {program}"),
        _ => format!("{program} could not be started: {source}"),
    }
}
