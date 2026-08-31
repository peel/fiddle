use super::stub::write_atomically;
use super::{Capability, CapabilityError, ExecutionInput};
use crate::effect::{EffectOutcome, EffectReceipt, Executor, IntegrationOperation, ObservedState};
use crate::github::{branch_name, EnsureBranchPublished, EnsureCheckRequested, EnsurePullRequest};
use fiddle_core::{
    correlation_key, CapabilityId, ChangeSetState, EffectName, EvidenceRef, Observation,
    ProposedEffect, Publication, Published, ReviewState, SourceRef, ENSURE_BRANCH_PUBLISHED,
    ENSURE_CHECK_REQUESTED, ENSURE_PULL_REQUEST,
};
use std::path::PathBuf;
use std::sync::Mutex;

const PUBLISH_ORIGIN: &str = "publish";

const OPEN: &str = "open";

pub struct PublishConfig {
    pub repo: String,

    pub head_owner: String,

    pub base: String,

    pub head_sha: String,

    pub title: String,

    pub body: String,

    pub workflow: String,

    pub required_checks: Vec<String>,

    pub stub_root: PathBuf,

    pub project: String,
}

pub struct PublishChange<'a> {
    executor: Executor<'a>,
    config: PublishConfig,
    receipts: Mutex<Vec<EvidenceRef>>,
    observed: Mutex<Observed>,
    publication: Mutex<Option<Publication>>,
}

#[derive(Default)]
struct Observed {
    branch: Option<String>,
    head_sha: Option<String>,
    pull_request: Option<u64>,
    failure: Option<String>,
}

impl PublishConfig {
    fn project_agrees_with(&self, executor: &Executor<'_>) -> bool {
        self.project == executor.project()
    }
}

impl<'a> PublishChange<'a> {
    pub fn new(executor: Executor<'a>, config: PublishConfig) -> Self {
        PublishChange {
            executor,
            config,
            receipts: Mutex::new(Vec::new()),
            observed: Mutex::new(Observed::default()),
            publication: Mutex::new(None),
        }
    }

    fn branch(&self) -> String {
        branch_name(self.executor.project(), self.executor.invocation_ref())
    }

    async fn propose<O>(
        &self,
        kind: EffectName,
        target: String,
        payload: String,
        operation: O,
    ) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, CapabilityError>
    where
        O: IntegrationOperation,
    {
        let proposed = ProposedEffect {
            capability: self.id(),
            kind: kind.clone(),
            target,
            payload,
        };
        let receipt = self.executor.execute(proposed, operation).await?;
        self.receipts
            .lock()
            .unwrap()
            .push(receipt_evidence(&kind, &receipt));
        Ok(receipt)
    }

    async fn publish(&self, branch: &str) -> Result<u64, CapabilityError> {
        let repo = &self.config.repo;

        let publish_branch = EnsureBranchPublished::new(
            repo.clone(),
            branch.to_string(),
            self.config.head_sha.clone(),
        );
        let published = self
            .propose(
                EffectName::shipped(ENSURE_BRANCH_PUBLISHED),
                publish_branch.target(),
                publish_branch.payload(),
                publish_branch,
            )
            .await?;
        {
            let mut observed = self.observed.lock().unwrap();
            observed.branch = Some(published.value.branch.clone());
            observed.head_sha = Some(published.value.sha.clone());
        }

        let open = EnsurePullRequest::new(
            repo.clone(),
            self.config.head_owner.clone(),
            branch.to_string(),
            self.config.base.clone(),
            self.config.title.clone(),
            self.config.body.clone(),
            false,
        );
        let opened = self
            .propose(
                EffectName::shipped(ENSURE_PULL_REQUEST),
                open.target(),
                open.payload(),
                open,
            )
            .await?;
        self.observed.lock().unwrap().pull_request = Some(opened.value.number);

        let request = EnsureCheckRequested::new(
            repo.clone(),
            self.config.workflow.clone(),
            branch.to_string(),
            self.executor.project(),
            self.executor.invocation_ref(),
        );
        self.propose(
            EffectName::shipped(ENSURE_CHECK_REQUESTED),
            request.target(),
            request.payload(),
            request,
        )
        .await?;

        Ok(opened.value.number)
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
        write_atomically(&destination, &state).map_err(|source| CapabilityError::Write {
            path: destination.clone(),
            source,
        })
    }

    async fn observe(&self) -> Publication {
        let source = || SourceRef(format!("github:{}", self.config.repo));
        let (branch, head_sha, pull_request, failure) = {
            let observed = self.observed.lock().unwrap();
            (
                observed.branch.clone(),
                observed.head_sha.clone(),
                observed.pull_request,
                observed.failure.clone(),
            )
        };

        let unreadable = |what: &str| {
            Published::of(match &failure {
                Some(why) => format!("{what}, so the forge was not read: {why}"),
                None => format!("{what}, so the forge was not read"),
            })
            .as_str()
            .to_string()
        };

        let review = match (&branch, head_sha.is_some()) {
            (Some(branch), true) => Observation::Available {
                value: ReviewState {
                    branch: Some(branch.clone()),
                    pull_request,
                    state: pull_request.map(|_| OPEN.to_string()),
                },
                source: source(),
                revision: head_sha.clone(),
            },
            _ => Observation::Unavailable {
                source: source(),
                reason: unreadable("no branch was observed"),
            },
        };

        let verification = match &head_sha {
            Some(head_sha) => {
                self.executor
                    .observe_checks(&self.config.repo, head_sha, &self.config.required_checks)
                    .await
            }
            None => Observation::Unavailable {
                source: source(),
                reason: unreadable("no head was published"),
            },
        };

        Publication {
            review,
            verification,
        }
    }
}

#[async_trait::async_trait]
impl Capability for PublishChange<'_> {
    fn id(&self) -> CapabilityId {
        fiddle_core::PUBLISH_CHANGE
    }

    fn stage(&self) -> &'static str {
        "publish"
    }

    async fn execute(&self, input: ExecutionInput<'_>) -> Result<EvidenceRef, CapabilityError> {
        let ExecutionInput {
            grant,
            work_id,
            invocation_ref,
            ..
        } = input;
        if grant.capability_id() != self.id() {
            return Err(CapabilityError::NotAuthorised {
                granted: grant.capability_id(),
                requested: self.id(),
            });
        }
        if invocation_ref != self.executor.invocation_ref()
            || !self.config.project_agrees_with(&self.executor)
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

        let branch = self.branch();
        let published = self.publish(&branch).await;
        if let Err(error) = &published {
            self.observed.lock().unwrap().failure = Some(error.to_string());
        }
        *self.publication.lock().unwrap() = Some(self.observe().await);
        let number = published?;

        self.record_change_set(work_id)?;
        Ok(EvidenceRef(format!(
            "{PUBLISH_ORIGIN}:{}/pull/{number}",
            self.config.repo
        )))
    }

    fn receipts(&self) -> Vec<EvidenceRef> {
        self.receipts.lock().unwrap().clone()
    }

    fn publication(&self) -> Option<Publication> {
        self.publication.lock().unwrap().clone()
    }
}

fn receipt_evidence<T>(kind: &EffectName, receipt: &EffectReceipt<T>) -> EvidenceRef {
    let outcome = match receipt.outcome {
        EffectOutcome::Committed => "committed",
        EffectOutcome::NotCommitted => "not_committed",
        EffectOutcome::Unknown => "unknown",
    };
    EvidenceRef(format!(
        "effect:{}:{}:{outcome}:{}:{}",
        kind.as_str(),
        receipt.effect_id.0,
        receipt.external_ref.as_deref().unwrap_or("-"),
        one_line(&receipt.postcondition),
    ))
}

fn one_line(text: &str) -> String {
    let flattened: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    Published::of(flattened).as_str().to_string()
}
