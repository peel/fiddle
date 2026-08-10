//! The draft-to-ready transition, at one revision and no other.
//!
//! This is the third [`IntegrationOperation`] against a pull request and the
//! first operation anywhere in this build whose own minimum requires a person.
//! [`refs`](super::refs) publishes a branch and [`pulls`](super::pulls) opens a
//! proposal; both are unattended, because neither asks anybody for anything.
//! Making a pull request ready is the moment the change enters a review queue,
//! so it is the moment a person has to have agreed to — and
//! [`EnsurePullRequestReady::minimum`] is where that is declared rather than
//! left to a deployment document that could be relaxed under time pressure.
//!
//! # The revision is in the target, not only in the payload
//!
//! The target is `{repo}#{pr}@{head_sha}` rather than `{repo}#{pr}`. What is
//! being acted on is *this pull request at this revision*, and under M2's rule
//! the target is what makes two proposals the same effect — so a head that
//! moved derives a different [`EffectId`](fiddle_core::EffectId), which derives
//! a different [`DecisionRequestId`](fiddle_core::DecisionRequestId), which is a
//! different question.
//!
//! That difference is the whole of the staleness property, and it is stronger
//! than the one a payload could carry. An approval given for the earlier
//! revision is not a *rejected* answer to the new question; it is not an answer
//! to it at all, because the new question was never asked of it. Put the sha in
//! [`EnsurePullRequestReady::payload`] alone and the identity would be unchanged,
//! so the stale approval would arrive looking like an answer and would be refused
//! for the wrong reason — a payload divergence, which reads as a caller sending
//! something other than what it proposed rather than as a person having approved
//! something that has since changed.
//!
//! # The transition is GraphQL, because `draft` is not a REST field
//!
//! `PATCH /repos/{owner}/{repo}/pulls/{number}` accepts `title`, `body`,
//! `state`, `base` and `maintainer_can_modify`. Sending `draft` there answers
//! **200 OK** and moves nothing, which is worse than a refusal: a REST
//! implementation would believe a success that never happened.
//! [ADR 018](../../../docs/technical/decisions/018-a-graphql-200-is-not-a-success.md)
//! records the measurement and `scripts/verify-graphql-ready.sh` is the
//! transcript. The transition exists only as `markPullRequestReadyForReview`,
//! whose input is a node id rather than a number, and whose refusals arrive as
//! 200 with an `errors[]` — which is what [`GhCli::graphql`](super::GhCli::graphql)
//! exists to classify.
//!
//! # One read answers both of this operation's questions
//!
//! `GET /repos/{repo}/pulls/{pr}` returns `draft` and `node_id` together, so the
//! read the executor was going to make at step 3 anyway supplies the input the
//! mutation needs. [`EnsurePullRequestReady::apply`] therefore fetches nothing:
//! an `apply` that could fetch could fetch a *different* pull request, and the
//! one thing this operation must not do is make a change somebody approved
//! somewhere they did not approve it. With no node id in hand it refuses with
//! [`GhError::NotSent`], which is `NotCommitted` — nothing left this process, so
//! there is no postcondition owed.

use crate::effect::{AuthorizedEffect, EffectContext, IntegrationOperation, ObservedState};
use crate::github::GhError;
use fiddle_core::HumanDecisionRequirement;
use std::sync::OnceLock;

/// The mutation, parameterised, with the node id bound as `$id`.
///
/// The node id is a value GitHub chose and this process passes on. Interpolated
/// into the query text it could rewrite the query it appears in, so it goes out
/// as a variable — `-f id=…`, which `gh` binds as a GraphQL variable and not as
/// a form field, measured against real GitHub by step 0 of
/// `scripts/verify-graphql-ready.sh`.
///
/// `pullRequest { isDraft }` is selected because a mutation has to select
/// something, and not because anything here reads it. What the mutation says
/// about itself is a claim; the executor's step 8 reads the pull request back,
/// exactly as it does for every other operation.
const READY_FOR_REVIEW: &str = "mutation($id: ID!) { markPullRequestReadyForReview(input: \
                                {pullRequestId: $id}) { pullRequest { isDraft } } }";

/// A pull request observed to be out of draft.
///
/// The `node_id` is carried because it is what was read, and because a receipt
/// that named the object only by number would leave the one identifier the
/// mutation uses out of the record of what was done to it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadyPullRequest {
    pub number: u64,
    pub node_id: String,
}

impl ObservedState for ReadyPullRequest {
    type Value = ReadyPullRequest;

    fn describe(&self) -> String {
        format!("pull request #{} is ready for review", self.number)
    }

    /// The number, because that is what a person and a later process both look
    /// the object up by. The node id is the mutation's business.
    fn reference(&self) -> Option<String> {
        Some(self.number.to_string())
    }

    fn into_value(self) -> ReadyPullRequest {
        self
    }
}

/// The canonical target identity for making one pull request ready.
///
/// Written here rather than at each call site for the reason
/// [`pull_request_target`](super::pull_request_target) is: it is hashed into the
/// effect identity, and two spellings of one target would be two effects, so a
/// process recomputing the identity from the same three facts has to arrive at
/// the same string.
///
/// The head sha is the third fact and the load-bearing one. Everything in
/// [`ready`](self)'s module documentation about staleness is a consequence of it
/// being in here.
pub fn pull_request_ready_target(repo: &str, pr: u64, head_sha: &str) -> String {
    format!("{repo}#{pr}@{head_sha}")
}

/// Take one pull request out of draft, at the revision the decision was about.
pub struct EnsurePullRequestReady {
    /// `owner/name`, as the API path spells it.
    repo: String,
    /// The pull request's number, which is what the REST read is addressed by.
    pr: u64,
    /// The head this transition is about. Identity, not payload — see the module
    /// documentation.
    head_sha: String,
    /// The node id, written by [`EnsurePullRequestReady::inspect`] and read by
    /// [`EnsurePullRequestReady::apply`].
    ///
    /// The one piece of state inside an operation in this crate, and it is here
    /// so that `apply` has no reason to read. It is written at most once: the
    /// executor calls `inspect` twice and both reads address the same path, so a
    /// second answer is the same object, and keeping the first means the value
    /// the mutation used is the value the pre-mutation read produced.
    ///
    /// [`OnceLock`] rather than a `Cell`, because [`IntegrationOperation`]
    /// requires `Sync` and the executor holds the operation across an await.
    node_id: OnceLock<String>,
}

impl EnsurePullRequestReady {
    pub fn new(repo: String, pr: u64, head_sha: String) -> Self {
        Self {
            repo,
            pr,
            head_sha,
            node_id: OnceLock::new(),
        }
    }

    /// The canonical target identity to propose this effect under.
    pub fn target(&self) -> String {
        pull_request_ready_target(&self.repo, self.pr, &self.head_sha)
    }

    /// The one read this operation makes, addressed by number.
    fn lookup_path(&self) -> String {
        format!("/repos/{}/pulls/{}", self.repo, self.pr)
    }

    /// Read the two fields this operation needs out of one pull request.
    ///
    /// Both are checked rather than defaulted, and the second is checked even
    /// on the path that will not use it. A response missing `draft` is a `gh`
    /// answering something this client cannot read, and defaulting it either way
    /// would turn that into a verdict — "still a draft", which mutates, or
    /// "already ready", which reports work nobody did. A response missing
    /// `node_id` is the same answer with the mutation's only input absent, and
    /// accepting it would leave the failure to surface at `apply`, one step
    /// after the read that could have explained it.
    fn read(&self, body: &serde_json::Value) -> Result<(bool, String), GhError> {
        let draft = body["draft"].as_bool().ok_or_else(|| {
            GhError::Malformed(format!("{} carried no draft state", self.lookup_path()))
        })?;
        let node_id = body["node_id"].as_str().ok_or_else(|| {
            GhError::Malformed(format!("{} carried no node id", self.lookup_path()))
        })?;
        Ok((draft, node_id.to_string()))
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for EnsurePullRequestReady {
    type State = ReadyPullRequest;

    /// A person, whatever the document says.
    ///
    /// The first `Human` in this build. Publishing a branch and opening a draft
    /// are preparation — they merge nothing and ask nobody — while this is the
    /// act that puts a change in front of reviewers, and fiddle is not entitled
    /// to decide that on its own. [`combine`](fiddle_core::combine) is what makes
    /// the declaration stick: a deployment may strengthen it to `Deny` and has no
    /// spelling that weakens it.
    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Human
    }

    /// The canonical payload: the three facts this operation acts on.
    ///
    /// The head sha is in here as well as in the target, and that is not a
    /// duplication to remove. The target is what the identity is derived over
    /// and the payload is what step 6 checks the approval was minted for, so a
    /// caller that proposed one revision and built the operation with another is
    /// refused before the mutation rather than only being a different effect.
    ///
    /// [`serde_json::Map`] is sorted, so the rendering is order-stable whatever
    /// order the keys are written in here.
    fn payload(&self) -> String {
        serde_json::Value::Object(serde_json::Map::from_iter([
            ("head".to_string(), self.head_sha.clone().into()),
            ("pr".to_string(), self.pr.into()),
            ("repo".to_string(), self.repo.clone().into()),
        ]))
        .to_string()
    }

    /// Is this pull request already out of draft?
    ///
    /// Called twice by the executor, before the mutation and after it, and both
    /// calls do the same thing: read the pull request and believe what it says.
    ///
    /// An already-ready pull request is the postcondition, and that agrees with
    /// what [`pulls`](super::pulls) decided in the other direction — a drafting
    /// run treats a readied pull request as satisfying *its* postcondition too,
    /// because re-drafting one a person had readied would undo human progress.
    /// The two operations therefore never fight over the same object.
    ///
    /// No arm turns a failed read into an absence. A pull request addressed by
    /// number either exists or answers 404, so an error here is a repository
    /// this process cannot read rather than a draft — and reading an outage as
    /// "still a draft" is how a mutation gets dispatched at a revision nobody
    /// looked at.
    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<ReadyPullRequest>, GhError> {
        let response = ctx
            .gh
            .api("GET", &self.lookup_path(), None, &ctx.cancel)
            .await?;
        let (draft, node_id) = self.read(&response.body)?;

        // Written on both paths, because the read is the same read either way
        // and the handoff is about where the value came from rather than about
        // which answer it accompanied. Already set is the step 8 call arriving;
        // the first value stands.
        let _ = self.node_id.set(node_id.clone());

        match draft {
            true => Ok(None),
            false => Ok(Some(ReadyPullRequest {
                number: self.pr,
                node_id,
            })),
        }
    }

    /// One `markPullRequestReadyForReview`, and the only line here that changes
    /// anything.
    ///
    /// The node id comes from [`EnsurePullRequestReady::inspect`] and from
    /// nowhere else. An empty cell is refused rather than repaired by fetching:
    /// this operation is spending an approval a person gave for one pull request
    /// at one revision, and a fetch inside `apply` is a second chance to decide
    /// which object that was.
    ///
    /// The response is discarded, `data` and all. It is what GitHub said about
    /// its own mutation, and the executor's next act is to read the pull request
    /// back — which is the answer, because a refused mutation arrives as a 200
    /// and a lost one arrives as nothing at all.
    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        let node_id = self.node_id.get().ok_or_else(|| {
            GhError::NotSent(format!(
                "the node id of {} was not read before the mutation",
                self.target()
            ))
        })?;

        ctx.gh
            .graphql(READY_FOR_REVIEW, &[("id", node_id)], &ctx.cancel)
            .await
            .map(|_data| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::payload_hash;

    fn ready_at(head_sha: &str) -> EnsurePullRequestReady {
        EnsurePullRequestReady::new("acme/r".to_string(), 7, head_sha.to_string())
    }

    /// The query is what makes the node id a value rather than syntax.
    #[test]
    fn the_mutation_names_its_input_as_a_variable() {
        assert!(READY_FOR_REVIEW.contains("markPullRequestReadyForReview"));
        assert!(READY_FOR_REVIEW.contains("$id"));
        assert!(READY_FOR_REVIEW.contains("mutation($id: ID!)"));
    }

    /// A read that answered half the question is not an answer this operation
    /// can act on, and the half it will not need yet is checked too.
    #[test]
    fn a_pull_request_missing_either_field_is_not_read() {
        let ready = ready_at("aaaa");
        assert!(ready
            .read(&serde_json::json!({"node_id": "PR_abc"}))
            .is_err());
        assert!(ready.read(&serde_json::json!({"draft": true})).is_err());
        assert_eq!(
            ready
                .read(&serde_json::json!({"draft": false, "node_id": "PR_abc"}))
                .unwrap(),
            (false, "PR_abc".to_string())
        );
    }

    /// The read is addressed by number and asks for nothing else.
    #[test]
    fn the_read_addresses_one_pull_request() {
        assert_eq!(ready_at("aaaa").lookup_path(), "/repos/acme/r/pulls/7");
    }

    /// The head sha moves the payload as well as the target, and that is a
    /// second property rather than a restatement of the first. The target makes
    /// two revisions two effects; the payload is what makes a caller that
    /// proposed one revision and built the operation with another refusable at
    /// step 6, against an identity that would otherwise agree.
    ///
    /// `ready_effect.rs` owns the identity half, where it is stated against the
    /// public surface a proposal is built from.
    #[test]
    fn a_moved_head_moves_the_payload_that_step_six_checks() {
        assert_ne!(
            payload_hash(&ready_at("aaaa").payload()),
            payload_hash(&ready_at("bbbb").payload())
        );
        assert_eq!(
            payload_hash(&ready_at("aaaa").payload()),
            payload_hash(&ready_at("aaaa").payload()),
            "and the payload is canonical: the same request hashes the same"
        );
    }
}
