//! What fiddle can do to the world, and the proof it is allowed to.
//!
//! A capability is the only thing in fiddle that *changes* anything, so the
//! interesting design question is not what it does but what it takes to reach
//! it. Design §4.4 states the rule: `execute` is reached only via
//! [`NextAction::Execute`]. That rule is made structural here rather than
//! enforced by a well-placed `if` — [`Capability::execute`] demands an
//! [`ExecutionGrant`], and the only way to obtain one is to hand
//! [`ExecutionGrant::authorise`] a derivation that said `Execute`. A caller who
//! forgets the check cannot compile, which is a stronger guarantee than a
//! caller who remembers it today.
//!
//! M0 ships exactly one capability, [`StubMark`], which writes this
//! invocation's correlation key into the fixture change set. It makes no
//! network call, no model call, and no `git` invocation, so the same fixture
//! and the same invocation reference always produce byte-identical output —
//! which is what makes the two-invocation stability proof checkable.

use crate::stub::STUB_ORIGIN;
use fiddle_core::{correlation_key, CapabilityId, ChangeSetState, EvidenceRef, NextAction};
use std::path::{Path, PathBuf};

/// Every capability this build can execute.
///
/// The single source of the known-id list: the CLI validates `--capability`
/// against it, so a build that gains a capability offers it and names it in a
/// diagnostic without anyone remembering to update a second list.
pub const CAPABILITIES: [CapabilityId; 1] = [fiddle_core::STUB_MARK];

/// Proof that a derivation authorised an execution.
///
/// The field is private and the only constructor is [`ExecutionGrant::authorise`],
/// so a value of this type cannot exist unless some [`NextAction`] was
/// `Execute`. That is the whole point: "the capability is never executed from a
/// blocked derivation" stops being a property of the orchestration's control
/// flow and becomes a property of the types, checkable by the compiler at every
/// call site that will ever exist.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionGrant {
    capability_id: CapabilityId,
}

impl ExecutionGrant {
    /// A grant for `action`, if and only if it authorises an execution.
    ///
    /// `Complete` and `Blocked` yield `None`, and there is no other way in.
    pub fn authorise(action: &NextAction) -> Option<Self> {
        match action {
            NextAction::Execute { capability_id } => Some(ExecutionGrant {
                capability_id: *capability_id,
            }),
            NextAction::Complete | NextAction::Blocked { .. } => None,
        }
    }

    /// The capability the derivation named.
    pub fn capability_id(self) -> CapabilityId {
        self.capability_id
    }
}

/// Something fiddle can do that changes the world.
///
/// Deliberately not `async` in M0: the one capability writes a single file and
/// never yields, so an executor would have nothing to drive. The seam that
/// matters is this trait — an awaiting capability changes this signature and
/// its two call sites, not the shape of the orchestration around it.
pub trait Capability: Send + Sync {
    /// The identity this capability is derived and reported under.
    fn id(&self) -> CapabilityId;

    /// Do the thing, and hand back what a reader can go and check.
    ///
    /// The `grant` argument is not consulted for permission by convention; it
    /// *is* the permission, and an implementation must reject a grant naming a
    /// different capability rather than doing that capability's work.
    fn execute(
        &self,
        grant: ExecutionGrant,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError>;
}

/// Why an execution did not produce evidence.
///
/// Every variant names the path or the identity involved, because a capability
/// failure surfaces to an operator as a run outcome's `reason` and a bare
/// "write failed" would leave them nothing to act on.
#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    /// The grant authorised a different capability than the one asked to run.
    ///
    /// Unreachable through the M0 orchestration, which only ever asks the
    /// capability the derivation named — but the check belongs to the
    /// capability, so that adding a second one cannot make the mismatch
    /// possible without also making it an error.
    #[error("capability `{requested}` was asked to run under a grant for `{granted}`")]
    NotAuthorised {
        granted: CapabilityId,
        requested: CapabilityId,
    },

    /// The change set could not be recorded.
    #[error("could not record the change set at {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

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

impl Capability for StubMark {
    fn id(&self) -> CapabilityId {
        fiddle_core::STUB_MARK
    }

    fn execute(
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
fn write_atomically(destination: &Path, state: &ChangeSetState) -> std::io::Result<()> {
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
    use fiddle_core::{Observation, STUB_MARK};

    const WORK_ID: &str = "fiddle-m0-demo";
    const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

    fn grant() -> ExecutionGrant {
        ExecutionGrant::authorise(&NextAction::Execute {
            capability_id: STUB_MARK,
        })
        .expect("an Execute derivation authorises")
    }

    /// The fail-closed rule, stated against the type rather than against a
    /// branch: the two non-executing derivations yield no grant at all, so no
    /// call to `execute` can be written from them.
    #[test]
    fn only_an_execute_derivation_yields_a_grant() {
        assert_eq!(grant().capability_id(), STUB_MARK);
        assert_eq!(ExecutionGrant::authorise(&NextAction::Complete), None);
        assert_eq!(
            ExecutionGrant::authorise(&NextAction::Blocked {
                reason: "unobservable".into()
            }),
            None
        );
    }

    /// What the capability writes must be what the change port reads back —
    /// asserted through the port rather than by re-parsing the file, because
    /// the next invocation's assessment reaches it that way.
    #[test]
    fn the_marker_it_writes_is_the_marker_the_change_port_observes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let capability = StubMark::new(root, "icecube");

        let evidence = capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
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
    #[test]
    fn executing_twice_produces_byte_identical_output() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("changes/fiddle-m0-demo.json");
        let capability = StubMark::new(dir.path(), "icecube");

        capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .unwrap();
        let first = std::fs::read(&path).unwrap();
        capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), first);
    }

    /// The rename is what makes a partial write unobservable; the temporary
    /// path it goes through must not survive a successful run either.
    #[test]
    fn it_leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().unwrap();
        let capability = StubMark::new(dir.path(), "icecube");
        capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
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
    #[test]
    fn a_grant_for_another_capability_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let foreign = ExecutionGrant::authorise(&NextAction::Execute {
            capability_id: CapabilityId("other_capability"),
        })
        .unwrap();

        let error = StubMark::new(dir.path(), "icecube")
            .execute(foreign, WORK_ID, INVOCATION_REF)
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
    #[test]
    fn an_unwritable_root_is_reported_with_its_path() {
        let dir = tempfile::tempdir().unwrap();
        // A file where the `changes` directory must go: `create_dir_all` cannot
        // succeed, so the failure happens before any byte is written.
        std::fs::write(dir.path().join("changes"), "not a directory").unwrap();

        let error = StubMark::new(dir.path(), "icecube")
            .execute(grant(), WORK_ID, INVOCATION_REF)
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
