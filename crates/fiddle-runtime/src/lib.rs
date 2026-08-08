//! Runtime layer for fiddle.
//!
//! Everything that touches the outside world — ports, stub adapters,
//! capabilities, orchestration, and evidence publication — lives here, so
//! `fiddle-core` can stay pure. A later M0 task adds evidence publication;
//! `ports` and `stub` are the observation seam — the traits the core's world is
//! observed through and M0's fixture-backed implementations of them —
//! `capability` is what fiddle can change about that world, and `orchestration`
//! is the plan that decides whether to.

pub mod capability;
pub mod orchestration;
pub mod ports;
pub mod stub;

pub use capability::{Capability, CapabilityError, ExecutionGrant, StubMark, CAPABILITIES};
pub use fiddle_core as core;
pub use orchestration::{observe, run, RunContext, RunReport};
pub use ports::{ChangePort, WorkItemPort};
pub use stub::{StubChangePort, StubWorkItemPort};
