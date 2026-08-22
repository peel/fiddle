use super::{Capability, CapabilityError, ExecutionGrant};
use crate::agent::{attempt, AgentBudget, Direction, ToolHost, ToolReceipts};
use crate::workspace::{DeclaredCommand, Workspace, WorkspaceCommand};
use fiddle_core::{correlation_key, CapabilityId, ChangeSetState, EvidenceRef};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

const REPAIR_ORIGIN: &str = "repair";

const REGISTERED_TOOLS: [&str; 6] = [
    "read_file",
    "edit_file",
    "write_file",
    "list_files",
    "run_check",
    "run_command",
];

const FOREIGN_TOOL: &str = "unregistered";

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

pub struct RepairConfig {
    pub fixture: PathBuf,

    pub workspace_root: PathBuf,

    pub stub_root: PathBuf,

    pub project: String,

    pub check: WorkspaceCommand,

    pub commands: Arc<Vec<DeclaredCommand>>,

    pub command_timeout: std::time::Duration,

    pub budget: AgentBudget,

    pub cancel: CancellationToken,
}

pub struct FixtureRepair<M> {
    model: M,
    config: RepairConfig,
    receipts: Arc<Mutex<ToolReceipts>>,
}

impl<M> FixtureRepair<M> {
    pub fn new(model: M, config: RepairConfig) -> Self {
        FixtureRepair {
            model,
            config,
            receipts: Arc::new(Mutex::new(ToolReceipts::default())),
        }
    }

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
        let attempt_id = grant.attempt_id();

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
            commands: Arc::clone(&config.commands),
            command_timeout: config.command_timeout,
            receipts: Arc::clone(&self.receipts),
        };

        let report = attempt(
            self.model.clone(),
            host,
            config.budget.clone(),
            Direction::Fresh,
        )
        .await?;

        let check = workspace.run(&config.check).await?;
        let changed = workspace.changed_files()?;

        if check.exit_code != 0 {
            return Err(CapabilityError::CheckFailed {
                claimed: report.claimed_complete,
                exit_code: check.exit_code,
                stderr: check.stderr,
            });
        }

        self.record_change_set(work_id, invocation_ref)?;
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

    const ATTEMPT: &str = "01JQZX0000000000000000000";

    const BROKEN: &str = "pub fn last_index(len: usize) -> usize { len }\n";

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

    struct Fixture {
        dir: tempfile::TempDir,
        repo: PathBuf,
    }

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
                commands: Arc::new(Vec::new()),
                command_timeout: Duration::from_secs(180),
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

    fn lies() -> Vec<MockTurn> {
        vec![
            MockTurn::tool_call("c1", "read_file", json!({"path": "src/lib.rs"})),
            MockTurn::text(r#"{"changed_files":[],"summary":"all good","claimed_complete":true}"#),
        ]
    }

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

    fn malformed() -> Vec<MockTurn> {
        vec![MockTurn::text("this is not the schema")]
    }

    fn nothing_but_malformed_calls() -> Vec<MockTurn> {
        vec![
            MockTurn::tool_call("c1", "write_file", json!({"wrong": "shape"})),
            MockTurn::tool_call("c2", "read_file", json!({})),
            MockTurn::text(r#"{"changed_files":[],"summary":"done","claimed_complete":true}"#),
        ]
    }

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
