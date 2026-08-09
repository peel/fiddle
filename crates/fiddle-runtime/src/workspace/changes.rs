//! What the agent actually touched, according to the repository.
//!
//! An agent's report of its own work is a claim, and a claim is not evidence: a
//! model that edited three files and said it edited one is indistinguishable, to
//! anything downstream, from a model that edited one — unless the changed-file
//! set is derived from somewhere the model does not author. The repository is
//! that somewhere. Nothing here reads a report.
//!
//! # Why one git command is not enough
//!
//! It is tempting to make this `git status` and be done. It was, and the flaw is
//! that ignore rules live in a file *inside the checkout*. An attempt that
//! writes `*` into `.gitignore` and then creates ten files has git report one
//! change, because it has edited an input to the question rather than the
//! answer. `--ignored` would close that and reopen the reason the exclusion
//! exists: one `run_check` writes a whole `target/` tree, and evidence drowned
//! in build output says as little as evidence suppressed by the model.
//!
//! So the set is derived in two halves, split along the line the difficulty
//! actually follows — **whether an ignore rule can bear on the answer at all**:
//!
//! 1. **What changed about files the repository already tracks**, from
//!    `git status`. No ignore rule reaches a tracked file, by git's own
//!    definition, so this half is beyond an attempt's influence for free. It is
//!    also where deletions and renames live, which no listing reports.
//! 2. **What the attempt created**, from `git ls-files --others` under the
//!    project's committed ignore rules and no others — see
//!    [`Workspace::baseline_ignore`](super::Workspace::baseline_ignore). This is
//!    the half where the rules matter, and the point is that they are the
//!    project's, fixed before the attempt began.
//!
//! `git status` is asked with `-uno` for the same reason: an untracked entry it
//! returned would have been filtered under the worktree's rules, which are not
//! the ones this module honours. The parser skips `??` records anyway, so the
//! flag and the filter are two independent guards over one gap.
//!
//! # Reading git's answer without introducing a second way to be wrong
//!
//! `-z` throughout, because the default `--porcelain` rendering is lossy in
//! exactly the cases that matter. It *quotes* any path containing a space or a
//! non-ASCII byte (`?? "src/a file.rs"`) and renders a rename as
//! `R  old -> new`, so a parser that slices a fixed prefix silently yields a
//! quoted path in one case and a run-on `old -> new` pseudo-path in the other.
//! With `-z` the entries are NUL-separated and never quoted, and a rename
//! arrives as two records instead of one ambiguous line.
//!
//! `ls-files --others` names files rather than directories, which is what makes
//! a wholly-new module appear as the files an agent wrote instead of as the
//! single entry `?? src/newmod/` that `git status` would have collapsed it into.
//!
//! Records are parsed as bytes rather than as `str`. That is not an
//! optimisation: `str::split_at(3)` panics when byte 3 is not a character
//! boundary, and a filename is not required to be valid UTF-8 on any platform
//! this runs on. Slicing bytes cannot panic, and the one place where a path must
//! become text is a checked decode that fails loudly.

use super::{WorkspaceError, WorkspacePath};

/// The status invocation, pinned to the format this parser was written against.
///
/// `=v1` is explicit rather than defaulted: `--porcelain` alone means "whatever
/// version is current", and the record layout below is v1's. `-uno` because the
/// untracked half of the answer is not taken from here; see the module
/// documentation.
const STATUS: &[&str] = &["status", "--porcelain=v1", "-z", "-uno"];

/// The width of a v1 status record's `XY ` prefix.
const PREFIX: usize = 3;

/// A v1 status field meaning "git has never heard of this path".
const UNTRACKED: &[u8] = b"??";

impl super::Workspace {
    /// The paths git reports as changed — the authoritative record of what this
    /// attempt did to the repository.
    ///
    /// Sorted and deduplicated, so that two runs over the same tree produce the
    /// same evidence and a diff between attempts means a difference in what
    /// happened rather than in what order git happened to walk.
    pub fn changed_files(&self) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        let mut paths = tracked(&super::git_stdout(self.root(), STATUS)?)?;
        paths.extend(self.created()?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    /// Every file of the project, as the project itself defines the project.
    ///
    /// `--cached` and `--others` together are what make this the project's files
    /// rather than either half of them: tracked files alone would omit
    /// everything the agent created, and untracked files alone would omit
    /// everything it was given.
    ///
    /// Each path goes back through [`WorkspacePath::parse`] rather than being
    /// trusted because it came from git: the type is the carrier of the
    /// containment guarantee, and a path that skipped the parse would be a path
    /// nothing had checked.
    pub fn list(&self) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        self.ls_files(&["--cached", "--others"])
    }

    /// The files this attempt created, under the project's committed rules.
    fn created(&self) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        self.ls_files(&["--others"])
    }

    /// `git ls-files` with `selection`, excluding under the baseline rules.
    ///
    /// `--exclude-from` rather than `--exclude-standard`, and that is the whole
    /// substance of this function. `--exclude-standard` reads the worktree's own
    /// `.gitignore` files — which the agent can write — along with this
    /// repository's `.git/info/exclude` and the operator's global excludes,
    /// which would make one attempt's evidence depend on whose machine it ran
    /// on. Naming one snapshot file instead answers under the project's
    /// committed rules and nothing else.
    fn ls_files(&self, selection: &[&str]) -> Result<Vec<WorkspacePath>, WorkspaceError> {
        let baseline = self.baseline_ignore().to_string_lossy().to_string();
        let mut args = vec!["ls-files", "-z", "--exclude-from", &baseline];
        args.extend_from_slice(selection);
        listed(&args, &super::git_stdout(self.root(), &args)?)
    }
}

/// Turn `git ls-files -z` output into workspace paths.
///
/// Simpler than [`tracked`] because there is no status field and no rename pair:
/// every record is a bare path. Sorted and deduplicated because `--cached` and
/// `--others` can each name a path that the other also names.
fn listed(command: &[&str], out: &[u8]) -> Result<Vec<WorkspacePath>, WorkspaceError> {
    let mut paths = Vec::new();
    for record in out.split(|byte| *byte == 0).filter(|r| !r.is_empty()) {
        paths.push(WorkspacePath::parse(decode(command, record)?)?);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Turn `git status --porcelain=v1 -z` output into the tracked paths it names.
///
/// Separated from the invocation so the record layout can be tested against the
/// exact bytes git was observed to emit, rather than against whatever a fixture
/// repository happens to produce today.
///
/// `??` records are skipped rather than parsed. Whether git emits any is the
/// flag's business; whether they are counted here is this function's, and they
/// must not be — an untracked entry from `git status` survived the worktree's
/// own ignore rules, which is precisely the filter this module refuses to
/// inherit. Created files arrive from
/// [`Workspace::created`](super::Workspace::created) instead.
fn tracked(out: &[u8]) -> Result<Vec<WorkspacePath>, WorkspaceError> {
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
        if status.starts_with(UNTRACKED) {
            continue;
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
        tracked(out)
            .unwrap()
            .iter()
            .map(|p| p.as_str().to_string())
            .collect()
    }

    /// The invocation a listing is parsed under, for the error-naming test.
    const LS_FILES: &[&str] = &["ls-files", "-z", "--exclude-from", "…", "--others"];

    #[test]
    fn a_listing_is_bare_paths_deduplicated_and_ordered() {
        let paths = listed(LS_FILES, b"src/lib.rs\0src/a file.rs\0src/lib.rs\0")
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
        // `-uno` should mean git never emits one. If it does — an operator's
        // config, a future git, a caller that changed the flag — it must still
        // not be counted here: a `??` entry from `git status` is an entry that
        // survived the *worktree's* ignore rules, which is the one filter this
        // module refuses to inherit, because an attempt can write it. Created
        // files come from `ls-files --others` under the committed rules instead.
        assert_eq!(
            parsed(b" M src/lib.rs\0?? src/smuggled.rs\0"),
            ["src/lib.rs"]
        );
    }

    #[test]
    fn a_space_in_a_path_is_not_a_field_separator() {
        // The plain rendering of this same entry is `M "src/a file.rs"`, quotes
        // included. Under `-z` there is nothing to unquote.
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
        let err = tracked(b" M src/bad\xffname.rs\0").unwrap_err();
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
                tracked(out).is_err(),
                "{:?} must be refused, not skipped",
                String::from_utf8_lossy(out)
            );
        }
        // A rename whose origin record never arrives is the same failure.
        assert!(tracked(b"R  src/renamed.rs\0").is_err());
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
        assert!(tracked("üü\0".as_bytes()).is_err());
    }
}
