use super::WorkspaceError;

const GIT_DIR: &str = ".git";

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct WorkspacePath(String);

impl WorkspacePath {
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
        if raw.chars().nth(1) == Some(':') {
            return Err(escape("the path carries a platform prefix"));
        }

        let mut parts = Vec::new();
        for part in raw.split(['/', '\\']) {
            match part {
                "" | "." => continue,
                ".." => return Err(escape("the path contains a parent-directory component")),
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
        for raw in [
            ".git",
            ".git/config",
            "./.git/info/exclude",
            "sub/.git",
            "sub/.git/config",
            ".git/worktrees/x/gitdir",
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
