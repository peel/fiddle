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

pub const NO_CHANGE: &str = "**fiddle made no change for the direction above.**";

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
    account: Option<&str>,
) -> (Vec<Reviewed>, Vec<HumanResponse>) {
    let mut before = Answered::default();
    for comment in &conversation {
        if !trusted(comment, account) {
            continue;
        }
        if let Some(answered) = Answered::read_from(&comment.body) {
            before.extend(answered);
        }
    }
    let reviews = reviews
        .into_iter()
        .filter(|it| !is_account(&it.author.login, account))
        .filter(|it| !before.reviews.contains(&it.id))
        .collect();
    let conversation = conversation
        .into_iter()
        .filter(|it| !written_by(it, account))
        .filter(|it| Answered::read_from(&it.body).is_none())
        .filter(|it| !before.comments.contains(&it.comment))
        .collect();
    (reviews, conversation)
}

fn is_account(login: &str, account: Option<&str>) -> bool {
    account.is_some_and(|it| login.eq_ignore_ascii_case(it))
}

fn written_by(comment: &HumanResponse, account: Option<&str>) -> bool {
    is_account(&comment.author.login, account)
}

fn trusted(comment: &HumanResponse, account: Option<&str>) -> bool {
    entitled(&comment.author_association) || written_by(comment, account)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Earlier {
    pub author: String,
    pub body: String,
    pub by_fiddle: bool,
}

fn without_marker(body: &str) -> String {
    body.lines()
        .filter(|line| !line.trim().starts_with(MARKER_OPEN))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn already_answered(
    reviews: &[Reviewed],
    conversation: &[HumanResponse],
    account: Option<&str>,
) -> Vec<Earlier> {
    let mut before = Answered::default();
    for comment in conversation {
        if trusted(comment, account) {
            if let Some(answered) = Answered::read_from(&comment.body) {
                before.extend(answered);
            }
        }
    }
    let reviewed = reviews
        .iter()
        .filter(|it| before.reviews.contains(&it.id))
        .filter(|it| entitled(&it.author_association) && !it.body.trim().is_empty())
        .map(|it| Earlier {
            author: it.author.login.clone(),
            body: it.body.trim().to_string(),
            by_fiddle: false,
        });
    let said = conversation
        .iter()
        .filter(|it| trusted(it, account))
        .filter_map(|it| match Answered::read_from(&it.body) {
            Some(_) => Some(Earlier {
                author: it.author.login.clone(),
                body: without_marker(&it.body),
                by_fiddle: true,
            }),
            None if before.comments.contains(&it.comment) => Some(Earlier {
                author: it.author.login.clone(),
                body: it.body.trim().to_string(),
                by_fiddle: written_by(it, account),
            }),
            None => None,
        });
    reviewed.chain(said).collect()
}

pub const NEEDS_AN_ANSWER: &str = "**fiddle made no change: it needs an answer before it can.**";

fn quoted(text: &str) -> String {
    text.trim()
        .lines()
        .map(|line| match line.trim_end() {
            "" => ">".to_string(),
            kept => format!("> {kept}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn asked(question: &str, answered: &Answered) -> String {
    format!(
        "{NEEDS_AN_ANSWER}\n\n{}\n\nAnswer on this pull request and fiddle will look again.\n\n{}",
        quoted(question),
        answered.marker()
    )
}

pub const STOPPED_WITHOUT_AN_ANSWER: &str =
    "**fiddle stopped before it reached an answer, and changed nothing.**";

pub fn stopped(reason: &str, answered: &Answered) -> String {
    format!(
        "{STOPPED_WITHOUT_AN_ANSWER}\n\nWhat stopped it: {}\n\nWrite on this pull request and \
         fiddle will look again.\n\n{}",
        reason.trim(),
        answered.marker()
    )
}

pub const CHANGED: &str = "**fiddle changed this pull request for the direction above.**";

pub fn changed(summary: &str, answered: &Answered) -> String {
    format!("{CHANGED}\n\n{}\n\n{}", summary.trim(), answered.marker())
}

pub const REJECTED: &str =
    "**fiddle published nothing for the direction above: its evaluation rejected the run.**";

pub fn rejected(findings: &[String], answered: &Answered) -> String {
    let listed: Vec<String> = findings
        .iter()
        .map(|finding| format!("- {}", finding.trim()))
        .collect();
    format!(
        "{REJECTED}\n\nWhat the evaluation found:\n\n{}\n\nWrite on this pull request and \
         fiddle will look again.\n\n{}",
        listed.join("\n"),
        answered.marker()
    )
}

pub fn reply(summary: &str, answered: &Answered) -> String {
    format!("{NO_CHANGE}\n\n{}\n\n{}", summary.trim(), answered.marker())
}

#[cfg(test)]
mod rendered {
    use super::*;

    #[test]
    fn every_reply_opens_on_a_bold_headline_and_hides_its_marker_last() {
        let answered = Answered::default();
        for (body, headline) in [
            (reply("- `metrics.go` emits a sample", &answered), NO_CHANGE),
            (
                changed("- added a constructor for `Sampled`", &answered),
                CHANGED,
            ),
            (
                asked("Where are Claude's two comments?", &answered),
                NEEDS_AN_ANSWER,
            ),
            (
                stopped("the turn budget of 24 was exhausted", &answered),
                STOPPED_WITHOUT_AN_ANSWER,
            ),
        ] {
            assert!(
                body.starts_with(headline)
                    && headline.starts_with("**")
                    && headline.ends_with("**"),
                "a reviewer reads one bold line first: {body}"
            );
            assert!(
                body.trim_end().ends_with(&answered.marker()),
                "the marker is last, where an HTML comment renders as nothing: {body}"
            );
            assert!(
                body.contains("\n\n"),
                "paragraphs are separated, so Markdown does not run them together: {body}"
            );
        }
    }

    #[test]
    fn a_question_is_quoted_line_by_line() {
        let body = asked(
            "Which two comments?\n\nThey are not in the brief.",
            &Answered::default(),
        );
        assert!(
            body.contains("> Which two comments?\n>\n> They are not in the brief."),
            "every line of the question stays inside the quote: {body}"
        );
    }
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
    account: Option<String>,
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
            account: None,
        })
    }

    pub fn written_as(mut self, account: Option<String>) -> Self {
        self.account = account;
        self
    }

    fn comments_path(&self) -> String {
        format!("/repos/{}/issues/{}/comments", self.repo, self.pr)
    }
}

impl FromStepParams for AnswerPullRequest {
    fn from_params(_executor: &Executor<'_>, params: &StepParams) -> Result<Self, EffectError> {
        let kind = EffectName::shipped(PULL_REQUEST_ANSWERED);
        Ok(Self::new(
            required(&params.repo, &kind, "repo")?,
            required(&params.pull_request, &kind, "pull_request")?,
            required(&params.body, &kind, "body")?,
        )?
        .written_as(params.account.clone()))
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
            .filter(|comment| trusted(comment, self.account.as_deref()))
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
            submitted_at: None,
            author: ActorRef {
                login: "peel".to_string(),
                id: 1,
            },
            author_association: "MEMBER".to_string(),
            state: "COMMENTED".to_string(),
            body: "commit message and PR description is missing".to_string(),
            commit_id: "3acb655".to_string(),
            by_a_bot: false,
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
            by_a_bot: false,
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
        let (reviews, conversation) = unanswered(vec![review(1), review(2)], vec![reply], None);

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
        let (reviews, _) = unanswered(vec![review(1)], vec![outsider], None);
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
            None,
        );
        assert_eq!(
            conversation.iter().map(|it| it.comment).collect::<Vec<_>>(),
            vec![12]
        );
    }

    #[test]
    fn what_was_answered_before_is_kept_as_context_and_what_was_not_is_left_out() {
        let answered = Answered::of(&[review(1)], &[comment(3, "MEMBER", "and this too")]);
        let fiddle = comment(9, "MEMBER", &reply("already here", &answered));
        let earlier = already_answered(
            &[review(1), review(2)],
            &[
                comment(3, "MEMBER", "and this too"),
                fiddle,
                comment(4, "NONE", "an outsider"),
                comment(5, "MEMBER", "Yes, do them."),
            ],
            None,
        );
        let kept: Vec<(&str, bool)> = earlier
            .iter()
            .map(|it| (it.body.as_str(), it.by_fiddle))
            .collect();
        assert_eq!(kept.len(), 3, "{kept:?}");
        assert_eq!(
            kept[0],
            ("commit message and PR description is missing", false)
        );
        assert_eq!(kept[1], ("and this too", false));
        assert!(
            kept[2].1
                && kept[2].0.contains("already here")
                && !kept[2].0.contains("fiddle:answered"),
            "fiddle's own answer is kept without its marker: {kept:?}"
        );
    }

    fn said(id: u64, association: &str, body: &str) -> HumanResponse {
        comment(id, association, body)
    }

    fn said_by(id: u64, login: &str, association: &str, body: &str) -> HumanResponse {
        HumanResponse {
            author: ActorRef {
                login: login.to_string(),
                id: 2,
            },
            ..comment(id, association, body)
        }
    }

    #[test]
    fn a_marker_the_configured_account_wrote_is_trusted_though_its_association_is_none() {
        let marker = Answered {
            reviews: BTreeSet::new(),
            comments: BTreeSet::from([4]),
        }
        .marker();
        let conversation = vec![
            said(4, "MEMBER", "@fiddle-ai fix CI"),
            said_by(
                5,
                "fiddle-ai[bot]",
                "NONE",
                &format!("**fiddle changed this pull request**\n\n{marker}"),
            ),
        ];
        let (_, unanswered_with_account) =
            unanswered(vec![], conversation.clone(), Some("fiddle-ai[bot]"));
        assert!(
            unanswered_with_account.iter().all(|it| it.comment != 4),
            "the answered mention is answered"
        );
        let (_, unanswered_without) = unanswered(vec![], conversation, None);
        assert!(
            unanswered_without.iter().any(|it| it.comment == 4),
            "without the account, a NONE author's marker is not trusted, as today"
        );
    }

    #[test]
    fn the_configured_account_is_matched_whatever_the_case_of_its_login() {
        let marker = Answered::of(&[], &[said(4, "MEMBER", "@fiddle-ai fix CI")]).marker();
        let conversation = vec![
            said(4, "MEMBER", "@fiddle-ai fix CI"),
            said_by(5, "Fiddle-AI[bot]", "NONE", &marker),
        ];
        let (_, kept) = unanswered(vec![], conversation.clone(), Some("fiddle-ai[bot]"));
        assert!(
            kept.is_empty(),
            "a login differing only in case is the account, so its marker is trusted: {kept:?}"
        );
        let (_, kept) = unanswered(vec![], conversation, Some("someone-else[bot]"));
        assert_eq!(
            kept.iter().map(|it| it.comment).collect::<Vec<_>>(),
            vec![4],
            "another account's name does not make this NONE author trusted"
        );
    }

    #[test]
    fn a_login_that_only_starts_like_the_account_is_not_trusted() {
        let marker = Answered::of(&[], &[said(4, "MEMBER", "@fiddle-ai fix CI")]).marker();
        let conversation = |login: &str| {
            vec![
                said(4, "MEMBER", "@fiddle-ai fix CI"),
                said_by(5, login, "NONE", &marker),
            ]
        };
        let (_, kept) = unanswered(vec![], conversation("fiddle-ai"), Some("fiddle-ai[bot]"));
        assert_eq!(
            kept.iter().map(|it| it.comment).collect::<Vec<_>>(),
            vec![4],
            "a person named fiddle-ai is not the App fiddle-ai[bot], so their marker answers nothing"
        );
        let (_, kept) = unanswered(
            vec![],
            conversation("fiddle-ai[bot]"),
            Some("fiddle-ai[bot]"),
        );
        assert!(
            kept.is_empty(),
            "the same marker from the account answers the mention: {kept:?}"
        );
    }

    #[test]
    fn the_configured_accounts_comments_are_never_direction() {
        let (_, kept) = unanswered(
            vec![],
            vec![said_by(9, "fiddle-ai[bot]", "MEMBER", "@fiddle-ai x")],
            Some("fiddle-ai[bot]"),
        );
        assert!(
            kept.is_empty(),
            "the account's own comment does not steer: {kept:?}"
        );
        let (_, kept) = unanswered(
            vec![],
            vec![said_by(9, "fiddle-ai[bot]", "MEMBER", "@fiddle-ai x")],
            None,
        );
        assert_eq!(
            kept.iter().map(|it| it.comment).collect::<Vec<_>>(),
            vec![9],
            "without the account, the same comment is direction, as today"
        );
    }

    #[test]
    fn a_review_the_configured_account_wrote_is_never_direction() {
        let by_account = Reviewed {
            author: ActorRef {
                login: "fiddle-ai[bot]".to_string(),
                id: 2,
            },
            ..review(7)
        };
        let (kept, _) = unanswered(
            vec![by_account.clone(), review(8)],
            vec![],
            Some("fiddle-ai[bot]"),
        );
        assert_eq!(
            kept.iter().map(|it| it.id).collect::<Vec<_>>(),
            vec![8],
            "the account's review is dropped and a member's review beside it is kept"
        );
        let (kept, _) = unanswered(vec![by_account, review(8)], vec![], None);
        assert_eq!(
            kept.iter().map(|it| it.id).collect::<Vec<_>>(),
            vec![7, 8],
            "without the account, both reviews are direction, as today"
        );
    }

    #[test]
    fn what_the_configured_account_answered_is_context_and_its_reply_is_fiddles() {
        let answered = Answered::of(&[], &[said(4, "MEMBER", "@fiddle-ai fix CI")]);
        let conversation = vec![
            said(4, "MEMBER", "@fiddle-ai fix CI"),
            said_by(5, "fiddle-ai[bot]", "NONE", &changed("fixed it", &answered)),
        ];
        let with_account = already_answered(&[], &conversation, Some("fiddle-ai[bot]"));
        let kept: Vec<(&str, &str, bool)> = with_account
            .iter()
            .map(|it| (it.author.as_str(), it.body.as_str(), it.by_fiddle))
            .collect();
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert_eq!(kept[0], ("peel", "@fiddle-ai fix CI", false));
        assert!(
            kept[1].0 == "fiddle-ai[bot]"
                && kept[1].2
                && kept[1].1.contains("fixed it")
                && !kept[1].1.contains("fiddle:answered"),
            "the account's reply is fiddle's and is kept without its marker: {kept:?}"
        );
        assert!(
            already_answered(&[], &conversation, None).is_empty(),
            "without the account, a NONE author's reply answers nothing, as today"
        );
    }

    #[test]
    fn a_reply_without_a_marker_cannot_be_built() {
        assert!(AnswerPullRequest::new("o/r".into(), 1, "no marker".into()).is_err());
    }
}
