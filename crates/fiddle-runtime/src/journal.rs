//! What an attempt was about to do, written down before it does it.
//!
//! The report bundle says what an attempt *did*, and it is published in one
//! atomic move once the attempt is over. That leaves a window: between a
//! capability changing the world and the bundle landing, nothing on disk records
//! that the world changed. In M0 the window is survivable by luck —
//! `stub_mark` writes a deterministic marker, so a later attempt observes
//! `Satisfied` and carries on. M2's effects are a branch and a pull request,
//! which are not idempotent: there, an attempt that mutated and was never
//! recorded is indistinguishable from one that never ran, and a retry has to
//! choose between duplicating the effect and skipping it. Neither is correct,
//! and no amount of reading the published bundles can tell the two apart,
//! because the whole problem is that no bundle exists.
//!
//! So the intent is recorded first. Before a capability is reached, this module
//! appends one line saying which capability is about to run under which attempt
//! id; after it returns, it appends a second saying how that ended. The two
//! records are what make an interrupted attempt *findable* — see
//! [`interrupted`].
//!
//! Three properties are worth the code:
//!
//! - **The journal is never the source of truth.** The published bundle is, and
//!   the journal is deleted the moment one lands ([`AttemptJournal::supersede`]).
//!   So for any attempt id at most one of the two exists, and they cannot be
//!   read as disagreeing: a journal record means "this attempt did not finish
//!   recording itself", full stop.
//! - **Only an attempt's own bundle clears its record.** A *later* attempt
//!   publishing successfully does not, even over the same work. Concluding that
//!   an earlier unrecorded effect is accounted for by a later attempt's bundle
//!   requires knowing the capability is idempotent — true of `stub_mark`, false
//!   of a branch and a pull request. So a record outlives every subsequent
//!   attempt until someone looks at it, which is the point: it is the standing
//!   signal that a change was made and never written down.
//! - **Recording the intent is fallible, and failing to record it stops the
//!   attempt.** A capability that ran without a durable record is precisely the
//!   hazard, so an unrecordable intent fails closed rather than proceeding
//!   unrecorded. That is why [`AttemptJournal::record_intent`] returns a
//!   `Result` and [`AttemptJournal::record_effect`] does not.
//! - **Records are appended, not rewritten.** An append cannot damage the line
//!   before it, so the intent record survives whatever happens to the effect
//!   record.

use crate::evidence::{EvidenceError, BUNDLE_FILE};
use fiddle_core::{AttemptId, CapabilityId, EvidenceRef};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The directory under `<report.dir>` holding attempt journals.
///
/// Dot-prefixed, and therefore unable to collide with a published invocation
/// directory: those are named by [`fiddle_core::InvocationRef::slug`], which
/// always begins with a scheme name.
pub const JOURNAL_DIR: &str = ".attempts";

/// What an attempt records about itself before, and after, changing the world.
///
/// A trait rather than a concrete type so the orchestration depends on the
/// recording seam instead of on the filesystem — which is what lets a test
/// assert the *ordering* (intent before execution) without a temporary
/// directory, and what keeps `run` free of a `<report.dir>` argument it would
/// otherwise have to thread through.
pub trait AttemptJournal {
    /// Record that this attempt is about to execute `capability`.
    ///
    /// Must not return `Ok` until the record is durable: the caller treats `Ok`
    /// as permission to change the world.
    fn record_intent(&self, capability: CapabilityId) -> Result<(), EvidenceError>;

    /// Record how the execution ended.
    ///
    /// Infallible by design. The intent record already makes the attempt
    /// detectable, and the bundle — if it publishes — is authoritative about
    /// what happened, so a failure here can neither hide an attempt nor
    /// contradict one. Returning an error would only offer the caller a decision
    /// it has no better answer for than "carry on and try to publish".
    fn record_effect(&self, capability: CapabilityId, status: &str, evidence: &[EvidenceRef]);

    /// A bundle landed; this attempt's journal has been superseded by it.
    fn supersede(&self);
}

/// The journal of one attempt, as a line-delimited JSON file under
/// `<report.dir>/.attempts/<slug>/<attempt-id>.jsonl`.
///
/// One file per attempt rather than one shared log per invocation, so two
/// concurrent attempts never interleave lines and superseding one cannot touch
/// the other's records.
pub struct FileJournal {
    path: PathBuf,
    attempt: AttemptId,
    invocation_ref: String,
}

impl FileJournal {
    /// The journal for `attempt` over `slug`, publishing under `report_dir`.
    pub fn new(
        report_dir: &Path,
        slug: &str,
        attempt: &AttemptId,
        invocation_ref: &str,
    ) -> FileJournal {
        FileJournal {
            path: journal_dir(report_dir, slug).join(format!("{}.jsonl", attempt.0)),
            attempt: attempt.clone(),
            invocation_ref: invocation_ref.to_string(),
        }
    }

    /// Append one record and return only once it is durable.
    ///
    /// `sync_all` on the file is what makes "durable" more than "handed to the
    /// page cache"; the directory sync that follows is what makes the file's
    /// *existence* durable rather than only its contents. The directory sync is
    /// best effort because not every platform admits opening a directory for
    /// it — where it is refused, the file contents are still synced, which is
    /// the larger half of the guarantee.
    fn append(&self, record: &serde_json::Value) -> Result<(), EvidenceError> {
        let directory = self
            .path
            .parent()
            .expect("a journal path always has a parent directory");
        self.io(directory, std::fs::create_dir_all(directory))?;

        let line = format!("{record}\n");
        self.io(
            &self.path,
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .and_then(|mut file| {
                    file.write_all(line.as_bytes())?;
                    file.sync_all()
                }),
        )?;
        let _ = std::fs::File::open(directory).and_then(|dir| dir.sync_all());
        Ok(())
    }

    /// Attach the path an operation was attempted on to its error.
    fn io(&self, path: &Path, result: std::io::Result<()>) -> Result<(), EvidenceError> {
        result.map_err(|source| EvidenceError::Journal {
            path: path.to_path_buf(),
            source,
        })
    }
}

impl AttemptJournal for FileJournal {
    fn record_intent(&self, capability: CapabilityId) -> Result<(), EvidenceError> {
        self.append(&serde_json::json!({
            "record": "intent",
            "attempt_id": self.attempt,
            "invocation_ref": self.invocation_ref,
            "capability_id": capability,
        }))
    }

    fn record_effect(&self, capability: CapabilityId, status: &str, evidence: &[EvidenceRef]) {
        let _ = self.append(&serde_json::json!({
            "record": "effect",
            "attempt_id": self.attempt,
            "invocation_ref": self.invocation_ref,
            "capability_id": capability,
            "status": status,
            "evidence": evidence,
        }));
    }

    fn supersede(&self) {
        // Best effort, and the only correct kind: the bundle has landed, so the
        // attempt is fully recorded whatever happens to this file. A removal
        // that failed and was reported would turn a successful attempt into a
        // failed one over a file nobody needs any more.
        let _ = std::fs::remove_file(&self.path);
        // The per-invocation directory is left in place. It is where an operator
        // looks for attempts in flight, and creating and removing it around
        // every attempt would be churn for no reader's benefit.
    }
}

/// An attempt that recorded an intent to change the world and never published a
/// bundle.
///
/// This is what "detectable" means concretely, and the three fields are the
/// three things a recovery has to know: which attempt, what it was going to do,
/// and whether it got as far as saying how that went.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterruptedAttempt {
    pub attempt_id: AttemptId,
    /// The capability the attempt intended to execute.
    pub capability: String,
    /// The status the journal recorded for the execution, or `None` when the
    /// attempt was interrupted before its capability returned.
    ///
    /// `None` is the dangerous case and is deliberately distinguishable: the
    /// world may or may not have moved, and only an inspection of the effect
    /// itself can say which. `Some("completed")` is the recoverable case — the
    /// world moved, it was recorded, and only the bundle is missing.
    pub effect: Option<String>,
}

/// Every attempt over `slug` that the journal still holds a record for and that
/// published no bundle, oldest first.
///
/// Both halves are checked rather than trusting the removal in
/// [`AttemptJournal::supersede`]: a process killed between the rename and the
/// removal would leave a record beside a perfectly good bundle, and reporting
/// that as interrupted would send a recovery after work that is already
/// recorded.
///
/// Attempt ids sort in the order they were minted, so the returned list is in
/// the order the attempts happened.
pub fn interrupted(report_dir: &Path, slug: &str) -> Vec<InterruptedAttempt> {
    let Ok(entries) = std::fs::read_dir(journal_dir(report_dir, slug)) else {
        return Vec::new();
    };

    let mut found: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let id = AttemptId(path.file_stem()?.to_str()?.to_string());
            if report_dir
                .join(slug)
                .join(&id.0)
                .join(BUNDLE_FILE)
                .try_exists()
                .unwrap_or(false)
            {
                return None;
            }
            read_records(&path, id)
        })
        .collect();
    found.sort_by(|a, b| a.attempt_id.cmp(&b.attempt_id));
    found
}

/// Fold one journal file into the attempt it describes, or `None` when it holds
/// no intent record.
///
/// A file without an intent record describes nothing actionable — the intent is
/// the first line written, so its absence means the file was created and never
/// completed — and reporting it would be reporting a partial write as an
/// interrupted attempt.
fn read_records(path: &Path, id: AttemptId) -> Option<InterruptedAttempt> {
    let text = std::fs::read_to_string(path).ok()?;
    let records: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    let capability = records.iter().find(|record| record["record"] == "intent")?["capability_id"]
        .as_str()?
        .to_string();
    let effect = records
        .iter()
        .rev()
        .find(|record| record["record"] == "effect")
        .and_then(|record| record["status"].as_str())
        .map(str::to_string);

    Some(InterruptedAttempt {
        attempt_id: id,
        capability,
        effect,
    })
}

/// Where the journals for `slug` live.
fn journal_dir(report_dir: &Path, slug: &str) -> PathBuf {
    report_dir.join(JOURNAL_DIR).join(slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::STUB_MARK;

    const SLUG: &str = "beans-fiddle-m0-demo";
    const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

    fn journal(root: &Path, attempt: &AttemptId) -> FileJournal {
        FileJournal::new(root, SLUG, attempt, INVOCATION_REF)
    }

    #[test]
    fn an_intent_without_an_effect_reports_an_unknown_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());

        journal(dir.path(), &attempt)
            .record_intent(STUB_MARK)
            .unwrap();

        assert_eq!(
            interrupted(dir.path(), SLUG),
            vec![InterruptedAttempt {
                attempt_id: attempt,
                capability: STUB_MARK.0.to_string(),
                effect: None,
            }]
        );
    }

    #[test]
    fn an_effect_record_is_reported_alongside_its_intent() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let recording = journal(dir.path(), &attempt);

        recording.record_intent(STUB_MARK).unwrap();
        recording.record_effect(
            STUB_MARK,
            "completed",
            &[EvidenceRef("stub:changes/x.json".to_string())],
        );

        let found = interrupted(dir.path(), SLUG);
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].effect.as_deref(), Some("completed"));
        // The evidence is in the record on disk even though `InterruptedAttempt`
        // does not project it: what a recovery reads is the file.
        let written = std::fs::read_to_string(
            dir.path()
                .join(JOURNAL_DIR)
                .join(SLUG)
                .join("01ATTEMPT.jsonl"),
        )
        .unwrap();
        assert!(written.contains("stub:changes/x.json"), "got {written}");
        assert_eq!(
            written.lines().count(),
            2,
            "records are appended: {written}"
        );
    }

    /// The journal is superseded by the bundle, not merely tidied up: once one
    /// exists the attempt is fully recorded and must stop being reported.
    #[test]
    fn a_superseded_journal_reports_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let recording = journal(dir.path(), &attempt);
        recording.record_intent(STUB_MARK).unwrap();

        recording.supersede();

        assert!(interrupted(dir.path(), SLUG).is_empty());
    }

    /// And a record that outlived its own removal must not be reported either:
    /// the bundle is the authority, so its presence settles the question.
    #[test]
    fn a_record_beside_a_published_bundle_is_not_interrupted() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        journal(dir.path(), &attempt)
            .record_intent(STUB_MARK)
            .unwrap();

        let published = dir.path().join(SLUG).join(&attempt.0);
        std::fs::create_dir_all(&published).unwrap();
        std::fs::write(published.join(BUNDLE_FILE), b"{}").unwrap();

        assert!(interrupted(dir.path(), SLUG).is_empty());
    }

    /// An unrecordable intent is an error naming the journal, so the reason the
    /// run reports tells an operator which of the three writable places is at
    /// fault.
    #[cfg(unix)]
    #[test]
    fn an_unwritable_report_dir_makes_recording_an_intent_fail() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let reports = dir.path().join("reports");
        std::fs::create_dir_all(&reports).unwrap();
        std::fs::set_permissions(&reports, std::fs::Permissions::from_mode(0o500)).unwrap();

        let recorded =
            journal(&reports, &AttemptId("01ATTEMPT".to_string())).record_intent(STUB_MARK);

        std::fs::set_permissions(&reports, std::fs::Permissions::from_mode(0o755)).unwrap();
        if recorded.is_ok() {
            return; // an identity that ignores the permission bits
        }
        let error = recorded.unwrap_err();
        assert!(
            matches!(error, EvidenceError::Journal { .. }),
            "got {error:?}"
        );
        assert!(error.to_string().contains("attempt journal"), "got {error}");
    }

    /// Journals for one invocation must not be reported under another's slug.
    #[test]
    fn journals_are_reported_per_invocation() {
        let dir = tempfile::tempdir().unwrap();
        journal(dir.path(), &AttemptId("01ATTEMPT".to_string()))
            .record_intent(STUB_MARK)
            .unwrap();

        assert!(interrupted(dir.path(), "jira-ICE-1").is_empty());
        assert_eq!(interrupted(dir.path(), SLUG).len(), 1);
    }

    /// Attempts are reported in the order they were minted, because that is the
    /// order a recovery has to work through them in.
    #[test]
    fn interrupted_attempts_are_reported_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        for id in ["01C", "01A", "01B"] {
            journal(dir.path(), &AttemptId(id.to_string()))
                .record_intent(STUB_MARK)
                .unwrap();
        }

        let ids: Vec<_> = interrupted(dir.path(), SLUG)
            .into_iter()
            .map(|found| found.attempt_id.0)
            .collect();
        assert_eq!(ids, ["01A", "01B", "01C"]);
    }
}
