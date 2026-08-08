//! The static M0 orchestration: observe, derive, maybe execute, observe again.
//!
//! Ordinary Rust rather than a workflow DSL. M0's plan is a single deterministic
//! step, and the value of writing it as plain control flow is that the two rules
//! it has to honour are visible in one screen:
//!
//! - The capability is reached only through [`NextAction::Execute`]. That is not
//!   a branch anyone has to remember — [`Capability::execute`] cannot be called
//!   without an [`ExecutionGrant`], and `ExecutionGrant::authorise` only issues
//!   one for an `Execute` derivation. A blocked or complete derivation therefore
//!   publishes an empty execution list because there was no way to fill it.
//! - After executing, the run observes *again* and derives *again*. What it
//!   reports as the next action is the state it left behind, not the intention
//!   it started with; design §4.7 shows `"next_action": "complete"` for a
//!   successful first run, and a completed run that advertised work still to do
//!   would send its caller round the loop for nothing.

use crate::capability::{Capability, ExecutionGrant};
use crate::ports::{ChangePort, WorkItemPort};
use fiddle_core::{
    correlation_key, derive_next, CapabilityExecution, EvidenceRef, NextAction, ProgressEntry,
    RunOutcome, WorkStateView,
};

/// Observe both sides of the world for one work item.
///
/// Nothing here can fail: a port that cannot read its source returns an
/// `Unavailable` observation rather than an error, so an unobservable world is
/// *reported* rather than aborting the caller. Shared by `run` and by the
/// read-only `inspect`, so both commands see the world through the same call.
pub fn observe(
    work_items: &dyn WorkItemPort,
    changes: &dyn ChangePort,
    work_id: &str,
) -> WorkStateView {
    WorkStateView {
        work_item: work_items.observe(work_id),
        changes: changes.observe(work_id),
    }
}

/// Everything one run acts on: who it is for, what it may touch, and what it
/// may do.
///
/// Ports and the capability are borrowed as trait objects, so the orchestration
/// depends on the seams rather than on the fixture-backed implementations M0
/// happens to ship.
pub struct RunContext<'a> {
    /// The project name the correlation key is derived from.
    pub project: &'a str,
    /// The canonical `<scheme>:<value>` text of the invocation.
    pub invocation_ref: &'a str,
    /// The work item both ports are asked about.
    pub work_id: &'a str,
    pub work_items: &'a dyn WorkItemPort,
    pub changes: &'a dyn ChangePort,
    pub capability: &'a dyn Capability,
}

impl RunContext<'_> {
    /// What this run's ports say about the world right now.
    pub fn observe(&self) -> WorkStateView {
        observe(self.work_items, self.changes, self.work_id)
    }

    /// The marker a satisfied change set must carry for this invocation.
    fn expected_marker(&self) -> String {
        correlation_key(self.project, self.invocation_ref)
    }
}

/// What a run did, in the form the CLI renders and a later task publishes.
///
/// `observations` is the view the report is *about*: the post-execution one
/// when the capability ran, and the entry view otherwise — always the view the
/// reported `next_action` was derived from, so the two can never describe
/// different moments.
pub struct RunReport {
    pub outcome: RunOutcome,
    pub next_action: NextAction,
    pub executions: Vec<CapabilityExecution>,
    pub progress: Vec<ProgressEntry>,
    pub observations: WorkStateView,
}

impl RunReport {
    /// A run that concluded without executing anything.
    ///
    /// Both non-executing derivations funnel through here, which is what makes
    /// "a blocked derivation executes nothing" true by construction rather than
    /// by two independently correct branches.
    fn without_execution(
        outcome: RunOutcome,
        next_action: NextAction,
        observations: WorkStateView,
    ) -> Self {
        RunReport {
            outcome,
            next_action,
            executions: Vec::new(),
            progress: Vec::new(),
            observations,
        }
    }
}

/// Execute the M0 plan for one invocation.
///
/// Total: every path returns a report. A capability failure becomes
/// [`RunOutcome::Retryable`] rather than an `Err`, because "try this again"
/// is a conclusion about the run, not an error the caller has to classify.
pub fn run(ctx: &RunContext<'_>) -> RunReport {
    let marker = ctx.expected_marker();
    let view = ctx.observe();
    let derived = derive_next(&view, &marker);

    // The grant is the gate. `Complete` and `Blocked` produce none, so the
    // executing arm below is the only code that can reach the capability at
    // all — there is no ordering mistake available here that would let a
    // blocked derivation slip through.
    let Some(grant) = ExecutionGrant::authorise(&derived) else {
        return match derived {
            NextAction::Complete => {
                RunReport::without_execution(RunOutcome::Completed, NextAction::Complete, view)
            }
            NextAction::Blocked { reason } => RunReport::without_execution(
                RunOutcome::Failed {
                    error: reason.clone(),
                },
                NextAction::Blocked { reason },
                view,
            ),
            // Unreachable: `authorise` returns `Some` for exactly this variant.
            NextAction::Execute { .. } => unreachable!("an Execute derivation always grants"),
        };
    };

    let capability_id = grant.capability_id();
    match ctx
        .capability
        .execute(grant, ctx.work_id, ctx.invocation_ref)
    {
        Ok(evidence) => {
            // Re-observe and re-derive: the report must describe the state the
            // run left behind, not the action it chose on entry.
            let after = ctx.observe();
            let next_action = derive_next(&after, &marker);
            debug_assert_eq!(
                next_action,
                NextAction::Complete,
                "a successful stub_mark must leave the work satisfied"
            );
            RunReport {
                outcome: RunOutcome::Completed,
                next_action,
                executions: vec![execution(
                    capability_id,
                    "completed",
                    vec![evidence.clone()],
                )],
                progress: vec![progress(
                    capability_id,
                    "completed",
                    format!("wrote correlation marker {marker}"),
                    vec![evidence],
                )],
                observations: after,
            }
        }
        Err(error) => {
            let reason = error.to_string();
            RunReport {
                outcome: RunOutcome::Retryable {
                    reason: reason.clone(),
                },
                next_action: derived,
                executions: vec![execution(capability_id, "failed", Vec::new())],
                progress: vec![progress(capability_id, "failed", reason, Vec::new())],
                observations: view,
            }
        }
    }
}

/// The one stage M0's capability has. Named once so the execution record and
/// the progress entry cannot disagree about what ran.
const STAGE: &str = "mark";

fn execution(
    capability_id: fiddle_core::CapabilityId,
    status: &str,
    evidence: Vec<EvidenceRef>,
) -> CapabilityExecution {
    CapabilityExecution {
        capability_id,
        status: status.to_string(),
        evidence,
    }
}

fn progress(
    capability_id: fiddle_core::CapabilityId,
    status: &str,
    summary: String,
    evidence: Vec<EvidenceRef>,
) -> ProgressEntry {
    ProgressEntry {
        capability_id,
        stage: STAGE.to_string(),
        status: status.to_string(),
        summary,
        evidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{CapabilityError, StubMark};
    use crate::stub::{StubChangePort, StubWorkItemPort};
    use fiddle_core::{CapabilityId, STUB_MARK};
    use std::sync::atomic::{AtomicUsize, Ordering};

    const WORK_ID: &str = "fiddle-m0-demo";
    const INVOCATION_REF: &str = "beans:fiddle-m0-demo";
    const PROJECT: &str = "icecube";

    /// A capability that records whether it was reached, so "never executed"
    /// can be asserted directly rather than inferred from its side effects.
    #[derive(Default)]
    struct Spy {
        calls: AtomicUsize,
    }

    impl Spy {
        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    impl Capability for Spy {
        fn id(&self) -> CapabilityId {
            STUB_MARK
        }

        fn execute(
            &self,
            _grant: ExecutionGrant,
            _work_id: &str,
            _invocation_ref: &str,
        ) -> Result<EvidenceRef, CapabilityError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(EvidenceRef("spy:executed".to_string()))
        }
    }

    fn context<'a>(
        capability: &'a dyn Capability,
        work_items: &'a StubWorkItemPort,
        changes: &'a StubChangePort,
    ) -> RunContext<'a> {
        RunContext {
            project: PROJECT,
            invocation_ref: INVOCATION_REF,
            work_id: WORK_ID,
            work_items,
            changes,
            capability,
        }
    }

    fn fixture_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("work")).unwrap();
        std::fs::create_dir_all(dir.path().join("changes")).unwrap();
        std::fs::write(
            dir.path().join(format!("work/{WORK_ID}.json")),
            format!(r#"{{"id":"{WORK_ID}","status":"open"}}"#),
        )
        .unwrap();
        dir
    }

    /// Unstarted work executes once and then reports the state it left, not the
    /// state it found.
    #[test]
    fn a_first_run_executes_and_then_reports_complete() {
        let dir = fixture_root();
        let capability = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());

        let report = run(&context(&capability, &work_items, &changes));

        assert_eq!(report.outcome, RunOutcome::Completed);
        assert_eq!(
            report.next_action,
            NextAction::Complete,
            "the report must describe the state the run left behind"
        );
        assert_eq!(report.executions.len(), 1);
        assert_eq!(report.progress.len(), 1);
        assert_eq!(
            report.observations.changes.value().unwrap().marker,
            Some(correlation_key(PROJECT, INVOCATION_REF)),
            "the reported observations must be the post-execution ones"
        );
    }

    /// The stability property, at the orchestration level: a second run over
    /// the world the first one left finds it satisfied and does nothing.
    #[test]
    fn a_second_run_completes_without_executing_again() {
        let dir = fixture_root();
        let spy = Spy::default();
        let marking = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());

        run(&context(&marking, &work_items, &changes));
        let report = run(&context(&spy, &work_items, &changes));

        assert_eq!(spy.calls(), 0, "a satisfied world must not execute");
        assert_eq!(report.outcome, RunOutcome::Completed);
        assert_eq!(report.next_action, NextAction::Complete);
        assert!(report.executions.is_empty());
        assert!(report.progress.is_empty());
    }

    /// The fail-closed arm: an unobservable world never reaches the capability,
    /// and says so with an empty execution list rather than a discarded one.
    #[test]
    fn a_blocked_derivation_never_reaches_the_capability() {
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("no-such-root");
        let spy = Spy::default();
        let work_items = StubWorkItemPort::new(&absent);
        let changes = StubChangePort::new(&absent);

        let report = run(&context(&spy, &work_items, &changes));

        assert_eq!(spy.calls(), 0, "a blocked derivation must not execute");
        assert!(matches!(report.outcome, RunOutcome::Failed { .. }));
        assert!(matches!(report.next_action, NextAction::Blocked { .. }));
        assert!(report.executions.is_empty());
        assert!(report.progress.is_empty());
    }

    /// A capability that could not write is retryable, and the failure is
    /// recorded as an execution that happened and failed — not as one that
    /// never ran.
    ///
    /// The world has to stay *observable* for the derivation to reach `Execute`
    /// at all, so the failure is injected as a readable but unwritable change
    /// directory rather than a missing one. That is a Unix permission, hence
    /// the gate; and `root` ignores the permission, hence the early return.
    #[cfg(unix)]
    #[test]
    fn a_capability_failure_is_retryable_and_recorded() {
        use std::os::unix::fs::PermissionsExt;

        let dir = fixture_root();
        let changes_dir = dir.path().join("changes");
        let capability = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());

        // Readable and listable, but not writable: observation still succeeds.
        std::fs::set_permissions(&changes_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let report = run(&context(&capability, &work_items, &changes));
        std::fs::set_permissions(&changes_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        if report.outcome == RunOutcome::Completed {
            // Running with an identity that ignores the permission bits.
            return;
        }

        match &report.outcome {
            RunOutcome::Retryable { reason } => assert!(reason.contains("change set"), "{reason}"),
            other => panic!("a failed write must be retryable, got {other:?}"),
        }
        assert_eq!(report.executions.len(), 1);
        assert_eq!(report.executions[0].status, "failed");
        assert!(report.executions[0].evidence.is_empty());
        assert_eq!(report.progress[0].status, "failed");
    }
}
