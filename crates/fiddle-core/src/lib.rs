//! Pure domain core for fiddle.
//!
//! This crate carries the shared types and the decision functions that map an
//! observed world onto an assessment. It is deliberately pure: no process,
//! filesystem, network, environment, or clock access, and no async runtime.
//! A later M0 task completes `report` with the published bundle; `identity`
//! holds the references a run is addressed by, `observation` holds what a run
//! saw of the world, `assessment` holds what that world means and what to do
//! about it, and `outcome` holds how the run ended.

pub mod assessment;
pub mod identity;
pub mod observation;
pub mod outcome;
pub mod report;

pub use assessment::{
    assess, correlation_key, derive_next, CapabilityAssessment, NextAction, STUB_MARK,
};
pub use identity::{CapabilityId, InvocationRef, InvocationRefError, InvocationScheme};
pub use observation::{ChangeSetState, Observation, SourceRef, WorkItemState, WorkStateView};
pub use outcome::{Mode, RunOutcome, UnknownMode};
pub use report::{CapabilityExecution, EvidenceRef, ProgressEntry};
