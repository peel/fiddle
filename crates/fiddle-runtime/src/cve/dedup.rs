//! The already-fixed set: which findings this run must not propose again.
//!
//! A scan photographs an image that was built before the branch reached its
//! current state, so a report routinely names two kinds of finding nobody should
//! act on — one the tree has already moved past, and one an earlier commit *on
//! this branch* already fixed. Dropping them is not tidiness. [`group`]'s
//! [`GroupError::AlreadyAtTheFix`] exists because a group whose tree is already
//! above the fix selects the latest patch inside the fixed minor, which is a
//! **downgrade** carrying a security fix's commit message; that guard is the
//! backstop and this module is what should have made it unreachable.
//!
//! # Two package types, two authorities
//!
//! A **library** finding is settled by comparing versions, through
//! [`version::at_least`] — a Go module version is a number and the tree holds
//! the current one. The scanner's own `current` field is deliberately not
//! consulted: it says what the *image* shipped when it was scanned, and this
//! stage exists precisely because the branch has moved since.
//!
//! An **OS** finding cannot be settled that way at all, because what fixes one
//! is moving a base image tag and base image tags do not sort: `3.20`,
//! `3.20-slim`, `bookworm`, a digest. There is no comparison to make. So the
//! authority is the branch's own record of what it did — the commit bodies
//! between `origin/<base>` and `HEAD`, which is the one statement of intent this
//! branch itself wrote and which a revert takes away again.
//!
//! Neither arm may borrow the other's evidence. A commit body naming a library's
//! advisory does not drop that finding: the body outlives the change, and a
//! revert leaves the sentence behind while taking the fix.
//!
//! # In M4a the OS arm has no producer, and a reader must not conclude otherwise
//!
//! **No run this build performs can write an OS advisory into a commit body.**
//! Selecting a base-image tag needs a registry M4a does not read, so
//! `CveMitigate::target_version` refuses every `Target::DockerfileBaseImage`
//! group with `GroupError::Unselectable`, and a refused group is recorded as
//! blocked before either commit producer is reached — the fold's `--allow-empty`
//! commit, whose message names the group's ids, and
//! [`crate::capability::cve::land`], which commits only `GroupStatus::Clean`.
//! That refusal is a scoping decision with its own note at the site; the
//! consequence is here, because this is where the assumption is *relied on*.
//!
//! So the `PackageType::Os` arm of [`already_fixed`] is reached on every run and
//! can only ever answer `false` against a history this build wrote. It answers
//! `true` only for a body **something other than this build** left in the range
//! — a person's own base-image bump, pushed onto the shared branch between runs.
//! That is a real case and the reason the arm is kept rather than made a
//! refusal; it is also the whole of what the arm can do until the registry
//! arrives. It is otherwise exercised only by seeded history, in
//! `crates/fiddle-runtime/tests/cve_dedup.rs` and in
//! `an_open_pull_request_covering_the_rest_reaches_already_in_progress`
//! (`crates/fiddle-acceptance/tests/cve_mitigation.rs`), which plants the commit
//! naming the OS advisory precisely because no run would. **Both sites say so at
//! themselves**, so that a reader meeting a hand-written body can tell this
//! scoped absence from a round trip nobody got round to driving.
//!
//! **[`commit_log_dedup`] itself is not orphaned by this, and the distinction
//! matters.** Its set has a second consumer — `Run::in_progress`'s `covers`,
//! which filters *every* finding through [`FixedInCommits::names`] and is what
//! puts a run on the `AlreadyInProgress` disposition. Library groups do commit
//! `Fixes:` bodies, and a reused branch's log is read back on the next run, so
//! the scan below and its shallow-history guard are load-bearing on the ordinary
//! path. What has no producer is the OS half of the answer, not the reading of
//! the log.
//!
//! That round trip is driven end to end rather than asserted here.
//! `cve_mitigation::a_second_run_reads_the_first_runs_own_commit_body` starts the
//! binary twice against one remote and seeds no history between the two runs, so
//! the body its second run reads is one
//! [`crate::capability::cve::land`] wrote on the first. It is the lane that holds
//! the range: it fails if `origin/<base>..HEAD` stops reaching the earlier run's
//! commit, and it fails if this reader and that producer come to disagree about
//! what a body looks like — landing on the `AlreadyFixed` row instead of
//! `AlreadyInProgress`, because the tree arm settles the same advisory on its own
//! and that is the row below.
//!
//! The decision and this consequence are recorded together in `docs/BACKLOG.md`
//! under `2026-08-18`, which also records that the registry client belongs to no
//! milestone yet.
//!
//! # A pull request is not an authority, and one incident says why
//!
//! A pull request's body is written when the pull request is opened and lists
//! what a scan found *then*. A rescan after the fix lands leaves everything that
//! was not fixed sitting in that same prose. On 2026-08-12 a merged grpc pull
//! request's body named `CVE-2026-45045` as unrelated leftover still present,
//! dedup read the mention as a fix, and an open finding was dropped. **A mention
//! is evidence a CVE was seen, not that it was fixed.**
//!
//! That is why [`Spawn`] exists rather than this module reaching for
//! `std::process::Command` at each site the way `workspace` and `agent::tools`
//! do. It is not for substitutability — there is nothing here worth faking, and
//! the suite drives real repositories. It is so that *every program this module
//! runs passes through one seam a test can hold*, which turns "no code path
//! consults a forge" from a claim in a comment into an assertion over a list.
//! `crates/fiddle-runtime/tests/cve_dedup.rs` holds it.
//!
//! # A truncated history is a refusal, and that is the surprising direction
//!
//! Under a `--depth 1` clone the log names nothing, so every OS finding reads as
//! *unfixed*. That output is **safe**: the run proposes fixes that are already
//! applied, which wastes a reviewer's time and endangers nothing. Which is
//! exactly why it would stay broken indefinitely if it were silent — nobody
//! chases a report that is merely wasteful. The precondition is therefore
//! asserted rather than relied on, and the diagnostic names the knob a caller
//! actually turns: `fetch-depth`.
//!
//! In M4a that argument reads across from the OS finding it was written about to
//! the `AlreadyInProgress` disposition, and it is worth saying which: with the
//! OS arm having no producer, a truncated history costs nothing an OS finding
//! would notice — it already reads as unfixed. What it costs is `covers`, which
//! goes empty, and a run then reports work still to do that is sitting in an
//! open pull request. Same degraded-but-safe shape, same reason to refuse rather
//! than shrug; a different consumer to name when the OS half acquires one.
//!
//! [`group`]: crate::cve::group
//! [`GroupError::AlreadyAtTheFix`]: crate::cve::group::GroupError::AlreadyAtTheFix

use crate::cve::attribute::{shipped_version, ModuleGraph};
use crate::cve::version;
use fiddle_core::{PackageType, ProjectedFinding};
use std::collections::BTreeSet;
use std::path::Path;

/// Why an already-fixed set could not be read, or a finding not settled.
#[derive(Debug, thiserror::Error)]
pub enum DedupError {
    /// The history does not reach `origin/<base>`, so it cannot say what this
    /// branch already fixed.
    ///
    /// The message names `fetch-depth` because that is the knob, and it is a
    /// refusal rather than an empty set for this module header's reason: the
    /// degraded answer is safe, so it is the one nobody would ever come back to.
    ///
    /// Two causes reach it and they are one defect — a clone git itself reports
    /// as shallow, and a clone that fetched only one branch so `origin/<base>`
    /// is absent. `why` distinguishes them for whoever is reading the failure
    /// without splitting the remedy, which is the same in both cases.
    #[error(
        "the history in {repo} cannot say what this branch already fixed: {why}. \
         Fetch the whole history — `fetch-depth: 0` on actions/checkout — because \
         a truncated log names nothing, and every OS finding then reads as unfixed"
    )]
    ShallowHistory {
        /// The repository that was read.
        repo: String,
        /// Which of the two truncations this was.
        why: String,
    },

    /// git could not be run, or ran and failed where a failure is not an answer.
    ///
    /// One variant for both, because the two are indistinguishable to a caller:
    /// neither leaves an already-fixed set, and the remedy for either is to read
    /// what git said. The one place a non-zero exit *is* an answer —
    /// `rev-parse --verify --quiet` on a ref that may not exist — reads the exit
    /// status itself rather than coming through here.
    #[error("`git {command}` in {repo}: {message}")]
    Git {
        /// The repository the command ran in.
        repo: String,
        /// The arguments, so the failure names itself.
        command: String,
        /// Whatever git or the operating system reported.
        message: String,
    },

    /// The module graph could not be asked what the tree ships.
    ///
    /// Propagated rather than folded into "not already fixed". The safe reading
    /// of a missing answer is that the finding stays open, and this module does
    /// take that reading for a module `go` *answers about* unhelpfully — but a
    /// resolver that could not be asked at all establishes nothing, and a run
    /// that quietly re-proposed every library fix because there was no `go` on
    /// the machine is a run whose report cannot be read.
    #[error(transparent)]
    Resolver(#[from] crate::cve::attribute::ResolverError),
}

/// Every program this module is allowed to start, and the one seam it starts
/// them through.
///
/// See the module header: the point is not substitution but *enumerability*. A
/// test holding an implementation of this holds the complete list of what dedup
/// ran, which is what makes the absence of a forge call assertable.
pub trait Spawn: Sync {
    /// Run `program` with `args` in `dir`.
    ///
    /// `Err` means the program could not be started at all. A program that ran
    /// and exited non-zero comes back as [`Ran`] with `ok` false, because for
    /// one of the three calls here a non-zero exit is the answer rather than a
    /// failure — see [`DedupError::Git`].
    fn run(&self, program: &str, args: &[&str], dir: &Path) -> Result<Ran, DedupError>;
}

/// What a program did.
///
/// Both streams and the status, rather than the `Result<String, _>` the callers
/// would each prefer, because the three call sites want different things from
/// the same shape: one reads stdout, one reads only whether it succeeded, and
/// one needs stderr to put in a diagnostic. Deciding that inside [`Spawn`] would
/// put the interpretation in the one place that does not know which call it is.
#[derive(Debug)]
pub struct Ran {
    /// Whether the program exited zero.
    pub ok: bool,
    /// Standard output, lossily decoded.
    pub stdout: String,
    /// Standard error, lossily decoded.
    pub stderr: String,
}

/// The real one: a child process, in `dir`, with the ambient environment.
///
/// No timeout and no cancellation token, unlike [`crate::process`]'s bounded
/// spawn. Every command here is a local read against an on-disk repository with
/// no network in it — `rev-parse` twice and a `log` — so the failure mode that
/// module exists for, a child that outlives its deadline or its attempt, has
/// nothing to arise from. It also carries no credential, which is why it does
/// not go through `git::publish`'s [`GitCli`] and its redaction.
///
/// [`GitCli`]: crate::git::publish::GitCli
pub struct Local;

impl Spawn for Local {
    fn run(&self, program: &str, args: &[&str], dir: &Path) -> Result<Ran, DedupError> {
        let output = std::process::Command::new(program)
            .args(args)
            .current_dir(dir)
            .output()
            .map_err(|source| DedupError::Git {
                repo: dir.display().to_string(),
                command: args.join(" "),
                message: source.to_string(),
            })?;
        Ok(Ran {
            ok: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}

/// The advisories this branch's own commits say it has already fixed.
///
/// A set of *words*, not of parsed advisory ids. Filtering to things that look
/// like an id would be a second opinion about what an id is, and
/// [`fiddle_core::AdvisoryId`] already holds the only one: any non-blank text.
/// The set is never enumerated — it is only ever asked whether it holds an id a
/// finding named — so the extra words in it cost a little memory and can answer
/// nothing wrong.
#[derive(Debug, Default)]
pub struct FixedInCommits {
    /// Upper-cased, because [`fiddle_core::AdvisoryId`] canonicalises that way
    /// and a commit body does not: `Fixes cve-2026-1` is the same claim.
    words: BTreeSet<String>,
}

impl FixedInCommits {
    /// Read the set out of commit bodies.
    ///
    /// # Why this splits rather than searches
    ///
    /// The obvious implementation asks, for each finding, whether the text
    /// *contains* its id — and that reading closes an open finding, silently:
    /// `CVE-2026-1` is contained in `CVE-2026-10`. Splitting the text into words
    /// first gives both boundaries at once, with no anchor to get half right.
    ///
    /// A word here is a run of ASCII alphanumerics and hyphens, which is exactly
    /// the shape of every advisory id and stops at the punctuation that
    /// surrounds one in prose: `Fixes CVE-2026-1, CVE-2026-2` yields the two
    /// ids, and that is the case the whole design turns on — **one body may name
    /// several advisories, so each is matched on its own.** A scan that matched
    /// the body as a whole, or stopped at the first id in it, would drop only
    /// one of that pair.
    pub fn read(bodies: &str) -> Self {
        let words = bodies
            .split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
            .filter(|word| !word.is_empty())
            .map(|word| word.to_ascii_uppercase())
            .collect();
        FixedInCommits { words }
    }

    /// Does some commit body name `cve`?
    ///
    /// Takes the id as text rather than an [`fiddle_core::AdvisoryId`], because
    /// the caller inside this module has one and the suite that reads a history
    /// back has a literal, and a set that could only be asked about a parsed id
    /// would make the second of those a ceremony.
    pub fn names(&self, cve: &str) -> bool {
        self.words.contains(&cve.to_ascii_uppercase())
    }
}

/// Read the already-fixed set out of `repo`, refusing a truncated history.
///
/// `base` is the branch this one is forked from — the range is
/// `origin/<base>..HEAD`, so what comes back is what *this branch* did and not
/// what the base branch ever did. A run against the base branch's whole history
/// would inherit every fix anybody ever committed, including the ones a later
/// commit reverted.
pub fn commit_log_dedup(repo: &Path, base: &str) -> Result<FixedInCommits, DedupError> {
    commit_log_dedup_with(repo, base, &Local)
}

/// [`commit_log_dedup`] with the spawn seam handed in.
///
/// The injectable form is the real one and the convenience above delegates to
/// it, rather than the other way round: a seam the production path went around
/// would be a seam a test could hold and still learn nothing from.
pub fn commit_log_dedup_with<S>(
    repo: &Path,
    base: &str,
    run: &S,
) -> Result<FixedInCommits, DedupError>
where
    S: Spawn + ?Sized,
{
    let git = |args: &[&str]| run.run("git", args, repo);
    let failed = |args: &[&str], ran: &Ran| DedupError::Git {
        repo: repo.display().to_string(),
        command: args.join(" "),
        message: ran.stderr.clone(),
    };

    // Asked of git rather than inferred from the number of commits, because a
    // repository with one commit is *short* and a repository with one commit and
    // a grafted parent is *truncated*, and only the second is the defect. This
    // is also the first command, so a directory that is not a repository at all
    // fails here naming itself rather than three commands later.
    const SHALLOW: [&str; 2] = ["rev-parse", "--is-shallow-repository"];
    let shallow = git(&SHALLOW)?;
    if !shallow.ok {
        return Err(failed(&SHALLOW, &shallow));
    }
    if shallow.stdout.trim() == "true" {
        return Err(DedupError::ShallowHistory {
            repo: repo.display().to_string(),
            why: "the clone is shallow, so commits before its graft point are absent".to_string(),
        });
    }

    // The second truncation, and a distinct one: a clone that fetched only the
    // head branch is not shallow and still has no `origin/<base>` to measure
    // against. Checked separately so the range below cannot fail with git's own
    // `unknown revision` wording, which names neither the cause nor the remedy.
    //
    // The only place a non-zero exit is an answer rather than a failure:
    // `--verify --quiet` is git's own spelling for *does this ref resolve*.
    let reference = format!("origin/{base}");
    let verify = ["rev-parse", "--verify", "--quiet", reference.as_str()];
    if !git(&verify)?.ok {
        return Err(DedupError::ShallowHistory {
            repo: repo.display().to_string(),
            why: format!("{reference} is not in this clone, so there is no range to read"),
        });
    }

    // `%B` is the raw body — subject and message together — because a `Fixes:`
    // trailer and a subject line naming an advisory are both records a person
    // meant, and reading only one of them would make the format the contract.
    let range = format!("{reference}..HEAD");
    let log = ["log", "--format=%B", range.as_str()];
    let bodies = git(&log)?;
    if !bodies.ok {
        return Err(failed(&log, &bodies));
    }

    Ok(FixedInCommits::read(&bodies.stdout))
}

/// Has this finding already been dealt with?
///
/// `true` means *drop it*: propose nothing, and do not let it reach grouping.
///
/// # It takes a [`ProjectedFinding`] and not an `Attributed`
///
/// Deliberately upstream of attribution. Attribution runs `go mod why` and, for
/// rule 2, writes and reverts a tree — all of it to place a finding this stage
/// is about to throw away. Running dedup first is not only cheaper, it is what
/// keeps [`GroupError::AlreadyAtTheFix`] a guard against the case where one
/// group's bump clears a later group's finding *mid-run*, rather than the
/// routine outcome for every stale report.
///
/// # No [`Spawn`]
///
/// By construction rather than by discipline: this function is handed the
/// already-read commit set and a module graph port, so there is no seam here
/// through which it could reach a forge even if somebody wanted to.
///
/// [`GroupError::AlreadyAtTheFix`]: crate::cve::group::GroupError::AlreadyAtTheFix
pub async fn already_fixed<G>(
    finding: &ProjectedFinding,
    graph: &G,
    fixed: &FixedInCommits,
) -> Result<bool, DedupError>
where
    G: ModuleGraph + ?Sized,
{
    match finding.package_type {
        PackageType::Library => library_is_at_the_fix(finding, graph).await,
        // The commit log and nothing else — see the module header on why a tag
        // comparison is not available here, and why a library may not be settled
        // this way in return.
        //
        // **In M4a this arm has no producer.** No run writes an OS advisory into
        // a commit body, because the base-image group is refused before either
        // commit producer is reached — the module header says which, and
        // `CveMitigate::target_version` says why the refusal is a scoping
        // decision. A hand-written bump on the shared branch is the only history
        // that makes this answer `true`; the suite's OS cases all seed one.
        PackageType::Os => Ok(fixed.names(finding.cve.as_str())),
    }
}

/// Does the tree already resolve this finding's module to the fix or past it?
///
/// The version comes from `go list -m -json`, through the existing
/// [`ModuleGraph::list`], and **not** from a new port method or a bare `go`.
/// That choice is worth stating: `go list -m -f '{{.Version}}'` prints exactly
/// the `Version` field the `-json` form already carries, so a second method
/// would be a second spelling of one question — a second arm in the offline
/// toolchain, a second spawn shape in [`crate::cve::go::Go`], and a second place
/// for the two producers' `v` prefixes to be got wrong. Attribution asks this
/// same question of this same command; there is one reader of the answer, in
/// `attribute`, and this calls it.
async fn library_is_at_the_fix<G>(finding: &ProjectedFinding, graph: &G) -> Result<bool, DedupError>
where
    G: ModuleGraph + ?Sized,
{
    // A finding that names no fixed version cannot be already fixed: there is no
    // version for the tree to be at or above. Answered before `go` is asked,
    // because the answer does not depend on the tree.
    let Some(fixed_version) = finding.fixed_version.as_deref() else {
        return Ok(false);
    };

    // `None` is every way `go` declines to describe the module — a path outside
    // the build list, the main module's own record, which carries no version at
    // all. All of them mean *this tree does not say the module is at anything*,
    // and the honest reading of that is that the finding stays open. An empty
    // string defaulted to and then compared would read as `0`, which is below
    // every fix and happens to give the same answer here — but only by accident,
    // and it would give the opposite one the moment a comparison was inverted.
    let Some(shipped) = shipped_version(&graph.list(finding.package.as_str()).await?) else {
        return Ok(false);
    };

    // The one comparison, `version::at_least`, which strips the leading `v` from
    // both operands and compares component-wise as numbers. Not re-implemented
    // here for the reason that module's header gives: this is the answer in the
    // capability that is wrong *silently*, and a second implementation of it is
    // a second chance to be silently wrong.
    Ok(version::at_least(&shipped, fixed_version))
}
