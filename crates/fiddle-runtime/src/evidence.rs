use fiddle_core::{AttemptId, ReportBundle};
use std::path::{Path, PathBuf};

pub const BUNDLE_FILE: &str = "report.json";

#[derive(Debug, thiserror::Error)]
pub enum EvidenceError {
    #[error("could not write the report bundle at {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not record the attempt journal at {path}: {source}")]
    Journal {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not render the report bundle as JSON: {source}")]
    Render {
        #[source]
        source: serde_json::Error,
    },
}

pub fn publish(
    report_dir: &Path,
    slug: &str,
    attempt: &AttemptId,
    bundle: &ReportBundle,
) -> Result<PathBuf, EvidenceError> {
    let invocation_dir = report_dir.join(slug);
    let destination = invocation_dir.join(&attempt.0);
    let staging = invocation_dir.join(format!(".{}.tmp", attempt.0));

    let rendered =
        serde_json::to_vec_pretty(bundle).map_err(|source| EvidenceError::Render { source })?;

    write_error(&staging, std::fs::create_dir_all(&staging))?;
    let staged = Staging::holding(&staging);

    let staged_bundle = staging.join(BUNDLE_FILE);
    write_error(&staged_bundle, std::fs::write(&staged_bundle, rendered))?;
    write_error(&destination, std::fs::rename(&staging, &destination))?;

    staged.published();
    Ok(destination.join(BUNDLE_FILE))
}

struct Staging<'a> {
    path: Option<&'a Path>,
}

impl<'a> Staging<'a> {
    fn holding(path: &'a Path) -> Self {
        Staging { path: Some(path) }
    }

    fn published(mut self) {
        self.path = None;
    }
}

impl Drop for Staging<'_> {
    fn drop(&mut self) {
        if let Some(path) = self.path {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn write_error(path: &Path, result: std::io::Result<()>) -> Result<(), EvidenceError> {
    result.map_err(|source| EvidenceError::Write {
        path: path.to_path_buf(),
        source,
    })
}

pub fn mint_attempt_id() -> AttemptId {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let milliseconds = since_epoch.as_millis() & ((1 << 48) - 1);
    AttemptId(format!(
        "{}{}",
        base32(milliseconds, 10),
        base32(entropy(since_epoch), 16)
    ))
}

fn entropy(since_epoch: std::time::Duration) -> u128 {
    use std::hash::{BuildHasher, Hasher};

    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(since_epoch.as_nanos());
    hasher.write_u32(std::process::id());
    ((hasher.finish() as u128) << 16) | (std::process::id() as u128 & 0xffff)
}

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
            disposition: None,
        }
    }

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
