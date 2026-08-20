pub mod cve;
pub mod mitigate;
pub mod propose;
pub mod publish;
pub mod repair;
pub mod stub;

pub use cve::{
    land, undeclared, DeclarationBreach, ForbiddenShape, Git, GroupMigration, GroupStatus,
    InWorktree, Landed, MigrationAttempt, MigrationConfig, NeedsWork,
};
pub use mitigate::{CveMitigate, MitigateConfig};
pub use propose::{attempt_worktree, ProposeChange, ProposeConfig};
pub use publish::{PublishChange, PublishConfig};
pub use repair::{FixtureRepair, RepairConfig};
pub use stub::StubMark;

use crate::human::validate::DecisionError;
use crate::human::InteractionRef;
use fiddle_core::{
    AttemptId, CapabilityId, DecisionRequestId, EvidenceRef, NextAction, Publication, Published,
    RunDisposition, TreeObservation,
};
use std::path::PathBuf;

pub const CAPABILITIES: [CapabilityId; 5] = [
    fiddle_core::STUB_MARK,
    fiddle_core::FIXTURE_REPAIR,
    fiddle_core::PUBLISH_CHANGE,
    fiddle_core::PROPOSE_CHANGE,
    fiddle_core::CVE_MITIGATE,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionGrant {
    capability_id: CapabilityId,
    attempt: AttemptId,
}

impl ExecutionGrant {
    pub fn authorise(action: &NextAction, attempt: &AttemptId) -> Option<Self> {
        match action {
            NextAction::Execute { capability_id } => Some(ExecutionGrant {
                capability_id: *capability_id,
                attempt: attempt.clone(),
            }),
            NextAction::Complete | NextAction::Blocked { .. } => None,
        }
    }

    pub fn capability_id(&self) -> CapabilityId {
        self.capability_id
    }

    pub fn attempt_id(&self) -> &AttemptId {
        &self.attempt
    }
}

#[async_trait::async_trait]
pub trait Capability: Send + Sync {
    fn id(&self) -> CapabilityId;

    fn stage(&self) -> &'static str;

    async fn execute(
        &self,
        grant: ExecutionGrant,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError>;

    fn receipts(&self) -> Vec<EvidenceRef> {
        Vec::new()
    }

    fn publication(&self) -> Option<Publication> {
        None
    }

    fn tree_observation(&self) -> Option<TreeObservation> {
        None
    }

    fn disposition(&self) -> Option<RunDisposition> {
        None
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error("capability `{requested}` was asked to run under a grant for `{granted}`")]
    NotAuthorised {
        granted: CapabilityId,
        requested: CapabilityId,
    },

    #[error("could not record the change set at {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "the check exited {exit_code}, so nothing was earned \
         (the model claimed completion: {claimed}): {stderr}"
    )]
    CheckFailed {
        claimed: bool,
        exit_code: i32,
        stderr: String,
    },

    #[error("the workspace could not be used: {0}")]
    Workspace(#[from] crate::workspace::WorkspaceError),

    #[error("the attempt produced no report: {0}")]
    Agent(#[from] crate::agent::AgentError),

    #[error("{0}")]
    Effect(#[from] crate::effect::EffectError),

    #[error("awaiting a human decision at {interaction} on request {}: {question}", request.0)]
    AwaitingDecision {
        request: DecisionRequestId,
        interaction: InteractionRef,
        question: String,
    },

    #[error("a person refused request {}: {reason}", request.0)]
    DecisionRejected {
        request: DecisionRequestId,
        reason: Published,
    },

    #[error("no decision could be established for request {}: {source}", request.0)]
    DecisionUnresolved {
        request: DecisionRequestId,
        #[source]
        source: DecisionError,
    },

    #[error("the attempt changed no file, so there is nothing to propose")]
    NothingProposed,

    #[error("this run publishes from {publishing} and its attempt works in {working}")]
    PublishesElsewhere {
        publishing: PathBuf,
        working: PathBuf,
    },

    #[error("{0}")]
    Scan(#[from] crate::scanner::ScanError),

    #[error("{0}")]
    Projection(#[from] crate::cve::project::ProjectionError),

    #[error("{0}")]
    Plan(#[from] crate::capability::cve::PlanError),

    #[error("{0}")]
    Dedup(#[from] crate::cve::dedup::DedupError),

    #[error("this executor is bound to `{bound}` and the run is `{asked}`")]
    Misbound { bound: String, asked: String },
}

impl CapabilityError {
    pub fn recurrence(&self) -> crate::effect::Recurrence {
        use crate::effect::Recurrence;
        match self {
            CapabilityError::Write { .. }
            | CapabilityError::CheckFailed { .. }
            | CapabilityError::Workspace(_)
            | CapabilityError::NothingProposed
            | CapabilityError::Agent(_) => Recurrence::Correctable,

            CapabilityError::NotAuthorised { .. }
            | CapabilityError::Misbound { .. }
            | CapabilityError::PublishesElsewhere { .. } => Recurrence::Permanent,

            CapabilityError::DecisionRejected { .. } => Recurrence::Permanent,

            CapabilityError::DecisionUnresolved { source, .. } => match source {
                DecisionError::Unreadable(_)
                | DecisionError::RequestAbsent(_)
                | DecisionError::ReplyEdited { .. }
                | DecisionError::HeadMoved { .. } => Recurrence::Correctable,
                DecisionError::DuplicateRequest { .. }
                | DecisionError::RequestEdited { .. }
                | DecisionError::ForeignEffect { .. }
                | DecisionError::ForeignPayload { .. }
                | DecisionError::NotOpen
                | DecisionError::AlreadyReady => Recurrence::Permanent,
            },

            CapabilityError::AwaitingDecision { .. } => Recurrence::Awaiting,

            CapabilityError::Effect(error) => error.recurrence(),

            CapabilityError::Scan(error) => error.recurrence(),

            CapabilityError::Projection(_) => Recurrence::Permanent,

            CapabilityError::Plan(error) => match error {
                cve::PlanError::Read(_) => Recurrence::Correctable,
                cve::PlanError::Refused(_) => Recurrence::Permanent,
            },

            CapabilityError::Dedup(error) => match error {
                crate::cve::dedup::DedupError::ShallowHistory { .. } => Recurrence::Permanent,
                crate::cve::dedup::DedupError::Git { .. } => Recurrence::Correctable,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::STUB_MARK;

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
    async fn a_capability_is_still_usable_as_a_trait_object() {
        let dir = tempfile::tempdir().unwrap();
        let marking = StubMark::new(dir.path(), "icecube");
        let capability: &dyn Capability = &marking;
        assert_eq!(capability.id(), STUB_MARK);
        assert!(capability
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .is_ok());
    }

    #[test]
    fn every_capability_this_build_has_is_registered() {
        assert_eq!(
            CAPABILITIES,
            [
                STUB_MARK,
                fiddle_core::FIXTURE_REPAIR,
                fiddle_core::PUBLISH_CHANGE,
                fiddle_core::PROPOSE_CHANGE,
                fiddle_core::CVE_MITIGATE
            ]
        );
    }

    #[test]
    fn only_an_execute_derivation_yields_a_grant() {
        let attempt = AttemptId(ATTEMPT.to_string());
        assert_eq!(grant().capability_id(), STUB_MARK);
        assert_eq!(
            ExecutionGrant::authorise(&NextAction::Complete, &attempt),
            None
        );
        assert_eq!(
            ExecutionGrant::authorise(
                &NextAction::Blocked {
                    reason: "unobservable".into()
                },
                &attempt
            ),
            None
        );
    }

    #[test]
    fn a_grant_names_the_attempt_it_was_issued_under() {
        assert_eq!(grant().attempt_id(), &AttemptId(ATTEMPT.to_string()));
    }

    #[test]
    fn a_scan_failure_is_given_the_row_its_own_table_decided() {
        use crate::effect::Recurrence;
        use crate::scanner::ScanError;

        let missing = CapabilityError::Scan(ScanError::Missing {
            program: PathBuf::from("/nowhere/wizcli"),
            reason: "No such file or directory".to_string(),
        });
        let daemon = CapabilityError::Scan(ScanError::DaemonUnreachable {
            stderr: "cannot connect".to_string(),
        });

        assert_eq!(
            missing.recurrence(),
            ScanError::Missing {
                program: PathBuf::from("/nowhere/wizcli"),
                reason: "No such file or directory".to_string(),
            }
            .recurrence(),
            "the capability must delegate rather than answer for itself"
        );
        assert_eq!(missing.recurrence(), Recurrence::Permanent);
        assert_eq!(daemon.recurrence(), Recurrence::Correctable);
    }

    #[test]
    fn a_branch_that_could_not_be_read_and_one_that_was_refused_are_different_rows() {
        use crate::effect::Recurrence;

        let unread = CapabilityError::Plan(cve::PlanError::Read(crate::GhError::Timeout(
            std::time::Duration::from_secs(1),
        )));
        let refused = CapabilityError::Plan(cve::PlanError::Refused(
            cve::Refusal::HeadOutsideThePushablePrefix {
                number: 7,
                head: "someones/branch".to_string(),
                prefix: cve::PUSHABLE_PREFIX,
            },
        ));

        assert_eq!(unread.recurrence(), Recurrence::Correctable);
        assert_eq!(refused.recurrence(), Recurrence::Permanent);
    }

    #[test]
    fn a_truncated_history_is_not_the_same_row_as_a_git_that_would_not_run() {
        use crate::cve::dedup::DedupError;
        use crate::effect::Recurrence;

        let unrunnable = CapabilityError::Dedup(DedupError::Git {
            repo: "/tmp/r".to_string(),
            command: "log".to_string(),
            message: "no such file".to_string(),
        });
        let shallow = CapabilityError::Dedup(DedupError::ShallowHistory {
            repo: "/tmp/r".to_string(),
            why: "the clone is shallow".to_string(),
        });

        assert_eq!(unrunnable.recurrence(), Recurrence::Correctable);
        assert_eq!(shallow.recurrence(), Recurrence::Permanent);
    }
}
