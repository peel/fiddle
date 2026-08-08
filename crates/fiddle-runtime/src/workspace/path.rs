//! Turning a requested path into one that provably stays inside the workspace.

use super::WorkspaceError;

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
