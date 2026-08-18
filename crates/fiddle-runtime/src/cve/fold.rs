//! Whether a group still has work to do, and what to do with it when it does
//! not.
//!
//! A run walks its groups in order, and each one ends with a rescan of the whole
//! image. That rescan does not stop at the group that caused it: it reports on
//! every package in the image, so it can show that a *later* group's advisories
//! have gone too. One bump routinely clears more than the advisories filed
//! against it — a base image tag moves and a dozen OS findings go with it — and
//! a run that re-attempted each of those groups anyway would open a repair
//! against a tree that already has the fix, and then have to explain a diff that
//! changes nothing.
//!
//! So there is a fold: a group whose every advisory an earlier rescan already
//! showed gone is [`Fold::AlreadyResolved`], recorded and not attempted.
//!
//! # A mid-run clearance is seen two ways, and this module owns both
//!
//! A bump clearing a later group's finding shows up in one of two places, and
//! **two different rules see it**:
//!
//! - **In the rescan.** The scan is of a container and what a container holds is
//!   a binary, so a package `go mod tidy` stopped linking leaves the image while
//!   its requirement is still sitting in `go.mod`. [`fold`] is the rule for that,
//!   and it is written over what the rescan reported for exactly this reason.
//! - **In the tree.** Minimal version selection raises a requirement for every
//!   consumer, so bumping one module routinely moves another one past its own
//!   fix. Nothing about the image has to change for that to be true, and the rule
//!   that sees it is [`select_target_version`](crate::cve::group::select_target_version),
//!   which reads the tree to find out what to move and answers
//!   [`GroupError::AlreadyAtTheFix`] when there is nothing to move.
//!
//! **They are the same fact about the world and they get the same disposition**:
//! recorded as resolved, attempted no further, and reported in no verdict. That
//! is what [`plan_group`] is — one place both arrive at — and it is a function
//! rather than a paragraph because the two had drifted. Until 2026-08-18 the
//! tree-seen half was a `Judgement::UpstreamBlocked` row in `verdicts.json`,
//! which is the classification an advisory *upstream has published no fix for*
//! gets: M4b's Jira step parses that document, so the run raised a ticket
//! against work it had just done, and the two opposite facts reached the operator
//! as the same row.
//!
//! ## Why the tree half is not simply folded in first
//!
//! Because [`fold`] cannot see it. This rule rests on the *previous* group's
//! rescan and refuses to rest on one that proved nothing — provisional, silent
//! about half the image, or describing a tree that was reverted. A requirement
//! the tree really moved is moved in all of those states, so a run that consulted
//! [`fold`] before the selection would still reach `AlreadyAtTheFix` and would
//! still have to say something honest about it. The two rules are not one rule in
//! the wrong order; they are two windows onto one event, and the reconciliation is
//! that they share an answer.
//!
//! The selection also has to run first for a reason that is not about clearance at
//! all — it is what keeps an OS advisory out of a commit body — and [`plan_group`]
//! is where that half is written down.
//!
//! # The direction this rule is dangerous in
//!
//! Folding is a claim that something is fixed. Getting it wrong in the
//! *refusing* direction costs a redundant attempt, which the evaluation would
//! then find nothing to do in; getting it wrong in the *folding* direction
//! records advisories as fixed that nothing fixed, on a branch that will be
//! merged and a report a person will read. The two are not comparable, so every
//! condition below is a reason to **proceed**, and `AlreadyResolved` is what is
//! left when none of them applies.
//!
//! There are three such conditions, and each is a different way an absence
//! arrives without a tree having been repaired.
//!
//! ## The group that produced the rescan must have ended clean
//!
//! [`Evaluation::accepted`] is that question already answered: every check
//! passed *and* [`RescanVerdict::Cleared`]. It is the gate that keeps a fold off
//! the two verdicts that look clean and are not —
//! [`RescanVerdict::Provisional`], where the absence was observed through a
//! moved advisory feed and so is no longer evidence about the tree, and
//! [`RescanVerdict::NotObserved`], where the scanner never reported on half the
//! image and the absence is silence rather than a finding that went away.
//!
//! Both of those are deliberately **not refusals** over in [`crate::evaluate`] —
//! nothing went wrong with the tree — so a disposition is free to keep such a
//! bump on the branch and flag it. That is exactly why this gate cannot be
//! inferred from whether the bump was committed: a committed provisional group
//! is a reachable state, and its rescan still proves nothing.
//!
//! Reading an absence there as proof is [`crate::cve::dedup`]'s misfire in a new
//! place — a CVE *mentioned* in a merged pull request's body is not one that
//! pull request *fixed* — and [`crate::cve::project::Arm`] exists one layer down
//! for the same reason: an array the scanner never wrote is not an array it
//! found empty.
//!
//! ## The bump that produced it must be on the branch
//!
//! A group that ended needs-work has its edit reverted, and then the tree its
//! rescan described does not exist any more. Its report is a perfectly accurate
//! account of a tree nobody will merge, and folding on it would record a group's
//! advisories as fixed by a change that is no longer there. [`Landed`] is that
//! fact, and it is a second one rather than the same one as clean: a clean group
//! whose commit did not happen is the identical hazard, arrived at from the
//! other side.
//!
//! ## Every id, and not merely one
//!
//! A group is one edit fixing a set of advisories. If an earlier rescan shows
//! three of its four gone, the fourth is still there and the edit is still owed
//! — folding would drop it silently, which is the same failure as never having
//! found it. So the test is over [`Group::cves`] with `all`, and a group that
//! names no advisories at all proceeds rather than folding on an empty `all`.
//!
//! # What this module does not own
//!
//! Running git. [`fold_commit_argv`] says what recording a fold *is* — the flags
//! and the body — and Task 15's committer is what runs it. The one thing that
//! could not be left to that seam is the flag pair, which is why it is decided
//! and asserted here: see [`fold_commit_argv`].

use crate::cve::group::{Group, GroupError};
use crate::cve::project::project;
use crate::evaluate::{Evaluation, Outcome};
use fiddle_core::{AdvisoryId, Severities};

/// What to do with a group, given what came before it.
///
/// Two arms and no third, because the caller has exactly two things it can do:
/// run the attempt or not. **Deliberately separate from Task 14's
/// `GroupStatus`**, which says how an attempt *ended* — `Clean` or `NeedsWork`.
/// The two are one enum's worth of temptation and would be wrong merged: this
/// one is decided *before* an attempt and over another group's evidence, that
/// one *after* one and over its own, and a single type would let a caller ask a
/// group that never ran how it went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fold {
    /// Every advisory in this group is already gone, proved by a rescan this
    /// rule was willing to rest on. Record it; do not attempt it.
    AlreadyResolved,

    /// Attempt the group. The default, and what every unmet condition answers.
    Proceed,
}

/// Whether the edit that produced a rescan is on the branch.
///
/// Named as a fact rather than passed as a `bool`, because the two values are
/// not "yes" and "no" to one obvious question — a caller reading
/// `PriorRescan::of(&evaluation, false)` has to go and find out what `false`
/// meant, and the cost of guessing wrong is a fold on a reverted bump.
///
/// This is **not** Task 15's committer and not its record of one. It is the
/// single bit of that outcome this rule consults, declared here so the rule can
/// be written and measured before `fiddle-unci` exists. See this module's
/// header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landed {
    /// The edit was committed, so the branch carries the tree the rescan
    /// described.
    Committed,

    /// The edit was reverted, so it does not. A needs-work group's ordinary
    /// end.
    Reverted,
}

/// An earlier group's rescan, and the two things about its provenance that
/// decide whether this rule may rest on it.
///
/// Constructed from an [`Evaluation`] and nothing else, so that the verdict and
/// the advisory list cannot disagree: a caller able to supply "clean" alongside
/// a hand-written list of ids could assert a rescan that never happened, and the
/// fold would be measured against a fixture written to agree with it.
#[derive(Clone, Debug)]
pub struct PriorRescan {
    /// [`Evaluation::accepted`] — every check passed and the rescan cleared.
    ended_clean: bool,

    /// Whether the tree it describes is on the branch.
    landed: Landed,

    /// Every advisory the rescan's document still reported, canonicalised.
    ///
    /// Empty where the document could not be projected at all, which is a state
    /// [`PriorRescan::ended_clean`] is false in — an emptiness that means "not
    /// read" must never be read as "not present", and the clean gate is what
    /// keeps the two apart here.
    reported: Vec<AdvisoryId>,
}

impl PriorRescan {
    /// What a finished group leaves for the next one.
    ///
    /// `landed` is handed in rather than derived from `evaluation`, because
    /// nothing in an evaluation knows whether anything was committed: evaluation
    /// judges a tree and the committer changes a branch, and inferring one from
    /// the other here would silently assume a disposition table this module does
    /// not own.
    /// `acted_on` is the deployment's grade set, for [`reported_by`]'s reason: it
    /// reads the rescan's document the way the rescan conditions read it, and that
    /// reading is the deployment's rather than this module's.
    pub fn of(evaluation: &Evaluation, landed: Landed, acted_on: &Severities) -> Self {
        PriorRescan {
            ended_clean: evaluation.accepted(),
            landed,
            reported: reported_by(evaluation, acted_on),
        }
    }
}

/// Every advisory the rescan's own document reported, read the way the rescan
/// conditions read it.
///
/// Through [`project`] rather than by walking the document, for the reason
/// `crate::evaluate::judge` gives: it is the code that reads *both* package
/// arrays, and a second walk here would be a second place for the
/// `libraries`-only collapse to come back. It also *selects*, which is the right
/// reading for this rule too — an advisory the rescan grades outside what this
/// deployment acts on is not one a group would have been opened for, so its
/// presence is not a reason to attempt one.
///
/// The **last** scanned report, matching `evaluate`'s own choice of which
/// document the rescan is: an earlier artefact check in the same contract
/// scanned something else.
fn reported_by(evaluation: &Evaluation, acted_on: &Severities) -> Vec<AdvisoryId> {
    let report = evaluation
        .checks()
        .iter()
        .rev()
        .find_map(|check| match &check.outcome {
            Outcome::Scanned(report) => Some(report),
            // Named rather than gathered under a wildcard, for the reason
            // `evaluate`'s own version of this walk gives: an outcome added
            // later that also carries a document has to be ruled on here, and a
            // wildcard would leave it silently unread — which here means an
            // empty list, which means every id looks absent, which is the
            // direction that folds.
            Outcome::Finished(_) | Outcome::NoArtefact(_) | Outcome::NotRun(_) => None,
        });

    match report.map(|report| project(report, acted_on)) {
        Some(Ok(projection)) => projection
            .all()
            .map(|finding| finding.cve.clone())
            .collect(),
        // No report, or one that could not be read. Both are the empty list, and
        // both are states `ended_clean` is false in — see [`PriorRescan`].
        Some(Err(_)) | None => Vec::new(),
    }
}

/// Has an earlier group's rescan already shown every advisory in this one gone?
///
/// `None` is the first group of a run, which has no earlier rescan and therefore
/// nothing to fold on.
///
/// **Not `async`**, and not because it is convenient. Every fact this decision
/// needs was gathered before it was called — a finished evaluation, a committed
/// or reverted branch, and a group's own ids — so there is nothing here to await
/// and nothing for a cancellation token to interrupt. A rule that took one would
/// be claiming it might do I/O, and the next reader would have to go and check.
pub fn fold(group: &Group, prior: Option<&PriorRescan>) -> Fold {
    // The first group of a run. Nothing has been proved yet, so there is nothing
    // this could rest on.
    let Some(prior) = prior else {
        return Fold::Proceed;
    };

    // Gate one: the rescan came from a group that ended clean. This is what
    // keeps a fold off `Provisional` and `NotObserved`, both of which look like
    // a clean result and are not evidence about the tree. See the header.
    if !prior.ended_clean {
        return Fold::Proceed;
    }

    // Gate two: the tree it described is on the branch. A reverted bump's rescan
    // is an accurate report about a tree nobody will merge.
    if prior.landed != Landed::Committed {
        return Fold::Proceed;
    }

    let cves = group.cves();

    // A group naming no advisories folds on `all` over an empty list, which is
    // vacuously true and would record a fold that fixed nothing. `group()` does
    // not build such a group — it collects findings, and a group with no
    // findings has no target to key on — so this is a guard against a future
    // construction rather than a state reachable today, and it is here because
    // the failure it prevents is silent.
    if cves.is_empty() {
        return Fold::Proceed;
    }

    // Every id, not any. Three of four gone still leaves the edit owed, and a
    // fold there would drop the fourth with nothing reporting it missing.
    // Compared through `AdvisoryId`, so `ghsa-…` and `GHSA-…` are one advisory
    // rather than two that never match.
    if cves.into_iter().all(|cve| !prior.reported.contains(cve)) {
        Fold::AlreadyResolved
    } else {
        Fold::Proceed
    }
}

/// What a run does with one group, once the selection has answered and the
/// previous group's rescan is in.
///
/// Three arms, because there are three things the caller can do, and the arm a
/// group lands in is decided here rather than at the two sites that used to
/// decide half of it each. See this module's header for what that cost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupPlan {
    /// Move the target to this version and attempt the group.
    Attempt(String),

    /// Record it as resolved and attempt nothing: an empty commit naming the ids,
    /// no model turn, no rescan, and **no verdict row**. Both clearance paths end
    /// here — the one an earlier rescan showed and the one the tree shows.
    AlreadyResolved,

    /// There is a move owed and this build may not make it. A verdict row,
    /// carrying the refusal's own words.
    Blocked(GroupError),
}

/// Which of the three a group gets.
///
/// # Why the selection has already run by the time this is called
///
/// `selection` arrives as a finished [`Result`], so the caller cannot reach this
/// without having asked
/// [`select_target_version`](crate::cve::group::select_target_version) first. That
/// order is deliberate twice over.
///
/// It is what makes the tree-seen clearance visible at all: the answer *the tree
/// is already at the fix* is a by-product of asking what to move to, and there is
/// nowhere earlier it could come from.
///
/// And it is load-bearing for a reason that has nothing to do with clearance.
/// `CveMitigate::target_version` refuses a `Target::DockerfileBaseImage` group
/// before it compares any version, so such a group reaches this function as
/// [`GroupError::Unselectable`] and leaves it as [`GroupPlan::Blocked`] — never as
/// [`GroupPlan::AlreadyResolved`], whose commit body would name an OS advisory.
/// [`crate::cve::dedup`]'s OS arm consults the commit log and nothing else, so
/// such a body is read back as settled for good, where a library fold's is
/// re-derived against the tree. That was previously a property of two blocks in
/// `sweep` being in one order, which was silently swappable; here it is a
/// property of one `match`, and
/// `a_refusal_this_build_cannot_move_past_is_blocked_even_where_a_fold_would_have_folded`
/// holds it at this tier while
/// `a_rescan_that_clears_the_os_advisory_blocks_it_rather_than_folding_it` holds
/// it from outside the process.
pub fn plan_group(
    group: &Group,
    selection: Result<String, GroupError>,
    prior: Option<&PriorRescan>,
) -> GroupPlan {
    // The refusals are matched with no wildcard, so a variant added to `GroupError`
    // has to be ruled on here. A wildcard would send it to `Blocked`, which is the
    // safe direction for an obstacle and the wrong one for a second way of
    // discovering that there is nothing to do — and nothing would report the choice
    // having been made by omission.
    let target = match selection {
        Ok(target) => target,
        // The clearance the tree shows. Deliberately not conditioned on `prior`:
        // minimal version selection moved the requirement whatever the previous
        // group's rescan turned out to be worth, and requiring a foldable rescan
        // here would put the row back for every run whose rescan was provisional.
        // The refusal is *not* carried out of this arm, and that is the whole of
        // the fix: a `GroupError` reaching the caller becomes a verdict row, and
        // there is nothing here for a person to act on.
        Err(GroupError::AlreadyAtTheFix { .. }) => return GroupPlan::AlreadyResolved,
        Err(
            error @ (GroupError::NoFixedVersion
            | GroupError::Unreadable { .. }
            | GroupError::NoRelease { .. }
            | GroupError::MajorBump { .. }
            | GroupError::Unselectable { .. }),
        ) => return GroupPlan::Blocked(error),
    };

    // The clearance an earlier rescan shows. Consulted after the selection and
    // over the previous group's evidence; `fold`'s own gates are what keep it off
    // a rescan that proved nothing.
    match fold(group, prior) {
        Fold::AlreadyResolved => GroupPlan::AlreadyResolved,
        Fold::Proceed => GroupPlan::Attempt(target),
    }
}

/// The git invocation that records a fold.
///
/// A fold changes no file — that is the whole of what it is — so it needs
/// `--allow-empty` to become a commit at all. The commit is worth making rather
/// than skipping because the body is what the *next* run reads:
/// [`crate::cve::dedup`]'s log scan walks commit bodies for advisory ids, so a
/// fold that left no commit would be re-derived from scratch every run, and one
/// whose body named no ids would be invisible to it.
///
/// **`--amend` is forbidden, and that is the load-bearing half.** Amending is the
/// obvious way to attach an empty change to the commit before it, and on a branch
/// this run is *reusing* the commit before it may belong to a previous run and
/// already be pushed. Rewriting it would then require a force-push, which this
/// system does not do. The failure is invisible on a fresh branch and arrives
/// only on a reused one, which is why the flags are decided here and asserted
/// rather than left to whoever wires the committer.
///
/// Argv and not a shell string: the body names advisory ids and is interpolated,
/// and a quoted string is one escaping bug away from a body that says something
/// else. Task 15's `fiddle-unci` runs this; nothing here spawns.
pub fn fold_commit_argv(group: &Group) -> Vec<String> {
    let ids: Vec<&str> = group.cves().iter().map(|cve| cve.as_str()).collect();
    [
        "commit",
        "--allow-empty",
        "-m",
        &format!(
            "fix: {} already resolved by an earlier bump",
            ids.join(", ")
        ),
    ]
    .iter()
    .map(|argument| argument.to_string())
    .collect()
}
