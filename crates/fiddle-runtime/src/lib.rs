pub mod agent;
pub mod capability;
pub mod cve;
pub mod effect;
pub mod evaluate;
pub mod evidence;
pub mod gateway;
pub mod git;
pub mod github;
pub mod human;
pub mod journal;
pub mod orchestration;
pub mod ports;
pub(crate) mod process;
pub mod scanner;
pub mod stub;
pub mod workspace;

pub use agent::{AgentBudget, Direction, ToolHost, ToolReceipt, ToolReceipts};
pub use capability::{
    attempt_worktree, Capability, CapabilityError, CveMitigate, ExecutionGrant, FixtureRepair,
    MitigateConfig, ProposeChange, ProposeConfig, PublishChange, PublishConfig, RepairConfig,
    StubMark, CAPABILITIES,
};
pub use effect::{
    AuthorizedEffect, DeploymentPolicy, EffectContext, EffectError, EffectOutcome, EffectReceipt,
    EffectTrace, ExecutionStep, Executor, IntegrationOperation, ObservedState, ResolvedDecision,
};
pub use evidence::{EvidenceError, BUNDLE_FILE};
pub use fiddle_core as core;
pub use gateway::{completion_model, GatewayError, GatewayModel};
pub use git::{GitCli, GitError, PublishedBranch};
pub use github::{branch_name, branch_target, BranchRef, EnsureBranchPublished};
pub use github::{
    check_request_target, observe_checks, CheckState, EnsureCheckRequested, WorkflowRun,
};
pub use github::{pull_request_target, EnsurePullRequest, PullRequest};
pub use github::{GhCli, GhError, GhResponse, RetryAdvice};
pub use human::{
    decision_request_target, render_request, GitHubConversation, HumanInteractionPort,
    InteractionRef, PublishDecisionRequest, PublishedRequest,
};
pub use journal::{AttemptJournal, AttemptTrace};
pub use orchestration::{
    attempt, observe, run, Addressed, AttemptContext, AttemptRecord, RunContext, RunReport,
};
pub use ports::{ChangePort, WorkItemPort};
pub use scanner::{ScanError, ScanReport, Scanner, WizCredential, Wizcli};
pub use stub::{StubChangePort, StubWorkItemPort};
pub use workspace::{WorkspaceCommand, WorkspaceError, WorkspacePath};
