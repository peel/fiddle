use crate::effect::EffectContext;
use crate::github::{read_conversation, read_one_comment, GhError, HumanResponse};
use crate::human::interpret::{interpret, InterpretationBounds};
use fiddle_core::decision::{
    decision_request_id, parse_marker, ActorRef, DecisionBinding, DecisionRequestId,
    InterpretedHumanDecision,
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
    RequestEdited { comment: u64 },
    #[error("reply {comment} changed between the listing and the re-read")]
    ReplyEdited { comment: u64 },
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
pub struct IgnoredReply {
    pub comment: u64,
    pub author: ActorRef,
    pub reason: Ignored,
}

#[derive(Clone, Debug)]
pub struct HumanAnswer {
    pub interpreted: InterpretedHumanDecision,
    pub acted_on: HumanResponse,
}

#[derive(Clone, Debug)]
pub struct DecisionResolution {
    pub answer: Option<HumanAnswer>,
    pub considered: Vec<HumanResponse>,
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
    pub max_pages: u32,
    pub project: &'a str,
    pub invocation_ref: &'a str,
    pub kind: EffectName,
    pub target: &'a str,
    pub payload: &'a str,
    pub allowlist: &'a [u64],
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
    let conversation = read_conversation(&ctx.gh, walk.repo, walk.pr, walk.max_pages, &ctx.cancel)
        .await
        .map_err(unreadable)?;
    let mut asking = conversation.iter().filter_map(|comment| {
        parse_marker(&comment.body)
            .ok()
            .filter(|binding| binding.request == request)
            .map(|binding| (comment, binding))
    });
    let Some((asked, binding)) = asking.next() else {
        return Err(DecisionError::RequestAbsent(request));
    };
    let duplicates = asking.count();
    if duplicates > 0 {
        return Err(DecisionError::DuplicateRequest {
            request,
            count: duplicates + 1,
        });
    }

    trace.step(DecisionStep::ParseBinding);
    if binding.effect != effect {
        return Err(DecisionError::ForeignEffect {
            found: binding.effect.0.clone(),
            derived: effect.0,
        });
    }

    trace.step(DecisionStep::SelectCandidates);
    let (candidates, ignored) = select_candidates(&conversation, asked.comment, walk.allowlist);

    trace.step(DecisionStep::ReReadCandidates);
    let asked_again = reread(ctx, walk.repo, asked, |comment| {
        DecisionError::RequestEdited { comment }
    })
    .await?;
    if asked_again.created_at != asked_again.updated_at {
        return Err(DecisionError::RequestEdited {
            comment: asked.comment,
        });
    }
    let mut considered = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        considered.push(
            reread(ctx, walk.repo, candidate, |comment| {
                DecisionError::ReplyEdited { comment }
            })
            .await?,
        );
    }

    trace.step(DecisionStep::ReObserveState);
    observe(ctx, walk, &binding).await?;

    let acted_on = considered.iter().max_by_key(|reply| reply.comment).cloned();
    considered.sort_by_key(|reply| reply.comment);
    let Some(acted_on) = acted_on else {
        return Ok(DecisionResolution {
            answer: None,
            considered,
            ignored,
        });
    };
    trace.step(DecisionStep::Interpret);
    let interpreted = interpret(model, question, &acted_on.body, bounds).await;

    trace.step(DecisionStep::ComparePayload);
    if binding.payload != payload {
        return Err(DecisionError::ForeignPayload {
            found: binding.payload.0.clone(),
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

fn select_candidates<'c>(
    conversation: &'c [HumanResponse],
    asked: u64,
    allowlist: &[u64],
) -> (Vec<&'c HumanResponse>, Vec<IgnoredReply>) {
    let mut candidates = Vec::new();
    let mut ignored = Vec::new();
    let mut decline = |comment: &HumanResponse, reason| {
        ignored.push(IgnoredReply {
            comment: comment.comment,
            author: comment.author.clone(),
            reason,
        });
    };
    for comment in conversation {
        if comment.comment == asked {
            decline(comment, Ignored::RequestComment);
        } else if comment.comment < asked {
        } else if comment.is_bot {
            decline(comment, Ignored::NotAPerson);
        } else if !allowlist.contains(&comment.author.id) {
            decline(comment, Ignored::ActorNotAuthorized);
        } else {
            candidates.push(comment);
        }
    }
    (candidates, ignored)
}

async fn reread(
    ctx: &EffectContext,
    repo: &str,
    listed: &HumanResponse,
    moved: fn(u64) -> DecisionError,
) -> Result<HumanResponse, DecisionError> {
    let current = read_one_comment(&ctx.gh, repo, listed.comment, &ctx.cancel)
        .await
        .map_err(unreadable)?;
    if current.updated_at != listed.updated_at {
        return Err(moved(listed.comment));
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
        .gh
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

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(id: u64, author: u64, is_bot: bool) -> HumanResponse {
        HumanResponse {
            comment: id,
            author: ActorRef {
                id: author,
                login: format!("u{author}"),
            },
            body: String::new(),
            created_at: "2026-08-10T00:00:00Z".to_string(),
            updated_at: "2026-08-10T00:00:00Z".to_string(),
            is_bot,
            author_association: "COLLABORATOR".to_string(),
        }
    }

    #[test]
    fn the_candidate_rule_is_indifferent_to_the_order_the_pages_arrived_in() {
        let conversation = [
            comment(10, 1, false),
            comment(20, 1, false),
            comment(30, 1, false),
            comment(40, 1, false),
        ];
        let chosen = |order: &[HumanResponse]| {
            let mut ids: Vec<u64> = select_candidates(order, 20, &[1])
                .0
                .iter()
                .map(|c| c.comment)
                .collect();
            ids.sort_unstable();
            ids
        };
        assert_eq!(chosen(&conversation), [30, 40]);

        let mut scrambled = conversation.clone();
        scrambled.reverse();
        assert_eq!(chosen(&scrambled), [30, 40]);

        scrambled.swap(0, 2);
        assert_eq!(chosen(&scrambled), [30, 40]);
    }

    #[test]
    fn every_comment_that_is_not_a_candidate_is_recorded_with_the_reason_it_is_not() {
        let conversation = [
            comment(10, 9, false),
            comment(20, 1, false),
            comment(30, 9, false),
            comment(40, 1, true),
            comment(50, 1, false),
        ];
        let (candidates, ignored) = select_candidates(&conversation, 20, &[1]);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].comment, 50);
        assert_eq!(
            ignored
                .iter()
                .map(|i| (i.comment, i.reason))
                .collect::<Vec<_>>(),
            [
                (20, Ignored::RequestComment),
                (30, Ignored::ActorNotAuthorized),
                (40, Ignored::NotAPerson),
            ],
            "a comment written before the question is not a declined reply"
        );
    }

    #[test]
    fn an_authorized_login_over_an_unauthorized_id_is_not_authorized() {
        let mut impostor = comment(30, 999_999, false);
        impostor.author.login = "u1".to_string();
        let conversation = [comment(20, 1, false), impostor];
        let (candidates, ignored) = select_candidates(&conversation, 20, &[1]);
        assert!(candidates.is_empty(), "a login is not an identity");
        assert!(ignored
            .iter()
            .any(|i| i.comment == 30 && i.reason == Ignored::ActorNotAuthorized));
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
