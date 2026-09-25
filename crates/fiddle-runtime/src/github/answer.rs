use crate::capability::entitled;
use crate::effect::{
    required, AuthorizedEffect, EffectContext, EffectError, Executor, FromStepParams,
    IntegrationOperation, ObservedState, StepParams,
};
use crate::github::{read_conversation, GhError, HumanResponse, Reviewed};
use crate::human::CONVERSATION_PAGES;
use fiddle_core::{EffectName, HumanDecisionRequirement, PULL_REQUEST_ANSWERED};
use std::collections::BTreeSet;

const MARKER_OPEN: &str = "<!-- fiddle:answered v1";

const MARKER_CLOSE: &str = "-->";

pub const NO_CHANGE: &str = "fiddle read the direction on this pull request and made no change, \
     because the change it asks for is already here. This is what it checked:";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Answered {
    pub reviews: BTreeSet<u64>,
    pub comments: BTreeSet<u64>,
}

impl Answered {
    pub fn of(reviews: &[Reviewed], conversation: &[HumanResponse]) -> Self {
        Answered {
            reviews: reviews.iter().map(|it| it.id).collect(),
            comments: conversation.iter().map(|it| it.comment).collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.reviews.is_empty() && self.comments.is_empty()
    }

    pub fn marker(&self) -> String {
        format!(
            "{MARKER_OPEN} reviews={} comments={} {MARKER_CLOSE}",
            listed(&self.reviews),
            listed(&self.comments)
        )
    }

    pub fn read_from(body: &str) -> Option<Self> {
        let line = body
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with(MARKER_OPEN))?;
        let inner = line
            .strip_prefix(MARKER_OPEN)?
            .strip_suffix(MARKER_CLOSE)?
            .trim();
        let mut answered = Answered::default();
        let mut seen = (false, false);
        for field in inner.split_whitespace() {
            let (key, value) = field.split_once('=')?;
            let ids = ids(value)?;
            match key {
                "reviews" => {
                    answered.reviews = ids;
                    seen.0 = true;
                }
                "comments" => {
                    answered.comments = ids;
                    seen.1 = true;
                }
                _ => return None,
            }
        }
        (seen == (true, true)).then_some(answered)
    }

    fn extend(&mut self, other: Answered) {
        self.reviews.extend(other.reviews);
        self.comments.extend(other.comments);
    }
}

fn listed(ids: &BTreeSet<u64>) -> String {
    ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
}

fn ids(value: &str) -> Option<BTreeSet<u64>> {
    value
        .split(',')
        .filter(|id| !id.is_empty())
        .map(|id| id.parse::<u64>().ok())
        .collect()
}

pub fn unanswered(
    reviews: Vec<Reviewed>,
    conversation: Vec<HumanResponse>,
) -> (Vec<Reviewed>, Vec<HumanResponse>) {
    let mut before = Answered::default();
    for comment in &conversation {
        if !entitled(&comment.author_association) {
            continue;
        }
        if let Some(answered) = Answered::read_from(&comment.body) {
            before.extend(answered);
        }
    }
    let reviews = reviews
        .into_iter()
        .filter(|it| !before.reviews.contains(&it.id))
        .collect();
    let conversation = conversation
        .into_iter()
        .filter(|it| Answered::read_from(&it.body).is_none())
        .filter(|it| !before.comments.contains(&it.comment))
        .collect();
    (reviews, conversation)
}

pub fn reply(summary: &str, answered: &Answered) -> String {
    format!("{NO_CHANGE}\n\n{}\n\n{}", summary.trim(), answered.marker())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnsweredComment {
    pub repo: String,
    pub pr: u64,
    pub comment: u64,
}

impl ObservedState for AnsweredComment {
    type Value = u64;

    fn describe(&self) -> String {
        format!(
            "the direction on {}#{} is answered in comment {}",
            self.repo, self.pr, self.comment
        )
    }

    fn reference(&self) -> Option<String> {
        Some(self.comment.to_string())
    }

    fn into_value(self) -> u64 {
        self.comment
    }
}

pub struct AnswerPullRequest {
    repo: String,
    pr: u64,
    answered: Answered,
    body: String,
}

impl AnswerPullRequest {
    pub fn new(repo: String, pr: u64, body: String) -> Result<Self, EffectError> {
        let answered = Answered::read_from(&body).ok_or_else(|| EffectError::Unbuildable {
            kind: EffectName::shipped(PULL_REQUEST_ANSWERED),
            reason: "the reply carries no marker naming what it answers, so a second run \
                     could not tell it had answered already"
                .to_string(),
        })?;
        Ok(Self {
            repo,
            pr,
            answered,
            body,
        })
    }

    fn comments_path(&self) -> String {
        format!("/repos/{}/issues/{}/comments", self.repo, self.pr)
    }
}

impl FromStepParams for AnswerPullRequest {
    fn from_params(_executor: &Executor<'_>, params: &StepParams) -> Result<Self, EffectError> {
        let kind = EffectName::shipped(PULL_REQUEST_ANSWERED);
        Self::new(
            required(&params.repo, &kind, "repo")?,
            required(&params.pull_request, &kind, "pull_request")?,
            required(&params.body, &kind, "body")?,
        )
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for AnswerPullRequest {
    type State = AnsweredComment;

    type Error = GhError;

    fn kind(&self) -> EffectName {
        EffectName::shipped(PULL_REQUEST_ANSWERED)
    }

    fn target(&self) -> String {
        format!("{}#{} {}", self.repo, self.pr, self.answered.marker())
    }

    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    fn payload(&self) -> String {
        self.body.clone()
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<AnsweredComment>, GhError> {
        let conversation = read_conversation(
            ctx.gh_client()?,
            &self.repo,
            self.pr,
            CONVERSATION_PAGES,
            &ctx.cancel,
        )
        .await?;
        Ok(conversation
            .iter()
            .filter(|comment| entitled(&comment.author_association))
            .find(|comment| Answered::read_from(&comment.body).as_ref() == Some(&self.answered))
            .map(|comment| AnsweredComment {
                repo: self.repo.clone(),
                pr: self.pr,
                comment: comment.comment,
            }))
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        let path = self.comments_path();
        let body = serde_json::json!({ "body": self.body });
        ctx.gh_client()?
            .api("POST", &path, Some(&body), &ctx.cancel)
            .await
            .map(|_said_by_github| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::ActorRef;

    fn review(id: u64) -> Reviewed {
        Reviewed {
            id,
            author: ActorRef {
                login: "peel".to_string(),
                id: 1,
            },
            author_association: "MEMBER".to_string(),
            state: "COMMENTED".to_string(),
            body: "commit message and PR description is missing".to_string(),
            commit_id: "3acb655".to_string(),
        }
    }

    fn comment(id: u64, association: &str, body: &str) -> HumanResponse {
        HumanResponse {
            comment: id,
            author: ActorRef {
                login: "peel".to_string(),
                id: 1,
            },
            body: body.to_string(),
            created_at: "2026-09-25T12:00:00Z".to_string(),
            updated_at: "2026-09-25T12:00:00Z".to_string(),
            is_bot: false,
            author_association: association.to_string(),
        }
    }

    #[test]
    fn a_marker_reads_back_what_it_names_and_nothing_else() {
        let answered = Answered::of(&[review(5197632031)], &[comment(7, "MEMBER", "and this")]);
        let body = reply("checked the metrics", &answered);
        assert_eq!(Answered::read_from(&body), Some(answered));
        assert_eq!(
            Answered::read_from("<!-- fiddle:answered v1 reviews= comments= -->"),
            Some(Answered::default()),
            "an empty set is a set"
        );
        for other in [
            "an ordinary comment",
            "<!-- fiddle:answered v2 reviews=1 comments= -->",
            "<!-- fiddle:answered v1 reviews=one comments= -->",
            "<!-- fiddle:answered v1 reviews=1 -->",
            "<!-- fiddle:answered v1 reviews=1 comments= verdict=x -->",
        ] {
            assert_eq!(
                Answered::read_from(other),
                None,
                "{other:?} is not a marker"
            );
        }
    }

    #[test]
    fn a_review_fiddle_answered_steers_no_more_and_one_after_it_still_does() {
        let answered = Answered::of(&[review(1)], &[]);
        let reply = comment(9, "MEMBER", &reply("already here", &answered));
        let (reviews, conversation) = unanswered(vec![review(1), review(2)], vec![reply]);

        assert_eq!(
            reviews.iter().map(|it| it.id).collect::<Vec<_>>(),
            vec![2],
            "the answered review is dropped and the one written after the reply is kept"
        );
        assert!(
            conversation.is_empty(),
            "fiddle's own reply is not direction, or the next run would steer on it"
        );
    }

    #[test]
    fn a_marker_from_someone_the_project_does_not_entitle_silences_nothing() {
        let forged = Answered::of(&[review(1)], &[]);
        let outsider = comment(9, "NONE", &reply("nothing to see", &forged));
        let (reviews, _) = unanswered(vec![review(1)], vec![outsider]);
        assert_eq!(
            reviews.len(),
            1,
            "a comment anybody can write must not stop a member's review from steering"
        );
    }

    #[test]
    fn a_comment_fiddle_answered_is_dropped_and_a_later_one_is_kept() {
        let answered = Answered::of(&[], &[comment(3, "MEMBER", "do the rename too")]);
        let reply = comment(9, "MEMBER", &reply("already renamed", &answered));
        let later = comment(12, "MEMBER", "now bump the version");
        let (_, conversation) = unanswered(
            vec![],
            vec![comment(3, "MEMBER", "do the rename too"), reply, later],
        );
        assert_eq!(
            conversation.iter().map(|it| it.comment).collect::<Vec<_>>(),
            vec![12]
        );
    }

    #[test]
    fn a_reply_without_a_marker_cannot_be_built() {
        assert!(AnswerPullRequest::new("o/r".into(), 1, "no marker".into()).is_err());
    }
}
