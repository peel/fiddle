//! One pull request, identified by the pair that actually identifies one.
//!
//! This is the second [`IntegrationOperation`], and where [`refs`](super::refs)
//! rests on `git push` being idempotent, this one rests on GitHub already
//! refusing the duplicate. Both leave the same work to be done here, and it is
//! *interpretation* rather than machinery: what the refusal means, and what the
//! object is looked up by.
//!
//! # The identity is head and base, and never the title
//!
//! release-please's own documentation records what a title-parsing identity
//! costs: change the title format and a second pull request opens, because the
//! existing one stops being recognised as the same thing. So the title and the
//! body are *payload* here. They belong in [`EnsurePullRequest::payload`], where
//! a change to either is detectable against an unchanged identity, and nowhere
//! near [`EnsurePullRequest::lookup_path`].
//!
//! The head must be owner-qualified — `head=owner:branch` — because the
//! parameter is matched against a head *label*, and an unqualified branch name
//! matches across repositories. The same spelling is sent to the create, so
//! there is one head in this module rather than two that could disagree.
//!
//! # A 422 is GitHub preventing the duplicate, not an error
//!
//! Creating a pull request for a head and base that already has an open one
//! answers **422**. That is the mechanism working: the duplicate this milestone
//! exists to prevent was prevented, by the one participant that can see both
//! requests. A client that reported it as a failure would be reporting a failure
//! that did not happen — and worse, would send its caller to retry.
//!
//! Nothing in this module resolves it, and that is the design. [`GhError::Http`]
//! with a 422 classifies [`EffectOutcome::Unknown`](crate::effect::EffectOutcome),
//! which forces the executor's step 8 into the postcondition read below, and the
//! read is what decides: exactly one open pull request for this head and base is
//! the postcondition holding, and none is a validation failure that stays one.
//!
//! **No branch here matches GitHub's prose.** A message is not a contract — it
//! can be reworded without notice, and it is localized, and a `contains("already
//! exists")` would be a client whose correctness depends on English. The
//! classification comes from the status code and from what the read finds, which
//! are the two things GitHub has committed to.

use crate::effect::{AuthorizedEffect, EffectContext, IntegrationOperation, ObservedState};
use crate::github::{encode, GhError};
use fiddle_core::HumanDecisionRequirement;

/// An open pull request, as it was observed to be.
///
/// The `number` is the external reference a later process looks the object up
/// by. The `title` is carried because a receipt is read by a person and the
/// title is what they will recognise — never because anything decides anything
/// from it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PullRequest {
    pub number: u64,
    /// Owner-qualified, exactly as the lookup asked for it.
    pub head: String,
    pub base: String,
    pub title: String,
}

impl ObservedState for PullRequest {
    type Value = PullRequest;

    fn describe(&self) -> String {
        format!(
            "pull request #{} from {} into {} ({:?})",
            self.number, self.head, self.base, self.title
        )
    }

    fn reference(&self) -> Option<String> {
        Some(self.number.to_string())
    }

    fn into_value(self) -> PullRequest {
        self
    }
}

/// The canonical target identity for a pull request effect.
///
/// GitHub's own compare spelling, `base...head`, and written here rather than at
/// each call site because it is hashed into the effect identity: two spellings
/// of the same target would be two effects, and a fresh process would fail to
/// recognise work it had really done.
///
/// What is *not* in it is the load-bearing half. No title, no body — so a run
/// that reworded either proposes the same effect against the same target, and
/// the difference shows up in the payload hash where it can be seen rather than
/// in the identity where it would silently open a second pull request.
pub fn pull_request_target(repo: &str, head_owner: &str, head: &str, base: &str) -> String {
    format!("{repo}/pulls/{base}...{head_owner}:{head}")
}

/// Open the pull request for this run's branch, or recognise the one that is
/// already open for it.
pub struct EnsurePullRequest {
    /// `owner/name`, as the API path spells it.
    repo: String,
    /// The owner the head branch lives under. Separate from `repo`'s owner
    /// because a head may come from a fork, and because the label is what the
    /// lookup matches on.
    head_owner: String,
    /// The head branch, without `refs/heads/`.
    head: String,
    /// The branch being merged into.
    base: String,
    /// Payload. Read by people, hashed for detectability, never matched on.
    title: String,
    /// Payload, as above.
    body: String,
    /// Open it as a draft.
    ///
    /// Payload rather than identity: a draft is the same proposal for the same
    /// head and base, and what distinguishes it is what the request asks for
    /// rather than which object it is about. The transition out of draft is the
    /// gated act — it is the moment the change enters a review queue — and this
    /// field is only the state it starts in.
    draft: bool,
}

impl EnsurePullRequest {
    pub fn new(
        repo: String,
        head_owner: String,
        head: String,
        base: String,
        title: String,
        body: String,
        draft: bool,
    ) -> Self {
        Self {
            repo,
            head_owner,
            head,
            base,
            title,
            body,
            draft,
        }
    }

    /// The `draft` key, present only when this run is drafting.
    ///
    /// Written once and rendered by both the canonical payload and the create,
    /// because a payload that claimed a key the request never sent would be a
    /// record of something that did not happen.
    ///
    /// Absent rather than `false` when not drafting, and that is the load-bearing
    /// half. The API omits the parameter when it is not asked for, so omitting is
    /// the honest rendering — and because [`serde_json::Map`] is sorted, an
    /// omitted key moves no other byte, which leaves every payload written before
    /// this field existed hashing exactly as it did.
    fn draft_key(&self) -> Option<(String, serde_json::Value)> {
        self.draft
            .then(|| ("draft".to_string(), serde_json::Value::Bool(true)))
    }

    /// The canonical target identity to propose this effect under.
    pub fn target(&self) -> String {
        pull_request_target(&self.repo, &self.head_owner, &self.head, &self.base)
    }

    /// `owner:branch` — the one spelling of the head in this module.
    ///
    /// Both the lookup and the create use it, because a lookup that qualified
    /// the head and a create that did not would be asking about one thing and
    /// making another.
    fn head_label(&self) -> String {
        format!("{}:{}", self.head_owner, self.head)
    }

    /// The list read that locates the pull request.
    ///
    /// Three parameters and no fourth. `head` owner-qualified, or the query
    /// matches a branch of that name in any repository; `base`, because the same
    /// branch may legitimately be proposed into two of them; and `state=open`,
    /// because a closed pull request is not the postcondition — the work is not
    /// proposed any more — and treating one as satisfaction would leave a run
    /// silently doing nothing.
    fn lookup_path(&self) -> String {
        format!(
            "/repos/{}/pulls?head={}&base={}&state=open",
            self.repo,
            encode(&self.head_label()),
            encode(&self.base),
        )
    }

    /// Read one listed pull request, checking it is the one that was asked for.
    ///
    /// The check is not ceremony. This value becomes the receipt's
    /// `external_ref`, and after a 422 it is the *entire* basis for calling the
    /// effect committed — so settling on an object whose head or base is not the
    /// one this operation is about would report somebody else's pull request as
    /// this run's work. The filtering is GitHub's; confirming that what came back
    /// is what was asked for is this client's.
    fn read(&self, listed: &serde_json::Value) -> Result<PullRequest, GhError> {
        let field = |value: Option<&str>, name: &str| {
            value.map(str::to_string).ok_or_else(|| {
                GhError::Malformed(format!("a listed pull request carried no {name}"))
            })
        };
        let number = listed["number"].as_u64().ok_or_else(|| {
            GhError::Malformed("a listed pull request carried no number".to_string())
        })?;
        let head = field(listed["head"]["label"].as_str(), "head label")?;
        let base = field(listed["base"]["ref"].as_str(), "base ref")?;

        if head != self.head_label() || base != self.base {
            return Err(GhError::Malformed(format!(
                "asked for {}...{} and was answered #{number}, which is {}...{}",
                self.base,
                self.head_label(),
                base,
                head
            )));
        }

        Ok(PullRequest {
            number,
            head,
            base,
            // Payload, so its absence is not a reason to refuse the object: a
            // pull request with no title is still this run's pull request.
            title: listed["title"].as_str().unwrap_or_default().to_string(),
        })
    }
}

#[async_trait::async_trait]
impl IntegrationOperation for EnsurePullRequest {
    type State = PullRequest;

    /// Unattended.
    ///
    /// A pull request is a *proposal*: it moves no branch and merges nothing,
    /// and the review it asks for is the human decision. Deployment may still
    /// strengthen this and can never weaken it — [`combine`](fiddle_core::combine)'s
    /// rule, not this method's.
    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    /// The canonical payload: the whole request, order-stable.
    ///
    /// The title and body are in here and in nothing else that decides
    /// anything. That is what makes a reworded title *detectable* — the payload
    /// hash moves — without making it a second pull request. What compares it is
    /// the executor's step 6, against the payload the proposal named: a caller
    /// that proposed one title and built this operation with another is refused
    /// before the create.
    ///
    /// What it is **not** compared against is the pull request already out there.
    /// The list read carries a title but no body, so the observed object cannot
    /// reconstruct this payload; a run that finds an open pull request for its
    /// head and base settles on it whatever it is titled, which is
    /// [`EnsurePullRequest::inspect`](IntegrationOperation::inspect)'s deliberate
    /// position and not an oversight here.
    ///
    /// The `draft` key is here only when it is asked for — see
    /// [`draft_key`](EnsurePullRequest::draft_key) for why the absent spelling is
    /// both the honest one and the one that leaves earlier payloads untouched.
    fn payload(&self) -> String {
        let mut request = serde_json::Map::from_iter([
            ("base".to_string(), self.base.clone().into()),
            ("body".to_string(), self.body.clone().into()),
            ("head".to_string(), self.head_label().into()),
            ("repo".to_string(), self.repo.clone().into()),
            ("title".to_string(), self.title.clone().into()),
        ]);
        request.extend(self.draft_key());
        serde_json::Value::Object(request).to_string()
    }

    /// Is there already an open pull request for this head and base?
    ///
    /// Called twice by the executor — before the create to find out whether it
    /// is needed, and after it to find out whether it happened, which is the
    /// call that resolves the 422.
    ///
    /// Note what is *not* here: no arm turns a failed read into an absence. The
    /// list endpoint answers `200` with `[]` when there is nothing, so an error
    /// is the repository being unreadable rather than the pull request being
    /// missing — and reading an outage as "no pull request" is precisely how the
    /// second one gets opened. That is the same rule
    /// [`refs`](super::refs) applies to a 404, arriving at the opposite
    /// treatment because the endpoint says absence differently.
    ///
    /// # And note what `draft` is not: part of the postcondition
    ///
    /// The match is on head, base and `state=open`, and deliberately not on
    /// `draft`. So a pull request **a person has already marked ready for review**
    /// satisfies this postcondition, and no re-draft happens. That is the intended
    /// rule and not a consequence of the list read being thin:
    ///
    /// - the effect is *a pull request exists for this head and base*; `draft` is a
    ///   property of **creation**, not of the postcondition;
    /// - re-drafting one somebody had readied would walk back a human action
    ///   because fiddle's own record was lost, which is the opposite of what a
    ///   decision walk exists to do.
    ///
    /// [`EnsurePullRequestReady::inspect`](super::ready::EnsurePullRequestReady)
    /// states the agreeing half in full, and states it from the other side: an
    /// already-ready pull request is *its* postcondition too, so the two operations
    /// never fight over the same object. The cross-reference is here because a
    /// reader of the drafting side looks here first, and until it existed the rule
    /// was written down only in the sibling module.
    ///
    /// `propose_capability::a_readied_pull_request_is_not_re_drafted` is the test
    /// that pins it. Before that test the rule held only because
    /// `fiddle-runtime`'s `PullRequest` carries no `draft` field for this arm to
    /// consult — which is an accident a later bean could have reversed with nothing
    /// objecting.
    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<PullRequest>, GhError> {
        let response = ctx
            .gh
            .api("GET", &self.lookup_path(), None, &ctx.cancel)
            .await?;

        // Checked rather than defaulted. A 200 whose body is not a list is a
        // `gh` answering something this client cannot read, and defaulting it to
        // empty would turn that into "no pull request" and open one.
        let listed = response.body.as_array().ok_or_else(|| {
            GhError::Malformed(format!(
                "{} answered {} with something that is not a list",
                self.lookup_path(),
                response.status
            ))
        })?;

        match listed.as_slice() {
            [] => Ok(None),
            [one] => self.read(one).map(Some),
            // Two open pull requests for one head and base is the state this
            // milestone exists to prevent, and it is reported rather than
            // resolved by picking one. GitHub will not create the second, so
            // arriving here means something outside this system did — which a
            // person needs to know about, not have chosen between silently.
            many => Err(GhError::Duplicate { count: many.len() }),
        }
    }

    /// One `POST /repos/{repo}/pulls`, and the only line here that changes
    /// anything.
    ///
    /// The response is deliberately discarded, number and all. It is what GitHub
    /// said, and the executor's next act is to read the world back; a receipt
    /// built from this value would be a receipt for a response rather than for
    /// an observation, which is the thing step 8 exists to prevent — and in this
    /// operation there may be no response at all, because the ordinary
    /// duplicate-prevention answer is a refusal.
    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), GhError> {
        let mut body = serde_json::Map::from_iter([
            ("title".to_string(), self.title.clone().into()),
            ("body".to_string(), self.body.clone().into()),
            ("head".to_string(), self.head_label().into()),
            ("base".to_string(), self.base.clone().into()),
        ]);
        body.extend(self.draft_key());
        ctx.gh
            .api(
                "POST",
                &format!("/repos/{}/pulls", self.repo),
                Some(&serde_json::Value::Object(body)),
                &ctx.cancel,
            )
            .await
            .map(|_response| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ensure(title: &str, base: &str) -> EnsurePullRequest {
        EnsurePullRequest::new(
            "peel/fiddle".to_string(),
            "peel".to_string(),
            "fiddle/abc".to_string(),
            base.to_string(),
            title.to_string(),
            "the body".to_string(),
            false,
        )
    }

    /// The target is what the identity is derived over, so its spelling is a
    /// contract and not a formatting choice — and what it leaves out is the
    /// contract that matters.
    #[test]
    fn the_target_is_the_head_and_base_and_carries_no_title() {
        assert_eq!(
            ensure("a title", "main").target(),
            "peel/fiddle/pulls/main...peel:fiddle/abc"
        );
        assert!(!ensure("a title", "main").target().contains("a title"));
        // The base is part of it: the same branch proposed into two bases is two
        // pull requests, and one identity for both would recognise the wrong one.
        assert_ne!(
            ensure("a title", "main").target(),
            ensure("a title", "release").target()
        );
    }

    /// Everything outside the unreserved set, including the two characters a
    /// head label is made of.
    #[test]
    fn a_query_value_is_percent_encoded() {
        assert_eq!(encode("peel:fiddle/abc"), "peel%3Afiddle%2Fabc");
        assert_eq!(encode("a b&c=d"), "a%20b%26c%3Dd");
        assert_eq!(encode("-._~AZaz09"), "-._~AZaz09");
    }
}
