use crate::capability::{Capability, ExecutionGrant};
use crate::effect::Recurrence;
use crate::evidence::{mint_attempt_id, publish, EvidenceError};
use crate::journal::{AttemptJournal, AttemptTrace, FileJournal};
use crate::ports::{ChangePort, WorkItemPort};
use fiddle_core::{
    correlation_key, derive_next, CapabilityExecution, EvidenceRef, FiddleBuild, InvocationRef,
    Mode, NextAction, ProgressEntry, Published, ReportBundle, RunOutcome, WorkRef, WorkStateView,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn observe(
    work_items: &dyn WorkItemPort,
    changes: &dyn ChangePort,
    addressed: Addressed<'_>,
) -> WorkStateView {
    let work_item = match addressed {
        Addressed::WorkItem(work_id) => work_items.observe(work_id),
        Addressed::NoWorkItem { .. } => fiddle_core::Observation::NotApplicable {
            reason: "this invocation names no work item, so no tracker was consulted".to_string(),
        },
    };
    WorkStateView::without_publication(work_item, changes.observe(addressed.change_set()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Addressed<'a> {
    WorkItem(&'a str),

    NoWorkItem { change_set: &'a str },
}

impl<'a> Addressed<'a> {
    pub fn of(reference: &'a fiddle_core::InvocationRef) -> Self {
        match reference.value() {
            "" => Addressed::NoWorkItem {
                change_set: reference.scheme().as_str(),
            },
            value => Addressed::WorkItem(value),
        }
    }

    pub fn change_set(&self) -> &'a str {
        match self {
            Addressed::WorkItem(work_id) => work_id,
            Addressed::NoWorkItem { change_set } => change_set,
        }
    }
}

pub struct RunContext<'a> {
    pub project: &'a str,
    pub invocation_ref: &'a str,
    pub addressed: Addressed<'a>,
    pub attempt: &'a fiddle_core::AttemptId,
    pub work_items: &'a dyn WorkItemPort,
    pub changes: &'a dyn ChangePort,
    pub capability: &'a dyn Capability,
    pub journal: &'a dyn AttemptJournal,
}

impl RunContext<'_> {
    pub fn observe(&self) -> WorkStateView {
        observe(self.work_items, self.changes, self.addressed)
    }

    fn observe_with(&self, capability: &dyn Capability) -> WorkStateView {
        with_publication(self.observe(), capability)
    }

    fn expected_marker(&self) -> String {
        correlation_key(self.project, self.invocation_ref)
    }
}

pub struct RunReport {
    pub outcome: RunOutcome,
    pub next_action: NextAction,
    pub executions: Vec<CapabilityExecution>,
    pub progress: Vec<ProgressEntry>,
    pub observations: WorkStateView,
    pub evidence_failure: Option<EvidenceError>,
}

impl RunReport {
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
            evidence_failure: None,
        }
    }
}

struct Authorised {
    grant: ExecutionGrant,
}

impl Authorised {
    fn recorded(
        journal: &dyn AttemptJournal,
        grant: ExecutionGrant,
    ) -> Result<Self, EvidenceError> {
        journal.record_intent(grant.capability_id())?;
        Ok(Authorised { grant })
    }

    fn capability_id(&self) -> fiddle_core::CapabilityId {
        self.grant.capability_id()
    }
}

fn concluded(next_action: &NextAction, after: &WorkStateView) -> RunOutcome {
    match next_action {
        NextAction::Complete => RunOutcome::Completed,
        NextAction::Blocked { reason } => RunOutcome::Failed {
            error: Published::of(format!(
                "the capability executed, and the work is not accounted for afterwards: {reason}"
            )),
        },
        NextAction::Execute { .. } if !after.has_completion_state() => RunOutcome::Completed,
        NextAction::Execute { capability_id } => RunOutcome::Retryable {
            reason: Published::of(format!(
                "{} executed and reported success, and the work is still not started \
                 afterwards",
                capability_id.0
            )),
        },
    }
}

pub async fn run(ctx: &RunContext<'_>) -> RunReport {
    let marker = ctx.expected_marker();
    let view = ctx.observe();
    let derived = derive_next(&view, &marker, ctx.capability.id());

    let Some(grant) = ExecutionGrant::authorise(&derived, ctx.attempt) else {
        return match derived {
            NextAction::Complete => {
                RunReport::without_execution(RunOutcome::Completed, NextAction::Complete, view)
            }
            NextAction::Blocked { reason } => RunReport::without_execution(
                RunOutcome::Failed {
                    error: Published::of(&reason),
                },
                NextAction::Blocked { reason },
                view,
            ),
            NextAction::Execute { .. } => unreachable!("an Execute derivation always grants"),
        };
    };

    let authorised = match Authorised::recorded(ctx.journal, grant) {
        Ok(authorised) => authorised,
        Err(error) => {
            let reason = Published::of(error.to_string());
            return RunReport {
                evidence_failure: Some(error),
                ..RunReport::without_execution(RunOutcome::Retryable { reason }, derived, view)
            };
        }
    };

    let capability_id = authorised.capability_id();
    match ctx
        .capability
        .execute(
            authorised.grant,
            ctx.addressed.change_set(),
            ctx.invocation_ref,
        )
        .await
    {
        Ok(evidence) => {
            ctx.journal
                .record_effect(capability_id, "completed", std::slice::from_ref(&evidence));
            let observed = ctx.capability.receipts();
            let after = ctx.observe_with(ctx.capability);
            let next_action = derive_next(&after, &marker, ctx.capability.id());
            RunReport {
                outcome: concluded(&next_action, &after),
                next_action,
                executions: vec![execution(
                    capability_id,
                    "completed",
                    with_receipts(evidence.clone(), &observed),
                )],
                progress: vec![progress(
                    capability_id,
                    ctx.capability.stage(),
                    "completed",
                    Published::of(format!("wrote correlation marker {marker}")),
                    with_receipts(evidence, &observed),
                )],
                observations: after,
                evidence_failure: None,
            }
        }
        Err(error) => {
            let reason = Published::of(error.to_string());
            let (outcome, status) = match error.recurrence() {
                Recurrence::Correctable => (
                    RunOutcome::Retryable {
                        reason: reason.clone(),
                    },
                    "failed",
                ),
                Recurrence::Permanent => (
                    RunOutcome::Failed {
                        error: reason.clone(),
                    },
                    "failed",
                ),
                Recurrence::Awaiting => (
                    RunOutcome::Suspended {
                        reason: reason.clone(),
                    },
                    "awaiting",
                ),
            };
            ctx.journal.record_effect(capability_id, status, &[]);
            let observed = ctx.capability.receipts();
            RunReport {
                outcome,
                next_action: derived,
                executions: vec![execution(capability_id, status, observed.clone())],
                progress: vec![progress(
                    capability_id,
                    ctx.capability.stage(),
                    status,
                    reason,
                    observed,
                )],
                observations: with_publication(view, ctx.capability),
                evidence_failure: None,
            }
        }
    }
}

pub struct AttemptContext<'a> {
    pub project: &'a str,
    pub reference: &'a InvocationRef,
    pub mode: Mode,
    pub build: FiddleBuild,
    pub report_dir: &'a Path,
    pub work_items: &'a dyn WorkItemPort,
    pub changes: &'a dyn ChangePort,
    pub capability: &'a dyn Capability,
    pub trace: Option<&'a AttemptTrace>,
}

pub struct AttemptRecord {
    pub bundle: ReportBundle,
    pub published: Option<PathBuf>,
    pub evidence_failure: Option<EvidenceError>,
}

pub async fn attempt(ctx: &AttemptContext<'_>) -> AttemptRecord {
    let attempt_id = mint_attempt_id();
    let invocation = ctx.reference.as_str();
    let slug = ctx.reference.slug();
    let journal: Arc<dyn AttemptJournal> = Arc::new(FileJournal::new(
        ctx.report_dir,
        &slug,
        &attempt_id,
        &invocation,
    ));
    if let Some(trace) = ctx.trace {
        trace.attach(Arc::clone(&journal));
    }

    let RunReport {
        outcome,
        next_action,
        executions,
        progress,
        observations,
        evidence_failure,
    } = run(&RunContext {
        project: ctx.project,
        invocation_ref: &invocation,
        addressed: Addressed::of(ctx.reference),
        attempt: &attempt_id,
        work_items: ctx.work_items,
        changes: ctx.changes,
        capability: ctx.capability,
        journal: journal.as_ref(),
    })
    .await;

    let bundle = ReportBundle {
        schema: fiddle_core::REPORT_SCHEMA,
        fiddle: ctx.build.clone(),
        invocation_ref: invocation.clone(),
        work_ref: Some(WorkRef(invocation)),
        attempt_id: attempt_id.clone(),
        mode: ctx.mode,
        outcome,
        next_action,
        capability_executions: executions,
        progress,
        observations,
        disposition: ctx.capability.disposition(),
    };

    match publish(ctx.report_dir, &slug, &attempt_id, &bundle) {
        Ok(path) => {
            journal.supersede();
            let relative = path.strip_prefix(ctx.report_dir).unwrap_or(&path);
            AttemptRecord {
                bundle,
                published: Some(relative.to_path_buf()),
                evidence_failure,
            }
        }
        Err(error) => {
            let (bundle, failure) = match evidence_failure {
                Some(journal_failure) => (bundle, journal_failure),
                None => (
                    ReportBundle {
                        outcome: RunOutcome::Retryable {
                            reason: Published::of(error.to_string()),
                        },
                        ..bundle
                    },
                    error,
                ),
            };
            AttemptRecord {
                bundle,
                published: None,
                evidence_failure: Some(failure),
            }
        }
    }
}

fn with_publication(view: WorkStateView, capability: &dyn Capability) -> WorkStateView {
    let observed = match capability.publication() {
        Some(publication) => {
            WorkStateView::with_publication(view.work_item, view.changes, publication)
        }
        None => view,
    };
    observed.at_revision(capability.tree_observation())
}

fn with_receipts(earned: EvidenceRef, observed: &[EvidenceRef]) -> Vec<EvidenceRef> {
    let mut evidence = vec![earned];
    evidence.extend_from_slice(observed);
    evidence
}

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
    stage: &str,
    status: &str,
    summary: Published,
    evidence: Vec<EvidenceRef>,
) -> ProgressEntry {
    ProgressEntry {
        capability_id,
        stage: stage.to_string(),
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
    const ATTEMPT: &str = "01JQZX0000000000000000000";

    #[derive(Default)]
    struct Log(std::sync::Mutex<Vec<String>>);

    impl Log {
        fn record(&self, event: impl Into<String>) {
            self.0.lock().unwrap().push(event.into());
        }

        fn events(&self) -> Vec<String> {
            self.0.lock().unwrap().clone()
        }
    }

    #[derive(Default)]
    struct Spy {
        calls: AtomicUsize,
        log: std::sync::Arc<Log>,
    }

    impl Spy {
        fn watching(log: &std::sync::Arc<Log>) -> Self {
            Spy {
                calls: AtomicUsize::new(0),
                log: std::sync::Arc::clone(log),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::Relaxed)
        }
    }

    #[async_trait::async_trait]
    impl Capability for Spy {
        fn id(&self) -> CapabilityId {
            STUB_MARK
        }

        fn stage(&self) -> &'static str {
            "spied"
        }

        async fn execute(
            &self,
            _grant: ExecutionGrant,
            _work_id: &str,
            _invocation_ref: &str,
        ) -> Result<EvidenceRef, CapabilityError> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.log.record("execute");
            Ok(EvidenceRef("spy:executed".to_string()))
        }
    }

    struct Watched {
        inner: StubMark,
        log: std::sync::Arc<Log>,
    }

    #[async_trait::async_trait]
    impl Capability for Watched {
        fn id(&self) -> CapabilityId {
            self.inner.id()
        }

        fn stage(&self) -> &'static str {
            self.inner.stage()
        }

        async fn execute(
            &self,
            grant: ExecutionGrant,
            work_id: &str,
            invocation_ref: &str,
        ) -> Result<EvidenceRef, CapabilityError> {
            self.log.record("execute");
            self.inner.execute(grant, work_id, invocation_ref).await
        }
    }

    #[derive(Default)]
    struct SpyJournal {
        log: std::sync::Arc<Log>,
        refuse: bool,
    }

    impl SpyJournal {
        fn watching(log: &std::sync::Arc<Log>) -> Self {
            SpyJournal {
                log: std::sync::Arc::clone(log),
                refuse: false,
            }
        }

        fn refusing(log: &std::sync::Arc<Log>) -> Self {
            SpyJournal {
                log: std::sync::Arc::clone(log),
                refuse: true,
            }
        }
    }

    impl AttemptJournal for SpyJournal {
        fn record_intent(&self, _capability: CapabilityId) -> Result<(), EvidenceError> {
            self.log.record("intent");
            if self.refuse {
                return Err(EvidenceError::Journal {
                    path: PathBuf::from("/nowhere/.attempts"),
                    source: std::io::Error::other("refused"),
                });
            }
            Ok(())
        }

        fn record_step(&self, kind: fiddle_core::EffectKind, step: crate::effect::ExecutionStep) {
            self.log
                .record(format!("step:{}:{}", kind.as_str(), step.as_str()));
        }

        fn record_decision_step(&self, step: crate::human::validate::DecisionStep) {
            self.log.record(format!("decision:{}", step.as_str()));
        }

        fn record_effect(&self, _capability: CapabilityId, status: &str, _e: &[EvidenceRef]) {
            self.log.record(format!("effect:{status}"));
        }

        fn supersede(&self) {
            self.log.record("supersede");
        }
    }

    fn context<'a>(
        capability: &'a dyn Capability,
        work_items: &'a StubWorkItemPort,
        changes: &'a StubChangePort,
        journal: &'a dyn AttemptJournal,
        attempt: &'a fiddle_core::AttemptId,
    ) -> RunContext<'a> {
        RunContext {
            project: PROJECT,
            invocation_ref: INVOCATION_REF,
            addressed: Addressed::WorkItem(WORK_ID),
            attempt,
            work_items,
            changes,
            capability,
            journal,
        }
    }

    fn attempt_id() -> fiddle_core::AttemptId {
        fiddle_core::AttemptId(ATTEMPT.to_string())
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

    #[tokio::test]
    async fn a_first_run_executes_and_then_reports_complete() {
        let dir = fixture_root();
        let capability = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::default();

        let report = run(&context(
            &capability,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

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

    #[tokio::test]
    async fn a_run_that_publishes_nothing_reports_no_review_and_no_verification() {
        let dir = fixture_root();
        let capability = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());

        let report = run(&context(
            &capability,
            &work_items,
            &changes,
            &SpyJournal::default(),
            &attempt_id(),
        ))
        .await;

        let json = serde_json::to_value(&report.observations).unwrap();
        for key in ["review", "verification"] {
            assert!(
                json[key]["available"].is_null(),
                "a run that reached no forge must publish no {key} value: {}",
                json[key]
            );
            assert!(
                json[key]["not_applicable"]["reason"]
                    .as_str()
                    .is_some_and(|reason| !reason.is_empty()),
                "{key} must say why the question does not apply: {}",
                json[key]
            );
        }
        assert_eq!(json["work_item"]["available"]["value"]["status"], "open");
        assert_eq!(
            json["changes"]["available"]["value"]["marker"],
            correlation_key(PROJECT, INVOCATION_REF),
            "the two new observations must not have displaced the post-execution ones"
        );
    }

    #[tokio::test]
    async fn a_second_run_completes_without_executing_again() {
        let dir = fixture_root();
        let log = std::sync::Arc::<Log>::default();
        let spy = Spy::watching(&log);
        let marking = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::watching(&log);

        run(&context(
            &marking,
            &work_items,
            &changes,
            &SpyJournal::default(),
            &attempt_id(),
        ))
        .await;
        let report = run(&context(
            &spy,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        assert_eq!(spy.calls(), 0, "a satisfied world must not execute");
        assert_eq!(report.outcome, RunOutcome::Completed);
        assert_eq!(report.next_action, NextAction::Complete);
        assert!(report.executions.is_empty());
        assert!(report.progress.is_empty());
        assert!(
            log.events().is_empty(),
            "nothing was going to change the world, so nothing may be journaled: {:?}",
            log.events()
        );
    }

    #[tokio::test]
    async fn a_blocked_derivation_never_reaches_the_capability() {
        let dir = tempfile::tempdir().unwrap();
        let absent = dir.path().join("no-such-root");
        let log = std::sync::Arc::<Log>::default();
        let spy = Spy::watching(&log);
        let work_items = StubWorkItemPort::new(&absent);
        let changes = StubChangePort::new(&absent);
        let journal = SpyJournal::watching(&log);

        let report = run(&context(
            &spy,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        assert_eq!(spy.calls(), 0, "a blocked derivation must not execute");
        assert!(matches!(report.outcome, RunOutcome::Failed { .. }));
        assert!(matches!(report.next_action, NextAction::Blocked { .. }));
        assert!(report.executions.is_empty());
        assert!(report.progress.is_empty());
        assert!(
            log.events().is_empty(),
            "a blocked derivation intends nothing, so it journals nothing: {:?}",
            log.events()
        );
    }

    #[tokio::test]
    async fn the_intent_is_recorded_before_the_capability_is_reached() {
        let dir = fixture_root();
        let log = std::sync::Arc::<Log>::default();
        let capability = Watched {
            inner: StubMark::new(dir.path(), PROJECT),
            log: std::sync::Arc::clone(&log),
        };
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::watching(&log);

        run(&context(
            &capability,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        assert_eq!(
            log.events(),
            ["intent", "execute", "effect:completed"],
            "the world must never move before the intention to move it is recorded"
        );
    }

    #[tokio::test]
    async fn an_unrecordable_intent_stops_the_run_before_the_capability() {
        let dir = fixture_root();
        let log = std::sync::Arc::<Log>::default();
        let spy = Spy::watching(&log);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::refusing(&log);

        let report = run(&context(
            &spy,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        assert_eq!(spy.calls(), 0);
        assert_eq!(log.events(), ["intent"], "nothing may follow a refusal");
        match &report.outcome {
            RunOutcome::Retryable { reason } => assert!(
                reason.as_str().contains("attempt journal"),
                "the reason must name the journal: {reason}"
            ),
            other => panic!("an unrecordable intent is retryable, got {other:?}"),
        }
        assert!(
            report.executions.is_empty() && report.progress.is_empty(),
            "nothing ran, so nothing may be reported as having run"
        );
        assert!(matches!(
            report.evidence_failure,
            Some(EvidenceError::Journal { .. })
        ));
    }

    #[tokio::test]
    async fn a_capability_failure_is_retryable_and_recorded() {
        let dir = fixture_root();
        let log = std::sync::Arc::<Log>::default();
        let changes_dir = dir.path().join("changes");
        let capability = StubMark::new(dir.path(), PROJECT);
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::watching(&log);

        std::fs::create_dir_all(changes_dir.join(format!(".{WORK_ID}.json.tmp"))).unwrap();
        let report = run(&context(
            &capability,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        match &report.outcome {
            RunOutcome::Retryable { reason } => {
                assert!(reason.as_str().contains("change set"), "{reason}")
            }
            other => panic!("a failed write must be retryable, got {other:?}"),
        }
        assert_eq!(report.executions.len(), 1);
        assert_eq!(report.executions[0].status, "failed");
        assert!(report.executions[0].evidence.is_empty());
        assert_eq!(report.progress[0].status, "failed");
        assert_eq!(log.events(), ["intent", "effect:failed"]);
    }

    struct Refusing {
        how: Refusal,
        log: std::sync::Arc<Log>,
    }

    #[derive(Clone, Copy)]
    enum Refusal {
        PolicyDenied,
        Unresolved,
        AwaitingDecision,
    }

    fn conversation() -> crate::human::InteractionRef {
        crate::human::InteractionRef::GitHubPullRequestComment {
            repo: "peel/fiddle-effects-acceptance".to_string(),
            pr: 4,
            comment: 991,
        }
    }

    #[async_trait::async_trait]
    impl Capability for Refusing {
        fn id(&self) -> CapabilityId {
            STUB_MARK
        }

        fn stage(&self) -> &'static str {
            "refused"
        }

        async fn execute(
            &self,
            _grant: ExecutionGrant,
            _work_id: &str,
            _invocation_ref: &str,
        ) -> Result<EvidenceRef, CapabilityError> {
            self.log.record("execute");
            let kind = fiddle_core::EffectKind::EnsurePullRequest;
            if let Refusal::AwaitingDecision = self.how {
                return Err(CapabilityError::AwaitingDecision {
                    request: fiddle_core::DecisionRequestId("0123456789abcdef".to_string()),
                    interaction: conversation(),
                    question: "may this change be marked ready for review?".to_string(),
                });
            }
            Err(CapabilityError::Effect(match self.how {
                Refusal::PolicyDenied => crate::effect::EffectError::PolicyDenied {
                    kind,
                    reason: "the deployment document denies this kind".to_string(),
                },
                Refusal::Unresolved => crate::effect::EffectError::Unresolved {
                    kind,
                    reason: "gh was killed before it answered".to_string(),
                },
                Refusal::AwaitingDecision => unreachable!(),
            }))
        }
    }

    #[tokio::test]
    async fn a_refused_effect_fails_and_an_unsettled_one_stays_retryable() {
        for (how, expect_permanent) in [(Refusal::PolicyDenied, true), (Refusal::Unresolved, false)]
        {
            let dir = fixture_root();
            let log = std::sync::Arc::<Log>::default();
            let capability = Refusing {
                how,
                log: std::sync::Arc::clone(&log),
            };
            let work_items = StubWorkItemPort::new(dir.path());
            let changes = StubChangePort::new(dir.path());
            let journal = SpyJournal::watching(&log);

            let report = run(&context(
                &capability,
                &work_items,
                &changes,
                &journal,
                &attempt_id(),
            ))
            .await;

            match (&report.outcome, expect_permanent) {
                (RunOutcome::Failed { error }, true) => assert!(
                    error.as_str().contains("policy denied"),
                    "the row must be earned by the refusal it names: {error}"
                ),
                (RunOutcome::Retryable { reason }, false) => assert!(
                    reason.as_str().contains("unresolved outcome"),
                    "the row must be earned by the ambiguity it names: {reason}"
                ),
                (other, _) => panic!("wrong row for this failure: {other:?}"),
            }

            assert_eq!(report.executions.len(), 1);
            assert_eq!(report.executions[0].status, "failed");
            assert_eq!(report.progress[0].status, "failed");
            assert_eq!(log.events(), ["intent", "execute", "effect:failed"]);
        }
    }

    #[tokio::test]
    async fn a_capability_awaiting_a_decision_suspends_rather_than_failing_or_retrying() {
        let dir = fixture_root();
        let log = std::sync::Arc::<Log>::default();
        let capability = Refusing {
            how: Refusal::AwaitingDecision,
            log: std::sync::Arc::clone(&log),
        };
        let work_items = StubWorkItemPort::new(dir.path());
        let changes = StubChangePort::new(dir.path());
        let journal = SpyJournal::watching(&log);

        let report = run(&context(
            &capability,
            &work_items,
            &changes,
            &journal,
            &attempt_id(),
        ))
        .await;

        let reason = match &report.outcome {
            RunOutcome::Suspended { reason } => reason.clone(),
            other => panic!("a published question is a wait, not a {other:?}"),
        };
        assert!(
            !matches!(report.outcome, RunOutcome::Retryable { .. }),
            "repeating asks the same question again"
        );
        assert!(
            !matches!(report.outcome, RunOutcome::Failed { .. }),
            "an answer would finish this run"
        );

        let named = conversation().to_string();
        assert!(
            reason.as_str().contains(&named),
            "the outcome must say where to look: {reason}"
        );
        assert_eq!(report.progress.len(), 1);
        assert_eq!(
            report.progress[0].stage, "refused",
            "the entry is filed under the capability's own stage"
        );
        assert!(
            report.progress[0].summary.as_str().contains(&named),
            "the bundle must say where to look: {}",
            report.progress[0].summary
        );

        assert_eq!(report.executions[0].status, "awaiting");
        assert_eq!(report.progress[0].status, "awaiting");
        assert_eq!(log.events(), ["intent", "execute", "effect:awaiting"]);
    }
}
