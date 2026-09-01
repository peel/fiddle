use crate::effect::EffectContext;
use crate::github::{read_one_comment, GhError, HumanResponse};
use crate::human::interpret::{interpret, InterpretationBounds};
use crate::human::{GitHubConversation, HumanInteractionPort, InteractionRef};
use crate::jira::conversation::{ConversationError, JiraConversation, JiraReply};
use fiddle_core::decision::{
    decision_request_id, parse_marker, DecisionBinding, DecisionRequestId, InterpretedHumanDecision,
};
use fiddle_core::{effect_id, payload_hash, EffectId, EffectName, PayloadHash};

#[derive(Clone, Copy, Debug, Eq, PartialEq, crate::effect::VariantCount)]
pub enum DecisionStep {
    RecomputeIdentity,
    FindRequest,
    ParseBinding,
    SelectCandidates,
    ReReadCandidates,
    ReObserveState,
    Interpret,
    ComparePayload,
}

impl DecisionStep {
    pub fn as_str(&self) -> &'static str {
        match self {
            DecisionStep::RecomputeIdentity => "recompute_identity",
            DecisionStep::FindRequest => "find_request",
            DecisionStep::ParseBinding => "parse_binding",
            DecisionStep::SelectCandidates => "select_candidates",
            DecisionStep::ReReadCandidates => "re_read_candidates",
            DecisionStep::ReObserveState => "re_observe_state",
            DecisionStep::Interpret => "interpret",
            DecisionStep::ComparePayload => "compare_payload",
        }
    }
}

pub trait DecisionTrace: Send + Sync {
    fn step(&self, step: DecisionStep);
}

#[derive(Debug, thiserror::Error, crate::effect::VariantCount)]
pub enum DecisionError {
    #[error("{count} comments name request {request:?}, expected at most one")]
    DuplicateRequest {
        request: DecisionRequestId,
        count: usize,
    },
    #[error("no comment names request {0:?}")]
    RequestAbsent(DecisionRequestId),
    #[error("the marker names effect {found} and this run derives {derived}")]
    ForeignEffect { found: String, derived: String },
    #[error("the marker names payload {found} and this run rebuilds {derived}")]
    ForeignPayload { found: String, derived: String },
    #[error("the request comment {comment} has been edited since fiddle wrote it")]
    RequestEdited { comment: String },
    #[error("reply {comment} changed between the listing and the re-read")]
    ReplyEdited { comment: String },
    #[error("the pull request is no longer open")]
    NotOpen,
    #[error("the pull request is already ready for review")]
    AlreadyReady,
    #[error("the head is {found} and the decision was asked about {approved}")]
    HeadMoved { found: String, approved: String },
    #[error("the conversation could not be read: {0}")]
    Unreadable(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, crate::effect::VariantCount)]
#[serde(rename_all = "snake_case")]
pub enum Ignored {
    RequestComment,
    NotAPerson,
    ActorNotAuthorized,
}

impl Ignored {
    pub fn as_str(&self) -> &'static str {
        match self {
            Ignored::RequestComment => "the request comment is not a reply to itself",
            Ignored::NotAPerson => "author is not a person",
            Ignored::ActorNotAuthorized => "actor not authorized",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Decider {
    GitHubAuthor(u64),
    JiraAccount(String),
}

impl std::fmt::Display for Decider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Decider::GitHubAuthor(id) => write!(f, "github author {id}"),
            Decider::JiraAccount(account) => write!(f, "jira account {account}"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IgnoredReply {
    pub comment: String,
    pub author: Decider,
    pub reason: Ignored,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reply {
    pub comment: String,
    pub author: Decider,
    pub body: String,
}

#[derive(Clone, Debug)]
pub struct HumanAnswer {
    pub interpreted: InterpretedHumanDecision,
    pub acted_on: Reply,
}

#[derive(Clone, Debug)]
pub struct DecisionResolution {
    pub answer: Option<HumanAnswer>,
    pub considered: Vec<Reply>,
    pub ignored: Vec<IgnoredReply>,
}

impl DecisionResolution {
    pub fn acted_on_nothing(&self) -> bool {
        self.answer.is_none()
    }
}

pub struct DecisionWalk<'a> {
    pub repo: &'a str,
    pub pr: u64,
    pub project: &'a str,
    pub invocation_ref: &'a str,
    pub kind: EffectName,
    pub target: &'a str,
    pub payload: &'a str,
    pub allowlist: &'a [Decider],
    pub asked_on: &'a InteractionRef,
}

impl DecisionWalk<'_> {
    fn identity(&self) -> (DecisionRequestId, EffectId, PayloadHash) {
        let effect = effect_id(
            self.project,
            self.invocation_ref,
            self.kind.as_str(),
            self.target,
        );
        let request = decision_request_id(self.project, self.invocation_ref, &effect);
        (request, effect, payload_hash(self.payload))
    }
}

#[derive(Clone, Debug)]
struct Listed {
    comment: String,
    order: u64,
    author: Decider,
    written_by_fiddle: bool,
    body: String,
    created_at: String,
    updated_at: String,
}

impl Listed {
    fn of_github(comment: &HumanResponse) -> Self {
        Listed {
            comment: comment.comment.to_string(),
            order: comment.comment,
            author: Decider::GitHubAuthor(comment.author.id),
            written_by_fiddle: comment.is_bot,
            body: comment.body.clone(),
            created_at: comment.created_at.clone(),
            updated_at: comment.updated_at.clone(),
        }
    }

    fn of_jira(at: usize, reply: &JiraReply) -> Self {
        Listed {
            comment: reply.comment.clone(),
            order: at as u64,
            author: Decider::JiraAccount(reply.author.account_id.clone()),
            written_by_fiddle: false,
            body: reply.text.clone(),
            created_at: reply.created.clone(),
            updated_at: reply.updated.clone(),
        }
    }

    fn answered(&self, body: String) -> Reply {
        Reply {
            comment: self.comment.clone(),
            author: self.author.clone(),
            body,
        }
    }

    fn speaks_as_the_asker(&self, asked: &Listed) -> bool {
        match self.author {
            Decider::GitHubAuthor(_) => false,
            Decider::JiraAccount(_) => self.author == asked.author,
        }
    }
}

enum Site {
    GitHub,
    Jira,
}

struct Located {
    site: Site,
    listed: Vec<Listed>,
    asked: Listed,
    binding: DecisionBinding,
}

pub async fn resolve<M>(
    ctx: &EffectContext,
    walk: &DecisionWalk<'_>,
    question: &str,
    model: M,
    bounds: &InterpretationBounds,
    trace: &dyn DecisionTrace,
) -> Result<DecisionResolution, DecisionError>
where
    M: rig_core::completion::CompletionModel + 'static,
{
    trace.step(DecisionStep::RecomputeIdentity);
    let (request, effect, payload) = walk.identity();

    trace.step(DecisionStep::FindRequest);
    let located = locate(ctx, walk, &request).await?;

    trace.step(DecisionStep::ParseBinding);
    if located.binding.effect != effect {
        return Err(DecisionError::ForeignEffect {
            found: located.binding.effect.0.clone(),
            derived: effect.0,
        });
    }

    trace.step(DecisionStep::SelectCandidates);
    let (mut candidates, ignored) =
        select_candidates(&located.listed, &located.asked, walk.allowlist);
    candidates.sort_by_key(|held| held.order);

    trace.step(DecisionStep::ReReadCandidates);
    let considered = confirm(ctx, walk, &located, &candidates).await?;

    trace.step(DecisionStep::ReObserveState);
    observe(ctx, walk, &located.binding).await?;

    let Some(acted_on) = considered.last().cloned() else {
        return Ok(DecisionResolution {
            answer: None,
            considered,
            ignored,
        });
    };
    trace.step(DecisionStep::Interpret);
    let interpreted = interpret(model, question, &acted_on.body, bounds).await;

    trace.step(DecisionStep::ComparePayload);
    if located.binding.payload != payload {
        return Err(DecisionError::ForeignPayload {
            found: located.binding.payload.0.clone(),
            derived: payload.0,
        });
    }

    Ok(DecisionResolution {
        answer: Some(HumanAnswer {
            interpreted,
            acted_on,
        }),
        considered,
        ignored,
    })
}

async fn locate(
    ctx: &EffectContext,
    walk: &DecisionWalk<'_>,
    request: &DecisionRequestId,
) -> Result<Located, DecisionError> {
    match walk.asked_on {
        InteractionRef::GitHubPullRequestComment { .. } => {
            let read = GitHubConversation
                .responses(ctx, walk.asked_on)
                .await
                .map_err(unreadable)?;
            let listed: Vec<Listed> = read.iter().map(Listed::of_github).collect();
            let (asked, binding) = one_request(&listed, request)?;
            Ok(Located {
                site: Site::GitHub,
                listed,
                asked,
                binding,
            })
        }
        InteractionRef::JiraIssueComment { issue, .. } => {
            let read = JiraConversation::reading(issue.clone())
                .responses(ctx, walk.asked_on)
                .await
                .map_err(unread_issue)?;
            let listed: Vec<Listed> = read
                .iter()
                .enumerate()
                .map(|(at, reply)| Listed::of_jira(at, reply))
                .collect();
            let (asked, binding) = one_request(&listed, request)?;
            Ok(Located {
                site: Site::Jira,
                listed,
                asked,
                binding,
            })
        }
    }
}

fn one_request(
    listed: &[Listed],
    request: &DecisionRequestId,
) -> Result<(Listed, DecisionBinding), DecisionError> {
    let mut naming = listed.iter().filter_map(|held| {
        parse_marker(&held.body)
            .ok()
            .filter(|binding| &binding.request == request)
            .map(|binding| (held, binding))
    });
    let Some((asked, binding)) = naming.next() else {
        return Err(DecisionError::RequestAbsent(request.clone()));
    };
    let duplicates = naming.count();
    if duplicates > 0 {
        return Err(DecisionError::DuplicateRequest {
            request: request.clone(),
            count: duplicates + 1,
        });
    }
    Ok((asked.clone(), binding))
}

fn select_candidates<'c>(
    listed: &'c [Listed],
    asked: &Listed,
    allowlist: &[Decider],
) -> (Vec<&'c Listed>, Vec<IgnoredReply>) {
    let mut candidates = Vec::new();
    let mut ignored = Vec::new();
    for held in listed {
        let decline = |reason| IgnoredReply {
            comment: held.comment.clone(),
            author: held.author.clone(),
            reason,
        };
        if held.comment == asked.comment {
            ignored.push(decline(Ignored::RequestComment));
        } else if held.order < asked.order {
        } else if held.written_by_fiddle || held.speaks_as_the_asker(asked) {
            ignored.push(decline(Ignored::NotAPerson));
        } else if !allowlist.contains(&held.author) {
            ignored.push(decline(Ignored::ActorNotAuthorized));
        } else {
            candidates.push(held);
        }
    }
    (candidates, ignored)
}

async fn confirm(
    ctx: &EffectContext,
    walk: &DecisionWalk<'_>,
    located: &Located,
    candidates: &[&Listed],
) -> Result<Vec<Reply>, DecisionError> {
    match located.site {
        Site::Jira => {
            if located.asked.created_at != located.asked.updated_at {
                return Err(DecisionError::RequestEdited {
                    comment: located.asked.comment.clone(),
                });
            }
            Ok(candidates
                .iter()
                .map(|held| held.answered(held.body.clone()))
                .collect())
        }
        Site::GitHub => {
            let asked_again = reread(ctx, walk.repo, &located.asked, |comment| {
                DecisionError::RequestEdited { comment }
            })
            .await?;
            if asked_again.created_at != asked_again.updated_at {
                return Err(DecisionError::RequestEdited {
                    comment: located.asked.comment.clone(),
                });
            }
            let mut considered = Vec::with_capacity(candidates.len());
            for candidate in candidates {
                let current = reread(ctx, walk.repo, candidate, |comment| {
                    DecisionError::ReplyEdited { comment }
                })
                .await?;
                considered.push(candidate.answered(current.body));
            }
            Ok(considered)
        }
    }
}

async fn reread(
    ctx: &EffectContext,
    repo: &str,
    listed: &Listed,
    moved: fn(String) -> DecisionError,
) -> Result<HumanResponse, DecisionError> {
    let current = read_one_comment(
        ctx.gh_client().map_err(unreadable)?,
        repo,
        listed.order,
        &ctx.cancel,
    )
    .await
    .map_err(unreadable)?;
    if current.updated_at != listed.updated_at {
        return Err(moved(listed.comment.clone()));
    }
    Ok(current)
}

async fn observe(
    ctx: &EffectContext,
    walk: &DecisionWalk<'_>,
    binding: &DecisionBinding,
) -> Result<(), DecisionError> {
    let path = format!("/repos/{}/pulls/{}", walk.repo, walk.pr);
    let response = ctx
        .gh_client()
        .map_err(unreadable)?
        .api("GET", &path, None, &ctx.cancel)
        .await
        .map_err(unreadable)?;
    let missing = |field: &str| DecisionError::Unreadable(format!("{path} carried no {field}"));

    let state = response.body["state"]
        .as_str()
        .ok_or_else(|| missing("state"))?;
    let draft = response.body["draft"]
        .as_bool()
        .ok_or_else(|| missing("draft state"))?;
    let head = response.body["head"]["sha"]
        .as_str()
        .ok_or_else(|| missing("head sha"))?;

    if state != "open" {
        return Err(DecisionError::NotOpen);
    }
    if !draft {
        return Err(DecisionError::AlreadyReady);
    }
    if head != binding.head_sha {
        return Err(DecisionError::HeadMoved {
            found: head.to_string(),
            approved: binding.head_sha.clone(),
        });
    }
    Ok(())
}

fn unreadable(error: GhError) -> DecisionError {
    DecisionError::Unreadable(error.to_string())
}

fn unread_issue(error: ConversationError) -> DecisionError {
    DecisionError::Unreadable(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jira::conversation::JiraActor;

    const STAMP: &str = "2026-08-10T00:00:00Z";

    fn on_github(id: u64, author: u64, is_bot: bool) -> Listed {
        Listed::of_github(&HumanResponse {
            comment: id,
            author: fiddle_core::decision::ActorRef {
                id: author,
                login: format!("u{author}"),
            },
            body: String::new(),
            created_at: STAMP.to_string(),
            updated_at: STAMP.to_string(),
            is_bot,
            author_association: "COLLABORATOR".to_string(),
        })
    }

    fn on_jira(at: usize, id: &str, account: &str) -> Listed {
        Listed::of_jira(
            at,
            &JiraReply {
                issue: "IDENT-1".to_string(),
                comment: id.to_string(),
                author: JiraActor {
                    account_id: account.to_string(),
                    display_name: "a person".to_string(),
                },
                text: String::new(),
                created: STAMP.to_string(),
                updated: STAMP.to_string(),
            },
        )
    }

    fn ids(chosen: &[&Listed]) -> Vec<String> {
        chosen.iter().map(|held| held.comment.clone()).collect()
    }

    #[test]
    fn the_candidate_rule_is_indifferent_to_the_order_the_pages_arrived_in() {
        let conversation = [
            on_github(10, 1, false),
            on_github(20, 1, false),
            on_github(30, 1, false),
            on_github(40, 1, false),
        ];
        let asked = on_github(20, 1, false);
        let chosen = |order: &[Listed]| {
            let mut named = ids(&select_candidates(order, &asked, &[Decider::GitHubAuthor(1)]).0);
            named.sort();
            named
        };
        assert_eq!(chosen(&conversation), ["30", "40"]);

        let mut scrambled = conversation.clone();
        scrambled.reverse();
        assert_eq!(chosen(&scrambled), ["30", "40"]);

        scrambled.swap(0, 2);
        assert_eq!(chosen(&scrambled), ["30", "40"]);
    }

    #[test]
    fn every_comment_that_is_not_a_candidate_is_recorded_with_the_reason_it_is_not() {
        let conversation = [
            on_github(10, 9, false),
            on_github(20, 1, false),
            on_github(30, 9, false),
            on_github(40, 1, true),
            on_github(50, 1, false),
        ];
        let asked = on_github(20, 1, false);
        let (candidates, ignored) =
            select_candidates(&conversation, &asked, &[Decider::GitHubAuthor(1)]);
        assert_eq!(ids(&candidates), ["50"]);
        assert_eq!(
            ignored
                .iter()
                .map(|i| (i.comment.as_str(), i.reason))
                .collect::<Vec<_>>(),
            [
                ("20", Ignored::RequestComment),
                ("30", Ignored::ActorNotAuthorized),
                ("40", Ignored::NotAPerson),
            ],
            "a comment written before the question is not a declined reply"
        );
    }

    #[test]
    fn an_authorized_login_over_an_unauthorized_id_is_not_authorized() {
        let impostor = HumanResponse {
            comment: 30,
            author: fiddle_core::decision::ActorRef {
                id: 999_999,
                login: "u1".to_string(),
            },
            body: String::new(),
            created_at: STAMP.to_string(),
            updated_at: STAMP.to_string(),
            is_bot: false,
            author_association: "COLLABORATOR".to_string(),
        };
        let listed = Listed::of_github(&impostor);
        assert_eq!(
            listed.author,
            Decider::GitHubAuthor(999_999),
            "the identity a reply carries is the numeric id and never the login the site \
             printed beside it"
        );

        let asked = on_github(20, 1, false);
        let conversation = [asked.clone(), listed];
        let (candidates, ignored) =
            select_candidates(&conversation, &asked, &[Decider::GitHubAuthor(1)]);
        assert!(candidates.is_empty(), "a login is not an identity");
        assert!(ignored
            .iter()
            .any(|i| i.comment == "30" && i.reason == Ignored::ActorNotAuthorized));
    }

    const CROSSED: u64 = 505_401;

    #[test]
    fn a_jira_account_id_spelled_like_an_allowed_github_id_is_not_that_decider() {
        let crossed = CROSSED.to_string();
        let asked = on_jira(0, "10001", "5b10a2844c20165700ede21g");
        let answered = on_jira(1, "10002", &crossed);
        let conversation = [asked.clone(), answered];

        let (refused, declined) = select_candidates(
            &conversation,
            &asked,
            &[Decider::GitHubAuthor(CROSSED), Decider::GitHubAuthor(1)],
        );

        assert!(
            refused.is_empty(),
            "a jira account id and a github author id are two names in two namespaces, and one \
             allowlist entry cannot stand for both"
        );
        assert_eq!(
            declined
                .iter()
                .map(|i| (i.comment.as_str(), i.reason))
                .collect::<Vec<_>>(),
            [
                ("10001", Ignored::RequestComment),
                ("10002", Ignored::ActorNotAuthorized),
            ],
            "and the reply is recorded as declined rather than dropped"
        );

        let (allowed, _) = select_candidates(
            &conversation,
            &asked,
            &[Decider::JiraAccount(crossed.clone())],
        );
        assert_eq!(
            ids(&allowed),
            ["10002"],
            "the same account id named as a jira account does authorize, so the refusal above \
             cannot pass by refusing every reply whatever the allowlist holds"
        );
    }

    #[test]
    fn a_github_author_id_spelled_like_an_allowed_jira_account_is_not_that_decider() {
        let crossed = CROSSED.to_string();
        let asked = on_github(20, 1, false);
        let conversation = [asked.clone(), on_github(30, CROSSED, false)];

        let (refused, _) = select_candidates(
            &conversation,
            &asked,
            &[Decider::JiraAccount(crossed.clone())],
        );
        assert!(
            refused.is_empty(),
            "the refusal holds in both directions, so neither channel can borrow the other's \
             allowlist"
        );

        let (allowed, _) =
            select_candidates(&conversation, &asked, &[Decider::GitHubAuthor(CROSSED)]);
        assert_eq!(ids(&allowed), ["30"]);
    }

    #[test]
    fn a_comment_fiddle_wrote_after_its_own_question_is_not_a_person_answering() {
        let us = "5b10a2844c20165700ede21g";
        let asked = on_jira(0, "10001", us);
        let conversation = [asked.clone(), on_jira(1, "10002", us)];

        let (candidates, ignored) = select_candidates(
            &conversation,
            &asked,
            &[Decider::JiraAccount(us.to_string())],
        );

        assert!(
            candidates.is_empty(),
            "fiddle's own later comments are not replies, or a run would answer itself"
        );
        assert_eq!(
            ignored.iter().map(|i| i.reason).collect::<Vec<_>>(),
            [Ignored::RequestComment, Ignored::NotAPerson],
            "and the account that asked is the account this run writes as, however the \
             allowlist reads"
        );
    }

    #[test]
    fn a_comment_written_before_the_question_is_not_an_answer_to_it() {
        let decider = "70121:aaaaaaaa";
        let asked = on_jira(1, "10002", "5b10a2844c20165700ede21g");
        let conversation = [on_jira(0, "10001", decider), asked.clone()];

        let (candidates, ignored) = select_candidates(
            &conversation,
            &asked,
            &[Decider::JiraAccount(decider.to_string())],
        );

        assert!(
            candidates.is_empty(),
            "an approval written before the question was asked answers a different question"
        );
        assert_eq!(
            ignored
                .iter()
                .map(|i| i.comment.as_str())
                .collect::<Vec<_>>(),
            ["10002"],
            "and the earlier comment is not recorded as a declined reply either"
        );
    }

    #[test]
    fn every_decider_reads_as_the_channel_it_belongs_to() {
        let github = Decider::GitHubAuthor(CROSSED).to_string();
        let jira = Decider::JiraAccount(CROSSED.to_string()).to_string();

        assert_ne!(
            github, jira,
            "one number in two namespaces reads as two deciders where an operator reads it"
        );
        assert!(github.contains("505401") && jira.contains("505401"));
    }

    #[test]
    fn every_reason_a_reply_was_declined_has_exactly_one_spelling() {
        let spellings: [&str; Ignored::VARIANT_COUNT] = [
            Ignored::RequestComment,
            Ignored::NotAPerson,
            Ignored::ActorNotAuthorized,
        ]
        .map(|reason| reason.as_str());
        for (at, reason) in spellings.iter().enumerate() {
            assert!(!reason.is_empty());
            assert!(
                !spellings[at + 1..].contains(reason),
                "{reason:?} spells two different exclusions"
            );
        }
    }

    #[test]
    fn every_step_of_the_order_has_its_own_stable_name() {
        let names: [&str; DecisionStep::VARIANT_COUNT] = [
            DecisionStep::RecomputeIdentity,
            DecisionStep::FindRequest,
            DecisionStep::ParseBinding,
            DecisionStep::SelectCandidates,
            DecisionStep::ReReadCandidates,
            DecisionStep::ReObserveState,
            DecisionStep::Interpret,
            DecisionStep::ComparePayload,
        ]
        .map(|step| step.as_str());
        for (at, name) in names.iter().enumerate() {
            assert!(!names[at + 1..].contains(name), "{name:?} names two steps");
        }
    }
}
