//! Turning a requested path into one that provably stays inside the workspace,
//! and names something the workspace is *for*.
//!
//! Containment is the older of the two rules and the more obvious one. The
//! second is that a workspace is a checkout of a project, and a checkout carries
//! the repository's own bookkeeping alongside it — which is inside the tree, is
//! reached by an ordinary relative path, and is not the project. See
//! [`GIT_DIR`].

use super::WorkspaceError;

/// The one name the repository's own bookkeeping goes by.
///
/// A path with this component is refused wherever it appears, for reading and
/// for writing alike, and the rule is about a *class* rather than a filename:
/// `.git` is the repository's private state, which is not part of the project
/// under repair in any sense a tool should honour. What is in the class with it
/// is everything underneath — `.git/config`, `.git/info/exclude`, `.git/hooks/`
/// — and the same directory as a submodule carries it, which is why the match is
/// on any component and not on a prefix.
///
/// The concrete harm, in the layout every attempt actually runs in: a linked
/// worktree's `.git` is a regular **file** whose whole contents are
/// `gitdir: <absolute host path>`. One `read_file(".git")` therefore hands the
/// model the operator's directory layout, the fixture repository's location and
/// the attempt's own identity, in a tool's *success* output — the surface
/// `relativised` does not cover and no schema assertion can see. The write side
/// matters for a different reason: `.git/info/exclude` and `.git/config` are
/// inputs to git's own answers, and the changed-file evidence is one of those
/// answers.
///
/// Compared case-insensitively because macOS and Windows checkouts live on
/// case-insensitive filesystems, where `.GIT/config` opens the same file.
/// git's own defence against this goes further still — it also refuses NTFS
/// short names and HFS+ ignorable code points — and this is deliberately the
/// ASCII fold rather than a reimplementation of that: the containment check in
/// [`super::Workspace::resolve`] is what keeps an exotic spelling inside the
/// workspace, and this is what keeps the ordinary ones out of the metadata.
const GIT_DIR: &str = ".git";

/// A path a model asked for, proven to name something inside the workspace.
///
/// Validation is syntactic and total: no filesystem access, so it cannot be
/// defeated by a race between checking and using. Anything that survives is a
/// plain relative path with no `..` component at all, which is what lets
/// `Workspace::resolve` do one join and one containment check rather than
/// reasoning about what the filesystem looked like a moment ago.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct WorkspacePath(String);

impl WorkspacePath {
    /// Refuse anything that is not plainly workspace-relative.
    ///
    /// The refusals are shapes rather than resolutions: an interior `..` that
    /// happens to stay inside is still rejected, because deciding that would
    /// require this function and the filesystem to agree about symlinks, and
    /// they do not.
    pub fn parse(raw: &str) -> Result<Self, WorkspaceError> {
        let escape = |reason: &str| WorkspaceError::Escape {
            path: raw.to_string(),
            reason: reason.to_string(),
        };

        if raw.is_empty() {
            return Err(escape("the path is empty"));
        }
        if raw.contains('\0') {
            return Err(escape("the path contains a NUL byte"));
        }
        if raw.starts_with('/') || raw.starts_with('\\') {
            return Err(escape("the path is absolute"));
        }
        // `C:` and friends, checked explicitly rather than via the host platform's
        // parser, so a Windows-shaped path is refused on Unix too.
        if raw.chars().nth(1) == Some(':') {
            return Err(escape("the path carries a platform prefix"));
        }

        let mut parts = Vec::new();
        for part in raw.split(['/', '\\']) {
            match part {
                "" | "." => continue,
                ".." => return Err(escape("the path contains a parent-directory component")),
                // Checked per component, so that a submodule's `.git` is refused
                // by the same rule as the root one, and so that the refusal
                // cannot be walked around with `.git/../.git` — which the arm
                // above has already declined anyway.
                other if other.eq_ignore_ascii_case(GIT_DIR) => {
                    return Err(WorkspaceError::NotProject {
                        path: raw.to_string(),
                        reason: "the path names the repository's own metadata".to_string(),
                    })
                }
                other => parts.push(other),
            }
        }
        if parts.is_empty() {
            return Err(escape("the path names no file"));
        }
        Ok(WorkspacePath(parts.join("/")))
    }

    /// The normalised, workspace-relative rendering of this path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_refuses_every_shape_that_leaves_the_workspace() {
        for raw in [
            "../outside.txt",
            "a/../../outside.txt",
            "/etc/passwd",
            "C:\\Windows\\system32",
            "a\0b",
            "",
            "./",
            "src/sub/../lib.rs",
        ] {
            assert!(
                WorkspacePath::parse(raw).is_err(),
                "{raw:?} must be refused before it can reach the filesystem"
            );
        }
    }

    #[test]
    fn it_refuses_the_repositorys_own_bookkeeping() {
        // Not a denylist of one filename: `.git` names a *class* — the
        // repository's own metadata — and the class is refused wherever it
        // appears in a path, because a submodule carries one too. In a linked
        // worktree `.git` is a regular *file* whose entire contents are an
        // absolute host path, so a `read_file` that admitted it would hand the
        // model the operator's directory layout in a tool's success output.
        for raw in [
            ".git",
            ".git/config",
            "./.git/info/exclude",
            "sub/.git",
            "sub/.git/config",
            ".git/worktrees/x/gitdir",
            // Two spellings of the same directory on a case-insensitive
            // filesystem, which is every macOS checkout by default.
            ".GIT/config",
            ".Git",
        ] {
            assert!(
                WorkspacePath::parse(raw).is_err(),
                "{raw:?} names the repository's own metadata and must be refused"
            );
        }
    }

    #[test]
    fn it_does_not_mistake_project_files_for_that_bookkeeping() {
        // The rule is a whole path *component* equal to `.git`, so the files a
        // project keeps beside it — which are ordinary versioned content — are
        // untouched by it.
        for raw in [
            ".gitignore",
            ".gitattributes",
            ".gitmodules",
            ".github/workflows/ci.yml",
            "src/git.rs",
        ] {
            assert!(
                WorkspacePath::parse(raw).is_ok(),
                "{raw:?} is project content, not repository metadata"
            );
        }
    }

    #[test]
    fn it_accepts_ordinary_relative_paths_and_normalises_them() {
        assert_eq!(
            WorkspacePath::parse("src/lib.rs").unwrap().as_str(),
            "src/lib.rs"
        );
        assert_eq!(
            WorkspacePath::parse("./src/lib.rs").unwrap().as_str(),
            "src/lib.rs"
        );
        assert_eq!(
            WorkspacePath::parse("src/./lib.rs").unwrap().as_str(),
            "src/lib.rs"
        );
    }

    #[test]
    fn the_error_names_the_path_and_why() {
        match WorkspacePath::parse("../outside.txt") {
            Err(WorkspaceError::Escape { path, reason }) => {
                assert_eq!(path, "../outside.txt");
                assert!(
                    !reason.is_empty(),
                    "an operator needs to know which rule fired"
                );
            }
            other => panic!("expected an escape error, got {other:?}"),
        }
    }
}
