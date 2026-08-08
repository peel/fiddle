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

pub mod path;

pub use path::WorkspacePath;

use fiddle_core::AttemptId;
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

/// What can go wrong when a requested path is turned into a usable one.
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
    fixture: PathBuf,
    cancel: CancellationToken,
    removed: bool,
}

impl Workspace {
    /// Branch a fresh worktree of `fixture` at `root/<attempt>`.
    ///
    /// Detached on purpose: an attempt is not a branch, and leaving HEAD
    /// attached would make two concurrent attempts fight over the same ref.
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
            fixture: fixture.to_path_buf(),
            cancel,
            removed: false,
        })
    }

    /// Where this workspace lives on disk.
    pub fn root(&self) -> &Path {
        &self.root
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

    /// Remove the worktree, reporting whether git could.
    ///
    /// Idempotent, so that the explicit call on the happy path and the [`Drop`]
    /// guard that follows it cannot both ask git to remove the same worktree —
    /// the second attempt would fail on a path git no longer knows about.
    pub fn remove(&mut self) -> Result<(), WorkspaceError> {
        if self.removed {
            return Ok(());
        }
        self.removed = true;
        git(
            &self.fixture,
            &[
                "worktree",
                "remove",
                "--force",
                &self.root.to_string_lossy(),
            ],
        )?;
        Ok(())
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
    Ok(())
}
