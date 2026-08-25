use fiddle_core::{
    DeploymentRule, EffectName, HumanDecisionRequirement, ProposedEffect, FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    install, AdapterError, AuthorizedEffect, DeploymentPolicy, EffectContext, EffectDescriptor,
    EffectError, EffectOutcome, EffectPhase, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ObservedState, ReadRetry,
};
use fiddle_runtime::git::GitCli;
use fiddle_runtime::{GhCli, GhError};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const PROJECT: &str = "acme/widget";

const INVOCATION_REF: &str = "beans:w-1";

const TRANSITION: &str = "jira.issue_transitioned";

const ISSUE: &str = "ACME-7";

const PAYLOAD: &str = r#"{"to":"In Review"}"#;

const JIRA: &[EffectDescriptor] = &[EffectDescriptor {
    name: TRANSITION,
    minimum: HumanDecisionRequirement::Automatic,
}];

#[derive(Debug, thiserror::Error)]
enum JiraError {
    #[error("the issue tracker refused the transition: {0}")]
    Refused(String),
    #[error("the transition was sent and the tracker never answered")]
    Lost,
}

impl AdapterError for JiraError {
    fn outcome(&self, _phase: EffectPhase) -> EffectOutcome {
        match self {
            JiraError::Refused(_) => EffectOutcome::NotCommitted,
            JiraError::Lost => EffectOutcome::Unknown,
        }
    }
}

struct TransitionedIssue {
    status: String,
}

impl ObservedState for TransitionedIssue {
    type Value = String;

    fn describe(&self) -> String {
        format!("{ISSUE} is {}", self.status)
    }

    fn reference(&self) -> Option<String> {
        Some(ISSUE.to_string())
    }

    fn into_value(self) -> String {
        self.status
    }
}

#[derive(Debug, Default)]
struct Tracker {
    transitioned: AtomicBool,
    reads: AtomicUsize,
    writes: AtomicUsize,
}

struct TransitionIssue<'t> {
    tracker: &'t Tracker,
    refusal: Option<fn() -> JiraError>,
}

#[async_trait::async_trait]
impl IntegrationOperation for TransitionIssue<'_> {
    type State = TransitionedIssue;

    type Error = JiraError;

    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    fn payload(&self) -> String {
        PAYLOAD.to_string()
    }

    async fn inspect(&self, _ctx: &EffectContext) -> Result<Option<TransitionedIssue>, JiraError> {
        self.tracker.reads.fetch_add(1, Ordering::SeqCst);
        match self.tracker.transitioned.load(Ordering::SeqCst) {
            false => Ok(None),
            true => Ok(Some(TransitionedIssue {
                status: "In Review".to_string(),
            })),
        }
    }

    async fn apply(
        &self,
        _ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), JiraError> {
        if let Some(refusal) = self.refusal {
            return Err(refusal());
        }
        self.tracker.writes.fetch_add(1, Ordering::SeqCst);
        self.tracker.transitioned.store(true, Ordering::SeqCst);
        Ok(())
    }
}

struct Allow;

impl DeploymentPolicy for Allow {
    fn rule_for(&self, _kind: &EffectName) -> DeploymentRule {
        DeploymentRule::Allow
    }
}

struct Silent;

impl EffectTrace for Silent {
    fn step(&self, _kind: &EffectName, _step: ExecutionStep) {}
}

fn unreachable_context() -> EffectContext {
    EffectContext::new(
        GhCli::new(
            PathBuf::from("/nonexistent/gh"),
            Vec::new(),
            String::new(),
            "GH_TOKEN",
            PathBuf::from("/nonexistent"),
            Duration::from_secs(1),
        ),
        GitCli::new(
            PathBuf::from("/nonexistent/git"),
            String::new(),
            "FIDDLE_GITHUB_TOKEN",
            Duration::from_secs(1),
        ),
        PathBuf::from("/nonexistent"),
        CancellationToken::new(),
    )
}

fn proposed() -> ProposedEffect {
    ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectName::parse(TRANSITION).unwrap(),
        target: ISSUE.to_string(),
        payload: PAYLOAD.to_string(),
    }
}

fn registered() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        install(JIRA).expect("the tracker's effect is installed once for this binary");
    });
}

async fn transition(
    tracker: &Tracker,
    refusal: Option<fn() -> JiraError>,
) -> Result<String, EffectError> {
    let ctx = unreachable_context();
    let deployment = Allow;
    let trace = Silent;
    let executor = Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        &trace,
        ReadRetry::none(),
    );
    executor
        .execute(proposed(), TransitionIssue { tracker, refusal })
        .await
        .map(|receipt| receipt.value)
}

#[tokio::test]
async fn an_operation_that_never_names_gherror_reaches_the_executor() {
    registered();
    let tracker = Tracker::default();

    let status = transition(&tracker, None)
        .await
        .expect("an adapter outside GitHub must reach the executor unchanged");

    assert_eq!(status, "In Review");
    assert_eq!(
        tracker.writes.load(Ordering::SeqCst),
        1,
        "the executor drove the tracker's own apply"
    );
}

#[tokio::test]
async fn a_tracker_refusal_is_reported_as_its_own_error_and_not_as_a_gherror() {
    registered();
    let tracker = Tracker::default();

    let error = transition(
        &tracker,
        Some(|| JiraError::Refused("no such transition".into())),
    )
    .await
    .expect_err("a refused transition is not a receipt");

    let source = error
        .adapter_source::<JiraError>()
        .unwrap_or_else(|| panic!("the tracker's own error must survive the boundary: {error:?}"));
    assert!(matches!(source, JiraError::Refused(_)), "got {source:?}");
    assert!(
        error.adapter_source::<GhError>().is_none(),
        "and it must not be readable as a GitHub failure: {error:?}"
    );
    assert!(
        !tracker.transitioned.load(Ordering::SeqCst),
        "a refusal the adapter called NotCommitted moved nothing"
    );
}

#[tokio::test]
async fn a_lost_answer_from_a_tracker_is_unresolved_because_the_adapter_called_it_unknown() {
    registered();
    let tracker = Tracker::default();

    let error = transition(&tracker, Some(|| JiraError::Lost))
        .await
        .expect_err("an answer nobody heard is not a receipt");

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "an Unknown outcome must not be reported as a refusal: {error:?}"
    );
    assert!(
        error.adapter_source::<JiraError>().is_none(),
        "and an unknown outcome is not an adapter failure to retry: {error:?}"
    );
}

#[tokio::test]
async fn an_adapter_that_classifies_only_its_outcome_needs_no_other_method() {
    registered();
    let tracker = Tracker::default();

    let _ = transition(&tracker, Some(|| JiraError::Lost)).await;

    assert_eq!(
        tracker.reads.load(Ordering::SeqCst),
        2,
        "the default is_worth_reading_again refuses a second look, so the reads are \
         the one before the write and the one after"
    );
    assert_eq!(
        JiraError::Lost.advice(),
        fiddle_runtime::RetryAdvice::default(),
        "an adapter that says nothing about backoff advises nothing"
    );
    assert_eq!(
        JiraError::Lost.duplicates(),
        None,
        "and an adapter that names no duplicate observation never yields DuplicateState"
    );
}
