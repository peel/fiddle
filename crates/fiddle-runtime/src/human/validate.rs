//! The eight-step validation order, and every way it refuses.
//!
//! A run that publishes a question stops. A different process — no workspace, no
//! journal, no memory of the first — comes back later and has to establish, from
//! the world alone, that the thing a person approved is the thing it is about to
//! do. This module is that establishment, and [`resolve`] is the whole of it.
//!
//! # Why the order is a contract rather than a sequence
//!
//! Steps 1 to 6 are deterministic and they run *before* the one model call.
//!
//! That is not a performance argument. Every refusal below is a decision the
//! shell can reach on its own — there is no request comment, there are two, the
//! marker names an effect this run does not derive, a comment was rewritten after
//! it was listed, the pull request was closed, it is already out of draft, its
//! head moved, nobody authorized has replied at all. A walk that asked a model to
//! read a comment it was going to refuse anyway has given the model a say in a
//! decision the shell had already made, and the model's say is the one part of
//! this system nobody can bound. So the order is announced to a
//! [`DecisionTrace`] step by step, in the shape
//! [`ExecutionStep`](crate::effect::ExecutionStep) established, and
//! `decision_protocol.rs` asserts both the sequence and the zero model calls that
//! each refusal costs.
//!
//! # A parse is not an authentication
//!
//! [`parse_marker`] cannot tell a request comment from a quotation of one, and
//! `fiddle-core` is right not to try: a verbatim quote is byte-identical, an
//! *edited* quote whose four fields are still well formed parses just as
//! happily, and neither distinction is a property of the body. The crate has no
//! access to the world, so it cannot have the answer.
//!
//! The whole of that safety therefore rests here, in two places:
//!
//! - **Step 2 refuses a plurality rather than choosing from it.** More than one
//!   comment naming this request is a state to report, not a set to pick the
//!   authoritative member of — the rule
//!   [`EnsurePullRequest`](crate::github::EnsurePullRequest) already applies to
//!   two open pull requests for one head, for the same reason. First is not more
//!   authoritative than last, and a conversation carrying two is a conversation
//!   somebody assembled.
//! - **Step 3's recomputation is what authenticates.** A body that parses has
//!   proven only that somebody can type. What proves it is *this* run's question
//!   is that the marker's [`EffectId`] equals the one recomputed from `(project,
//!   invocation_ref, kind, target)` — four values that come from the run's own
//!   canonical inputs and that the conversation cannot supply. Nothing here acts
//!   on a binding that arrived from parsing alone, and a later refactor that made
//!   it possible to reach step 7 with a parsed-but-unverified binding would be a
//!   defect of this module whatever any test said.
//!
//! # Candidates are chosen by id, never by position
//!
//! [`read_conversation`]'s query pins no sort order. GitHub's default for issue
//! comments happens to be ascending by id, but the adapter does not ask for it,
//! so a changed default — or a proxy, or a later API version — could reorder the
//! pages without anything failing.
//!
//! Every rule here is therefore stated as a comparison of comment ids, which is
//! a total order over the conversation whatever sequence the pages arrive in. A
//! candidate is a comment whose id is **greater than the request comment's**, and
//! not a comment that appears after it in the returned vector; the reply that
//! decides is the one with the **greatest id**, and not the last element. Against
//! a sorted fixture the two readings are indistinguishable, which is exactly why
//! `a_scrambled_listing_reaches_the_same_decision` exists.
//!
//! # What the model is handed, and what it is not
//!
//! [`interpret`] takes the question as text precisely so that it can never
//! receive an identity, and that guarantee is total inside it and conditional on
//! whoever composes the string. This is that caller, and it composes nothing: the
//! `question` argument is passed through byte for byte. The [`EffectId`], the
//! [`PayloadHash`], the head sha and the binding are all in scope here and none of
//! them is interpolated into anything that reaches a provider. A diagnostic that
//! wants one of them wants it in a log line.
//!
//! [`EffectId`]: fiddle_core::EffectId
//! [`PayloadHash`]: fiddle_core::PayloadHash

use crate::effect::EffectContext;
use crate::github::{read_conversation, read_one_comment, GhError, HumanResponse};
use crate::human::interpret::{interpret, InterpretationBounds};
use fiddle_core::decision::{
    decision_request_id, parse_marker, ActorRef, DecisionBinding, DecisionRequestId,
    InterpretedHumanDecision,
};
use fiddle_core::{effect_id, payload_hash, EffectId, EffectKind, PayloadHash};

/// One step of the validation order, named.
///
/// A closed enum for [`ExecutionStep`](crate::effect::ExecutionStep)'s reason: the
/// order is the contract, and a contract spelled by whoever happened to be
/// writing a log line is not one. [`DecisionStep::as_str`] is the single
/// spelling.
///
/// Eight variants and eight steps, with no gaps — unlike the authorization
/// order, whose steps 5 and 9 are not moments in this design. Every one of these
/// is work something does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecisionStep {
    /// 1. The effect id, the payload digest and the request id, from this run's
    ///    own canonical inputs.
    RecomputeIdentity,
    /// 2. Which comment in the conversation is this run's question. Exactly one,
    ///    or a refusal.
    FindRequest,
    /// 3. The marker's binding, and the recomputation that authenticates it.
    ParseBinding,
    /// 4. Which comments are replies whose author may decide.
    SelectCandidates,
    /// 5. The request comment and every candidate, read again by id.
    ReReadCandidates,
    /// 6. The pull request's state, draft flag and head, as they are now.
    ReObserveState,
    /// 7. The one bounded model call, and the only step that is not arithmetic.
    Interpret,
    /// 8. The rebuilt operation's payload digest against the binding's.
    ComparePayload,
}

impl DecisionStep {
    /// The step's stable name.
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

/// Where the walk writes down which step it is on.
///
/// The sibling of [`EffectTrace`](crate::effect::EffectTrace), and separate from
/// it rather than a widening of it: that trait's `step` carries an
/// [`EffectKind`] because the authorization order repeats once per effect, and
/// this one runs once for the single effect a question gates. There is no default
/// implementation, for [`EffectTrace`]'s reason — a sink that discarded by
/// omission would let a production path go dark without anybody deciding it
/// should.
///
/// [`EffectTrace`]: crate::effect::EffectTrace
pub trait DecisionTrace: Send + Sync {
    fn step(&self, step: DecisionStep);
}

/// Every way the walk refuses, each naming what actually moved.
///
/// One variant per condition rather than one shared `Stale`, because a refusal
/// whose message is "stale" sends its reader back to the conversation to guess
/// which of eight things happened. Each message below names the thing.
#[derive(Debug, thiserror::Error)]
pub enum DecisionError {
    /// More than one comment names this request. Reported, never chosen from.
    #[error("{count} comments name request {request:?}, expected at most one")]
    DuplicateRequest {
        request: DecisionRequestId,
        count: usize,
    },
    /// No comment names this request. The question was never asked here, or it
    /// was asked about something this run does not derive.
    #[error("no comment names request {0:?}")]
    RequestAbsent(DecisionRequestId),
    /// The marker parsed and names an effect this run does not derive. The
    /// refusal that makes step 3 an authentication rather than a formality.
    #[error("the marker names effect {found} and this run derives {derived}")]
    ForeignEffect { found: String, derived: String },
    /// The marker names a payload digest the rebuilt operation does not produce.
    ///
    /// Not in the epic's sketch of this enum, which lists step 8 in the walk and
    /// then gives it no spelling; added rather than folded into
    /// [`DecisionError::ForeignEffect`] because the two are different claims. An
    /// effect that does not match means this question is not ours; a payload that
    /// does not match means it is ours and the work has changed underneath it.
    #[error("the marker names payload {found} and this run rebuilds {derived}")]
    ForeignPayload { found: String, derived: String },
    /// A comment is not the comment that was read. Either it changed between the
    /// listing and the re-read, or — for the request comment, which fiddle wrote
    /// and never rewrites — it has been edited at all.
    #[error("comment {comment} changed since it was listed")]
    Edited { comment: u64 },
    /// The pull request was closed or merged. Whatever was approved, it is not a
    /// transition this object can still make.
    #[error("the pull request is no longer open")]
    NotOpen,
    /// The pull request is already out of draft. Nothing is owed.
    #[error("the pull request is already ready for review")]
    AlreadyReady,
    /// The revision the pull request points at is not the revision the question
    /// was asked about.
    #[error("the head is {found} and the decision was asked about {approved}")]
    HeadMoved { found: String, approved: String },
    /// The conversation, a comment or the pull request could not be read.
    ///
    /// Never an absence, for [`read_conversation`]'s reason: "nobody has
    /// answered" is a fact this system acts on by continuing to wait, and a read
    /// that failed is not that fact.
    #[error("the conversation could not be read: {0}")]
    Unreadable(String),
}

/// Why one comment was not counted as a reply.
///
/// A closed enum rather than a written-out string, for [`DecisionStep`]'s reason
/// and one more: these reach a person, in the follow-up a run publishes, so two
/// spellings of "not on the allowlist" would be two things an operator has to
/// learn are the same.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Ignored {
    /// fiddle's own question. It contains the word an interpreter is looking for
    /// — it asks whether something may be marked ready — so a walk that let it be
    /// a candidate would read its own question as the first thing resembling an
    /// answer.
    RequestComment,
    /// An account whose type is `Bot`, or a comment an app posted through
    /// somebody's credential. Both spellings, because both are ways of not being
    /// a person.
    NotAPerson,
    /// An author whose immutable numeric id is not on the allowlist.
    ActorNotAuthorized,
}

impl Ignored {
    /// The reason, as a reader sees it.
    pub fn as_str(&self) -> &'static str {
        match self {
            Ignored::RequestComment => "the request comment is not a reply to itself",
            Ignored::NotAPerson => "author is not a person",
            Ignored::ActorNotAuthorized => "actor not authorized",
        }
    }
}

/// A comment that was read, observed, and not counted.
///
/// Recorded rather than silently dropped, so that somebody who tried to answer
/// and was not allowed to learns that they were not. A filter that dropped these
/// would make "nobody has replied" and "three people replied and none of them may
/// decide" the same observation, and only one of those is a state an operator can
/// fix.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IgnoredReply {
    pub comment: u64,
    pub author: ActorRef,
    pub reason: Ignored,
}

/// The reply that decided, and what it was read to mean.
#[derive(Clone, Debug)]
pub struct HumanAnswer {
    /// The four-valued verdict, and the only thing a model chose.
    pub interpreted: InterpretedHumanDecision,
    /// The comment it was read from, whole, so a later reader can go and look.
    pub acted_on: HumanResponse,
}

/// What the walk established, once it established anything.
///
/// Not to be confused with
/// [`ResolvedDecision`](crate::effect::ResolvedDecision), which is the narrower
/// thing the executor's step 4 consumes: an *approval* bound to the question it
/// answered, with no spelling for the other three verdicts. This type is the
/// whole observation the walk made — including a rejection, including a
/// conversation nobody has answered yet — and turning one into the other is the
/// caller's act, through
/// [`ResolvedDecision::approved`](crate::effect::ResolvedDecision::approved).
#[derive(Clone, Debug)]
pub struct DecisionResolution {
    /// `None` when no authorized reply exists yet. Not a failure: an unanswered
    /// question is what a suspended run is waiting for, and the caller's next act
    /// is to go on waiting.
    pub answer: Option<HumanAnswer>,
    /// Every candidate reply, oldest id first — the one that decided and the ones
    /// it superseded. Kept because the superseded ones are the evidence that a
    /// person changed their mind, which is the whole reason the *last* reply is
    /// the one that counts.
    pub considered: Vec<HumanResponse>,
    /// Every comment that was read and not counted, with the reason.
    pub ignored: Vec<IgnoredReply>,
}

impl DecisionResolution {
    /// Whether the walk found nobody's answer to act on.
    pub fn acted_on_nothing(&self) -> bool {
        self.answer.is_none()
    }
}

/// The deterministic inputs of one walk: the conversation to read, the effect
/// being gated, and who may decide.
///
/// Grouped into one struct rather than passed as nine arguments, and it carries
/// nothing the model sees — the question travels as [`resolve`]'s own parameter,
/// beside the model rather than inside this, so that the two kinds of input are
/// not adjacent in any signature.
pub struct DecisionWalk<'a> {
    /// `owner/name`, as the API path spells it.
    pub repo: &'a str,
    /// The pull request whose conversation carries the question.
    pub pr: u64,
    /// How many pages of that conversation may be read before the read is a
    /// refusal rather than a truncation. [`read_conversation`]'s bound.
    pub max_pages: u32,
    /// The run's project, hashed into the effect identity.
    pub project: &'a str,
    /// The run's invocation reference, hashed into the effect identity.
    pub invocation_ref: &'a str,
    /// Which kind of effect the question gates.
    pub kind: EffectKind,
    /// The gated effect's canonical target.
    pub target: &'a str,
    /// The rebuilt operation's canonical payload — the bytes, not the digest,
    /// because the digest is what this module derives and comparing a digest a
    /// caller computed would move step 8's arithmetic outside step 8.
    pub payload: &'a str,
    /// The immutable numeric user ids that may decide.
    ///
    /// Ids and not logins: a login can be changed and the vacated name reclaimed,
    /// so an allowlist matching one would let a renamed-and-reclaimed account
    /// inherit an approver's authority. And not `author_association` either,
    /// which says what somebody's relationship to the repository is rather than
    /// whether this deployment nominated them.
    pub allowlist: &'a [u64],
}

impl DecisionWalk<'_> {
    /// The three identities this run derives for itself, in step 1.
    fn identity(&self) -> (DecisionRequestId, EffectId, PayloadHash) {
        let effect = effect_id(self.project, self.invocation_ref, self.kind, self.target);
        let request = decision_request_id(self.project, self.invocation_ref, &effect);
        (request, effect, payload_hash(self.payload))
    }
}

/// Walk the eight steps, and either resolve a decision or say what refused it.
///
/// Each step is announced to `trace` **before** the work behind it, so a trace
/// that stops at `FindRequest` says the conversation read is what failed, rather
/// than leaving a reader to infer it from the absence of the next entry. The
/// deliberate consequence is that a trace never announces work that did not
/// happen: a walk nobody has answered yet announces six steps and stops, because
/// there was nothing to interpret and nothing to compare.
///
/// `question` is text, and it is passed to [`interpret`] byte for byte. See this
/// module's documentation before composing anything into it.
///
/// Generic over Rig's own
/// [`CompletionModel`](rig_core::completion::CompletionModel) rather than over a
/// trait of ours, for [`interpret`]'s reason: a test substitutes a scripted model
/// and drives these branches without a credential or a socket, and there is no
/// second implementation to keep in step with the first.
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
    // Step 1. Nothing is read yet, and that is the point: these three values come
    // from the run's own canonical inputs, so they are what everything the
    // conversation offers is measured against.
    trace.step(DecisionStep::RecomputeIdentity);
    let (request, effect, payload) = walk.identity();

    // Step 2. Which comment is this run's question.
    trace.step(DecisionStep::FindRequest);
    let conversation = read_conversation(&ctx.gh, walk.repo, walk.pr, walk.max_pages, &ctx.cancel)
        .await
        .map_err(unreadable)?;
    // The parse here is a *sieve* and grants nothing. It answers "which comment",
    // and it does so by the one field a forger can copy off the visible
    // conversation — which is why step 3 exists and why this loop is not it.
    let mut asking = conversation.iter().filter_map(|comment| {
        parse_marker(&comment.body)
            .ok()
            .filter(|binding| binding.request == request)
            .map(|binding| (comment, binding))
    });
    let Some((asked, binding)) = asking.next() else {
        return Err(DecisionError::RequestAbsent(request));
    };
    // The whole remainder rather than a boolean, because
    // `DecisionError::DuplicateRequest` reports the number and a reader deciding
    // what happened to their conversation wants it.
    let duplicates = asking.count();
    if duplicates > 0 {
        return Err(DecisionError::DuplicateRequest {
            request,
            count: duplicates + 1,
        });
    }

    // Step 3. The recomputation, which is the whole of the authentication. A
    // marker naming this request id proves only that its author could read the
    // conversation; the effect id is derived from four values the conversation
    // does not carry.
    trace.step(DecisionStep::ParseBinding);
    if binding.effect != effect {
        return Err(DecisionError::ForeignEffect {
            found: binding.effect.0.clone(),
            derived: effect.0,
        });
    }

    // Step 4. Which comments are replies somebody authorized wrote.
    trace.step(DecisionStep::SelectCandidates);
    let (candidates, ignored) = select_candidates(&conversation, asked.comment, walk.allowlist);

    // Step 5. Everything the decision rests on, read again by its own id.
    trace.step(DecisionStep::ReReadCandidates);
    let asked_again = reread(ctx, walk.repo, asked).await?;
    // fiddle wrote this comment and has no path that edits one, so an edit is
    // somebody else's. `created_at` and `updated_at` are equal on a comment
    // nobody has touched, which makes the whole history visible rather than only
    // the window between the listing and this read.
    if asked_again.created_at != asked_again.updated_at {
        return Err(DecisionError::Edited {
            comment: asked.comment,
        });
    }
    let mut considered = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        // The re-read is what is carried onward, not the listing. An approval this
        // run acts on is the text of a comment it has just read, rather than a
        // snapshot of one taken some pages ago.
        considered.push(reread(ctx, walk.repo, candidate).await?);
    }

    // Step 6. The world, as it is now rather than as it was when the question was
    // asked.
    trace.step(DecisionStep::ReObserveState);
    observe(ctx, walk, &binding).await?;

    // Step 7. The last authorized reply decides — the greatest comment id, not
    // the last element of a vector whose order nothing pinned. The earlier ones
    // stay in `considered`, because an approval followed by "wait, no" is the case
    // this rule exists for and the retraction is the evidence.
    //
    // Chosen *before* `considered` is put in order, and deliberately: sorting
    // first would make the two lines below agree with `last()` and there would be
    // no arrangement in which this comparison was the thing deciding. Two claims
    // are being made — which reply decides, and in what order a reader is handed
    // the evidence — and each is held by its own line.
    let acted_on = considered.iter().max_by_key(|reply| reply.comment).cloned();
    considered.sort_by_key(|reply| reply.comment);
    let Some(acted_on) = acted_on else {
        // Nobody authorized has answered. Not a refusal, and not a model call:
        // there is no reply to read.
        return Ok(DecisionResolution {
            answer: None,
            considered,
            ignored,
        });
    };
    trace.step(DecisionStep::Interpret);
    let interpreted = interpret(model, question, &acted_on.body, bounds).await;

    // Step 8. The second of the two payload comparisons, and a different claim
    // from the executor's: this one is against what the conversation recorded,
    // and step 6 of the authorization order is against what the proposal carried.
    // Deleting either as redundant would delete one of the two claims.
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

/// Which comments are replies whose author may decide, and why each of the rest
/// is not.
///
/// Every rule is a comparison of ids. A comment whose id is *below* the request
/// comment's predates the question and is not recorded as an exclusion — it is
/// not a reply that was declined, it is a conversation that was already going on
/// — while the request comment itself is recorded, because it is the one comment
/// somebody could reasonably expect to have been read and it must never be.
///
/// What comes back is a *set* in the order the conversation arrived in, and no
/// caller may read anything into that order. Putting the candidates in id order
/// here would be the tidier thing and it would hide a defect: with the list
/// sorted, [`resolve`]'s choice of the greatest id agrees with the last element
/// however that choice is written, so no arrangement would ever distinguish the
/// two. The ordering belongs where it is used and is applied there.
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
            // Written before the question. Nothing here is an answer to it.
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

/// One comment, read again by its own id, refused if it is not the comment that
/// was listed.
///
/// `updated_at` is the evidence: it is equal to `created_at` on a comment nobody
/// has touched and moves on every edit, so a value that differs from the one the
/// listing carried means the text this walk is about to read is not the text it
/// selected. An approval that was rewritten after it was listed is not an
/// approval this run may act on.
async fn reread(
    ctx: &EffectContext,
    repo: &str,
    listed: &HumanResponse,
) -> Result<HumanResponse, DecisionError> {
    let current = read_one_comment(&ctx.gh, repo, listed.comment, &ctx.cancel)
        .await
        .map_err(unreadable)?;
    if current.updated_at != listed.updated_at {
        return Err(DecisionError::Edited {
            comment: listed.comment,
        });
    }
    Ok(current)
}

/// The pull request as it is now: open, still a draft, and at the revision the
/// question was asked about.
///
/// Three separate refusals rather than one, because they are three different
/// things for an operator to do about. Every field is required rather than
/// defaulted, for [`EnsurePullRequestReady::read`]'s reason: a missing `draft`
/// defaulted either way would turn a `gh` this client cannot read into a verdict,
/// and one of the two verdicts mutates.
///
/// The head is compared against the *binding* and not against the target. For
/// [`EnsurePullRequestReady`](crate::github::EnsurePullRequestReady) the two agree
/// — its target carries the head, so step 3 has already pinned it — but the rule
/// belongs to the question rather than to one effect kind, and a kind whose target
/// omitted the revision would otherwise have no such check at all.
///
/// [`EnsurePullRequestReady::read`]: crate::github::EnsurePullRequestReady
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

/// Every read failure, as the one refusal that is not a fact about the decision.
fn unreadable(error: GhError) -> DecisionError {
    DecisionError::Unreadable(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The walk is proven end to end against a scripted `gh` and a scripted model
    /// in `tests/decision_protocol.rs`. What is worth a unit test here is the
    /// candidate rule, whose whole subject is that it does not depend on the
    /// order the conversation arrived in — a property an integration fixture can
    /// only demonstrate one arrangement of at a time.
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

    /// The rule is an id comparison, so every permutation of one conversation
    /// selects the same candidates.
    ///
    /// This is the case that tells "everything with a higher id than the request
    /// comment" apart from "everything after the request comment in the vector".
    /// Both pass against a sorted fixture; only the first passes here.
    ///
    /// The comparison is over the *membership* and not over the sequence,
    /// because the sequence is not a claim this function makes — see its
    /// documentation for why it deliberately does not sort.
    #[test]
    fn the_candidate_rule_is_indifferent_to_the_order_the_pages_arrived_in() {
        let conversation = [
            comment(10, 1, false), // before the question
            comment(20, 1, false), // the question
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

    /// Each exclusion is recorded with its own reason, and a comment that
    /// predates the question is not an exclusion at all.
    #[test]
    fn every_comment_that_is_not_a_candidate_is_recorded_with_the_reason_it_is_not() {
        let conversation = [
            comment(10, 9, false), // before the question, and not on the allowlist
            comment(20, 1, false), // the question
            comment(30, 9, false), // a stranger
            comment(40, 1, true),  // a bot with an authorized id
            comment(50, 1, false), // the one reply that counts
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

    /// The allowlist is consulted on the numeric id, and the login is not
    /// consulted at all.
    ///
    /// Asserted here as well as through the whole walk because it is one
    /// comparison, and a unit test can put a login collision in front of it
    /// directly: the author below is spelled exactly like an authorized one and
    /// carries a different id.
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

    /// The reasons a person reads are one spelling each.
    #[test]
    fn every_reason_a_reply_was_declined_has_exactly_one_spelling() {
        let spellings = [
            Ignored::RequestComment,
            Ignored::NotAPerson,
            Ignored::ActorNotAuthorized,
        ]
        .map(|reason| reason.as_str());
        assert_eq!(spellings.len(), 3);
        for (at, reason) in spellings.iter().enumerate() {
            assert!(!reason.is_empty());
            assert!(
                !spellings[at + 1..].contains(reason),
                "{reason:?} spells two different exclusions"
            );
        }
    }

    /// Eight steps, eight names, and no two the same.
    #[test]
    fn every_step_of_the_order_has_its_own_stable_name() {
        let names = [
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
