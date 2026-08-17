//! Judging a tree by an ordered list of checks, each against the criterion its
//! own declaration names.
//!
//! A repair attempt hands back a tree, and something has to say whether that
//! tree is better than the one it started from. Design §2.6's answer is five
//! commands — `go build`, `go fmt`, `go vet`, `docker build` and a `wizcli`
//! rescan — and this module is the thing that runs them and collects what they
//! said. It decides no orchestration, opens no pull request and changes nothing
//! outside the tree it is asked about.
//!
//! # One exit code must not stand for five checks
//!
//! The cheap implementation is a shell line: `go build ./... && go fmt ./... &&
//! go vet ./...`, one spawn, one status. It is wrong in a way that only shows up
//! when something fails, which is the only time anybody is reading. `&&` stops
//! at the first failure, so a tree with a build error is never vetted and the
//! operator is told one thing about five. Worse, an aggregate cannot express the
//! second half of this module at all: two of the five are not judged by their
//! exit status, and a chain has nowhere to put that.
//!
//! So [`evaluate`] walks the contract in order, asks the tree for **one
//! observation per check**, and records a [`CheckResult`] for every one of them
//! — *including the ones after a failure*. [`Evaluation::first_failure`] is what
//! restores the "which one broke first" reading that `&&` gave for free, and it
//! is a reading over a complete list rather than a truncated one.
//!
//! # A criterion is declared, never recognised
//!
//! `gofmt -l` exits **zero** and prints the names of the files it would rewrite,
//! so a runner reading exit statuses reports a green `go fmt` over a tree that
//! is not formatted. The fix is not to teach this module about `go fmt`.
//! [`Success`] is the criterion, it arrives on the [`Check`], and the only thing
//! this module ever does with a program name is start it and put it in the
//! record. An operator who pins a version, renames a check or puts a wrapper in
//! front of one has changed *what runs* and not *what it decides*; the opposite
//! arrangement fails as a green run that should have been red, which is the
//! worst direction for a gate to be wrong in.
//!
//! `fiddle_cli::config::Success` is the same closed set in the document, and
//! this one is deliberately a second definition rather than that one imported:
//! `fiddle-runtime` does not depend on `fiddle-cli`, and acquiring a dependency
//! on the binary crate so a runner can name a criterion would invert the
//! layering. The mapping is one `match` in the crate that owns the document.
//!
//! # What this file does not own
//!
//! It does not spawn. [`Tree`] is the seam, and the whole of the reason is that
//! *starting a program in the tree under repair* needs things this file has no
//! business holding: a workspace with its four-name environment, a scanner
//! credential, a scratch directory for an artefact, and a deadline. This file
//! owns the order and the criteria; [`InWorkspace`] is where those four things
//! are held and where the port is really implemented. It is the same split
//! [`crate::scanner`] keeps between its port and its one adapter, and the same
//! one `crate::process` keeps between a bound every child shares and an
//! environment no two spawn sites share.
//!
//! [`InWorkspace`] is that implementation, and it is in this module rather than
//! at the capability that will wire it because a port with no production
//! implementation is a port nothing can be measured against: every claim here
//! about *five separate commands* would rest on a recorder written to agree with
//! it. `cve_evaluate_spawn` is that measurement, over real children in a real
//! worktree.
//!
//! # Two judgements, and only one of them is a check
//!
//! Five green checks are not the same claim as *this repair worked*. `docker
//! build` succeeding says the image still builds; the rescan's own criterion,
//! [`Success::ArtefactWritten`], says only that the scanner left a report
//! behind. Neither of them reads the report, and the report is where the answer
//! is.
//!
//! So there is a second judgement, over the document the rescan wrote, and it
//! puts two conditions to it:
//!
//! - **(a) every advisory the group set out to fix is gone**, from *both*
//!   package arrays. An id surviving in `osPackages` is not gone, and reading
//!   only `libraries` is the exact defect `crate::cve::project`'s
//!   `both_package_arrays_are_read` exists to prevent — which is why this
//!   condition is asked through [`project`] rather than by walking the document
//!   again here. A second reader would be a second place for that collapse to
//!   reappear.
//! - **(b) no finding appeared that was not in the input.** This is the one the
//!   happy path never reaches: a bump that trades one vulnerability for another
//!   clears the group's own advisory, so (a) passes, and only (b) sees the new
//!   `HIGH`. It is the whole reason two conditions are needed rather than one.
//!
//! # Two limits on what a clean answer proves
//!
//! Both conditions are satisfied by an **absence**, and an absence has two ways
//! of arriving that have nothing to do with the tree.
//!
//! - **The feed moved.** A finding leaves a scan because the tree changed *or*
//!   because the advisory feed did. If the rescan ran at a different scanner
//!   version from the scan the input came from, an absence is no longer evidence
//!   about the tree, and the result is [`RescanVerdict::Provisional`].
//! - **Nobody looked.** A document carrying no `osPackages` key has not reported
//!   that the OS findings are gone; it has said nothing about OS packages at
//!   all, and both conditions were therefore answered about half an image. That
//!   is [`RescanVerdict::NotObserved`], and it is why
//!   `crate::cve::project::Arm` distinguishes an absent array from an empty one:
//!   an empty `osPackages` is a distroless runtime's ordinary state and *is* an
//!   observation, and a rule that collapsed the two would refuse every such
//!   image forever.
//!
//! Neither **satisfies [`Evaluation::accepted`]**, and neither is a refusal
//! either. That is why accepted and rejected are two questions here rather than
//! one negation: an unproved rescan is neither.
//!
//! Note which way round both limits apply. Conditions (a) and (b) are *positive*
//! observations — an id is present, a finding is present — and neither a moved
//! feed nor a missing array conjures a record into a report about a tree that no
//! longer has the package. It is the **absence** that they can fake, so they
//! qualify the clean answer and not the dirty ones.
//!
//! # What this module still does not decide
//!
//! What to *do* with any of it. [`Reason`] is introduced here with the two
//! variants an evaluation can produce, and Task 16's disposition extends it with
//! one variant per row of the table it owns. The split is deliberate: evaluation
//! produces a result and disposition consumes it, in that order, and a module
//! that decided both would be the one place a "no findings left" could quietly
//! become "open a pull request".

mod in_workspace;

pub use in_workspace::{InWorkspace, Rescan};

use crate::cve::project::{project, Arm};
use crate::scanner::{ScanError, ScanReport};
use async_trait::async_trait;
use fiddle_core::{AdvisoryId, Severity};

/// What it means for one check to have succeeded.
///
/// **A closed set, and each check names its own member.** The three came from
/// three real programs that disagree: a build succeeds by exiting zero, a
/// formatter succeeds by exiting zero *and printing nothing* — it reports the
/// files it would rewrite on stdout and still exits zero — and a scanner
/// succeeds by *writing its artefact*, whatever it exits, because a non-zero
/// exit is how it reports findings rather than how it reports failure.
///
/// The rejected alternative was to recognise the program: `if program ==
/// "wizcli"`, or a table mapping known commands to the meaning each is known to
/// have. **No code here derives a `Success` from a program name**, and the two
/// halves of `cve_evaluate`'s wrapper pair are what hold that: one moves the
/// formatter behind a path with no `go` and no `fmt` in it and keeps the
/// verdict, the other keeps the spelling and changes the declaration and gets
/// the other verdict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Success {
    /// The process exited zero. M1's only meaning, and a build's.
    ExitZero,

    /// The process exited zero and wrote nothing to stdout or stderr — the
    /// formatter shape, where the output *is* the complaint.
    ExitZeroAndNoOutput,

    /// The artefact was written, whatever the process exited. The scanner
    /// shape; [`crate::scanner::Wizcli`] is the implementation of it, and
    /// [`Tree::scan`] is how this module reaches one rather than reimplementing
    /// the rule.
    ArtefactWritten,
}

/// One check: a program, its arguments, and what success means for it.
///
/// The same three fields `fiddle_cli::config::CheckRef` deserializes, and for
/// the module header's reason not that type. There is no `name` field: see
/// [`Check::name`].
#[derive(Clone, Debug)]
pub struct Check {
    /// The program to run, resolved against the tree's `PATH`.
    pub program: String,

    /// Its arguments, already separated — never a shell string, because a shell
    /// string has to be split by somebody and every splitter is wrong about
    /// quoting somewhere.
    pub args: Vec<String>,

    /// **What this check decides by.** Declared, never inferred from
    /// [`Check::program`].
    pub success: Success,
}

impl Check {
    /// How a result names the check it came from: the command line itself.
    ///
    /// Derived rather than stored, and that is the point. A separate label
    /// would be a second thing an operator writes down and a second thing that
    /// can end up naming something other than what ran — a report saying `go
    /// fmt` failed over a wrapper script nobody can find is worse than one
    /// naming the wrapper. Deriving it means a renamed check reports its new
    /// name for free, which is the same property [`Success`] exists for seen
    /// from the reporting side.
    pub fn name(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The whole of what a tree is judged against: the checks, and the repair the
/// rescan is read for.
///
/// **Two origins in one value, and they are kept visibly apart.** [`checks`] is
/// read from the operator's document — it is configuration, the same for every
/// attempt in a run. [`repair`] is this attempt's: which advisories *this group*
/// set out to clear, what the input scan reported, and which scanner said so. A
/// runner cannot judge a rescan without both, and a signature that took them
/// separately would let a caller pass the second attempt's premise with the
/// first attempt's checks and get an answer that looked fine.
///
/// [`checks`]: Contract::checks
/// [`repair`]: Contract::repair
#[derive(Clone, Debug)]
pub struct Contract {
    /// The checks, in the order they run.
    pub checks: Vec<Check>,

    /// What the rescan is compared against, or `None` where nothing set out to
    /// be fixed.
    ///
    /// `None` is not an error and is not a shortcut: a contract can legitimately
    /// be run over a tree nobody claimed to have repaired — the check list is
    /// still meaningful on its own — and in that case there is no group's
    /// advisory to look for and no earlier scan to compare a version with. What
    /// it must never do is *pass*: with no premise there is nothing that could
    /// have been proved, so the rescan is [`RescanVerdict::NotCompared`] and
    /// [`Evaluation::accepted`] is false. Forgetting to supply one therefore
    /// fails closed.
    pub repair: Option<Repair>,
}

impl Contract {
    /// A contract of `checks` with nothing claimed about a repair.
    ///
    /// The spelling every caller that only wants to run commands should use, so
    /// that `repair: None` is a decision somebody wrote down rather than a field
    /// they did not notice.
    pub fn of(checks: Vec<Check>) -> Self {
        Self {
            checks,
            repair: None,
        }
    }
}

/// What this attempt set out to do, and what its rescan is measured against.
///
/// Every field is a fact about the scan the repair *started* from. That is the
/// point of gathering them into one type: the rescan alone cannot say whether an
/// advisory is missing because the tree was fixed or because it was never
/// reported, and it cannot say whether the same feed was consulted twice.
#[derive(Clone, Debug)]
pub struct Repair {
    /// Every advisory this group set out to clear — condition (a).
    ///
    /// The group's, not the scan's. A run repairs one bump target at a time and
    /// the other groups' findings are still in the image, so a condition (a)
    /// asked over the whole input would refuse every honest repair.
    pub must_clear: Vec<AdvisoryId>,

    /// Every advisory the input scan reported — condition (b)'s baseline.
    ///
    /// A superset of [`Repair::must_clear`], and the distinction is load-bearing
    /// in the other direction: condition (b) asks whether a finding is *new*, and
    /// a baseline of only this group's advisories would read every one of the
    /// other groups' untouched findings as one that just appeared.
    pub input: Vec<AdvisoryId>,

    /// The scanner version the input scan reported for itself.
    ///
    /// Not an `Option`. Every [`ScanReport`] carries one — it is read from the
    /// child's banner before the document is parsed — so a caller that has an
    /// input scan has this, and a `None` arm here would be an unreachable
    /// branch whose only real use would be to skip the comparison.
    pub scanned_at: String,
}

/// Why an evaluation reached the disposition it did, and why a *run* did.
///
/// **A closed set of nine, filled in two halves.** The first two variants are an
/// *evaluation*'s and were introduced here; the seven after them are a
/// *disposition*'s, one per row of Design §3's table, and were added by
/// [`crate::cve::verdict`]. The ownership is split that way because evaluation
/// produces a result and disposition consumes it, in that order — nothing in the
/// first half should grow a variant about what to do next, and nothing in the
/// second should grow one about what a report said.
///
/// They are one enum rather than two because they are one field: a run's record
/// carries exactly one reason, and a reader asking *why did this run come out
/// like that?* should not have to know which of two stages authored the answer.
///
/// A reason is *not* produced for an ordinary failing check. That is what
/// [`Evaluation::first_failure`] already names, in the check's own words, and a
/// second spelling of it here would be a second thing to keep in step with the
/// contract's own list.
///
/// # Six of the seven are fieldless, and the seventh is not, and that is a
/// decision
///
/// The evidence for a row lives on
/// [`Disposition`](crate::cve::verdict::Disposition) — the verdicts, the
/// deferred list, the already-fixed set, the branch — because it is needed
/// whichever row was reached. A reason carrying a copy of it would be a second
/// place for the same fact, and the two would disagree the first time one of
/// them was filtered. What those six carry is *which row*, and nothing else.
///
/// [`Reason::ScanUnusable`] is the exception because it is the only row where
/// the disposition holds no evidence at all: there is no projection, so there
/// are no verdicts, nothing deferred and nothing already fixed. Its diagnostic
/// has nowhere else to be, and a `Retryable` outcome whose text said only
/// *retryable* would tell an operator to repeat a run without saying what to fix
/// first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Reason {
    /// Condition (b): the rescan reports a finding the input scan did not.
    ///
    /// The advisory and its grade travel, because the sentence an operator needs
    /// is *this bump traded CVE-A for CVE-B* and a variant with no id in it
    /// cannot say which one appeared.
    NewFindingAppeared {
        /// The advisory that was not in the input.
        cve: AdvisoryId,
        /// How the rescan graded it.
        severity: Severity,
    },

    /// The rescan ran at a different scanner version from the scan the input
    /// came from, so an advisory that left the scan may have left because the
    /// feed moved rather than because the tree changed.
    ///
    /// Both versions travel rather than a boolean, because the remedy depends on
    /// which way the move went and on how far: an operator who can see `1.2.3`
    /// against `1.3.0` can decide to rescan the input at the new version, and one
    /// told only "provisional" can decide nothing.
    Provisional {
        /// What the input scan reported for itself.
        scanned_at: String,
        /// What the rescan reported for itself.
        rescanned_at: String,
    },

    // -----------------------------------------------------------------------
    // Design §3's table. One variant per row, added by `crate::cve::verdict`.
    // -----------------------------------------------------------------------
    /// The scan ran and both of the projection's sets were empty: there is
    /// nothing in this image this capability acts on.
    ///
    /// **Not the same as [`Reason::ScanUnusable`]**, and the pair is the whole
    /// point of the table. Both are produced from an absence of findings; one is
    /// an absence the scanner *reported* and the other is an absence caused by
    /// there being no report. A run that returned one word for both would make a
    /// broken scanner indistinguishable from a clean image, and the broken
    /// scanner is the one nobody would chase.
    NothingToDo,

    /// The fixable set was empty and there were findings to report anyway.
    ///
    /// No group was formed, so no branch was cut and no tree was touched — Design
    /// §3 row 2. The run has real output, and confusing it with
    /// [`Reason::NothingToDo`] would throw that output away.
    VerdictsOnly,

    /// The shared pull request already covers everything this run would have
    /// done.
    ///
    /// Distinguished from [`Reason::AlreadyFixed`] by where the fix *is*: this
    /// one is on a branch awaiting review, so the action it implies is to go and
    /// merge it, and that one is already in the tree and implies nothing.
    AlreadyInProgress,

    /// Every finding the scan reported had already been dealt with — the tree is
    /// at or above the fix, or a commit on this branch already names it.
    AlreadyFixed,

    /// At least one group ended clean, so a branch carries commits and a pull
    /// request is open on it.
    ///
    /// *At least* one, not all. Design §2.7: a needs-work group does not stop
    /// the run, remaining groups still process and clean ones still land — so a
    /// run with one of each reaches this row and reports the other as a verdict.
    PullRequest,

    /// Bounded attempts ran and not one of them could be shown safe, so every
    /// edit was reverted.
    ///
    /// The difference from [`Reason::VerdictsOnly`] is whether anything was
    /// *attempted*. There, no move existed to make; here, a move was made,
    /// judged, and taken back — which is a thing a person can give direction
    /// about.
    UnsafeWithoutDirection,

    /// The scanner is absent, unreachable, wrote nothing, or wrote a document
    /// this build cannot read.
    ///
    /// **The one row that is not `Completed`.** Design §3: *`Retryable`, never
    /// `NoChange`* — the world was not observed, so the run has concluded
    /// nothing about it, and repeating the invocation once somebody has fixed
    /// what the diagnostic names is exactly what should happen.
    ScanUnusable {
        /// What went wrong, in the producer's own words —
        /// [`ScanError`](crate::scanner::ScanError)'s text, or a rescan's
        /// [`RescanVerdict::Unreadable`].
        why: String,
    },
}

/// What the rescan's own document said, once both conditions had been put to it.
///
/// Separate from [`CheckResult::passed`] on purpose. A check's verdict is a
/// function of the criterion its declaration named and of nothing else — that is
/// the property the whole of [`Success`] exists to hold — and *the group's
/// advisory is still in the report* is not a criterion any declaration names. It
/// is a second reading of what the fifth check produced, so it is recorded as a
/// second thing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RescanVerdict {
    /// Nothing was compared: the contract carried no [`Repair`], or no artefact
    /// check produced a report to read.
    ///
    /// Not a pass. See [`Contract::repair`] — this arm is why a missing premise
    /// fails closed rather than accepting silently.
    NotCompared,

    /// Both conditions held, at the scanner version the input was scanned at.
    /// This is the one arm that is *proof*.
    Cleared,

    /// Both conditions held, at a different scanner version. Carries
    /// [`Reason::Provisional`].
    Provisional(Reason),

    /// Both conditions held, over a document that carried no such package array
    /// at all — so they were answered about half an image.
    ///
    /// **Silence is not clearance.** Conditions (a) and (b) are both satisfied
    /// by an *absence*, and an array the scanner never wrote supplies absences
    /// for free: it did not say the OS findings were gone, it said nothing about
    /// OS packages. Reading that as proof is the same error as reading a CVE
    /// *mentioned* in a merged pull request's body as one that pull request
    /// *fixed* — the misfire [`crate::cve::dedup`] exists to refuse — and this
    /// arm refuses it here.
    ///
    /// **Absence, not emptiness.** An `osPackages` holding no packages is the
    /// ordinary state of a distroless runtime and *is* an observation: the
    /// scanner looked and found none. [`Arm`] is the type that keeps the two
    /// apart, and collapsing them here would refuse every distroless image
    /// forever rather than only the scans that went quiet.
    ///
    /// Not a refusal, for [`RescanVerdict::Provisional`]'s reason: nothing went
    /// wrong with the tree, and a scanner that stopped emitting an array would
    /// otherwise make every honest repair in a run look broken. What is missing
    /// is proof.
    NotObserved {
        /// The array the rescan's document did not carry: `libraries` or
        /// `osPackages`.
        ///
        /// Named rather than left to the reader, for
        /// [`RescanVerdict::StillReported`]'s reason: an operator has to be able
        /// to tell a scanner that stopped reporting OS packages from one that
        /// stopped reporting libraries, and the two have different causes.
        array: &'static str,
    },

    /// Condition (a) failed: these advisories are still reported.
    ///
    /// Carries the ids rather than a count, because a group that cleared three
    /// of four is a different situation from one that cleared none, and an
    /// operator reading the record should not have to diff two reports to find
    /// out which.
    StillReported(Vec<AdvisoryId>),

    /// Condition (b) failed. Carries [`Reason::NewFindingAppeared`].
    NewFinding(Reason),

    /// The rescan wrote a document this build cannot read as a scan report, and
    /// why.
    ///
    /// It refuses the tree, for the reason the artefact criterion refuses one:
    /// a scan that produced nothing usable is not evidence that the repair
    /// worked, and a gate that excused it would get weaker exactly when the
    /// scanner started misbehaving. Task 16's `ScanUnusable` is the disposition
    /// row this arm feeds.
    Unreadable(String),
}

/// What a check's program left behind — the answer a check produced.
///
/// Both streams and the status, rather than the boolean each criterion would
/// prefer, because the criteria want different things from the same shape:
/// [`Success::ExitZero`] reads the status and [`Success::ExitZeroAndNoOutput`]
/// reads all three. Deciding that inside [`Tree`] would put the interpretation
/// in the one place that does not know which criterion it is running under —
/// which is [`crate::cve::dedup::Ran`]'s argument, arrived at again here.
#[derive(Clone, Debug)]
pub struct Answered {
    /// The process's exit status, or `-1` where it was killed by a signal and
    /// has none. `-1` is not a status any process can return, so it cannot be
    /// confused with one the program chose.
    pub exit_code: i32,
    /// Standard output, lossily decoded.
    pub stdout: String,
    /// Standard error, lossily decoded.
    pub stderr: String,
}

/// Why a check produced no observation at all.
///
/// Separate from a check that ran and failed, because the two have opposite
/// remedies: an uninstalled `docker` is an operator's machine to fix, and a
/// failing `docker build` is the repair to revert. A tree that reported both as
/// an exit status would let the second be read as the first, and the loop would
/// throw away a correct repair because a laptop had no daemon.
#[derive(Debug)]
pub enum Unanswered {
    /// The program could not be started: it is not on the machine, it is not
    /// executable, or the operating system refused the spawn.
    ///
    /// The `io::Error` travels rather than a message the tree composed, because
    /// the runner classifies it — see [`evaluate`]. A tree that had already
    /// turned it into prose would make "the program is missing" a phrase to
    /// match on rather than a kind to read.
    NotStarted {
        /// The program that could not be started.
        program: String,
        /// What the operating system said.
        source: std::io::Error,
    },

    /// The check did not finish within its bound and was killed.
    ///
    /// A third kind rather than one of the two beside it, because it is neither.
    /// The program was there and it started, so [`Unanswered::NotStarted`] would
    /// be a claim about the machine that is not true; and nobody asked for the
    /// attempt to end, so [`Unanswered::Cancelled`] would abandon an evaluation
    /// that should still be produced. What it is not either is an *answer*: a
    /// killed child has no exit status and printed however much of its output it
    /// had reached, and recording that as a failing check would report a `docker
    /// build` that hung on a loaded machine identically to one the repair broke.
    ///
    /// It is here rather than in the first version of this enum because that
    /// version had no implementation of [`Tree`] that could reach it — a
    /// scripted tree answers or it does not, and only a real deadline over a
    /// real child makes the case occur. See [`InWorkspace`].
    TimedOut {
        /// The program that was killed.
        program: String,
        /// The bound it overran.
        timeout: std::time::Duration,
    },

    /// The attempt was cancelled, so this check and every check after it was
    /// abandoned.
    ///
    /// Not a failing check, and the difference is the whole reason it is a
    /// variant: nothing went wrong with the tree, so an evaluation must not be
    /// produced at all. See [`Cancelled`].
    Cancelled,
}

/// The tree under judgement, and the one seam every check is started through.
///
/// # Why a port here at all
///
/// Two reasons, and the first is the one [`crate::cve::dedup::Spawn`] gives:
/// **enumerability**. This is the single way the runner starts anything, so an
/// implementation of it holds the complete list of what was run — which is what
/// turns "each check ran as its own command" from a claim about the code into
/// an assertion over a list, and it is the half `Evaluation::checks` cannot
/// show, because five results could be five copies of one status.
///
/// The second is one `dedup` explicitly did *not* have: there is genuinely
/// something worth substituting. `dedup` drives real repositories because a git
/// history is cheap to build; a tree where `docker build` fails and `go vet`
/// passes is not, and neither is one where the container daemon is missing. The
/// contract is offline and the situations are the point, so the world is
/// scripted.
///
/// # Two methods, because two of the five are not commands to read a status of
///
/// [`Tree::run`] answers the exit-status criteria. [`Tree::scan`] answers
/// [`Success::ArtefactWritten`], and it is a method of its own so that this
/// module never reimplements *success is the artefact, not the status line* —
/// [`crate::scanner`] already decides that, over a report it parses, and a
/// second copy of the rule here would be a second thing to keep in step. The
/// runner picks between them by the check's **declaration** and by nothing
/// else.
#[async_trait]
pub trait Tree: Sync {
    /// Start `check`'s program in this tree and wait for it.
    ///
    /// A non-zero status is a *result*, not an error: it is the observation the
    /// whole contract exists to make. `Err` is for a check that produced no
    /// observation.
    async fn run(&self, check: &Check) -> Result<Answered, Unanswered>;

    /// Run `check`'s program as a scanner over this tree's image, and return
    /// the report it wrote.
    ///
    /// The scanner's exit status is deliberately not in the return type. An
    /// implementation routes to [`crate::scanner::Scanner`], which reads the
    /// artefact first and consults the status only to disambiguate its absence.
    ///
    /// A cancellation cannot be distinguished here, and that is
    /// [`ScanError`]'s decision rather than an omission of this one: a deadline
    /// and a cancellation both arrive as [`ScanError::Failed`], because a scan
    /// changes nothing outside the process and *produced no report* is the whole
    /// of what a caller can act on. The consequence for this module is written
    /// at [`Cancelled`].
    async fn scan(&self, check: &Check) -> Result<ScanReport, ScanError>;
}

/// What one check actually did, before any criterion was applied to it.
///
/// The verdict is [`CheckResult::passed`] and lives beside this rather than
/// being derivable from it, because a verdict is a function of the outcome
/// **and** the declaration: exit zero with a filename printed is a pass under
/// one criterion and a failure under another, and an outcome alone cannot say
/// which was asked.
#[derive(Debug)]
pub enum Outcome {
    /// The program ran to completion and this is what it left behind.
    Finished(Answered),

    /// An artefact check's scanner produced its report. Carried whole rather
    /// than reduced to a boolean, because the rescan conditions are read off
    /// this document and a result that had thrown it away would have to run the
    /// scanner again to answer them.
    Scanned(ScanReport),

    /// An artefact check's scanner ran and left nothing this build can use, and
    /// why.
    NoArtefact(String),

    /// The check produced no observation at all, and why.
    ///
    /// **Not the same as a failing check**, and that is the entire reason it is
    /// a variant rather than a `passed: false` with a status of `-1`. See
    /// [`Unanswered`]. It fails the contract all the same — an unanswered check
    /// is not an answered one — but a reader of the record can tell which of the
    /// two remedies they are looking at.
    NotRun(String),
}

/// One check's name, its verdict, and what it did.
#[derive(Debug)]
pub struct CheckResult {
    /// The command line, as [`Check::name`] spells it.
    pub name: String,

    /// Whether this check's **declared** criterion was met.
    pub passed: bool,

    /// What happened, before the criterion was applied. See [`Outcome`].
    pub outcome: Outcome,
}

/// Every check's result, in the order the contract declared them, and what the
/// rescan's own report said.
#[derive(Debug)]
pub struct Evaluation {
    checks: Vec<CheckResult>,
    rescan: RescanVerdict,
}

impl Evaluation {
    /// Every result, in declared order. One per check in the contract, always:
    /// a failure part-way through does not shorten this list.
    pub fn checks(&self) -> &[CheckResult] {
        &self.checks
    }

    /// The earliest failing check in declared order, or `None` where every one
    /// passed.
    ///
    /// *Earliest in the contract*, not first to finish and not last to fail.
    /// The list is built in contract order and the checks are run one at a
    /// time, so `find` is the whole implementation — but the ordering is the
    /// claim, and `first_failure_is_the_earliest_in_declared_order` is what
    /// holds it, over a tree with two failures rather than one.
    pub fn first_failure(&self) -> Option<&CheckResult> {
        self.checks.iter().find(|check| !check.passed)
    }

    /// What the rescan's document said, once both conditions had been put to it.
    ///
    /// The record behind [`Evaluation::accepted`] and [`Evaluation::reason`],
    /// exposed because the two questions those answer are deliberately coarse: a
    /// caller that needs to tell *the group's own advisory survived* from *a new
    /// finding appeared* — and both are refusals — reads it here.
    pub fn rescan(&self) -> &RescanVerdict {
        &self.rescan
    }

    /// Why this evaluation came out as it did, where there is a reason to give.
    ///
    /// `None` for an ordinary failing check — [`Evaluation::first_failure`] is
    /// what names that, in the check's own words — and `None` for a clean run,
    /// which has nothing to explain. See [`Reason`].
    pub fn reason(&self) -> Option<&Reason> {
        match &self.rescan {
            RescanVerdict::Provisional(reason) | RescanVerdict::NewFinding(reason) => Some(reason),
            // `NotObserved` is here rather than carrying a reason of its own
            // because [`Reason`] is closed at Task 12's two variants and Task
            // 16 owns the rest of it. Nothing is lost: the arm names the array
            // it is about, which is the whole of what a reader needs, and
            // [`Evaluation::rescan`] is what a caller that wants it reads.
            RescanVerdict::NotCompared
            | RescanVerdict::Cleared
            | RescanVerdict::NotObserved { .. }
            | RescanVerdict::StillReported(_)
            | RescanVerdict::Unreadable(_) => None,
        }
    }

    /// Whether this tree is *proved* better than the one it started from.
    ///
    /// **The affirmative claim, and it is not the negation of
    /// [`Evaluation::rejected`].** Every check passed by its own declared
    /// criterion, every advisory the group set out to clear is gone from both
    /// package arrays, nothing appeared that was not in the input, and the
    /// rescan both reported on both arrays and ran at the scanner version the
    /// input was scanned at. Anything short of that is not accepted — including
    /// [`RescanVerdict::Provisional`] and [`RescanVerdict::NotObserved`], which
    /// are the two cases this method exists to exclude: an absence observed
    /// through a moved feed is not evidence about a tree, and an absence from an
    /// array nobody reported on is not an observation at all.
    pub fn accepted(&self) -> bool {
        self.first_failure().is_none() && matches!(self.rescan, RescanVerdict::Cleared)
    }

    /// Whether this tree is refused: any check that did not pass refuses it, and
    /// so does a rescan that contradicted the repair.
    ///
    /// Including a check that never ran. An unanswered check is not an answered
    /// one, and a contract that accepted a tree because `docker` was missing
    /// would be a gate that gets weaker as a machine gets more broken.
    ///
    /// A provisional rescan is **not** refused, and neither is one that went
    /// quiet about a package array. That is the whole of why this and
    /// [`Evaluation::accepted`] are two questions. Nothing went wrong with the
    /// tree; what is missing is proof, and reporting an unproved repair as a
    /// failed one would throw away work over a scanner upgrade.
    pub fn rejected(&self) -> bool {
        self.first_failure().is_some()
            || matches!(
                self.rescan,
                RescanVerdict::StillReported(_)
                    | RescanVerdict::NewFinding(_)
                    | RescanVerdict::Unreadable(_)
            )
    }
}

/// The attempt was cancelled, so the tree was never judged.
///
/// A refusal to answer rather than a verdict, and deliberately not an
/// `Evaluation` with everything failing: nothing went wrong with the tree, and
/// an outcome derived from a cancellation must not read as a repair that tried
/// and lost. It is [`crate::workspace::WorkspaceError::Cancelled`]'s reasoning,
/// reached again one layer up.
#[derive(Debug, thiserror::Error)]
#[error("the evaluation was cancelled, so this tree was neither accepted nor rejected")]
pub struct Cancelled;

/// Run every check in `contract` against `tree`, in order, and collect what
/// each of them said.
///
/// **One command per check, and every check runs.** The loop does not stop at
/// the first failure: a tree with a build error is still vetted, still built as
/// an image and still rescanned, because an operator reading a failed run wants
/// the five answers rather than the first one. What `&&` gave for free —
/// *which one broke* — is [`Evaluation::first_failure`], over the complete list.
///
/// **Each criterion comes from its own check's declaration.** The `match` below
/// is on [`Check::success`] and reads [`Check::program`] for nothing but the
/// diagnostic. That is the single place the property could be lost, so it is
/// the single place worth reading twice.
///
/// The one thing that does stop the walk is a cancellation, and it stops it by
/// abandoning the whole evaluation rather than by recording four results and a
/// stub — see [`Cancelled`].
///
/// **Then the second judgement**, over the document the rescan wrote — see this
/// module's header, and `judge` below for the two conditions themselves. It
/// happens after the walk rather than inside it because it is not a check: it
/// reads a
/// report one of the checks produced, and a contract can declare its rescan
/// anywhere in the list.
pub async fn evaluate(contract: &Contract, tree: &impl Tree) -> Result<Evaluation, Cancelled> {
    let mut checks = Vec::with_capacity(contract.checks.len());
    for check in &contract.checks {
        let (outcome, passed) = match check.success {
            // Two criteria, one command. They differ by one clause and share
            // everything else, so they share an arm rather than duplicating the
            // spawn and the not-started handling — which is where a copy would
            // eventually disagree with itself.
            Success::ExitZero | Success::ExitZeroAndNoOutput => match tree.run(check).await {
                Ok(ran) => {
                    // The `go fmt` clause, and it is a clause about the
                    // *declaration* rather than about the program: whatever is
                    // running, this check was declared to treat its output as
                    // the complaint. `gofmt -l` exits zero and names the files
                    // it would rewrite; both streams count, because a formatter
                    // that complains on stderr has still complained.
                    let output_is_the_complaint = check.success == Success::ExitZeroAndNoOutput;
                    let quiet = ran.stdout.is_empty() && ran.stderr.is_empty();
                    let passed = ran.exit_code == 0 && (!output_is_the_complaint || quiet);
                    (Outcome::Finished(ran), passed)
                }
                // Every check after this one still runs: the tree is what could
                // not answer, and the contract is not shortened by that.
                Err(Unanswered::NotStarted { program, source }) => {
                    (Outcome::NotRun(not_started(&program, &source)), false)
                }
                // Unanswered for the same reason and recorded the same way: the
                // check produced no observation, so it did not pass, and the
                // record says which of the two situations an operator is
                // looking at rather than inventing an exit status for it.
                Err(Unanswered::TimedOut { program, timeout }) => (
                    Outcome::NotRun(format!(
                        "{program} did not finish within {timeout:?} and was killed"
                    )),
                    false,
                ),
                Err(Unanswered::Cancelled) => return Err(Cancelled),
            },

            // The artefact criterion, routed rather than reimplemented. What
            // comes back has already had *success is the artefact, not the
            // status line* applied to it by the adapter that owns that rule.
            Success::ArtefactWritten => match tree.scan(check).await {
                Ok(report) => (Outcome::Scanned(report), true),
                // One arm named and the rest gathered, deliberately. The
                // distinction this runner draws is *unanswered* against
                // *answered badly*, and `Missing` is the only one of the six on
                // the unanswered side — there is no scanner on the machine,
                // which is the operator's to fix exactly as an uninstalled
                // `docker` is. Every other arm is a scan that happened and
                // produced nothing usable, which is a fact about the tree's
                // artefact. A seventh arm joins the majority, and that is the
                // safe direction: it fails the check rather than excusing it.
                Err(error @ ScanError::Missing { .. }) => {
                    (Outcome::NotRun(error.to_string()), false)
                }
                Err(error) => (Outcome::NoArtefact(error.to_string()), false),
            },
        };

        checks.push(CheckResult {
            name: check.name(),
            passed,
            outcome,
        });
    }

    // The *last* report, where a contract somehow declares two artefact checks.
    // The rescan is the thing that ran over the repaired tree last, and an
    // earlier scan in the same contract is a scan of something else. `rev` says
    // that in one word rather than leaving it to the order a `find` happens to
    // walk in.
    let report = checks.iter().rev().find_map(|check| match &check.outcome {
        Outcome::Scanned(report) => Some(report),
        // Named rather than gathered under a wildcard, for the reason
        // `InWorkspace::run`'s last arm gives: an outcome added later that also
        // carries a document has to be ruled on here, and a wildcard would
        // silently leave it unjudged — which is the direction that accepts.
        Outcome::Finished(_) | Outcome::NoArtefact(_) | Outcome::NotRun(_) => None,
    });

    // Both halves are required, and the `None` on either side is
    // `NotCompared` rather than a pass. See `Contract::repair`.
    let rescan = match (&contract.repair, report) {
        (Some(repair), Some(report)) => judge(repair, report),
        _ => RescanVerdict::NotCompared,
    };

    Ok(Evaluation { checks, rescan })
}

/// Put both rescan conditions to the report the rescan wrote, and say what the
/// document it was answered over lets that answer prove.
///
/// The order is deliberate and it is not the order the conditions are numbered
/// in the design. An unreadable document comes first because the two conditions
/// cannot be asked of it at all; then (a), then (b), and the two *qualifiers* —
/// what the scanner reported on, and what version it ran at — **last**, because
/// they qualify only the clean answer. Neither a moved feed nor a missing array
/// can make an advisory appear in a report about a tree that no longer carries
/// the package; both can make one disappear. So a surviving id and a new finding
/// are facts either way and are refused without qualification, and it is the
/// clean answer that has to earn the right to be called proof.
fn judge(repair: &Repair, report: &ScanReport) -> RescanVerdict {
    // Through the projection rather than over `report.document` directly. It is
    // the code that reads *both* package arrays and the code the OS half of
    // condition (a) is really about — a second walk of the document here would
    // be a second place for the `libraries`-only collapse to come back, in the
    // one module whose job is to catch it.
    //
    // It selects as well as reads, which is the right reading for both
    // conditions: an advisory the rescan grades below what
    // `fiddle_core::selected` acts on is one this build would not have opened a
    // repair for, so it is neither something the group failed to clear nor
    // something that newly demands attention.
    let projection = match project(report) {
        Ok(projection) => projection,
        Err(why) => return RescanVerdict::Unreadable(why.to_string()),
    };

    // Condition (a): every advisory the group set out to fix is gone. `all()` is
    // both arrays, in document order, which is the whole content of "gone from
    // both" — see above.
    let still_reported: Vec<AdvisoryId> = repair
        .must_clear
        .iter()
        .filter(|&cve| projection.all().any(|finding| &finding.cve == cve))
        .cloned()
        .collect();
    if !still_reported.is_empty() {
        return RescanVerdict::StillReported(still_reported);
    }

    // Condition (b): nothing appeared that the input scan did not report. The
    // baseline is `input` and not `must_clear`, and that is the difference
    // between a condition that catches a traded vulnerability and one that
    // refuses every repair of an image with more than one group's findings in
    // it.
    if let Some(appeared) = projection
        .all()
        .find(|finding| acts_on(finding.severity) && !repair.input.contains(&finding.cve))
    {
        return RescanVerdict::NewFinding(Reason::NewFindingAppeared {
            cve: appeared.cve.clone(),
            severity: appeared.severity,
        });
    }

    // What the two answers above are worth, and the first of the two limits on
    // it. Both conditions are satisfied by an *absence*, and an array the
    // document does not carry supplies absences for free: the scanner did not
    // say the OS findings were gone, it said nothing about OS packages at all.
    //
    // Last, with the version comparison, and for the same reason: a surviving id
    // and a new finding are *positive* observations, and no silence about the
    // other array makes one of them stop being true. Before the version
    // comparison, because a scan that did not look at half the image is a weaker
    // document than one that looked at all of it with a different feed, and
    // where both hold that is the one to say first.
    //
    // Both arms, through the pair, for `cve::project`'s reason for iterating its
    // own: two call sites are free to grow a difference, and the difference this
    // one would grow is a rule that refuses silence about OS packages and
    // accepts it about libraries.
    for (array, arm) in [
        ("libraries", projection.library_arm()),
        ("osPackages", projection.os_arm()),
    ] {
        if arm == Arm::Absent {
            return RescanVerdict::NotObserved { array };
        }
    }

    // The second limit. A string comparison and not a version-order one: the
    // question is whether the *same* scanner looked
    // twice, and "newer" and "older" are the same answer to it — either way a
    // different feed was consulted and an absence stopped being about the tree.
    if report.scanner_version != repair.scanned_at {
        return RescanVerdict::Provisional(Reason::Provisional {
            scanned_at: repair.scanned_at.clone(),
            rescanned_at: report.scanner_version.clone(),
        });
    }

    RescanVerdict::Cleared
}

/// Is a grade one condition (b) refuses a repair over?
///
/// `HIGH` or `CRITICAL`, which is the design's wording and the first arm of
/// `fiddle_core::selected`. Exhaustive with no wildcard, for that function's
/// reason: a grade added later has to be ruled on here rather than defaulting
/// into "not new", which is the failure that presents as *the rescan found
/// nothing*.
fn acts_on(severity: Severity) -> bool {
    match severity {
        Severity::Critical | Severity::High => true,
        Severity::Medium | Severity::Low | Severity::Informational => false,
    }
}

/// Why a check never started, said by the runner rather than by the tree.
///
/// The phrase belongs here because the *distinction* belongs here: whether a
/// program is absent or the spawn failed for some other reason is read off
/// [`std::io::ErrorKind`] rather than matched out of prose an implementation of
/// [`Tree`] happened to compose. A tree that wrote the sentence itself would
/// make every implementation responsible for wording a classification, and two
/// of them would eventually word it differently.
fn not_started(program: &str, source: &std::io::Error) -> String {
    match source.kind() {
        std::io::ErrorKind::NotFound => format!("no such program {program}"),
        _ => format!("{program} could not be started: {source}"),
    }
}
