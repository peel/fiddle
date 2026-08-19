use crate::effect::{EffectTrace, ExecutionStep};
use crate::evidence::{EvidenceError, BUNDLE_FILE};
use crate::human::validate::{DecisionStep, DecisionTrace};
use fiddle_core::{AttemptId, CapabilityId, EffectKind, EvidenceRef};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const JOURNAL_DIR: &str = ".attempts";

pub trait AttemptJournal: Send + Sync {
    fn record_intent(&self, capability: CapabilityId) -> Result<(), EvidenceError>;

    fn record_step(&self, kind: EffectKind, step: ExecutionStep);

    fn record_decision_step(&self, step: DecisionStep);

    fn record_effect(&self, capability: CapabilityId, status: &str, evidence: &[EvidenceRef]);

    fn supersede(&self);
}

pub struct FileJournal {
    path: PathBuf,
    attempt: AttemptId,
    invocation_ref: String,
}

impl FileJournal {
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

    fn record_step(&self, kind: EffectKind, step: ExecutionStep) {
        let _ = self.append(&serde_json::json!({
            "record": "effect_step",
            "attempt_id": self.attempt,
            "kind": kind.as_str(),
            "step": step.as_str(),
        }));
    }

    fn record_decision_step(&self, step: DecisionStep) {
        let _ = self.append(&serde_json::json!({
            "record": "decision_step",
            "attempt_id": self.attempt,
            "step": step.as_str(),
        }));
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
        let _ = std::fs::remove_file(&self.path);
    }
}

pub struct AttemptTrace {
    journal: Mutex<Option<Arc<dyn AttemptJournal>>>,
}

impl AttemptTrace {
    pub fn new() -> Self {
        AttemptTrace {
            journal: Mutex::new(None),
        }
    }

    pub fn attach(&self, journal: Arc<dyn AttemptJournal>) {
        *self.journal.lock().unwrap() = Some(journal);
    }
}

impl Default for AttemptTrace {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectTrace for AttemptTrace {
    fn step(&self, kind: EffectKind, step: ExecutionStep) {
        let journal = self.journal.lock().unwrap().clone();
        if let Some(journal) = journal {
            journal.record_step(kind, step);
        }
    }
}

impl DecisionTrace for AttemptTrace {
    fn step(&self, step: DecisionStep) {
        let journal = self.journal.lock().unwrap().clone();
        if let Some(journal) = journal {
            journal.record_decision_step(step);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterruptedAttempt {
    pub attempt_id: AttemptId,
    pub capability: String,
    pub effect: Option<String>,
}

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

    #[test]
    fn a_superseded_journal_reports_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let recording = journal(dir.path(), &attempt);
        recording.record_intent(STUB_MARK).unwrap();

        recording.supersede();

        assert!(interrupted(dir.path(), SLUG).is_empty());
    }

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
            return;
        }
        let error = recorded.unwrap_err();
        assert!(
            matches!(error, EvidenceError::Journal { .. }),
            "got {error:?}"
        );
        assert!(error.to_string().contains("attempt journal"), "got {error}");
    }

    #[test]
    fn a_step_record_is_appended_and_leaves_the_intent_reading_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let recording = journal(dir.path(), &attempt);

        recording.record_intent(STUB_MARK).unwrap();
        recording.record_step(EffectKind::EnsurePullRequest, ExecutionStep::Apply);

        let written = std::fs::read_to_string(
            dir.path()
                .join(JOURNAL_DIR)
                .join(SLUG)
                .join("01ATTEMPT.jsonl"),
        )
        .unwrap();
        assert_eq!(written.lines().count(), 2, "appended, not rewritten");
        assert!(
            written.contains(r#""step":"apply""#)
                && written.contains(r#""kind":"ensure_pull_request""#),
            "the step and the effect it belongs to must both be there: {written}"
        );
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
    fn a_decision_step_is_recorded_beside_an_effect_step_and_neither_is_an_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let recording = journal(dir.path(), &attempt);

        recording.record_intent(STUB_MARK).unwrap();
        recording.record_step(EffectKind::EnsurePullRequestReady, ExecutionStep::Apply);
        recording.record_decision_step(DecisionStep::ReObserveState);

        let written = std::fs::read_to_string(
            dir.path()
                .join(JOURNAL_DIR)
                .join(SLUG)
                .join("01ATTEMPT.jsonl"),
        )
        .unwrap();
        assert_eq!(written.lines().count(), 3, "appended, not rewritten");
        assert!(
            written.contains(r#""record":"decision_step""#)
                && written.contains(r#""step":"re_observe_state""#),
            "the validation step and its record kind must both be there: {written}"
        );
        assert!(
            written.contains(r#""record":"effect_step""#),
            "the two orders are two records: {written}"
        );
        let decision: serde_json::Value = written
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|record| record["record"] == "decision_step")
            .expect("the decision record is there");
        assert_eq!(decision["kind"], serde_json::Value::Null, "{decision}");

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
    fn a_trace_records_nothing_until_an_attempt_attaches_its_journal() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let path = dir
            .path()
            .join(JOURNAL_DIR)
            .join(SLUG)
            .join("01ATTEMPT.jsonl");

        let trace = AttemptTrace::new();
        EffectTrace::step(
            &trace,
            EffectKind::EnsureBranchPublished,
            ExecutionStep::Apply,
        );
        DecisionTrace::step(&trace, DecisionStep::RecomputeIdentity);
        assert!(
            !path.exists(),
            "an unattached trace must not create a journal of its own, on either \
             order"
        );

        trace.attach(Arc::new(journal(dir.path(), &attempt)));
        EffectTrace::step(
            &trace,
            EffectKind::EnsureBranchPublished,
            ExecutionStep::Apply,
        );
        DecisionTrace::step(&trace, DecisionStep::RecomputeIdentity);

        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            written.lines().count(),
            2,
            "exactly the two steps recorded after attaching, one per order: {written}"
        );
        assert!(
            written.contains(r#""record":"effect_step""#)
                && written.contains(r#""record":"decision_step""#),
            "both orders share this attempt's journal: {written}"
        );
    }

    #[test]
    fn journals_are_reported_per_invocation() {
        let dir = tempfile::tempdir().unwrap();
        journal(dir.path(), &AttemptId("01ATTEMPT".to_string()))
            .record_intent(STUB_MARK)
            .unwrap();

        assert!(interrupted(dir.path(), "jira-ICE-1").is_empty());
        assert_eq!(interrupted(dir.path(), SLUG).len(), 1);
    }

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
