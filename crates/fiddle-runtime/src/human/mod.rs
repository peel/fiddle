pub mod interpret;
pub mod validate;

use crate::effect::{AuthorizedEffect, EffectContext, IntegrationOperation, ObservedState};
use crate::github::{read_conversation, GhError};
use fiddle_core::{
    parse_marker, render_marker, HumanDecisionRequest, HumanDecisionRequirement, MarkerError,
};

pub use crate::github::HumanResponse;

pub(crate) const CONVERSATION_PAGES: u32 = 10;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum InteractionRef {
    GitHubPullRequestComment { repo: String, pr: u64, comment: u64 },
}

impl std::fmt::Display for InteractionRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InteractionRef::GitHubPullRequestComment { repo, pr, comment } => {
                write!(f, "{repo}#{pr} comment {comment}")
            }
        }
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

#[async_trait::async_trait]
impl IntegrationOperation for PublishDecisionRequest {
    type State = PublishedRequest;

    type Error = GhError;

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
    async fn request(
        &self,
        ctx: &EffectContext,
        request: &PublishDecisionRequest,
        authorized: &AuthorizedEffect<PublishDecisionRequest>,
    ) -> Result<InteractionRef, GhError>;

    async fn responses(
        &self,
        ctx: &EffectContext,
        interaction: &InteractionRef,
    ) -> Result<Vec<HumanResponse>, GhError>;
}

pub struct GitHubConversation;

#[async_trait::async_trait]
impl HumanInteractionPort for GitHubConversation {
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
