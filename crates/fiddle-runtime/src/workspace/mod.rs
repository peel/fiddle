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
//!
//! # What the workspace knows without asking
//!
//! Deriving the answer from git rather than from the agent is only half of
//! independence. The other half is *which rules git is asked under*, because a
//! checkout contains the file that carries them. An agent that writes
//! `.gitignore` is writing an input to `git status`, so a derivation made under
//! the worktree's current rules is one the agent has a hand in — and the
//! changed-file set is the cap and the published evidence both.
//!
//! One decision settles it for every question asked here: the rules are the
//! project's, **as committed at the HEAD this worktree was branched from**,
//! captured before the attempt began and kept outside the tree. See
//! [`Workspace::baseline_ignore`]. It is what makes [`Workspace::changed_files`]
//! independent, what keeps a build tree out of [`Workspace::list`] without
//! letting an agent decide the same, and what tells [`Workspace::read`] which
//! files are the project at all.

mod changes;
pub mod command;
pub mod path;

pub use command::{CommandResult, WorkspaceCommand};
pub use path::WorkspacePath;

use fiddle_core::AttemptId;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// The ignore file whose committed contents are the project's own rules.
///
/// The repository root's, and only that one — see
/// [`Workspace::baseline_ignore`] for why nested ones are left out and which way
/// that errs.
const IGNORE_FILE: &str = ".gitignore";

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

    /// The path is inside the workspace and names something that is not part of
    /// the project the workspace holds — the repository's own metadata, or a
    /// file the project's committed ignore rules exclude.
    ///
    /// Distinct from [`WorkspaceError::Escape`] because it is a different claim
    /// about a different thing: an escape is a containment failure, and this is
    /// a path that is contained perfectly well and still names none of the
    /// project. Collapsing them would make an operator reading a refusal unable
    /// to tell an attempted breakout from a `read_file("target/…")`.
    #[error("path {path} is not part of the project: {reason}")]
    NotProject { path: String, reason: String },

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
    baseline_ignore: PathBuf,
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
    ///
    /// The project's ignore rules are snapshotted beside it, before anything has
    /// run — see [`Workspace::baseline_ignore`]. Doing it here rather than at
    /// the point of use is the whole of the guarantee: what is captured is the
    /// state of the repository as it was handed over.
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
        let baseline_ignore = root.join(format!("{}.ignore", attempt.0));
        let workspace = Workspace {
            root: path,
            home,
            fixture: fixture.to_path_buf(),
            baseline_ignore,
            cancel,
            removed: false,
        };
        // After the struct exists, so that a failure here still tears the
        // worktree down through the Drop guard rather than leaking it.
        workspace.snapshot_baseline_ignore()?;
        Ok(workspace)
    }

    /// Capture the project's committed ignore rules where nothing this attempt
    /// does can reach them.
    ///
    /// `ls-tree` first and `show` second rather than a `show` whose failure is
    /// swallowed: a repository with no ignore file at all and a git that could
    /// not answer are different situations, and treating the second as the
    /// first would silently widen the evidence with no diagnostic anywhere.
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

    /// The exclude file every derivation about this workspace is made under.
    ///
    /// **The rules that decide what counts as the project must not be rules the
    /// attempt can write.** `.gitignore` is an ordinary versioned file: it
    /// parses, it resolves, and `write_file` will write it. So an attempt that
    /// wrote `*` into it and then created ten files would, under
    /// `--exclude-standard`, have git report one change — the changed-file cap
    /// bypassed, and a published evidence reference naming a count that is not
    /// true. Adding `--ignored` would answer that and lose the thing the
    /// exclusion is *for*: one `cargo test` writes thousands of files nobody
    /// edited, and evidence drowned in build output says as little as evidence
    /// suppressed by the model.
    ///
    /// So neither. What is used instead is the ignore file **as committed at the
    /// HEAD this worktree was branched from**, snapshotted into a file outside
    /// the worktree before the attempt began. It excludes exactly what the
    /// project says it excludes, an attempt cannot add to it or take from it,
    /// and it says the same thing on every machine — `--exclude-standard` would
    /// also honour the operator's global excludes and this repository's
    /// `.git/info/exclude`, which would make one attempt's evidence depend on
    /// whose laptop it ran on.
    ///
    /// Two things it deliberately does not do. Ignore files in *subdirectories*
    /// are not honoured, because `--exclude-from` reads one flat list whose
    /// patterns are all relative to the top and concatenating nested files would
    /// change what they mean; the error is therefore towards reporting more,
    /// which is the safe direction. And a file written into a path the project
    /// already excludes — `target/something` — is still not counted, which is
    /// the residue of the same trade: the exclusion is the project's own
    /// declaration, made before the attempt, and an attempt that hides work
    /// where the project keeps no source has still earned nothing, because the
    /// check decides the verdict.
    pub fn baseline_ignore(&self) -> &Path {
        &self.baseline_ignore
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

    /// Read a file of the project from inside the workspace.
    ///
    /// Containment is not the whole question, and treating it as though it were
    /// is what let `read_file(".git")` return the operator's directory layout.
    /// A workspace is a checkout, and a checkout holds three kinds of thing: the
    /// project, the repository's own metadata, and whatever a build left behind.
    /// Only the first is what "the project you are repairing" means, and only
    /// the first is served — the second is refused at
    /// [`WorkspacePath::parse`](crate::workspace::WorkspacePath::parse), and the
    /// third is refused here.
    ///
    /// Refused here rather than by a rule about names, because the third kind
    /// has no name to write down: a dependency file cargo fills with absolute
    /// host paths is `target/debug/fixture.d` in this project and something else
    /// in the next one. What every member of it *does* have in common is that
    /// the project's own committed rules exclude it, which is a question git can
    /// answer — see [`Workspace::baseline_ignore`] for why the answer is taken
    /// under those rules and not the worktree's current ones.
    ///
    /// The cost is one `git ls-files` per read. That is a subprocess for a
    /// question the filesystem could not have answered, on a path taken a
    /// handful of times per attempt.
    pub fn read(&self, path: &WorkspacePath) -> Result<String, WorkspaceError> {
        // Containment first, so that a path resolving outside is still reported
        // as the escape it is rather than as a file the project happens not to
        // contain.
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
    /// Every half is attempted whichever fails: the scratch home and the ignore
    /// snapshot are ordinary paths git knows nothing about, so a
    /// `worktree remove` that failed must not be allowed to leave them behind as
    /// well.
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
        // Closures rather than the functions themselves: both are generic over
        // `AsRef<Path>`, so passing the item directly leaves the compiler unable
        // to prove it holds for every lifetime.
        let home = discarded(&self.home, |path| std::fs::remove_dir_all(path));
        let baseline = discarded(&self.baseline_ignore, |path| std::fs::remove_file(path));
        worktree.and(home).and(baseline)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        // Best-effort: a Drop cannot report, but a leaked worktree would make the
        // next attempt's `worktree add` fail on a path that already exists.
        let _ = self.remove();
    }
}

/// Throw `path` away with `remove`, treating an absent path as already gone.
///
/// Idempotence is the point: [`Workspace::remove`] runs on the happy path and
/// again from the [`Drop`] guard, and a second removal must be a no-op rather
/// than a failure about a path nobody expects to still be there.
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
