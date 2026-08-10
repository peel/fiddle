//! Publish one change: a branch, a pull request, and a requested check.
//!
//! This capability **proposes**; it never mutates. It holds no credential and
//! constructs no client — what it receives is an
//! [`Executor`](crate::effect::Executor) already bound to its own
//! [`CapabilityId`], so it cannot propose an effect in another capability's
//! name, and the authorization envelope it never constructs is what proves the
//! checks happened. The same arrangement [`crate::gateway`] makes for the model
//! credential, made here for the forge's.
//!
//! Everything before this module built parts. This is where they become a
//! capability, and three things follow from that which are worth stating.
//!
//! # One capability per run, three effects inside it
//!
//! `capability_executions` carries exactly one entry and `progress` exactly one
//! stage, whatever happens out there. ADR 013 priced the alternative and
//! deferred it: every existing consumer of a bundle has seen that shape, and a
//! run that composed two capability executions to publish a branch and a pull
//! request would be changing a published contract to describe an implementation
//! detail. The three effects are *inside* one execution, and each one's receipt
//! reaches the bundle as evidence.
//!
//! # The identity comes from the executor, and from nowhere else
//!
//! [`EnsureCheckRequested::new`] computes this effect's identity itself, because
//! its lookup happens at the executor's step 3 — *before* step 6 mints the
//! envelope. A caller that built the operation from one `(project,
//! invocation_ref)` pair and the executor from another would produce a workflow
//! run **named** by one identity and **looked up** by the other: every attempt
//! would find nothing and dispatch again, without bound. That is the exact
//! failure this milestone exists to prevent, and the guard in
//! [`EnsureCheckRequested::apply`] is a backstop rather than the arrangement.
//!
//! The arrangement is here: this capability reads the pair off the executor it
//! was handed — [`Executor::project`] and [`Executor::invocation_ref`] — and
//! holds no copy of its own. There is therefore nothing to drift. The
//! `invocation_ref` the orchestration passes into [`Capability::execute`] is
//! *checked against* that pair and used for nothing else, so an executor built
//! for another run is refused rather than silently publishing under the wrong
//! name.
//!
//! # A refusal stops the sequence
//!
//! The three effects run in order and each one's failure returns. That is not
//! merely how `?` behaves; it is the property: a pull request that policy denies
//! must not be followed by a dispatched check, because the check would be
//! verifying work nothing has proposed. What was already published stands — a
//! branch is not unpublished by a later refusal, and pretending otherwise would
//! mean a second mutation on a failure path — and the receipts for the steps
//! that did complete still reach the bundle through [`Capability::receipts`].

use super::stub::write_atomically;
use super::{Capability, CapabilityError, ExecutionGrant};
use crate::effect::{EffectOutcome, EffectReceipt, Executor, IntegrationOperation, ObservedState};
use crate::github::{branch_name, EnsureBranchPublished, EnsureCheckRequested, EnsurePullRequest};
use fiddle_core::{
    correlation_key, CapabilityId, ChangeSetState, EffectKind, EvidenceRef, Observation,
    ProposedEffect, Publication, Published, ReviewState, SourceRef,
};
use std::path::PathBuf;
use std::sync::Mutex;

/// The origin this capability's earned evidence is named under.
const PUBLISH_ORIGIN: &str = "publish";

/// The state a pull request this run just observed is in.
///
/// A constant rather than a field read off the object, and that is a claim about
/// the read rather than an assumption: [`EnsurePullRequest`]'s lookup is
/// `state=open`, and a create answers with an open pull request. The only pull
/// request this operation can settle on is therefore an open one — a closed one
/// is not the postcondition, because the work is no longer proposed.
const OPEN: &str = "open";

/// Everything [`PublishChange`] needs that is not the executor.
///
/// One struct rather than nine constructor arguments, for the reason
/// [`RepairConfig`](super::RepairConfig) is one: every field is a deployment
/// decision an operator configures and none is derivable from the others.
///
/// **No credential is in here, and there is nowhere to put one.** The `gh` and
/// the `git` that carry one were resolved before the executor was built and live
/// behind it; what this struct holds is the description of *what* to publish.
pub struct PublishConfig {
    /// `owner/name`, as the API path spells it.
    pub repo: String,

    /// The owner the head branch lives under. Separate from `repo`'s owner
    /// because a head may come from a fork, and because the label is what the
    /// pull request lookup matches on.
    pub head_owner: String,

    /// The branch the pull request is opened against.
    pub base: String,

    /// The commit the branch must point at for the publication to hold.
    ///
    /// Supplied rather than read here, for the reason
    /// [`EnsureBranchPublished`] takes it rather than resolving `HEAD` itself:
    /// the commit being published is the attempt's business, and a capability
    /// that resolved it would be free to publish a commit its own proposal never
    /// named — with the payload hash still matching, because the payload would
    /// never have mentioned it.
    pub head_sha: String,

    /// The pull request's title. Payload: read by people, hashed for
    /// detectability, matched on by nothing.
    pub title: String,

    /// The pull request's body. Payload, as above.
    pub body: String,

    /// The workflow to request, spelled as the API path spells it — a file name
    /// or a numeric id.
    pub workflow: String,

    /// The check names a reader of the verification cares about, matched by
    /// name. A check nobody required is not consulted, and an unrelated green
    /// one satisfies nothing.
    pub required_checks: Vec<String>,

    /// The fixture root the change set is recorded under, which is where the
    /// next invocation's assessment looks for the marker.
    pub stub_root: PathBuf,

    /// The project half of the correlation key.
    ///
    /// Held even though [`Executor::project`] carries the same value, because
    /// the two are not the same *question*: this one is what the change-set
    /// marker is derived from, and [`Capability::execute`] refuses a
    /// configuration where the two differ rather than letting a run write a
    /// marker its own effects were not named after.
    pub project: String,
}

/// One change, published: a branch, a pull request, and a requested check.
///
/// Borrows its executor rather than owning one, and the lifetime is the design.
/// An owned executor would mean an owned [`EffectContext`](crate::effect::EffectContext),
/// and an owned `EffectContext` is a held credential — so the capability would
/// be back to carrying the thing this whole arrangement exists to keep on the
/// other side of a seam. What it holds instead is a borrow of something somebody
/// else built and bound.
pub struct PublishChange<'a> {
    executor: Executor<'a>,
    config: PublishConfig,
    /// What each completed effect left behind, appended as it happens.
    ///
    /// Held here rather than only on the success path so
    /// [`Capability::receipts`] can read it *after* an execution that failed —
    /// which is precisely when an operator needs to know what did reach the
    /// forge before it stopped.
    receipts: Mutex<Vec<EvidenceRef>>,
    /// What this run has established about the forge so far.
    observed: Mutex<Observed>,
    /// The pair the orchestration reads back after the execution, filled once
    /// the run has finished reaching the forge.
    publication: Mutex<Option<Publication>>,
}

/// The two external references a publication is described by, as they become
/// known.
///
/// `None` means *not observed*, never *not there*: a run that never got past the
/// branch has not read the forge and found no pull request, and the difference
/// is what keeps [`PublishChange::publication`] from publishing an
/// [`Observation::Available`] review it did not earn.
#[derive(Default)]
struct Observed {
    branch: Option<String>,
    head_sha: Option<String>,
    pull_request: Option<u64>,
    /// Why the forge could not be described, when it could not be.
    failure: Option<String>,
}

impl PublishConfig {
    /// Whether the project this configuration derives its marker from is the one
    /// the executor derives its effect identities from.
    fn project_agrees_with(&self, executor: &Executor<'_>) -> bool {
        self.project == executor.project()
    }
}

impl<'a> PublishChange<'a> {
    /// A capability that will publish `config` through `executor`.
    ///
    /// The executor is expected to be bound to
    /// [`fiddle_core::PUBLISH_CHANGE`]; one that is not is refused by the
    /// executor's own step 1 on the first proposal, which is the check that
    /// belongs to the executor rather than to its callers.
    pub fn new(executor: Executor<'a>, config: PublishConfig) -> Self {
        PublishChange {
            executor,
            config,
            receipts: Mutex::new(Vec::new()),
            observed: Mutex::new(Observed::default()),
            publication: Mutex::new(None),
        }
    }

    /// The branch this run publishes, recomputed the way a fresh process would.
    ///
    /// From the executor's own identity, so the name and the effect id it is
    /// derived from cannot come from two different pairs.
    fn branch(&self) -> String {
        branch_name(self.executor.project(), self.executor.invocation_ref())
    }

    /// Propose one effect, record its receipt as evidence, and hand back the
    /// observed value.
    ///
    /// Every effect goes through here so that "the receipt reaches the bundle"
    /// is one statement rather than three that could disagree — including on the
    /// arm where the *next* effect fails, which is the arm the ordering property
    /// is about.
    async fn propose<O>(
        &self,
        kind: EffectKind,
        target: String,
        payload: String,
        operation: O,
    ) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, CapabilityError>
    where
        O: IntegrationOperation,
    {
        let proposed = ProposedEffect {
            // Its own id, and not a parameter. A capability that could name the
            // proposing capability could propose in another's name; this one
            // cannot, and the executor refuses it at step 1 if it tries.
            capability: self.id(),
            kind,
            target,
            payload,
        };
        let receipt = self.executor.execute(proposed, operation).await?;
        self.receipts
            .lock()
            .unwrap()
            .push(receipt_evidence(kind, &receipt));
        Ok(receipt)
    }

    /// The three effects, in order, each one's receipt recorded before the next
    /// is proposed.
    async fn publish(&self, branch: &str) -> Result<u64, CapabilityError> {
        let repo = &self.config.repo;

        // 1. The branch. Nothing after this can be proposed without it: a pull
        //    request needs a head, and a workflow needs a ref.
        let publish_branch = EnsureBranchPublished::new(
            repo.clone(),
            branch.to_string(),
            self.config.head_sha.clone(),
        );
        let published = self
            .propose(
                EffectKind::EnsureBranchPublished,
                publish_branch.target(),
                publish_branch.payload(),
                publish_branch,
            )
            .await?;
        {
            let mut observed = self.observed.lock().unwrap();
            observed.branch = Some(published.value.branch.clone());
            // The sha the remote was *observed* to hold, never the one the push
            // reported — which is the whole reason the receipt carries it.
            observed.head_sha = Some(published.value.sha.clone());
        }

        // 2. The pull request, from that branch into the configured base.
        let open = EnsurePullRequest::new(
            repo.clone(),
            self.config.head_owner.clone(),
            branch.to_string(),
            self.config.base.clone(),
            self.config.title.clone(),
            self.config.body.clone(),
        );
        let opened = self
            .propose(
                EffectKind::EnsurePullRequest,
                open.target(),
                open.payload(),
                open,
            )
            .await?;
        self.observed.lock().unwrap().pull_request = Some(opened.value.number);

        // 3. The check, dispatched against the ref the branch effect published.
        //    Built from the executor's own identity — see this module's
        //    documentation for what a second copy of that pair would cost.
        let request = EnsureCheckRequested::new(
            repo.clone(),
            self.config.workflow.clone(),
            branch.to_string(),
            self.executor.project(),
            self.executor.invocation_ref(),
        );
        self.propose(
            EffectKind::EnsureCheckRequested,
            request.target(),
            request.payload(),
            request,
        )
        .await?;

        Ok(opened.value.number)
    }

    /// Record this invocation's correlation key as the change set for the work
    /// item.
    ///
    /// Deliberately identical to what [`StubMark`](super::StubMark) and
    /// [`FixtureRepair`](super::FixtureRepair) write, through the same atomic
    /// write: the assessment that reads it does not know or care which
    /// capability produced it, and three capabilities writing subtly different
    /// files for one reader is a defect waiting for a change of capability to
    /// expose it.
    ///
    /// Written **after** all three effects committed and on no other path. A
    /// marker says "this invocation accounts for this work", which the next
    /// invocation reads and completes on; a publication that stopped at the
    /// branch has not earned that claim.
    fn record_change_set(&self, work_id: &str) -> Result<(), CapabilityError> {
        let state = ChangeSetState {
            marker: Some(correlation_key(
                &self.config.project,
                self.executor.invocation_ref(),
            )),
        };
        let destination = self
            .config
            .stub_root
            .join(format!("changes/{work_id}.json"));
        write_atomically(&destination, &state).map_err(|source| CapabilityError::Write {
            path: destination.clone(),
            source,
        })
    }

    /// Read the checks at the head this run published, and pair the answer with
    /// what the forge was observed to hold.
    ///
    /// Reached through the executor, which is this capability's whole window on
    /// the outside world — see [`Executor::observe_checks`]. Called on both arms,
    /// because an execution that published a branch and then lost its pull
    /// request has still put a commit somewhere a reader can go and look at.
    async fn observe(&self) -> Publication {
        let source = || SourceRef(format!("github:{}", self.config.repo));
        let (branch, head_sha, pull_request, failure) = {
            let observed = self.observed.lock().unwrap();
            (
                observed.branch.clone(),
                observed.head_sha.clone(),
                observed.pull_request,
                observed.failure.clone(),
            )
        };

        // The `reason` an unobserved forge carries. Bounded here rather than
        // trusted, because it is rendered from an `EffectError` whose `Adapter`
        // arm quotes whatever came back from out there.
        let unreadable = |what: &str| {
            Published::of(match &failure {
                Some(why) => format!("{what}, so the forge was not read: {why}"),
                None => format!("{what}, so the forge was not read"),
            })
            .as_str()
            .to_string()
        };

        let review = match (&branch, head_sha.is_some()) {
            // The branch was read back, so the forge *was* read, and what it
            // holds for this run is what these fields say. `pull_request: None`
            // is then a real absence rather than an unasked question.
            (Some(branch), true) => Observation::Available {
                value: ReviewState {
                    branch: Some(branch.clone()),
                    pull_request,
                    // Only alongside a pull request. A state naming no object
                    // would be this capability describing nothing.
                    state: pull_request.map(|_| OPEN.to_string()),
                },
                source: source(),
                // The head the answer is about, so a consumer can tell whether
                // the world moved underneath it.
                revision: head_sha.clone(),
            },
            // Nothing was read back. `Unavailable` and never an `Available`
            // review with every field `None`: that would be the positive claim
            // *the forge was read and holds nothing*, which is exactly what a
            // run that never got an answer must not be able to make.
            _ => Observation::Unavailable {
                source: source(),
                reason: unreadable("no branch was observed"),
            },
        };

        let verification = match &head_sha {
            Some(head_sha) => {
                self.executor
                    .observe_checks(&self.config.repo, head_sha, &self.config.required_checks)
                    .await
            }
            // A check belongs to a commit, and this run has not established one.
            // Asking about a head it does not have would be asking about
            // somebody else's work.
            None => Observation::Unavailable {
                source: source(),
                reason: unreadable("no head was published"),
            },
        };

        Publication {
            review,
            verification,
        }
    }
}

#[async_trait::async_trait]
impl Capability for PublishChange<'_> {
    fn id(&self) -> CapabilityId {
        fiddle_core::PUBLISH_CHANGE
    }

    /// This capability's own word for its own step, beside M0's `mark` and M1's
    /// `repair`. There is no neutral stage name, which is why
    /// [`Capability::stage`] has no default.
    fn stage(&self) -> &'static str {
        "publish"
    }

    async fn execute(
        &self,
        grant: ExecutionGrant,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        if grant.capability_id() != self.id() {
            return Err(CapabilityError::NotAuthorised {
                granted: grant.capability_id(),
                requested: self.id(),
            });
        }
        // The executor was built for a run, and this is that run — or this
        // capability publishes under one name while the bundle, the journal and
        // the marker are filed under another. Checked before anything is
        // proposed, so a misbound executor provably reaches no forge.
        if invocation_ref != self.executor.invocation_ref()
            || !self.config.project_agrees_with(&self.executor)
        {
            return Err(CapabilityError::Misbound {
                bound: format!(
                    "{}/{}",
                    self.executor.project(),
                    self.executor.invocation_ref()
                ),
                asked: format!("{}/{invocation_ref}", self.config.project),
            });
        }

        let branch = self.branch();
        let published = self.publish(&branch).await;
        // Recorded before the forge is described, so an `Unavailable` review can
        // say *why* rather than only that it saw nothing.
        if let Err(error) = &published {
            self.observed.lock().unwrap().failure = Some(error.to_string());
        }
        // Read on both arms, and before the result is propagated: whatever the
        // run concluded, what reached the forge is what a reader has to be told.
        *self.publication.lock().unwrap() = Some(self.observe().await);
        let number = published?;

        self.record_change_set(work_id)?;
        Ok(EvidenceRef(format!(
            "{PUBLISH_ORIGIN}:{}/pull/{number}",
            self.config.repo
        )))
    }

    /// One evidence reference per effect that produced a receipt, in the order
    /// they were proposed.
    fn receipts(&self) -> Vec<EvidenceRef> {
        self.receipts.lock().unwrap().clone()
    }

    fn publication(&self) -> Option<Publication> {
        self.publication.lock().unwrap().clone()
    }
}

/// One effect receipt, as an evidence reference a bundle can carry.
///
/// # Why a rendered string rather than the receipt itself
///
/// [`EvidenceRef`] is a string and a bundle's evidence is a list of them, so a
/// structured receipt would have to be given a new home in the report schema —
/// and the schema is a published contract this task has no business widening.
/// The same argument [`super::repair`] makes for summarising its tool receipts,
/// reaching the same answer.
///
/// What it carries is what the criterion asks of it and what a reader would go
/// and check with: the **kind**, so it is clear which effect this was; the
/// **effect id**, which is the name the object was created under and the name a
/// fresh process recomputes to find it again; the **outcome**, because a
/// committed effect and one whose answer was lost are different news; the
/// **external reference**, which is the sha, the pull request number or the run
/// id a person opens; and the **postcondition** that was observed to hold.
///
/// The postcondition is bounded through [`Published::of`] and stripped of
/// control characters, and that is not decoration: it is rendered from the
/// observed object, and two of the three — a pull request's title, a workflow
/// run's status — are text somebody else wrote. A published document's size must
/// be a property of fiddle rather than of whatever the forge was holding.
fn receipt_evidence<T>(kind: EffectKind, receipt: &EffectReceipt<T>) -> EvidenceRef {
    // **Only the first arm is reachable today, and the other two are here rather
    // than collapsed into a `_`.** A receipt is built at exactly two places in
    // `crate::effect`, both from a postcondition that was read back, and both
    // with `Committed`; the other two outcomes leave as an `EffectError` and never
    // reach a receipt, so `receipts()` carries `committed` for every entry a
    // bundle has ever shown.
    //
    // Written out anyway because `EffectOutcome` is the *published* vocabulary —
    // a bundle consumer matches on these three strings — and this match is the
    // one place fiddle spells them. A wildcard here would render a future
    // `Unknown` receipt as whatever the fallback said, which is the collapse the
    // three-valued outcome exists to prevent; an `unreachable!()` would turn it
    // into a panic in the run that first needed to report an ambiguity. Naming
    // all three costs two lines and makes the *rendering* total whatever the
    // constructors later do.
    let outcome = match receipt.outcome {
        EffectOutcome::Committed => "committed",
        EffectOutcome::NotCommitted => "not_committed",
        EffectOutcome::Unknown => "unknown",
    };
    EvidenceRef(format!(
        "effect:{}:{}:{outcome}:{}:{}",
        // The kind's own stable wire name, which is also what its identity was
        // hashed over — so an evidence reference and the effect id beside it
        // cannot come to disagree about which effect this was.
        kind.as_str(),
        receipt.effect_id.0,
        receipt.external_ref.as_deref().unwrap_or("-"),
        one_line(&receipt.postcondition),
    ))
}

/// Bound and flatten one externally-authored string.
fn one_line(text: &str) -> String {
    let flattened: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    Published::of(flattened).as_str().to_string()
}
