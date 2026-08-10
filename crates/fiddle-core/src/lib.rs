//! Pure domain core for fiddle.
//!
//! This crate carries the shared types and the decision functions that map an
//! observed world onto an assessment. It is deliberately pure: no process,
//! filesystem, network, environment, or clock access, and no async runtime.
//! `identity` holds the references a run is addressed by, `observation` holds
//! what a run saw of the world, `assessment` holds what that world means and
//! what to do about it, `outcome` holds how the run ended, `published` holds the
//! bound every piece of free text a run publishes is subject to, and `report`
//! holds the document a run publishes to say all of that to a later reader.

pub mod assessment;
pub mod identity;
pub mod observation;
pub mod outcome;
pub mod published;
pub mod report;

pub use assessment::{
    assess, correlation_key, derive_next, CapabilityAssessment, NextAction, FIXTURE_REPAIR,
    STUB_MARK,
};
pub use identity::{
    AttemptId, CapabilityId, InvocationRef, InvocationRefError, InvocationScheme, WorkRef,
};
pub use observation::{ChangeSetState, Observation, SourceRef, WorkItemState, WorkStateView};
pub use outcome::{Mode, RunOutcome, UnknownMode};
pub use published::{Published, PUBLISHED_TEXT_LIMIT};
pub use report::{
    CapabilityExecution, EvidenceRef, FiddleBuild, ProgressEntry, ReportBundle,
    CONFIG_CHECK_SCHEMA, INSPECT_SCHEMA, REPORT_SCHEMA, RUN_SCHEMA, UNKNOWN_REVISION,
};
