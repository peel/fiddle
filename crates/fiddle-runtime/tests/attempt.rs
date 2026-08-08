//! The run-and-publish transaction, exercised at the runtime layer.
//!
//! These are integration tests rather than unit tests inside the crate because
//! the property they are about is the *whole* attempt: observe, record the
//! intent, execute, re-observe, publish. A test able to reach a private helper
//! could assert those halves separately, and the halves passing separately is
//! exactly the failure this file exists to prevent — that was the state of the
//! code before [`fiddle_runtime::attempt`] existed, when the CLI held the second
//! half and no test in this crate covered the two together.
//!
//! Everything here is asserted against the filesystem the attempt wrote to, the
//! way an operator or a later attempt would find it, and never through the CLI:
//! the guarantee belongs to the runtime, so the runtime is where it is proven.

use fiddle_core::{
    CapabilityId, EvidenceRef, FiddleBuild, InvocationRef, Mode, RunOutcome, STUB_MARK,
    UNKNOWN_REVISION,
};
use fiddle_runtime::{
    attempt, journal, AttemptContext, Capability, CapabilityError, ExecutionGrant, StubChangePort,
    StubMark, StubWorkItemPort, BUNDLE_FILE,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

const WORK_ID: &str = "fiddle-m0-demo";
const INVOCATION_REF: &str = "beans:fiddle-m0-demo";
const SLUG: &str = "beans-fiddle-m0-demo";
const PROJECT: &str = "icecube";

/// A disposable project: a fixture root holding one open work item, and a
/// report directory that does not exist yet.
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

    /// Take the fixture root away, so every source the ports name becomes
    /// unobservable and the derivation is forced to block.
    fn hide_stub_root(&self) {
        std::fs::rename(self.stub_root(), self.dir.path().join("hidden")).unwrap();
    }

    /// The marker the capability writes, read back the way the change port
    /// reads it, or `None` when nothing wrote one.
    fn marker(&self) -> Option<String> {
        let path = self.stub_root().join(format!("changes/{WORK_ID}.json"));
        let text = std::fs::read_to_string(path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        value["marker"].as_str().map(str::to_string)
    }

    /// Every `report.json` under `<report.dir>`, however deep.
    fn bundles(&self) -> Vec<PathBuf> {
        files(&self.report_dir())
            .into_iter()
            .filter(|path| path.file_name().is_some_and(|name| name == BUNDLE_FILE))
            .collect()
    }

    /// The attempts this project's journal still reports as in flight.
    fn in_flight(&self) -> Vec<journal::InterruptedAttempt> {
        journal::interrupted(&self.report_dir(), SLUG)
    }
}

/// Every file under `root`, recursively, sorted. A missing `root` yields an
/// empty list, so "nothing was created at all" is expressible.
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

/// The reference every scenario runs against, parsed once.
fn reference() -> InvocationRef {
    INVOCATION_REF.parse().unwrap()
}

/// One attempt over `capability`, publishing into `report_dir`.
///
/// A free function taking every borrow explicitly rather than a builder, so each
/// scenario's context is visible where the scenario is written — these tests are
/// about what the attempt does with its ports, its capability, and its report
/// directory, and hiding any of the three would hide the setup that matters.
fn run_attempt(
    project: &Project,
    capability: &dyn Capability,
    report_dir: &Path,
) -> fiddle_runtime::AttemptRecord {
    let reference = reference();
    let work_items = StubWorkItemPort::new(project.stub_root());
    let changes = StubChangePort::new(project.stub_root());
    attempt(&AttemptContext {
        project: PROJECT,
        reference: &reference,
        mode: Mode::Unattended,
        build: FiddleBuild::new("0.1.0", UNKNOWN_REVISION),
        report_dir,
        work_items: &work_items,
        changes: &changes,
        capability,
    })
}

/// A capability that counts its calls, so "the capability never ran" is
/// asserted directly rather than inferred from the absence of its side effect.
#[derive(Default)]
struct Spy {
    calls: AtomicUsize,
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

/// A capability that really changes the world and then dies, which is the shape
/// of an interruption: the effect landed and the process never reached
/// publication.
struct MutateThenDie(StubMark);

impl Capability for MutateThenDie {
    fn id(&self) -> CapabilityId {
        STUB_MARK
    }

    fn execute(
        &self,
        grant: ExecutionGrant,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        self.0.execute(grant, work_id, invocation_ref).unwrap();
        panic!("the process died after the effect landed");
    }
}

/// Make `path` readable and listable but not writable.
#[cfg(unix)]
fn seal(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o500)).unwrap();
}

/// Undo [`seal`], so the test can read what the failed attempt left behind and
/// the temporary directory can be removed on drop.
#[cfg(unix)]
fn unseal(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The happy path, stated first so the failure cases are read against it: the
/// bundle lands, and the journal — having been superseded by it — leaves no
/// record of an attempt in flight.
#[test]
fn a_published_attempt_leaves_the_bundle_and_no_journal_record() {
    let project = Project::unstarted();

    let record = run_attempt(
        &project,
        &StubMark::new(project.stub_root(), PROJECT),
        &project.report_dir(),
    );

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

/// **The property this bean exists for.** A capability that succeeded and a
/// publication that failed must not add up to a world that moved with nothing
/// recording that it moved.
///
/// `<report.dir>` is sealed so publication cannot create the attempt directory
/// under it, while the journal directory — created before the seal, as a
/// previous attempt would have left it — stays writable. That isolates the one
/// failure this is about: the effect landed, the bundle did not, and the
/// question is what a later reader can still find out.
#[cfg(unix)]
#[test]
fn an_executed_capability_is_recorded_even_when_publication_fails() {
    let project = Project::unstarted();
    // The journal's own directory, as an earlier attempt would have left it.
    std::fs::create_dir_all(project.report_dir().join(journal::JOURNAL_DIR)).unwrap();
    seal(&project.report_dir());

    let record = run_attempt(
        &project,
        &StubMark::new(project.stub_root(), PROJECT),
        &project.report_dir(),
    );

    unseal(&project.report_dir());
    if record.published.is_some() {
        return; // an identity that ignores the permission bits
    }

    assert!(
        project.bundles().is_empty(),
        "publication failed, so no bundle may exist: {:?}",
        project.bundles()
    );
    assert!(
        project.marker().is_some(),
        "the capability was expected to succeed; this case is about what happens next"
    );
    // The record the attempt hands back, and the record on disk, must agree
    // that the capability ran.
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

/// The interruption itself: the effect lands and the process dies before
/// publication, exactly as a crash or a `SIGKILL` between the two would.
///
/// Simulated with a panic rather than described in a comment, because the whole
/// claim is about a path no ordinary return takes. What has to be true
/// afterwards is that the attempt is *detectable* — a later reader can tell
/// "something ran and was never recorded" from "nothing ran", which is the
/// distinction M2's non-idempotent GitHub effects cannot recover without.
#[test]
fn an_attempt_interrupted_between_the_effect_and_publication_is_detectable() {
    let project = Project::unstarted();
    let capability = MutateThenDie(StubMark::new(project.stub_root(), PROJECT));

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_attempt(&project, &capability, &project.report_dir())
    }));
    std::panic::set_hook(previous);

    assert!(died.is_err(), "the capability was supposed to die");
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

/// The contrast that gives the previous test its meaning: an attempt that never
/// reached its capability leaves nothing in flight. Without this, "a journal
/// record exists" would be evidence of nothing in particular.
#[test]
fn an_attempt_that_never_reached_its_capability_leaves_nothing_in_flight() {
    let project = Project::unstarted();
    project.hide_stub_root();
    let spy = Spy::default();

    let record = run_attempt(&project, &spy, &project.report_dir());

    assert_eq!(spy.calls.load(Ordering::Relaxed), 0);
    assert!(matches!(record.bundle.outcome, RunOutcome::Failed { .. }));
    assert!(
        project.in_flight().is_empty(),
        "nothing intended to change the world, so nothing may be reported in flight: {:?}",
        project.in_flight()
    );
}

/// The fail-closed direction of the ordering decision: if the intent cannot be
/// recorded durably, the capability must not run at all. A capability that ran
/// without a durable record is the very hazard the journal exists to remove, so
/// an unrecordable intent has to stop the attempt rather than proceed
/// unrecorded.
#[cfg(unix)]
#[test]
fn an_unrecordable_intent_stops_the_attempt_before_the_capability_runs() {
    let project = Project::unstarted();
    std::fs::create_dir_all(project.report_dir()).unwrap();
    seal(&project.report_dir());
    let spy = Spy::default();

    let record = run_attempt(&project, &spy, &project.report_dir());

    unseal(&project.report_dir());
    if record.published.is_some() {
        return; // an identity that ignores the permission bits
    }

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
            reason.contains("attempt journal"),
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

/// A publication failure is reported as retryable, not failed: repeating the
/// same invocation after the operator fixes `<report.dir>` succeeds, which is
/// what `Retryable` promises and what `Failed` denies.
///
/// Asserted as a real repetition rather than as a claim about the word, because
/// the whole defect being settled here was a variant name that did not match
/// what repeating actually did.
#[cfg(unix)]
#[test]
fn a_publication_failure_is_retryable_and_repeating_it_afterwards_succeeds() {
    let project = Project::unstarted();
    std::fs::create_dir_all(project.report_dir().join(journal::JOURNAL_DIR)).unwrap();
    seal(&project.report_dir());

    let failed = run_attempt(
        &project,
        &StubMark::new(project.stub_root(), PROJECT),
        &project.report_dir(),
    );

    unseal(&project.report_dir());
    if failed.published.is_some() {
        return; // an identity that ignores the permission bits
    }
    match &failed.bundle.outcome {
        RunOutcome::Retryable { reason } => assert!(
            reason.contains("report bundle"),
            "the reason must name the bundle, keeping it distinct from the other retryable \
             causes — the change set and the attempt journal: {reason}"
        ),
        other => panic!("a publication failure that repeating fixes is retryable, got {other:?}"),
    }

    // The operator fixed the directory; the same invocation, repeated.
    let repeated = run_attempt(
        &project,
        &StubMark::new(project.stub_root(), PROJECT),
        &project.report_dir(),
    );

    assert_eq!(
        repeated.bundle.outcome,
        RunOutcome::Completed,
        "repeating after the fix must succeed, or `Retryable` was the wrong word"
    );
    assert!(repeated.published.is_some());

    // And the first attempt is *still* reported as interrupted. Only the
    // publication of that attempt's own bundle clears its record; a later
    // attempt succeeding does not, because concluding "the earlier effect is
    // accounted for" requires knowing the capability is idempotent — which is
    // true of `stub_mark` and false of M2's branch and pull request. Clearing it
    // here would be the exact assumption this whole design exists to stop
    // relying on.
    let in_flight = project.in_flight();
    assert_eq!(in_flight.len(), 1, "got {in_flight:?}");
    assert_ne!(
        in_flight[0].attempt_id, repeated.bundle.attempt_id,
        "the record left standing is the interrupted attempt's, not the successful one's"
    );
}

/// Nothing changes the world on a second attempt, so nothing is journaled: the
/// journal records an *intent to mutate*, and an attempt over satisfied work has
/// none. It still publishes its own bundle.
#[test]
fn a_second_attempt_over_satisfied_work_publishes_without_journaling() {
    let project = Project::unstarted();
    let capability = StubMark::new(project.stub_root(), PROJECT);

    run_attempt(&project, &capability, &project.report_dir());
    let spy = Spy::default();
    let second = run_attempt(&project, &spy, &project.report_dir());

    assert_eq!(spy.calls.load(Ordering::Relaxed), 0);
    assert_eq!(second.bundle.outcome, RunOutcome::Completed);
    assert!(second.bundle.capability_executions.is_empty());
    assert!(second.published.is_some());
    assert_eq!(project.bundles().len(), 2, "each attempt publishes its own");
    assert!(project.in_flight().is_empty());
}
