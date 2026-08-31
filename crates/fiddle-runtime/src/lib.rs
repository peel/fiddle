extern crate self as fiddle_runtime;

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
pub mod jira;
pub mod journal;
pub mod orchestration;
pub mod ports;
pub(crate) mod process;
pub mod scanner;
pub mod stub;
pub mod toil;
pub mod workspace;

#[doc(hidden)]
pub mod derive_support {
    pub use async_trait::async_trait;
    pub use serde_json;
}

pub use agent::{
    AgentBudget, Direction, ToolHost, ToolReceipt, ToolReceipts, TranscriptHook, Transcripts,
};
pub use capability::{
    attempt_worktree, Capability, CapabilityError, CveMitigate, Executed, ExecutionGrant,
    ExecutionInput, FixtureRepair, MitigateConfig, ProposeChange, ProposeConfig, PublishChange,
    PublishConfig, RepairConfig, StubMark, CAPABILITIES,
};
pub use effect::{
    AdapterError, AuthorizedEffect, DeploymentPolicy, EffectContext, EffectError, EffectOutcome,
    EffectPhase, EffectReceipt, EffectTrace, ExecutionStep, Executor, IntegrationOperation,
    ObservedState, ResolvedDecision,
};
pub use evidence::{EvidenceError, BUNDLE_FILE};
pub use fiddle_core as core;
pub use gateway::{completion_model, Gateway, GatewayError, GatewayModel, Redaction, REDACTED};
pub use git::{GitCli, GitError, PublishedBranch};
pub use github::{branch_name, branch_target, BranchRef, EnsureBranchPublished};
pub use github::{
    check_request_target, observe_checks, CheckState, EnsureCheckRequested, WorkflowRun,
};
pub use github::{pull_request_target, EnsurePullRequest, PullRequest};
pub use github::{GhCli, GhError, GhResponse, RetryAdvice};
pub use human::{
    authoritative, decision_request_target, publish, render_request, ChannelError, DecisionChannel,
    GitHubConversation, HumanInteractionPort, InteractionRef, PublishDecisionRequest, PublishError,
    PublishedAsk, PublishedRequest,
};
pub use jira::{
    project, ConfiguredNames, ConversationError, JiraActor, JiraConversation, JiraError, JiraHttp,
    JiraReply, JiraResponse, JiraWorkItemPort,
};
pub use journal::{AttemptJournal, AttemptTrace};
pub use orchestration::{
    attempt, observe, run, Addressed, AttemptContext, AttemptRecord, RunContext, RunReport,
};
pub use ports::{ChangePort, WorkItemPort};
pub use scanner::{ScanError, ScanReport, Scanner, Wizcli};
pub use stub::{StubChangePort, StubWorkItemPort};
pub use workspace::{DeclaredCommand, Extend, WorkspaceCommand, WorkspaceError, WorkspacePath};
