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
//! live — it does not. `gateway` is the single construction of a model that
//! talks to a real provider, and the only reason this crate has one: everything
//! else is generic over Rig's completion-model trait, which is what lets the
//! milestone's central property be proven offline. `github` is the second
//! credential-carrying construction beside it — one `gh`, one environment, one
//! place to look — and `git` is the third: the one `git push` that publishes a
//! branch, carrying its credential through git's environment configuration
//! channel because `argv` is world-readable and the environment is not.
//! `effect` is the mandatory authorization boundary in front
//! of it: the executor that walks validate → identity → postcondition → policy →
//! authorize → delegate → observe, the envelope no caller can forge, and the
//! vocabulary in which the difference between a refused write and a lost answer
//! is made. `process` is private and holds the one thing every child this runtime
//! spawns has in common: a deadline it cannot outlive and a process group that
//! dies with it. What a child may *see* is never shared; only the bound is.
//!
//! [`attempt`] is the front door: one call executes and records one attempt.
//! Publication is deliberately not re-exported beside it, because "execute" and
//! "record" being separately callable is what let a capability change the world
//! with nothing on disk saying so.

pub mod agent;
pub mod capability;
pub mod effect;
pub mod evidence;
pub mod gateway;
pub mod git;
pub mod github;
pub mod journal;
pub mod orchestration;
pub mod ports;
pub(crate) mod process;
pub mod stub;
pub mod workspace;

pub use agent::{AgentBudget, ToolHost, ToolReceipt, ToolReceipts};
pub use capability::{
    Capability, CapabilityError, ExecutionGrant, FixtureRepair, PublishChange, PublishConfig,
    RepairConfig, StubMark, CAPABILITIES,
};
// `mint_attempt_id` is deliberately *not* re-exported beside these, for the
// same reason publication is not re-exported beside [`attempt`]: minting an id
// out here is what let a caller name an attempt the run it belonged to had never
// heard of. [`attempt`] mints exactly one and hands it to the capability through
// its [`ExecutionGrant`]. It stays reachable as `evidence::mint_attempt_id`;
// what it is no longer is the front door.
pub use effect::{
    AuthorizedEffect, DeploymentPolicy, EffectContext, EffectError, EffectOutcome, EffectReceipt,
    EffectTrace, ExecutionStep, Executor, IntegrationOperation, ObservedState,
};
pub use evidence::{EvidenceError, BUNDLE_FILE};
pub use fiddle_core as core;
pub use gateway::{completion_model, GatewayError, GatewayModel};
pub use git::{GitCli, GitError, PublishedBranch};
pub use github::{branch_name, branch_target, BranchRef, EnsureBranchPublished};
// `classify` and `run_name` are deliberately not here. They read as check
// vocabulary under `github::checks` and as nothing in particular at the root of
// a crate that also has runs, attempts and outcomes; both stay reachable where
// their meaning is.
pub use github::{
    check_request_target, observe_checks, CheckState, EnsureCheckRequested, WorkflowRun,
};
pub use github::{pull_request_target, EnsurePullRequest, PullRequest};
pub use github::{GhCli, GhError, GhResponse};
pub use journal::{AttemptJournal, AttemptTrace};
pub use orchestration::{
    attempt, observe, run, AttemptContext, AttemptRecord, RunContext, RunReport,
};
pub use ports::{ChangePort, WorkItemPort};
pub use stub::{StubChangePort, StubWorkItemPort};
pub use workspace::{WorkspaceCommand, WorkspaceError, WorkspacePath};
