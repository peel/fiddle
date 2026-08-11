//! Where a question reaches a person, and how a later reader finds it again.
//!
//! The module the epic's `## Contracts` addresses [`InteractionRef`] to. What
//! lives here is the outward half of the decision exchange: a name for the
//! conversation a suspended run is waiting on, the one rendering of that name,
//! the question itself as something the executor performs, and the port the
//! question travels through.
//!
//! [`interpret`] is the other half: the one bounded model call in the decision
//! walk, whose whole output is one enum and one string. [`validate`] is what
//! stands between them — the walk that decides whether a reply is an answer this
//! run may act on.
//!
//! # Asking is a mutation, and it goes through the executor like any other
//!
//! [`PublishDecisionRequest`] is an [`IntegrationOperation`], and that is not
//! ceremony. `POST /repos/{repo}/issues/{pr}/comments` documents no idempotency
//! key of any kind, so a request comment whose answer was lost and which is then
//! re-sent makes a **second** comment — and two comments naming one question is
//! precisely the state [`validate`] cannot resolve, because the candidate replies
//! are the ones after "the" request comment and there would be two of those. The
//! executor's step 3 is exactly the inspect-before-write the endpoint does not
//! offer, and its step 8 is what settles a lost answer by reading instead of by
//! asking again.
//!
//! Its [`IntegrationOperation::minimum`] is
//! [`Automatic`](HumanDecisionRequirement::Automatic), and it has to be: a
//! question that required a question to ask would not terminate. That is
//! asserted rather than left to a reader of the struct — see
//! `publishing_a_question_never_requires_a_question`.

pub mod interpret;
pub mod validate;

use crate::effect::{AuthorizedEffect, EffectContext, IntegrationOperation, ObservedState};
use crate::github::{read_conversation, GhError};
use fiddle_core::{
    parse_marker, render_marker, HumanDecisionRequest, HumanDecisionRequirement, MarkerError,
};

/// A GitHub comment, as the adapter that reads one describes it.
///
/// Re-exported and not defined here: it carries `is_bot`, `author_association`
/// and the two timestamps, which are facts about a GitHub comment and so the
/// adapter's business rather than the domain's. It is named here because
/// [`HumanInteractionPort::responses`] answers with them, and a reader of that
/// signature should not have to go looking.
///
/// # `ActorRef` and `InterpretedHumanDecision` are deliberately *not* beside it
///
/// Both are `fiddle-core`'s and both were re-exported here for a while, on the
/// reasoning that a reader of `human` should find the whole vocabulary of a
/// decision in one place. That was wrong, and `github/mod.rs` had already written
/// down why:
///
/// > a second path to it through the GitHub adapter would invite a consumer to
/// > reach for the domain's identity type by way of the client that happens to
/// > read one
///
/// `human` is such a client. The rule there is the rule here, and there is now
/// one rule rather than two comments disagreeing.
///
/// What settles it is that nobody walked the second path. Every consumer reaches
/// these types directly — `github/comments.rs` takes `fiddle_core::ActorRef`, and
/// [`interpret`], which is this module's *own submodule*, takes
/// `fiddle_core::decision::InterpretedHumanDecision`. So the re-exports had zero
/// consumers, which makes them inert surface, which this milestone has refused on
/// three separate beans. [`HumanResponse`] stays because the port's signature
/// names it and removing it would send a reader of that signature to another
/// module for a type this one hands them.
pub use crate::github::HumanResponse;

/// How far a postcondition read will follow the conversation before refusing.
///
/// A bound rather than "all of it", because [`read_conversation`] has no
/// unbounded mode by design: reaching the bound is an error and never a
/// truncation, since "I read everything and found no request" and "I read as
/// much as I was allowed and found no request" are different facts and only the
/// first is a decision.
///
/// Ten pages is a thousand comments at the adapter's page size. It is generous
/// for a conversation fiddle itself opened, and it is a number rather than an
/// absence so that a repository whose conversation is pathological refuses
/// visibly instead of spending a run walking it.
const CONVERSATION_PAGES: u32 = 10;

/// The conversation a question was put on, and where an answer will be found.
///
/// One variant. The RFC's Jira and attended arms are not written, because a
/// variant nothing constructs is the inert surface M2's `RequireHumanDecision`
/// was criticised for being — and adding one later is a line of code, while
/// removing one consumers have matched on is not.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum InteractionRef {
    GitHubPullRequestComment { repo: String, pr: u64, comment: u64 },
}

/// One spelling of a conversation, so nothing can disagree about how it is
/// named.
///
/// A suspended run names its conversation in three places — the `--json`
/// outcome's reason, the published bundle's progress entry, and the line a
/// person reads at a terminal — and each of them is produced by different code.
/// Three `format!`s would be three chances for one of them to drift, and the
/// consequence of drifting is an operator who cannot find the pull request a
/// run told them to go and look at. There is one implementation, and every one
/// of those surfaces reaches it.
impl std::fmt::Display for InteractionRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InteractionRef::GitHubPullRequestComment { repo, pr, comment } => {
                write!(f, "{repo}#{pr} comment {comment}")
            }
        }
    }
}

/// The canonical target identity for publishing one question.
///
/// `{repo}#{pr}:{request_id}` — the conversation, and which question on it.
/// Written here rather than at each call site for the reason
/// [`pull_request_target`](crate::github::pull_request_target) is: it is hashed
/// into the effect identity, so a process recomputing that identity from the same
/// three facts has to arrive at the same string.
///
/// The request id is in the target and not only in the payload, and that is what
/// makes a *second* question on one pull request a different effect rather than a
/// changed request. One conversation therefore holds as many questions as the run
/// needs, which is the property
/// `a_marker_for_another_request_is_not_the_postcondition` pins.
pub fn decision_request_target(
    repo: &str,
    pr: u64,
    request: &fiddle_core::DecisionRequestId,
) -> String {
    format!("{repo}#{pr}:{}", request.0)
}

/// Render the comment a person actually reads, ending with the marker a later
/// process finds it by.
///
/// **This is [`PublishDecisionRequest::payload`],** and the two being one
/// function is load-bearing rather than tidy. The payload is what step 6 hashes
/// and what an approval is minted against, and the body is what gets posted; two
/// renderings would let the question a person answered differ from the question
/// that was hashed, so an approval would be spent on text nobody read. The
/// executor's step 6 compares the digests and would refuse, but the failure would
/// arrive as a caller defect at the mutation rather than as the drift it is.
/// `the_posted_body_carries_the_marker_and_is_the_hashed_payload` asserts the
/// posted bytes and the hashed bytes are the same bytes.
///
/// Prose first and the marker last, because the marker is bookkeeping. It renders
/// as an HTML comment, so a person reading the conversation sees the question and
/// not the digests, while the API body a later process reads carries them exactly.
///
/// An empty list renders no heading at all. A request with no risks is a request
/// with nothing to say under that heading, and an empty **Risks** section reads
/// as a claim that there are none rather than as a field nobody filled in.
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

    // Which part of the system is asking, and about what. A person deciding is
    // entitled to both: an approval is given to a capability acting on a work
    // item, and a comment that named neither would ask them to agree to a
    // sentence with no subject.
    body.push_str(&format!("\n_Asked by {} for ", request.capability));
    match &request.work_ref {
        Some(work) => body.push_str(&format!("{work}")),
        None => body.push_str(&request.invocation_ref),
    }
    body.push_str(&format!(" at {}._\n\n", request.binding.head_sha));

    body.push_str(&render_marker(&request.binding));
    body
}

/// A question observed to be on the conversation already.
///
/// The comment id is carried because it is the whole of what a later reader
/// needs: [`validate`] mines the replies that came *after* this comment, so the
/// question's own id is what orders the candidate set.
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

    /// The comment id, because that is what both a person and a later process
    /// look the question up by.
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

/// Put one question on one pull request's conversation, exactly once.
pub struct PublishDecisionRequest {
    /// `owner/name`, as the API path spells it.
    repo: String,
    /// The pull request whose conversation carries the question.
    pr: u64,
    /// The question. Everything hashed into the payload comes from here.
    request: HumanDecisionRequest,
}

impl PublishDecisionRequest {
    pub fn new(repo: String, pr: u64, request: HumanDecisionRequest) -> Self {
        Self { repo, pr, request }
    }

    /// The canonical target identity to propose this effect under.
    ///
    /// Read off [`self.request.binding.request`](fiddle_core::DecisionBinding),
    /// and **not** off the [`HumanDecisionRequest::request`] field beside it. See
    /// `asking` below for why those are two different values
    /// and why only one of them is safe to use.
    pub fn target(&self) -> String {
        decision_request_target(&self.repo, self.pr, self.asking())
    }

    /// Which question this is, from the one field that reaches the marker.
    ///
    /// # Why this method exists at all
    ///
    /// [`HumanDecisionRequest`] carries the request id **twice** — as its own
    /// `request` field and as `binding.request` — and nothing makes the two agree.
    /// Only `binding.request` is rendered into the marker, because
    /// [`render_marker`] takes the binding, so it is the only one a later process
    /// can ever read back out of the conversation.
    ///
    /// An operation that matched on the other field would therefore publish a
    /// marker naming one id and then look for a different one. It would find
    /// nothing, conclude it had not asked yet, and **post again on every attempt,
    /// forever** — the unbounded duplicate supply this whole operation goes
    /// through the executor to prevent, arriving through the one door the executor
    /// cannot close, because from step 3's point of view the postcondition
    /// genuinely is absent every time.
    ///
    /// So the marker, the target and the postcondition lookup all read this one
    /// method, which reads the one field that reaches the wire. The duplication in
    /// the type is filed as `fiddle-11vj`; until it is collapsed, this is where
    /// the two are prevented from being confused.
    fn asking(&self) -> &fiddle_core::DecisionRequestId {
        &self.request.binding.request
    }

    /// The conversation collection, which is the one path this operation reads
    /// and the one it writes.
    ///
    /// `/issues/{pr}/comments` and never `/pulls/{pr}/comments`. The two do not
    /// overlap at GitHub: the first is the timeline a person types into and the
    /// second is review comments pinned to lines of a diff. A question posted to
    /// the second would be a question about a line, and — worse — `read_conversation`
    /// has no path that names it, so the run would never find its own request
    /// again and would ask on every attempt.
    fn comments_path(&self) -> String {
        format!("/repos/{}/issues/{}/comments", self.repo, self.pr)
    }

    /// Is this comment *this* question?
    ///
    /// The request id and nothing else, which is a narrower test than comparing
    /// the whole binding and is the right one. The id is derived from the run and
    /// the gated effect, so a comment carrying it is either this run's own
    /// request or a tampered copy of it — and in both cases the postcondition
    /// "a comment carrying this request's marker exists here" holds. Comparing
    /// the whole binding would read an edited marker as *absent*, post a second
    /// comment under the same request id, and manufacture exactly the duplicate
    /// state this operation exists to avoid. Detecting the tamper is
    /// [`validate`]'s job, where there is a run to compare against and somewhere
    /// to report it.
    ///
    /// A body with no marker is the ordinary case — every other comment in the
    /// conversation looks like that — and so is a body whose marker names another
    /// question. Neither is an error here.
    ///
    /// The id compared against is `asking`'s, which is
    /// the one this operation's own marker carries. Comparing the other one would
    /// post forever; that method's documentation is where the reason lives.
    fn is_this_request(&self, body: &str) -> bool {
        match parse_marker(body) {
            Ok(binding) => &binding.request == self.asking(),
            // Including `Malformed` and `Version`: a body this build cannot read a
            // marker out of is not a comment it can claim as its own request.
            // Refusing the whole read on one would let any comment in the
            // conversation stop the run.
            Err(MarkerError::Absent | MarkerError::Malformed(_) | MarkerError::Version(_)) => false,
        }
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for PublishDecisionRequest {
    type State = PublishedRequest;

    /// Automatic, and it must be: a question that needed a question would not
    /// terminate.
    ///
    /// This is the one operation in the build whose minimum is structurally
    /// forced rather than chosen. `combine` may still be strengthened to `Deny`
    /// by a deployment document — an operator who wants fiddle to ask nobody
    /// anything can have that — but there is no coherent `Human` here, so the
    /// declaration is not a policy position and cannot be relaxed into one.
    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    /// The rendered comment body: the payload *is* the question.
    ///
    /// So a question whose text changed is a widened request and step 6 refuses
    /// it, rather than a different comment arriving under an approval given for
    /// the earlier wording. Every other operation's payload is a normalized
    /// request document; this one's is the artifact itself, because the artifact
    /// is what the request consists of.
    fn payload(&self) -> String {
        render_request(&self.request)
    }

    /// Is the question already on the conversation?
    ///
    /// Called twice by the executor — before the mutation to find out whether it
    /// is needed, and after it to find out whether it happened — and both calls
    /// do the same thing. The second is what settles a lost answer by *reading*:
    /// the comment was posted, `gh` died before saying so, and this read finds
    /// the comment and reports the effect committed. Nothing re-posts.
    ///
    /// Every page, or an error. [`read_conversation`] refuses rather than
    /// truncates at `CONVERSATION_PAGES`, and an unreadable conversation is
    /// never an empty one — which matters more here than almost anywhere: reading
    /// a failed listing as "no request yet" would post a duplicate question on
    /// every attempt for as long as the listing stayed broken.
    ///
    /// Two comments naming one question is a state to report and never a set to
    /// pick from — the same rule `EnsurePullRequest` applies to two open pull
    /// requests, and here it is sharper, because [`validate`] chooses candidate
    /// replies by their position relative to *the* request comment and there
    /// would be two of those.
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

    /// One `POST` to the conversation, carrying the rendered body — sent through
    /// [`HumanInteractionPort`] rather than around it.
    ///
    /// The body is [`IntegrationOperation::payload`]'s own output and not a second
    /// rendering of the same request, so the bytes a person reads are the bytes
    /// step 6 hashed. Reached only with an [`AuthorizedEffect`] in hand, so there
    /// is no path to posting a question that skipped identity, payload and policy —
    /// and the envelope is passed on rather than dropped, because the port is the
    /// thing that must not be callable without one.
    ///
    /// **The port's answer is discarded, and that is deliberate.** It is what
    /// GitHub *said*; the executor's next act is to read the world back, and a
    /// receipt built from a response rather than from an observation is the thing
    /// step 8 exists to prevent. [`EnsurePullRequest::apply`](crate::github::EnsurePullRequest)
    /// discards its response for the same reason. So a create that answered
    /// without a comment id fails here, the failure classifies
    /// [`Unknown`](crate::effect::EffectOutcome::Unknown), and step 8's read is
    /// what decides what actually happened — which is the same route a lost answer
    /// takes and needs no rule of its own.
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

/// Where a question reaches a person, and where their replies are read from.
///
/// Two methods, because a decision channel has exactly two directions. Every
/// caller of either reaches an [`EffectContext`], so nothing here holds a
/// credential of its own: the one `gh` construction stays the one `gh`
/// construction.
///
/// # `request` takes the operation beside the envelope, and the contract is what
/// is wrong
///
/// Design §6.4 and the epic's `## Contracts` pin
/// `request(&self, ctx, authorized: &AuthorizedEffect<PublishDecisionRequest>)`.
/// **That signature cannot be implemented.** [`AuthorizedEffect`] holds its
/// operation in a private field and exposes
/// [`effect_id`](AuthorizedEffect::effect_id) and
/// [`payload_hash`](AuthorizedEffect::payload_hash) and nothing else, so a port
/// handed only the envelope has the identity, the digest, and no body to post.
///
/// The fix is not to widen [`AuthorizedEffect`]. That type's entire surface is a
/// private constructor and two accessors, and adding an `operation()` accessor to
/// a capability token so that a signature written without noticing the privacy
/// can compile is the wrong direction — the narrowness is the property. So this
/// takes the shape [`PublishDecisionRequest::apply`] already calls with, where
/// `&self` is the operation and the envelope arrives beside it: the port adopts
/// the executor's own convention instead of inventing one, and it has a real
/// production caller, which is the whole point of a port. §6.4's signature is
/// recorded as a design defect rather than met.
///
/// # Why the error is [`GhError`] and not an `InteractionError`
///
/// Because there is one channel. An `InteractionError` today would be an enum
/// with one GitHub-shaped variant, which is the reasoning that already gave
/// [`InteractionRef`] exactly one variant on purpose: a variant nothing
/// constructs is inert surface, and adding one later is a line of code while
/// removing one consumers have matched on is not. When a second channel exists
/// the error type earns its existence.
#[async_trait::async_trait]
pub trait HumanInteractionPort: Send + Sync {
    /// Publish one question, and say where it went.
    ///
    /// `authorized` is required rather than convenient: it is the proof that
    /// identity, payload and policy were checked for this exact request, and a
    /// port that could be called without one would be a second way to change the
    /// world.
    async fn request(
        &self,
        ctx: &EffectContext,
        request: &PublishDecisionRequest,
        authorized: &AuthorizedEffect<PublishDecisionRequest>,
    ) -> Result<InteractionRef, GhError>;

    /// Every reply on that conversation, oldest first.
    ///
    /// No envelope, because reading changes nothing. What it returns is the whole
    /// conversation and not the replies this port judges relevant: which comments
    /// are candidate answers is [`validate`]'s decision, made against a run and an
    /// allowlist that a transport has no business knowing about.
    async fn responses(
        &self,
        ctx: &EffectContext,
        interaction: &InteractionRef,
    ) -> Result<Vec<HumanResponse>, GhError>;
}

/// The one implementation: a pull request's conversation, through the run's own
/// `gh`.
///
/// A unit struct rather than something holding a client, because the client is in
/// the [`EffectContext`] every method already takes. Two ways to reach `gh` would
/// be two environments and two credentials to keep in step, which is the thing
/// `github` is one module for.
pub struct GitHubConversation;

#[async_trait::async_trait]
impl HumanInteractionPort for GitHubConversation {
    /// One `POST`, and the comment it created.
    ///
    /// The id comes off the create's own response, which is the only place it
    /// exists at this moment — and it is checked rather than defaulted, for
    /// [`EnsurePullRequest::inspect`](crate::github::EnsurePullRequest)'s reason:
    /// a `0` standing in for an id nobody sent would name a comment that is not
    /// this question. What makes reading it safe is that no decision rests on it;
    /// see [`PublishDecisionRequest::apply`].
    ///
    /// **That refusal is untested, and cannot be tested today.** It is invisible
    /// through the executor — `apply` discards this value and step 8 reads the
    /// world back whichever way the mutation reported — and it is unreachable from
    /// a test directly, because [`AuthorizedEffect`] has no public constructor, so
    /// nothing outside this crate's own walk can call this method. Recorded here
    /// rather than left to look covered; `a_create_that_answers_without_a_comment_id_is_settled_by_the_read`
    /// says the same thing from the other side.
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

    /// [`read_conversation`], and nothing added.
    ///
    /// Bounded at `CONVERSATION_PAGES`, so a conversation longer than that is
    /// refused rather than truncated: "nobody has answered" and "I read as much as
    /// I was allowed" are different facts, and only the first is one to act on.
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

    /// The rendering is the contract, not an incidental of `Debug`. A reader
    /// holding this string can open the pull request and find the comment
    /// without being told the shape of a GitHub URL.
    #[test]
    fn a_conversation_renders_as_the_repository_the_pull_request_and_the_comment() {
        assert_eq!(
            conversation().to_string(),
            "peel/fiddle-effects-acceptance#4 comment 2147483647"
        );
    }

    /// Every component is present, checked separately, so a rendering that
    /// dropped one — the comment id being the easy one to lose, since the pull
    /// request alone looks like enough — fails here rather than in an operator's
    /// hands.
    #[test]
    fn no_component_of_the_conversation_is_dropped() {
        let rendered = conversation().to_string();
        for part in ["peel/fiddle-effects-acceptance", "#4", "2147483647"] {
            assert!(rendered.contains(part), "{part} is missing from {rendered}");
        }
    }
}
