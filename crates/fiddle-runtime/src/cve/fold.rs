use crate::cve::group::{Group, GroupError};
use crate::cve::project::project;
use crate::evaluate::{Evaluation, Outcome};
use fiddle_core::{AdvisoryId, Severities};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    AlreadyResolved,

    Proceed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landed {
    Committed,

    Reverted,
}

#[derive(Clone, Debug)]
pub struct PriorRescan {
    ended_clean: bool,

    landed: Landed,

    reported: Vec<AdvisoryId>,
}

impl PriorRescan {
    pub fn of(evaluation: &Evaluation, landed: Landed, acted_on: &Severities) -> Self {
        PriorRescan {
            ended_clean: evaluation.accepted(),
            landed,
            reported: reported_by(evaluation, acted_on),
        }
    }
}

fn reported_by(evaluation: &Evaluation, acted_on: &Severities) -> Vec<AdvisoryId> {
    let report = evaluation
        .checks()
        .iter()
        .rev()
        .find_map(|check| match &check.outcome {
            Outcome::Scanned(report) => Some(report),
            Outcome::Finished(_) | Outcome::NoArtefact(_) | Outcome::NotRun(_) => None,
        });

    match report.map(|report| project(report, acted_on)) {
        Some(Ok(projection)) => projection
            .all()
            .map(|finding| finding.cve.clone())
            .collect(),
        Some(Err(_)) | None => Vec::new(),
    }
}

pub fn fold(group: &Group, prior: Option<&PriorRescan>) -> Fold {
    let Some(prior) = prior else {
        return Fold::Proceed;
    };

    if !prior.ended_clean {
        return Fold::Proceed;
    }

    if prior.landed != Landed::Committed {
        return Fold::Proceed;
    }

    let cves = group.cves();

    if cves.is_empty() {
        return Fold::Proceed;
    }

    if cves.into_iter().all(|cve| !prior.reported.contains(cve)) {
        Fold::AlreadyResolved
    } else {
        Fold::Proceed
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupPlan {
    Attempt(String),

    AlreadyResolved,

    Blocked(GroupError),
}

pub fn plan_group(
    group: &Group,
    selection: Result<String, GroupError>,
    prior: Option<&PriorRescan>,
) -> GroupPlan {
    let target = match selection {
        Ok(target) => target,
        Err(GroupError::AlreadyAtTheFix { .. }) => return GroupPlan::AlreadyResolved,
        Err(
            error @ (GroupError::NoFixedVersion
            | GroupError::Unreadable { .. }
            | GroupError::NoRelease { .. }
            | GroupError::MajorBump { .. }
            | GroupError::Unselectable { .. }),
        ) => return GroupPlan::Blocked(error),
    };

    match fold(group, prior) {
        Fold::AlreadyResolved => GroupPlan::AlreadyResolved,
        Fold::Proceed => GroupPlan::Attempt(target),
    }
}

pub fn fold_commit_argv(group: &Group) -> Vec<String> {
    let ids: Vec<&str> = group.cves().iter().map(|cve| cve.as_str()).collect();
    [
        "commit",
        "--allow-empty",
        "-m",
        &format!(
            "fix: {} already resolved by an earlier bump",
            ids.join(", ")
        ),
    ]
    .iter()
    .map(|argument| argument.to_string())
    .collect()
}
