//! Repair a broken fixture through one bounded agent attempt, and believe the
//! check rather than the model.
//!
//! Everything in [`crate::agent`] is about giving a model a small, bounded
//! surface. This module is about what happens to what it says afterwards, and
//! the whole of it is one rule: **the outcome is decided by a check this
//! capability runs itself, over the tree the attempt actually left behind.**
//! [`RepairReport::claimed_complete`](crate::agent::RepairReport::claimed_complete)
//! travels only as far as [`CapabilityError::CheckFailed`], where it is recorded
//! as evidence beside the exit code that overruled it. No branch anywhere reads
//! it.
//!
//! That rule is what makes the correlation marker mean something. A marker says
//! "this invocation accounts for this work", the next invocation's assessment
//! reads it and completes without executing again, and so writing one is the
//! strongest claim fiddle makes. It is therefore written *after* the check
//! exits 0 and on no other path — a repair that did not pass its check has not
//! earned it, however confident the model was.
//!
//! # Two questions this module is where to settle
//!
//! **A transcript of nothing but malformed tool calls.** A model can spend an
//! entire attempt sending arguments that do not decode: no tool body runs,
//! [`AuditHook`](crate::agent::AuditHook) records every call as `malformed`, and
//! the run still reaches a well-formed final report. It would be easy to add a
//! rule failing such an attempt outright. Deliberately not done, because the
//! rule would be worse than the gap it closes. The check already refuses the
//! only harmful case — an attempt that changed nothing over a fixture that is
//! still broken fails its check and earns nothing, which
//! `a_transcript_of_nothing_but_malformed_calls_is_judged_by_the_check` proves.
//! What the extra rule *would* add is a case where the check passes and the
//! capability fails anyway, on the grounds of how the model behaved on its way
//! there — which is the model's conduct deciding the outcome, the exact
//! inversion this milestone exists to prevent. The transcript stays visible in
//! the receipts, where an operator can act on it; it does not get a vote.
//!
//! **An attempt that never produced a report** is a different case and does
//! short-circuit: [`attempt`] returning `Err` means no repair was completed, so
//! there is nothing to verify and the check is not run. That is not the model's
//! claim deciding anything — a cancelled or bounded attempt has no claim to
//! decide with.
//!
//! # What survives
//!
//! Nothing in the worktree. The workspace is per-attempt and removed however the
//! execution ends, so what an accepted repair leaves behind is the marker and
//! the evidence reference naming how many files git saw change. Getting the
//! repair itself out of the workspace is M2's branch and pull request; M1 proves
//! the verdict, not the delivery.

use super::{Capability, CapabilityError, ExecutionGrant};
use crate::agent::{attempt, AgentBudget, Direction, ToolHost, ToolReceipts};
use crate::workspace::{Workspace, WorkspaceCommand};
use fiddle_core::{correlation_key, CapabilityId, ChangeSetState, EvidenceRef};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// The origin this capability's evidence is named under.
const REPAIR_ORIGIN: &str = "repair";

/// The tool names this crate registers, and the only ones an evidence
/// reference may repeat.
///
/// A receipt's `tool` field is *not* always ours. `AuditHook` records a call to
/// a tool that does not exist under the name the **model** chose, which is
/// model-authored and unbounded — and evidence is published. Every name outside
/// this set is therefore collapsed to [`FOREIGN_TOOL`] rather than quoted, the
/// same discipline `AgentError` already keeps when it refuses to name the tool
/// in `the model called a tool that does not exist`. The count survives; the
/// string the model made up does not.
const REGISTERED_TOOLS: [&str; 4] = ["read_file", "write_file", "list_files", "run_check"];

/// What a call to anything outside [`REGISTERED_TOOLS`] is counted as.
const FOREIGN_TOOL: &str = "unregistered";

/// One attempt's tool receipts, as evidence references a bundle can carry.
///
/// # Why a summary rather than the receipts themselves
///
/// [`EvidenceRef`] is a string, and the bundle's evidence is a list of them.
/// The receipts are a `Vec` of records with durations, which would have to be
/// either serialised into one enormous reference or given a new home in the
/// report schema — and the schema is a published contract that this is not the
/// task to widen. A summary answers the questions a bundle is actually asked:
/// *were any tools called at all*, *which ones*, and *how did each go*.
///
/// The leading `tools:<n>` is emitted **even when `n` is zero**, and that is the
/// whole reason this function exists. An attempt in which the model called
/// nothing is the exact shape of the defect that made every model on the
/// gateway fail, and it is invisible from outside a process unless something
/// says so out loud. `tools:0` says so.
///
/// Counts are sorted so two runs that did the same things produce byte-identical
/// evidence, which is what makes a diff between two bundles readable.
fn tool_evidence(receipts: &ToolReceipts) -> Vec<EvidenceRef> {
    let mut counts: std::collections::BTreeMap<(&str, &str), usize> =
        std::collections::BTreeMap::new();
    for call in &receipts.calls {
        let tool = REGISTERED_TOOLS
            .iter()
            .find(|known| **known == call.tool)
            .copied()
            .unwrap_or(FOREIGN_TOOL);
        *counts.entry((tool, call.outcome)).or_default() += 1;
    }

    let mut evidence = vec![EvidenceRef(format!("tools:{}", receipts.calls.len()))];
    evidence.extend(
        counts
            .into_iter()
            .map(|((tool, outcome), count)| EvidenceRef(format!("tool:{tool}:{outcome}:{count}"))),
    );
    evidence
}

/// Everything [`FixtureRepair`] needs that is not the model.
///
/// One struct rather than seven constructor arguments, because every field here
/// is a host decision an operator configures and none of them is derivable from
/// the others. Grouping them also keeps the model — the one value with a
/// credential behind it — visibly separate from the rest.
///
/// **The attempt id is not here**, and its absence is the point. It used to be:
/// the CLI minted one when it assembled the capability, and the runtime minted
/// the bundle's separately, so the evidence reference below named an attempt
/// that appeared in no bundle and on no disk. It now arrives on the
/// [`ExecutionGrant`], which is the one value that already means "this attempt
/// authorises this execution" — so there is nowhere left for a caller to supply
/// a second one. Every field that remains is a *deployment* decision; the
/// attempt is a property of the run, and the run owns it.
pub struct RepairConfig {
    /// The repository under repair. Each attempt branches a worktree from it and
    /// never writes to it.
    pub fixture: PathBuf,

    /// Where per-attempt worktrees are created.
    pub workspace_root: PathBuf,

    /// The fixture root the change set is recorded under, which is where the
    /// next invocation's assessment looks for the marker.
    pub stub_root: PathBuf,

    /// The project half of the correlation key. Held rather than derived,
    /// because the key is a function of the project *and* the invocation
    /// reference, and the capability must compute the same one the assessment
    /// will compare against.
    pub project: String,

    /// The check that decides whether this repair earned anything.
    pub check: WorkspaceCommand,

    /// What one bounded attempt runs inside.
    pub budget: AgentBudget,

    /// Stops the attempt, the tools, and the check together.
    pub cancel: CancellationToken,
}

/// One bounded agent attempt at repairing a fixture, verified independently.
///
/// Generic over Rig's own completion-model trait for the reason
/// [`attempt`] is: a test substitutes a scripted model and drives the real
/// tools, the real worktree and the real check without a credential or a socket.
/// The whole of this milestone's central property is therefore provable offline.
pub struct FixtureRepair<M> {
    model: M,
    config: RepairConfig,
    /// The record the tools append to, held here rather than only inside the
    /// [`ToolHost`] so that [`Capability::receipts`] can read it *after* the
    /// execution — including after one that failed. The host gets a clone of
    /// this same `Arc`, so there is one record and no copy-back step that an
    /// early return could skip.
    receipts: Arc<Mutex<ToolReceipts>>,
}

impl<M> FixtureRepair<M> {
    /// A capability that will run `model` under `config`.
    pub fn new(model: M, config: RepairConfig) -> Self {
        FixtureRepair {
            model,
            config,
            receipts: Arc::new(Mutex::new(ToolReceipts::default())),
        }
    }

    /// Record this invocation's correlation key as the change set for the work
    /// item.
    ///
    /// Deliberately identical to what [`StubMark`](super::StubMark) writes,
    /// through the same atomic write: the assessment that reads it does not know
    /// or care which capability produced it, and two capabilities writing
    /// subtly different files for the same reader is a defect waiting for a
    /// change of capability to expose it.
    fn record_change_set(
        &self,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<(), CapabilityError> {
        let state = ChangeSetState {
            marker: Some(correlation_key(&self.config.project, invocation_ref)),
        };
        let destination = self
            .config
            .stub_root
            .join(format!("changes/{work_id}.json"));
        super::stub::write_atomically(&destination, &state).map_err(|source| {
            CapabilityError::Write {
                path: destination.clone(),
                source,
            }
        })
    }
}

#[async_trait::async_trait]
impl<M> Capability for FixtureRepair<M>
where
    M: rig_core::completion::CompletionModel + 'static,
{
    fn id(&self) -> CapabilityId {
        fiddle_core::FIXTURE_REPAIR
    }

    /// One bounded attempt judged by one check is one step, so this capability
    /// has one stage too — but it is *this* capability's step, and naming it
    /// after M0's is what published `stage: "mark"` on a repair.
    fn stage(&self) -> &'static str {
        "repair"
    }

    fn receipts(&self) -> Vec<EvidenceRef> {
        tool_evidence(
            &self
                .receipts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    }

    async fn execute(
        &self,
        grant: ExecutionGrant,
        work_id: &str,
        invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        if grant.capability_id() != self.id() {
            return Err(CapabilityError::NotAuthorised {
                granted: grant.capability_id(),
                requested: self.id(),
            });
        }
        let config = &self.config;
        // The attempt this execution is part of, taken from the grant rather
        // than from the configuration. It names the worktree and it is quoted in
        // the evidence below, and both of those are claims about *this run* —
        // so both must be the id the journal record and the published bundle are
        // filed under, which is the one the grant carries.
        let attempt_id = grant.attempt_id();

        // Held for the whole of this function and dropped at the end of it on
        // every path out — an early return, a `?`, a panic — because the Drop
        // guard is what removes the worktree. The `Arc` exists only because the
        // tools reach the same workspace; nothing outside this scope keeps a
        // clone, so the count is back to one by the time it matters.
        let workspace = Arc::new(Workspace::create(
            &config.fixture,
            &config.workspace_root,
            attempt_id,
            config.cancel.clone(),
        )?);

        let host = ToolHost {
            workspace: Arc::clone(&workspace),
            cancel: config.cancel.clone(),
            check: config.check.clone(),
            receipts: Arc::clone(&self.receipts),
        };

        // One attempt, bounded. An attempt that failed produced no repair, so
        // there is nothing below for the check to be a check *of*.
        let report = attempt(
            self.model.clone(),
            host,
            config.budget.clone(),
            // A repair is never redirected: nothing asks anybody anything on this
            // capability, so there is no instruction there could be one of.
            Direction::Fresh,
        )
        .await?;

        // Verified by the shell, independently, whatever the report said. The
        // model may have run the same check itself through `run_check`; that
        // result is a message in its transcript and this one is the verdict.
        let check = workspace.run(&config.check).await?;
        // Asked of git rather than of the report, for the same reason: a
        // changed-file list the model authored is a claim about a tree fiddle
        // can simply go and look at.
        let changed = workspace.changed_files()?;

        if check.exit_code != 0 {
            return Err(CapabilityError::CheckFailed {
                claimed: report.claimed_complete,
                exit_code: check.exit_code,
                stderr: check.stderr,
            });
        }

        // Earned: the check passed, so this invocation may account for the work.
        self.record_change_set(work_id, invocation_ref)?;
        // `repair:<changed>:<attempt>` is a cross-reference, and it now resolves:
        // the attempt it names is the one this run's bundle is published under,
        // so a reader holding this reference can go and open that document.
        Ok(EvidenceRef(format!(
            "{REPAIR_ORIGIN}:{}:{}",
            changed.len(),
            attempt_id.0
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::{AttemptId, NextAction};
    use rig_core::test_utils::{MockCompletionModel, MockTurn};
    use serde_json::json;
    use std::path::Path;
    use std::time::Duration;

    const WORK_ID: &str = "fiddle-m1-demo";
    const INVOCATION_REF: &str = "beans:fiddle-m1-demo";
    const PROJECT: &str = "icecube";

    /// The attempt every scenario here executes under. Fixed rather than minted,
    /// so the evidence reference these tests assert on is a function of the run
    /// rather than of the clock.
    const ATTEMPT: &str = "01JQZX0000000000000000000";

    /// The defect the fixture ships with: an off-by-one that compiles cleanly
    /// and fails its own test, so a repair has to be a real edit rather than a
    /// syntax fix a formatter could have made.
    const BROKEN: &str = "pub fn last_index(len: usize) -> usize { len }\n";

    /// The edit that makes the check pass.
    const REPAIRED: &str = "pub fn last_index(len: usize) -> usize { len - 1 }\n";

    fn grant() -> ExecutionGrant {
        ExecutionGrant::authorise(
            &NextAction::Execute {
                capability_id: fiddle_core::FIXTURE_REPAIR,
            },
            &AttemptId(ATTEMPT.to_string()),
        )
        .expect("an Execute derivation authorises")
    }

    /// A disposable project: a broken crate as a git repository, a place for
    /// per-attempt worktrees, and the fixture root the change set is written to.
    struct Fixture {
        dir: tempfile::TempDir,
        repo: PathBuf,
    }

    /// A deliberately broken zero-dependency Rust crate, as a git repository.
    ///
    /// Zero dependencies so `cargo test --offline` needs nothing from the
    /// network. `target/` and `Cargo.lock` are gitignored so that
    /// `git status --porcelain` over a worktree reports what an attempt
    /// *changed* and not what running the check *produced* — a lock file
    /// regenerated by the check is not a repair, and counting it would spend the
    /// changed-file cap on noise.
    fn broken_fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let repo = dir.path().join("fixture");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(repo.join("tests")).unwrap();
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n",
        )
        .unwrap();
        std::fs::write(repo.join("src/lib.rs"), BROKEN).unwrap();
        std::fs::write(
            repo.join("tests/repair.rs"),
            "#[test]\nfn the_last_index_is_one_before_the_length() {\n    \
             assert_eq!(fixture::last_index(3), 2);\n}\n",
        )
        .unwrap();
        std::fs::write(repo.join(".gitignore"), "target/\nCargo.lock\n").unwrap();

        git(&repo, &["init", "-q", "."]);
        git(&repo, &["add", "-A"]);
        git(
            &repo,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "the broken fixture",
            ],
        );
        Fixture { dir, repo }
    }

    /// Run git in `dir`, panicking with its stderr if it fails.
    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap_or_else(|source| panic!("could not run git {args:?}: {source}"));
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    impl Fixture {
        fn config(&self) -> RepairConfig {
            RepairConfig {
                fixture: self.repo.clone(),
                workspace_root: self.workspace_root(),
                stub_root: self.stub_root(),
                project: PROJECT.to_string(),
                check: WorkspaceCommand {
                    program: "cargo".to_string(),
                    args: vec!["test".to_string(), "--offline".to_string()],
                    timeout: Duration::from_secs(180),
                },
                budget: AgentBudget {
                    max_turns: 8,
                    max_tokens: 4096,
                    deadline: Duration::from_secs(300),
                    max_changed_files: 16,
                    tool_timeout: Duration::from_secs(180),
                },
                cancel: CancellationToken::new(),
            }
        }

        fn stub_root(&self) -> PathBuf {
            self.dir.path().join("stub-state")
        }

        fn workspace_root(&self) -> PathBuf {
            self.dir.path().join("workspaces")
        }

        fn marker_path(&self, work_id: &str) -> PathBuf {
            self.stub_root().join(format!("changes/{work_id}.json"))
        }
    }

    /// The model reads one file, changes nothing, and says it is done.
    fn lies() -> Vec<MockTurn> {
        vec![
            MockTurn::tool_call("c1", "read_file", json!({"path": "src/lib.rs"})),
            MockTurn::text(r#"{"changed_files":[],"summary":"all good","claimed_complete":true}"#),
        ]
    }

    /// The model writes the fix, runs the check itself, and reports.
    fn repairs() -> Vec<MockTurn> {
        vec![
            MockTurn::tool_call(
                "c1",
                "write_file",
                json!({"path": "src/lib.rs", "contents": REPAIRED}),
            ),
            MockTurn::tool_call("c2", "run_check", json!({})),
            MockTurn::text(
                r#"{"changed_files":["src/lib.rs"],"summary":"fixed","claimed_complete":true}"#,
            ),
        ]
    }

    /// The model's final message is not the schema at all.
    fn malformed() -> Vec<MockTurn> {
        vec![MockTurn::text("this is not the schema")]
    }

    /// Every tool call the model makes has arguments that do not decode, so no
    /// tool body ever runs — and then it claims completion anyway.
    fn nothing_but_malformed_calls() -> Vec<MockTurn> {
        vec![
            MockTurn::tool_call("c1", "write_file", json!({"wrong": "shape"})),
            MockTurn::tool_call("c2", "read_file", json!({})),
            MockTurn::text(r#"{"changed_files":[],"summary":"done","claimed_complete":true}"#),
        ]
    }

    /// **The centre of this milestone.** The agent claims completion over a
    /// fixture whose check still fails. The shell must disbelieve it.
    #[tokio::test]
    async fn a_model_that_lies_about_success_does_not_earn_the_marker() {
        let f = broken_fixture();
        let error = FixtureRepair::new(MockCompletionModel::new(lies()), f.config())
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .unwrap_err();

        match &error {
            CapabilityError::CheckFailed {
                claimed, exit_code, ..
            } => {
                assert!(
                    *claimed,
                    "the claim is carried as evidence, so it must be recorded"
                );
                assert_ne!(*exit_code, 0, "the check is what decided this");
            }
            other => panic!("a failing check must be reported as such, got {other:?}"),
        }
        assert!(
            !f.marker_path(WORK_ID).exists(),
            "a repair that did not pass its check must leave no correlation marker"
        );
    }

    /// The other direction of the same rule, and the one that proves the flag is
    /// not consulted rather than merely inverted: a model that says it did *not*
    /// finish, over a tree whose check passes, still earns the marker.
    #[tokio::test]
    async fn a_model_that_disclaims_success_still_earns_a_marker_its_check_passes() {
        let f = broken_fixture();
        let mut script = repairs();
        script.pop();
        script.push(MockTurn::text(
            r#"{"changed_files":["src/lib.rs"],"summary":"not sure","claimed_complete":false}"#,
        ));

        FixtureRepair::new(MockCompletionModel::new(script), f.config())
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .expect("the check passed, so the outcome is decided");

        assert!(
            f.marker_path(WORK_ID).exists(),
            "the check exited 0, and nothing else has a vote"
        );
    }

    #[tokio::test]
    async fn a_real_repair_passes_the_check_and_records_the_marker() {
        let f = broken_fixture();
        let evidence = FixtureRepair::new(MockCompletionModel::new(repairs()), f.config())
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .unwrap();

        let state: ChangeSetState =
            serde_json::from_str(&std::fs::read_to_string(f.marker_path(WORK_ID)).unwrap())
                .unwrap();
        assert_eq!(
            state.marker.as_deref(),
            Some(correlation_key(PROJECT, INVOCATION_REF).as_str()),
            "the marker must be the one the next invocation's assessment expects"
        );
        assert_eq!(
            evidence.0,
            format!("repair:1:{ATTEMPT}"),
            "the evidence names what git saw change, not what the model claimed \
             — and names the attempt it was granted, not one of its own"
        );
    }

    #[tokio::test]
    async fn the_workspace_is_gone_whatever_happened() {
        for (name, script) in [
            ("malformed", malformed()),
            ("lies", lies()),
            ("repairs", repairs()),
        ] {
            let f = broken_fixture();
            let _ = FixtureRepair::new(MockCompletionModel::new(script), f.config())
                .execute(grant(), WORK_ID, INVOCATION_REF)
                .await;

            // The root itself must exist, or "empty" would be the vacuous truth
            // of an attempt that never got as far as preparing a workspace.
            assert!(
                f.workspace_root().exists(),
                "the `{name}` attempt never prepared a workspace, so nothing was proven"
            );
            let leftovers: Vec<_> = std::fs::read_dir(f.workspace_root())
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            assert!(
                leftovers.is_empty(),
                "the `{name}` attempt left a workspace behind: {leftovers:?}"
            );
        }
    }

    /// The carried-in question from Task 7, answered by the check rather than by
    /// a rule about transcripts: a model that malformed every call it made did
    /// not repair anything, so its check fails and it earns nothing.
    #[tokio::test]
    async fn a_transcript_of_nothing_but_malformed_calls_is_judged_by_the_check() {
        let f = broken_fixture();
        let error = FixtureRepair::new(
            MockCompletionModel::new(nothing_but_malformed_calls()),
            f.config(),
        )
        .execute(grant(), WORK_ID, INVOCATION_REF)
        .await
        .unwrap_err();

        assert!(
            matches!(error, CapabilityError::CheckFailed { .. }),
            "no separate transcript rule is needed; the check already refuses it: {error:?}"
        );
        assert!(!f.marker_path(WORK_ID).exists());
    }

    /// An attempt that never produced a report is not verified and not
    /// rewarded — there is no repair to check.
    #[tokio::test]
    async fn a_failed_attempt_is_reported_as_the_agents_failure_and_earns_nothing() {
        let f = broken_fixture();
        let error = FixtureRepair::new(MockCompletionModel::new(malformed()), f.config())
            .execute(grant(), WORK_ID, INVOCATION_REF)
            .await
            .unwrap_err();

        assert!(
            matches!(
                error,
                CapabilityError::Agent(crate::agent::AgentError::Protocol { .. })
            ),
            "got {error:?}"
        );
        assert!(!f.marker_path(WORK_ID).exists());
    }

    /// A capability handed someone else's grant refuses before it prepares
    /// anything at all.
    #[tokio::test]
    async fn a_grant_for_another_capability_is_refused_before_any_workspace_exists() {
        let f = broken_fixture();
        let foreign = ExecutionGrant::authorise(
            &NextAction::Execute {
                capability_id: fiddle_core::STUB_MARK,
            },
            &AttemptId(ATTEMPT.to_string()),
        )
        .unwrap();

        let error = FixtureRepair::new(MockCompletionModel::new(repairs()), f.config())
            .execute(foreign, WORK_ID, INVOCATION_REF)
            .await
            .unwrap_err();

        assert!(
            matches!(error, CapabilityError::NotAuthorised { .. }),
            "got {error:?}"
        );
        assert!(
            !f.workspace_root().exists(),
            "a refused execution must not even prepare a workspace"
        );
        assert!(!f.marker_path(WORK_ID).exists());
    }
}
