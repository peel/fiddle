use crate::effect::{
    AuthorizedEffect, Effect, EffectContext, EffectError, Executor, FromStepParams, ObservedState,
    StepParams,
};
use crate::jira::work_item::{canonical_revision, failure_for};
use crate::jira::{JiraError, JiraHttp};
use fiddle_core::{effect_id, EffectId, EffectName};
use tokio_util::sync::CancellationToken;

pub const JIRA_COMMENT_ADDED: &str = "jira.comment_added";

const MARKER: &str = "fiddle-effect:";

pub(crate) const UNBUILT: &str = "unbuilt";

const UNPROBED: &str = "this effect reads the issue and never asks `/rest/api/3/myself` which of \
                        the two it is";

pub fn marker_for(effect: &EffectId) -> String {
    format!("{MARKER}{}", effect.0)
}

pub fn canonical_updated(raw: &str) -> Result<String, JiraError> {
    canonical_revision(raw).ok_or_else(|| {
        JiraError::Malformed(format!(
            "`{raw}` is not a `fields.updated` this effect can read, so no identity was built"
        ))
    })
}

pub fn agreed(authorized: &EffectId, held: &EffectId) -> Result<String, JiraError> {
    match authorized == held {
        true => Ok(marker_for(held)),
        false => Err(JiraError::Malformed(format!(
            "the comment would carry `{}` and be looked up as `{}`, so nothing was posted",
            marker_for(authorized),
            marker_for(held)
        ))),
    }
}

pub fn marked_body(text: &str, marker: &str) -> serde_json::Value {
    serde_json::json!({
        "body": {
            "type": "doc",
            "version": 1,
            "content": [paragraph(text), paragraph(marker)],
        }
    })
}

fn paragraph(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "paragraph",
        "content": [{"type": "text", "text": text}],
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkedComment {
    pub issue: String,
    pub comment_id: String,
}

impl ObservedState for MarkedComment {
    type Value = MarkedComment;

    fn describe(&self) -> String {
        format!(
            "comment {} on {} carries this effect's marker",
            self.comment_id, self.issue
        )
    }

    fn reference(&self) -> Option<String> {
        Some(self.comment_id.clone())
    }

    fn into_value(self) -> MarkedComment {
        self
    }
}

pub fn marked_comment(
    issue: &str,
    read: &serde_json::Value,
    marker: &str,
) -> Result<Option<MarkedComment>, JiraError> {
    let held = &read["fields"]["comment"];
    let Some(comments) = held["comments"].as_array() else {
        return Err(JiraError::Malformed(format!(
            "the read of `{issue}` carried no `fields.comment.comments` array, so it says \
             nothing about whether this effect already commented"
        )));
    };
    let Some(total) = held["total"].as_u64() else {
        return Err(JiraError::Malformed(format!(
            "the read of `{issue}` carried {} comments and no `fields.comment.total`, so an \
             absent marker would be a floor and not an answer",
            comments.len()
        )));
    };
    if total > comments.len() as u64 {
        return Err(JiraError::Malformed(format!(
            "the read of `{issue}` carried {} of {total} comments, so an absent marker would \
             be a floor and not an answer",
            comments.len()
        )));
    }
    let carried: Vec<&serde_json::Value> = comments
        .iter()
        .filter(|comment| comment["body"].to_string().contains(marker))
        .collect();
    match carried.as_slice() {
        [] => Ok(None),
        [one] => Ok(Some(MarkedComment {
            issue: issue.to_string(),
            comment_id: named_id(issue, one)?,
        })),
        many => Err(JiraError::Malformed(format!(
            "{} comments on `{issue}` carry `{marker}` and this effect writes one, so the read \
             cannot say which one it wrote",
            many.len()
        ))),
    }
}

fn named_id(issue: &str, comment: &serde_json::Value) -> Result<String, JiraError> {
    comment["id"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| JiraError::Malformed(format!("a comment on `{issue}` carried no `id`")))
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

pub async fn read_marked_comment(
    http: &JiraHttp,
    issue: &str,
    marker: &str,
    cancel: &CancellationToken,
) -> Result<Option<MarkedComment>, JiraError> {
    let path = format!("/rest/api/3/issue/{issue}?fields=comment");
    let answered = http.api("GET", &path, None, cancel).await?;
    match answered.status {
        status if (200..300).contains(&status) => marked_comment(issue, &answered.body, marker),
        status => Err(told_apart(failure_for(
            status,
            issue,
            http.quoted(&answered.body).as_deref(),
        ))),
    }
}

pub async fn post_marked_comment(
    http: &JiraHttp,
    issue: &str,
    text: &str,
    marker: &str,
    cancel: &CancellationToken,
) -> Result<(), JiraError> {
    let path = format!("/rest/api/3/issue/{issue}/comment");
    let sent = marked_body(text, marker);
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

#[derive(Effect)]
#[effect(
    name = JIRA_COMMENT_ADDED,
    minimum = "automatic",
    target = "{issue_key}@{issue_updated}",
    state = MarkedComment,
    error = JiraError
)]
pub struct AddComment {
    issue_key: String,
    issue_updated: String,
    #[payload]
    text: String,
    effect_id: EffectId,
}

impl FromStepParams for AddComment {
    fn from_params(_executor: &Executor<'_>, _params: &StepParams) -> Result<Self, EffectError> {
        Err(EffectError::Unbuildable {
            kind: EffectName::shipped(JIRA_COMMENT_ADDED),
            reason: "a step names no issue key and no `fields.updated`, and this operation's \
                     identity is built from both, so it is constructed from a read of the issue \
                     and never from a step alone"
                .to_string(),
        })
    }
}

impl AddComment {
    pub fn new(
        issue_key: String,
        raw_updated: &str,
        text: String,
        project: &str,
        invocation_ref: &str,
    ) -> Result<Self, JiraError> {
        let unnamed = Self {
            issue_key,
            issue_updated: canonical_updated(raw_updated)?,
            text,
            effect_id: EffectId(UNBUILT.to_string()),
        };
        let effect_id = effect_id(
            project,
            invocation_ref,
            JIRA_COMMENT_ADDED,
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
            &self.text,
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
    use serde_json::json;

    const MARKER_ONE: &str = "fiddle-effect:1111111111111111";

    const MARKER_TWO: &str = "fiddle-effect:2222222222222222";

    fn read(comments: serde_json::Value, total: u64) -> serde_json::Value {
        json!({"fields": {"comment": {
            "comments": comments,
            "maxResults": 50,
            "startAt": 0,
            "total": total,
        }}})
    }

    fn carrying(id: &str, marker: &str) -> serde_json::Value {
        let mut comment = marked_body("a person reads this", marker);
        comment["id"] = json!(id);
        comment
    }

    #[test]
    fn one_read_answers_none_for_a_marker_no_comment_carries_and_some_for_one_a_comment_does() {
        let held = read(json!([carrying("20001", MARKER_ONE)]), 1);

        assert_eq!(
            marked_comment("IDENT-1", &held, MARKER_TWO).expect("the read is complete"),
            None,
            "a marker no comment carries is absent, or the search matches on there being any \
             comment at all rather than on the marker"
        );
        assert_eq!(
            marked_comment("IDENT-1", &held, MARKER_ONE).expect("the read is complete"),
            Some(MarkedComment {
                issue: "IDENT-1".to_string(),
                comment_id: "20001".to_string(),
            }),
            "and the same read finds the marker a comment does carry, so the line above cannot \
             pass by finding nothing anywhere"
        );
    }

    #[test]
    fn a_marker_spelled_in_a_comments_id_and_in_no_comment_body_is_not_a_comment_this_effect_wrote()
    {
        let mut comment = marked_body("a person reads this", MARKER_TWO);
        comment["id"] = json!(MARKER_ONE);

        assert_eq!(
            marked_comment("IDENT-1", &read(json!([comment]), 1), MARKER_ONE)
                .expect("the read is complete"),
            None,
            "the marker is written into the comment body, so a search that read the whole \
             comment object would answer a comment this effect never wrote"
        );
    }

    #[test]
    fn a_read_that_carries_fewer_comments_than_it_counts_refuses_rather_than_answering_none() {
        let short = read(json!([carrying("20001", MARKER_TWO)]), 9);

        let refused = marked_comment("IDENT-1", &short, MARKER_ONE)
            .expect_err("a page is a floor and not a total");

        assert!(
            format!("{refused}").contains("1 of 9"),
            "the refusal prints the denominator, so a reader can see the read was partial: \
             {refused}"
        );
    }

    #[test]
    fn a_read_that_counts_nothing_refuses_because_an_absent_marker_would_be_unbounded() {
        let uncounted = json!({"fields": {"comment": {"comments": []}}});

        let refused = marked_comment("IDENT-1", &uncounted, MARKER_ONE)
            .expect_err("a read with no total bounds nothing");

        assert!(
            format!("{refused}").contains("fields.comment.total"),
            "the refusal names the count it did not get: {refused}"
        );
    }

    #[test]
    fn a_read_that_carries_no_comments_array_refuses_rather_than_reading_as_an_issue_with_none() {
        let absent = json!({"fields": {"updated": "2026-08-26T07:00:00.000+0000"}});

        let refused = marked_comment("IDENT-1", &absent, MARKER_ONE)
            .expect_err("a read that answered no comment field says nothing about comments");

        assert!(
            format!("{refused}").contains("fields.comment.comments"),
            "the refusal names the field it did not get: {refused}"
        );
    }

    #[test]
    fn two_comments_carrying_one_marker_refuse_rather_than_answering_the_first() {
        let twice = read(
            json!([carrying("20001", MARKER_ONE), carrying("20002", MARKER_ONE)]),
            2,
        );

        let refused = marked_comment("IDENT-1", &twice, MARKER_ONE)
            .expect_err("this effect writes one comment, so two is not an answer");

        assert!(
            format!("{refused}").contains('2'),
            "the refusal counts what it found: {refused}"
        );
    }

    #[test]
    fn a_comment_that_carries_the_marker_and_no_id_refuses_rather_than_naming_an_empty_reference() {
        let mut comment = marked_body("a person reads this", MARKER_ONE);
        comment.as_object_mut().expect("an object").remove("id");

        let refused = marked_comment("IDENT-1", &read(json!([comment]), 1), MARKER_ONE)
            .expect_err("a receipt cannot point at a comment with no id");

        assert!(format!("{refused}").contains("no `id`"), "{refused}");
    }

    #[test]
    fn an_identity_the_executor_does_not_share_refuses_and_names_both_markers() {
        let held = EffectId("1111111111111111".to_string());
        let authorized = EffectId("2222222222222222".to_string());

        assert_eq!(
            agreed(&held, &held).expect("one identity agrees with itself"),
            MARKER_ONE
        );
        let refused = agreed(&authorized, &held).expect_err("two identities do not agree");
        assert!(
            format!("{refused}").contains(MARKER_ONE) && format!("{refused}").contains(MARKER_TWO),
            "a reader has to see both markers to know which one the world would carry: {refused}"
        );
    }

    #[test]
    fn the_posted_body_carries_the_text_and_the_marker_in_the_document_jira_v3_takes() {
        let sent = marked_body("the fixture is repaired", MARKER_ONE);

        assert_eq!(sent["body"]["type"], "doc");
        assert_eq!(sent["body"]["version"], 1);
        assert_eq!(
            sent["body"]["content"][0]["content"][0]["text"],
            "the fixture is repaired"
        );
        assert_eq!(sent["body"]["content"][1]["content"][0]["text"], MARKER_ONE);
    }

    #[test]
    fn the_payload_and_the_target_are_what_this_build_writes() {
        let operation = AddComment::new(
            "IDENT-1".to_string(),
            "2026-08-26T07:00:00.000+0000",
            "the fixture is repaired".to_string(),
            "acme/widget",
            "beans:w-1",
        )
        .expect("the stamp reads");

        assert_eq!(operation.target(), "IDENT-1@2026-08-26T07:00:00Z");
        assert_eq!(operation.payload(), r#"{"text":"the fixture is repaired"}"#);
        assert_eq!(operation.marker(), "fiddle-effect:43feabce0ad25e35");
    }
}
