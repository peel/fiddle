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
    InvocationRef, InvocationScheme, MarkerError, ProposedEffect, WorkItemState,
    JIRA_COMMENT_ADDED, PUBLISH_DECISION_REQUEST,
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

    pub fn asked_by(&self) -> EffectName {
        match self {
            DecisionChannel::GitHubPullRequest { .. } => {
                EffectName::shipped(PUBLISH_DECISION_REQUEST)
            }
            DecisionChannel::JiraIssue { .. } => EffectName::shipped(JIRA_COMMENT_ADDED),
        }
    }

    pub fn named_by(
        invocation_ref: &str,
        work_item: Option<&WorkItemState>,
        pull_request: Option<(&str, u64)>,
    ) -> Vec<DecisionChannel> {
        let scheme = invocation_ref
            .parse::<InvocationRef>()
            .ok()
            .map(|reference| reference.scheme());
        let named = match scheme {
            Some(InvocationScheme::Jira) => work_item.and_then(DecisionChannel::for_issue),
            Some(
                InvocationScheme::Beans
                | InvocationScheme::Scheduled
                | InvocationScheme::Scanner
                | InvocationScheme::Cve,
            )
            | None => pull_request.map(|(repo, pr)| DecisionChannel::GitHubPullRequest {
                repo: repo.to_string(),
                pr,
            }),
        };
        named.into_iter().collect()
    }

    fn for_issue(work_item: &WorkItemState) -> Option<DecisionChannel> {
        work_item
            .revision
            .as_ref()
            .map(|updated| DecisionChannel::JiraIssue {
                issue: work_item.id.clone(),
                updated: updated.clone(),
            })
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
    #[error(
        "{0} (a run that names its channel from a github pull request alone never reaches this \
         arm, and a run steered by a jira reference reaches it whenever nothing observed the \
         issue; ADR 081 records the trade)"
    )]
    Channel(#[from] ChannelError),
    #[error("{0}")]
    Unpublished(#[from] EffectError),
    #[error(
        "the named jira issue carries no revision this run can build an identity from: {0} (a \
         github channel never reaches this arm; ADR 081 records the trade)"
    )]
    Unaddressable(#[from] JiraError),
}

#[derive(Debug)]
pub struct PublishedAsk {
    pub asked_by: EffectName,
    pub receipt: EffectReceipt<InteractionRef>,
}

pub async fn publish(
    executor: &Executor<'_>,
    named: &[DecisionChannel],
    request: &HumanDecisionRequest,
) -> Result<PublishedAsk, PublishError> {
    let channel = authoritative(named)?;
    let asked_by = channel.asked_by();
    match channel {
        DecisionChannel::GitHubPullRequest { repo, pr } => {
            let ask = PublishDecisionRequest::new(repo.clone(), *pr, request.clone());
            Ok(PublishedAsk {
                asked_by,
                receipt: executor.execute(proposing(executor, &ask), ask).await?,
            })
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
            Ok(PublishedAsk {
                asked_by,
                receipt: EffectReceipt {
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
    use fiddle_core::JIRA_COMMENT_ADDED;

    const HELD: &str = "2026-08-26T10:00:00.000+0000";

    fn observed(id: &str, revision: Option<&str>) -> WorkItemState {
        WorkItemState {
            id: id.to_string(),
            status: "In Progress".to_string(),
            projected_status: None,
            revision: revision.map(str::to_string),
            labels: None,
            description: None,
            comments: None,
        }
    }

    fn on_jira() -> DecisionChannel {
        DecisionChannel::JiraIssue {
            issue: "IDENT-1".to_string(),
            updated: HELD.to_string(),
        }
    }

    fn on_github() -> DecisionChannel {
        DecisionChannel::GitHubPullRequest {
            repo: "acme/widget".to_string(),
            pr: 7,
        }
    }

    #[test]
    fn a_jira_invocation_asks_on_the_issue_the_run_observed() {
        let item = observed("IDENT-1", Some(HELD));

        assert_eq!(
            DecisionChannel::named_by("jira:IDENT-1", Some(&item), Some(("acme/widget", 7))),
            vec![on_jira()]
        );
    }

    #[test]
    fn a_pull_request_run_asks_on_the_pull_request_although_it_observed_an_issue() {
        let item = observed("IDENT-1", Some(HELD));

        assert_eq!(
            DecisionChannel::named_by("beans:w-1", Some(&item), Some(("acme/widget", 7))),
            vec![on_github()],
            "the channel follows the invocation; an observation the run holds for another \
             reason does not redirect the question"
        );
    }

    #[test]
    fn a_jira_invocation_that_observed_no_revision_names_no_channel() {
        let unrevised = observed("IDENT-1", None);

        assert!(
            DecisionChannel::named_by("jira:IDENT-1", Some(&unrevised), Some(("acme/widget", 7)))
                .is_empty(),
            "a comment on an issue builds its identity from the revision the issue was read \
             at, so an unrevised observation addresses nothing"
        );
        assert!(
            DecisionChannel::named_by("jira:IDENT-1", None, Some(("acme/widget", 7))).is_empty(),
            "and a run that observed nothing addresses nothing either"
        );
    }

    #[test]
    fn an_unreadable_invocation_asks_on_the_pull_request_the_run_holds() {
        assert_eq!(
            DecisionChannel::named_by("this is not a reference", None, Some(("acme/widget", 7))),
            vec![on_github()]
        );
    }

    #[test]
    fn a_run_that_opened_no_pull_request_and_names_no_issue_names_no_channel() {
        assert!(DecisionChannel::named_by("beans:w-1", None, None).is_empty());
    }

    #[test]
    fn no_invocation_names_two_channels() {
        let items = [
            Some(observed("IDENT-1", Some(HELD))),
            Some(observed("IDENT-1", None)),
            None,
        ];
        let (mut jira, mut github, mut none) = (0, 0, 0);
        for invocation in [
            "jira:IDENT-1",
            "jira:IDENT-1:sub",
            "beans:w-1",
            "scanner:s-1",
            "scheduled:nightly",
            "cve",
            "this is not a reference",
        ] {
            for item in &items {
                for pull_request in [Some(("acme/widget", 7)), None] {
                    let named = DecisionChannel::named_by(invocation, item.as_ref(), pull_request);
                    match named.as_slice() {
                        [] => none += 1,
                        [DecisionChannel::JiraIssue { .. }] => jira += 1,
                        [DecisionChannel::GitHubPullRequest { .. }] => github += 1,
                        two_or_more => panic!(
                            "`{invocation}` named {two_or_more:?}, and `authoritative` refuses two"
                        ),
                    }
                }
            }
        }

        assert_eq!(
            jira + github + none,
            42,
            "seven invocations against three observations against two pull-request states is \
             the denominator this sweep reports against"
        );
        assert_eq!(
            (jira, github, none),
            (4, 15, 23),
            "the bound alone passes on a `named_by` that answers nothing, so the sweep counts \
             what it names: the two jira references name the issue only when the observation \
             carries a revision, the five other references name the pull request only when the \
             run holds one, and the remaining cases name nobody"
        );
    }

    #[test]
    fn the_effect_name_the_evidence_line_spells_follows_the_channel() {
        assert_eq!(on_github().asked_by().as_str(), PUBLISH_DECISION_REQUEST);
        assert_eq!(on_jira().asked_by().as_str(), JIRA_COMMENT_ADDED);
    }

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
