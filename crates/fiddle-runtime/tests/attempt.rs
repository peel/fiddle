use fiddle_core::{
    CapabilityId, ChangeSetState, EvidenceRef, FiddleBuild, InvocationRef, Mode, NextAction,
    Observation, RunOutcome, STUB_MARK, UNKNOWN_REVISION,
};
use fiddle_runtime::{
    attempt, journal, AttemptContext, AttemptJournal, Capability, CapabilityError, ChangePort,
    ExecutionInput, StubChangePort, StubMark, StubWorkItemPort, BUNDLE_FILE,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

mod support;

const WORK_ID: &str = "fiddle-m0-demo";
const INVOCATION_REF: &str = "beans:fiddle-m0-demo";
const SLUG: &str = "beans-fiddle-m0-demo";
const PROJECT: &str = "icecube";

struct Project {
    dir: tempfile::TempDir,
}

impl Project {
    fn unstarted() -> Self {
        let project = Project {
            dir: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(project.stub_root().join("work")).unwrap();
        std::fs::create_dir_all(project.stub_root().join("changes")).unwrap();
        std::fs::write(
            project.stub_root().join(format!("work/{WORK_ID}.json")),
            format!(r#"{{"id":"{WORK_ID}","status":"open"}}"#),
        )
        .unwrap();
        project
    }

    fn stub_root(&self) -> PathBuf {
        self.dir.path().join("stub-state")
    }

    fn report_dir(&self) -> PathBuf {
        self.dir.path().join("reports")
    }

    fn hide_stub_root(&self) {
        std::fs::rename(self.stub_root(), self.dir.path().join("hidden")).unwrap();
    }

    fn marker(&self) -> Option<String> {
        let path = self.stub_root().join(format!("changes/{WORK_ID}.json"));
        let text = std::fs::read_to_string(path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        value["marker"].as_str().map(str::to_string)
    }

    fn bundles(&self) -> Vec<PathBuf> {
        files(&self.report_dir())
            .into_iter()
            .filter(|path| path.file_name().is_some_and(|name| name == BUNDLE_FILE))
            .collect()
    }

    fn in_flight(&self) -> Vec<journal::InterruptedAttempt> {
        journal::interrupted(&self.report_dir(), SLUG)
    }
}

fn files(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn reference() -> InvocationRef {
    INVOCATION_REF.parse().unwrap()
}

async fn run_attempt(
    project: &Project,
    capability: &dyn Capability,
    report_dir: &Path,
) -> fiddle_runtime::AttemptRecord {
    run_attempt_observing(
        project,
        capability,
        &StubChangePort::new(project.stub_root()),
        report_dir,
    )
    .await
}

async fn run_attempt_observing(
    project: &Project,
    capability: &dyn Capability,
    changes: &dyn ChangePort,
    report_dir: &Path,
) -> fiddle_runtime::AttemptRecord {
    let reference = reference();
    let work_items = StubWorkItemPort::new(project.stub_root());
    attempt(&AttemptContext {
        project: PROJECT,
        reference: &reference,
        mode: Mode::Unattended,
        build: FiddleBuild::new("0.1.0", UNKNOWN_REVISION),
        report_dir,
        work_items: &work_items,
        changes,
        capability,
        trace: None,
        cancel: &tokio_util::sync::CancellationToken::new(),
    })
    .await
}

#[derive(Default)]
struct Spy {
    calls: AtomicUsize,
}

#[async_trait::async_trait]
impl Capability for Spy {
    fn id(&self) -> CapabilityId {
        STUB_MARK
    }

    fn stage(&self) -> &'static str {
        "spied"
    }

    async fn execute(&self, _input: ExecutionInput<'_>) -> Result<EvidenceRef, CapabilityError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(EvidenceRef("spy:executed".to_string()))
    }
}

struct MutateThenDie(StubMark);

#[async_trait::async_trait]
impl Capability for MutateThenDie {
    fn id(&self) -> CapabilityId {
        STUB_MARK
    }

    fn stage(&self) -> &'static str {
        self.0.stage()
    }

    async fn execute(&self, input: ExecutionInput<'_>) -> Result<EvidenceRef, CapabilityError> {
        self.0.execute(input).await.unwrap();
        panic!("the process died after the effect landed");
    }
}

const FOREIGN_MARKER: &str = "0123456789abcdef";

struct ForeignWriterBetweenObservations {
    inner: StubChangePort,
    change_set: PathBuf,
    observations: AtomicUsize,
}

impl ForeignWriterBetweenObservations {
    fn over(project: &Project) -> Self {
        ForeignWriterBetweenObservations {
            inner: StubChangePort::new(project.stub_root()),
            change_set: project.stub_root().join(format!("changes/{WORK_ID}.json")),
            observations: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl ChangePort for ForeignWriterBetweenObservations {
    async fn observe(
        &self,
        work_id: &str,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Observation<ChangeSetState> {
        if self.observations.fetch_add(1, Ordering::Relaxed) == 1 {
            std::fs::write(
                &self.change_set,
                format!(r#"{{"marker":"{FOREIGN_MARKER}"}}"#),
            )
            .unwrap();
        }
        self.inner.observe(work_id, cancel).await
    }
}

#[cfg(unix)]
fn seal(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o500)).unwrap();
}

#[cfg(unix)]
fn unseal(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[tokio::test]
async fn a_published_attempt_leaves_the_bundle_and_no_journal_record() {
    let project = Project::unstarted();

    let record = run_attempt(
        &project,
        &StubMark::new(project.stub_root(), PROJECT),
        &project.report_dir(),
    )
    .await;

    assert_eq!(record.bundle.outcome, RunOutcome::Completed);
    assert!(record.evidence_failure.is_none());
    let published = record
        .published
        .expect("a completed attempt must publish its bundle");
    assert!(
        project
            .report_dir()
            .join(&published)
            .try_exists()
            .unwrap_or(false),
        "the path the record names must be the path the bundle landed at: {published:?}"
    );
    assert_eq!(project.bundles().len(), 1);
    assert!(project.marker().is_some(), "the capability must have run");
    assert!(
        project.in_flight().is_empty(),
        "a published bundle supersedes the journal, got {:?}",
        project.in_flight()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn an_executed_capability_is_recorded_even_when_publication_fails() {
    let project = Project::unstarted();
    std::fs::create_dir_all(project.report_dir().join(journal::JOURNAL_DIR)).unwrap();
    seal(&project.report_dir());

    let record = run_attempt(
        &project,
        &StubMark::new(project.stub_root(), PROJECT),
        &project.report_dir(),
    )
    .await;

    unseal(&project.report_dir());
    assert!(
        record.published.is_none(),
        "the sealed directory did not stop the publication, so this test asserted \
         nothing. Measured 2026-08-13: uid 501 in the dev shell and GitHub's \
         ubuntu-latest runner in CI, neither of them root — so if this fires, the \
         identity changed. The durable fix is an obstacle that is a property of the \
         path rather than a permission the caller can be exempt from: see \
         `orchestration::tests::a_capability_failure_is_retryable_and_recorded`, which \
         puts a directory where the temporary file must go and fails `EISDIR` for every \
         identity."
    );

    assert!(
        project.bundles().is_empty(),
        "publication failed, so no bundle may exist: {:?}",
        project.bundles()
    );
    assert!(
        project.marker().is_some(),
        "the capability was expected to succeed; this case is about what happens next"
    );
    assert_eq!(record.bundle.capability_executions.len(), 1);
    assert_eq!(record.bundle.capability_executions[0].status, "completed");

    let in_flight = project.in_flight();
    assert_eq!(
        in_flight.len(),
        1,
        "an unpublished attempt must be findable, got {in_flight:?}"
    );
    assert_eq!(in_flight[0].attempt_id, record.bundle.attempt_id);
    assert_eq!(in_flight[0].capability, STUB_MARK.0);
    assert_eq!(
        in_flight[0].effect.as_deref(),
        Some("completed"),
        "the journal must record that the capability executed, not only that it was going to"
    );
}

#[tokio::test]
async fn an_attempt_interrupted_between_the_effect_and_publication_is_detectable() {
    let project = Arc::new(Project::unstarted());
    let capability = MutateThenDie(StubMark::new(project.stub_root(), PROJECT));

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let died = tokio::spawn({
        let project = Arc::clone(&project);
        async move {
            let report_dir = project.report_dir();
            run_attempt(&project, &capability, &report_dir).await
        }
    })
    .await;
    std::panic::set_hook(previous);

    assert!(
        died.is_err_and(|joined| joined.is_panic()),
        "the capability was supposed to die"
    );
    assert!(
        project.marker().is_some(),
        "the world must have moved, or there is nothing to detect"
    );
    assert!(
        project.bundles().is_empty(),
        "the attempt died before publication, so no bundle may exist"
    );

    let in_flight = project.in_flight();
    assert_eq!(
        in_flight.len(),
        1,
        "an interrupted attempt must be detectable, got {in_flight:?}"
    );
    assert_eq!(in_flight[0].capability, STUB_MARK.0);
    assert_eq!(
        in_flight[0].effect, None,
        "the process died inside the capability, so how it ended is unknown — \
         which is what a reader must be told rather than guess"
    );
}

#[tokio::test]
async fn an_attempt_that_never_reached_its_capability_leaves_nothing_in_flight() {
    let project = Project::unstarted();
    project.hide_stub_root();
    let spy = Spy::default();

    let record = run_attempt(&project, &spy, &project.report_dir()).await;

    assert_eq!(spy.calls.load(Ordering::Relaxed), 0);
    assert!(matches!(record.bundle.outcome, RunOutcome::Failed { .. }));
    assert!(
        project.in_flight().is_empty(),
        "nothing intended to change the world, so nothing may be reported in flight: {:?}",
        project.in_flight()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn an_unrecordable_intent_stops_the_attempt_before_the_capability_runs() {
    let project = Project::unstarted();
    std::fs::create_dir_all(project.report_dir()).unwrap();
    seal(&project.report_dir());
    let spy = Spy::default();

    let record = run_attempt(&project, &spy, &project.report_dir()).await;

    unseal(&project.report_dir());
    assert!(
        record.published.is_none(),
        "the sealed directory did not stop the publication, so this test asserted \
         nothing. Measured 2026-08-13: uid 501 in the dev shell and GitHub's \
         ubuntu-latest runner in CI, neither of them root — so if this fires, the \
         identity changed. The durable fix is an obstacle that is a property of the \
         path rather than a permission the caller can be exempt from: see \
         `orchestration::tests::a_capability_failure_is_retryable_and_recorded`, which \
         puts a directory where the temporary file must go and fails `EISDIR` for every \
         identity."
    );

    assert_eq!(
        spy.calls.load(Ordering::Relaxed),
        0,
        "the capability must not run when its intent could not be recorded"
    );
    assert!(
        project.marker().is_none(),
        "and the world must not have moved"
    );
    match &record.bundle.outcome {
        RunOutcome::Retryable { reason } => assert!(
            reason.as_str().contains("attempt journal"),
            "the reason must name the journal, so an operator knows what to fix: {reason}"
        ),
        other => panic!("an unrecordable intent is retryable, got {other:?}"),
    }
    assert!(
        record.bundle.capability_executions.is_empty(),
        "nothing ran, so nothing may be reported as having run"
    );
    assert!(record.evidence_failure.is_some());
}

#[cfg(unix)]
#[tokio::test]
async fn a_publication_failure_is_retryable_and_repeating_it_afterwards_succeeds() {
    let project = Project::unstarted();
    std::fs::create_dir_all(project.report_dir().join(journal::JOURNAL_DIR)).unwrap();
    seal(&project.report_dir());

    let failed = run_attempt(
        &project,
        &StubMark::new(project.stub_root(), PROJECT),
        &project.report_dir(),
    )
    .await;

    unseal(&project.report_dir());
    assert!(
        failed.published.is_none(),
        "the sealed directory did not stop the publication, so this test asserted \
         nothing. Measured 2026-08-13: uid 501 in the dev shell and GitHub's \
         ubuntu-latest runner in CI, neither of them root — so if this fires, the \
         identity changed. The durable fix is an obstacle that is a property of the \
         path rather than a permission the caller can be exempt from: see \
         `orchestration::tests::a_capability_failure_is_retryable_and_recorded`, which \
         puts a directory where the temporary file must go and fails `EISDIR` for every \
         identity."
    );
    match &failed.bundle.outcome {
        RunOutcome::Retryable { reason } => assert!(
            reason.as_str().contains("report bundle"),
            "the reason must name the bundle, keeping it distinct from the other retryable \
             causes — the change set and the attempt journal: {reason}"
        ),
        other => panic!("a publication failure that repeating fixes is retryable, got {other:?}"),
    }

    let repeated = run_attempt(
        &project,
        &StubMark::new(project.stub_root(), PROJECT),
        &project.report_dir(),
    )
    .await;

    assert_eq!(
        repeated.bundle.outcome,
        RunOutcome::Completed,
        "repeating after the fix must succeed, or `Retryable` was the wrong word"
    );
    assert!(repeated.published.is_some());

    let in_flight = project.in_flight();
    assert_eq!(in_flight.len(), 1, "got {in_flight:?}");
    assert_ne!(
        in_flight[0].attempt_id, repeated.bundle.attempt_id,
        "the record left standing is the interrupted attempt's, not the successful one's"
    );
}

#[tokio::test]
async fn a_second_attempt_over_satisfied_work_publishes_without_journaling() {
    let project = Project::unstarted();
    let capability = StubMark::new(project.stub_root(), PROJECT);

    run_attempt(&project, &capability, &project.report_dir()).await;
    let spy = Spy::default();
    let second = run_attempt(&project, &spy, &project.report_dir()).await;

    assert_eq!(spy.calls.load(Ordering::Relaxed), 0);
    assert_eq!(second.bundle.outcome, RunOutcome::Completed);
    assert!(second.bundle.capability_executions.is_empty());
    assert!(second.published.is_some());
    assert_eq!(project.bundles().len(), 2, "each attempt publishes its own");
    assert!(project.in_flight().is_empty());
}

#[tokio::test]
async fn a_run_whose_world_is_taken_over_after_executing_reports_the_derivation_it_left() {
    let project = Project::unstarted();
    let changes = ForeignWriterBetweenObservations::over(&project);

    let record = run_attempt_observing(
        &project,
        &StubMark::new(project.stub_root(), PROJECT),
        &changes,
        &project.report_dir(),
    )
    .await;

    assert_eq!(
        project.marker().as_deref(),
        Some(FOREIGN_MARKER),
        "the other writer must have taken the change set over, or there is no disagreement to report"
    );

    let NextAction::Blocked { reason } = &record.bundle.next_action else {
        panic!(
            "the post-execution derivation must be blocked, got {:?}",
            record.bundle.next_action
        );
    };
    assert!(
        reason.contains(FOREIGN_MARKER),
        "the derivation must name the marker it found: {reason}"
    );

    match &record.bundle.outcome {
        RunOutcome::Failed { error } => assert!(
            error.as_str().contains(FOREIGN_MARKER) && error.as_str().contains("executed"),
            "the error must name both the foreign marker and the fact that the capability had \
             already run, so this exit 20 is distinguishable from an unobservable source: {error}"
        ),
        other => {
            panic!("a post-execution blocked derivation must not report completed, got {other:?}")
        }
    }

    let published = record
        .published
        .as_ref()
        .expect("the attempt still concluded, so it still publishes what it concluded");
    let bundle: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(project.report_dir().join(published)).unwrap(),
    )
    .unwrap();
    assert!(
        bundle["outcome"]["failed"].is_object(),
        "got {}",
        bundle["outcome"]
    );
    assert!(
        bundle["next_action"]["blocked"].is_object(),
        "got {}",
        bundle["next_action"]
    );

    assert_eq!(record.bundle.capability_executions.len(), 1);
    assert_eq!(record.bundle.capability_executions[0].status, "completed");
    assert!(
        project.in_flight().is_empty(),
        "the bundle published, so it supersedes the journal: {:?}",
        project.in_flight()
    );
}

#[tokio::test]
async fn a_run_whose_effect_left_no_trace_reports_that_there_is_still_work_to_do() {
    let project = Project::unstarted();
    let spy = Spy::default();

    let record = run_attempt(&project, &spy, &project.report_dir()).await;

    assert_eq!(
        spy.calls.load(Ordering::Relaxed),
        1,
        "the capability must have run, or this is not the case under test"
    );
    assert!(
        project.marker().is_none(),
        "and it must have left nothing behind, or the world would be satisfied"
    );
    assert!(
        matches!(record.bundle.next_action, NextAction::Execute { .. }),
        "got {:?}",
        record.bundle.next_action
    );
    match &record.bundle.outcome {
        RunOutcome::Retryable { reason } => assert!(
            reason.as_str().contains("executed") && reason.as_str().contains("not started"),
            "the reason must say the capability already ran and the work is still \
             undone, keeping this exit 11 distinct from the change set, the attempt \
             journal and the report bundle: {reason}"
        ),
        other => panic!("an effect that left no trace is not a completed run, got {other:?}"),
    }
}

#[tokio::test]
async fn the_executors_steps_reach_the_attempt_journal() {
    let dir = tempfile::tempdir().unwrap();
    let attempt_id = fiddle_core::AttemptId("01STEPS".to_string());
    let journal = std::sync::Arc::new(journal::FileJournal::new(
        dir.path(),
        SLUG,
        &attempt_id,
        INVOCATION_REF,
    ));
    journal
        .record_intent(fiddle_core::PUBLISH_CHANGE)
        .expect("the journal directory is writable");

    let trace = fiddle_runtime::AttemptTrace::new();
    trace.attach(journal.clone() as std::sync::Arc<dyn fiddle_runtime::AttemptJournal>);

    let harness = support::Harness::new(support::Script::AbsentThenWritten);
    let receipt = harness
        .executor_observed_by(&trace)
        .execute(support::branch_effect(), harness.operation())
        .await
        .expect("the scripted world commits");
    assert_eq!(
        receipt.outcome,
        fiddle_runtime::EffectOutcome::Committed,
        "the effect must really have been walked end to end"
    );

    let recorded = read_journal(dir.path(), &attempt_id);
    let steps: Vec<&str> = recorded
        .iter()
        .filter(|record| record["record"] == "effect_step")
        .filter_map(|record| record["step"].as_str())
        .collect();
    assert_eq!(
        steps,
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
            "authorize",
            "apply",
            "observe_postcondition",
        ],
        "the journal must hold the whole authorization order, in order"
    );
    assert!(
        recorded
            .iter()
            .filter(|record| record["record"] == "effect_step")
            .all(|record| record["kind"] == "ensure_branch_published"),
        "every step must name the effect it belongs to: {recorded:?}"
    );
    assert_eq!(
        journal::interrupted(dir.path(), SLUG),
        vec![journal::InterruptedAttempt {
            attempt_id: attempt_id.clone(),
            capability: fiddle_core::PUBLISH_CHANGE.0.to_string(),
            effect: None,
        }],
        "the step records must be invisible to the interrupted-attempt reading"
    );
}

#[tokio::test]
async fn an_unattached_trace_records_nothing_and_refuses_nothing() {
    let trace = fiddle_runtime::AttemptTrace::new();
    let harness = support::Harness::new(support::Script::AbsentThenWritten);
    let receipt = harness
        .executor_observed_by(&trace)
        .execute(support::branch_effect(), harness.operation())
        .await
        .expect("an unobserved walk is still a walk");
    assert_eq!(receipt.outcome, fiddle_runtime::EffectOutcome::Committed);
    assert_eq!(
        harness.world.mutations(),
        1,
        "the effect must still have been applied, exactly once"
    );
    assert!(
        harness.world.steps().is_empty(),
        "and this world was not the sink, so it must have seen no steps at all"
    );
}

fn read_journal(report_dir: &Path, attempt: &fiddle_core::AttemptId) -> Vec<serde_json::Value> {
    let path = report_dir
        .join(journal::JOURNAL_DIR)
        .join(SLUG)
        .join(format!("{}.jsonl", attempt.0));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read {} ({e})", path.display()))
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
