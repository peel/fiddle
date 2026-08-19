use super::{Capability, CapabilityError, ExecutionGrant};
use crate::stub::STUB_ORIGIN;
use fiddle_core::{correlation_key, CapabilityId, ChangeSetState, EvidenceRef};
use std::path::{Path, PathBuf};

pub struct StubMark {
    root: PathBuf,
    project: String,
}

impl StubMark {
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

    #[tokio::test]
    async fn an_unwritable_root_is_reported_with_its_path() {
        let dir = tempfile::tempdir().unwrap();
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
