//! The four tools the model is given, and the host context they read from.
//!
//! Read a file, write a file, list the files, run the check. That is the whole
//! of what a model can do here, and the smallness is the point: a tool set is a
//! grant of authority, and every tool added is a grant that has to be argued
//! for rather than assumed.
//!
//! Three rules hold across all four, in this order, before anything else
//! happens:
//!
//! 1. **The host context is required, not defaulted.** [`ToolContext::require`]
//!    rather than `get`, because the fallback a `get` invites is the process's
//!    own working directory — which is the repository fiddle is *running from*,
//!    not the one it is repairing. A tool that cannot tell which tree it is
//!    pointed at must refuse to act, not guess.
//! 2. **Cancellation is checked before resolution and before IO.** A guard that
//!    ran afterwards would end the future without preventing the effect, and a
//!    cancelled attempt whose last write still landed is worse than one that
//!    never started.
//! 3. **A requested path is proven, never joined.** [`WorkspacePath::parse`] is
//!    the syntactic half and [`Workspace::resolve`], reached through the
//!    workspace's own `read`/`write`, is the half that knows about symlinks.
//!
//! # What goes back to the model
//!
//! A tool result is a message to the model, so it is subject to the same
//! discipline as the schema: it may carry what the model already knows and what
//! it needs to act, and nothing about the host. [`ToolError`] therefore states
//! its own diagnostics without absolute paths and keeps the underlying
//! [`WorkspaceError`] — which names the *resolved* path on the operator's
//! filesystem — as an unrendered source. [`Tool::map_error`] is overridden for
//! the same reason: the default classifies every domain error as opaque, which
//! is safe but tells a model that mistyped a path nothing it can use.

use super::ToolReceipts;
use crate::workspace::{Workspace, WorkspaceCommand, WorkspaceError, WorkspacePath};
use rig_agent::tool::{Tool, ToolContext, ToolExecutionError};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// The host-only values a tool may reach. Never a tool argument.
///
/// Everything here is a fact about the attempt that the model has no standing
/// to choose: which checkout is being repaired, whether the attempt is still
/// live, and which program the check runs. It travels through Rig's
/// [`ToolContext`], which is host-populated and never serialized towards the
/// provider, so there is no representation of any of it that a model could
/// author.
///
/// `Clone` because `ToolContext` clones its inbound values once per dispatch.
/// The `Arc`s are what make that clone mean *shared*: two tool calls in the same
/// attempt must see one workspace and append to one set of receipts, not to
/// private copies that are discarded when the call ends.
#[derive(Clone)]
pub struct ToolHost {
    pub workspace: Arc<Workspace>,
    pub cancel: CancellationToken,
    pub check: WorkspaceCommand,
    pub receipts: Arc<Mutex<ToolReceipts>>,
}

impl ToolHost {
    /// The host context, or a refusal.
    ///
    /// Cloned out of the context rather than borrowed from it so that a tool may
    /// hold it across an `await` without borrowing the context for the whole
    /// call; the clone is two `Arc` bumps and a token.
    fn from_context(ctx: &ToolContext) -> Result<Self, ToolError> {
        // `require`, not `get`: the missing case is a host misconfiguration, and
        // the only safe response to it is to do nothing at all.
        ctx.require::<ToolHost>()
            .cloned()
            .map_err(|_| ToolError::NoHostContext)
    }

    /// Checked at the top of every tool, before any resolution or IO.
    fn guard(&self) -> Result<(), ToolError> {
        if self.cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        Ok(())
    }
}

/// Why a tool did not do what it was asked.
///
/// Each variant's message is written to be shown to a model: it names the
/// model's own input where that helps it recover, and never names the host's
/// filesystem. The [`WorkspaceError`] that caused it is carried as a `source`,
/// which `thiserror` does not render into the `Display` output, so the operator
/// keeps the resolved path and the model does not see it.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// No [`ToolHost`] was in the context, so the tool has no tree to act on.
    #[error("this tool is not configured and will not act")]
    NoHostContext,

    /// The attempt was cancelled before the tool did anything.
    #[error("the attempt was cancelled")]
    Cancelled,

    /// The requested path was refused before it reached the filesystem.
    ///
    /// Both fields are safe to show: `path` is the model's own string, and
    /// `reason` is one of a fixed set of English phrases naming the rule that
    /// fired.
    #[error("the path `{path}` was refused: {reason}")]
    Rejected {
        path: String,
        reason: String,
        #[source]
        source: WorkspaceError,
    },

    /// The check did not finish inside its bound and was killed.
    ///
    /// Deliberately anonymous. Naming the program would tell the model which
    /// command the host chose to run, which is exactly the fact `run_check`
    /// exists to keep out of its hands.
    #[error("the check did not finish within its time limit")]
    Timeout {
        #[source]
        source: WorkspaceError,
    },

    /// The operation was permitted but did not succeed.
    #[error("{operation} did not succeed")]
    Failed {
        operation: &'static str,
        #[source]
        source: WorkspaceError,
    },
}

impl ToolError {
    /// Turn a workspace failure into one a model may be shown.
    ///
    /// The interesting arm is the last one. [`WorkspaceError::Io`] renders the
    /// *resolved absolute* path and [`WorkspaceError::Git`] carries git's stderr
    /// verbatim, and both of those describe the operator's machine; they are
    /// collapsed into a single `operation did not succeed`, with the original
    /// kept as the source for whoever is reading logs rather than prompts.
    fn from_workspace(operation: &'static str, source: WorkspaceError) -> Self {
        match source {
            WorkspaceError::Escape {
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

    /// Classify this failure for Rig, keeping the message model-visible.
    ///
    /// Rig's default [`Tool::map_error`] redacts the message on the assumption
    /// that a domain error may be carrying secrets. These messages are written
    /// not to, so they are published deliberately: a refusal a model cannot read
    /// is a turn spent learning nothing, and the budget is finite.
    fn into_execution_error(self) -> ToolExecutionError {
        let message = self.to_string();
        let classified = match &self {
            // `refused` rather than `other` so a telemetry reader can tell a
            // deliberate decline from a fault. The model is told only that the
            // tool will not act; there is nothing it could do with more.
            ToolError::NoHostContext => ToolExecutionError::refused(message),
            ToolError::Cancelled => ToolExecutionError::cancelled(message),
            ToolError::Rejected { .. } => ToolExecutionError::invalid_args(message),
            ToolError::Timeout { .. } => ToolExecutionError::timeout(message),
            ToolError::Failed { .. } => ToolExecutionError::other(message),
        };
        // The whole chain, including the workspace error that names the resolved
        // path, is kept downcastable for the operator. It is not the model-
        // visible output, which is `message` and only `message`.
        classified.with_source(self)
    }
}

/// Arguments for a tool that takes none.
///
/// A named type rather than `()` because Rig deserializes arguments from the
/// provider's JSON object, and because the *absence* of fields is the security
/// property: unknown keys are ignored by serde, so a model that invents
/// `{"program": "..."}` has its invention dropped on the floor rather than
/// honoured or turned into an error it can probe.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct NoArgs {}

/// The schema of a tool that takes no arguments.
///
/// `"properties": {}` is written explicitly rather than left out. An absent
/// `properties` key is a schema that says nothing about what may be passed;
/// an empty one says there is nothing to pass, which is the claim being made.
fn no_parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "additionalProperties": false
    })
}

/// Read one file from the project under repair.
pub struct ReadFile;

/// The path to read, relative to the project.
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
        host.guard()?;
        let path = parse(&args.path)?;
        host.workspace
            .read(&path)
            .map_err(|source| ToolError::from_workspace("reading the file", source))
    }

    fn map_error(&self, error: Self::Error) -> ToolExecutionError {
        error.into_execution_error()
    }
}

/// Replace the contents of one file in the project under repair.
pub struct WriteFile;

/// The path to write and what to put in it.
#[derive(Clone, Debug, Deserialize)]
pub struct WriteFileArgs {
    pub path: String,
    pub contents: String,
}

/// What a write did, in terms the model can check.
#[derive(Clone, Debug, Serialize)]
pub struct WriteReceipt {
    /// The normalised relative path, echoing back what was actually written to.
    pub path: String,
    pub bytes: usize,
}

impl Tool for WriteFile {
    const NAME: &'static str = "write_file";
    type Args = WriteFileArgs;
    type Output = WriteReceipt;
    type Error = ToolError;

    fn description(&self) -> String {
        "Replace one file in the project you are repairing with the contents you supply.".into()
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
        // Before the parse and before the write, both. A guard placed after
        // either would let a cancelled attempt leave a file behind.
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

    fn map_error(&self, error: Self::Error) -> ToolExecutionError {
        error.into_execution_error()
    }
}

/// Name every file in the project under repair.
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

    fn map_error(&self, error: Self::Error) -> ToolExecutionError {
        error.into_execution_error()
    }
}

/// Run the host's check over the project under repair.
pub struct RunCheck;

/// What the check said.
///
/// A non-zero `exit_code` is a result rather than an error, for the same reason
/// [`crate::workspace::CommandResult`] treats it that way: a failing check is
/// the observation the repair loop exists to consume.
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

    /// No parameters, and that is the security property rather than a
    /// simplification. A tool that took the program to run would be arbitrary
    /// code execution wearing a tool's name; which command constitutes "the
    /// check" is a host decision, and it arrives through [`ToolHost::check`].
    fn parameters(&self) -> serde_json::Value {
        no_parameters()
    }

    async fn call(&self, ctx: &mut ToolContext, _args: NoArgs) -> Result<CheckOutcome, ToolError> {
        let host = ToolHost::from_context(ctx)?;
        host.guard()?;
        let result = host
            .workspace
            .run(&host.check)
            .await
            .map_err(|source| ToolError::from_workspace("running the check", source))?;
        let root = host.workspace.root();
        Ok(CheckOutcome {
            exit_code: result.exit_code,
            stdout: relativised(&result.stdout, root),
            stderr: relativised(&result.stderr, root),
        })
    }

    fn map_error(&self, error: Self::Error) -> ToolExecutionError {
        error.into_execution_error()
    }
}

/// Parse a requested path, mapping the refusal into a model-visible one.
fn parse(raw: &str) -> Result<WorkspacePath, ToolError> {
    WorkspacePath::parse(raw)
        .map_err(|source| ToolError::from_workspace("reading the path", source))
}

/// Rewrite the workspace's absolute path out of a child process's output.
///
/// Check runners announce where they are working — `cargo` prints
/// `Compiling foo v0.1.0 (/…/ws/<attempt>)` on every build — so returning the
/// output verbatim would hand the model the operator's directory layout on the
/// first call, without anybody deciding to. Rewriting the prefix to `.` costs
/// nothing diagnostically and gains something: what is left is the relative path
/// the model can pass straight back to `read_file`.
///
/// Both spellings of the root are rewritten, and the canonical one first.
/// macOS's temporary directories live under `/var`, which is a symlink to
/// `/private/var`, so a child resolving its own working directory reports a path
/// that is not the string the workspace was created with — and stripping only
/// the string it was created with would strip nothing at all.
///
/// This is a prefix, not a redactor. A child is free to print an absolute path
/// of its own choosing — a toolchain in the Nix store, a registry checkout in
/// `~/.cargo` — and nothing here can stop it; what it cannot do is reveal where
/// this attempt is working.
fn relativised(text: &str, root: &Path) -> String {
    let mut spellings = Vec::new();
    if let Ok(canonical) = root.canonicalize() {
        spellings.push(canonical.display().to_string());
    }
    spellings.push(root.display().to_string());
    spellings.sort_by_key(|spelling| std::cmp::Reverse(spelling.len()));

    let mut text = text.to_string();
    for spelling in spellings {
        if !spelling.is_empty() {
            text = text.replace(&spelling, ".");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::ToolReceipts;
    use crate::workspace::{Workspace, WorkspaceCommand};
    use fiddle_core::AttemptId;
    use rig_agent::tool::{Tool, ToolContext};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;

    /// A host context over a throwaway one-commit repository.
    ///
    /// The `TempDir` comes back with it because dropping it would take the
    /// workspace with it; a test that let it fall would be reading a tree that
    /// no longer exists.
    fn test_host() -> (ToolHost, tempfile::TempDir) {
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
        let mut ctx = ToolContext::new(); // deliberately empty
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
        // An `Err` alone would be satisfied by a tool that wrote the file and
        // then reported a problem. The refusal has to have happened before the
        // filesystem was touched, and this is the only way to see that from
        // outside.
        assert!(
            !outside.exists(),
            "the refusal came after the write: {}",
            outside.display()
        );
    }

    #[tokio::test]
    async fn cancellation_between_inspection_and_mutation_prevents_the_write() {
        // The interleaving that matters: the agent has already read, and the
        // token is cancelled before it writes. The mutation must not happen.
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
        // RunCheckArgs has no fields at all: a model-chosen command would be
        // arbitrary code execution wearing a tool's name.
        assert_eq!(RunCheck.parameters()["properties"], serde_json::json!({}));
    }

    #[tokio::test]
    async fn a_model_supplied_program_is_ignored_and_the_host_command_runs() {
        // The schema having no properties is a claim made to the provider. This
        // is the claim made to the code: arguments that name a program arrive,
        // deserialize, and change nothing about what executes.
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
        // A check runner announces where it is working — `cargo` prints the
        // package's absolute directory on every `Compiling` line — so a result
        // returned verbatim hands the model the operator's filesystem without
        // anybody having decided to. `--show-toplevel` is that behaviour in one
        // deterministic line.
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
        // The generalisation of the schema assertion above: the *values* the
        // host holds are what must not appear, and they must not appear in the
        // description either, which is model-visible too.
        let (host, _g) = test_host();
        let root = host.workspace.root().display().to_string();
        let surfaces = [
            (ReadFile.parameters().to_string(), ReadFile.description()),
            (WriteFile.parameters().to_string(), WriteFile.description()),
            (ListFiles.parameters().to_string(), ListFiles.description()),
            (RunCheck.parameters().to_string(), RunCheck.description()),
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
        // `Workspace` reports IO failures against the *resolved absolute* path,
        // which names the operator's filesystem. That diagnostic is for the
        // operator; what goes back to the model is a different string.
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

    /// Every spelling of the workspace root this platform might produce.
    ///
    /// macOS hands out temporary directories under `/var`, which is itself a
    /// symlink to `/private/var`, so a leak can wear either spelling and a check
    /// against only one of them would miss half of them.
    fn roots(host: &ToolHost) -> Vec<String> {
        let root = host.workspace.root();
        let mut spellings = vec![root.display().to_string()];
        if let Ok(canonical) = root.canonicalize() {
            spellings.push(canonical.display().to_string());
        }
        spellings
    }
}
