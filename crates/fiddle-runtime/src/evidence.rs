//! Publishing a report bundle so that a reader never observes a partial one.
//!
//! The bundle itself is a `fiddle-core` value with no idea where it lives; this
//! module is the whole of "where it lives". Two properties are worth the code:
//!
//! - **A bundle appears whole or not at all.** The attempt directory is built
//!   under a sibling temporary name and moved into place with a single
//!   [`rename`](std::fs::rename), so a reader listing `<report.dir>/<slug>/`
//!   either sees a complete attempt directory or does not see it yet. Writing
//!   `report.json` in place would leave a window in which the bundle parses as
//!   truncated JSON, and a truncated bundle is worse than a missing one — it
//!   invites a downstream tool to conclude something.
//! - **A failure leaves nothing behind.** Cleanup is a [`Drop`] guard rather
//!   than a call at each error site, so every path out of [`publish`] —
//!   including a `?` added by a later edit, and including a panic — removes the
//!   temporary directory. A manual cleanup is only correct until someone adds a
//!   return above it.

use fiddle_core::{AttemptId, ReportBundle};
use std::path::{Path, PathBuf};

/// The file name every published attempt directory contains.
pub const BUNDLE_FILE: &str = "report.json";

/// Why an attempt could not record itself durably.
///
/// Every variant names what could not be done and where, because this surfaces
/// to an operator as the reason a run failed: `<report.dir>` is theirs to fix,
/// and a bare "could not publish" would leave them nothing to act on. The
/// wording differs per variant on purpose — all three of these become a
/// [`RunOutcome::Retryable`](fiddle_core::RunOutcome::Retryable) reason, as does
/// a change-set write a capability could not complete, and a reader of the
/// payload alone has to be able to tell which of the three failed.
#[derive(Debug, thiserror::Error)]
pub enum EvidenceError {
    /// The bundle could not be written, moved into place, or given a directory.
    #[error("could not write the report bundle at {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The attempt journal could not be appended to.
    ///
    /// Distinct from [`EvidenceError::Write`] because it fails at a different
    /// moment and has a different consequence: a journal that cannot be written
    /// stops the attempt *before* the capability runs, so nothing has changed
    /// and the reason must not read as if the run had already done its work.
    #[error("could not record the attempt journal at {path}: {source}")]
    Journal {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The bundle could not be rendered as JSON.
    ///
    /// Unreachable for the shapes M0 builds — every field serializes
    /// infallibly — but represented rather than unwrapped, so a bundle that
    /// grows a fallible field fails the run honestly instead of aborting it.
    #[error("could not render the report bundle as JSON: {source}")]
    Render {
        #[source]
        source: serde_json::Error,
    },
}

/// Write `bundle` into `<report_dir>/<slug>/<attempt>/report.json`, atomically,
/// and hand back the path it landed at.
///
/// The move is of the whole attempt *directory*, not of the file inside it, so
/// the directory's appearance and the bundle's completeness are the same event.
pub fn publish(
    report_dir: &Path,
    slug: &str,
    attempt: &AttemptId,
    bundle: &ReportBundle,
) -> Result<PathBuf, EvidenceError> {
    let invocation_dir = report_dir.join(slug);
    let destination = invocation_dir.join(&attempt.0);
    // A sibling of the destination, so the rename stays within one directory
    // and therefore within one filesystem: a temporary directory somewhere else
    // could not be moved into place atomically.
    let staging = invocation_dir.join(format!(".{}.tmp", attempt.0));

    let rendered =
        serde_json::to_vec_pretty(bundle).map_err(|source| EvidenceError::Render { source })?;

    // Nothing is cleaned up if this fails, because nothing was created: an
    // unwritable `<report.dir>` fails here, before any temporary path exists.
    write_error(&staging, std::fs::create_dir_all(&staging))?;
    // Armed from this line on. Every subsequent `?`, and any panic, removes the
    // staging directory on the way out.
    let staged = Staging::holding(&staging);

    let staged_bundle = staging.join(BUNDLE_FILE);
    write_error(&staged_bundle, std::fs::write(&staged_bundle, rendered))?;
    write_error(&destination, std::fs::rename(&staging, &destination))?;

    staged.published();
    Ok(destination.join(BUNDLE_FILE))
}

/// A staging directory that removes itself unless publication claimed it.
///
/// The disarm is [`Staging::published`], which consumes the guard — so "the
/// bundle landed" and "stop cleaning up" are one statement that cannot be made
/// in the wrong order or forgotten in a branch.
struct Staging<'a> {
    path: Option<&'a Path>,
}

impl<'a> Staging<'a> {
    fn holding(path: &'a Path) -> Self {
        Staging { path: Some(path) }
    }

    /// The staging directory became the published one; there is nothing left to
    /// remove.
    fn published(mut self) {
        self.path = None;
    }
}

impl Drop for Staging<'_> {
    fn drop(&mut self) {
        if let Some(path) = self.path {
            // Best effort by necessity: `drop` cannot report, and the caller is
            // already returning the failure that brought it here. A removal that
            // itself fails must not mask that.
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

/// Attach the path an IO operation was attempted on to its error.
fn write_error(path: &Path, result: std::io::Result<()>) -> Result<(), EvidenceError> {
    result.map_err(|source| EvidenceError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Mint the identity of one attempt: a ULID-style, lexicographically ordered,
/// path-safe token.
///
/// Ten Crockford base-32 characters of millisecond timestamp followed by
/// sixteen of entropy, so attempt directories sort in the order they were
/// created while two attempts starting in the same millisecond — the two
/// back-to-back invocations the stability proof runs — still get different
/// names. Sorting matters because the attempt id *is* the directory name: a
/// reader listing `<report.dir>/<slug>/` gets the attempts in order for free.
///
/// The caller mints one per run, which is what makes an attempt id name an
/// attempt rather than a moment.
pub fn mint_attempt_id() -> AttemptId {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        // A clock behind the epoch is not worth failing a run over; the entropy
        // half still separates two attempts, they merely stop sorting usefully.
        .unwrap_or_default();
    let milliseconds = since_epoch.as_millis() & ((1 << 48) - 1);
    AttemptId(format!(
        "{}{}",
        base32(milliseconds, 10),
        base32(entropy(since_epoch), 16)
    ))
}

/// Eighty bits that differ between two attempts started in the same
/// millisecond, including two started by two different processes.
///
/// `RandomState` is seeded from the operating system's randomness once per
/// process, so hashing anything at all with a fresh one yields a value that is
/// unrelated to the last process's; the nanosecond reading and the process id
/// separate two attempts *within* one process and one seed.
fn entropy(since_epoch: std::time::Duration) -> u128 {
    use std::hash::{BuildHasher, Hasher};

    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(since_epoch.as_nanos());
    hasher.write_u32(std::process::id());
    ((hasher.finish() as u128) << 16) | (std::process::id() as u128 & 0xffff)
}

/// The low `width * 5` bits of `value` in Crockford base 32, most significant
/// character first.
///
/// Crockford's alphabet rather than RFC 4648's because it omits `I`, `L`, `O`,
/// and `U`: an attempt id ends up in directory listings and bug reports, where
/// a `1`/`I` confusion costs someone a wrong path.
fn base32(value: u128, width: usize) -> String {
    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    (0..width)
        .rev()
        .map(|position| ALPHABET[((value >> (position * 5)) & 0x1f) as usize] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::{
        FiddleBuild, Mode, NextAction, Observation, ReportBundle, RunOutcome, WorkStateView,
        REPORT_SCHEMA, UNKNOWN_REVISION,
    };

    const SLUG: &str = "beans-fiddle-m0-demo";

    fn bundle(attempt: &AttemptId) -> ReportBundle {
        ReportBundle {
            schema: REPORT_SCHEMA,
            fiddle: FiddleBuild::new("0.1.0", UNKNOWN_REVISION),
            invocation_ref: "beans:fiddle-m0-demo".to_string(),
            work_ref: None,
            attempt_id: attempt.clone(),
            mode: Mode::Unattended,
            outcome: RunOutcome::Completed,
            next_action: NextAction::Complete,
            capability_executions: Vec::new(),
            progress: Vec::new(),
            observations: WorkStateView::without_publication(
                Observation::NotApplicable {
                    reason: "fixture".to_string(),
                },
                Observation::NotApplicable {
                    reason: "fixture".to_string(),
                },
            ),
        }
    }

    /// Every path under `root`, recursively, relative to it.
    fn entries(root: &Path) -> Vec<String> {
        let mut found = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in read.flatten() {
                let path = entry.path();
                found.push(path.strip_prefix(root).unwrap().display().to_string());
                if path.is_dir() {
                    stack.push(path);
                }
            }
        }
        found.sort();
        found
    }

    #[test]
    fn a_published_bundle_lands_at_the_attempt_path_and_parses() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = mint_attempt_id();

        let path = publish(dir.path(), SLUG, &attempt, &bundle(&attempt)).unwrap();

        assert_eq!(
            path,
            dir.path().join(SLUG).join(&attempt.0).join(BUNDLE_FILE)
        );
        let parsed: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(parsed["schema"], "fiddle.report.v0");
        assert_eq!(parsed["attempt_id"], attempt.0.as_str());
    }

    /// The staging directory is an implementation detail; a successful
    /// publication must not leave the reader anything to wonder about.
    #[test]
    fn publication_leaves_no_staging_directory_behind() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = mint_attempt_id();

        publish(dir.path(), SLUG, &attempt, &bundle(&attempt)).unwrap();

        let leftovers: Vec<_> = entries(dir.path())
            .into_iter()
            .filter(|path| path.contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "got {leftovers:?}");
    }

    /// The guard's reason to exist: a failure *after* staging succeeded. The
    /// rename is made to fail by occupying the destination with a non-empty
    /// directory, so the staging directory provably exists when the error
    /// surfaces — and provably does not survive it.
    #[test]
    fn a_failure_after_staging_removes_the_staging_directory() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = mint_attempt_id();
        let occupied = dir.path().join(SLUG).join(&attempt.0);
        std::fs::create_dir_all(&occupied).unwrap();
        std::fs::write(occupied.join("squatter"), b"in the way").unwrap();

        let error = publish(dir.path(), SLUG, &attempt, &bundle(&attempt)).unwrap_err();

        assert!(
            matches!(error, EvidenceError::Write { .. }),
            "got {error:?}"
        );
        let leftovers: Vec<_> = entries(dir.path())
            .into_iter()
            .filter(|path| path.contains(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "staging survived a failure: {leftovers:?}"
        );
        assert!(
            !occupied.join(BUNDLE_FILE).exists(),
            "a failed publication must not have written a bundle"
        );
    }

    /// An unwritable `<report.dir>` fails before anything is created, and the
    /// diagnostic names a path under it so an operator knows what to fix.
    #[cfg(unix)]
    #[test]
    fn an_unwritable_report_dir_is_reported_with_its_path_and_creates_nothing() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let reports = dir.path().join("reports");
        std::fs::create_dir_all(&reports).unwrap();
        std::fs::set_permissions(&reports, std::fs::Permissions::from_mode(0o500)).unwrap();
        let attempt = mint_attempt_id();

        let published = publish(&reports, SLUG, &attempt, &bundle(&attempt));

        std::fs::set_permissions(&reports, std::fs::Permissions::from_mode(0o755)).unwrap();
        if published.is_ok() {
            // Running with an identity that ignores the permission bits.
            return;
        }
        match published.unwrap_err() {
            EvidenceError::Write { path, .. } => assert!(
                path.starts_with(&reports),
                "the diagnostic must name a path under <report.dir>, got {path:?}"
            ),
            other => panic!("an unwritable report dir must fail on a write, got {other:?}"),
        }
        assert!(
            entries(&reports).is_empty(),
            "a failed publication must create nothing: {:?}",
            entries(&reports)
        );
    }

    /// Two attempts must be distinguishable even back to back, or the stability
    /// proof's "this was a genuinely new attempt" claim rests on nothing.
    #[test]
    fn attempt_ids_are_distinct_ordered_and_path_safe() {
        let ids: Vec<_> = (0..64).map(|_| mint_attempt_id().0).collect();

        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "attempt ids collided: {ids:?}");
        for id in &ids {
            assert_eq!(id.len(), 26, "got {id}");
            assert!(
                id.bytes()
                    .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase()),
                "an attempt id names a directory; got {id}"
            );
        }
        assert!(
            ids.windows(2).all(|pair| pair[0][..10] <= pair[1][..10]),
            "the timestamp half must order attempts as they were minted: {ids:?}"
        );
    }

    #[test]
    fn base32_encodes_most_significant_character_first() {
        assert_eq!(base32(0, 10), "0000000000");
        assert_eq!(base32(1, 4), "0001");
        assert_eq!(base32(31, 4), "000Z");
        assert_eq!(base32(32, 4), "0010");
    }
}
