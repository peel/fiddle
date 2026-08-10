//! Running a child process under a bound it cannot outlive.
//!
//! There are two places in this runtime where control passes to a program this
//! project did not write: a check runner inside a workspace
//! ([`workspace::command`](crate::workspace::command)) and the `gh` invocation
//! that carries the GitHub credential ([`github::cli`](crate::github::cli)).
//! They share nothing about *what* the child may see — the workspace builds a
//! four-name environment around a scratch `HOME`, `gh` builds a five-name one
//! with no `HOME` at all, and keeping those two sets apart is the whole of M1's
//! and M2's isolation argument. What they do share is the harder half: a child
//! that must die when its deadline passes or its attempt is cancelled, and that
//! must take its own descendants with it when it goes.
//!
//! That half lives here, once, so the second spawn site inherits the reasoning
//! rather than a copy of it. The environment is deliberately *not* this
//! module's business: a caller hands in a [`Command`] it has already built, and
//! the allowlist it built stays in the module that can argue for it.
//!
//! # Why the child gets a process group of its own
//!
//! `kill_on_drop` kills the direct child and nothing below it, and both callers
//! are exactly the shape that breaks on: `cargo test` compiles and then *spawns
//! test binaries*, and `gh` is free to fork whatever it likes. A timed-out
//! parent would be reported correctly and still leave processes behind.
//!
//! So the child is made the leader of a new process group with
//! `process_group(0)` — every descendant inherits that group unless it goes out
//! of its way not to — and both interrupting arms signal the *group* rather
//! than the child. The cost is real and is worth naming: the child no longer
//! shares this process's group, so a terminal `SIGINT` no longer reaches it,
//! and a `^C` that kills the runner outright would now orphan a running child
//! rather than take it down alongside. The trade is still the right way round —
//! the leak this closes happens on every timeout, the one it opens only when
//! the runner is killed without being asked to stop — but it puts an obligation
//! on whoever wires the CLI: a `SIGINT` handler has to cancel the token,
//! because cancellation is now the only channel that reaches a bounded child.
//!
//! A process-group id is not reused while the group still has members, so
//! signalling it after the leader has already died reaches exactly the
//! descendants that outlived it and nothing else. With no members left the call
//! fails with `ESRCH`, and there was nothing to kill.

use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// How a bounded run ended.
///
/// The two interrupted arms are separate values rather than errors because
/// each caller names them differently — a workspace check calls a deadline
/// `WorkspaceError::Timeout`, and `gh` calls it `GhError::Timeout`, which
/// classifies `Unknown` because a request may already have reached GitHub.
/// Deciding that here would put the interpretation in the one module that
/// cannot know which of the two it is running for.
///
/// **Every variant here is reached only after `spawn` returned a running child**,
/// and for the two interrupted ones that is the load-bearing fact rather than an
/// implementation detail. A caller whose child can change something outside this
/// process therefore knows, from the variant alone, that the change may already
/// have happened — which is why [`Bounded::CancelledAfterSpawn`] is spelled the
/// way it is. A cancellation *refused before* a spawn never comes through this
/// type at all: each caller checks its token itself, and that check is the only
/// place a cancellation is knowledge rather than an ambiguity.
#[derive(Debug)]
pub(crate) enum Bounded {
    /// The child ran to completion and this is what it left behind. A non-zero
    /// status is a *result*, not an error: it is the observation both callers
    /// exist to make.
    Finished(Output),
    /// The deadline passed and the whole process group was killed.
    TimedOut,
    /// The attempt was cancelled *while the child was running*, and the whole
    /// process group was killed.
    ///
    /// The name carries the provenance because the classification depends on it
    /// and nothing downstream can recover it: this arm and [`Bounded::TimedOut`]
    /// are produced by the same `reap` on the same already-spawned child, so a
    /// caller that treated the two differently with respect to *whether the
    /// request reached the far side* would be inventing a distinction the runner
    /// never made. M2's holistic review found exactly that — see
    /// [`GhError::CancelledAfterSpawn`](crate::github::GhError::CancelledAfterSpawn).
    CancelledAfterSpawn,
}

/// Spawn `command`, optionally feed it `stdin`, and wait for it under both a
/// timeout and a cancellation token.
///
/// The three stream dispositions are set here rather than left to the caller
/// because this function is what reads them back: inherited streams would let a
/// child read this process's terminal and write over its output, and
/// `wait_with_output` needs pipes to read at all. `stdin` is a pipe exactly
/// when there is something to write into it and `/dev/null` otherwise, so a
/// child that reads stdin without being given any sees EOF rather than blocking
/// on a terminal.
///
/// Both bounds are `select!` arms rather than wrappers because they have to be
/// able to interrupt the child, not merely stop waiting for it. Losing an arm
/// drops the child future, and `kill_on_drop` turns that drop into a kill;
/// without it a timed-out `sleep 30` would keep running with nobody left
/// holding its handle. [`reap`] then covers what `kill_on_drop` does not.
///
/// The returned `io::Error` is either the spawn failing or the wait failing.
/// Both are the runner itself breaking rather than the child reporting
/// something, so callers map them to the same variant.
pub(crate) async fn run_bounded(
    command: &mut Command,
    stdin: Option<Vec<u8>>,
    timeout: Duration,
    cancel: &CancellationToken,
) -> std::io::Result<Bounded> {
    command
        .stdin(match stdin {
            Some(_) => Stdio::piped(),
            None => Stdio::null(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command.spawn()?;
    // Read before the handle is consumed below, because the arms that never get
    // the handle back are the ones that need to name the group.
    let group = child.id();
    let pipe = child.stdin.take();

    // `wait_with_output` and not `wait`: the child's stdout is a pipe, and a
    // program that filled it while nobody was reading would deadlock against
    // its own bound rather than finish inside it. The write happens inside the
    // same future so that a child which refuses to read its input is bounded by
    // the same deadline as one which refuses to finish.
    let finished = async move {
        if let (Some(mut pipe), Some(bytes)) = (pipe, stdin) {
            use tokio::io::AsyncWriteExt;
            pipe.write_all(&bytes).await?;
            // Dropped rather than merely flushed: the child is waiting on EOF,
            // and a pipe that stays open is a hang the timeout would have to
            // clean up.
            pipe.shutdown().await?;
        }
        child.wait_with_output().await
    };

    tokio::select! {
        // Reached only from here, which is *after* the `spawn` above returned a
        // running child. That is the whole of the difference between this arm and
        // a caller's own pre-spawn cancellation check, and it is a difference
        // about the world rather than about the runner: whatever this child was
        // asked to do, it had already started doing it.
        _ = cancel.cancelled() => {
            reap(group);
            Ok(Bounded::CancelledAfterSpawn)
        },
        _ = tokio::time::sleep(timeout) => {
            reap(group);
            Ok(Bounded::TimedOut)
        },
        out = finished => out.map(Bounded::Finished),
    }
}

/// Kill whatever is still running in the child's process group.
///
/// Best-effort and deliberately unreported: this runs on a path that has
/// already decided what to tell the caller — a timeout or a cancellation — and
/// failing to signal a group that may already be empty is not a second thing to
/// say.
///
/// `None` means tokio had already reaped the child, so there is no group left
/// to name.
#[cfg(unix)]
pub(crate) fn reap(group: Option<u32>) {
    if let Some(pid) = group {
        // SAFETY: a plain signal call with no memory effects. `pid` is that of a
        // child this process spawned under `process_group(0)`, so it is also
        // that child's process-group id.
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        }
    }
}

/// Where process groups are not a concept, `kill_on_drop` remains the whole of
/// the guarantee and the limitation above stands.
#[cfg(not(unix))]
pub(crate) fn reap(_group: Option<u32>) {}
