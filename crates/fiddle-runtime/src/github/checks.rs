use crate::effect::{
    required, AuthorizedEffect, EffectContext, EffectError, Executor, FromStepParams,
    IntegrationOperation, ObservedState, StepParams,
};
use crate::github::{encode, GhCli, GhError};
use fiddle_core::{
    effect_id, EffectId, EffectName, HumanDecisionRequirement, Observation, SourceRef,
    VerificationState, ENSURE_CHECK_REQUESTED,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckState {
    Absent,
    Queued,
    InProgress,
    Passed,
    Failed,
    Cancelled,
    TimedOut,
    ActionRequired,
    Neutral,
    Skipped,
    Stale,
    Unrecognized,
}

impl CheckState {
    pub fn is_passed(self) -> bool {
        matches!(self, CheckState::Passed)
    }

    pub fn is_pending(self) -> bool {
        matches!(self, CheckState::Queued | CheckState::InProgress)
    }

    pub fn blames_the_change(self) -> bool {
        matches!(self, CheckState::Failed)
    }
}

pub fn classify(status: &str, conclusion: Option<&str>) -> CheckState {
    match (status, conclusion) {
        ("queued" | "waiting" | "requested" | "pending", _) => CheckState::Queued,
        ("in_progress", _) => CheckState::InProgress,
        ("completed", Some(conclusion)) => match conclusion {
            "success" => CheckState::Passed,
            "failure" => CheckState::Failed,
            "cancelled" => CheckState::Cancelled,
            "timed_out" => CheckState::TimedOut,
            "action_required" => CheckState::ActionRequired,
            "neutral" => CheckState::Neutral,
            "skipped" => CheckState::Skipped,
            "stale" => CheckState::Stale,
            _ => CheckState::Unrecognized,
        },
        _ => CheckState::Unrecognized,
    }
}

fn check_runs_path(repo: &str, head_sha: &str) -> String {
    format!("/repos/{repo}/commits/{head_sha}/check-runs")
}

pub async fn observe_checks(
    gh: &GhCli,
    repo: &str,
    head_sha: &str,
    required: &[String],
    cancel: &CancellationToken,
) -> Observation<VerificationState> {
    let path = check_runs_path(repo, head_sha);
    let source = SourceRef(format!("github:{repo}/commits/{head_sha}/check-runs"));
    let unavailable = |reason: String| Observation::Unavailable {
        source: SourceRef(source.0.clone()),
        reason,
    };

    let response = match gh.api("GET", &path, None, cancel).await {
        Ok(response) => response,
        Err(error) => return unavailable(format!("the check runs could not be read: {error}")),
    };

    let runs = match response.body["check_runs"].as_array() {
        Some(runs) => runs,
        None => {
            return unavailable(format!(
                "{path} answered {} with no check_runs array",
                response.status
            ))
        }
    };

    let mut observed: Vec<(&str, CheckState)> = Vec::with_capacity(runs.len());
    for run in runs {
        let Some(name) = run["name"].as_str() else {
            return unavailable("a listed check run carried no name".to_string());
        };
        match run["head_sha"].as_str() {
            Some(sha) if sha == head_sha => {}
            Some(sha) => {
                return unavailable(format!(
                    "asked about {head_sha} and was answered a run for {sha}"
                ))
            }
            None => {
                return unavailable(format!("the check run {name:?} carried no head sha"));
            }
        }
        observed.push((
            name,
            classify(
                run["status"].as_str().unwrap_or_default(),
                run["conclusion"].as_str(),
            ),
        ));
    }

    let mut state = VerificationState {
        head_sha: head_sha.to_string(),
        required_missing: Vec::new(),
        failed: Vec::new(),
        pending: Vec::new(),
    };
    for name in required {
        let states: Vec<CheckState> = observed
            .iter()
            .filter(|(observed_name, _)| observed_name == name)
            .map(|(_, state)| *state)
            .collect();
        match states.as_slice() {
            [] => state.required_missing.push(name.clone()),
            states => {
                if states.iter().any(|s| !s.is_passed() && !s.is_pending()) {
                    state.failed.push(name.clone());
                } else if states.iter().any(|s| s.is_pending()) {
                    state.pending.push(name.clone());
                }
            }
        }
    }

    Observation::Available {
        value: state,
        source,
        revision: Some(head_sha.to_string()),
    }
}

pub fn run_name(effect_id: &EffectId) -> String {
    format!("fiddle-{}", effect_id.0)
}

pub fn check_request_target(repo: &str, workflow: &str, git_ref: &str) -> String {
    format!("{repo}/actions/workflows/{workflow}@{git_ref}")
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkflowRun {
    pub id: u64,
    pub name: String,
    pub status: String,
}

impl ObservedState for WorkflowRun {
    type Value = WorkflowRun;

    fn describe(&self) -> String {
        format!(
            "workflow run {} named {:?} is {}",
            self.id, self.name, self.status
        )
    }

    fn reference(&self) -> Option<String> {
        Some(self.id.to_string())
    }

    fn into_value(self) -> WorkflowRun {
        self
    }
}

pub struct EnsureCheckRequested {
    repo: String,
    workflow: String,
    git_ref: String,
    effect_id: EffectId,
}

impl EnsureCheckRequested {
    pub fn new(
        repo: String,
        workflow: String,
        git_ref: String,
        project: &str,
        invocation_ref: &str,
    ) -> Self {
        let effect_id = effect_id(
            project,
            invocation_ref,
            ENSURE_CHECK_REQUESTED,
            &check_request_target(&repo, &workflow, &git_ref),
        );
        Self {
            repo,
            workflow,
            git_ref,
            effect_id,
        }
    }

    pub fn target(&self) -> String {
        check_request_target(&self.repo, &self.workflow, &self.git_ref)
    }

    pub fn run_name(&self) -> String {
        run_name(&self.effect_id)
    }

    fn runs_path(&self) -> String {
        format!(
            "/repos/{}/actions/workflows/{}/runs?branch={}&event=workflow_dispatch",
            self.repo,
            self.workflow,
            encode(&self.git_ref),
        )
    }

    fn dispatch_path(&self) -> String {
        format!(
            "/repos/{}/actions/workflows/{}/dispatches",
            self.repo, self.workflow
        )
    }

    fn read(&self, listed: &serde_json::Value) -> Result<WorkflowRun, GhError> {
        let id = listed["id"]
            .as_u64()
            .ok_or_else(|| GhError::Malformed("a listed workflow run carried no id".to_string()))?;
        let name = listed["name"]
            .as_str()
            .ok_or_else(|| GhError::Malformed(format!("the workflow run {id} carried no name")))?
            .to_string();
        if name != self.run_name() {
            return Err(GhError::Malformed(format!(
                "asked for {} and was answered run {id}, which is {name:?}",
                self.run_name()
            )));
        }
        Ok(WorkflowRun {
            id,
            name,
            status: listed["status"].as_str().unwrap_or_default().to_string(),
        })
    }
}

impl FromStepParams for EnsureCheckRequested {
    fn from_params(executor: &Executor<'_>, params: &StepParams) -> Result<Self, EffectError> {
        let kind = EffectName::shipped(ENSURE_CHECK_REQUESTED);
        Ok(Self::new(
            required(&params.repo, &kind, "repo")?,
            required(&params.check_workflow, &kind, "check_workflow")?,
            required(&params.branch, &kind, "branch")?,
            executor.project(),
            executor.invocation_ref(),
        ))
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for EnsureCheckRequested {
    type State = WorkflowRun;

    type Error = GhError;

    fn kind(&self) -> EffectName {
        EffectName::shipped(ENSURE_CHECK_REQUESTED)
    }

    fn target(&self) -> String {
        EnsureCheckRequested::target(self)
    }

    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    fn payload(&self) -> String {
        serde_json::json!({
            "inputs": { "fiddle_effect_id": self.effect_id.0 },
            "ref": self.git_ref,
            "repo": self.repo,
            "workflow": self.workflow,
        })
        .to_string()
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<WorkflowRun>, GhError> {
        let response = ctx
            .gh
            .api("GET", &self.runs_path(), None, &ctx.cancel)
            .await?;

        let runs = response.body["workflow_runs"].as_array().ok_or_else(|| {
            GhError::Malformed(format!(
                "{} answered {} with no workflow_runs array",
                self.runs_path(),
                response.status
            ))
        })?;

        let wanted = self.run_name();
        let ours: Vec<&serde_json::Value> = runs
            .iter()
            .filter(|run| run["name"].as_str() == Some(wanted.as_str()))
            .collect();

        match ours.as_slice() {
            [] => Ok(None),
            [one] => self.read(one).map(Some),
            many => Err(GhError::Duplicate { count: many.len() }),
        }
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        if authorized.effect_id() != &self.effect_id {
            return Err(GhError::NotSent(format!(
                "the run would be named {} and looked up as {}; nothing was dispatched",
                run_name(authorized.effect_id()),
                self.run_name(),
            )));
        }
        let body = serde_json::json!({
            "ref": self.git_ref,
            "inputs": { "fiddle_effect_id": self.effect_id.0 },
        });
        ctx.gh
            .api("POST", &self.dispatch_path(), Some(&body), &ctx.cancel)
            .await
            .map(|_response| ())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlamedCheck {
    pub name: String,
    pub details_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenuineFailure {
    pub head_sha: String,
    pub blamed: Vec<BlamedCheck>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settlement {
    pub read: usize,
    pub settled: usize,
    pub failure: Option<GenuineFailure>,
}

impl Settlement {
    pub fn has_settled(&self) -> bool {
        self.read == self.settled
    }

    pub fn pending(&self) -> usize {
        self.read.saturating_sub(self.settled)
    }
}

fn has_settled(run: &serde_json::Value) -> bool {
    !matches!(
        classify(
            run["status"].as_str().unwrap_or_default(),
            run["conclusion"].as_str(),
        ),
        CheckState::Queued | CheckState::InProgress
    )
}

fn tests_the_head(run: &serde_json::Value, head_sha: &str) -> bool {
    run["head_sha"].as_str() == Some(head_sha)
}

fn blames_the_change(run: &serde_json::Value) -> bool {
    classify(
        run["status"].as_str().unwrap_or_default(),
        run["conclusion"].as_str(),
    )
    .blames_the_change()
}

fn blame(run: &serde_json::Value) -> BlamedCheck {
    BlamedCheck {
        name: run["name"].as_str().unwrap_or_default().to_string(),
        details_url: run["details_url"].as_str().map(str::to_string),
    }
}

pub async fn observe_genuine_failure(
    gh: &GhCli,
    repo: &str,
    head_sha: &str,
    cancel: &CancellationToken,
) -> Observation<Settlement> {
    let path = check_runs_path(repo, head_sha);
    let source = SourceRef(format!("github:{repo}/commits/{head_sha}/check-runs"));
    let unavailable = |reason: String| Observation::Unavailable {
        source: SourceRef(source.0.clone()),
        reason,
    };

    let response = match gh.api("GET", &path, None, cancel).await {
        Ok(response) => response,
        Err(error) => return unavailable(format!("the check runs could not be read: {error}")),
    };

    let Some(runs) = response.body["check_runs"].as_array() else {
        return unavailable(format!(
            "{path} answered {} with no check_runs array",
            response.status
        ));
    };

    let blamed: Vec<BlamedCheck> = runs
        .iter()
        .filter(|run| tests_the_head(run, head_sha) && blames_the_change(run))
        .map(blame)
        .collect();

    Observation::Available {
        value: Settlement {
            read: runs.len(),
            settled: runs.iter().filter(|run| has_settled(run)).count(),
            failure: match blamed.is_empty() {
                true => None,
                false => Some(GenuineFailure {
                    head_sha: head_sha.to_string(),
                    blamed,
                }),
            },
        },
        source,
        revision: Some(head_sha.to_string()),
    }
}
