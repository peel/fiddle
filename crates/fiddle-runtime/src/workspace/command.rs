//! Running something inside the workspace, under an environment built from
//! nothing.
//!
//! A check runner is the one place where the bounded rig hands control to a
//! process it did not write, so three properties have to hold at once and each
//! is a separate arm of [`Workspace::run`]: the child sees no credential this
//! process holds, it cannot outlive its deadline, and it cannot start after the
//! attempt has been cancelled.
//!
//! The environment is the part worth reading twice. It is *cleared* and then
//! rebuilt from an allowlist, rather than having known-dangerous names removed
//! from it. The difference only shows up in the future: a denylist protects the
//! credentials someone remembered, an allowlist protects the ones nobody has
//! added yet.

use super::{Workspace, WorkspaceError};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;

/// A program to run inside a workspace, with the bound it must finish within.
///
/// The timeout is a field rather than a runner-wide setting because the two
/// things that will run through here — a build and a test suite — are not the
/// same order of magnitude, and a single bound would have to be the looser one.
#[derive(Debug, Clone)]
pub struct WorkspaceCommand {
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Duration,
}

/// What a finished workspace command left behind.
///
/// A non-zero `exit_code` is a *result*, not an error: a failing `cargo test` is
/// exactly the observation the repair loop is asking for, and turning it into an
/// `Err` would put the interesting case on the path reserved for the runner
/// itself breaking.
#[derive(Debug, Clone)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// The `PATH` every workspace command sees.
///
/// Resolved once per process from this process's own `PATH`, falling back to a
/// fixed minimum when there is none. A constant would be tidier and is what the
/// plan first assumed, but it cannot work here: the toolchain a check runner
/// must find — `cargo`, `rustc`, `git` — is materialized by Nix at a
/// content-hashed store path that no constant can name, and under this project's
/// dev shell none of it is on `/usr/bin:/bin`. A later task runs
/// `cargo test --offline` inside a workspace; with a fixed `PATH` it would fail
/// to find cargo at all.
///
/// This is a deliberate narrowing of the "built from nothing" rule to exactly
/// one variable, and it is defensible because of what `PATH` is: a list of
/// directories to search, not a secret. It grants the child no authority it
/// could not already reach by absolute path, whereas an inherited
/// `LITELLM_API_KEY` would grant it the ability to spend money as us. Everything
/// that is credential-shaped stays outside.
///
/// Resolved once and cached so that the value cannot shift underneath two
/// commands of the same attempt.
static TOOL_PATH: LazyLock<String> = LazyLock::new(|| match std::env::var("PATH") {
    Ok(path) if !path.is_empty() => path,
    _ => MINIMUM_PATH.to_string(),
});

/// Where to look for tools when this process was started without a `PATH`.
const MINIMUM_PATH: &str = "/usr/bin:/bin";

impl Workspace {
    /// Run `cmd` in the workspace with an environment built from nothing.
    ///
    /// `env_clear` then an explicit allowlist: the parent environment is never
    /// consulted, so a credential added to the runner tomorrow is excluded by
    /// default rather than by remembering to deny it. `std::env::remove_var`
    /// would mutate this process and is wrong for a concurrent runtime — it
    /// would strip the credential from every other attempt running beside this
    /// one, and race with anything reading the environment meanwhile.
    ///
    /// `HOME` points at [`Workspace::home`] — a throwaway directory *beside* the
    /// worktree — so that a tool which insists on writing a cache or a config
    /// lands somewhere that is deleted with the attempt rather than in the
    /// operator's real home, and, just as importantly, not in the tree whose
    /// diff is the evidence.
    ///
    /// Both bounds are `select!` arms rather than wrappers because they have to
    /// be able to interrupt the child, not merely stop waiting for it. Losing an
    /// arm drops the child future, and `kill_on_drop` turns that drop into a
    /// kill; without it a timed-out `sleep 30` would keep running with nobody
    /// left holding its handle.
    ///
    /// # Why the child gets a process group of its own
    ///
    /// `kill_on_drop` kills the direct child and nothing below it, and the one
    /// command this runner exists to run is exactly the shape that breaks on:
    /// `cargo test` compiles and then *spawns test binaries*, so a timed-out
    /// check would kill cargo while its tests kept running. The runner would
    /// report `Timeout` perfectly correctly and still leave processes behind,
    /// holding a worktree that is about to be deleted underneath them.
    ///
    /// So the child is made the leader of a new process group with
    /// `process_group(0)` — every descendant inherits that group unless it goes
    /// out of its way not to — and both interrupting arms signal the *group*
    /// rather than the child. The cost is real and is worth naming: the child no
    /// longer shares this process's group, so a terminal `SIGINT` no longer
    /// reaches it, and a `^C` that kills the runner outright would now orphan a
    /// running check rather than take it down alongside. The trade is still the
    /// right way round — the leak this closes happens on every timeout, the one
    /// it opens only when the runner is killed without being asked to stop — but
    /// it puts an obligation on whoever wires the CLI: a `SIGINT` handler has to
    /// cancel the token, because cancellation is now the only channel that
    /// reaches a workspace command.
    ///
    /// A process-group id is not reused while the group still has members, so
    /// signalling it after the leader has already died reaches exactly the
    /// descendants that outlived it and nothing else. With no members left the
    /// call fails with `ESRCH`, and there was nothing to kill.
    pub async fn run(&self, cmd: &WorkspaceCommand) -> Result<CommandResult, WorkspaceError> {
        // Checked before spawning, not only raced against: cancellation has to
        // prevent the effect, and a command that has already started may have
        // written something before it dies.
        if self.cancel.is_cancelled() {
            return Err(WorkspaceError::Cancelled);
        }

        let mut command = tokio::process::Command::new(&cmd.program);
        command
            .args(&cmd.args)
            .current_dir(&self.root)
            .env_clear()
            .env("HOME", self.home())
            .env("PATH", &*TOOL_PATH)
            .env("LANG", "C")
            // Spelled out because this spawns rather than calling `output()`,
            // which would have set all three. Inherited streams would let a
            // check read this process's terminal and write over its output;
            // piped ones are also what `wait_with_output` reads back.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);

        let child = command.spawn().map_err(|source| WorkspaceError::Io {
            path: PathBuf::from(&cmd.program),
            source,
        })?;
        // Read before the handle is consumed below, because the arms that never
        // get the handle back are the ones that need to name the group.
        let group = child.id();
        // `wait_with_output` and not `wait`: the child's stdout is a pipe, and a
        // `cargo test` that filled it while nobody was reading would deadlock
        // against its own bound rather than finish inside it.
        let finished = child.wait_with_output();

        tokio::select! {
            _ = self.cancel.cancelled() => {
                reap(group);
                Err(WorkspaceError::Cancelled)
            },
            _ = tokio::time::sleep(cmd.timeout) => {
                reap(group);
                Err(WorkspaceError::Timeout {
                    program: cmd.program.clone(),
                    timeout: cmd.timeout,
                })
            },
            out = finished => {
                let out = out.map_err(|source| WorkspaceError::Io {
                    path: PathBuf::from(&cmd.program),
                    source,
                })?;
                Ok(CommandResult {
                    // A child killed by a signal has no exit code. `-1` is not a
                    // status any process can return, so it cannot be confused
                    // with one the command chose.
                    exit_code: out.status.code().unwrap_or(-1),
                    // Lossy on purpose: a compiler diagnostic quoting a source
                    // file with invalid UTF-8 in it is still the evidence the
                    // caller needs, and refusing to decode it would discard the
                    // whole run over one byte.
                    stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                })
            }
        }
    }
}

/// Kill whatever is still running in the child's process group.
///
/// Best-effort and deliberately unreported: this runs on a path that has already
/// decided what to tell the caller — a timeout or a cancellation — and failing
/// to signal a group that may already be empty is not a second thing to say.
///
/// `None` means tokio had already reaped the child, so there is no group left to
/// name.
#[cfg(unix)]
fn reap(group: Option<u32>) {
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
fn reap(_group: Option<u32>) {}
