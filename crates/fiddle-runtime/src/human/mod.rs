pub mod interpret;
pub mod validate;

use crate::effect::{
    required, AuthorizedEffect, EffectContext, EffectError, EffectReceipt, Executor,
    FromStepParams, IntegrationOperation, ObservedState, StepParams,
};
use crate::github::{read_conversation, GhError};
use crate::jira::comment::AddComment;
use crate::jira::JiraError;
use fiddle_core::{
    parse_marker, render_marker, EffectName, HumanDecisionRequest, HumanDecisionRequirement,
    MarkerError, ProposedEffect, PUBLISH_DECISION_REQUEST,
};

pub use crate::github::HumanResponse;

pub(crate) const CONVERSATION_PAGES: u32 = 10;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum InteractionRef {
    GitHubPullRequestComment { repo: String, pr: u64, comment: u64 },
    JiraIssueComment { issue: String, comment: String },
}

impl InteractionRef {
    pub fn channel(&self) -> &'static str {
        match self {
            InteractionRef::GitHubPullRequestComment { .. } => GITHUB,
            InteractionRef::JiraIssueComment { .. } => JIRA,
        }
    }
}

impl std::fmt::Display for InteractionRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InteractionRef::GitHubPullRequestComment { repo, pr, comment } => {
                write!(f, "{repo}#{pr} comment {comment}")
            }
            InteractionRef::JiraIssueComment { issue, comment } => {
                write!(f, "{issue} comment {comment}")
            }
        }
    }
}

pub const GITHUB: &str = "github";

pub const JIRA: &str = "jira";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecisionChannel {
    GitHubPullRequest { repo: String, pr: u64 },
    JiraIssue { issue: String, updated: String },
}

impl DecisionChannel {
    pub fn channel(&self) -> &'static str {
        match self {
            DecisionChannel::GitHubPullRequest { .. } => GITHUB,
            DecisionChannel::JiraIssue { .. } => JIRA,
        }
    }
}

impl std::fmt::Display for DecisionChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionChannel::GitHubPullRequest { repo, pr } => write!(f, "github {repo}#{pr}"),
            DecisionChannel::JiraIssue { issue, updated } => {
                write!(f, "jira {issue}@{updated}")
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ChannelError {
    #[error(
        "no channel is named for this decision request and exactly one channel is authoritative \
         for one request, so nothing was published"
    )]
    NoneNamed,
    #[error(
        "{named} channels are named for this decision request and exactly one channel is \
         authoritative for one request, so nothing was published to any of them: {spelled}"
    )]
    NotOne { named: usize, spelled: String },
}

pub fn authoritative(named: &[DecisionChannel]) -> Result<&DecisionChannel, ChannelError> {
    match named {
        [] => Err(ChannelError::NoneNamed),
        [only] => Ok(only),
        many => Err(ChannelError::NotOne {
            named: many.len(),
            spelled: many
                .iter()
                .map(DecisionChannel::to_string)
                .collect::<Vec<String>>()
                .join(", "),
        }),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("{0}")]
    Channel(#[from] ChannelError),
    #[error("{0}")]
    Unpublished(#[from] EffectError),
    #[error("the named jira issue carries no revision this run can build an identity from: {0}")]
    Unaddressable(#[from] JiraError),
}

pub async fn publish(
    executor: &Executor<'_>,
    named: &[DecisionChannel],
    request: &HumanDecisionRequest,
) -> Result<EffectReceipt<InteractionRef>, PublishError> {
    match authoritative(named)? {
        DecisionChannel::GitHubPullRequest { repo, pr } => {
            let ask = PublishDecisionRequest::new(repo.clone(), *pr, request.clone());
            Ok(executor.execute(proposing(executor, &ask), ask).await?)
        }
        DecisionChannel::JiraIssue { issue, updated } => {
            let ask = AddComment::new(
                issue.clone(),
                updated,
                render_request(request),
                executor.project(),
                executor.invocation_ref(),
            )?;
            let receipt = executor.execute(proposing(executor, &ask), ask).await?;
            Ok(EffectReceipt {
                effect_id: receipt.effect_id,
                payload_hash: receipt.payload_hash,
                target: receipt.target,
                outcome: receipt.outcome,
                postcondition: receipt.postcondition,
                external_ref: receipt.external_ref,
                value: InteractionRef::JiraIssueComment {
                    issue: receipt.value.issue,
                    comment: receipt.value.comment_id,
                },
            })
        }
    }
}

fn proposing<O: IntegrationOperation>(executor: &Executor<'_>, ask: &O) -> ProposedEffect {
    ProposedEffect {
        capability: executor.capability(),
        kind: ask.kind(),
        target: ask.target(),
        payload: ask.payload(),
    }
}

pub fn decision_request_target(
    repo: &str,
    pr: u64,
    request: &fiddle_core::DecisionRequestId,
) -> String {
    format!("{repo}#{pr}:{}", request.0)
}

pub fn render_request(request: &HumanDecisionRequest) -> String {
    let mut body = String::from("**fiddle needs a decision before it can continue.**\n\n");
    body.push_str(&request.question);
    body.push_str("\n\n");
    body.push_str(&request.rationale);
    body.push('\n');

    for (heading, items) in [
        ("Risks", &request.risks),
        ("Alternatives considered", &request.alternatives),
    ] {
        if items.is_empty() {
            continue;
        }
        body.push_str(&format!("\n**{heading}**\n\n"));
        for item in items {
            body.push_str(&format!("- {item}\n"));
        }
    }
    if !request.evidence.is_empty() {
        body.push_str("\n**Evidence**\n\n");
        for reference in &request.evidence {
            body.push_str(&format!("- {reference}\n"));
        }
    }

    body.push_str(&format!("\n_Asked by {} for ", request.capability));
    match &request.work_ref {
        Some(work) => body.push_str(&format!("{work}")),
        None => body.push_str(&request.invocation_ref),
    }
    body.push_str(&format!(" at {}._\n\n", request.binding.head_sha));

    body.push_str(&render_marker(&request.binding));
    body
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishedRequest {
    pub repo: String,
    pub pr: u64,
    pub comment: u64,
}

impl ObservedState for PublishedRequest {
    type Value = InteractionRef;

    fn describe(&self) -> String {
        format!(
            "the decision request is published as {}",
            InteractionRef::GitHubPullRequestComment {
                repo: self.repo.clone(),
                pr: self.pr,
                comment: self.comment,
            }
        )
    }

    fn reference(&self) -> Option<String> {
        Some(self.comment.to_string())
    }

    fn into_value(self) -> InteractionRef {
        InteractionRef::GitHubPullRequestComment {
            repo: self.repo,
            pr: self.pr,
            comment: self.comment,
        }
    }
}

pub struct PublishDecisionRequest {
    repo: String,
    pr: u64,
    request: HumanDecisionRequest,
}

impl PublishDecisionRequest {
    pub fn new(repo: String, pr: u64, request: HumanDecisionRequest) -> Self {
        Self { repo, pr, request }
    }

    pub fn target(&self) -> String {
        decision_request_target(&self.repo, self.pr, self.asking())
    }

    fn asking(&self) -> &fiddle_core::DecisionRequestId {
        &self.request.binding.request
    }

    fn comments_path(&self) -> String {
        format!("/repos/{}/issues/{}/comments", self.repo, self.pr)
    }

    fn is_this_request(&self, body: &str) -> bool {
        match parse_marker(body) {
            Ok(binding) => &binding.request == self.asking(),
            Err(MarkerError::Absent | MarkerError::Malformed(_) | MarkerError::Version(_)) => false,
        }
    }
}

impl FromStepParams for PublishDecisionRequest {
    fn from_params(_executor: &Executor<'_>, params: &StepParams) -> Result<Self, EffectError> {
        let kind = EffectName::shipped(PUBLISH_DECISION_REQUEST);
        Ok(Self::new(
            required(&params.repo, &kind, "repo")?,
            required(&params.pull_request, &kind, "pull_request")?,
            required(&params.decision_request, &kind, "decision_request")?,
        ))
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for PublishDecisionRequest {
    type State = PublishedRequest;

    type Error = GhError;

    fn kind(&self) -> EffectName {
        EffectName::shipped(PUBLISH_DECISION_REQUEST)
    }

    fn target(&self) -> String {
        PublishDecisionRequest::target(self)
    }

    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    fn payload(&self) -> String {
        render_request(&self.request)
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<PublishedRequest>, GhError> {
        let conversation = read_conversation(
            &ctx.gh,
            &self.repo,
            self.pr,
            CONVERSATION_PAGES,
            &ctx.cancel,
        )
        .await?;

        let mine: Vec<u64> = conversation
            .iter()
            .filter(|comment| self.is_this_request(&comment.body))
            .map(|comment| comment.comment)
            .collect();

        match mine.as_slice() {
            [] => Ok(None),
            [comment] => Ok(Some(PublishedRequest {
                repo: self.repo.clone(),
                pr: self.pr,
                comment: *comment,
            })),
            several => Err(GhError::Duplicate {
                count: several.len(),
            }),
        }
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        GitHubConversation
            .request(ctx, self, authorized)
            .await
            .map(|_said_by_github| ())
    }
}

#[async_trait::async_trait]
pub trait HumanInteractionPort: Send + Sync {
    type Ask: IntegrationOperation;

    type Reply: Send;

    type Error: std::error::Error + Send + Sync;

    async fn request(
        &self,
        ctx: &EffectContext,
        request: &Self::Ask,
        authorized: &AuthorizedEffect<Self::Ask>,
    ) -> Result<InteractionRef, Self::Error>;

    async fn responses(
        &self,
        ctx: &EffectContext,
        interaction: &InteractionRef,
    ) -> Result<Vec<Self::Reply>, Self::Error>;
}

pub struct GitHubConversation;

#[async_trait::async_trait]
impl HumanInteractionPort for GitHubConversation {
    type Ask = PublishDecisionRequest;

    type Reply = HumanResponse;

    type Error = GhError;

    async fn request(
        &self,
        ctx: &EffectContext,
        request: &PublishDecisionRequest,
        _authorized: &AuthorizedEffect<PublishDecisionRequest>,
    ) -> Result<InteractionRef, GhError> {
        let path = request.comments_path();
        let body = serde_json::json!({ "body": request.payload() });
        let response = ctx.gh.api("POST", &path, Some(&body), &ctx.cancel).await?;
        let comment = response.body["id"].as_u64().ok_or_else(|| {
            GhError::Malformed(format!(
                "{path} answered {} with no comment id",
                response.status
            ))
        })?;
        Ok(InteractionRef::GitHubPullRequestComment {
            repo: request.repo.clone(),
            pr: request.pr,
            comment,
        })
    }

    async fn responses(
        &self,
        ctx: &EffectContext,
        interaction: &InteractionRef,
    ) -> Result<Vec<HumanResponse>, GhError> {
        match interaction {
            InteractionRef::GitHubPullRequestComment { repo, pr, .. } => {
                read_conversation(&ctx.gh, repo, *pr, CONVERSATION_PAGES, &ctx.cancel).await
            }
            InteractionRef::JiraIssueComment { .. } => Err(GhError::NotSent(format!(
                "{interaction} is a jira interaction and this port reads a github conversation; \
                 exactly one channel is authoritative for one request, so no github comment was \
                 read for it"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conversation() -> InteractionRef {
        InteractionRef::GitHubPullRequestComment {
            repo: "peel/fiddle-effects-acceptance".to_string(),
            pr: 4,
            comment: 2_147_483_647,
        }
    }

    #[test]
    fn a_conversation_renders_as_the_repository_the_pull_request_and_the_comment() {
        assert_eq!(
            conversation().to_string(),
            "peel/fiddle-effects-acceptance#4 comment 2147483647"
        );
    }

    #[test]
    fn no_component_of_the_conversation_is_dropped() {
        let rendered = conversation().to_string();
        for part in ["peel/fiddle-effects-acceptance", "#4", "2147483647"] {
            assert!(rendered.contains(part), "{part} is missing from {rendered}");
        }
    }
}
