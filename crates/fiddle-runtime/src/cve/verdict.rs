//! What a run *came to*, and the report a person reads afterwards.
//!
//! Every other module in [`crate::cve`] answers a question about one finding, one
//! group or one tree. This one answers the question the host asks: *what
//! happened?* — and Design §3 is emphatic about the shape of that answer,
//! because M3's central defect was relocated into it. **An outcome two different
//! causes produce identically is not an assertion about either of them.** A
//! scanner that never ran, a scanner that ran and found nothing, a repository
//! whose findings were all fixed last week, and a repository whose one fixable
//! group needs a major version bump are four situations with four different
//! remedies, and a run that reported `NoChange` for all four would have recorded
//! none of them.
//!
//! So the disposition is a *pair* — [`RunOutcome`] and [`Reason`] — and the
//! seven causes reach seven pairs. The outcome is what the host branches on and
//! what the exit code is derived from; the reason is what the record has to
//! carry so that the cause can be recovered from the bundle a week later, when
//! the worktree, the image and the scan are all gone.
//!
//! # Why the table is first-match-wins, and why the order is the substance
//!
//! [`disposition`] is one ordered table, the way
//! [`GroupStatus::of`](crate::capability::cve::GroupStatus::of) is. Two of its
//! rows are the ones a re-ordering would silently break:
//!
//! - **The unusable scan comes first.** Nothing below it is knowable when the
//!   scan produced no document: *no findings* and *no scan* are the same
//!   absence, read from the same emptiness, and every row after this one would
//!   read it as the first.
//! - **`NothingToDo` comes second, before every row that quantifies over a
//!   set.** *Every finding was already fixed* is vacuously true of a scan that
//!   reported no findings, and a table that asked it first would report a clean
//!   image as one whose vulnerabilities somebody else had already dealt with.
//!   That is the same vacuous-`all`-over-an-empty-set defect the evidence
//!   discipline note is about, in the one place where it produces a plausible
//!   answer rather than an obviously wrong one.
//!
//! # What a verdict is, and what it is not
//!
//! A [`Verdict`] is *one unfixed advisory and the sentence explaining why it is
//! still unfixed*. Five fields, and the rationale is carried **verbatim** from
//! whichever upstream value produced it — [`GroupError`]'s own
//! [`Display`](std::fmt::Display) for a group no move exists for, and
//! [`NeedsWork`]'s for a group that was attempted and reverted. Nothing here
//! paraphrases: `GroupError`'s own doc says *the rationale a needs-work verdict
//! reports is this error's own `Display`, so the wording is the interface*, and
//! this is the module on the other side of that sentence.
//!
//! A **deferred** finding is not a verdict, and that distinction is Design §2.5
//! spelled into the type: a finding the run budget did not reach was never
//! assessed, so reporting it beside the ones that were would tell an operator
//! that fiddle looked at it and declined. It is also not already-fixed, because
//! the next run must still be free to take it.
//!
//! # The report is written even when it is empty
//!
//! [`Disposition::write_report`] always writes. A downstream consumer — the
//! workflow's Jira step, a job summary — that has to distinguish *the file is
//! absent* from *there was nothing to report* is a consumer that will get it
//! wrong, and the failure is silent in the direction that matters: a missing
//! file reads as a clean run.
//!
//! [`GroupError`]: crate::cve::group::GroupError
//! [`NeedsWork`]: crate::capability::cve::NeedsWork

use crate::capability::cve::{GroupStatus, MigrationAttempt};
use crate::cve::group::GroupError;
use crate::cve::project::Projection;
use crate::evaluate::Reason;
use fiddle_core::{AdvisoryId, Published, RunOutcome, Severity};
use std::path::{Path, PathBuf};

/// The file the verdict report is written to, beside the report bundle.
///
/// Its own file rather than a key inside `report.json`, because its consumer is
/// not the bundle's: the workflow's Jira step reads this and nothing else, and a
/// step that had to parse the whole bundle to reach one array would break on
/// every unrelated bundle change.
pub const REPORT_FILE: &str = "verdicts.json";

// ---------------------------------------------------------------------------
// The world a disposition is computed from
// ---------------------------------------------------------------------------

/// Everything one run observed, in the order it observed it.
///
/// **A record, not a decision.** Every field here is something that already
/// happened — a scan that produced a document or did not, a dedup that dropped
/// findings, a bounded attempt that ended — and [`disposition`] is the only
/// thing that reads a conclusion out of them. That split is why this type has no
/// `outcome` field for a caller to set: a `Run` a caller could contradict would
/// make the table below advisory.
///
/// It is deliberately *not* async and touches nothing. Every expensive
/// question — did the tree already carry the fix, did the checks pass, does the
/// shared pull request already cover this — was answered upstream by the module
/// that owns it, and asking any of them again here would be a second opinion
/// that could disagree with the first.
#[derive(Debug)]
pub struct Run {
    /// What the scan came to: the projection it produced, or why this build can
    /// make no use of what it wrote.
    ///
    /// A `Result` and not an `Option`, because the failure has to carry its own
    /// diagnostic: Design §3's last row is `Retryable` and an operator repeating
    /// the run needs to know whether the scanner was missing, unreachable or
    /// writing documents this build cannot read. See [`Reason::ScanUnusable`].
    scan: Result<Projection, String>,

    /// The advisories dedup settled before any group was formed — the tree is
    /// already at or above the fix, or a commit on this branch already names it.
    ///
    /// Reported rather than dropped. Design §3 row 3 is a disposition of its
    /// own precisely because *the scan found nothing* and *everything the scan
    /// found was already dealt with* are two situations, and a run that dropped
    /// these silently would present the second as the first.
    pub already_fixed: Vec<AdvisoryId>,

    /// The shared pull request that already covers work this run would
    /// otherwise do, where there is one.
    pub in_progress: Option<InProgress>,

    /// Groups a target version could not be selected for, each with Task 9's
    /// own error.
    ///
    /// Distinct from [`Projection::upstream_blocked`], and the two are not
    /// interchangeable: those are findings the *report* names no fix for
    /// anywhere, and these are findings with a published fix that this build may
    /// not move to — a major bump, a minor with no release carrying it, a
    /// version string it cannot compare. Both produce verdicts and they are
    /// different sentences.
    pub blocked: Vec<Blocked>,

    /// Groups a bounded attempt actually ran on.
    ///
    /// **This being non-empty is what separates Design §3 row 5 from row 2.**
    /// Row 2 is *there was nothing to attempt*; row 5 is *something was
    /// attempted and could not be shown safe*. An operator can act on the second
    /// and not on the first.
    pub attempted: Vec<Attempted>,

    /// Findings the per-run budget did not reach. See [`Budget`].
    pub deferred: Vec<Deferred>,

    /// The branch this run's commits landed on and the pull request carrying
    /// them, once both are known.
    ///
    /// `None` until something has actually been committed and published. A run
    /// that *planned* a branch and then found nothing to put on it has no branch
    /// to report — see [`Disposition::branch`].
    pub landed: Option<Landed>,
}

impl Run {
    /// A run whose scan produced `projection` and which has done nothing else
    /// yet.
    pub fn scanned(projection: Projection) -> Self {
        Run {
            scan: Ok(projection),
            already_fixed: Vec::new(),
            in_progress: None,
            blocked: Vec::new(),
            attempted: Vec::new(),
            deferred: Vec::new(),
            landed: None,
        }
    }

    /// A run whose scan left nothing this build can act on, and why.
    ///
    /// `why` is the [`ScanError`](crate::scanner::ScanError)'s own text. It is
    /// taken as a string rather than as the error because a `Run` is a record
    /// that outlives the scan, and because the same row is reached from a
    /// rescan's [`RescanVerdict::Unreadable`](crate::evaluate::RescanVerdict) —
    /// two producers, one row, and neither of their types is the other's.
    pub fn unusable(why: impl Into<String>) -> Self {
        Run {
            scan: Err(why.into()),
            already_fixed: Vec::new(),
            in_progress: None,
            blocked: Vec::new(),
            attempted: Vec::new(),
            deferred: Vec::new(),
            landed: None,
        }
    }

    /// What the scan produced, where it produced anything.
    pub fn projection(&self) -> Option<&Projection> {
        self.scan.as_ref().ok()
    }
}

/// A shared pull request that already carries the fix for some of this run's
/// findings.
///
/// The number *and* the advisories, because both halves are needed and neither
/// implies the other: the number is what a person clicks, and the covered set is
/// what says whether this run has anything left to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InProgress {
    /// The pull request — the lowest-numbered, where a person has left more than
    /// one open.
    pub number: u64,

    /// The advisories its own commit bodies say it already fixes.
    ///
    /// Read from the branch's commits, never from the pull request's *body*: a
    /// body lists what a scan found when the pull request was opened, so a
    /// mention there is evidence a CVE was seen and not that it was fixed. See
    /// [`crate::cve::dedup`] for the incident that settled this.
    pub covers: Vec<AdvisoryId>,
}

/// A group with a published fix this build may not move to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Blocked {
    /// Every finding in the group. All of them get a verdict, because an
    /// operator reads the report per advisory and a group is not a thing a
    /// ticket is filed against.
    pub findings: Vec<fiddle_core::ProjectedFinding>,

    /// Why no move was available. Its [`Display`](std::fmt::Display) is the
    /// rationale, unaltered.
    pub error: GroupError,
}

/// A group a bounded attempt ran on, and everything it left behind.
#[derive(Debug)]
pub struct Attempted {
    /// Every finding in the group.
    pub findings: Vec<fiddle_core::ProjectedFinding>,

    /// How it ended, as
    /// [`GroupStatus::of`](crate::capability::cve::GroupStatus::of) decided it.
    pub status: GroupStatus,

    /// What the attempt left behind: what the model said, what git saw, and
    /// every shape the scope rules forbid.
    pub attempt: MigrationAttempt,
}

/// The branch a run's commits landed on and the pull request carrying them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Landed {
    /// The shared branch, bare.
    pub branch: String,
    /// The pull request opened or updated at the end of the run.
    pub pull_request: u64,
}

// ---------------------------------------------------------------------------
// The per-run budget
// ---------------------------------------------------------------------------

/// One finding the run budget did not reach.
///
/// **Not a verdict and not already-fixed.** Design §2.5: a finding deferred by
/// one run must be eligible for the next, so deferral touches neither set. What
/// it does carry is the bound that deferred it, because *this run stopped at
/// five* is the only sentence that distinguishes it from a finding fiddle
/// assessed and declined — and the two look identical in a report that names
/// only the advisory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Deferred {
    /// The advisory that will have to wait.
    pub cve: AdvisoryId,
    /// The value of `[orchestration.cve] max_findings` that deferred it.
    pub bound: usize,
}

/// The per-run selection Design §2.5 bounds.
///
/// # Where the number comes from, stated rather than assumed
///
/// `[orchestration.cve] max_findings` is in the PRD's configuration example and
/// is **not yet a key `fiddle-cli`'s config reader accepts** — Task 11 added the
/// ordered check list to `[workspace]` and nothing has yet added the
/// `[orchestration.cve]` table. So this type owns the bound and
/// [`Budget::DEFAULT_MAX_FINDINGS`] is the PRD's own value; wiring the key is
/// the work of whichever task first constructs the capability from a
/// configuration document. Naming that here rather than inventing a reader is
/// the honest half: a default that pretended to be configured would be a bound
/// no operator could change and nobody would notice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Budget {
    max_findings: usize,
}

impl Budget {
    /// The PRD's `[orchestration.cve] max_findings`.
    pub const DEFAULT_MAX_FINDINGS: usize = 5;

    /// A budget of `max_findings` per run.
    pub fn of(max_findings: usize) -> Self {
        Budget { max_findings }
    }

    /// The bound this budget applies.
    pub fn max_findings(&self) -> usize {
        self.max_findings
    }

    /// Split `fixable` into what this run takes and what it defers.
    ///
    /// A **selection**, applied after deduplication rather than as a filter
    /// before it, and Design §2.5 says why in one sentence: a finding deferred
    /// by one run must be eligible for the next. A bound applied earlier would
    /// have to be applied to the already-fixed read as well, and the sixth
    /// finding would then be absent from the record rather than deferred in it.
    ///
    /// Document order, which is the scanner's. Nothing here re-ranks by
    /// severity: [`fiddle_core::selected`] has already discarded everything this
    /// capability does not act on, and a second ordering rule would be a second
    /// thing for a report to disagree with.
    pub fn apply(
        &self,
        fixable: Vec<fiddle_core::ProjectedFinding>,
    ) -> (Vec<fiddle_core::ProjectedFinding>, Vec<Deferred>) {
        let mut taken = fixable;
        let overflow = taken.split_off(taken.len().min(self.max_findings));
        let deferred = overflow
            .into_iter()
            .map(|finding| Deferred {
                cve: finding.cve,
                bound: self.max_findings,
            })
            .collect();
        (taken, deferred)
    }
}

impl Default for Budget {
    fn default() -> Self {
        Budget::of(Budget::DEFAULT_MAX_FINDINGS)
    }
}

// ---------------------------------------------------------------------------
// The verdict report
// ---------------------------------------------------------------------------

/// What this build concluded about one advisory it did not fix.
///
/// **Exactly five fields, in this order**, because the order is the serialized
/// contract: `serde` writes a struct's fields as they are declared, so the array
/// a downstream consumer parses is fixed here and nowhere else. A sixth field
/// added without a decision would change that document for every reader of it.
///
/// One verdict per *advisory*, not per group. A group is this build's unit of
/// work — findings sharing one bump target — and it is not a unit anybody files
/// a ticket against, so a report keyed on it would make its reader do the
/// expansion.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Verdict {
    /// The advisory, canonically spelled.
    pub cve: AdvisoryId,

    /// The package it is against, in the scanner's own naming.
    pub package: String,

    /// Why it is still unfixed, **verbatim** from the value that decided it.
    ///
    /// Never composed here. This is what a person reads in the ticket, and a
    /// sentence this module rewrote would be one that drifted from the rule that
    /// produced it the first time somebody edited either.
    pub rationale: String,

    /// How the scanner graded it.
    pub severity: Severity,

    /// Which kind of unfixed it is. See [`Judgement`].
    pub verdict: Judgement,
}

/// The two ways a finding reaches the report unfixed.
///
/// Two and not three: a **deferred** finding is deliberately not here. It was
/// never judged, so a report row for it would be this build claiming an opinion
/// it does not have — and the next run must be free to take it. See
/// [`Deferred`].
///
/// Nor is a **mid-run clearance**, for the stronger reason that it is not
/// unfixed: an advisory an earlier group's bump already dealt with is recorded as
/// resolved and reaches no row at all. Both ways of seeing one are reconciled in
/// [`crate::cve::fold::plan_group`]. A third variant here was the other way to
/// discharge that, and it was refused: this document is what M4b's Jira step
/// parses, and a row in it is a ticket however it is classified.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Judgement {
    /// There is no move to make: no fix is published, or none this build may
    /// reach without crossing a major or a minor.
    UpstreamBlocked,

    /// A move was made, could not be shown safe, and was reverted. A person has
    /// direction to give.
    NeedsWork,
}

// ---------------------------------------------------------------------------
// The disposition
// ---------------------------------------------------------------------------

/// What one attempted group left behind, published beside the verdict that
/// judged it.
///
/// This is where `claimed_complete` surfaces, and it is the **only** place in
/// the product that reads it. Design §2.5: *`claimed_complete` is evidence
/// beside the exit code that overruled it and is branched on nowhere.* Nothing
/// in [`disposition`] consults this field; it is recorded and published so that
/// a reader can see the model said it was done and see the check that said
/// otherwise, side by side. `cve_protocol::nothing_in_this_workspace_decides_on_
/// claimed_complete` is what holds that across the whole workspace.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttemptRecord {
    /// Every advisory the group covered.
    pub cves: Vec<AdvisoryId>,

    /// How the group ended.
    pub status: GroupStatus,

    /// Whether the model said it had finished. **Evidence only.**
    pub claimed_complete: bool,

    /// Every shape the scope rules forbid in the attempt's diff, in path order.
    ///
    /// **All of them, not the one that decided the group.**
    /// [`GroupStatus::of`](crate::capability::cve::GroupStatus::of) takes the
    /// first for its *reason*, which is all a refusal needs; this is the list an
    /// operator fixing the group by hand works from, and by then the worktree it
    /// was computed in no longer exists.
    pub forbidden: Vec<crate::capability::cve::ForbiddenShape>,
}

/// What a run came to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Disposition {
    outcome: RunOutcome,
    reason: Reason,
    deferred: Vec<Deferred>,
    verdicts: Vec<Verdict>,
    already_fixed: Vec<AdvisoryId>,
    attempts: Vec<AttemptRecord>,
    branch: Option<String>,
    pull_request: Option<u64>,
}

impl Disposition {
    /// The typed result the host branches on, and the exit code is derived from.
    pub fn outcome(&self) -> &RunOutcome {
        &self.outcome
    }

    /// Which of Design §3's rows this run reached.
    pub fn reason(&self) -> &Reason {
        &self.reason
    }

    /// The findings the run budget did not reach.
    pub fn deferred(&self) -> &[Deferred] {
        &self.deferred
    }

    /// Every unfixed advisory and the sentence explaining it.
    pub fn verdicts(&self) -> &[Verdict] {
        &self.verdicts
    }

    /// The advisories dedup settled before this run formed a group for them.
    pub fn already_fixed(&self) -> &[AdvisoryId] {
        &self.already_fixed
    }

    /// What each attempted group left behind. See [`AttemptRecord`].
    pub fn attempts(&self) -> &[AttemptRecord] {
        &self.attempts
    }

    /// The branch this run's commits landed on.
    ///
    /// `Some` on exactly one row — [`Reason::PullRequest`] — and that is a
    /// property of how the value is built rather than of what it was handed: a
    /// run that observed a shared branch and then committed nothing to it has no
    /// branch to name, and naming one anyway would tell a reader that work
    /// landed there.
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// The pull request this run's work is in, or the open one that already
    /// covers it.
    pub fn pull_request(&self) -> Option<u64> {
        self.pull_request
    }

    /// The verdict report, as the document a consumer parses.
    pub fn report(&self) -> serde_json::Value {
        serde_json::to_value(&self.verdicts).expect("a verdict holds no value serde can refuse")
    }

    /// This disposition as the bundle publishes it.
    ///
    /// # Why this exists at all
    ///
    /// Because everything above it was unreadable from outside a run. The
    /// capability computed the pair, wrote [`Disposition::write_report`], and
    /// returned an evidence reference carrying neither half — so `NothingToDo`,
    /// `AlreadyFixed` and `AlreadyInProgress` published byte-identical
    /// artefacts, and `VerdictsOnly` and `UnsafeWithoutDirection` differed only
    /// in the prose of a rationale. Design §3 asks for the opposite in one
    /// sentence: *every `NoChange` carries the evidence for its own reason; one
    /// whose reason cannot be checked from the bundle is not evidenced.*
    ///
    /// # Why it is not `write_report`'s document
    ///
    /// [`REPORT_FILE`] is a contract with the host workflow's Jira and Slack
    /// steps, which read a **bare array** of five-field rows. A header wrapped
    /// around it would break them, so the header goes in the bundle instead —
    /// which is the document Design §3's sentence names anyway.
    ///
    /// # What it drops, deliberately
    ///
    /// The verdicts themselves: they are the report this run already wrote, and
    /// a second copy in the bundle would be a second place for one fact. What
    /// crosses is their count, which is what tells a reader whether the report
    /// beside this bundle has anything in it.
    ///
    /// The scanner's diagnostic on [`Reason::ScanUnusable`] likewise: it is
    /// already the `Retryable` outcome's own text, in the same bundle.
    pub fn published(&self) -> fiddle_core::RunDisposition {
        fiddle_core::RunDisposition {
            reason: self.reason.row().to_string(),
            verdicts: self.verdicts.len(),
            already_fixed: self.already_fixed.clone(),
            deferred: self
                .deferred
                .iter()
                .map(|deferred| fiddle_core::DeferredFinding {
                    cve: deferred.cve.clone(),
                    bound: deferred.bound,
                })
                .collect(),
            attempts: self
                .attempts
                .iter()
                .map(|attempt| fiddle_core::AttemptOutcome {
                    cves: attempt.cves.clone(),
                    // Matched with no wildcard, for `verdicts_of`'s reason: a
                    // status added to `GroupStatus` has to be named here rather
                    // than silently published under a neighbour's word.
                    status: match attempt.status {
                        GroupStatus::Clean => "clean",
                        GroupStatus::NeedsWork { .. } => "needs_work",
                    }
                    .to_string(),
                    claimed_complete: attempt.claimed_complete,
                    // Rendered through `ForbiddenShape`'s own `Display`, which
                    // is the wording a verdict already carries. See
                    // [`fiddle_core::AttemptOutcome::forbidden`].
                    forbidden: attempt.forbidden.iter().map(ToString::to_string).collect(),
                })
                .collect(),
            branch: self.branch.clone(),
            pull_request: self.pull_request,
        }
    }

    /// Write the verdict report into `dir`, **whether or not there is anything
    /// in it**.
    ///
    /// An empty report is `[]` and it is still written. A consumer that had to
    /// tell *the file is absent* from *there was nothing to report* would be
    /// distinguishing a failed run from a clean one by a missing file, and the
    /// direction it gets wrong is the dangerous one — absence reads as success.
    pub fn write_report(&self, dir: &Path) -> Result<PathBuf, std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(REPORT_FILE);
        let mut document = serde_json::to_vec_pretty(&self.verdicts)
            .expect("a verdict holds no value serde can refuse");
        document.push(b'\n');
        std::fs::write(&path, document)?;
        Ok(path)
    }
}

/// What `run` came to. Design §3's table, first match wins.
///
/// # The order, which is the substance
///
/// Six tests and a fall-through, and three of the orderings are load-bearing:
///
/// 1. **The unusable scan is first**, because nothing under it is knowable. A
///    run with no document has no projection, so every set below is empty and
///    every row below would read *the scanner found nothing*. Design §3 calls
///    this the row the milestone is most likely to get wrong.
/// 2. **A clean group beats a needs-work one.** Design §2.7: a needs-work group
///    does not stop the run, remaining groups still process and clean ones still
///    land. The test is `any`, not `all` — a run with one of each has a pull
///    request to point at, and the other group is reported as a verdict on it.
/// 3. **Something attempted beats something merely reported.** That is the whole
///    difference between rows 5 and 2 of Design's table: there was no move to
///    make, against a move that was made, judged and taken back. Only the second
///    is a thing a person can give direction about.
///
/// [`Reason::NothingToDo`] is the **fall-through** rather than a test of its
/// own, and that is deliberate. *Nothing was attempted, nothing is blocked,
/// nothing is awaiting review and nothing had already been fixed* is exactly
/// what having nothing to do consists of, and a separate emptiness test above
/// the others would be a second definition of it — one that could come to
/// disagree with this one, in the direction where a run with real findings
/// reports as a clean image.
///
/// # What this function may not do
///
/// It does not consult `claimed_complete`. Design §2.5: the claim is evidence
/// beside the exit code that overruled it and is branched on nowhere. It reaches
/// [`AttemptRecord`] as a recording and gets no further, and
/// `cve_protocol::nothing_in_this_workspace_decides_on_claimed_complete` is what
/// holds that across the workspace rather than this sentence.
pub fn disposition(run: &Run) -> Disposition {
    // Row 6, and first. See the doc above.
    let projection = match &run.scan {
        Err(why) => {
            return Disposition {
                outcome: RunOutcome::Retryable {
                    reason: Published::of(why),
                },
                reason: Reason::ScanUnusable { why: why.clone() },
                // Every list is empty because there is nothing to put in one: no
                // document means no findings, and a deferred or already-fixed
                // set here would be a claim about an image nobody looked at.
                deferred: Vec::new(),
                verdicts: Vec::new(),
                already_fixed: Vec::new(),
                attempts: Vec::new(),
                branch: None,
                pull_request: None,
            };
        }
        Ok(projection) => projection,
    };

    // Computed before the table and not inside it, because they are the same
    // values whichever row is reached. A report assembled per-arm would be six
    // chances for one arm to forget the deferred list.
    let verdicts = verdicts_of(run, projection);
    let attempts = attempts_of(run);
    let landed = |reason: Reason| Disposition {
        outcome: RunOutcome::Completed,
        reason,
        deferred: run.deferred.clone(),
        verdicts: verdicts.clone(),
        already_fixed: run.already_fixed.clone(),
        attempts: attempts.clone(),
        branch: None,
        pull_request: None,
    };

    // Row 4. `any`, not `all`.
    if run
        .attempted
        .iter()
        .any(|group| group.status == GroupStatus::Clean)
    {
        return Disposition {
            // The branch is named on this row and on no other. A run that
            // observed a shared branch and committed nothing to it has no branch
            // to report, and naming one would tell a reader work landed there.
            branch: run.landed.as_ref().map(|it| it.branch.clone()),
            pull_request: run.landed.as_ref().map(|it| it.pull_request),
            ..landed(Reason::PullRequest)
        };
    }

    // Row 5. Something was attempted and not one of them could be shown safe.
    if !run.attempted.is_empty() {
        return landed(Reason::UnsafeWithoutDirection);
    }

    // Row 2. Nothing was attempted and there is still something to report.
    if !verdicts.is_empty() {
        return landed(Reason::VerdictsOnly);
    }

    // The seventh row. An open pull request already carries the work, so the
    // action is to go and merge it — which is why the number travels and the
    // branch does not.
    if let Some(in_progress) = &run.in_progress {
        if !in_progress.covers.is_empty() {
            return Disposition {
                pull_request: Some(in_progress.number),
                ..landed(Reason::AlreadyInProgress)
            };
        }
    }

    // Row 3. **`!is_empty()` and deliberately not `every finding is in the
    // set`.** The second reads better and is vacuously true of a scan that
    // reported nothing, which would report a clean image as one whose
    // vulnerabilities somebody had already dealt with — plausible enough that
    // nobody would chase it, which is Design §3's whole complaint.
    if !run.already_fixed.is_empty() {
        return landed(Reason::AlreadyFixed);
    }

    // Row 1, as the fall-through. The deferred list is still carried: the one
    // way to arrive here with findings in hand is a budget that took none of
    // them, and the record has to say so.
    landed(Reason::NothingToDo)
}

/// The verdict report `run` produces, as the document a consumer parses.
///
/// A free function beside [`disposition`] rather than only a method on the
/// result, because *the report is always written* is a claim about a run and not
/// about a value somebody remembered to ask for.
pub fn report_of(run: &Run) -> serde_json::Value {
    disposition(run).report()
}

/// Every advisory this run could not patch, and the sentence for each.
///
/// Three producers, in the order a reader meets them: findings the report itself
/// named no fix for, groups a fix exists for that this build may not move to,
/// and groups that were moved and taken back. All three are *upstream* values
/// rendered by their own [`Display`](std::fmt::Display) — nothing here composes
/// a sentence.
///
/// A **clean** group contributes nothing, because its advisories were patched. A
/// **deferred** finding contributes nothing either, and that is the distinction
/// Design §2.5 turns on: it was never judged, so a row for it would be this
/// build claiming an opinion it does not have and would make the next run look
/// like it was re-proposing something already declined.
fn verdicts_of(run: &Run, projection: &Projection) -> Vec<Verdict> {
    let mut verdicts = Vec::new();

    // The report named no fix anywhere. Task 9's own wording for that situation,
    // read off the error rather than written here, so the two cannot drift.
    let no_fix = GroupError::NoFixedVersion.to_string();
    for finding in projection.upstream_blocked() {
        verdicts.push(verdict(finding, no_fix.clone(), Judgement::UpstreamBlocked));
    }

    for group in &run.blocked {
        let rationale = group.error.to_string();
        for finding in &group.findings {
            verdicts.push(verdict(
                finding,
                rationale.clone(),
                Judgement::UpstreamBlocked,
            ));
        }
    }

    for group in &run.attempted {
        // Matched with no wildcard: a status added to `GroupStatus` has to be
        // ruled on here rather than silently contributing no verdict, which is
        // the failure that presents as a clean report.
        let reason = match &group.status {
            GroupStatus::Clean => continue,
            GroupStatus::NeedsWork { reason } => reason,
        };
        let rationale = reason.to_string();
        for finding in &group.findings {
            verdicts.push(verdict(finding, rationale.clone(), Judgement::NeedsWork));
        }
    }

    verdicts
}

/// One finding's row in the report.
fn verdict(
    finding: &fiddle_core::ProjectedFinding,
    rationale: String,
    judgement: Judgement,
) -> Verdict {
    Verdict {
        cve: finding.cve.clone(),
        package: finding.package.clone(),
        rationale,
        severity: finding.severity,
        verdict: judgement,
    }
}

/// What each attempted group left behind.
///
/// This is the one place in the product that reads `claimed_complete`, and it
/// reads it into a field of the same name. See [`AttemptRecord`].
fn attempts_of(run: &Run) -> Vec<AttemptRecord> {
    run.attempted
        .iter()
        .map(|group| {
            let report = &group.attempt.report;
            AttemptRecord {
                cves: group
                    .findings
                    .iter()
                    .map(|finding| finding.cve.clone())
                    .collect(),
                status: group.status.clone(),
                claimed_complete: report.claimed_complete,
                // Cloned whole and in the order `classify` produced it. See the
                // field's own doc for why the first is not enough.
                forbidden: group.attempt.forbidden.clone(),
            }
        })
        .collect()
}
