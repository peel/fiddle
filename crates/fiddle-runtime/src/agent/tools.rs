use super::{ToolReceipt, ToolReceipts};
use crate::workspace::declared::{resolve, DeclaredCommand, Undeclared};
use crate::workspace::{Workspace, WorkspaceCommand, WorkspaceError, WorkspacePath};
use rig_agent::tool::{Tool, ToolContext, ToolExecutionError};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct ToolHost {
    pub workspace: Arc<Workspace>,
    pub cancel: CancellationToken,
    pub check: WorkspaceCommand,
    pub commands: Arc<Vec<DeclaredCommand>>,
    pub command_timeout: std::time::Duration,
    pub receipts: Arc<Mutex<ToolReceipts>>,
}

impl ToolHost {
    fn from_context(ctx: &ToolContext) -> Result<Self, ToolError> {
        ctx.require::<ToolHost>()
            .cloned()
            .map_err(|_| ToolError::NoHostContext)
    }

    fn guard(&self) -> Result<(), ToolError> {
        if self.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        Ok(())
    }

    fn recorded<T>(
        &self,
        tool: &'static str,
        started: Instant,
        result: Result<T, ToolError>,
    ) -> Result<T, ToolError> {
        let receipt = ToolReceipt {
            tool: tool.to_string(),
            outcome: outcome_of(&result),
            duration_ms: started.elapsed().as_millis() as u64,
        };
        self.receipts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .calls
            .push(receipt);
        result
    }

    pub fn receipts(&self) -> ToolReceipts {
        self.receipts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

fn outcome_of<T>(result: &Result<T, ToolError>) -> &'static str {
    match result {
        Ok(_) => "ok",
        Err(ToolError::NoHostContext)
        | Err(ToolError::Rejected { .. })
        | Err(ToolError::Undeclared { .. }) => "refused",
        Err(ToolError::Cancelled) => "cancelled",
        Err(ToolError::Timeout { .. }) | Err(ToolError::Failed { .. }) => "failed",
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("this tool is not configured and will not act")]
    NoHostContext,

    #[error("the attempt was cancelled")]
    Cancelled,

    #[error("the path `{path}` was refused: {reason}")]
    Rejected {
        path: String,
        reason: String,
        #[source]
        source: WorkspaceError,
    },

    #[error("{source}")]
    Undeclared {
        #[source]
        source: Undeclared,
    },

    #[error("the check did not finish within its time limit")]
    Timeout {
        #[source]
        source: WorkspaceError,
    },

    #[error("{operation} did not succeed")]
    Failed {
        operation: &'static str,
        #[source]
        source: WorkspaceError,
    },
}

impl ToolError {
    fn from_workspace(operation: &'static str, source: WorkspaceError) -> Self {
        match source {
            WorkspaceError::Escape {
                ref path,
                ref reason,
            }
            | WorkspaceError::NotProject {
                ref path,
                ref reason,
            } => ToolError::Rejected {
                path: path.clone(),
                reason: reason.clone(),
                source,
            },
            WorkspaceError::Cancelled => ToolError::Cancelled,
            WorkspaceError::Timeout { .. } => ToolError::Timeout { source },
            WorkspaceError::Io { .. } | WorkspaceError::Git { .. } => {
                ToolError::Failed { operation, source }
            }
        }
    }

    fn into_execution_error(self) -> ToolExecutionError {
        let message = self.to_string();
        let classified = match &self {
            ToolError::NoHostContext => ToolExecutionError::refused(message),
            ToolError::Cancelled => ToolExecutionError::cancelled(message),
            ToolError::Rejected { .. } | ToolError::Undeclared { .. } => {
                ToolExecutionError::invalid_args(message)
            }
            ToolError::Timeout { .. } => ToolExecutionError::timeout(message),
            ToolError::Failed { .. } => ToolExecutionError::other(message),
        };
        classified.with_source(self)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct NoArgs {}

fn no_parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

pub struct ReadFile;

#[derive(Clone, Debug, Deserialize)]
pub struct ReadFileArgs {
    pub path: String,
}

impl Tool for ReadFile {
    const NAME: &'static str = "read_file";
    type Args = ReadFileArgs;
    type Output = String;
    type Error = ToolError;

    fn description(&self) -> String {
        "Read one file from the project you are repairing, by its relative path.".into()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path of the file, for example src/lib.rs."
                }
            },
            "required": ["path"],
            "additionalProperties": false
        })
    }

    async fn call(&self, ctx: &mut ToolContext, args: Self::Args) -> Result<String, ToolError> {
        let host = ToolHost::from_context(ctx)?;
        let started = Instant::now();
        let result = async {
            host.guard()?;
            let path = parse(&args.path)?;
            host.workspace
                .read(&path)
                .map_err(|source| ToolError::from_workspace("reading the file", source))
        }
        .await;
        host.recorded(Self::NAME, started, result)
    }

    fn map_error(&self, error: Self::Error) -> ToolExecutionError {
        error.into_execution_error()
    }
}

pub struct WriteFile;

#[derive(Clone, Debug, Deserialize)]
pub struct WriteFileArgs {
    pub path: String,
    pub contents: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WriteReceipt {
    pub path: String,
    pub bytes: usize,
}

impl Tool for WriteFile {
    const NAME: &'static str = "write_file";
    type Args = WriteFileArgs;
    type Output = WriteReceipt;
    type Error = ToolError;

    fn description(&self) -> String {
        "Replace one file in the project you are repairing with the contents you supply. \
         The file is created if it does not exist, along with any directories on the way to it."
            .into()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path of the file, for example src/lib.rs."
                },
                "contents": {
                    "type": "string",
                    "description": "The complete new contents of the file."
                }
            },
            "required": ["path", "contents"],
            "additionalProperties": false
        })
    }

    async fn call(
        &self,
        ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<WriteReceipt, ToolError> {
        let host = ToolHost::from_context(ctx)?;
        let started = Instant::now();
        let result = async {
            host.guard()?;
            let path = parse(&args.path)?;
            host.workspace
                .write(&path, &args.contents)
                .map_err(|source| ToolError::from_workspace("writing the file", source))?;
            Ok(WriteReceipt {
                path: path.as_str().to_string(),
                bytes: args.contents.len(),
            })
        }
        .await;
        host.recorded(Self::NAME, started, result)
    }

    fn map_error(&self, error: Self::Error) -> ToolExecutionError {
        error.into_execution_error()
    }
}

pub struct ListFiles;

impl Tool for ListFiles {
    const NAME: &'static str = "list_files";
    type Args = NoArgs;
    type Output = Vec<String>;
    type Error = ToolError;

    fn description(&self) -> String {
        "List the files of the project you are repairing, as relative paths.".into()
    }

    fn parameters(&self) -> serde_json::Value {
        no_parameters()
    }

    async fn call(&self, ctx: &mut ToolContext, _args: NoArgs) -> Result<Vec<String>, ToolError> {
        let host = ToolHost::from_context(ctx)?;
        let started = Instant::now();
        let result = async {
            host.guard()?;
            let listed = host
                .workspace
                .list()
                .map_err(|source| ToolError::from_workspace("listing the files", source))?;
            Ok(listed
                .into_iter()
                .map(|path| path.as_str().to_string())
                .collect())
        }
        .await;
        host.recorded(Self::NAME, started, result)
    }

    fn map_error(&self, error: Self::Error) -> ToolExecutionError {
        error.into_execution_error()
    }
}

pub struct RunCheck;

#[derive(Clone, Debug, Serialize)]
pub struct CheckOutcome {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Tool for RunCheck {
    const NAME: &'static str = "run_check";
    type Args = NoArgs;
    type Output = CheckOutcome;
    type Error = ToolError;

    fn description(&self) -> String {
        "Run the project's build and test check and return what it printed.".into()
    }

    fn parameters(&self) -> serde_json::Value {
        no_parameters()
    }

    async fn call(&self, ctx: &mut ToolContext, _args: NoArgs) -> Result<CheckOutcome, ToolError> {
        let host = ToolHost::from_context(ctx)?;
        let started = Instant::now();
        let result = async {
            host.guard()?;
            let result = host
                .workspace
                .run(&host.check)
                .await
                .map_err(|source| ToolError::from_workspace("running the check", source))?;
            Ok(CheckOutcome {
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
            })
        }
        .await;
        host.recorded(Self::NAME, started, result)
    }

    fn map_error(&self, error: Self::Error) -> ToolExecutionError {
        error.into_execution_error()
    }
}

pub struct RunCommand;

#[derive(Clone, Debug, Deserialize)]
pub struct RunCommandArgs {
    pub program: String,

    #[serde(default)]
    pub args: Vec<String>,
}

impl Tool for RunCommand {
    const NAME: &'static str = "run_command";
    type Args = RunCommandArgs;
    type Output = CheckOutcome;
    type Error = ToolError;

    fn description(&self) -> String {
        "Run one of the programs this project declares, in the project, and return \
         what it printed. Name the program and give its arguments as a list. A \
         program the project does not declare is refused, and the refusal names \
         every program it does declare and the arguments each one takes. There is \
         no interpreter between you and the program: the arguments reach it as you \
         wrote them, nothing in them is expanded, and one argument cannot become two."
            .into()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "program": {
                    "type": "string",
                    "description": "The program to run, named as the project declares it."
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "The whole argument list, one entry per argument. \
                                    A declaration's own arguments come first and in order; \
                                    you may append to them where the declaration permits it."
                }
            },
            "required": ["program", "args"],
            "additionalProperties": false
        })
    }

    async fn call(
        &self,
        ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<CheckOutcome, ToolError> {
        let host = ToolHost::from_context(ctx)?;
        let started = Instant::now();
        let result = async {
            host.guard()?;
            let command = resolve(
                &host.commands,
                &args.program,
                &args.args,
                host.command_timeout,
            )
            .map_err(|source| ToolError::Undeclared { source })?;
            let result = host
                .workspace
                .run(&command)
                .await
                .map_err(|source| ToolError::from_workspace("running the command", source))?;
            Ok(CheckOutcome {
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
            })
        }
        .await;
        host.recorded(Self::NAME, started, result)
    }

    fn map_error(&self, error: Self::Error) -> ToolExecutionError {
        error.into_execution_error()
    }
}

fn parse(raw: &str) -> Result<WorkspacePath, ToolError> {
    WorkspacePath::parse(raw)
        .map_err(|source| ToolError::from_workspace("reading the path", source))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::agent::ToolReceipts;
    use crate::workspace::declared::Extend;
    use crate::workspace::{Workspace, WorkspaceCommand};
    use fiddle_core::AttemptId;
    use rig_agent::tool::{Tool, ToolContext};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    pub(crate) fn test_host() -> (ToolHost, tempfile::TempDir) {
        test_host_declaring(Vec::new())
    }

    fn declaration(program: &str, args: &[&str], extend: Extend) -> DeclaredCommand {
        DeclaredCommand {
            program: program.to_string(),
            args: args.iter().map(|a| a.to_string()).collect(),
            extend,
        }
    }

    fn owned(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    fn test_host_declaring(commands: Vec<DeclaredCommand>) -> (ToolHost, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let repo = dir.path().join("fixture");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
        std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        for args in [
            &["init", "-q", "."][..],
            &["add", "-A"][..],
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "fixture",
            ][..],
        ] {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        let cancel = CancellationToken::new();
        let workspace = Workspace::create(
            &repo,
            &dir.path().join("ws"),
            &AttemptId("01JQZX0000000000000000000".to_string()),
            cancel.clone(),
        )
        .expect("a workspace");

        let host = ToolHost {
            workspace: Arc::new(workspace),
            cancel,
            check: WorkspaceCommand {
                program: "git".to_string(),
                args: vec!["rev-parse".to_string(), "--is-inside-work-tree".to_string()],
                timeout: Duration::from_secs(30),
            },
            commands: Arc::new(commands),
            command_timeout: Duration::from_secs(30),
            receipts: Arc::new(Mutex::new(ToolReceipts::default())),
        };
        (host, dir)
    }

    #[tokio::test]
    async fn a_tool_reads_its_workspace_from_host_context_not_from_arguments() {
        let (host, _g) = test_host();
        let mut ctx = ToolContext::new();
        ctx.insert(host);
        assert!(ReadFile
            .call(
                &mut ctx,
                ReadFileArgs {
                    path: "src/lib.rs".into()
                }
            )
            .await
            .unwrap()
            .contains("pub fn"));

        let schema = ReadFile.parameters().to_string();
        assert!(schema.contains("path"));
        assert!(
            !schema.contains("workspace") && !schema.contains("root") && !schema.contains("cancel"),
            "the host context must not be model-visible: {schema}"
        );
    }

    #[tokio::test]
    async fn a_tool_without_host_context_fails_rather_than_defaulting() {
        let mut ctx = ToolContext::new();
        assert!(
            ReadFile
                .call(
                    &mut ctx,
                    ReadFileArgs {
                        path: "src/lib.rs".into()
                    }
                )
                .await
                .is_err(),
            "a missing host context must fail closed, never fall back to the process cwd"
        );
    }

    #[tokio::test]
    async fn write_file_refuses_a_path_that_leaves_the_workspace() {
        let (host, _g) = test_host();
        let outside = host
            .workspace
            .root()
            .parent()
            .expect("the workspace has a parent to escape into")
            .join("escape.txt");
        let mut ctx = ToolContext::new();
        ctx.insert(host);
        assert!(WriteFile
            .call(
                &mut ctx,
                WriteFileArgs {
                    path: "../escape.txt".into(),
                    contents: "x".into()
                }
            )
            .await
            .is_err());
        assert!(
            !outside.exists(),
            "the refusal came after the write: {}",
            outside.display()
        );
    }

    #[tokio::test]
    async fn write_file_can_add_a_file_in_a_directory_the_project_does_not_have_yet() {
        let (host, _g) = test_host();
        let root = host.workspace.root().to_path_buf();
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let receipt = WriteFile
            .call(
                &mut ctx,
                WriteFileArgs {
                    path: "src/newmod/deep/a.rs".into(),
                    contents: "pub fn a() {}\n".into(),
                },
            )
            .await
            .expect("a new directory is the workspace's to make, not the model's to work around");

        assert_eq!(receipt.path, "src/newmod/deep/a.rs");
        assert_eq!(
            std::fs::read_to_string(root.join("src/newmod/deep/a.rs")).unwrap(),
            "pub fn a() {}\n",
            "the receipt has to describe a file that is there"
        );
    }

    #[tokio::test]
    async fn cancellation_between_inspection_and_mutation_prevents_the_write() {
        let (host, _g) = test_host();
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        ReadFile
            .call(
                &mut ctx,
                ReadFileArgs {
                    path: "src/lib.rs".into(),
                },
            )
            .await
            .unwrap();
        let before = std::fs::read_to_string(host.workspace.root().join("src/lib.rs")).unwrap();
        host.cancel.cancel();

        assert!(WriteFile
            .call(
                &mut ctx,
                WriteFileArgs {
                    path: "src/lib.rs".into(),
                    contents: "mutated".into()
                }
            )
            .await
            .is_err());
        assert_eq!(
            std::fs::read_to_string(host.workspace.root().join("src/lib.rs")).unwrap(),
            before,
            "cancellation must prevent the effect, not merely end the future"
        );
    }

    #[tokio::test]
    async fn run_check_executes_the_host_command_not_a_model_supplied_one() {
        assert_eq!(RunCheck.parameters()["properties"], serde_json::json!({}));
    }

    #[tokio::test]
    async fn a_model_supplied_program_is_ignored_and_the_host_command_runs() {
        let (host, _g) = test_host();
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let smuggled: NoArgs =
            serde_json::from_str(r#"{"program":"rm","args":["-rf","/"]}"#).expect("ignored");
        let outcome = RunCheck.call(&mut ctx, smuggled).await.unwrap();
        assert_eq!(outcome.exit_code, 0);
        assert!(
            outcome.stdout.contains("true"),
            "the host's own check ran, not the smuggled one: {outcome:?}"
        );
    }

    #[tokio::test]
    async fn the_checks_own_output_does_not_carry_the_host_layout_to_the_model() {
        let (mut host, _g) = test_host();
        host.check = WorkspaceCommand {
            program: "git".to_string(),
            args: vec!["rev-parse".to_string(), "--show-toplevel".to_string()],
            timeout: Duration::from_secs(30),
        };
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        let outcome = RunCheck.call(&mut ctx, NoArgs::default()).await.unwrap();
        assert_eq!(outcome.exit_code, 0);
        for root in roots(&host) {
            assert!(
                !outcome.stdout.contains(&root),
                "the workspace's absolute path reached the model: {outcome:?}"
            );
        }
        assert!(
            outcome.stdout.trim() == ".",
            "what is left must still be a usable path: {outcome:?}"
        );
    }

    #[tokio::test]
    async fn list_files_names_the_project_and_not_what_a_build_leaves_behind() {
        let (host, _g) = test_host();
        std::fs::create_dir_all(host.workspace.root().join("target/debug")).unwrap();
        std::fs::write(host.workspace.root().join("target/debug/junk"), "x").unwrap();
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let listed = ListFiles.call(&mut ctx, NoArgs::default()).await.unwrap();
        assert!(listed.contains(&"src/lib.rs".to_string()), "{listed:?}");
        assert!(
            !listed.iter().any(|p| p.starts_with("target/")),
            "an ignored build tree would drown the model's context: {listed:?}"
        );
    }

    #[tokio::test]
    async fn no_tool_advertises_a_host_fact() {
        let (host, _g) = test_host();
        let root = host.workspace.root().display().to_string();
        let surfaces = [
            (ReadFile.parameters().to_string(), ReadFile.description()),
            (WriteFile.parameters().to_string(), WriteFile.description()),
            (ListFiles.parameters().to_string(), ListFiles.description()),
            (RunCheck.parameters().to_string(), RunCheck.description()),
            (
                RunCommand.parameters().to_string(),
                RunCommand.description(),
            ),
        ];
        for (schema, description) in surfaces {
            for text in [&schema, &description] {
                assert!(!text.contains(&root), "a host path is advertised: {text}");
                assert!(
                    !text.contains(&host.check.program),
                    "the check program is advertised: {text}"
                );
                for banned in ["workspace", "root", "cancel"] {
                    assert!(
                        !text.contains(banned),
                        "`{banned}` names a host fact the model must not be invited to supply: {text}"
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn a_refusal_tells_the_model_its_own_path_and_nothing_of_the_host() {
        let (host, _g) = test_host();
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        let refused = WriteFile
            .call(
                &mut ctx,
                WriteFileArgs {
                    path: "../escape.txt".into(),
                    contents: "x".into(),
                },
            )
            .await
            .unwrap_err();
        let seen = WriteFile.map_error(refused);
        let text = seen
            .model_output()
            .as_text()
            .unwrap_or_default()
            .to_string();
        assert!(
            text.contains("../escape.txt"),
            "a refusal the model cannot act on is a wasted turn: {text}"
        );
        for root in roots(&host) {
            assert!(
                !text.contains(&root),
                "a host path leaked in a refusal: {text}"
            );
        }
    }

    #[tokio::test]
    async fn a_failed_read_never_hands_the_model_a_host_path() {
        let (host, _g) = test_host();
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        let failed = ReadFile
            .call(
                &mut ctx,
                ReadFileArgs {
                    path: "src/absent.rs".into(),
                },
            )
            .await
            .unwrap_err();
        for text in [
            failed.to_string(),
            ReadFile
                .map_error(failed)
                .model_output()
                .as_text()
                .unwrap_or_default()
                .to_string(),
        ] {
            for root in roots(&host) {
                assert!(
                    !text.contains(&root),
                    "the host's filesystem layout leaked to the model: {text}"
                );
            }
        }
    }

    #[tokio::test]
    async fn the_runtime_records_every_tool_call_without_the_hook() {
        let (host, _g) = test_host();
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        ReadFile
            .call(
                &mut ctx,
                ReadFileArgs {
                    path: "src/lib.rs".into(),
                },
            )
            .await
            .unwrap();
        let _ = WriteFile
            .call(
                &mut ctx,
                WriteFileArgs {
                    path: "../nope".into(),
                    contents: "x".into(),
                },
            )
            .await;

        let receipts = host.receipts();
        assert_eq!(
            receipts.calls.len(),
            2,
            "both the success and the refusal are evidence"
        );
        assert_eq!(receipts.calls[0].tool, "read_file");
        assert_eq!(receipts.calls[0].outcome, "ok");
        assert_eq!(receipts.calls[1].tool, "write_file");
        assert_eq!(receipts.calls[1].outcome, "refused");
    }

    #[tokio::test]
    async fn a_receipt_records_how_long_the_call_took() {
        let (mut host, _g) = test_host();
        host.check = WorkspaceCommand {
            program: "sleep".to_string(),
            args: vec!["0.2".to_string()],
            timeout: Duration::from_secs(30),
        };
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        RunCheck.call(&mut ctx, NoArgs::default()).await.unwrap();

        let receipts = host.receipts();
        assert_eq!(receipts.calls.len(), 1);
        assert_eq!(receipts.calls[0].tool, "run_check");
        assert!(
            receipts.calls[0].duration_ms >= 150,
            "the receipt did not time the call it describes: {:?}",
            receipts.calls[0]
        );
    }

    #[tokio::test]
    async fn a_cancelled_call_is_recorded_because_a_stopped_attempt_is_still_an_attempt() {
        let (host, _g) = test_host();
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        host.cancel.cancel();

        assert!(ReadFile
            .call(
                &mut ctx,
                ReadFileArgs {
                    path: "src/lib.rs".into()
                }
            )
            .await
            .is_err());

        let receipts = host.receipts();
        assert_eq!(
            receipts.calls.len(),
            1,
            "a call the guard stopped still happened: {receipts:?}"
        );
        assert_eq!(receipts.calls[0].outcome, "cancelled");
    }

    #[tokio::test]
    async fn a_check_that_outruns_its_bound_is_a_failure_and_not_a_refusal() {
        let (mut host, _g) = test_host();
        host.check = WorkspaceCommand {
            program: "sleep".to_string(),
            args: vec!["30".to_string()],
            timeout: Duration::from_millis(50),
        };
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        assert!(RunCheck.call(&mut ctx, NoArgs::default()).await.is_err());

        let receipts = host.receipts();
        assert_eq!(receipts.calls.len(), 1);
        assert_eq!(receipts.calls[0].outcome, "failed");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn receipts_survive_being_read_while_they_are_being_written() {
        let (host, _g) = test_host();

        let reader = {
            let host = host.clone();
            tokio::spawn(async move {
                let mut seen = 0;
                for _ in 0..500 {
                    seen = seen.max(host.receipts().calls.len());
                    tokio::task::yield_now().await;
                }
                seen
            })
        };

        let writers: Vec<_> = (0..8)
            .map(|_| {
                let host = host.clone();
                tokio::spawn(async move {
                    let mut ctx = ToolContext::new();
                    ctx.insert(host);
                    for _ in 0..10 {
                        ReadFile
                            .call(
                                &mut ctx,
                                ReadFileArgs {
                                    path: "src/lib.rs".into(),
                                },
                            )
                            .await
                            .unwrap();
                    }
                })
            })
            .collect();

        for writer in writers {
            writer.await.expect("a writer finished");
        }
        let observed = reader.await.expect("the reader finished");

        assert_eq!(
            host.receipts().calls.len(),
            80,
            "eight clones must append to one record, not to eight private ones"
        );
        assert!(observed <= 80, "a reader saw more calls than were made");
    }

    #[tokio::test]
    async fn a_receipt_carries_nothing_of_the_host_filesystem() {
        let (host, _g) = test_host();
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        ReadFile
            .call(
                &mut ctx,
                ReadFileArgs {
                    path: "src/lib.rs".into(),
                },
            )
            .await
            .unwrap();
        let _ = ReadFile
            .call(
                &mut ctx,
                ReadFileArgs {
                    path: "src/absent.rs".into(),
                },
            )
            .await;

        let published = serde_json::to_string(&host.receipts()).expect("receipts serialize");
        for root in roots(&host) {
            assert!(
                !published.contains(&root),
                "the host's filesystem layout reached the evidence bundle: {published}"
            );
        }
        assert!(
            !published.contains("absent.rs"),
            "a receipt echoed a model-authored argument: {published}"
        );
    }

    #[tokio::test]
    async fn a_tools_success_output_never_carries_the_host_layout() {
        let (host, dir) = test_host();
        let root = host.workspace.root().to_path_buf();

        let metadata = std::fs::read_to_string(root.join(".git")).expect(".git is a file here");
        assert!(
            metadata.contains("gitdir:") && metadata.contains("fixture"),
            "the premise is gone: .git no longer names the host's filesystem: {metadata}"
        );
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::write(
            root.join("target/debug/fixture.d"),
            format!(
                "{}/src/lib.rs: {}/src/lib.rs\n",
                root.display(),
                root.display()
            ),
        )
        .unwrap();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        for path in [".git", ".git/config", "target/debug/fixture.d"] {
            let result = ReadFile
                .call(
                    &mut ctx,
                    ReadFileArgs {
                        path: path.to_string(),
                    },
                )
                .await;
            let refusal = match result {
                Ok(contents) => panic!("`{path}` was served to the model: {contents}"),
                Err(refusal) => refusal,
            };
            let text = ReadFile
                .map_error(refusal)
                .model_output()
                .as_text()
                .unwrap_or_default()
                .to_string();
            for secret in layout(&host, &dir) {
                assert!(
                    !text.contains(&secret),
                    "reading `{path}` put {secret:?} in front of the model: {text}"
                );
            }
        }

        assert!(ReadFile
            .call(
                &mut ctx,
                ReadFileArgs {
                    path: "src/lib.rs".into()
                }
            )
            .await
            .unwrap()
            .contains("pub fn"));
    }

    #[tokio::test]
    async fn run_command_runs_a_declared_program_and_refuses_an_undeclared_one() {
        let (host, _g) =
            test_host_declaring(vec![declaration("/bin/echo", &["it-ran"], Extend::None)]);
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let declared = RunCommand
            .call(
                &mut ctx,
                RunCommandArgs {
                    program: "/bin/echo".into(),
                    args: owned(&["it-ran"]),
                },
            )
            .await
            .expect("a declared program is the one thing this tool runs");
        assert_eq!(declared.exit_code, 0);
        assert!(declared.stdout.contains("it-ran"), "{declared:?}");

        let refused = RunCommand
            .call(
                &mut ctx,
                RunCommandArgs {
                    program: "curl".into(),
                    args: owned(&["it-ran"]),
                },
            )
            .await
            .expect_err("and an undeclared one is the one thing it refuses");
        let text = RunCommand
            .map_error(refused)
            .model_output()
            .as_text()
            .unwrap_or_default()
            .to_string();
        assert!(
            text.contains("curl"),
            "a refusal that does not name what it refused cannot be acted on: {text}"
        );
    }

    #[tokio::test]
    async fn run_command_refuses_a_shell_because_a_shell_is_not_a_declared_program() {
        let (host, _g) = test_host_declaring(vec![declaration(
            "/bin/echo",
            &["it-ran"],
            Extend::Arguments,
        )]);
        let root = host.workspace.root().to_path_buf();
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let refused = RunCommand
            .call(
                &mut ctx,
                RunCommandArgs {
                    program: "/bin/sh".into(),
                    args: owned(&["-c", "echo reached > reached.txt"]),
                },
            )
            .await
            .expect_err("a shell turns one declared program into every program");
        let text = RunCommand
            .map_error(refused)
            .model_output()
            .as_text()
            .unwrap_or_default()
            .to_string();
        assert!(
            text.contains("/bin/sh"),
            "the refusal must name the shell it refused: {text}"
        );
        assert!(
            !root.join("reached.txt").exists(),
            "the refusal came after the shell ran"
        );
    }

    #[tokio::test]
    async fn a_declared_command_may_be_extended_where_the_declaration_says_so() {
        let (host, _g) = test_host_declaring(vec![
            declaration("/bin/echo", &["fixed"], Extend::None),
            declaration("/usr/bin/touch", &[], Extend::Arguments),
        ]);
        let root = host.workspace.root().to_path_buf();
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        RunCommand
            .call(
                &mut ctx,
                RunCommandArgs {
                    program: "/usr/bin/touch".into(),
                    args: owned(&["derived.txt"]),
                },
            )
            .await
            .expect("the declaration permits an appended argument");
        assert!(
            root.join("derived.txt").exists(),
            "a command runs in the project, so what it writes is in the project"
        );
        assert!(
            host.workspace
                .changed_files()
                .expect("a change set")
                .iter()
                .any(|path| path.as_str() == "derived.txt"),
            "a file a command wrote is in the diff the attempt has to declare"
        );

        assert!(
            RunCommand
                .call(
                    &mut ctx,
                    RunCommandArgs {
                        program: "/bin/echo".into(),
                        args: owned(&["fixed", "and-more"]),
                    },
                )
                .await
                .is_err(),
            "a declaration that permits no append takes no argument from the model"
        );
    }

    #[tokio::test]
    async fn a_declared_command_that_outruns_its_bound_is_a_failure_and_not_a_refusal() {
        let (mut host, _g) = test_host_declaring(vec![declaration("sleep", &["30"], Extend::None)]);
        host.command_timeout = Duration::from_millis(50);
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        assert!(RunCommand
            .call(
                &mut ctx,
                RunCommandArgs {
                    program: "sleep".into(),
                    args: owned(&["30"]),
                }
            )
            .await
            .is_err());

        let receipts = host.receipts();
        assert_eq!(receipts.calls.len(), 1);
        assert_eq!(receipts.calls[0].tool, "run_command");
        assert_eq!(receipts.calls[0].outcome, "failed");
    }

    #[tokio::test]
    async fn a_refused_command_is_recorded_as_refused_and_names_no_host_path() {
        let (host, _g) =
            test_host_declaring(vec![declaration("/bin/echo", &["it-ran"], Extend::None)]);
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        let refused = RunCommand
            .call(
                &mut ctx,
                RunCommandArgs {
                    program: "curl".into(),
                    args: Vec::new(),
                },
            )
            .await
            .expect_err("an undeclared program is refused");
        let text = RunCommand
            .map_error(refused)
            .model_output()
            .as_text()
            .unwrap_or_default()
            .to_string();
        for root in roots(&host) {
            assert!(
                !text.contains(&root),
                "a host path leaked in a refusal: {text}"
            );
        }

        let receipts = host.receipts();
        assert_eq!(receipts.calls.len(), 1);
        assert_eq!(receipts.calls[0].tool, "run_command");
        assert_eq!(
            receipts.calls[0].outcome, "refused",
            "a refusal is evidence: {receipts:?}"
        );
    }

    #[tokio::test]
    async fn a_cancelled_attempt_runs_no_declared_command() {
        let (host, _g) =
            test_host_declaring(vec![declaration("/usr/bin/touch", &[], Extend::Arguments)]);
        let root = host.workspace.root().to_path_buf();
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        host.cancel.cancel();

        assert!(RunCommand
            .call(
                &mut ctx,
                RunCommandArgs {
                    program: "/usr/bin/touch".into(),
                    args: owned(&["derived.txt"]),
                }
            )
            .await
            .is_err());
        assert!(
            !root.join("derived.txt").exists(),
            "cancellation must prevent the effect, not merely end the future"
        );
    }

    #[tokio::test]
    async fn the_schema_invites_no_program_so_the_declaration_is_the_only_source() {
        let schema = RunCommand.parameters().to_string();
        assert!(schema.contains("program") && schema.contains("args"));
        assert_eq!(
            RunCommand.parameters()["additionalProperties"],
            serde_json::json!(false),
            "a menu with room for one more field invites the model to fill it"
        );
    }

    fn roots(host: &ToolHost) -> Vec<String> {
        let root = host.workspace.root();
        let mut spellings = vec![root.display().to_string()];
        if let Ok(canonical) = root.canonicalize() {
            spellings.push(canonical.display().to_string());
        }
        spellings
    }

    fn layout(host: &ToolHost, dir: &tempfile::TempDir) -> Vec<String> {
        let mut secrets = roots(host);
        for path in [dir.path().to_path_buf(), dir.path().join("fixture")] {
            secrets.push(path.display().to_string());
            if let Ok(canonical) = path.canonicalize() {
                secrets.push(canonical.display().to_string());
            }
        }
        secrets.push("01JQZX0000000000000000000".to_string());
        secrets
    }
}
