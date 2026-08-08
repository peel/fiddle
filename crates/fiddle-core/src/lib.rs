//! Pure domain core for fiddle.
//!
//! This crate carries the shared types and the decision functions that map an
//! observed world onto an assessment. It is deliberately pure: no process,
//! filesystem, network, environment, or clock access, and no async runtime.
//! Later M0 tasks populate `assessment`, `outcome`, and `report` here;
//! `identity` holds the references a run is addressed by and `observation`
//! holds what a run saw of the world.

pub mod identity;
pub mod observation;

pub use identity::{InvocationRef, InvocationRefError, InvocationScheme};
pub use observation::{ChangeSetState, Observation, SourceRef, WorkItemState, WorkStateView};
