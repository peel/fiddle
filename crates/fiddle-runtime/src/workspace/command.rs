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
    /// `HOME` points at the workspace so that a tool which insists on writing a
    /// cache or a config lands inside the tree that gets thrown away, rather
    /// than in the operator's real home.
    ///
    /// Both bounds are `select!` arms rather than wrappers because they have to
    /// be able to interrupt the child, not merely stop waiting for it. Losing an
    /// arm drops the child future, and `kill_on_drop` turns that drop into a
    /// kill; without it a timed-out `sleep 30` would keep running with nobody
    /// left holding its handle.
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
            .env("HOME", &self.root)
            .env("PATH", &*TOOL_PATH)
            .env("LANG", "C")
            .kill_on_drop(true);

        let child = command.output();
        tokio::select! {
            _ = self.cancel.cancelled() => Err(WorkspaceError::Cancelled),
            _ = tokio::time::sleep(cmd.timeout) => Err(WorkspaceError::Timeout {
                program: cmd.program.clone(),
                timeout: cmd.timeout,
            }),
            out = child => {
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
