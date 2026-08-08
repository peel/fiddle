//! The bounded rig's view of the filesystem.
//!
//! Everything a model asks for arrives as a string, and a string is not proof
//! of anything. This module is where such a string becomes a value that carries
//! its own guarantee: [`path`] turns a requested path into a [`WorkspacePath`],
//! which by construction names something inside the workspace and nothing
//! outside it. ADR 011 records what happens when a derived path is trusted
//! instead of proven, so containment is a property of the type here too, not a
//! check each call site is expected to remember.
//!
//! [`Workspace`] is the tree those paths are relative to: a detached git
//! worktree of the repository under repair, created per attempt and removed when
//! the attempt ends however it ends. Two guarantees are split across the two
//! halves deliberately. [`WorkspacePath::parse`] is syntactic, so it cannot be
//! defeated by a race; [`Workspace::resolve`] is the last word, because only the
//! filesystem knows where a symlink points.

//!
//! [`command`] is the other half of that containment: a path check is worth
//! nothing if the process the workspace hands control to can read the
//! credentials of the process that started it, so a workspace command's
//! environment is built from an allowlist rather than inherited.
//!
//! `changes` closes the loop by answering what an attempt actually did, from the
//! repository rather than from the agent's account of itself — which is why the
//! worktree holds only the repository. Anything a command writes because it was
//! given a `HOME` goes to [`Workspace::home`], a scratch directory beside the
//! tree: a cargo cache inside the tree would be reported as work an agent did.

mod changes;
pub mod command;
pub mod path;

pub use command::{CommandResult, WorkspaceCommand};
pub use path::WorkspacePath;

use fiddle_core::AttemptId;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// What can go wrong when a requested path is turned into a usable one, or when
/// a command is run against it.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    /// The requested path was refused because it does not provably name
    /// something inside the workspace. `reason` names the rule that fired, so
    /// an operator reading the diagnostic learns which shape was rejected
    /// rather than only that something was.
    #[error("path {path} escapes the workspace: {reason}")]
    Escape { path: String, reason: String },

    /// The path was legal but the filesystem operation on it failed.
    #[error("io error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A git invocation the workspace depends on failed. `stderr` is carried
    /// verbatim because git's own diagnostic is more specific than anything this
    /// layer could say about, say, a worktree path that is already registered.
    #[error("git {command} failed: {stderr}")]
    Git { command: String, stderr: String },

    /// A workspace command did not finish within its bound and was killed.
    ///
    /// The program is named because "something timed out" is not actionable:
    /// an attempt runs a build and a test suite through the same runner, and
    /// which of them hung is the first thing an operator needs to know.
    #[error("{program} did not finish within {timeout:?} and was killed")]
    Timeout { program: String, timeout: Duration },

    /// The attempt was cancelled, so the work was not done.
    ///
    /// Distinct from a failure on purpose: nothing went wrong, and an outcome
    /// derived from this must not read as the capability having tried and lost.
    #[error("cancelled")]
    Cancelled,
}

/// One attempt's private checkout of the repository under repair.
///
/// The checkout is a detached git worktree rather than a copy, so it shares the
/// object store with the repository it came from while its working tree, its
/// index, and its HEAD are its own — which is what makes a write here provably
/// invisible to the fixture and to every other attempt.
///
/// The worktree is removed on [`Drop`] as well as by [`Workspace::remove`]. The
/// explicit call exists so that a failure to tear down can be *reported*; the
/// guard exists because an early return, a `?`, or a panic would otherwise leak
/// a directory that the next attempt's `worktree add` would then collide with.
pub struct Workspace {
    root: PathBuf,
    home: PathBuf,
    fixture: PathBuf,
    cancel: CancellationToken,
    removed: bool,
}

impl Workspace {
    /// Branch a fresh worktree of `fixture` at `root/<attempt>`.
    ///
    /// Detached on purpose: an attempt is not a branch, and leaving HEAD
    /// attached would make two concurrent attempts fight over the same ref.
    ///
    /// A scratch home is created beside the worktree at the same time, because
    /// a workspace command needs somewhere to be pointed at that is throwaway
    /// *and* is not the tree whose changes are the evidence — see
    /// [`Workspace::home`].
    pub fn create(
        fixture: &Path,
        root: &Path,
        attempt: &AttemptId,
        cancel: CancellationToken,
    ) -> Result<Self, WorkspaceError> {
        std::fs::create_dir_all(root).map_err(|source| WorkspaceError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let path = root.join(attempt.0.as_str());
        let home = root.join(format!("{}.home", attempt.0));
        std::fs::create_dir_all(&home).map_err(|source| WorkspaceError::Io {
            path: home.clone(),
            source,
        })?;
        git(
            fixture,
            &[
                "worktree",
                "add",
                "--detach",
                "-q",
                &path.to_string_lossy(),
                "HEAD",
            ],
        )?;
        Ok(Workspace {
            root: path,
            home,
            fixture: fixture.to_path_buf(),
            cancel,
            removed: false,
        })
    }

    /// Where this workspace lives on disk.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The `HOME` a command run inside this workspace is given.
    ///
    /// Beside the worktree rather than inside it, and that placement is load
    /// bearing. Pointing `HOME` at the worktree contains a tool's caches, which
    /// is what it was for — but it also puts them *in the tree whose diff is the
    /// evidence*. Cargo is the case that proves it: with `HOME` set to the
    /// worktree, one `cargo test` leaves `.cargo/.package-cache`,
    /// `.cargo/.global-cache` and `.cargo/.package-cache-mutate` behind, and
    /// [`Workspace::changed_files`] then reports three files an agent never
    /// touched — spending the changed-file cap on noise and putting fabricated
    /// paths into published evidence. A repository cannot defend itself against
    /// that by gitignoring names it does not know about, so the fix belongs
    /// here.
    ///
    /// Still inside the per-attempt directory, so it is still thrown away with
    /// everything else: containment is kept, the diff is not polluted.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// The token that cancels the work running against this workspace.
    ///
    /// Held by the workspace so that whatever executes inside it inherits the
    /// same deadline as the teardown that will follow.
    pub fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    /// The absolute path `path` names, proven inside this workspace.
    ///
    /// [`WorkspacePath`] already guarantees no `..`, so the only remaining way
    /// out is a symlink. `canonicalize` follows every link and containment is
    /// checked against the *result* — which is why this, not the parse, is the
    /// last word.
    pub fn resolve(&self, path: &WorkspacePath) -> Result<PathBuf, WorkspaceError> {
        let escape = |reason: &str| WorkspaceError::Escape {
            path: path.as_str().to_string(),
            reason: reason.to_string(),
        };
        let root = self
            .root
            .canonicalize()
            .map_err(|source| WorkspaceError::Io {
                path: self.root.clone(),
                source,
            })?;
        let joined = root.join(path.as_str());
        let resolved = match joined.canonicalize() {
            Ok(resolved) => resolved,
            // The leaf may legitimately not exist yet (a create). Resolve its
            // parent — which must itself be inside — and re-attach the name.
            Err(_) => {
                // Unless something *is* there and merely would not resolve. A
                // dangling symlink is the case that matters: canonicalize fails,
                // yet `std::fs::write` would happily follow it and create the
                // file at the far end, outside. Refusing here is what stops the
                // not-yet-exists branch from becoming the escape hatch.
                if joined.symlink_metadata().is_ok() {
                    return Err(escape("the path is a link that does not resolve"));
                }
                let parent = joined
                    .parent()
                    .unwrap_or(&root)
                    .canonicalize()
                    .map_err(|source| WorkspaceError::Io {
                        path: joined.clone(),
                        source,
                    })?;
                if !parent.starts_with(&root) {
                    return Err(escape(
                        "the parent directory resolves outside the workspace",
                    ));
                }
                parent.join(joined.file_name().unwrap_or_default())
            }
        };
        if !resolved.starts_with(&root) {
            return Err(escape("the path resolves outside the workspace"));
        }
        Ok(resolved)
    }

    /// Read a file from inside the workspace.
    pub fn read(&self, path: &WorkspacePath) -> Result<String, WorkspaceError> {
        let resolved = self.resolve(path)?;
        std::fs::read_to_string(&resolved).map_err(|source| WorkspaceError::Io {
            path: resolved,
            source,
        })
    }

    /// Write a file inside the workspace.
    ///
    /// [`Workspace::resolve`] runs first and its failure returns, so a path that
    /// resolves outside is refused *before* any file is opened. That ordering is
    /// the whole protection: `std::fs::write` follows a symlink and writes
    /// through it, so a check made afterwards would be a check made too late.
    pub fn write(&self, path: &WorkspacePath, contents: &str) -> Result<(), WorkspaceError> {
        let resolved = self.resolve(path)?;
        std::fs::write(&resolved, contents).map_err(|source| WorkspaceError::Io {
            path: resolved,
            source,
        })
    }

    /// Remove the worktree and the scratch home, reporting whether it could.
    ///
    /// Idempotent, so that the explicit call on the happy path and the [`Drop`]
    /// guard that follows it cannot both ask git to remove the same worktree —
    /// the second attempt would fail on a path git no longer knows about.
    ///
    /// Both halves are attempted whichever fails: the scratch home is an
    /// ordinary directory git knows nothing about, so a `worktree remove` that
    /// failed must not be allowed to leave it behind as well.
    pub fn remove(&mut self) -> Result<(), WorkspaceError> {
        if self.removed {
            return Ok(());
        }
        self.removed = true;
        let worktree = git(
            &self.fixture,
            &[
                "worktree",
                "remove",
                "--force",
                &self.root.to_string_lossy(),
            ],
        );
        let home = match std::fs::remove_dir_all(&self.home) {
            Err(source) if source.kind() != std::io::ErrorKind::NotFound => {
                Err(WorkspaceError::Io {
                    path: self.home.clone(),
                    source,
                })
            }
            _ => Ok(()),
        };
        worktree.and(home)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        // Best-effort: a Drop cannot report, but a leaked worktree would make the
        // next attempt's `worktree add` fail on a path that already exists.
        let _ = self.remove();
    }
}

/// Run git in `dir`, turning a non-zero exit into a [`WorkspaceError::Git`].
fn git(dir: &Path, args: &[&str]) -> Result<(), WorkspaceError> {
    git_stdout(dir, args).map(|_| ())
}

/// As [`git`], but hands back what git said on stdout.
///
/// Bytes rather than a `String`: git reports paths, and a path is a sequence of
/// bytes that is not obliged to be valid UTF-8. Decoding here would force a
/// choice between refusing every status because of one odd filename and
/// replacing it with U+FFFD; the caller that knows which paths it is looking at
/// is better placed to decide than this one is.
fn git_stdout(dir: &Path, args: &[&str]) -> Result<Vec<u8>, WorkspaceError> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|source| WorkspaceError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
    if !output.status.success() {
        return Err(WorkspaceError::Git {
            command: args.join(" "),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(output.stdout)
}
