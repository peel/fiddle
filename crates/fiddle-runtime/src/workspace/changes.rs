use super::{WorkspaceError, WorkspacePath};
use std::collections::HashMap;

const STATUS: &[&str] = &["status", "--porcelain=v1", "-z", "-uno"];

const PREFIX: usize = 3;

const UNTRACKED: &[u8] = b"??";

const COMMITTED: &[&str] = &["ls-tree", "-r", "--name-only", "-z", "HEAD"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Content {
    Absent,

    Text(String),

    Opaque,
}

impl Content {
    fn of(bytes: Vec<u8>) -> Content {
        match String::from_utf8(bytes) {
            Ok(text) => Content::Text(text),
            Err(_) => Content::Opaque,
        }
    }

    fn text(&self) -> &str {
        match self {
            Content::Text(text) => text,
            Content::Absent | Content::Opaque => "",
        }
    }
}

#[derive(Clone, Debug)]
pub struct FileEdit {
    pub path: WorkspacePath,

    pub before: Content,

    pub after: Content,
}

impl FileEdit {
    pub fn added(&self) -> Vec<&str> {
        only_in(self.after.text(), self.before.text())
    }

    pub fn removed(&self) -> Vec<&str> {
        only_in(self.before.text(), self.after.text())
    }

    pub fn unreadable(&self) -> bool {
        matches!(self.before, Content::Opaque) || matches!(self.after, Content::Opaque)
    }
}

fn only_in<'a>(text: &'a str, other: &str) -> Vec<&'a str> {
    let mut available: HashMap<&str, usize> = HashMap::new();
    for line in other.lines() {
        *available.entry(line).or_default() += 1;
    }
    text.lines()
        .filter(|line| match available.get_mut(line) {
            Some(count) if *count > 0 => {
                *count -= 1;
                false
            }
            _ => true,
        })
        .collect()
}

impl super::Workspace {
    pub fn changed_files(&self) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        let mut paths = tracked(&super::git_stdout(self.root(), STATUS)?)?;
        paths.extend(self.created()?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    pub fn edits(&self) -> Result<Vec<FileEdit>, WorkspaceError> {
        let committed = self.committed()?;
        let mut edits = Vec::new();
        for path in self.changed_files()? {
            let before = if committed.contains(&path) {
                let spec = format!("HEAD:{}", path.as_str());
                Content::of(super::git_stdout(self.root(), &["show", &spec])?)
            } else {
                Content::Absent
            };
            let after = self.working(&path)?;
            edits.push(FileEdit {
                path,
                before,
                after,
            });
        }
        Ok(edits)
    }

    fn committed(&self) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        listed(COMMITTED, &super::git_stdout(self.root(), COMMITTED)?)
    }

    fn working(&self, path: &WorkspacePath) -> Result<Content, WorkspaceError> {
        let resolved = self.resolve(path)?;
        match std::fs::read(&resolved) {
            Ok(bytes) => Ok(Content::of(bytes)),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(Content::Absent),
            Err(source) => Err(WorkspaceError::Io {
                path: resolved,
                source,
            }),
        }
    }

    pub fn list(&self) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        self.ls_files(&["--cached", "--others"])
    }

    fn created(&self) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        self.ls_files(&["--others"])
    }

    fn ls_files(&self, selection: &[&str]) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        let baseline = self.baseline_ignore().to_string_lossy().to_string();
        let mut args = vec!["ls-files", "-z", "--exclude-from", &baseline];
        args.extend_from_slice(selection);
        listed(&args, &super::git_stdout(self.root(), &args)?)
    }
}

fn listed(command: &[&str], out: &[u8]) -> Result<Vec<WorkspacePath>, WorkspaceError> {
    let mut paths = Vec::new();
    for record in out.split(|byte| *byte == 0).filter(|r| !r.is_empty()) {
        paths.push(WorkspacePath::parse(decode(command, record)?)?);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn tracked(out: &[u8]) -> Result<Vec<WorkspacePath>, WorkspaceError> {
    let mut paths = Vec::new();
    let mut records = out.split(|byte| *byte == 0).filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        let (status, path) = split(record)?;
        if status.contains(&b'R') || status.contains(&b'C') {
            records.next().ok_or_else(|| {
                malformed(
                    STATUS,
                    "a rename record was not followed by its origin path",
                )
            })?;
        }
        if status.starts_with(UNTRACKED) {
            continue;
        }
        paths.push(WorkspacePath::parse(decode(STATUS, path)?)?);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn split(record: &[u8]) -> Result<(&[u8], &[u8]), WorkspaceError> {
    match record.split_at_checked(PREFIX) {
        Some((status, path)) if status[PREFIX - 1] == b' ' && !path.is_empty() => {
            Ok((status, path))
        }
        _ => Err(malformed(
            STATUS,
            &format!(
                "expected an `XY <path>` status record, got {:?}",
                String::from_utf8_lossy(record)
            ),
        )),
    }
}

fn decode<'a>(command: &[&str], path: &'a [u8]) -> Result<&'a str, WorkspaceError> {
    std::str::from_utf8(path).map_err(|_| {
        malformed(
            command,
            &format!(
                "git named a path that is not valid UTF-8: {:?}",
                String::from_utf8_lossy(path)
            ),
        )
    })
}

fn malformed(command: &[&str], reason: &str) -> WorkspaceError {
    WorkspaceError::Git {
        command: command.join(" "),
        stderr: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(out: &[u8]) -> Vec<String> {
        tracked(out)
            .unwrap()
            .iter()
            .map(|p| p.as_str().to_string())
            .collect()
    }

    const LS_FILES: &[&str] = &["ls-files", "-z", "--exclude-from", "…", "--others"];

    #[test]
    fn a_listing_is_bare_paths_deduplicated_and_ordered() {
        let paths = listed(LS_FILES, b"src/lib.rs\0src/a file.rs\0src/lib.rs\0")
            .unwrap()
            .iter()
            .map(|p| p.as_str().to_string())
            .collect::<Vec<_>>();
        assert_eq!(paths, ["src/a file.rs", "src/lib.rs"]);
    }

    #[test]
    fn a_listed_path_that_is_not_text_is_refused_not_mangled() {
        let err = listed(LS_FILES, b"src/bad\xffname.rs\0").unwrap_err();
        assert!(
            matches!(&err, WorkspaceError::Git { command, stderr }
                if command.starts_with("ls-files") && stderr.contains("not valid UTF-8")),
            "the failure must name the invocation it came from: {err:?}"
        );
    }

    #[test]
    fn a_clean_worktree_reports_nothing() {
        assert!(parsed(b"").is_empty());
    }

    #[test]
    fn it_reads_the_ordinary_shapes() {
        assert_eq!(
            parsed(b" M src/lib.rs\0 D src/gone.rs\0AM src/two.rs\0"),
            ["src/gone.rs", "src/lib.rs", "src/two.rs"],
            "the status field is two columns wide whatever it says"
        );
    }

    #[test]
    fn an_untracked_record_is_not_this_halfs_business() {
        assert_eq!(
            parsed(b" M src/lib.rs\0?? src/smuggled.rs\0"),
            ["src/lib.rs"]
        );
    }

    #[test]
    fn a_space_in_a_path_is_not_a_field_separator() {
        assert_eq!(parsed(b" M src/a file.rs\0"), ["src/a file.rs"]);
        assert_eq!(
            listed(LS_FILES, b"src/a file.rs\0").unwrap()[0].as_str(),
            "src/a file.rs",
            "and the other half of the derivation has the same property"
        );
    }

    #[test]
    fn a_renames_origin_is_consumed_not_counted() {
        assert_eq!(
            parsed(b"R  src/renamed.rs\0src/lib.rs\0"),
            ["src/renamed.rs"]
        );
        assert_eq!(parsed(b"RM src/moved.rs\0src/big.rs\0"), ["src/moved.rs"]);
    }

    #[test]
    fn an_origin_path_is_never_mistaken_for_a_status_record() {
        assert_eq!(
            parsed(b"R  new.rs\0 M old.rs\0 M other.rs\0"),
            ["new.rs", "other.rs"]
        );
    }

    #[test]
    fn a_path_git_cannot_render_as_text_is_refused_not_mangled() {
        let err = tracked(b" M src/bad\xffname.rs\0").unwrap_err();
        assert!(
            matches!(&err, WorkspaceError::Git { stderr, .. } if stderr.contains("not valid UTF-8")),
            "got {err:?}"
        );
    }

    #[test]
    fn a_record_that_is_not_a_status_record_is_refused() {
        for out in [&b"X\0"[..], &b"XYZsrc/lib.rs\0"[..], &b" M \0"[..]] {
            assert!(
                tracked(out).is_err(),
                "{:?} must be refused, not skipped",
                String::from_utf8_lossy(out)
            );
        }
        assert!(tracked(b"R  src/renamed.rs\0").is_err());
    }

    #[test]
    fn no_record_can_panic_the_split() {
        assert_eq!(parsed(" M ünicode.rs\0".as_bytes()), ["ünicode.rs"]);
        assert!(tracked("üü\0".as_bytes()).is_err());
    }
}
