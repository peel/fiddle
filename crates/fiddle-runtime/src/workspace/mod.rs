mod changes;
pub mod command;
pub mod declared;
pub mod path;

pub use changes::{Content, FileEdit};
pub use command::{CommandResult, WorkspaceCommand};
pub use declared::{DeclaredCommand, Extend, Undeclared};
pub use path::WorkspacePath;

use fiddle_core::AttemptId;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const IGNORE_FILE: &str = ".gitignore";

#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    #[error("path {path} escapes the workspace: {reason}")]
    Escape { path: String, reason: String },

    #[error("path {path} is not part of the project: {reason}")]
    NotProject { path: String, reason: String },

    #[error("io error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("git {command} failed: {stderr}")]
    Git { command: String, stderr: String },

    #[error("{program} did not finish within {timeout:?} and was killed")]
    Timeout { program: String, timeout: Duration },

    #[error("cancelled")]
    Cancelled,
}

pub struct Workspace {
    root: PathBuf,
    home: PathBuf,
    fixture: PathBuf,
    baseline_ignore: PathBuf,
    cancel: CancellationToken,
    removed: bool,
}

impl Workspace {
    pub fn create(
        fixture: &Path,
        root: &Path,
        attempt: &AttemptId,
        cancel: CancellationToken,
    ) -> Result<Self, WorkspaceError> {
        Workspace::create_at(fixture, root, attempt, "HEAD", cancel)
    }

    pub fn create_at(
        fixture: &Path,
        root: &Path,
        attempt: &AttemptId,
        revision: &str,
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
                revision,
            ],
        )?;
        let baseline_ignore = root.join(format!("{}.ignore", attempt.0));
        let workspace = Workspace {
            root: path,
            home,
            fixture: fixture.to_path_buf(),
            baseline_ignore,
            cancel,
            removed: false,
        };
        workspace.snapshot_baseline_ignore()?;
        Ok(workspace)
    }

    fn snapshot_baseline_ignore(&self) -> Result<(), WorkspaceError> {
        let committed = git_stdout(
            &self.root,
            &["ls-tree", "-z", "--name-only", "HEAD", "--", IGNORE_FILE],
        )?;
        let rules = if committed.is_empty() {
            Vec::new()
        } else {
            git_stdout(&self.root, &["show", &format!("HEAD:{IGNORE_FILE}")])?
        };
        std::fs::write(&self.baseline_ignore, rules).map_err(|source| WorkspaceError::Io {
            path: self.baseline_ignore.clone(),
            source,
        })
    }

    pub fn baseline_ignore(&self) -> &Path {
        &self.baseline_ignore
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn cancel(&self) -> &CancellationToken {
        &self.cancel
    }

    pub fn resolve(&self, path: &WorkspacePath) -> Result<PathBuf, WorkspaceError> {
        let escape = |reason: &str| WorkspaceError::Escape {
            path: path.as_str().to_string(),
            reason: reason.to_string(),
        };
        let root = self.canonical_root()?;

        let mut resolved = root.clone();
        let mut components = path.as_str().split('/').peekable();
        while let Some(component) = components.next() {
            let leaf = components.peek().is_none();
            let candidate = resolved.join(component);
            match candidate.symlink_metadata() {
                Ok(_) => match candidate.canonicalize() {
                    Ok(real) if real.starts_with(&root) => resolved = real,
                    Ok(_) if leaf => return Err(escape("the path resolves outside the workspace")),
                    Ok(_) => {
                        return Err(escape(
                            "the parent directory resolves outside the workspace",
                        ))
                    }
                    Err(_) => return Err(escape("the path is a link that does not resolve")),
                },
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                    resolved.push(component);
                    break;
                }
                Err(source) => {
                    return Err(WorkspaceError::Io {
                        path: candidate,
                        source,
                    })
                }
            }
        }
        for component in components {
            resolved.push(component);
        }

        if !resolved.starts_with(&root) {
            return Err(escape("the path resolves outside the workspace"));
        }
        Ok(resolved)
    }

    fn canonical_root(&self) -> Result<PathBuf, WorkspaceError> {
        self.root
            .canonicalize()
            .map_err(|source| WorkspaceError::Io {
                path: self.root.clone(),
                source,
            })
    }

    pub fn read(&self, path: &WorkspacePath) -> Result<String, WorkspaceError> {
        let resolved = self.resolve(path)?;
        if !self.list()?.contains(path) {
            return Err(WorkspaceError::NotProject {
                path: path.as_str().to_string(),
                reason: "the project does not contain that file".to_string(),
            });
        }
        std::fs::read_to_string(&resolved).map_err(|source| WorkspaceError::Io {
            path: resolved,
            source,
        })
    }

    pub fn write(&self, path: &WorkspacePath, contents: &str) -> Result<(), WorkspaceError> {
        let resolved = self.prepared(path, self.resolve(path)?)?;
        std::fs::write(&resolved, contents).map_err(|source| WorkspaceError::Io {
            path: resolved,
            source,
        })
    }

    fn prepared(&self, path: &WorkspacePath, resolved: PathBuf) -> Result<PathBuf, WorkspaceError> {
        let (Some(parent), Some(leaf)) = (resolved.parent(), resolved.file_name()) else {
            return Ok(resolved);
        };
        std::fs::create_dir_all(parent).map_err(|source| WorkspaceError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        let root = self.canonical_root()?;
        let parent = parent.canonicalize().map_err(|source| WorkspaceError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        if !parent.starts_with(&root) {
            return Err(WorkspaceError::Escape {
                path: path.as_str().to_string(),
                reason: "the parent directory resolves outside the workspace".to_string(),
            });
        }
        Ok(parent.join(leaf))
    }

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
        let home = discarded(&self.home, |path| std::fs::remove_dir_all(path));
        let baseline = discarded(&self.baseline_ignore, |path| std::fs::remove_file(path));
        worktree.and(home).and(baseline)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

fn discarded<F>(path: &Path, remove: F) -> Result<(), WorkspaceError>
where
    F: FnOnce(&Path) -> std::io::Result<()>,
{
    match remove(path) {
        Err(source) if source.kind() != std::io::ErrorKind::NotFound => Err(WorkspaceError::Io {
            path: path.to_path_buf(),
            source,
        }),
        _ => Ok(()),
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<(), WorkspaceError> {
    git_stdout(dir, args).map(|_| ())
}

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
