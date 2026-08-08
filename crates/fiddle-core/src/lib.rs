//! Pure domain core for fiddle.
//!
//! This crate carries the shared types and the decision functions that map an
//! observed world onto an assessment. It is deliberately pure: no process,
//! filesystem, network, environment, or clock access, and no async runtime.
//! Later M0 tasks populate `outcome` and the rest of `report` here; `identity`
//! holds the references a run is addressed by, `observation` holds what a run
//! saw of the world, and `assessment` holds what that world means and what to
//! do about it.

pub mod assessment;
pub mod identity;
pub mod observation;
pub mod report;

pub use assessment::{
    assess, correlation_key, derive_next, CapabilityAssessment, NextAction, STUB_MARK,
};
pub use identity::{CapabilityId, InvocationRef, InvocationRefError, InvocationScheme};
pub use observation::{ChangeSetState, Observation, SourceRef, WorkItemState, WorkStateView};
pub use report::EvidenceRef;
