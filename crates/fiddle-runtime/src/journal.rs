//! What an attempt was about to do, written down before it does it.
//!
//! The report bundle says what an attempt *did*, and it is published in one
//! atomic move once the attempt is over. That leaves a window: between a
//! capability changing the world and the bundle landing, nothing on disk records
//! that the world changed. In M0 the window is survivable by luck —
//! `stub_mark` writes a deterministic marker, so a later attempt observes
//! `Satisfied` and carries on. M2's effects are a branch and a pull request,
//! which are not idempotent: there, an attempt that mutated and was never
//! recorded is indistinguishable from one that never ran, and a retry has to
//! choose between duplicating the effect and skipping it. Neither is correct,
//! and no amount of reading the published bundles can tell the two apart,
//! because the whole problem is that no bundle exists.
//!
//! So the intent is recorded first. Before a capability is reached, this module
//! appends one line saying which capability is about to run under which attempt
//! id; after it returns, it appends a second saying how that ended. The two
//! records are what make an interrupted attempt *findable* — see
//! [`interrupted`].
//!
//! Three properties are worth the code:
//!
//! - **The journal is never the source of truth.** The published bundle is, and
//!   the journal is deleted the moment one lands ([`AttemptJournal::supersede`]).
//!   So for any attempt id at most one of the two exists, and they cannot be
//!   read as disagreeing: a journal record means "this attempt did not finish
//!   recording itself", full stop.
//! - **Only an attempt's own bundle clears its record.** A *later* attempt
//!   publishing successfully does not, even over the same work. Concluding that
//!   an earlier unrecorded effect is accounted for by a later attempt's bundle
//!   requires knowing the capability is idempotent — true of `stub_mark`, false
//!   of a branch and a pull request. So a record outlives every subsequent
//!   attempt until someone looks at it, which is the point: it is the standing
//!   signal that a change was made and never written down.
//! - **Recording the intent is fallible, and failing to record it stops the
//!   attempt.** A capability that ran without a durable record is precisely the
//!   hazard, so an unrecordable intent fails closed rather than proceeding
//!   unrecorded. That is why [`AttemptJournal::record_intent`] returns a
//!   `Result` and [`AttemptJournal::record_effect`] does not.
//! - **Records are appended, not rewritten.** An append cannot damage the line
//!   before it, so the intent record survives whatever happens to the effect
//!   record.

use crate::effect::{EffectTrace, ExecutionStep};
use crate::evidence::{EvidenceError, BUNDLE_FILE};
use crate::human::validate::{DecisionStep, DecisionTrace};
use fiddle_core::{AttemptId, CapabilityId, EffectKind, EvidenceRef};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The directory under `<report.dir>` holding attempt journals.
///
/// Dot-prefixed, and therefore unable to collide with a published invocation
/// directory: those are named by [`fiddle_core::InvocationRef::slug`], which
/// always begins with a scheme name.
pub const JOURNAL_DIR: &str = ".attempts";

/// What an attempt records about itself before, and after, changing the world.
///
/// A trait rather than a concrete type so the orchestration depends on the
/// recording seam instead of on the filesystem — which is what lets a test
/// assert the *ordering* (intent before execution) without a temporary
/// directory, and what keeps `run` free of a `<report.dir>` argument it would
/// otherwise have to thread through.
/// `Send + Sync` because [`AttemptTrace`] holds one behind an [`Arc`] shared with
/// an executor that may be driven from any task the runtime happens to be on.
pub trait AttemptJournal: Send + Sync {
    /// Record that this attempt is about to execute `capability`.
    ///
    /// Must not return `Ok` until the record is durable: the caller treats `Ok`
    /// as permission to change the world.
    fn record_intent(&self, capability: CapabilityId) -> Result<(), EvidenceError>;

    /// Record that the execution of an effect of `kind` reached `step`.
    ///
    /// # What this adds to an interrupted attempt's record
    ///
    /// The intent record says *which capability was about to run*. That is
    /// enough to make the attempt findable, and not enough to act on: a
    /// publication walks three effects, and "the capability may have run" leaves
    /// a recovery unable to say whether a branch, a pull request or a workflow
    /// dispatch may be out there. A record naming
    /// [`ExecutionStep::Apply`](crate::effect::ExecutionStep::Apply) and the
    /// kind it was entered for is the difference between *something may have
    /// happened* and *this may have happened*.
    ///
    /// It is written *before* the step it names is performed, which is what makes
    /// it worth writing at all: a record appended afterwards would be missing
    /// from exactly the attempts that were interrupted, which are the only
    /// attempts a journal exists for.
    ///
    /// # Why this is infallible where [`AttemptJournal::record_intent`] is not
    ///
    /// The fail-closed invariant is `record_intent`'s and stays entirely with
    /// it: a capability whose intent could not be recorded does not run, so a
    /// journal that cannot be written has already stopped the attempt before any
    /// step could be traced. This is therefore never the write that decides
    /// whether the world may change; it refines a record whose existence is
    /// already guaranteed. Returning a `Result` here would offer the executor a
    /// decision it has no better answer for than "carry on", in the middle of an
    /// authorization order whose whole value is that it is fixed.
    ///
    /// # What may be in it
    ///
    /// Two closed enumerations and nothing else — no target, no payload, no
    /// response, no postcondition. That is a bound on what it *can* say rather
    /// than a convention about what it does: neither
    /// [`EffectKind::as_str`](fiddle_core::EffectKind::as_str) nor
    /// [`ExecutionStep::as_str`](crate::effect::ExecutionStep::as_str) can
    /// render externally-authored text, so no credential and no unbounded string
    /// reaches this file through here. The postcondition — which *is* somebody
    /// else's text — goes to the published bundle through
    /// [`fiddle_core::Published`], where the receipts are.
    ///
    /// It is also the reason this is cheap enough to do per step: a publication
    /// walks seven steps three times, so twenty-one lines of about sixty bytes
    /// are appended beside the two records an attempt already writes.
    fn record_step(&self, kind: EffectKind, step: ExecutionStep);

    /// Record that the validation order reached `step`.
    ///
    /// The sibling of [`AttemptJournal::record_step`], and separate from it for the
    /// reason [`DecisionTrace`] is separate from
    /// [`EffectTrace`](crate::effect::EffectTrace): the authorization order repeats
    /// once per effect and carries an [`EffectKind`] to say which, while the
    /// validation order runs once for the single effect a question gates and has no
    /// second axis to name.
    ///
    /// # What a suspension leaves behind without it
    ///
    /// A continuation's whole subject is a walk that reads a conversation and may
    /// refuse at any of eight numbered places. The two records an attempt already
    /// writes say *which capability was about to run* and, afterwards, how that
    /// ended — and between them a walk that stopped at step 5 is
    /// indistinguishable from one that stopped at step 2, which are two entirely
    /// different things for an operator to go and look at. Design §6.5 asks for the
    /// order to be "observable rather than merely intended", and this is where that
    /// is paid for in a process nobody is watching.
    ///
    /// Infallible for [`AttemptJournal::record_step`]'s reason, unchanged: the
    /// fail-closed invariant belongs entirely to
    /// [`AttemptJournal::record_intent`], so a journal that cannot be written has
    /// already stopped the attempt before any step could be traced.
    ///
    /// # What may be in it
    ///
    /// One closed enumeration and nothing else, which is a narrower bound than
    /// `record_step`'s two rather than a weaker one.
    /// [`DecisionStep::as_str`](crate::human::validate::DecisionStep::as_str)
    /// renders eight fixed names, so no comment body, no marker and no credential
    /// can reach this file through here — the property design §6.7 states, held by
    /// construction rather than by review. That matters more on this order than on
    /// the other one: every input the validation order reads is text somebody
    /// outside this deployment wrote.
    fn record_decision_step(&self, step: DecisionStep);

    /// Record how the execution ended.
    ///
    /// Infallible by design. The intent record already makes the attempt
    /// detectable, and the bundle — if it publishes — is authoritative about
    /// what happened, so a failure here can neither hide an attempt nor
    /// contradict one. Returning an error would only offer the caller a decision
    /// it has no better answer for than "carry on and try to publish".
    fn record_effect(&self, capability: CapabilityId, status: &str, evidence: &[EvidenceRef]);

    /// A bundle landed; this attempt's journal has been superseded by it.
    fn supersede(&self);
}

/// The journal of one attempt, as a line-delimited JSON file under
/// `<report.dir>/.attempts/<slug>/<attempt-id>.jsonl`.
///
/// One file per attempt rather than one shared log per invocation, so two
/// concurrent attempts never interleave lines and superseding one cannot touch
/// the other's records.
pub struct FileJournal {
    path: PathBuf,
    attempt: AttemptId,
    invocation_ref: String,
}

impl FileJournal {
    /// The journal for `attempt` over `slug`, publishing under `report_dir`.
    pub fn new(
        report_dir: &Path,
        slug: &str,
        attempt: &AttemptId,
        invocation_ref: &str,
    ) -> FileJournal {
        FileJournal {
            path: journal_dir(report_dir, slug).join(format!("{}.jsonl", attempt.0)),
            attempt: attempt.clone(),
            invocation_ref: invocation_ref.to_string(),
        }
    }

    /// Append one record and return only once it is durable.
    ///
    /// `sync_all` on the file is what makes "durable" more than "handed to the
    /// page cache"; the directory sync that follows is what makes the file's
    /// *existence* durable rather than only its contents. The directory sync is
    /// best effort because not every platform admits opening a directory for
    /// it — where it is refused, the file contents are still synced, which is
    /// the larger half of the guarantee.
    fn append(&self, record: &serde_json::Value) -> Result<(), EvidenceError> {
        let directory = self
            .path
            .parent()
            .expect("a journal path always has a parent directory");
        self.io(directory, std::fs::create_dir_all(directory))?;

        let line = format!("{record}\n");
        self.io(
            &self.path,
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .and_then(|mut file| {
                    file.write_all(line.as_bytes())?;
                    file.sync_all()
                }),
        )?;
        let _ = std::fs::File::open(directory).and_then(|dir| dir.sync_all());
        Ok(())
    }

    /// Attach the path an operation was attempted on to its error.
    fn io(&self, path: &Path, result: std::io::Result<()>) -> Result<(), EvidenceError> {
        result.map_err(|source| EvidenceError::Journal {
            path: path.to_path_buf(),
            source,
        })
    }
}

impl AttemptJournal for FileJournal {
    fn record_intent(&self, capability: CapabilityId) -> Result<(), EvidenceError> {
        self.append(&serde_json::json!({
            "record": "intent",
            "attempt_id": self.attempt,
            "invocation_ref": self.invocation_ref,
            "capability_id": capability,
        }))
    }

    /// One line per step, appended and synced like every other record.
    ///
    /// `"effect_step"` and not `"effect"`: [`read_records`] finds the effect
    /// record by exact string, so these are invisible to it and an
    /// [`InterruptedAttempt`]'s meaning is unchanged by their presence. A reader
    /// that wants the walk reads the file.
    fn record_step(&self, kind: EffectKind, step: ExecutionStep) {
        let _ = self.append(&serde_json::json!({
            "record": "effect_step",
            "attempt_id": self.attempt,
            "kind": kind.as_str(),
            "step": step.as_str(),
        }));
    }

    /// One line per step, `"decision_step"` rather than `"effect_step"`.
    ///
    /// A third record kind and not a widening of the second, because the two orders
    /// are two orders: a reader asking which effect got how far and a reader asking
    /// where the validation order stopped are asking different questions, and a
    /// shared `"step"` key whose meaning depended on whether a `kind` was beside it
    /// would answer neither cleanly. [`read_records`] matches `"intent"` and
    /// `"effect"` by exact string, so this is invisible to it and an
    /// [`InterruptedAttempt`]'s meaning is unchanged by its presence — the same
    /// constraint `record_step` was added under.
    fn record_decision_step(&self, step: DecisionStep) {
        let _ = self.append(&serde_json::json!({
            "record": "decision_step",
            "attempt_id": self.attempt,
            "step": step.as_str(),
        }));
    }

    fn record_effect(&self, capability: CapabilityId, status: &str, evidence: &[EvidenceRef]) {
        let _ = self.append(&serde_json::json!({
            "record": "effect",
            "attempt_id": self.attempt,
            "invocation_ref": self.invocation_ref,
            "capability_id": capability,
            "status": status,
            "evidence": evidence,
        }));
    }

    fn supersede(&self) {
        // Best effort, and the only correct kind: the bundle has landed, so the
        // attempt is fully recorded whatever happens to this file. A removal
        // that failed and was reported would turn a successful attempt into a
        // failed one over a file nobody needs any more.
        let _ = std::fs::remove_file(&self.path);
        // The per-invocation directory is left in place. It is where an operator
        // looks for attempts in flight, and creating and removing it around
        // every attempt would be churn for no reader's benefit.
    }
}

/// The executor's step trace, sunk into the journal of the attempt it runs
/// inside.
///
/// # Why the sink is attached late
///
/// The binding order is forced by the two things this joins, and neither of them
/// can move. An [`Executor`](crate::effect::Executor) is built *before* a run
/// starts, because the capability that borrows it is what the run is about — and
/// it must be built by the caller that owns the credential-carrying clients, on
/// that caller's stack. A journal is named by an attempt id, which is minted
/// inside [`attempt`](crate::attempt) precisely so there is one minting site. So
/// the journal does not exist when the executor is built, and it does not outlive
/// the executor either.
///
/// What crosses that gap is this: created by the caller beside the executor,
/// handed to it as its sink, and filled in by `attempt` with the journal it just
/// built. An [`Arc`] rather than a borrow because the journal is the shorter-lived
/// of the two, which is exactly the direction a reference could not go.
///
/// # What an unattached trace does
///
/// Discards, silently, and only for as long as no attempt owns it. That is not a
/// default sink by another name: the window is the few statements between
/// construction and `attempt`, during which no executor has been asked to do
/// anything. A run that reached an effect with nothing attached would be an
/// executor running outside any attempt, which the orchestration has no path to.
pub struct AttemptTrace {
    journal: Mutex<Option<Arc<dyn AttemptJournal>>>,
}

impl AttemptTrace {
    /// A trace with no attempt behind it yet.
    pub fn new() -> Self {
        AttemptTrace {
            journal: Mutex::new(None),
        }
    }

    /// Attach the journal of the attempt this trace belongs to.
    pub fn attach(&self, journal: Arc<dyn AttemptJournal>) {
        *self.journal.lock().unwrap() = Some(journal);
    }
}

impl Default for AttemptTrace {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectTrace for AttemptTrace {
    /// The handle is cloned out from under the lock before the record is written,
    /// so an `fsync` is never held across it. Twenty-one appends per publication
    /// is not a hot path beside the two children each effect spawns, but holding a
    /// mutex across a disk sync would be a bad habit whatever the count.
    fn step(&self, kind: EffectKind, step: ExecutionStep) {
        let journal = self.journal.lock().unwrap().clone();
        if let Some(journal) = journal {
            journal.record_step(kind, step);
        }
    }
}

/// The validation order's steps, sunk into the same journal by the same value.
///
/// **One value implementing both traits is the arrangement
/// [`ProposeChange`](crate::ProposeChange) already describes as intended**, and
/// making it true of the production trace rather than only of the runtime's test
/// doubles is what puts the two orders of one attempt in one file, in the order
/// they happened. A capability walks the authorization order once per effect and
/// the validation order once, interleaved, and a reader reconstructing what a
/// suspended attempt was doing needs them interleaved.
///
/// Everything [`EffectTrace`]'s implementation above argues — the late attachment,
/// the handle cloned out from under the lock so no `fsync` is held across it, the
/// silent discard while no attempt owns it — applies here unchanged, because it is
/// the same cell and the same journal behind it.
impl DecisionTrace for AttemptTrace {
    fn step(&self, step: DecisionStep) {
        let journal = self.journal.lock().unwrap().clone();
        if let Some(journal) = journal {
            journal.record_decision_step(step);
        }
    }
}

/// An attempt that recorded an intent to change the world and never published a
/// bundle.
///
/// This is what "detectable" means concretely, and the three fields are the
/// three things a recovery has to know: which attempt, what it was going to do,
/// and whether it got as far as saying how that went.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterruptedAttempt {
    pub attempt_id: AttemptId,
    /// The capability the attempt intended to execute.
    pub capability: String,
    /// The status the journal recorded for the execution, or `None` when the
    /// attempt was interrupted before its capability returned.
    ///
    /// `None` is the dangerous case and is deliberately distinguishable: the
    /// world may or may not have moved, and only an inspection of the effect
    /// itself can say which. `Some("completed")` is the recoverable case — the
    /// world moved, it was recorded, and only the bundle is missing.
    pub effect: Option<String>,
}

/// Every attempt over `slug` that the journal still holds a record for and that
/// published no bundle, oldest first.
///
/// Both halves are checked rather than trusting the removal in
/// [`AttemptJournal::supersede`]: a process killed between the rename and the
/// removal would leave a record beside a perfectly good bundle, and reporting
/// that as interrupted would send a recovery after work that is already
/// recorded.
///
/// Attempt ids sort in the order they were minted, so the returned list is in
/// the order the attempts happened.
pub fn interrupted(report_dir: &Path, slug: &str) -> Vec<InterruptedAttempt> {
    let Ok(entries) = std::fs::read_dir(journal_dir(report_dir, slug)) else {
        return Vec::new();
    };

    let mut found: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let id = AttemptId(path.file_stem()?.to_str()?.to_string());
            if report_dir
                .join(slug)
                .join(&id.0)
                .join(BUNDLE_FILE)
                .try_exists()
                .unwrap_or(false)
            {
                return None;
            }
            read_records(&path, id)
        })
        .collect();
    found.sort_by(|a, b| a.attempt_id.cmp(&b.attempt_id));
    found
}

/// Fold one journal file into the attempt it describes, or `None` when it holds
/// no intent record.
///
/// A file without an intent record describes nothing actionable — the intent is
/// the first line written, so its absence means the file was created and never
/// completed — and reporting it would be reporting a partial write as an
/// interrupted attempt.
fn read_records(path: &Path, id: AttemptId) -> Option<InterruptedAttempt> {
    let text = std::fs::read_to_string(path).ok()?;
    let records: Vec<serde_json::Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    let capability = records.iter().find(|record| record["record"] == "intent")?["capability_id"]
        .as_str()?
        .to_string();
    let effect = records
        .iter()
        .rev()
        .find(|record| record["record"] == "effect")
        .and_then(|record| record["status"].as_str())
        .map(str::to_string);

    Some(InterruptedAttempt {
        attempt_id: id,
        capability,
        effect,
    })
}

/// Where the journals for `slug` live.
fn journal_dir(report_dir: &Path, slug: &str) -> PathBuf {
    report_dir.join(JOURNAL_DIR).join(slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::STUB_MARK;

    const SLUG: &str = "beans-fiddle-m0-demo";
    const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

    fn journal(root: &Path, attempt: &AttemptId) -> FileJournal {
        FileJournal::new(root, SLUG, attempt, INVOCATION_REF)
    }

    #[test]
    fn an_intent_without_an_effect_reports_an_unknown_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());

        journal(dir.path(), &attempt)
            .record_intent(STUB_MARK)
            .unwrap();

        assert_eq!(
            interrupted(dir.path(), SLUG),
            vec![InterruptedAttempt {
                attempt_id: attempt,
                capability: STUB_MARK.0.to_string(),
                effect: None,
            }]
        );
    }

    #[test]
    fn an_effect_record_is_reported_alongside_its_intent() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let recording = journal(dir.path(), &attempt);

        recording.record_intent(STUB_MARK).unwrap();
        recording.record_effect(
            STUB_MARK,
            "completed",
            &[EvidenceRef("stub:changes/x.json".to_string())],
        );

        let found = interrupted(dir.path(), SLUG);
        assert_eq!(found.len(), 1, "got {found:?}");
        assert_eq!(found[0].effect.as_deref(), Some("completed"));
        // The evidence is in the record on disk even though `InterruptedAttempt`
        // does not project it: what a recovery reads is the file.
        let written = std::fs::read_to_string(
            dir.path()
                .join(JOURNAL_DIR)
                .join(SLUG)
                .join("01ATTEMPT.jsonl"),
        )
        .unwrap();
        assert!(written.contains("stub:changes/x.json"), "got {written}");
        assert_eq!(
            written.lines().count(),
            2,
            "records are appended: {written}"
        );
    }

    /// The journal is superseded by the bundle, not merely tidied up: once one
    /// exists the attempt is fully recorded and must stop being reported.
    #[test]
    fn a_superseded_journal_reports_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let recording = journal(dir.path(), &attempt);
        recording.record_intent(STUB_MARK).unwrap();

        recording.supersede();

        assert!(interrupted(dir.path(), SLUG).is_empty());
    }

    /// And a record that outlived its own removal must not be reported either:
    /// the bundle is the authority, so its presence settles the question.
    #[test]
    fn a_record_beside_a_published_bundle_is_not_interrupted() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        journal(dir.path(), &attempt)
            .record_intent(STUB_MARK)
            .unwrap();

        let published = dir.path().join(SLUG).join(&attempt.0);
        std::fs::create_dir_all(&published).unwrap();
        std::fs::write(published.join(BUNDLE_FILE), b"{}").unwrap();

        assert!(interrupted(dir.path(), SLUG).is_empty());
    }

    /// An unrecordable intent is an error naming the journal, so the reason the
    /// run reports tells an operator which of the three writable places is at
    /// fault.
    #[cfg(unix)]
    #[test]
    fn an_unwritable_report_dir_makes_recording_an_intent_fail() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let reports = dir.path().join("reports");
        std::fs::create_dir_all(&reports).unwrap();
        std::fs::set_permissions(&reports, std::fs::Permissions::from_mode(0o500)).unwrap();

        let recorded =
            journal(&reports, &AttemptId("01ATTEMPT".to_string())).record_intent(STUB_MARK);

        std::fs::set_permissions(&reports, std::fs::Permissions::from_mode(0o755)).unwrap();
        if recorded.is_ok() {
            return; // an identity that ignores the permission bits
        }
        let error = recorded.unwrap_err();
        assert!(
            matches!(error, EvidenceError::Journal { .. }),
            "got {error:?}"
        );
        assert!(error.to_string().contains("attempt journal"), "got {error}");
    }

    /// A step record is appended beside the intent and changes nothing about how
    /// the intent reads — which is the constraint on adding it at all.
    #[test]
    fn a_step_record_is_appended_and_leaves_the_intent_reading_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let recording = journal(dir.path(), &attempt);

        recording.record_intent(STUB_MARK).unwrap();
        recording.record_step(EffectKind::EnsurePullRequest, ExecutionStep::Apply);

        let written = std::fs::read_to_string(
            dir.path()
                .join(JOURNAL_DIR)
                .join(SLUG)
                .join("01ATTEMPT.jsonl"),
        )
        .unwrap();
        assert_eq!(written.lines().count(), 2, "appended, not rewritten");
        assert!(
            written.contains(r#""step":"apply""#)
                && written.contains(r#""kind":"ensure_pull_request""#),
            "the step and the effect it belongs to must both be there: {written}"
        );
        // The reading a recovery makes is unchanged: `effect` is matched by exact
        // string, so `effect_step` is invisible to it and this attempt still has
        // no recorded outcome.
        assert_eq!(
            interrupted(dir.path(), SLUG),
            vec![InterruptedAttempt {
                attempt_id: attempt,
                capability: STUB_MARK.0.to_string(),
                effect: None,
            }]
        );
    }

    /// The validation order's steps land in the same file as the authorization
    /// order's, distinguishably, and neither changes what a recovery reads.
    ///
    /// Both orders in one assertion because their *coexistence* is the property:
    /// one attempt writes both, a reader has to be able to tell them apart, and the
    /// reading [`interrupted`] makes has to be unmoved by either. Three claims that
    /// only mean something together — a test of the new record alone would pass
    /// against a spelling that collided with `"effect_step"`.
    #[test]
    fn a_decision_step_is_recorded_beside_an_effect_step_and_neither_is_an_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let recording = journal(dir.path(), &attempt);

        recording.record_intent(STUB_MARK).unwrap();
        recording.record_step(EffectKind::EnsurePullRequestReady, ExecutionStep::Apply);
        recording.record_decision_step(DecisionStep::ReObserveState);

        let written = std::fs::read_to_string(
            dir.path()
                .join(JOURNAL_DIR)
                .join(SLUG)
                .join("01ATTEMPT.jsonl"),
        )
        .unwrap();
        assert_eq!(written.lines().count(), 3, "appended, not rewritten");
        assert!(
            written.contains(r#""record":"decision_step""#)
                && written.contains(r#""step":"re_observe_state""#),
            "the validation step and its record kind must both be there: {written}"
        );
        // Told apart from the authorization order's record rather than sharing its
        // kind: a reader asking which effect got how far and a reader asking where
        // the walk stopped are asking different questions.
        assert!(
            written.contains(r#""record":"effect_step""#),
            "the two orders are two records: {written}"
        );
        // And the walk carries no `kind`, because it has no second axis to name.
        // Asserted so a later widening that added one has to come back here.
        let decision: serde_json::Value = written
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .find(|record| record["record"] == "decision_step")
            .expect("the decision record is there");
        assert_eq!(decision["kind"], serde_json::Value::Null, "{decision}");

        // Neither record is an outcome. `interrupted` matches `"effect"` by exact
        // string, so this attempt still has none — which is the constraint adding
        // any third record kind to this file has to satisfy.
        assert_eq!(
            interrupted(dir.path(), SLUG),
            vec![InterruptedAttempt {
                attempt_id: attempt,
                capability: STUB_MARK.0.to_string(),
                effect: None,
            }]
        );
    }

    /// The bridge: nothing before an attempt owns it, the journal afterwards —
    /// **for both traits the one trace implements**.
    ///
    /// The two halves are one test because they share the cell: a `DecisionTrace`
    /// that reached for a journal of its own would pass a test of the effect half
    /// and write to the wrong file, and one that discarded after attachment would
    /// pass a test of the unattached window. Each half is asserted on both sides of
    /// the attachment.
    #[test]
    fn a_trace_records_nothing_until_an_attempt_attaches_its_journal() {
        let dir = tempfile::tempdir().unwrap();
        let attempt = AttemptId("01ATTEMPT".to_string());
        let path = dir
            .path()
            .join(JOURNAL_DIR)
            .join(SLUG)
            .join("01ATTEMPT.jsonl");

        // Both traits are named at the call site rather than left to method
        // resolution, which cannot choose between them: one type carries two `step`
        // methods, and spelling the trait is also what says which order a line is
        // asserting about.
        let trace = AttemptTrace::new();
        EffectTrace::step(
            &trace,
            EffectKind::EnsureBranchPublished,
            ExecutionStep::Apply,
        );
        DecisionTrace::step(&trace, DecisionStep::RecomputeIdentity);
        assert!(
            !path.exists(),
            "an unattached trace must not create a journal of its own, on either \
             order"
        );

        trace.attach(Arc::new(journal(dir.path(), &attempt)));
        EffectTrace::step(
            &trace,
            EffectKind::EnsureBranchPublished,
            ExecutionStep::Apply,
        );
        DecisionTrace::step(&trace, DecisionStep::RecomputeIdentity);

        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            written.lines().count(),
            2,
            "exactly the two steps recorded after attaching, one per order: {written}"
        );
        // And they went to the *same* file, which is the half a trace reaching for a
        // journal of its own would fail while still passing the count above.
        assert!(
            written.contains(r#""record":"effect_step""#)
                && written.contains(r#""record":"decision_step""#),
            "both orders share this attempt's journal: {written}"
        );
    }

    /// Journals for one invocation must not be reported under another's slug.
    #[test]
    fn journals_are_reported_per_invocation() {
        let dir = tempfile::tempdir().unwrap();
        journal(dir.path(), &AttemptId("01ATTEMPT".to_string()))
            .record_intent(STUB_MARK)
            .unwrap();

        assert!(interrupted(dir.path(), "jira-ICE-1").is_empty());
        assert_eq!(interrupted(dir.path(), SLUG).len(), 1);
    }

    /// Attempts are reported in the order they were minted, because that is the
    /// order a recovery has to work through them in.
    #[test]
    fn interrupted_attempts_are_reported_oldest_first() {
        let dir = tempfile::tempdir().unwrap();
        for id in ["01C", "01A", "01B"] {
            journal(dir.path(), &AttemptId(id.to_string()))
                .record_intent(STUB_MARK)
                .unwrap();
        }

        let ids: Vec<_> = interrupted(dir.path(), SLUG)
            .into_iter()
            .map(|found| found.attempt_id.0)
            .collect();
        assert_eq!(ids, ["01A", "01B", "01C"]);
    }
}
