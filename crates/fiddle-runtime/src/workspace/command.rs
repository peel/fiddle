//! Running something inside the workspace, under an environment built from
//! nothing.
//!
//! A check runner is one of the two places where this runtime hands control to
//! a process it did not write, so three properties have to hold at once: the
//! child sees no credential this process holds, it cannot outlive its deadline,
//! and it cannot start after the attempt has been cancelled. The first is this
//! module's, and the environment below is the whole statement of it. The other
//! two are common to every child the runtime starts and live in
//! [`crate::process`], which M2's `gh` invocation spawns through as well — the
//! bound is shared, the environment deliberately is not.
//!
//! The environment is the part worth reading twice. It is *cleared* and then
//! rebuilt from an allowlist, rather than having known-dangerous names removed
//! from it. The difference only shows up in the future: a denylist protects the
//! credentials someone remembered, an allowlist protects the ones nobody has
//! added yet.
//!
//! **The allowlist is four names, and this is the statement of it.** `HOME`,
//! pointed at the workspace's scratch home; `LANG`, fixed to `C`; `PATH`,
//! inherited from this process or [`MINIMUM_PATH`] when it has none; and
//! `RUSTUP_HOME`, inherited only when the parent has one and absent otherwise.
//! `workspace::a_workspace_command_inherits_no_credential` asserts both shapes
//! of that set exactly, so a fifth name cannot arrive without an assertion
//! changing. `docs/technical/SYSTEM.md`'s Invariants carry the same four for a
//! reader who is not in this file; every other mention in the repository points
//! at one of the two rather than restating a count.
//!
//! Two of the four come from the parent rather than from a
//! constant — [`TOOL_PATH`] and `RUSTUP_HOME` — and both are narrowings of
//! "built from nothing" rather than exceptions to it. The rule they are narrowed
//! by is stated once, here: **a locator may be inherited, an authority may
//! not.** `PATH` and `RUSTUP_HOME` say *where a toolchain is*; they grant a
//! child nothing it could not already reach by absolute path, and a child that
//! reads either learns the operator's directory layout and no more. A
//! `LITELLM_API_KEY` says *who the child may act as*, which is a different kind
//! of thing entirely and stays out. Everything credential-shaped stays out by
//! default, because the list is closed rather than filtered.

use super::{Workspace, WorkspaceError};
use crate::process::{run_bounded, Bounded};
use std::path::{Path, PathBuf};
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
///
/// # Both streams are already relativised
///
/// `stdout` and `stderr` are the child's, with this workspace's own absolute
/// path rewritten out of them — see [`relativised`]. That happens in
/// [`Workspace::run`], where the only value of this type is built, rather than
/// at the call sites that read one, and the difference is not cosmetic. When it
/// was a call site's job there were exactly two of them, both inside the
/// `run_check` tool; the capability's own verifying check ran through the same
/// function, embedded `stderr` in `CapabilityError::CheckFailed`, and published
/// the absolute worktree path in a report bundle — the surface the model is
/// protected from and the operator's reader is not. A guarantee that has to be
/// re-applied by each reader is a guarantee about today's readers.
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

/// Where a rustup-installed toolchain lives, when the parent names one.
///
/// `PATH` is necessary for a child to *find* `cargo` and, where cargo is a real
/// binary, sufficient for it to run. Where cargo is a **rustup proxy** it is
/// not: the proxy resolves which toolchain to exec through `RUSTUP_HOME`,
/// defaulting to `$HOME/.rustup` — and [`Workspace::run`] deliberately points
/// `HOME` at a per-attempt scratch directory that has no `.rustup` in it. The
/// proxy then exits non-zero with "no default toolchain configured", every
/// nested check fails for a reason that has nothing to do with the tree under
/// repair, and the capability reports `CheckFailed` over a repair that was
/// correct.
///
/// That failure is invisible wherever cargo is a real binary — a Nix dev shell,
/// for one — and appears on any machine that installed Rust through rustup,
/// which includes `dtolnay/rust-toolchain` and therefore this project's own
/// merge gate. `a_toolchain_proxy_finds_its_toolchain_because_rustup_home_survives`
/// is what keeps it from becoming invisible again.
///
/// Read at spawn rather than resolved once into a [`LazyLock`] the way
/// [`TOOL_PATH`] is, and the asymmetry is deliberate: `TOOL_PATH` *computes* a
/// value — it falls back to [`MINIMUM_PATH`] — so caching it is what stops two
/// commands of one attempt from disagreeing about a fallback. There is nothing
/// to compute here. The variable is passed through when the parent has one and
/// is simply absent when it does not, so caching would only make the child's
/// view a function of which command in the process happened to run first.
const RUSTUP_HOME: &str = "RUSTUP_HOME";

impl Workspace {
    /// Run `cmd` in the workspace with an environment built from nothing.
    ///
    /// `env_clear` then an explicit allowlist: the parent environment is
    /// consulted for exactly the two locators the module doc argues for — `PATH`
    /// and `RUSTUP_HOME` — and for nothing else, so a credential added to the
    /// runner tomorrow is excluded by default rather than by remembering to deny
    /// it. `std::env::remove_var` would mutate this process and is wrong for a
    /// concurrent runtime — it would strip the credential from every other
    /// attempt running beside this one, and race with anything reading the
    /// environment meanwhile.
    ///
    /// `HOME` points at [`Workspace::home`] — a throwaway directory *beside* the
    /// worktree — so that a tool which insists on writing a cache or a config
    /// lands somewhere that is deleted with the attempt rather than in the
    /// operator's real home, and, just as importantly, not in the tree whose
    /// diff is the evidence.
    ///
    /// # What this function is, and what it is not
    ///
    /// Everything above the spawn is this module's alone: the four-name
    /// environment, the workspace root as the working directory, and the
    /// relativisation applied on the way out. Everything below it — the process
    /// group, the deadline, the cancellation arm and the group kill — belongs to
    /// [`crate::process::run_bounded`], which M2's `gh` invocation spawns
    /// through as well. The split is deliberate and is the one the isolation
    /// argument depends on: the *bound* is common to every child this runtime
    /// starts, while the *environment* is specific to each spawn site and must
    /// stay where it can be argued for. `gh` sees five names and no `HOME`; a
    /// workspace command sees these four; neither set can drift into the other
    /// by sharing a runner.
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
            .env("LANG", "C");
        // The one conditional entry on the allowlist, and conditional in the
        // honest direction: present only when the parent has one, never
        // fabricated. See [`RUSTUP_HOME`] for why a toolchain locator is
        // inheritable where a credential is not.
        if let Ok(rustup_home) = std::env::var(RUSTUP_HOME) {
            command.env(RUSTUP_HOME, rustup_home);
        }

        // No stdin: a check runner is unattended, and a program that waits on a
        // terminal it does not have would hang until the deadline rather than
        // report anything.
        let bounded = run_bounded(&mut command, None, cmd.timeout, &self.cancel)
            .await
            .map_err(|source| WorkspaceError::Io {
                path: PathBuf::from(&cmd.program),
                source,
            })?;

        match bounded {
            Bounded::Cancelled => Err(WorkspaceError::Cancelled),
            Bounded::TimedOut => Err(WorkspaceError::Timeout {
                program: cmd.program.clone(),
                timeout: cmd.timeout,
            }),
            Bounded::Finished(out) => Ok(CommandResult {
                // A child killed by a signal has no exit code. `-1` is not a
                // status any process can return, so it cannot be confused with
                // one the command chose.
                exit_code: out.status.code().unwrap_or(-1),
                // Lossy on purpose: a compiler diagnostic quoting a source file
                // with invalid UTF-8 in it is still the evidence the caller
                // needs, and refusing to decode it would discard the whole run
                // over one byte.
                //
                // Relativised here, at the one place a `CommandResult` comes
                // into existence, so that no reader of one can be holding an
                // unrelativised stream. See [`CommandResult`].
                stdout: relativised(&String::from_utf8_lossy(&out.stdout), &self.root),
                stderr: relativised(&String::from_utf8_lossy(&out.stderr), &self.root),
            }),
        }
    }
}

/// Rewrite the workspace's absolute path out of a child process's output.
///
/// Check runners announce where they are working — `cargo` prints
/// `Compiling foo v0.1.0 (/…/ws/<attempt>)` on every build — so carrying the
/// output verbatim hands the operator's directory layout to whoever reads it
/// next, without anybody deciding to. There are two such readers and they are
/// easy to mistake for one: the **model**, through `run_check`'s output, and a
/// **published bundle**, through the `stderr` a failing check puts in
/// `CapabilityError::CheckFailed`. Rewriting the prefix to `.` costs nothing
/// diagnostically and gains something for the first of them: what is left is
/// the relative path the model can pass straight back to `read_file`.
///
/// Both spellings of the root are rewritten, and the longer one first. macOS's
/// temporary directories live under `/var`, which is a symlink to
/// `/private/var`, so a child resolving its own working directory reports a path
/// that is not the string the workspace was created with — and stripping only
/// the string it was created with would strip nothing at all. Longest-first
/// matters because one spelling is a suffix-extension of the other: rewriting
/// `/var/…` first would leave `/private.` behind.
///
/// This is a prefix rewrite, not a redactor. A child is free to print an
/// absolute path of its own choosing — a toolchain in the Nix store, a registry
/// checkout in `~/.cargo` — and nothing here can stop it; what it cannot do is
/// reveal where this attempt is working.
fn relativised(text: &str, root: &Path) -> String {
    let mut spellings = Vec::new();
    if let Ok(canonical) = root.canonicalize() {
        spellings.push(canonical.display().to_string());
    }
    spellings.push(root.display().to_string());
    spellings.sort_by_key(|spelling| std::cmp::Reverse(spelling.len()));

    let mut text = text.to_string();
    for spelling in spellings {
        if !spelling.is_empty() {
            text = text.replace(&spelling, ".");
        }
    }
    text
}
