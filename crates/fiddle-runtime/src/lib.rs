//! Runtime layer for fiddle.
//!
//! Everything that touches the outside world — ports, stub adapters,
//! capabilities, orchestration, journalling, and evidence publication — lives
//! here, so `fiddle-core` can stay pure. `ports` and `stub` are the observation
//! seam — the traits the core's world is observed through and M0's fixture-backed
//! implementations of them — `capability` is what fiddle can change about that
//! world, `journal` is where an attempt writes down what it is about to change
//! before it changes it, `evidence` is how what it did is published where someone
//! else can read it, and `orchestration` is the plan that decides whether to act
//! and owns the whole attempt from observation to publication.
//!
//! [`attempt`] is the front door: one call executes and records one attempt.
//! Publication is deliberately not re-exported beside it, because "execute" and
//! "record" being separately callable is what let a capability change the world
//! with nothing on disk saying so.

pub mod capability;
pub mod evidence;
pub mod journal;
pub mod orchestration;
pub mod ports;
pub mod stub;

pub use capability::{Capability, CapabilityError, ExecutionGrant, StubMark, CAPABILITIES};
pub use evidence::{EvidenceError, BUNDLE_FILE};
pub use fiddle_core as core;
pub use journal::AttemptJournal;
pub use orchestration::{
    attempt, observe, run, AttemptContext, AttemptRecord, RunContext, RunReport,
};
pub use ports::{ChangePort, WorkItemPort};
pub use stub::{StubChangePort, StubWorkItemPort};
