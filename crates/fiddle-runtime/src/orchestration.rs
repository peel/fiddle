//! The static M0 orchestration: observe, derive, record, maybe execute, observe
//! again, publish.
//!
//! Ordinary Rust rather than a workflow DSL. M0's plan is a single deterministic
//! step, and the value of writing it as plain control flow is that the rules it
//! has to honour are visible in one screen:
//!
//! - The capability is reached only through [`NextAction::Execute`]. That is not
//!   a branch anyone has to remember — [`Capability::execute`] cannot be called
//!   without an [`ExecutionGrant`], and `ExecutionGrant::authorise` only issues
//!   one for an `Execute` derivation. A blocked or complete derivation therefore
//!   publishes an empty execution list because there was no way to fill it.
//! - An authorised execution is journaled before it happens. That is likewise
//!   not a branch: the grant is wrapped in an [`Authorised`] whose only
//!   constructor records the intent, and the one call to `execute` in this
//!   module takes an `Authorised`.
//! - After executing, the run observes *again* and derives *again*. What it
//!   reports as the next action is the state it left behind, not the intention
//!   it started with; design §4.7 shows `"next_action": "complete"` for a
//!   successful first run, and a completed run that advertised work still to do
//!   would send its caller round the loop for nothing.
//! - Its *outcome* comes out of that same re-derivation, through [`concluded`].
//!   Not a branch either, and deliberately not an assertion: the two fields of a
//!   bundle a caller switches on are computed from one value, so no observation
//!   and no race can produce a record that says `completed` beside `blocked`.
//! - Executing and recording are one transaction, owned by [`attempt`]. The
//!   ordering rationale is written there.
//!
//! # The publication boundary
//!
//! This module is also where a *string* becomes something a stranger reads. Four
//! fields of a bundle are free text — the three [`RunOutcome`] reasons and
//! [`ProgressEntry::summary`] — and every one of them is filled in here, from an
//! error rendered with `to_string()`. Whatever the run happened to be holding at
//! that moment therefore lands in a published document: an `io::Error`, a check
//! runner's stderr, a response body written at the other end of a socket.
//!
//! The bound on that is not a rule this module remembers. All four fields are
//! [`Published`], whose only constructor applies it, so a fifth field or a fifth
//! variant is bounded by being written at all rather than by whoever writes it
//! having read this paragraph. What the bound does and does not cover — and
//! where the two things it deliberately does not do are handled instead — is in
//! [`fiddle_core::published`].

use crate::capability::{Capability, ExecutionGrant};
use crate::evidence::{mint_attempt_id, publish, EvidenceError};
use crate::journal::{AttemptJournal, AttemptTrace, FileJournal};
use crate::ports::{ChangePort, WorkItemPort};
use fiddle_core::{
    correlation_key, derive_next, CapabilityExecution, EvidenceRef, FiddleBuild, InvocationRef,
    Mode, NextAction, ProgressEntry, Published, ReportBundle, RunOutcome, WorkRef, WorkStateView,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Observe both local sides of the world for one work item.
///
/// Nothing here can fail: a port that cannot read its source returns an
/// `Unavailable` observation rather than an error, so an unobservable world is
/// *reported* rather than aborting the caller. Shared by `run` and by the
/// read-only `inspect`, so both commands see the world through the same call.
///
/// The review and the verification are
/// [`NotApplicable`](fiddle_core::Observation::NotApplicable) here rather than
/// empty, and [`WorkStateView::without_publication`] is where that is written
/// down. Both ports are local: this call reaches no forge, so it has no standing
/// to claim a forge holds nothing.
pub fn observe(
    work_items: &dyn WorkItemPort,
    changes: &dyn ChangePort,
    work_id: &str,
) -> WorkStateView {
    WorkStateView::without_publication(work_items.observe(work_id), changes.observe(work_id))
}

/// Everything one run acts on: who it is for, what it may touch, and what it
/// may do.
///
/// Ports and the capability are borrowed as trait objects, so the orchestration
/// depends on the seams rather than on the fixture-backed implementations M0
/// happens to ship.
pub struct RunContext<'a> {
    /// The project name the correlation key is derived from.
    pub project: &'a str,
    /// The canonical `<scheme>:<value>` text of the invocation.
    pub invocation_ref: &'a str,
    /// The work item both ports are asked about.
    pub work_id: &'a str,
    /// The attempt this run *is*, minted by [`attempt`] and borrowed here.
    ///
    /// Borrowed rather than minted: an id names an attempt, one attempt is one
    /// run, and the journal and the bundle are both filed under this value. It
    /// travels on into [`ExecutionGrant`] so a capability can quote it in
    /// evidence and have the quotation lead somewhere.
    pub attempt: &'a fiddle_core::AttemptId,
    pub work_items: &'a dyn WorkItemPort,
    pub changes: &'a dyn ChangePort,
    pub capability: &'a dyn Capability,
    /// Where this run writes down what it is about to do, before it does it.
    pub journal: &'a dyn AttemptJournal,
}

impl RunContext<'_> {
    /// What this run's ports say about the world right now.
    pub fn observe(&self) -> WorkStateView {
        observe(self.work_items, self.changes, self.work_id)
    }

    /// The same, plus whatever the capability saw of a forge.
    ///
    /// Two ports and one capability, rather than three ports, and the asymmetry
    /// is the point: the ports are local and are read here, while a review and a
    /// verification exist only if something reached a forge — and the only
    /// participant in a run that can do that is the capability. Reading them
    /// here instead would put a credentialled call inside [`observe`], which
    /// `inspect` shares and which is credential-free for every value of
    /// `--capability`.
    ///
    /// A capability that reached no forge answers `None` and the view keeps the
    /// `NotApplicable` pair [`WorkStateView::without_publication`] gives it, so
    /// M0's and M1's bundles are unchanged.
    ///
    /// Called on both arms of the execution, so a run that published a branch
    /// and then failed still publishes what it observed. The two halves are then
    /// from slightly different moments — the ports read now, the forge read
    /// during the execution — which is the same arrangement
    /// [`Capability::receipts`] already has, and the honest one: the alternative
    /// is a bundle that says nothing about a branch that really is out there.
    fn observe_with(&self, capability: &dyn Capability) -> WorkStateView {
        with_publication(self.observe(), capability)
    }

    /// The marker a satisfied change set must carry for this invocation.
    fn expected_marker(&self) -> String {
        correlation_key(self.project, self.invocation_ref)
    }
}

/// What a run did, in the form the CLI renders and a later task publishes.
///
/// `observations` is the view the report is *about*: the post-execution one
/// when the capability ran, and the entry view otherwise — always the view the
/// reported `next_action` was derived from, so the two can never describe
/// different moments.
pub struct RunReport {
    pub outcome: RunOutcome,
    pub next_action: NextAction,
    pub executions: Vec<CapabilityExecution>,
    pub progress: Vec<ProgressEntry>,
    pub observations: WorkStateView,
    /// Set when the run could not record its intent, and therefore refused to
    /// execute. Carried separately from `outcome` because the outcome says what
    /// to do next while this says which path an operator has to make writable.
    pub evidence_failure: Option<EvidenceError>,
}

impl RunReport {
    /// A run that concluded without executing anything.
    ///
    /// Both non-executing derivations funnel through here, which is what makes
    /// "a blocked derivation executes nothing" true by construction rather than
    /// by two independently correct branches.
    fn without_execution(
        outcome: RunOutcome,
        next_action: NextAction,
        observations: WorkStateView,
    ) -> Self {
        RunReport {
            outcome,
            next_action,
            executions: Vec::new(),
            progress: Vec::new(),
            observations,
            evidence_failure: None,
        }
    }
}

/// A grant whose intent has been recorded.
///
/// The same trick as [`ExecutionGrant`], one layer up: the field is private and
/// the only constructor is [`Authorised::recorded`], which journals the intent
/// before it hands one back. The single call to [`Capability::execute`] in this
/// module consumes an `Authorised`, so "the world is never changed before the
/// intent to change it is durable" is a property the compiler holds rather than
/// an ordering someone has to keep remembering.
struct Authorised {
    grant: ExecutionGrant,
}

impl Authorised {
    /// Record `grant`'s intent, and authorise the execution only if that
    /// succeeded.
    fn recorded(
        journal: &dyn AttemptJournal,
        grant: ExecutionGrant,
    ) -> Result<Self, EvidenceError> {
        journal.record_intent(grant.capability_id())?;
        Ok(Authorised { grant })
    }

    fn capability_id(&self) -> fiddle_core::CapabilityId {
        self.grant.capability_id()
    }
}

/// What a run that executed concluded, given what it derived *afterwards*.
///
/// # Why this is a function of the re-derivation
///
/// A capability returning `Ok` says the capability succeeded, not that the work
/// is done. Between the moment it wrote and the moment this run looked again,
/// nothing holds `<stub.root>/changes/<id>.json` still, and `fiddle-core`
/// deliberately reads a change set carrying somebody else's marker as
/// [`NextAction::Blocked`] rather than as satisfied. So a second writer landing
/// in that window — a routine event once M1 has a second capability and
/// references from external sources — makes the post-execution derivation
/// disagree with the assumption that executing implies completion.
///
/// The disagreement therefore has to be *concluded from*, not asserted away. A
/// `debug_assert_eq!` here made the behaviour depend on the build profile: the
/// tested artefact panicked with a code in no row of the exit-code table, and
/// the shipped one published `"outcome":"completed"` beside
/// `"next_action":{"blocked":…}` and exited 0.
///
/// # Which outcome each derivation means
///
/// - `Complete` — the ordinary case: the world the run left behind is the world
///   it was asked to reach.
///
/// - `Blocked` — [`RunOutcome::Failed`], the same conclusion a `Blocked`
///   derivation reaches before executing, so the mapping `Blocked ⇒ Failed` is
///   one rule rather than two that happen to agree. That symmetry is the
///   argument: this run and a run that found the same foreign marker on entry
///   leave *identical* worlds, and an outcome that differed between them would
///   be describing this process's history rather than the world. It also matches
///   what the two words promise. `Retryable` means repeating the invocation
///   succeeds once the named thing is fixed; repeating this one re-derives
///   `Blocked` from its entry observation and concludes `Failed` again, and will
///   keep doing so until somebody settles whose change set it is — which is
///   exactly [`RunOutcome::Failed`]'s "will not succeed by being repeated as
///   invoked". `Suspended` is wrong for a different reason: nothing is waiting
///   on a decision fiddle could offer a human here, and M0 has no decision point
///   at all.
///
/// - `Execute` — [`RunOutcome::Retryable`]. The world is fully observable and
///   records no change set, so the effect this run made did not survive;
///   repeating the invocation executes again and may well succeed, which is
///   precisely what `Retryable` promises and `Failed` denies.
///
/// Both reasons name the fact that the capability had already run, because
/// `Failed` and `Retryable` each have other producers and the exit code alone
/// cannot tell them apart: exit 20 otherwise means "fiddle could not observe the
/// world", and exit 11 otherwise names the change set, the attempt journal, or
/// the report bundle. Neither adds a row to the exit-code table — the CLI's
/// single `exit_code_for` maps these to 20 and 11 unchanged.
fn concluded(next_action: &NextAction) -> RunOutcome {
    match next_action {
        NextAction::Complete => RunOutcome::Completed,
        NextAction::Blocked { reason } => RunOutcome::Failed {
            error: Published::of(format!(
                "the capability executed, and the work is not accounted for afterwards: {reason}"
            )),
        },
        NextAction::Execute { capability_id } => RunOutcome::Retryable {
            reason: Published::of(format!(
                "{} executed and reported success, and the work is still not started \
                 afterwards",
                capability_id.0
            )),
        },
    }
}

/// Execute the M0 plan for one invocation.
///
/// Total: every path returns a report. A capability failure becomes
/// [`RunOutcome::Retryable`] rather than an `Err`, because "try this again"
/// is a conclusion about the run, not an error the caller has to classify.
pub async fn run(ctx: &RunContext<'_>) -> RunReport {
    let marker = ctx.expected_marker();
    let view = ctx.observe();
    // The capability under consideration comes from the context, not from the
    // core: the runtime is the half that knows which capability this run is
    // holding, so it says so rather than leaving the pure core to guess.
    let derived = derive_next(&view, &marker, ctx.capability.id());

    // The grant is the gate. `Complete` and `Blocked` produce none, so the
    // executing arm below is the only code that can reach the capability at
    // all — there is no ordering mistake available here that would let a
    // blocked derivation slip through.
    let Some(grant) = ExecutionGrant::authorise(&derived, ctx.attempt) else {
        return match derived {
            NextAction::Complete => {
                RunReport::without_execution(RunOutcome::Completed, NextAction::Complete, view)
            }
            // `Blocked ⇒ Failed`, the same rule [`concluded`] applies to a
            // derivation taken after executing. Stated in both places because
            // the two arms build different reason texts, not because they
            // disagree about what a blocked world means.
            NextAction::Blocked { reason } => RunReport::without_execution(
                RunOutcome::Failed {
                    error: Published::of(&reason),
                },
                NextAction::Blocked { reason },
                view,
            ),
            // Unreachable: `authorise` returns `Some` for exactly this variant.
            NextAction::Execute { .. } => unreachable!("an Execute derivation always grants"),
        };
    };

    // Fail closed on the journal. An intent that could not be recorded means a
    // capability that would change the world with nothing saying it did, which
    // is the hazard the journal exists to remove — so an unrecordable intent
    // stops the run here rather than proceeding unrecorded. `Retryable`, because
    // fixing the directory and repeating the invocation is exactly what works.
    let authorised = match Authorised::recorded(ctx.journal, grant) {
        Ok(authorised) => authorised,
        Err(error) => {
            let reason = Published::of(error.to_string());
            return RunReport {
                evidence_failure: Some(error),
                // Through the same constructor as the other two non-executing
                // paths, so "nothing ran, so nothing is reported as having run"
                // stays one statement rather than three.
                ..RunReport::without_execution(RunOutcome::Retryable { reason }, derived, view)
            };
        }
    };

    let capability_id = authorised.capability_id();
    match ctx
        .capability
        .execute(authorised.grant, ctx.work_id, ctx.invocation_ref)
        .await
    {
        Ok(evidence) => {
            // Recorded before anything else happens: until the bundle publishes,
            // this is the only thing that says the world moved.
            ctx.journal
                .record_effect(capability_id, "completed", std::slice::from_ref(&evidence));
            // What the capability observed of its own run, asked for on both
            // arms — see `Capability::receipts`. Empty for every capability that
            // has nothing to say, which is what keeps M0's bundles unchanged.
            let observed = ctx.capability.receipts();
            // Re-observe and re-derive: the report must describe the state the
            // run left behind, not the action it chose on entry — including
            // what it left behind on a forge, which only the capability saw.
            let after = ctx.observe_with(ctx.capability);
            let next_action = derive_next(&after, &marker, ctx.capability.id());
            RunReport {
                // Derived from the re-derivation, never asserted to agree with
                // it. See [`concluded`] for why the two can differ at all.
                outcome: concluded(&next_action),
                next_action,
                executions: vec![execution(
                    capability_id,
                    "completed",
                    with_receipts(evidence.clone(), &observed),
                )],
                progress: vec![progress(
                    capability_id,
                    ctx.capability.stage(),
                    "completed",
                    Published::of(format!("wrote correlation marker {marker}")),
                    with_receipts(evidence, &observed),
                )],
                observations: after,
                evidence_failure: None,
            }
        }
        Err(error) => {
            // **The widest of the four.** A capability failure renders whatever
            // the capability was holding — a check runner's stderr, or an
            // `AgentError` that has already declined to quote a response body —
            // and this is the one place it is turned into something published.
            // Bounded here, once, for both fields, because they carry the same
            // text and a bound applied to one of them would be a bound on
            // neither.
            let reason = Published::of(error.to_string());
            // Recorded too: "the capability tried and failed" and "the
            // capability's fate is unknown" are different things to recover
            // from, and only a record written here can tell them apart.
            ctx.journal.record_effect(capability_id, "failed", &[]);
            // **The arm this exists for.** An execution that failed is when an
            // operator most needs to know what it did before it failed, and
            // until this line the answer published here was `[]`.
            let observed = ctx.capability.receipts();
            RunReport {
                outcome: RunOutcome::Retryable {
                    reason: reason.clone(),
                },
                next_action: derived,
                executions: vec![execution(capability_id, "failed", observed.clone())],
                progress: vec![progress(
                    capability_id,
                    ctx.capability.stage(),
                    "failed",
                    reason,
                    observed,
                )],
                // The entry view, plus whatever the capability did reach before
                // it failed. A run that published a branch and then lost its
                // pull request has still put a commit somewhere a reader can go
                // and look at, and a bundle that said nothing about it would be
                // the same gap `receipts` exists to have closed.
                observations: with_publication(view, ctx.capability),
                evidence_failure: None,
            }
        }
    }
}

/// Everything one attempt needs: who it is for, what it may touch, what it may
/// do, and where it records what it did.
///
/// The whole attempt, not the plan alone — which is the point. `RunContext` and
/// [`run`] describe *executing*; this describes executing *and recording*, and
/// the two are one transaction because separating them is what let a capability
/// change the world with nothing on disk saying so.
pub struct AttemptContext<'a> {
    /// The project name the correlation key is derived from.
    pub project: &'a str,
    /// The work this attempt is about, as it was addressed.
    pub reference: &'a InvocationRef,
    /// How the attempt was invoked, recorded in what it publishes.
    pub mode: Mode,
    /// Which build is running, supplied by the binary that compiled it in
    /// rather than looked up here — a bundle must be attributable to the exact
    /// artefact that wrote it.
    pub build: FiddleBuild,
    /// Where bundles and journals live.
    pub report_dir: &'a Path,
    pub work_items: &'a dyn WorkItemPort,
    pub changes: &'a dyn ChangePort,
    pub capability: &'a dyn Capability,
    /// The step trace the capability's executor was built with, if it has one, so
    /// this attempt can point it at its own journal.
    ///
    /// `Option` rather than always present, and the shape is the honest one: only
    /// a capability that reaches the outside world through an
    /// [`Executor`](crate::effect::Executor) has a walk to record, and M0's
    /// `stub_mark` and M1's `fixture_repair` have none. A trace supplied for
    /// those would be a seam that could never be called, which is the class of
    /// thing this bean was wiring up rather than adding to.
    ///
    /// See [`AttemptTrace`] for why the attaching happens here rather than where
    /// the executor was built.
    pub trace: Option<&'a AttemptTrace>,
}

/// What one attempt concluded, and what it managed to record.
pub struct AttemptRecord {
    /// The bundle this attempt concluded with — whether or not it was published,
    /// this is what it concluded, and `bundle.outcome` is the outcome to act on.
    pub bundle: ReportBundle,
    /// Where the bundle landed, relative to `<report.dir>`; `None` when
    /// publication failed. A path naming a bundle that is not there would be
    /// worse than no path at all.
    pub published: Option<PathBuf>,
    /// The durable-evidence problem an operator has to fix, if any. Present in
    /// exactly the cases whose `outcome` is a retryable evidence failure, so a
    /// caller can render a diagnostic naming the path without re-deriving it
    /// from the reason text.
    pub evidence_failure: Option<EvidenceError>,
}

/// Execute and record one attempt: the whole transaction, start to finish.
///
/// # The ordering, and why it is this one
///
/// The sequence is: **observe, derive, record the intent, execute, re-observe,
/// publish, supersede the journal.**
///
/// Recording comes before executing because the alternative is unrecoverable.
/// M0's one capability is idempotent, so an attempt that mutated and was never
/// recorded self-heals — the next attempt observes `Satisfied` and carries on.
/// That is a property of `stub_mark`, not of this design. M2 carries this same
/// sequence over a branch and a pull request, which are not idempotent: there, a
/// crash between "the effect happened" and "the effect was recorded" cannot be
/// resolved by inspection, because a retry has no way to tell *never ran* from
/// *ran but was not recorded*, and will either duplicate the effect or skip it.
/// Writing the intent down first is what makes that distinction survive the
/// crash — see [`crate::journal`].
///
/// If the intent itself cannot be recorded, the capability does not run. That is
/// the fail-closed direction, and it is the only consistent one: a journal whose
/// write may be skipped is not a journal. The run reports
/// [`RunOutcome::Retryable`] naming the journal, because fixing `<report.dir>`
/// and repeating the invocation is precisely what succeeds.
///
/// # Which record is authoritative
///
/// The published bundle, always. The journal is not a second source of truth
/// about what happened; it is a record that an attempt was in flight, and it is
/// removed the moment a bundle for that attempt lands. For any attempt id at
/// most one of the two exists, so they cannot be read as disagreeing, and a
/// journal record means exactly one thing: this attempt did not finish recording
/// itself. [`crate::journal::interrupted`] is how a later reader finds those.
pub async fn attempt(ctx: &AttemptContext<'_>) -> AttemptRecord {
    // Minted once, here: an attempt id names this attempt, and one attempt is
    // one run. It is minted before anything is recorded because both the journal
    // and the bundle are filed under it — and because a capability that quotes
    // an attempt id in its evidence is handed *this* one, through the grant, so
    // the quotation names a document that exists. Nothing outside this function
    // mints one for a run; a second minting site is how the two came to disagree.
    let attempt_id = mint_attempt_id();
    let invocation = ctx.reference.as_str();
    let slug = ctx.reference.slug();
    // Shared rather than owned, because the executor's trace outlives this frame
    // and the journal does not — see [`AttemptTrace`]. The attaching happens
    // before `run`, so the very first step of the very first effect is already
    // recorded: a trace connected afterwards would be missing exactly the steps
    // an interrupted attempt is asked about.
    let journal: Arc<dyn AttemptJournal> = Arc::new(FileJournal::new(
        ctx.report_dir,
        &slug,
        &attempt_id,
        &invocation,
    ));
    if let Some(trace) = ctx.trace {
        trace.attach(Arc::clone(&journal));
    }

    let RunReport {
        outcome,
        next_action,
        executions,
        progress,
        observations,
        evidence_failure,
    } = run(&RunContext {
        project: ctx.project,
        invocation_ref: &invocation,
        work_id: ctx.reference.value(),
        attempt: &attempt_id,
        work_items: ctx.work_items,
        changes: ctx.changes,
        capability: ctx.capability,
        journal: journal.as_ref(),
    })
    .await;

    // `work_ref` is the invocation reference in M0, where a beans reference is
    // both the request and the identity of the work. It is a separate field
    // because the two diverge as soon as a second scheme can address the same
    // work — and because the stability proof compares `work_ref` across two
    // attempts, which would prove nothing if it were derived from the attempt.
    let bundle = ReportBundle {
        schema: fiddle_core::REPORT_SCHEMA,
        fiddle: ctx.build.clone(),
        invocation_ref: invocation.clone(),
        work_ref: Some(WorkRef(invocation)),
        attempt_id: attempt_id.clone(),
        mode: ctx.mode,
        outcome,
        next_action,
        capability_executions: executions,
        progress,
        observations,
    };

    match publish(ctx.report_dir, &slug, &attempt_id, &bundle) {
        Ok(path) => {
            // The bundle is the authoritative record from here on, so the
            // journal has nothing left to say.
            journal.supersede();
            // Relative to `<report.dir>`, so a caller's payload stays the same
            // whatever absolute prefix the configuration happens to name. `path`
            // was built by joining onto that directory, so the strip cannot fail.
            let relative = path.strip_prefix(ctx.report_dir).unwrap_or(&path);
            AttemptRecord {
                bundle,
                published: Some(relative.to_path_buf()),
                evidence_failure,
            }
        }
        Err(error) => {
            // A run whose intent could not be recorded already concluded on that,
            // and it is the root cause: it is why nothing executed, and the
            // publication failing afterwards is the same unwritable directory
            // being reported twice. Otherwise publication is the last thing that
            // can change what this attempt concluded, and it just did.
            let (bundle, failure) = match evidence_failure {
                Some(journal_failure) => (bundle, journal_failure),
                None => (
                    ReportBundle {
                        outcome: RunOutcome::Retryable {
                            reason: Published::of(error.to_string()),
                        },
                        ..bundle
                    },
                    error,
                ),
            };
            AttemptRecord {
                bundle,
                published: None,
                evidence_failure: Some(failure),
            }
        }
    }
}

/// `view`, with the review and the verification the capability observed folded
/// in.
///
/// Written once rather than at each of its call sites, so the executing arm and
/// the failing arm cannot come to disagree about whether a capability's
/// observation of a forge is worth publishing. It is, on both.
fn with_publication(view: WorkStateView, capability: &dyn Capability) -> WorkStateView {
    match capability.publication() {
        Some(publication) => {
            WorkStateView::with_publication(view.work_item, view.changes, publication)
        }
        None => view,
    }
}

/// `earned` first, then whatever the capability observed of its own run.
///
/// The order is the contract: the reference a capability *returned* is what a
/// reader is looking for, and the receipts are context underneath it. A
/// capability that observed nothing produces exactly `[earned]`, unchanged from
/// before this existed.
fn with_receipts(earned: EvidenceRef, observed: &[EvidenceRef]) -> Vec<EvidenceRef> {
    let mut evidence = vec![earned];
    evidence.extend_from_slice(observed);
    evidence
}

fn execution(
    capability_id: fiddle_core::CapabilityId,
    status: &str,
    evidence: Vec<EvidenceRef>,
) -> CapabilityExecution {
    CapabilityExecution {
        capability_id,
        status: status.to_string(),
        evidence,
    }
}

/// One progress entry, filed under the stage the capability names for itself.
///
/// `stage` is a parameter rather than a constant here because the orchestration
/// holds a `&dyn Capability` and therefore does not know which capability it is
/// running — which is the point of the seam. A constant in this module was
/// necessarily one capability's vocabulary applied to every other one; see
/// [`Capability::stage`].
fn progress(
    capability_id: fiddle_core::CapabilityId,
    stage: &str,
    status: &str,
    summary: Published,
    evidence: Vec<EvidenceRef>,
) -> ProgressEntry {
    ProgressEntry {
        capability_id,
        stage: stage.to_string(),
        status: status.to_string(),
        summary,
        evidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityError, StubMark};
    use crate::stub::{StubChangePort, StubWorkItemPort};
    use fiddle_core::{CapabilityId, STUB_MARK};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const WORK_ID: &str = "fiddle-m0-demo";
    const INVOCATION_REF: &str = "beans:fiddle-m0-demo";
    const PROJECT: &str = "icecube";
    const ATTEMPT: &str = "01JQZX0000000000000000000";

    /// Everything a run reached, in the order it reached it.
    ///
    /// Shared by the spy capability and the spy journal, so "the intent was
    /// recorded before the world changed" is an assertion about one sequence
    /// rather than about two counters that happen to agree.
    #[derive(Default)]
    struct Log(std::sync::Mutex<Vec<String>>);

    impl Log {
        fn record(&self, event: impl Into<String>) {
            self.0.lock().unwrap().push(event.into());
        }

        fn events(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    /// A capability that records whether it was reached, so "never executed"
    /// can be asserted directly rather than inferred from its side effects.
    #[derive(Default)]
    struct Spy {
        calls: AtomicUsize,
        log: std::sync::Arc<Log>,
    }

    impl Spy {
        fn watching(log: &std::sync::Arc<Log>) -> Self {
            Spy {
                calls: AtomicUsize::new(0),
                log: std::sync::Arc::clone(log),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    #[async_trait::async_trait]
    impl Capability for Spy {
        fn id(&self) -> CapabilityId {
            STUB_MARK
        }

        fn stage(&self) -> &'static str {
            "spied"
        }

        async fn execute(
            &self,
            _grant: ExecutionGrant,
            _work_id: &str,
            _invocation_ref: &str,
        ) -> Result<EvidenceRef, CapabilityError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.log.record("execute");
            Ok(EvidenceRef("spy:executed".to_string()))
        }
    }

    /// The real capability, logging when it is reached.
    ///
    /// Used where the assertion is about ordering rather than about reachability:
    /// the world has to actually change, or the re-observation the run performs
    /// afterwards has nothing to see.
    struct Watched {
        inner: StubMark,
        log: std::sync::Arc<Log>,
    }

    #[async_trait::async_trait]
    impl Capability for Watched {
        fn id(&self) -> CapabilityId {
            self.inner.id()
        }

        fn stage(&self) -> &'static str {
            self.inner.stage()
        }

        async fn execute(
            &self,
            grant: ExecutionGrant,
            work_id: &str,
            invocation_ref: &str,
        ) -> Result<EvidenceRef, CapabilityError> {
            self.log.record("execute");
            self.inner.execute(grant, work_id, invocation_ref).await
        }
    }

    /// A journal that logs what it was asked to record, and can be told to
    /// refuse — which is how the fail-closed direction is tested without a
    /// filesystem.
    #[derive(Default)]
    struct SpyJournal {
        log: std::sync::Arc<Log>,
        refuse: bool,
    }

    impl SpyJournal {
        fn watching(log: &std::sync::Arc<Log>) -> Self {
            SpyJournal {
                log: std::sync::Arc::clone(log),
                refuse: false,
            }
        }

        fn refusing(log: &std::sync::Arc<Log>) -> Self {
            SpyJournal {
                log: std::sync::Arc::clone(log),
                refuse: true,
            }
        }
    }

    impl AttemptJournal for SpyJournal {
        fn record_intent(&self, _capability: CapabilityId) -> Result<(), EvidenceError> {
            self.log.record("intent");
            if self.refuse {
                return Err(EvidenceError::Journal {
                    path: PathBuf::from("/nowhere/.attempts"),
                    source: std::io::Error::other("refused"),
                });
            }
            Ok(())
        }

        fn record_step(&self, kind: fiddle_core::EffectKind, step: crate::effect::ExecutionStep) {
            self.log
                .record(format!("step:{}:{}", kind.as_str(), step.as_str()));
        }

        fn record_effect(&self, _capability: CapabilityId, status: &str, _e: &[EvidenceRef]) {
            self.log.record(format!("effect:{status}"));
        }

        fn supersede(&self) {
            self.log.record("supersede");
        }
    }

    fn context<'a>(
        capability: &'a dyn Capability,
        work_items: &'a StubWorkItemPort,
        changes: &'a StubChangePort,
        journal: &'a dyn AttemptJournal,
        attempt: &'a fiddle_core::AttemptId,
    ) -> RunContext<'a> {
        RunContext {
            project: PROJECT,
            invocation_ref: INVOCATION_REF,
            work_id: WORK_ID,
            attempt,
            work_items,
            changes,
            capability,
            journal,
        }
    }

    /// The attempt every scenario below runs under. A fixed value rather than a
    /// minted one: these tests are about what a run does, and a stable id keeps
    /// what they assert a function of the world rather than of the clock.
    fn attempt_id() -> fiddle_core::AttemptId {
        fiddle_core::AttemptId(ATTEMPT.to_string())
    }

    fn fixture_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("work")).unwrap();
        std::fs::create_dir_all(dir.path().join("changes")).unwrap();
        std::fs::write(
            dir.path().join(format!("work/{WORK_ID}.json")),
            format!(r#"{{"id":"{WORK_ID}","status":"open"}}"#),
        )
        .unwrap();
        dir
    }

    /// Unstarted work executes once and then reports the state it left, not the
    /// state it found.
    #[tokio::test]
    async fn a_first_run_executes_and_then_reports_complete() {
        let dir = fixture_root();
        let capability = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::default();

        let report = run(&context(
            &capability,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        assert_eq!(report.outcome, RunOutcome::Completed);
        assert_eq!(
            report.next_action,
            NextAction::Complete,
            "the report must describe the state the run left behind"
        );
        assert_eq!(report.executions.len(), 1);
        assert_eq!(report.progress.len(), 1);
        assert_eq!(
            report.observations.changes.value().unwrap().marker,
            Some(correlation_key(PROJECT, INVOCATION_REF)),
            "the reported observations must be the post-execution ones"
        );
    }

    /// **The production path, not only the constructor.** A run whose capability
    /// publishes nothing reports the review and the verification as not
    /// applicable in the bundle it publishes — never as an `Available` value.
    /// Without this, [`WorkStateView::without_publication`] could be entirely
    /// correct and never reached, and a run that spoke to no forge would be
    /// publishing a clean review it never looked for.
    #[tokio::test]
    async fn a_run_that_publishes_nothing_reports_no_review_and_no_verification() {
        let dir = fixture_root();
        let capability = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());

        let report = run(&context(
            &capability,
            &work_items,
            &changes,
            &SpyJournal::default(),
            &attempt_id(),
        ))
        .await;

        let json = serde_json::to_value(&report.observations).unwrap();
        for key in ["review", "verification"] {
            assert!(
                json[key]["available"].is_null(),
                "a run that reached no forge must publish no {key} value: {}",
                json[key]
            );
            assert!(
                json[key]["not_applicable"]["reason"]
                    .as_str()
                    .is_some_and(|reason| !reason.is_empty()),
                "{key} must say why the question does not apply: {}",
                json[key]
            );
        }
        // And the two observations M0's lane reads are still exactly where they
        // were, still saying what the run left behind.
        assert_eq!(json["work_item"]["available"]["value"]["status"], "open");
        assert_eq!(
            json["changes"]["available"]["value"]["marker"],
            correlation_key(PROJECT, INVOCATION_REF),
            "the two new observations must not have displaced the post-execution ones"
        );
    }

    /// The stability property, at the orchestration level: a second run over
    /// the world the first one left finds it satisfied and does nothing.
    #[tokio::test]
    async fn a_second_run_completes_without_executing_again() {
        let dir = fixture_root();
        let log = std::sync::Arc::<Log>::default();
        let spy = Spy::watching(&log);
        let marking = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::watching(&log);

        run(&context(
            &marking,
            &work_items,
            &changes,
            &SpyJournal::default(),
            &attempt_id(),
        ))
        .await;
        let report = run(&context(
            &spy,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        assert_eq!(spy.calls(), 0, "a satisfied world must not execute");
        assert_eq!(report.outcome, RunOutcome::Completed);
        assert_eq!(report.next_action, NextAction::Complete);
        assert!(report.executions.is_empty());
        assert!(report.progress.is_empty());
        assert!(
            log.events().is_empty(),
            "nothing was going to change the world, so nothing may be journaled: {:?}",
            log.events()
        );
    }

    /// The fail-closed arm: an unobservable world never reaches the capability,
    /// and says so with an empty execution list rather than a discarded one.
    #[tokio::test]
    async fn a_blocked_derivation_never_reaches_the_capability() {
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("no-such-root");
        let log = std::sync::Arc::<Log>::default();
        let spy = Spy::watching(&log);
        let work_items = StubWorkItemPort::new(&absent);
        let changes = StubChangePort::new(&absent);
        let journal = SpyJournal::watching(&log);

        let report = run(&context(
            &spy,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        assert_eq!(spy.calls(), 0, "a blocked derivation must not execute");
        assert!(matches!(report.outcome, RunOutcome::Failed { .. }));
        assert!(matches!(report.next_action, NextAction::Blocked { .. }));
        assert!(report.executions.is_empty());
        assert!(report.progress.is_empty());
        assert!(
            log.events().is_empty(),
            "a blocked derivation intends nothing, so it journals nothing: {:?}",
            log.events()
        );
    }

    /// **The ordering, asserted as an order.** The intent is recorded before the
    /// capability is reached, and the effect after it returns — one sequence, so
    /// no pair of independently-correct assertions can pass while the order is
    /// wrong.
    #[tokio::test]
    async fn the_intent_is_recorded_before_the_capability_is_reached() {
        let dir = fixture_root();
        let log = std::sync::Arc::<Log>::default();
        let capability = Watched {
            inner: StubMark::new(dir.path(), PROJECT),
            log: std::sync::Arc::clone(&log),
        };
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::watching(&log);

        run(&context(
            &capability,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        assert_eq!(
            log.events(),
            ["intent", "execute", "effect:completed"],
            "the world must never move before the intention to move it is recorded"
        );
    }

    /// The fail-closed direction of that ordering: an intent that could not be
    /// recorded stops the run instead of letting it change the world unrecorded.
    #[tokio::test]
    async fn an_unrecordable_intent_stops_the_run_before_the_capability() {
        let dir = fixture_root();
        let log = std::sync::Arc::<Log>::default();
        let spy = Spy::watching(&log);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::refusing(&log);

        let report = run(&context(
            &spy,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        assert_eq!(spy.calls(), 0);
        assert_eq!(log.events(), ["intent"], "nothing may follow a refusal");
        match &report.outcome {
            RunOutcome::Retryable { reason } => assert!(
                reason.as_str().contains("attempt journal"),
                "the reason must name the journal: {reason}"
            ),
            other => panic!("an unrecordable intent is retryable, got {other:?}"),
        }
        assert!(
            report.executions.is_empty() && report.progress.is_empty(),
            "nothing ran, so nothing may be reported as having run"
        );
        assert!(matches!(
            report.evidence_failure,
            Some(EvidenceError::Journal { .. })
        ));
    }

    /// A capability that could not write is retryable, and the failure is
    /// recorded as an execution that happened and failed — not as one that
    /// never ran.
    ///
    /// The world has to stay *observable* for the derivation to reach `Execute`
    /// at all, so the failure is injected as a readable but unwritable change
    /// directory rather than a missing one. That is a Unix permission, hence
    /// the gate; and `root` ignores the permission, hence the early return.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_capability_failure_is_retryable_and_recorded() {
        use std::os::unix::fs::PermissionsExt;

        let dir = fixture_root();
        let log = std::sync::Arc::<Log>::default();
        let changes_dir = dir.path().join("changes");
        let capability = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::watching(&log);

        // Readable and listable, but not writable: observation still succeeds.
        std::fs::set_permissions(&changes_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let report = run(&context(
            &capability,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;
        std::fs::set_permissions(&changes_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        if report.outcome == RunOutcome::Completed {
            // Running with an identity that ignores the permission bits.
            return;
        }

        match &report.outcome {
            RunOutcome::Retryable { reason } => {
                assert!(reason.as_str().contains("change set"), "{reason}")
            }
            other => panic!("a failed write must be retryable, got {other:?}"),
        }
        assert_eq!(report.executions.len(), 1);
        assert_eq!(report.executions[0].status, "failed");
        assert!(report.executions[0].evidence.is_empty());
        assert_eq!(report.progress[0].status, "failed");
        // Journaled as *failed*, not left unknown: a later reader must be able to
        // tell "the capability tried and failed" from "the capability's fate was
        // never recorded", which is the whole distinction the journal carries.
        assert_eq!(log.events(), ["intent", "effect:failed"]);
    }
}
