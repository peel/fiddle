//! Runtime layer for fiddle.
//!
//! Everything that touches the outside world — ports, stub adapters,
//! capabilities, orchestration, and evidence publication — lives here, so
//! `fiddle-core` can stay pure. Later M0 tasks populate the remaining modules;
//! `ports` and `stub` are the observation seam: the traits the core's world is
//! observed through, and M0's fixture-backed implementations of them.

pub mod ports;
pub mod stub;

pub use fiddle_core as core;
pub use ports::{ChangePort, WorkItemPort};
pub use stub::{StubChangePort, StubWorkItemPort};
