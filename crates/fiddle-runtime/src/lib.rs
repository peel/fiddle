//! Runtime layer for fiddle.
//!
//! Everything that touches the outside world — ports, stub adapters,
//! capabilities, orchestration, journalling, and evidence publication — lives
//! here, so `fiddle-core` can stay pure. `ports` and `stub` are the observation
//! seam — the traits the core's world is observed through and M0's fixture-backed
//! implementations of them — `capability` is what fiddle can change about that
//! world, `journal` is where an attempt writes down what it is about to change
//! before it changes it, `evidence` is how what it did is published where someone
//! else can read it, `orchestration` is the plan that decides whether to act
//! and owns the whole attempt from observation to publication, and `workspace`
//! is where a path a model asked for is proven to stay inside the tree it is
//! allowed to touch before anything opens it. `agent` is the only part of any
//! of it a model can see: four tools, whose arguments the model authors and
//! whose context — which workspace, which check, whether the attempt is still
//! live — it does not.
//!
//! [`attempt`] is the front door: one call executes and records one attempt.
//! Publication is deliberately not re-exported beside it, because "execute" and
//! "record" being separately callable is what let a capability change the world
//! with nothing on disk saying so.

pub mod agent;
pub mod capability;
pub mod evidence;
pub mod journal;
pub mod orchestration;
pub mod ports;
pub mod stub;
pub mod workspace;

pub use agent::{ToolHost, ToolReceipt, ToolReceipts};
pub use capability::{Capability, CapabilityError, ExecutionGrant, StubMark, CAPABILITIES};
pub use evidence::{EvidenceError, BUNDLE_FILE};
pub use fiddle_core as core;
pub use journal::AttemptJournal;
pub use orchestration::{
    attempt, observe, run, AttemptContext, AttemptRecord, RunContext, RunReport,
};
pub use ports::{ChangePort, WorkItemPort};
pub use stub::{StubChangePort, StubWorkItemPort};
pub use workspace::{WorkspaceError, WorkspacePath};
