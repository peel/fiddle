//! Pure domain core for fiddle.
//!
//! This crate carries the shared types and the decision functions that map an
//! observed world onto an assessment. It is deliberately pure: no process,
//! filesystem, network, environment, or clock access, and no async runtime.
//! `identity` holds the references a run is addressed by, `observation` holds
//! what a run saw of the world, `assessment` holds what that world means and
//! what to do about it, `effect` holds the identity by which a later process
//! recognises an external effect an earlier one performed, `policy` holds
//! whether such an effect is permitted at all and who has to be asked first,
//! `decision` holds the identity of the question put to that person, the
//! marker by which a later process finds it again, the reference by which that
//! person is recognised, and the four values their answer can amount to,
//! `outcome` holds how
//! the run ended, `published` holds the
//! bound every piece of free text a run publishes is subject to, and `report`
//! holds the document a run publishes to say all of that to a later reader.
//! `finding` holds the projection boundary in the other direction: the six
//! fields a scanner's report is allowed to become, and the one canonical
//! spelling an advisory id has once it is inside.

pub mod assessment;
pub mod decision;
pub mod effect;
pub mod finding;
pub mod identity;
pub mod observation;
pub mod outcome;
pub mod policy;
pub mod published;
pub mod report;

pub use assessment::{
    assess, correlation_key, derive_next, CapabilityAssessment, NextAction, CVE_MITIGATE,
    FIXTURE_REPAIR, PROPOSE_CHANGE, PUBLISH_CHANGE, STUB_MARK,
};
pub use decision::{
    decision_request_id, parse_marker, render_marker, ActorRef, DecisionBinding, DecisionRequestId,
    HumanDecisionRequest, InterpretedHumanDecision, MarkerError, MARKER_VERSION,
};
pub use effect::{
    content_digest, effect_id, payload_hash, EffectId, EffectKind, PayloadHash, ProposedEffect,
};
pub use finding::{
    selected, AdvisoryId, AdvisoryIdError, PackageType, ProjectedFinding, Severities,
    SeveritiesError, Severity,
};
pub use identity::{
    AttemptId, CapabilityId, InvocationRef, InvocationRefError, InvocationScheme, WorkRef,
};
pub use observation::{
    ChangeSetState, Observation, Publication, ReviewState, SourceRef, TreeObservation,
    VerificationState, WorkItemState, WorkStateView,
};
pub use outcome::{Mode, RunOutcome, UnknownMode};
pub use policy::{combine, DeploymentRule, HumanDecisionRequirement, PolicyDecision};
pub use published::{Published, PUBLISHED_TEXT_LIMIT};
pub use report::{
    AttemptOutcome, CapabilityExecution, DeferredFinding, EvidenceRef, FiddleBuild, ProgressEntry,
    ReportBundle, RunDisposition, CONFIG_CHECK_SCHEMA, INSPECT_SCHEMA, REPORT_SCHEMA, RUN_SCHEMA,
    UNKNOWN_REVISION,
};
