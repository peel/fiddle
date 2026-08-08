//! What the agent actually touched, according to the repository.
//!
//! An agent's report of its own work is a claim, and a claim is not evidence: a
//! model that edited three files and said it edited one is indistinguishable, to
//! anything downstream, from a model that edited one — unless the changed-file
//! set is derived from somewhere the model does not author. `git status` over
//! the workspace is that somewhere. Nothing here reads a report.
//!
//! The whole of the difficulty is in parsing git's answer without introducing a
//! second way for the evidence to be wrong. Two flags carry that weight:
//!
//! `-z`, because the default `--porcelain` rendering is lossy in exactly the
//! cases that matter. It *quotes* any path containing a space or a non-ASCII
//! byte (`?? "src/a file.rs"`) and renders a rename as `R  old -> new`, so a
//! parser that slices a fixed prefix silently yields a quoted path in one case
//! and a run-on `old -> new` pseudo-path in the other. With `-z` the entries are
//! NUL-separated and never quoted, and a rename arrives as two records instead
//! of one ambiguous line.
//!
//! `-uall`, because git's default untracked mode reports a wholly-new directory
//! as the single entry `?? src/newmod/` rather than the files inside it. A
//! directory is not a changed file, and an agent that adds a module would
//! otherwise have its work described by a path that names no file it wrote. The
//! same flag also pins the behaviour against a `status.showUntrackedFiles=no` in
//! an operator's config, which would drop every created file from the evidence
//! without saying so.
//!
//! Records are parsed as bytes rather than as `str`. That is not an
//! optimisation: `str::split_at(3)` panics when byte 3 is not a character
//! boundary, and a filename is not required to be valid UTF-8 on any platform
//! this runs on. Slicing bytes cannot panic, and the one place where a path must
//! become text is a checked decode that fails loudly.

use super::{WorkspaceError, WorkspacePath};
use std::path::Path;

/// The status invocation, pinned to the format this parser was written against.
///
/// `=v1` is explicit rather than defaulted: `--porcelain` alone means "whatever
/// version is current", and the record layout below is v1's.
const STATUS: &[&str] = &["status", "--porcelain=v1", "-z", "-uall"];

/// The listing invocation.
///
/// `--cached` and `--others` together are what make this the *project's* files
/// rather than either half of them: tracked files alone would omit everything
/// the agent created, and untracked files alone would omit everything it was
/// given. `--exclude-standard` applies the repository's own ignore rules, which
/// is the whole reason this is git's job and not a directory walk — after one
/// `run_check` a walk would return the entire `target/` tree, tens of thousands
/// of paths that are neither the project nor anything a model should be handed.
/// `-z` for the same reason as above: git quotes any path with a space in it
/// unless told to separate records with NUL.
const LIST: &[&str] = &[
    "ls-files",
    "--cached",
    "--others",
    "--exclude-standard",
    "-z",
];

/// The width of a v1 status record's `XY ` prefix.
const PREFIX: usize = 3;

impl super::Workspace {
    /// The paths git reports as changed — the authoritative record of what this
    /// attempt did to the repository.
    ///
    /// Sorted and deduplicated, so that two runs over the same tree produce the
    /// same evidence and a diff between attempts means a difference in what
    /// happened rather than in what order git happened to walk.
    pub fn changed_files(&self) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        changed_files(self.root())
    }

    /// Every file of the project, as git understands the project.
    ///
    /// Each path goes back through [`WorkspacePath::parse`] rather than being
    /// trusted because it came from git: the type is the carrier of the
    /// containment guarantee, and a path that skipped the parse would be a path
    /// nothing had checked.
    pub fn list(&self) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        listed(&super::git_stdout(self.root(), LIST)?)
    }
}

/// The changed-file set of the git worktree rooted at `root`.
fn changed_files(root: &Path) -> Result<Vec<WorkspacePath>, WorkspaceError> {
    parse(&super::git_stdout(root, STATUS)?)
}

/// Turn `git ls-files -z` output into workspace paths.
///
/// Simpler than [`parse`] because there is no status field and no rename pair:
/// every record is a bare path. Sorted and deduplicated because `--cached` and
/// `--others` can each name a path that the other also names.
fn listed(out: &[u8]) -> Result<Vec<WorkspacePath>, WorkspaceError> {
    let mut paths = Vec::new();
    for record in out.split(|byte| *byte == 0).filter(|r| !r.is_empty()) {
        paths.push(WorkspacePath::parse(decode(LIST, record)?)?);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Turn `git status --porcelain=v1 -z -uall` output into workspace paths.
///
/// Separated from the invocation so the record layout can be tested against the
/// exact bytes git was observed to emit, rather than against whatever a fixture
/// repository happens to produce today.
fn parse(out: &[u8]) -> Result<Vec<WorkspacePath>, WorkspaceError> {
    let mut paths = Vec::new();
    let mut records = out.split(|byte| *byte == 0).filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        let (status, path) = split(record)?;
        // A rename or a copy spends two records: `R  <new>\0<origin>\0`. The
        // origin is a second *description of the same change*, not a second
        // change, so it is consumed here. Failing to consume it would report a
        // path the agent deleted as though it had written it.
        if status.contains(&b'R') || status.contains(&b'C') {
            records.next().ok_or_else(|| {
                malformed(
                    STATUS,
                    "a rename record was not followed by its origin path",
                )
            })?;
        }
        paths.push(WorkspacePath::parse(decode(STATUS, path)?)?);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Split `XY <path>` into its status field and its path.
///
/// The separator is *checked* rather than assumed. The offset is fixed in v1, so
/// verifying that byte 2 really is the space is what turns a format that shifted
/// underneath us into a loud failure instead of a path missing its first
/// character.
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

/// A path git named, as text — or a failure that says so.
///
/// Lossy decoding is refused deliberately. `from_utf8_lossy` would turn an
/// undecodable filename into a path containing U+FFFD, which parses cleanly as a
/// `WorkspacePath` and names nothing on disk; the changed-file set would then be
/// confidently wrong. Evidence that cannot be produced correctly is worth less
/// than evidence that admits it.
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

/// git ran, but said something this parser does not understand.
///
/// The invocation is a parameter rather than a constant because two of them are
/// parsed here now, and an error naming the wrong one would send whoever reads
/// it to the wrong parser.
fn malformed(command: &[&str], reason: &str) -> WorkspaceError {
    WorkspaceError::Git {
        command: command.join(" "),
        stderr: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every byte string below was captured from `git status --porcelain=v1 -z
    /// -uall` on git 2.51 rather than recalled, because the two shapes that make
    /// this parser necessary — the quoted path and the two-record rename — are
    /// precisely the ones that are easy to misremember.
    fn parsed(out: &[u8]) -> Vec<String> {
        parse(out)
            .unwrap()
            .iter()
            .map(|p| p.as_str().to_string())
            .collect()
    }

    #[test]
    fn a_listing_is_bare_paths_deduplicated_and_ordered() {
        let paths = listed(b"src/lib.rs\0src/a file.rs\0src/lib.rs\0")
            .unwrap()
            .iter()
            .map(|p| p.as_str().to_string())
            .collect::<Vec<_>>();
        // `--cached` and `--others` can each name the same path; the model must
        // not be told about one file twice.
        assert_eq!(paths, ["src/a file.rs", "src/lib.rs"]);
    }

    #[test]
    fn a_listed_path_that_is_not_text_is_refused_not_mangled() {
        let err = listed(b"src/bad\xffname.rs\0").unwrap_err();
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
            parsed(b" M src/lib.rs\0?? src/new.rs\0 D src/gone.rs\0AM src/two.rs\0"),
            ["src/gone.rs", "src/lib.rs", "src/new.rs", "src/two.rs"],
            "the status field is two columns wide whatever it says"
        );
    }

    #[test]
    fn a_space_in_a_path_is_not_a_field_separator() {
        // The plain rendering of this same entry is `?? "src/a file.rs"`, quotes
        // included. Under `-z` there is nothing to unquote.
        assert_eq!(parsed(b"?? src/a file.rs\0"), ["src/a file.rs"]);
    }

    #[test]
    fn a_renames_origin_is_consumed_not_counted() {
        assert_eq!(
            parsed(b"R  src/renamed.rs\0src/lib.rs\0"),
            ["src/renamed.rs"]
        );
        // `RM`: renamed in the index and modified again in the worktree. Still
        // one change, still one trailing origin record — so the rename must be
        // recognised by either status column, not only by the first.
        assert_eq!(parsed(b"RM src/moved.rs\0src/big.rs\0"), ["src/moved.rs"]);
    }

    #[test]
    fn an_origin_path_is_never_mistaken_for_a_status_record() {
        // The origin that follows a rename is a bare path, and a bare path can
        // begin with anything at all — including bytes that would parse as a
        // status field. Whether it is skipped by position or sniffed at is the
        // difference between one changed file and two.
        assert_eq!(
            parsed(b"R  new.rs\0 M old.rs\0 M other.rs\0"),
            ["new.rs", "other.rs"]
        );
    }

    #[test]
    fn a_path_git_cannot_render_as_text_is_refused_not_mangled() {
        let err = parse(b" M src/bad\xffname.rs\0").unwrap_err();
        assert!(
            matches!(&err, WorkspaceError::Git { stderr, .. } if stderr.contains("not valid UTF-8")),
            "got {err:?}"
        );
    }

    #[test]
    fn a_record_that_is_not_a_status_record_is_refused() {
        // Not `continue`d past: a record this parser cannot read means it has
        // lost its place in the stream, and every path after it is suspect.
        for out in [&b"X\0"[..], &b"XYZsrc/lib.rs\0"[..], &b" M \0"[..]] {
            assert!(
                parse(out).is_err(),
                "{:?} must be refused, not skipped",
                String::from_utf8_lossy(out)
            );
        }
        // A rename whose origin record never arrives is the same failure.
        assert!(parse(b"R  src/renamed.rs\0").is_err());
    }

    #[test]
    fn no_record_can_panic_the_split() {
        // A well-formed record's `XY ` prefix is ASCII, so byte 3 is a character
        // boundary and the path's own multi-byte content is never cut.
        assert_eq!(parsed(" M ünicode.rs\0".as_bytes()), ["ünicode.rs"]);
        // A malformed record carries no such guarantee, and this is where the
        // choice of bytes over `str` earns itself: `str::split_at(3)` on these
        // four bytes *panics*, because byte 3 falls inside a character. The same
        // input has to come back as a refusal, not as a downed runtime.
        assert!(parse("üü\0".as_bytes()).is_err());
    }
}
