use crate::effect::{
    AuthorizedEffect, Effect, EffectContext, EffectError, Executor, FromStepParams, StepParams,
};
use crate::jira::comment::{
    agreed, canonical_updated, marker_for, post_marked_comment, read_marked_comment, MarkedComment,
    UNBUILT,
};
use crate::jira::JiraError;
use fiddle_core::{effect_id, EffectId, EffectName, JIRA_PULL_REQUEST_LINKED};

const FORGE: &str = "https://github.com";

pub fn pull_request_url(repo: &str, number: u64) -> String {
    format!("{FORGE}/{repo}/pull/{number}")
}

pub fn link_text(repo: &str, number: u64) -> String {
    format!(
        "pull request {repo}#{number}: {}",
        pull_request_url(repo, number)
    )
}

#[derive(Effect)]
#[effect(
    name = JIRA_PULL_REQUEST_LINKED,
    minimum = "automatic",
    target = "{issue_key}@{issue_updated}",
    state = MarkedComment,
    error = JiraError
)]
pub struct LinkPullRequest {
    issue_key: String,
    issue_updated: String,
    #[payload]
    repo: String,
    #[payload(rename = "pull_request")]
    number: u64,
    effect_id: EffectId,
}

impl FromStepParams for LinkPullRequest {
    fn from_params(_executor: &Executor<'_>, _params: &StepParams) -> Result<Self, EffectError> {
        Err(EffectError::Unbuildable {
            kind: EffectName::shipped(JIRA_PULL_REQUEST_LINKED),
            reason: "a step names no issue key and no `fields.updated`, and this operation's \
                     identity is built from both, so it is constructed from a read of the issue \
                     and never from a step alone"
                .to_string(),
        })
    }
}

impl LinkPullRequest {
    pub fn new(
        issue_key: String,
        raw_updated: &str,
        repo: String,
        number: u64,
        project: &str,
        invocation_ref: &str,
    ) -> Result<Self, JiraError> {
        let unnamed = Self {
            issue_key,
            issue_updated: canonical_updated(raw_updated)?,
            repo,
            number,
            effect_id: EffectId(UNBUILT.to_string()),
        };
        let effect_id = effect_id(
            project,
            invocation_ref,
            JIRA_PULL_REQUEST_LINKED,
            &unnamed.target(),
        );
        Ok(Self {
            effect_id,
            ..unnamed
        })
    }

    pub fn marker(&self) -> String {
        marker_for(&self.effect_id)
    }

    pub fn text(&self) -> String {
        link_text(&self.repo, self.number)
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<MarkedComment>, JiraError> {
        read_marked_comment(
            ctx.jira_client()?,
            &self.issue_key,
            &self.marker(),
            &ctx.cancel,
        )
        .await
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), JiraError> {
        let marker = agreed(authorized.effect_id(), &self.effect_id)?;
        post_marked_comment(
            ctx.jira_client()?,
            &self.issue_key,
            &self.text(),
            &marker,
            &ctx.cancel,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::IntegrationOperation;

    fn operation() -> LinkPullRequest {
        LinkPullRequest::new(
            "IDENT-1".to_string(),
            "2026-08-26T07:00:00.000+0000",
            "peel/fiddle-test".to_string(),
            42,
            "acme/widget",
            "beans:w-1",
        )
        .expect("the stamp reads")
    }

    #[test]
    fn the_text_names_the_repository_the_number_and_the_url_a_reader_can_follow() {
        assert_eq!(
            link_text("peel/fiddle-test", 42),
            "pull request peel/fiddle-test#42: https://github.com/peel/fiddle-test/pull/42"
        );
    }

    #[test]
    fn the_payload_and_the_target_are_what_this_build_writes() {
        let operation = operation();

        assert_eq!(operation.target(), "IDENT-1@2026-08-26T07:00:00Z");
        assert_eq!(
            operation.payload(),
            r#"{"pull_request":42,"repo":"peel/fiddle-test"}"#
        );
        assert_eq!(operation.marker(), "fiddle-effect:6f814e2900364681");
    }

    #[test]
    fn a_link_and_a_comment_on_one_issue_carry_two_markers() {
        let comment = crate::jira::comment::AddComment::new(
            "IDENT-1".to_string(),
            "2026-08-26T07:00:00.000+0000",
            "the fixture is repaired".to_string(),
            "acme/widget",
            "beans:w-1",
        )
        .expect("the stamp reads");

        assert_eq!(
            comment.target(),
            operation().target(),
            "the two effects name one issue at one revision"
        );
        assert_ne!(
            comment.marker(),
            operation().marker(),
            "and the kind is an input to the identity, so one marker cannot answer for both"
        );
    }
}
