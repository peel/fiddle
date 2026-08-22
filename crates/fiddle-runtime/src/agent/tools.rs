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
        | Err(ToolError::EditRefused { .. })
        | Err(ToolError::ReadRefused { .. })
        | Err(ToolError::ListingRefused { .. })
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

    #[error("the edit to `{path}` was refused: {reason}")]
    EditRefused { path: String, reason: String },

    #[error("the read of `{path}` was refused: {reason}")]
    ReadRefused { path: String, reason: String },

    #[error("the listing was refused: {reason}")]
    ListingRefused { reason: String },

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
            ToolError::Rejected { .. }
            | ToolError::EditRefused { .. }
            | ToolError::ReadRefused { .. }
            | ToolError::ListingRefused { .. }
            | ToolError::Undeclared { .. } => ToolExecutionError::invalid_args(message),
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

pub const RESULT_CAP_BYTES: usize = 16 * 1024;

pub const STREAM_CAP_BYTES: usize = RESULT_CAP_BYTES / 2;

pub const NOTE_ALLOWANCE_BYTES: usize = 512;

pub struct ReadFile;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReadFileArgs {
    pub path: String,

    #[serde(default)]
    pub offset: Option<usize>,

    #[serde(default)]
    pub limit: Option<usize>,
}

impl Tool for ReadFile {
    const NAME: &'static str = "read_file";
    type Args = ReadFileArgs;
    type Output = String;
    type Error = ToolError;

    fn description(&self) -> String {
        format!(
            "Read one file from the project you are repairing, by its relative path. \
             One read gives you at most {RESULT_CAP_BYTES} bytes of it. Give `offset` to \
             start at a later line, and `limit` to take fewer lines. A read that gives \
             you part of a file opens with a note in square brackets. The note counts \
             the lines it withheld and names the offset to read on from. The note is \
             not part of the file."
        )
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Relative path of the file, for example src/lib.rs."
                },
                "offset": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "The first line to read. The first line of a file is 1. \
                                    Leave it out to start at the first line."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "How many lines to read. Leave it out to read to the end \
                                    of the file, or to the byte limit, whichever comes first."
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
            let held = host
                .workspace
                .read(&path)
                .map_err(|source| ToolError::from_workspace("reading the file", source))?;
            page_of_lines(
                &held,
                args.offset.unwrap_or(1),
                args.limit,
                RESULT_CAP_BYTES,
            )
            .map_err(|reason| ToolError::ReadRefused {
                path: path.as_str().to_string(),
                reason,
            })
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

pub struct EditFile;

#[derive(Clone, Debug, Deserialize)]
pub struct EditFileArgs {
    pub path: String,
    pub find: String,
    pub replace: String,
}

impl Tool for EditFile {
    const NAME: &'static str = "edit_file";
    type Args = EditFileArgs;
    type Output = WriteReceipt;
    type Error = ToolError;

    fn description(&self) -> String {
        "Change part of one file in the project you are repairing. Give the text \
         to find and the text to put in its place. Every other line of the file \
         stays as it is. The text to find must occur one time in the file. If it \
         is absent, or if it occurs more than one time, this tool changes nothing \
         and tells you which of the two happened; add the lines above and below \
         it until it occurs one time. Use this tool to change a file that already \
         exists. Use `write_file` to create a file, and to replace a short file \
         whole."
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
                "find": {
                    "type": "string",
                    "description": "The text to find, copied from the file exactly as it reads there."
                },
                "replace": {
                    "type": "string",
                    "description": "The text to put in its place. An empty string deletes the text you found."
                }
            },
            "required": ["path", "find", "replace"],
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
            let refuse = |reason: String| ToolError::EditRefused {
                path: path.as_str().to_string(),
                reason,
            };
            if args.find.is_empty() {
                return Err(refuse("the text to find is empty".to_string()));
            }
            let held = host
                .workspace
                .read(&path)
                .map_err(|source| ToolError::from_workspace("reading the file", source))?;
            match held.matches(args.find.as_str()).count() {
                1 => {}
                0 => {
                    return Err(refuse(
                        "the text to find does not occur in the file".to_string(),
                    ))
                }
                occurrences => {
                    return Err(refuse(format!(
                        "the text to find is not unique: it occurs {occurrences} times"
                    )))
                }
            }
            let edited = held.replacen(args.find.as_str(), &args.replace, 1);
            host.workspace
                .write(&path, &edited)
                .map_err(|source| ToolError::from_workspace("writing the file", source))?;
            Ok(WriteReceipt {
                path: path.as_str().to_string(),
                bytes: edited.len(),
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

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ListFilesArgs {
    #[serde(default)]
    pub offset: Option<usize>,

    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Listing {
    pub paths: Vec<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub withheld: Option<String>,
}

impl Tool for ListFiles {
    const NAME: &'static str = "list_files";
    type Args = ListFilesArgs;
    type Output = Listing;
    type Error = ToolError;

    fn description(&self) -> String {
        format!(
            "List the files of the project you are repairing, as relative paths. One \
             listing gives you at most {RESULT_CAP_BYTES} bytes of paths. Give `offset` \
             to start at a later path, and `limit` to take fewer paths. A listing that \
             gives you part of the project carries a `withheld` sentence. That sentence \
             counts the paths it withheld and names the offset to list on from."
        )
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "offset": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "The first path to list. The first path is 1. Leave it \
                                    out to start at the first path."
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "How many paths to list. Leave it out to list to the end, \
                                    or to the byte limit, whichever comes first."
                }
            },
            "required": [],
            "additionalProperties": false
        })
    }

    async fn call(&self, ctx: &mut ToolContext, args: ListFilesArgs) -> Result<Listing, ToolError> {
        let host = ToolHost::from_context(ctx)?;
        let started = Instant::now();
        let result = async {
            host.guard()?;
            let listed = host
                .workspace
                .list()
                .map_err(|source| ToolError::from_workspace("listing the files", source))?;
            let paths: Vec<String> = listed
                .into_iter()
                .map(|path| path.as_str().to_string())
                .collect();
            page_of_paths(
                paths,
                args.offset.unwrap_or(1),
                args.limit,
                RESULT_CAP_BYTES,
            )
            .map_err(|reason| ToolError::ListingRefused { reason })
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
                stdout: head_and_tail(&result.stdout, STREAM_CAP_BYTES),
                stderr: head_and_tail(&result.stderr, STREAM_CAP_BYTES),
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
                stdout: head_and_tail(&result.stdout, STREAM_CAP_BYTES),
                stderr: head_and_tail(&result.stderr, STREAM_CAP_BYTES),
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

fn cut_to_char_boundary(text: &str, cap: usize) -> &str {
    let mut end = cap.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

fn page_of_lines(
    held: &str,
    offset: usize,
    limit: Option<usize>,
    cap: usize,
) -> Result<String, String> {
    if offset == 0 {
        return Err("the lines of a file are numbered from 1, so there is no line 0".to_string());
    }
    if limit == Some(0) {
        return Err("a limit of 0 lines returns no line; ask for 1 line or more".to_string());
    }
    if held.is_empty() {
        if offset == 1 {
            return Ok(String::new());
        }
        return Err(format!("the file is empty, so it has no line {offset}"));
    }

    let lines: Vec<&str> = held.split_inclusive('\n').collect();
    let total = lines.len();
    if offset > total {
        return Err(format!(
            "the file has {total} lines, so it has no line {offset}"
        ));
    }
    let start = offset - 1;
    let end = match limit {
        Some(limit) => start.saturating_add(limit).min(total),
        None => total,
    };
    let wanted = &lines[start..end];

    let mut taken = 0;
    let mut bytes = 0;
    for line in wanted {
        if bytes + line.len() > cap {
            break;
        }
        bytes += line.len();
        taken += 1;
    }

    if taken == 0 {
        let part = cut_to_char_boundary(wanted[0], cap);
        let withheld = wanted[0].len() - part.len();
        return Ok(format!(
            "[read_file gave you the first {} bytes of line {offset}, in a file of {total} \
             lines. It withheld {withheld} bytes of that one line. A line longer than \
             {cap} bytes does not fit in one read, and read_file reaches no further into \
             it. This note is not part of the file.]\n{part}",
            part.len()
        ));
    }

    let part: String = wanted[..taken].concat();
    if offset == 1 && taken == total {
        return Ok(part);
    }
    let last = offset + taken - 1;
    let withheld = total - taken;
    let mut note = format!(
        "[read_file gave you lines {offset} to {last} of {total}. It withheld {withheld} \
         lines. This note is not part of the file."
    );
    if last < total {
        let next = last + 1;
        note.push_str(&format!(" Call read_file with offset {next} to read on."));
    }
    note.push_str("]\n");
    Ok(format!("{note}{part}"))
}

fn page_of_paths(
    paths: Vec<String>,
    offset: usize,
    limit: Option<usize>,
    cap: usize,
) -> Result<Listing, String> {
    if offset == 0 {
        return Err(
            "the paths of a project are numbered from 1, so there is no path 0".to_string(),
        );
    }
    if limit == Some(0) {
        return Err("a limit of 0 paths returns no path; ask for 1 path or more".to_string());
    }
    let total = paths.len();
    if total == 0 {
        return Ok(Listing {
            paths,
            withheld: None,
        });
    }
    if offset > total {
        return Err(format!(
            "the project holds {total} files, so it has no path {offset}"
        ));
    }
    let start = offset - 1;
    let end = match limit {
        Some(limit) => start.saturating_add(limit).min(total),
        None => total,
    };

    let mut taken: Vec<String> = Vec::new();
    let mut bytes = 0;
    for path in &paths[start..end] {
        if !taken.is_empty() && bytes + path.len() > cap {
            break;
        }
        bytes += path.len();
        taken.push(path.clone());
    }

    if offset == 1 && taken.len() == total {
        return Ok(Listing {
            paths: taken,
            withheld: None,
        });
    }
    let last = offset + taken.len() - 1;
    let withheld = total - taken.len();
    let mut sentence = format!(
        "list_files gave you paths {offset} to {last} of {total}. It withheld {withheld} paths."
    );
    if last < total {
        let next = last + 1;
        sentence.push_str(&format!(" Call list_files with offset {next} to list on."));
    }
    Ok(Listing {
        paths: taken,
        withheld: Some(sentence),
    })
}

fn head_and_tail(text: &str, cap: usize) -> String {
    if text.len() <= cap {
        return text.to_string();
    }
    let half = cap / 2;
    let head = cut_to_char_boundary(text, half);
    let head = match head.rfind('\n') {
        Some(at) => &head[..at + 1],
        None => head,
    };
    let mut from = text.len() - half;
    while from < text.len() && !text.is_char_boundary(from) {
        from += 1;
    }
    let tail = &text[from..];
    let tail = match tail.find('\n') {
        Some(at) => &tail[at + 1..],
        None => tail,
    };
    let dropped = text[head.len()..text.len() - tail.len()].lines().count();
    let bytes = text.len();
    format!(
        "{head}[this tool dropped {dropped} lines here. The program printed {bytes} bytes \
         and this tool gives you {cap} of them. The lines above are the start of what it \
         printed. The lines below are the end.]\n{tail}"
    )
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

    pub(crate) fn whole(path: &str) -> ReadFileArgs {
        ReadFileArgs {
            path: path.to_string(),
            ..ReadFileArgs::default()
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
            .call(&mut ctx, whole("src/lib.rs"))
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
            ReadFile.call(&mut ctx, whole("src/lib.rs")).await.is_err(),
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

    const A_LOCK: &str = "a v1.0.0\nb v1.2.2\nc v3.0.0\n";

    const THE_SAME_LOCK_TWICE_OVER: &str = "a v1.0.0\nb v1.2.2\nc v3.0.0\nb v1.2.2\n";

    const TARGET: &str = "b v1.2.2";

    const REPLACEMENT: &str = "b v1.2.3";

    fn seeded(host: &ToolHost, path: &str, contents: &str) {
        let at = host.workspace.root().join(path);
        std::fs::create_dir_all(at.parent().expect("a file has a parent")).unwrap();
        std::fs::write(at, contents).unwrap();
    }

    fn an_edit(path: &str) -> EditFileArgs {
        EditFileArgs {
            path: path.to_string(),
            find: TARGET.to_string(),
            replace: REPLACEMENT.to_string(),
        }
    }

    async fn refusal_of(args: EditFileArgs, host: &ToolHost) -> String {
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        let refused = EditFile
            .call(&mut ctx, args)
            .await
            .expect_err("this edit must be refused");
        EditFile
            .map_error(refused)
            .model_output()
            .as_text()
            .unwrap_or_default()
            .to_string()
    }

    #[tokio::test]
    async fn edit_file_changes_the_one_place_the_text_occurs_and_keeps_the_rest() {
        let (host, _g) = test_host();
        seeded(&host, "deps.lock", A_LOCK);
        let root = host.workspace.root().to_path_buf();
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let receipt = EditFile
            .call(&mut ctx, an_edit("deps.lock"))
            .await
            .expect("the text occurs once, so the change it describes is unambiguous");

        assert_eq!(receipt.path, "deps.lock");
        assert_eq!(
            std::fs::read_to_string(root.join("deps.lock")).unwrap(),
            "a v1.0.0\nb v1.2.3\nc v3.0.0\n",
            "an edit changes the text it names, and every other line survives it"
        );
    }

    #[tokio::test]
    async fn edit_file_refuses_the_same_edit_where_the_text_occurs_twice() {
        let (host, _g) = test_host();
        seeded(&host, "deps.lock", THE_SAME_LOCK_TWICE_OVER);
        let root = host.workspace.root().to_path_buf();

        let text = refusal_of(an_edit("deps.lock"), &host).await;
        assert!(
            text.contains("deps.lock"),
            "the model cannot act on a refusal that does not name the file: {text}"
        );
        assert!(
            text.contains("not unique") && text.contains("2 times"),
            "one input separates this from the test above, and the refusal has to \
             say which input it was: {text}"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("deps.lock")).unwrap(),
            THE_SAME_LOCK_TWICE_OVER,
            "a refused edit changes nothing, so neither place is rewritten"
        );
    }

    #[tokio::test]
    async fn edit_file_refuses_text_that_is_absent_rather_than_appending_it() {
        let (host, _g) = test_host();
        seeded(&host, "deps.lock", A_LOCK);
        let root = host.workspace.root().to_path_buf();

        let text = refusal_of(
            EditFileArgs {
                path: "deps.lock".into(),
                find: "d v9.9.9".into(),
                replace: REPLACEMENT.into(),
            },
            &host,
        )
        .await;
        assert!(
            text.contains("does not occur"),
            "a model that guessed the text must learn that it guessed: {text}"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("deps.lock")).unwrap(),
            A_LOCK,
            "text that is absent is not text to add somewhere"
        );
    }

    #[tokio::test]
    async fn edit_file_refuses_an_empty_search_because_it_matches_no_one_place() {
        let (host, _g) = test_host();
        seeded(&host, "deps.lock", A_LOCK);
        let root = host.workspace.root().to_path_buf();

        let text = refusal_of(
            EditFileArgs {
                path: "deps.lock".into(),
                find: String::new(),
                replace: REPLACEMENT.into(),
            },
            &host,
        )
        .await;
        assert!(
            text.contains("empty"),
            "an empty search names every position in the file and none of them: {text}"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("deps.lock")).unwrap(),
            A_LOCK,
            "the refusal came before the write"
        );
    }

    #[tokio::test]
    async fn edit_file_is_bounded_by_the_same_path_rules_as_read_file_and_write_file() {
        let (host, _g) = test_host();
        let outside = host
            .workspace
            .root()
            .parent()
            .expect("the fixture has a parent to escape into")
            .join("escape.txt");
        std::fs::write(&outside, A_LOCK).unwrap();
        seeded(&host, "src/deps.lock", A_LOCK);

        for path in ["../escape.txt", ".git/config", "src/../../escape.txt"] {
            let text = refusal_of(an_edit(path), &host).await;
            assert!(
                text.contains(path),
                "a refusal has to name the path the model wrote: {text}"
            );
            for root in roots(&host) {
                assert!(
                    !text.contains(&root),
                    "a host path leaked in a refusal: {text}"
                );
            }
        }
        assert_eq!(
            std::fs::read_to_string(&outside).unwrap(),
            A_LOCK,
            "a path outside the project stayed as it was: {}",
            outside.display()
        );
    }

    #[tokio::test]
    async fn a_cancelled_attempt_edits_nothing_and_the_call_is_still_recorded() {
        let (host, _g) = test_host();
        seeded(&host, "deps.lock", A_LOCK);
        let root = host.workspace.root().to_path_buf();
        host.cancel.cancel();

        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());
        assert!(EditFile.call(&mut ctx, an_edit("deps.lock")).await.is_err());

        assert_eq!(
            std::fs::read_to_string(root.join("deps.lock")).unwrap(),
            A_LOCK,
            "cancellation must prevent the effect, not merely end the future"
        );
        let receipts = host.receipts();
        assert_eq!(receipts.calls.len(), 1);
        assert_eq!(receipts.calls[0].tool, "edit_file");
        assert_eq!(receipts.calls[0].outcome, "cancelled");
    }

    #[tokio::test]
    async fn an_edit_and_a_refused_edit_are_both_recorded_under_the_tools_name() {
        let (host, _g) = test_host();
        seeded(&host, "deps.lock", A_LOCK);
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        EditFile
            .call(&mut ctx, an_edit("deps.lock"))
            .await
            .expect("the first edit succeeds");
        let _ = EditFile.call(&mut ctx, an_edit("deps.lock")).await;

        let receipts = host.receipts();
        assert_eq!(
            receipts.calls.len(),
            2,
            "the second call found nothing to change, and a refusal is evidence: {receipts:?}"
        );
        assert_eq!(receipts.calls[0].tool, "edit_file");
        assert_eq!(receipts.calls[0].outcome, "ok");
        assert_eq!(receipts.calls[1].tool, "edit_file");
        assert_eq!(receipts.calls[1].outcome, "refused");
    }

    #[tokio::test]
    async fn cancellation_between_inspection_and_mutation_prevents_the_write() {
        let (host, _g) = test_host();
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        ReadFile.call(&mut ctx, whole("src/lib.rs")).await.unwrap();
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

        let listed = ListFiles
            .call(&mut ctx, ListFilesArgs::default())
            .await
            .unwrap();
        assert!(
            listed.paths.contains(&"src/lib.rs".to_string()),
            "{listed:?}"
        );
        assert!(
            !listed.paths.iter().any(|p| p.starts_with("target/")),
            "an ignored build tree would drown the model's context: {listed:?}"
        );
        assert!(
            listed.withheld.is_none(),
            "this project fits in one listing, so nothing was withheld: {listed:?}"
        );
    }

    fn numbered(line: usize) -> String {
        format!("{line:0>39}\n")
    }

    fn lines_of(count: usize) -> String {
        (1..=count).map(numbered).collect()
    }

    const LINE_BYTES: usize = 40;

    #[tokio::test]
    async fn a_read_inside_the_limit_gives_the_whole_file_and_withholds_nothing() {
        let (host, _g) = test_host();
        let held = lines_of(100);
        assert!(held.len() < RESULT_CAP_BYTES);
        std::fs::write(host.workspace.root().join("short.txt"), &held).unwrap();
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let got = ReadFile.call(&mut ctx, whole("short.txt")).await.unwrap();

        assert_eq!(got, held, "a file under the limit reaches the model whole");
        assert!(
            !got.contains("withheld"),
            "a whole read must claim no withholding"
        );
    }

    #[tokio::test]
    async fn the_same_read_beyond_the_limit_gives_part_and_counts_the_lines_it_withheld() {
        let (host, _g) = test_host();
        let total = 2_000;
        let held = lines_of(total);
        assert!(
            held.len() > RESULT_CAP_BYTES,
            "the fixture must exceed the limit, or this test asserts nothing: {}",
            held.len()
        );
        std::fs::write(host.workspace.root().join("long.txt"), &held).unwrap();
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let got = ReadFile.call(&mut ctx, whole("long.txt")).await.unwrap();

        let given = RESULT_CAP_BYTES / LINE_BYTES;
        let note = got
            .lines()
            .next()
            .expect("a partial read opens with a note");
        assert!(
            note.contains(&format!("gave you lines 1 to {given} of {total}")),
            "the note must say which lines the model holds: {note}"
        );
        assert!(
            note.contains(&format!("It withheld {} lines", total - given)),
            "the note must count what the model does not hold: {note}"
        );
        assert!(
            note.contains(&format!("offset {}", given + 1)),
            "the note must name the offset that reads on: {note}"
        );
        assert!(
            got.contains(&numbered(given)) && !got.contains(&numbered(given + 1)),
            "the returned text must stop where the note says it stops"
        );
        assert!(
            got.len() <= RESULT_CAP_BYTES + NOTE_ALLOWANCE_BYTES,
            "the read returned {} bytes",
            got.len()
        );
    }

    #[tokio::test]
    async fn an_offset_reaches_the_last_line_of_a_file_no_one_read_can_hold() {
        let (host, _g) = test_host();
        let total = 2_000;
        std::fs::write(host.workspace.root().join("long.txt"), lines_of(total)).unwrap();
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let got = ReadFile
            .call(
                &mut ctx,
                ReadFileArgs {
                    path: "long.txt".to_string(),
                    offset: Some(1_992),
                    limit: None,
                },
            )
            .await
            .unwrap();

        assert!(
            got.contains(&numbered(total)),
            "a limit without an offset would put the last line out of reach"
        );
        let note = got
            .lines()
            .next()
            .expect("a partial read opens with a note");
        assert!(
            note.contains(&format!("gave you lines 1992 to {total} of {total}")),
            "{note}"
        );
        assert!(note.contains("It withheld 1991 lines"), "{note}");
        assert!(
            !note.contains("offset"),
            "nothing follows the last line, so the note must name no next offset: {note}"
        );
    }

    #[tokio::test]
    async fn a_line_longer_than_the_limit_is_cut_and_the_note_counts_the_bytes() {
        let (host, _g) = test_host();
        let over = 5_000;
        let held = format!("{}\n", "x".repeat(RESULT_CAP_BYTES + over));
        std::fs::write(host.workspace.root().join("one.txt"), &held).unwrap();
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let got = ReadFile.call(&mut ctx, whole("one.txt")).await.unwrap();

        let note = got.lines().next().expect("a cut read opens with a note");
        assert!(
            note.contains(&format!("the first {RESULT_CAP_BYTES} bytes of line 1")),
            "a line the tool cut must be counted in bytes, because a line count \
             cannot describe part of one line: {note}"
        );
        assert!(
            note.contains(&format!("It withheld {} bytes of that one line", over + 1)),
            "{note}"
        );
        assert!(
            got.len() <= RESULT_CAP_BYTES + NOTE_ALLOWANCE_BYTES,
            "the read returned {} bytes",
            got.len()
        );
    }

    #[tokio::test]
    async fn a_read_refuses_an_offset_past_the_end_and_names_the_line_count() {
        let (host, _g) = test_host();
        std::fs::write(host.workspace.root().join("short.txt"), lines_of(100)).unwrap();
        let mut ctx = ToolContext::new();
        ctx.insert(host.clone());

        let refused = ReadFile
            .call(
                &mut ctx,
                ReadFileArgs {
                    path: "short.txt".to_string(),
                    offset: Some(101),
                    limit: None,
                },
            )
            .await
            .unwrap_err();

        let text = refused.to_string();
        assert!(
            text.contains("the file has 100 lines, so it has no line 101"),
            "a read that cannot answer must refuse and say why: {text}"
        );
        let receipts = host.receipts();
        assert_eq!(receipts.calls[0].outcome, "refused", "{receipts:?}");
    }

    #[tokio::test]
    async fn a_read_result_stays_inside_one_bound_however_large_the_file_grows() {
        let (host, _g) = test_host();
        let grown = [1_000usize, 8_000, 40_000];
        for count in grown {
            std::fs::write(
                host.workspace.root().join(format!("grown-{count}.txt")),
                lines_of(count),
            )
            .unwrap();
        }
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        for count in grown {
            let got = ReadFile
                .call(&mut ctx, whole(&format!("grown-{count}.txt")))
                .await
                .unwrap();
            assert!(
                got.len() <= RESULT_CAP_BYTES + NOTE_ALLOWANCE_BYTES,
                "a file of {} bytes put {} bytes into the conversation",
                count * LINE_BYTES,
                got.len()
            );
        }
    }

    #[tokio::test]
    async fn a_listing_beyond_the_limit_gives_part_and_counts_the_paths_it_withheld() {
        let (host, _g) = test_host();
        let added = 1_000;
        for n in 0..added {
            std::fs::write(host.workspace.root().join(format!("f{n:0>30}.txt")), "x").unwrap();
        }
        let total = added + 2;
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let listed = ListFiles
            .call(&mut ctx, ListFilesArgs::default())
            .await
            .unwrap();

        let withheld = listed
            .withheld
            .as_deref()
            .expect("a listing that holds part of the project must say so");
        assert!(
            withheld.contains(&format!("of {total}")),
            "the sentence must name the count the project holds: {withheld}"
        );
        assert!(
            withheld.contains(&format!("It withheld {} paths", total - listed.paths.len())),
            "{withheld}"
        );
        let bytes: usize = listed.paths.iter().map(String::len).sum();
        assert!(
            bytes <= RESULT_CAP_BYTES,
            "the listing put {bytes} bytes into the conversation"
        );

        let next = listed.paths.len() + 1;
        let rest = ListFiles
            .call(
                &mut ctx,
                ListFilesArgs {
                    offset: Some(next),
                    limit: None,
                },
            )
            .await
            .unwrap();
        assert!(
            !rest.paths.is_empty() && !listed.paths.contains(&rest.paths[0]),
            "the offset the sentence named must reach the paths it withheld"
        );
    }

    #[tokio::test]
    async fn a_check_that_prints_past_the_limit_keeps_its_start_and_its_end() {
        let (mut host, _g) = test_host();
        let printed = 4_000;
        host.check = WorkspaceCommand {
            program: "sh".to_string(),
            args: vec![
                "-c".to_string(),
                format!("i=1; while [ $i -le {printed} ]; do echo \"line $i\"; i=$((i+1)); done"),
            ],
            timeout: Duration::from_secs(60),
        };
        let mut ctx = ToolContext::new();
        ctx.insert(host);

        let outcome = RunCheck.call(&mut ctx, NoArgs::default()).await.unwrap();

        assert!(
            outcome.stdout.len() <= STREAM_CAP_BYTES + NOTE_ALLOWANCE_BYTES,
            "the check put {} bytes into the conversation",
            outcome.stdout.len()
        );
        assert!(
            outcome.stdout.starts_with("line 1\n"),
            "the first error a check prints must survive the cut"
        );
        assert!(
            outcome
                .stdout
                .trim_end()
                .ends_with(&format!("line {printed}")),
            "the summary a check prints last must survive the cut"
        );
        let note = outcome
            .stdout
            .lines()
            .find(|line| line.starts_with("[this tool dropped"))
            .expect("a cut stream must say it was cut");
        let dropped: usize = note
            .split_whitespace()
            .nth(3)
            .and_then(|count| count.parse().ok())
            .unwrap_or_else(|| panic!("the note must count the lines it dropped: {note}"));
        let kept = outcome.stdout.lines().count() - 1;
        assert_eq!(
            kept + dropped,
            printed,
            "the note must account for every line the program printed: {note}"
        );
    }

    #[tokio::test]
    async fn no_tool_advertises_a_host_fact() {
        let (host, _g) = test_host();
        let root = host.workspace.root().display().to_string();
        let surfaces = [
            (ReadFile.parameters().to_string(), ReadFile.description()),
            (EditFile.parameters().to_string(), EditFile.description()),
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
            .call(&mut ctx, whole("src/absent.rs"))
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

        ReadFile.call(&mut ctx, whole("src/lib.rs")).await.unwrap();
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

        assert!(ReadFile.call(&mut ctx, whole("src/lib.rs")).await.is_err());

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
                        ReadFile.call(&mut ctx, whole("src/lib.rs")).await.unwrap();
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

        ReadFile.call(&mut ctx, whole("src/lib.rs")).await.unwrap();
        let _ = ReadFile.call(&mut ctx, whole("src/absent.rs")).await;

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
            let result = ReadFile.call(&mut ctx, whole(path)).await;
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
            .call(&mut ctx, whole("src/lib.rs"))
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
            text.contains("`curl` is not a program"),
            "a refusal has to say that the program is undeclared, and name it; a \
             refusal about its arguments would be a different answer: {text}"
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
            .expect_err("an interpreter would make the declared list unreadable");
        let text = RunCommand
            .map_error(refused)
            .model_output()
            .as_text()
            .unwrap_or_default()
            .to_string();
        assert!(
            text.contains("`/bin/sh` is not a program"),
            "a shell is refused for one reason — no declaration names it — and \
             the refusal must say that rather than complain about `-c`: {text}"
        );
        assert!(
            !root.join("reached.txt").exists(),
            "the refusal came after the interpreter ran"
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
