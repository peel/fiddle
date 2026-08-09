//! The fixture-backed capability: record this invocation's correlation key.

use super::{Capability, CapabilityError, ExecutionGrant};
use crate::stub::STUB_ORIGIN;
use fiddle_core::{correlation_key, CapabilityId, ChangeSetState, EvidenceRef};
use std::path::{Path, PathBuf};

/// The M0 capability: record this invocation's correlation key as the change
/// set for the work item.
///
/// Holds the project name because the correlation key is a function of the
/// project *and* the invocation reference (design §4.3); the capability derives
/// the key it writes from the same pure function the assessment compares
/// against, so a run's own marker is by construction the one its next
/// assessment recognises.
pub struct StubMark {
    root: PathBuf,
    project: String,
}

impl StubMark {
    /// A capability writing into the fixture root at `root` on behalf of
    /// `project`.
    pub fn new(root: impl Into<PathBuf>, project: impl Into<String>) -> Self {
        StubMark {
            root: root.into(),
            project: project.into(),
        }
    }
}

#[async_trait::async_trait]
impl Capability for StubMark {
    fn id(&self) -> CapabilityId {
        fiddle_core::STUB_MARK
    }

    /// The one stage M0's capability has, and the name every M0 bundle already
    /// carries. Stated here rather than in the orchestration so that it is this
    /// capability's word for its own step.
    fn stage(&self) -> &'static str {
        "mark"
    }

    async fn execute(
        &self,
        grant: ExecutionGrant,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        if grant.capability_id() != self.id() {
            return Err(CapabilityError::NotAuthorised {
                granted: grant.capability_id(),
                requested: self.id(),
            });
        }

        let state = ChangeSetState {
            marker: Some(correlation_key(&self.project, invocation_ref)),
        };
        let relative = format!("changes/{work_id}.json");
        let destination = self.root.join(&relative);
        write_atomically(&destination, &state).map_err(|source| CapabilityError::Write {
            path: destination.clone(),
            source,
        })?;
        Ok(EvidenceRef(format!("{STUB_ORIGIN}:{relative}")))
    }
}

/// Serialize `state` to `destination` so that no reader ever observes it
/// half-written.
///
/// Write-to-temp-then-rename, because the change set is the very file the next
/// invocation's assessment reads: a torn write would be observed as a malformed
/// source, which fails closed to `Blocked` and would strand the work. The
/// temporary file is removed on every failure path, so a run that could not
/// finish leaves no debris behind for the next one to trip over.
///
/// Shared with [`super::repair`] rather than reimplemented there: both
/// capabilities write the same file for the same reader, and two spellings of
/// "record the change set" would be two chances to get the torn-write rule
/// wrong.
pub(super) fn write_atomically(destination: &Path, state: &ChangeSetState) -> std::io::Result<()> {
    let directory = destination.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(directory)?;

    let name = destination
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let temporary = directory.join(format!(".{name}.tmp"));

    let attempt = (|| {
        let bytes = serde_json::to_vec_pretty(state).map_err(std::io::Error::other)?;
        std::fs::write(&temporary, bytes)?;
        std::fs::rename(&temporary, destination)
    })();

    if attempt.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    attempt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::ChangePort;
    use crate::stub::StubChangePort;
    use fiddle_core::{AttemptId, NextAction, Observation, STUB_MARK};

    const WORK_ID: &str = "fiddle-m0-demo";
    const INVOCATION_REF: &str = "beans:fiddle-m0-demo";
    const ATTEMPT: &str = "01JQZX0000000000000000000";

    fn grant() -> ExecutionGrant {
        ExecutionGrant::authorise(
            &NextAction::Execute {
                capability_id: STUB_MARK,
            },
            &AttemptId(ATTEMPT.to_string()),
        )
        .expect("an Execute derivation authorises")
    }

    /// What the capability writes must be what the change port reads back —
    /// asserted through the port rather than by re-parsing the file, because
    /// the next invocation's assessment reaches it that way.
    #[tokio::test]
    async fn the_marker_it_writes_is_the_marker_the_change_port_observes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let capability = StubMark::new(root, "icecube");

        let evidence = capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .unwrap();
        assert_eq!(evidence.0, "stub:changes/fiddle-m0-demo.json");

        match StubChangePort::new(root).observe(WORK_ID) {
            Observation::Available { value, .. } => assert_eq!(
                value.marker.as_deref(),
                Some(correlation_key("icecube", INVOCATION_REF).as_str())
            ),
            other => panic!("the written change set must be observable, got {other:?}"),
        }
    }

    /// Executing twice must land on the same bytes: the capability is a
    /// function of the fixture and the invocation reference, which is what the
    /// stability proof rests on.
    #[tokio::test]
    async fn executing_twice_produces_byte_identical_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("changes/fiddle-m0-demo.json");
        let capability = StubMark::new(dir.path(), "icecube");

        capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .unwrap();
        let first = std::fs::read(&path).unwrap();
        capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), first);
    }

    /// The rename is what makes a partial write unobservable; the temporary
    /// path it goes through must not survive a successful run either.
    #[tokio::test]
    async fn it_leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let capability = StubMark::new(dir.path(), "icecube");
        capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("changes"))
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name != "fiddle-m0-demo.json")
            .collect();
        assert!(leftovers.is_empty(), "got {leftovers:?}");
    }

    /// A capability handed someone else's grant refuses rather than doing the
    /// work the grant was not for.
    #[tokio::test]
    async fn a_grant_for_another_capability_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let foreign = ExecutionGrant::authorise(
            &NextAction::Execute {
                capability_id: CapabilityId("other_capability"),
            },
            &AttemptId(ATTEMPT.to_string()),
        )
        .unwrap();

        let error = StubMark::new(dir.path(), "icecube")
            .execute(foreign, WORK_ID, INVOCATION_REF)
            .await
            .unwrap_err();

        assert!(
            matches!(error, CapabilityError::NotAuthorised { .. }),
            "got {error:?}"
        );
        assert!(
            !dir.path().join("changes").exists(),
            "a refused execution must write nothing"
        );
    }

    /// A write that cannot land is reported with the path it failed on, and
    /// leaves nothing behind — the caller turns this into a retryable outcome.
    #[tokio::test]
    async fn an_unwritable_root_is_reported_with_its_path() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the `changes` directory must go: `create_dir_all` cannot
        // succeed, so the failure happens before any byte is written.
        std::fs::write(dir.path().join("changes"), "not a directory").unwrap();

        let error = StubMark::new(dir.path(), "icecube")
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .unwrap_err();

        match error {
            CapabilityError::Write { path, .. } => {
                assert!(
                    path.ends_with("changes/fiddle-m0-demo.json"),
                    "got {path:?}"
                );
            }
            other => panic!("an unwritable root must report a write failure, got {other:?}"),
        }
    }
}
