//! The production [`Tree`]: a real worktree, real children, and the one
//! scanner adapter this crate has.
//!
//! [`evaluate`](super::evaluate) owns the order of the checks and the criterion
//! each of them is judged by; it deliberately owns no spawn. This is the
//! implementation that supplies one, and it is the thing that makes the
//! contract a statement about processes rather than about a trait: without it
//! the port has no production implementation at all, and *each check runs as
//! its own command* is evidenced only by a recorder agreeing with itself.
//!
//! # It composes two spawn sites and adds none
//!
//! That is the whole shape of it, and it is the reason the module is short.
//! [`InWorkspace::run`] goes through [`Workspace::run`], which owns the
//! four-name environment a check child sees and the relativisation applied to
//! what it printed; [`InWorkspace::scan`] goes through [`Wizcli`], which owns
//! the five-name environment a scanner child sees, the credential channel, and
//! *success is the artefact, not the status line*. Nothing here builds a
//! [`tokio::process::Command`], reads an environment variable, or decides what a
//! scanner's exit status meant. An adapter that did any of those would be a
//! fifth spawn site with a fifth environment nobody had argued for, and the two
//! boundary assertions those modules carry
//! — `workspace::a_workspace_command_inherits_no_credential` and
//! `scanner::the_wizcli_environment_is_exactly_its_allowlist_and_no_credential_reaches_argv`
//! — would stop being statements about how a check actually runs.
//!
//! # The check decides what is executed, and this is where that is at risk
//!
//! [`Tree::scan`] is the method the risk lives in. A [`Wizcli`] needs four
//! things a [`Check`] does not carry — a scratch directory, a tenant
//! credential, a deadline and a cancellation token — and the obvious way to
//! satisfy that is to build one scanner up front and hold it. That adapter would
//! then run *its* program for every artefact check, whatever the check declared,
//! and an operator who pinned `wizcli` to a wrapper would find the wrapper
//! quietly ignored: the seam would still be in the document and no longer in the
//! product.
//!
//! So the four ambient things arrive at construction, in [`Rescan`], and the
//! **program and its arguments arrive from the check**, on every call. A scanner
//! is built per artefact check out of the two halves. It costs a struct per
//! scan, which is nothing, and it keeps [`Check::program`] the only thing that
//! decides what runs — which is the property the whole of `evaluate` exists to
//! hold, seen from the one place it could have been lost.
//!
//! # What it cannot do
//!
//! Three limits, stated here rather than left to be discovered:
//!
//! - **One deadline for the whole contract.** A [`Check`] declares a program, its
//!   arguments and its criterion, and no bound; so [`InWorkspace::new`] takes one
//!   and every check runs under it. `go fmt` and `docker build` are not the same
//!   order of magnitude and a single bound has to be the looser one — the same
//!   trade [`WorkspaceCommand`] names, resolved the same way, because a per-check
//!   timeout is a field on the declaration and therefore a change to the document
//!   the declaration is read from.
//! - **The image is told, not learnt.** A rescan needs something to scan, and the
//!   thing to scan is what the `docker build` check earlier in the contract
//!   produced. Nothing here watches that check or parses its output for an id:
//!   the reference is a [`Rescan`] field the caller supplies. A check declares a
//!   *command*, not a subject, and inferring the subject from a neighbouring
//!   check's stdout would be exactly the program-recognition this module's
//!   sibling refuses.
//! - **A cancelled rescan is not a cancelled evaluation.** [`Tree::scan`] returns
//!   [`ScanError`], which has nowhere to say *cancelled* — see that type's header
//!   for why a scan has no ambiguous-write vocabulary — so a rescan interrupted
//!   part-way is recorded as an artefact check that produced nothing rather than
//!   as [`Cancelled`](super::Cancelled). What this adapter can do it does: it
//!   refuses to *start* a scanner once the token is cancelled, so the loop cannot
//!   spend a child on an attempt that has already ended.

use super::{Answered, Check, Tree, Unanswered};
use crate::scanner::{ScanError, ScanReport, Scanner, WizCredential, Wizcli};
use crate::workspace::{Workspace, WorkspaceCommand, WorkspaceError};
use async_trait::async_trait;
use std::path::PathBuf;
use std::time::Duration;

/// Everything a rescan needs that a check declaration does not carry.
///
/// A struct rather than three parameters on [`InWorkspace::new`] because they
/// belong to one thing — the artefact check — and because a caller with no
/// artefact check in its contract should be able to see, from the signature,
/// exactly what it is being asked for and why.
///
/// It deliberately does **not** carry a program. See the module header: the
/// program is the check's, on every call, and a field for it here is the one
/// change to this file that would silently disconnect the operator seam.
pub struct Rescan {
    /// Where the scanner's report is written, and where it stays.
    ///
    /// Owned by the caller rather than created here, for [`Wizcli`]'s reason: a
    /// scan's artefact has to outlive the scan long enough to be published as
    /// evidence, and exactly as long as the attempt that produced it. A
    /// directory this adapter created and dropped would take the report with it.
    pub scratch: PathBuf,

    /// The tenant credential every rescan in this contract authenticates with.
    pub credential: WizCredential,

    /// What to scan, in whatever spelling the scanner accepts.
    ///
    /// Supplied rather than derived. See the module header's second limit.
    pub image: String,
}

/// A tree under judgement that is a real worktree on disk.
///
/// Borrows the workspace rather than owning it because the workspace is the
/// attempt's — it was created before the contract was, its [`Drop`] removes the
/// worktree, and an evaluation is one of several things that happen inside it.
/// The cancellation token is the workspace's own, read through
/// [`Workspace::cancel`] rather than held again here, so a check and the
/// teardown that follows it cannot be under two different deadlines.
pub struct InWorkspace<'a> {
    workspace: &'a Workspace,
    timeout: Duration,
    rescan: Rescan,
}

impl<'a> InWorkspace<'a> {
    /// Judge `workspace`, giving every check `timeout` and every artefact check
    /// `rescan`.
    pub fn new(workspace: &'a Workspace, timeout: Duration, rescan: Rescan) -> Self {
        Self {
            workspace,
            timeout,
            rescan,
        }
    }
}

#[async_trait]
impl Tree for InWorkspace<'_> {
    /// Start the check's own program inside the worktree and wait for it.
    ///
    /// The command is built from [`Check::program`] and [`Check::args`] and from
    /// nothing else. What comes back is [`Workspace::run`]'s
    /// [`CommandResult`](crate::workspace::CommandResult) renamed: both streams
    /// have already had the attempt's absolute path rewritten out of them there,
    /// at the one place such a value comes into existence, so an
    /// [`Evaluation`](super::Evaluation) cannot be holding an unrelativised one.
    async fn run(&self, check: &Check) -> Result<Answered, Unanswered> {
        let command = WorkspaceCommand {
            program: check.program.clone(),
            args: check.args.clone(),
            timeout: self.timeout,
        };
        match self.workspace.run(&command).await {
            Ok(result) => Ok(Answered {
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
            }),

            // Nothing went wrong with the tree, so the whole evaluation is
            // abandoned rather than four results and a stub being recorded. The
            // runner does the abandoning; this only reports which kind it is.
            Err(WorkspaceError::Cancelled) => Err(Unanswered::Cancelled),

            // Killed at its deadline, so there is no observation — and no exit
            // status either, which is the reason it cannot be reported as a
            // failing check. See [`Unanswered::TimedOut`].
            Err(WorkspaceError::Timeout { program, timeout }) => {
                Err(Unanswered::TimedOut { program, timeout })
            }

            // The spawn itself failed. The `io::Error` travels rather than a
            // sentence, because the runner is what classifies it: an absent
            // `docker` and a `docker` this process may not execute are read off
            // `ErrorKind` there and would be prose to match on if they were
            // worded here.
            Err(WorkspaceError::Io { source, .. }) => Err(Unanswered::NotStarted {
                program: check.program.clone(),
                source,
            }),

            // `Workspace::run` returns none of the three remaining variants —
            // they belong to the path and repository questions the same type
            // answers — so this arm is unreachable rather than merely unlikely.
            // It is still written, and written to *refuse* the check, because
            // the alternative to an arm is a wildcard that would swallow a
            // seventh variant somebody adds later, and a gate that gets weaker
            // when a new failure appears is wrong in the direction that matters.
            Err(unreachable) => Err(Unanswered::NotStarted {
                program: check.program.clone(),
                source: std::io::Error::other(unreachable.to_string()),
            }),
        }
    }

    /// Run the check's own program as a scanner over this contract's image.
    ///
    /// The scanner is built here, per check, out of the check's program and
    /// arguments and this adapter's ambient [`Rescan`]. The module header argues
    /// for every word of that: a scanner built once and held would run its own
    /// program whatever the check declared.
    ///
    /// Everything after the construction is [`Wizcli`]'s — the credential, the
    /// five-name environment, the artefact this scan writes and the rule that
    /// reads it before the status line. This method restates none of it.
    async fn scan(&self, check: &Check) -> Result<ScanReport, ScanError> {
        // Checked before the scanner is built, not only raced against inside
        // `run_bounded`. A scan changes nothing outside the process, so this is
        // not about preventing an effect; it is that an attempt which has ended
        // must not go on starting children, and this is the only place that can
        // decline to.
        if self.workspace.cancel().is_cancelled() {
            return Err(ScanError::Failed {
                status: "the attempt was cancelled before the scanner started".to_string(),
                stderr: String::new(),
            });
        }

        Wizcli::new(
            PathBuf::from(&check.program),
            check.args.clone(),
            self.rescan.scratch.clone(),
            self.timeout,
            self.workspace.cancel().clone(),
            self.rescan.credential.clone(),
        )
        .scan(&self.rescan.image)
        .await
    }
}
