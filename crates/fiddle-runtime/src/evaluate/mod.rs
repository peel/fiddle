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
//! # What this module does not own
//!
//! It does not spawn. [`Tree`] is the seam, and the whole of the reason is that
//! *starting a program in the tree under repair* needs things this module has no
//! business holding: a workspace with its four-name environment, a scanner
//! credential, a scratch directory for an artefact, and a deadline. The
//! capability that owns those builds the tree; this module owns the order and
//! the criteria. It is the same split [`crate::scanner`] keeps between its port
//! and its one adapter, and the same one `crate::process` keeps between a bound
//! every child shares and an environment no two spawn sites share.

use crate::scanner::{ScanError, ScanReport};
use async_trait::async_trait;

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

/// Every check's result, in the order the contract declared them.
#[derive(Debug)]
pub struct Evaluation {
    checks: Vec<CheckResult>,
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

    /// Whether this tree is refused: any check that did not pass refuses it.
    ///
    /// Including a check that never ran. An unanswered check is not an answered
    /// one, and a contract that accepted a tree because `docker` was missing
    /// would be a gate that gets weaker as a machine gets more broken.
    pub fn rejected(&self) -> bool {
        self.first_failure().is_some()
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
pub async fn evaluate(contract: &[Check], tree: &impl Tree) -> Result<Evaluation, Cancelled> {
    let mut checks = Vec::with_capacity(contract.len());
    for check in contract {
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
    Ok(Evaluation { checks })
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
