use crate::effect::{AuthorizedEffect, EffectContext, IntegrationOperation, ObservedState};
use crate::human::{render_request, HumanInteractionPort, InteractionRef};
use crate::jira::comment::{canonical_updated, document};
use crate::jira::work_item::failure_for;
use crate::jira::{JiraError, JiraHttp};
use fiddle_core::decision::{parse_marker, render_marker, DecisionRequestId};
use fiddle_core::{EffectName, HumanDecisionRequest, HumanDecisionRequirement, JIRA_COMMENT_ADDED};
use tokio_util::sync::CancellationToken;

const UNPROBED: &str = "this read asks the issue for its comments and never asks \
                        `/rest/api/3/myself` which of the two it is";

#[derive(Debug, thiserror::Error)]
pub enum ConversationError {
    #[error("{0}")]
    Site(#[from] JiraError),
    #[error(
        "{held} names the {channel} channel and a jira conversation reads a jira issue comment; \
         exactly one channel is authoritative for one request, so nothing was read"
    )]
    NotThisChannel { held: String, channel: String },
    #[error(
        "the comment `{marker}` was posted to `{issue}` and a read of the issue did not find it, \
         so no interaction can be named"
    )]
    Unlocatable { issue: String, marker: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JiraActor {
    pub account_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JiraReply {
    pub issue: String,
    pub comment: String,
    pub author: JiraActor,
    pub text: String,
    pub created: String,
    pub updated: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AskedOnIssue {
    pub issue: String,
    pub comment: String,
    pub observed_at: String,
}

impl AskedOnIssue {
    fn interaction(&self) -> InteractionRef {
        InteractionRef::JiraIssueComment {
            issue: self.issue.clone(),
            comment: self.comment.clone(),
        }
    }
}

impl ObservedState for AskedOnIssue {
    type Value = InteractionRef;

    fn describe(&self) -> String {
        format!(
            "the question is published as {}, on the issue this run observed at {}",
            self.interaction(),
            self.observed_at
        )
    }

    fn reference(&self) -> Option<String> {
        Some(self.comment.clone())
    }

    fn into_value(self) -> InteractionRef {
        self.interaction()
    }
}

#[derive(Debug)]
pub struct AskOnIssue {
    issue: String,
    observed_at: String,
    request: DecisionRequestId,
    marker: String,
    text: String,
}

impl AskOnIssue {
    pub fn new(
        issue: String,
        raw_updated: &str,
        request: &HumanDecisionRequest,
    ) -> Result<Self, JiraError> {
        Ok(Self {
            issue,
            observed_at: canonical_updated(raw_updated)?,
            request: request.binding.request.clone(),
            marker: render_marker(&request.binding),
            text: render_request(request),
        })
    }

    pub fn issue(&self) -> &str {
        &self.issue
    }

    pub fn observed_at(&self) -> &str {
        &self.observed_at
    }

    pub fn marker(&self) -> String {
        self.marker.clone()
    }

    fn asks_this_question(&self, reply: &JiraReply) -> bool {
        parse_marker(&reply.text).is_ok_and(|binding| binding.request == self.request)
    }

    fn asked_at(&self, comment: &str) -> AskedOnIssue {
        AskedOnIssue {
            issue: self.issue.clone(),
            comment: comment.to_string(),
            observed_at: self.observed_at.clone(),
        }
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for AskOnIssue {
    type State = AskedOnIssue;

    type Error = JiraError;

    fn kind(&self) -> EffectName {
        EffectName::shipped(JIRA_COMMENT_ADDED)
    }

    fn target(&self) -> String {
        format!("{}#{}", self.issue, self.request.0)
    }

    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    fn payload(&self) -> String {
        self.text.clone()
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<AskedOnIssue>, JiraError> {
        let read = read_comments(ctx.jira_client()?, &self.issue, &ctx.cancel).await?;
        let asking: Vec<String> = replies_in(&self.issue, &read)?
            .into_iter()
            .filter(|reply| self.asks_this_question(reply))
            .map(|reply| reply.comment)
            .collect();
        match asking.as_slice() {
            [] => Ok(None),
            [one] => Ok(Some(self.asked_at(one))),
            many => Err(JiraError::Ambiguous {
                marker: self.marker.clone(),
                count: many.len(),
            }),
        }
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), JiraError> {
        post_question(ctx.jira_client()?, &self.issue, &self.text, &ctx.cancel).await
    }
}

pub struct JiraConversation {
    issue: String,
}

impl JiraConversation {
    pub fn reading(issue: String) -> Self {
        Self { issue }
    }

    pub fn issue(&self) -> &str {
        &self.issue
    }

    pub fn asking(
        &self,
        raw_updated: &str,
        request: &HumanDecisionRequest,
    ) -> Result<AskOnIssue, JiraError> {
        AskOnIssue::new(self.issue.clone(), raw_updated, request)
    }
}

#[async_trait::async_trait]
impl HumanInteractionPort for JiraConversation {
    type Ask = AskOnIssue;

    type Reply = JiraReply;

    type Error = ConversationError;

    async fn request(
        &self,
        ctx: &EffectContext,
        request: &AskOnIssue,
        authorized: &AuthorizedEffect<AskOnIssue>,
    ) -> Result<InteractionRef, ConversationError> {
        IntegrationOperation::apply(request, ctx, authorized).await?;
        let posted = IntegrationOperation::inspect(request, ctx)
            .await?
            .ok_or_else(|| ConversationError::Unlocatable {
                issue: self.issue.clone(),
                marker: request.marker(),
            })?;
        Ok(posted.into_value())
    }

    async fn responses(
        &self,
        ctx: &EffectContext,
        interaction: &InteractionRef,
    ) -> Result<Vec<JiraReply>, ConversationError> {
        let issue = match interaction {
            InteractionRef::JiraIssueComment { issue, .. } => issue,
            InteractionRef::GitHubPullRequestComment { .. } => {
                return Err(ConversationError::NotThisChannel {
                    held: interaction.to_string(),
                    channel: interaction.channel().to_string(),
                })
            }
        };
        let read = read_comments(ctx.jira_client()?, issue, &ctx.cancel).await?;
        Ok(replies_in(issue, &read)?)
    }
}

pub async fn read_comments(
    http: &JiraHttp,
    issue: &str,
    cancel: &CancellationToken,
) -> Result<serde_json::Value, JiraError> {
    let path = format!("/rest/api/3/issue/{issue}?fields=comment");
    let answered = http.api("GET", &path, None, cancel).await?;
    match answered.status {
        status if (200..300).contains(&status) => Ok(answered.body),
        status => Err(told_apart(failure_for(
            status,
            issue,
            http.quoted(&answered.body).as_deref(),
        ))),
    }
}

pub async fn post_question(
    http: &JiraHttp,
    issue: &str,
    text: &str,
    cancel: &CancellationToken,
) -> Result<(), JiraError> {
    let path = format!("/rest/api/3/issue/{issue}/comment");
    let sent = document(text);
    let answered = http.api("POST", &path, Some(&sent), cancel).await?;
    match answered.status {
        status if (200..300).contains(&status) => Ok(()),
        status => Err(told_apart(failure_for(
            status,
            issue,
            http.quoted(&answered.body).as_deref(),
        ))),
    }
}

fn told_apart(failure: JiraError) -> JiraError {
    match failure {
        JiraError::Absent { key } => JiraError::AbsentOrRefused {
            key,
            why: UNPROBED.to_string(),
        },
        named => named,
    }
}

pub fn replies_in(issue: &str, read: &serde_json::Value) -> Result<Vec<JiraReply>, JiraError> {
    let held = &read["fields"]["comment"];
    let Some(comments) = held["comments"].as_array() else {
        return Err(JiraError::Malformed(format!(
            "the read of `{issue}` carried no `fields.comment.comments` array, so it says \
             nothing about who replied"
        )));
    };
    let Some(total) = held["total"].as_u64() else {
        return Err(JiraError::Malformed(format!(
            "the read of `{issue}` carried {} comments and no `fields.comment.total`, so an \
             absent reply would be a floor and not an answer",
            comments.len()
        )));
    };
    if total > comments.len() as u64 {
        return Err(JiraError::Malformed(format!(
            "the read of `{issue}` carried {} of {total} comments, so an absent reply would be \
             a floor and not an answer",
            comments.len()
        )));
    }
    comments
        .iter()
        .map(|comment| reply_from(issue, comment))
        .collect()
}

fn reply_from(issue: &str, comment: &serde_json::Value) -> Result<JiraReply, JiraError> {
    let named = |field: &str| {
        JiraError::Malformed(format!(
            "a comment on `{issue}` carried no `{field}`, so it names no reply this run can \
             weigh"
        ))
    };
    let account_id = comment["author"]["accountId"]
        .as_str()
        .ok_or_else(|| named("author.accountId"))?;
    Ok(JiraReply {
        issue: issue.to_string(),
        comment: comment["id"]
            .as_str()
            .ok_or_else(|| named("id"))?
            .to_string(),
        author: JiraActor {
            account_id: account_id.to_string(),
            display_name: comment["author"]["displayName"]
                .as_str()
                .unwrap_or(account_id)
                .to_string(),
        },
        text: written(&comment["body"]),
        created: comment["created"].as_str().unwrap_or_default().to_string(),
        updated: comment["updated"].as_str().unwrap_or_default().to_string(),
    })
}

pub fn written(body: &serde_json::Value) -> String {
    match body {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Object(fields) => {
            if let Some(serde_json::Value::String(text)) = fields.get("text") {
                return text.clone();
            }
            let between = match fields.get("type").and_then(serde_json::Value::as_str) {
                Some("paragraph") | Some("heading") => "",
                _ => "\n",
            };
            match fields.get("content") {
                Some(serde_json::Value::Array(held)) => joined(held, between),
                _ => String::new(),
            }
        }
        serde_json::Value::Array(held) => joined(held, "\n"),
        _ => String::new(),
    }
}

fn joined(held: &[serde_json::Value], between: &str) -> String {
    held.iter()
        .map(written)
        .filter(|read| !read.is_empty())
        .collect::<Vec<String>>()
        .join(between)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DECIDER: &str = "70121:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

    fn asked_about(head_sha: &str) -> HumanDecisionRequest {
        let effect = fiddle_core::effect_id(
            "acme/widget",
            "jira:IDENT-1",
            fiddle_core::ENSURE_PULL_REQUEST_READY,
            &format!("acme/widget#7@{head_sha}"),
        );
        HumanDecisionRequest {
            invocation_ref: "jira:IDENT-1".to_string(),
            work_ref: Some(fiddle_core::WorkRef("IDENT-1".to_string())),
            capability: fiddle_core::PROPOSE_CHANGE,
            binding: fiddle_core::DecisionBinding {
                request: fiddle_core::decision_request_id("acme/widget", "jira:IDENT-1", &effect),
                effect,
                payload: fiddle_core::payload_hash(r#"{"pr":7}"#),
                head_sha: head_sha.to_string(),
            },
            question: "May fiddle mark this ready for review?".to_string(),
            rationale: "The check passed at this revision.".to_string(),
            risks: vec!["review notifications reach the team".to_string()],
            alternatives: vec!["leave it a draft".to_string()],
            evidence: vec![fiddle_core::EvidenceRef("check=pass".to_string())],
        }
    }

    fn ask_at(raw_updated: &str, head_sha: &str) -> AskOnIssue {
        AskOnIssue::new("IDENT-1".to_string(), raw_updated, &asked_about(head_sha))
            .expect("the stamp reads")
    }

    const A_HEAD: &str = "1111111111111111111111111111111111111111";

    const ANOTHER_HEAD: &str = "2222222222222222222222222222222222222222";

    #[test]
    fn the_revision_is_canonicalised_once_and_carried_rather_than_read_again() {
        let colonless = ask_at("2026-08-26T07:00:00.000+0000", A_HEAD);
        let rfc_3339 = ask_at("2026-08-26T07:00:00Z", A_HEAD);

        assert_eq!(colonless.observed_at(), "2026-08-26T07:00:00Z");
        assert_eq!(
            colonless.observed_at(),
            rfc_3339.observed_at(),
            "one instant spelled two ways is one snapshot, so one record of it"
        );
    }

    #[test]
    fn a_revision_the_run_cannot_read_builds_no_question() {
        let refused = AskOnIssue::new("IDENT-1".to_string(), "yesterday", &asked_about(A_HEAD))
            .expect_err("a revision this run cannot read observed nothing it can speak about");

        assert!(
            format!("{refused}").contains("yesterday"),
            "the refusal quotes what it could not read: {refused}"
        );
    }

    #[test]
    fn the_question_the_issue_is_asked_is_identified_by_the_request_and_not_by_the_revision() {
        let held = ask_at("2026-08-26T07:00:00.000+0000", A_HEAD);
        let moved = ask_at("2026-08-26T09:30:00.000+0000", A_HEAD);
        let about_another_commit = ask_at("2026-08-26T07:00:00.000+0000", ANOTHER_HEAD);

        assert_eq!(
            held.target(),
            moved.target(),
            "a comment on the issue advances `fields.updated`, so an identity built from the \
             revision would make a fresh invocation ask the same question a second time"
        );
        assert_eq!(held.marker(), moved.marker());
        assert_ne!(
            held.target(),
            about_another_commit.target(),
            "and a question about another commit is another question, so the identity is not \
             merely constant"
        );
        assert!(
            held.target().starts_with("IDENT-1#"),
            "the target names the issue it is asked on: {}",
            held.target()
        );
    }

    #[test]
    fn the_posted_question_carries_the_marker_a_later_invocation_looks_for() {
        let held = ask_at("2026-08-26T07:00:00.000+0000", A_HEAD);
        let posted = document(&held.payload());
        let read_back = written(&posted["body"]);

        assert!(
            read_back.contains(&held.marker()),
            "the marker has to survive the round trip through a jira document, or a later \
             invocation reads its own question and does not recognise it: {read_back}"
        );
        assert_eq!(
            parse_marker(&read_back)
                .expect("one marker, and one only")
                .request,
            held.request,
            "and it parses back to the request this question is asked under"
        );
        assert!(
            read_back.contains("May fiddle mark this ready for review?"),
            "the words a person answers survive too: {read_back}"
        );
    }

    fn commented(id: &str, author: &str, text: &str) -> serde_json::Value {
        json!({
            "id": id,
            "author": {"accountId": author, "displayName": "a person"},
            "body": {"type": "doc", "version": 1, "content": [
                {"type": "paragraph", "content": [{"type": "text", "text": text}]}
            ]},
            "created": "2026-08-26T07:05:00.000+0000",
            "updated": "2026-08-26T07:05:00.000+0000",
        })
    }

    fn read(comments: serde_json::Value, total: u64) -> serde_json::Value {
        json!({"fields": {"comment": {
            "comments": comments,
            "maxResults": 50,
            "startAt": 0,
            "total": total,
        }}})
    }

    #[test]
    fn a_read_that_carries_fewer_comments_than_it_counts_refuses_rather_than_answering_none() {
        let refused = replies_in(
            "IDENT-1",
            &read(json!([commented("10001", DECIDER, "yes")]), 9),
        )
        .expect_err("a page is a floor and not a total");

        assert!(
            format!("{refused}").contains("1 of 9"),
            "the refusal prints the denominator, so a reply that fell off the end of the page \
             cannot read as an issue with no reply: {refused}"
        );
    }

    #[test]
    fn a_comment_with_no_account_id_refuses_rather_than_naming_an_actor_the_site_did_not() {
        let mut anonymous = commented("10001", DECIDER, "yes");
        anonymous["author"]
            .as_object_mut()
            .expect("an object")
            .remove("accountId");

        let refused = replies_in("IDENT-1", &read(json!([anonymous]), 1))
            .expect_err("an actor with no account id cannot be weighed against an allowlist");

        assert!(
            format!("{refused}").contains("author.accountId"),
            "the refusal names the field it did not get: {refused}"
        );
    }

    #[test]
    fn an_adf_body_reads_as_the_words_a_person_wrote() {
        assert_eq!(
            written(&json!({"type": "doc", "version": 1, "content": [
                {"type": "paragraph", "content": [{"type": "text", "text": "approve"}]},
                {"type": "paragraph", "content": [{"type": "text", "text": "it is fine"}]}
            ]})),
            "approve\nit is fine",
            "a reply reaches interpretation as the words it holds and never as its json"
        );
        assert_eq!(
            written(&json!({"type": "doc", "version": 1, "content": [
                {"type": "paragraph", "content": [
                    {"type": "text", "text": "approve "},
                    {"type": "text", "text": "E-17", "marks": [{"type": "strong"}]}
                ]}
            ]})),
            "approve E-17",
            "two inline runs of one sentence are one sentence, and a node name is not a word a \
             person wrote"
        );
    }
}
