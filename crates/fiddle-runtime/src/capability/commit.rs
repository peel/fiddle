use super::CapabilityError;
use crate::effect::CommitAuthor;
use crate::workspace::{Workspace, WorkspaceCommand, WorkspaceError, WorkspacePath};
use std::time::Duration;

const FIDDLE_NAME: &str = "fiddle";

const FIDDLE_EMAIL: &str = "fiddle@invalid";

const HOOKS_DISABLED: &str = "core.hooksPath=/dev/null";

pub(super) fn committer(author: Option<&CommitAuthor>) -> Vec<String> {
    let (name, email) = match author {
        Some(author) => (author.name.as_str(), author.email.as_str()),
        None => (FIDDLE_NAME, FIDDLE_EMAIL),
    };
    [
        format!("user.name={name}"),
        format!("user.email={email}"),
        HOOKS_DISABLED.to_string(),
    ]
    .into_iter()
    .flat_map(|setting| ["-c".to_string(), setting])
    .collect()
}

pub(super) fn message(project: &str, invocation_ref: &str) -> String {
    format!("{project}: {invocation_ref}")
}

pub(super) async fn commit_changed(
    workspace: &Workspace,
    changed: &[WorkspacePath],
    message: &str,
    author: Option<&CommitAuthor>,
    timeout: Duration,
) -> Result<String, CapabilityError> {
    commit_described(workspace, changed, message, None, None, author, timeout).await
}

pub(super) async fn commit_described(
    workspace: &Workspace,
    changed: &[WorkspacePath],
    subject: &str,
    body: Option<&str>,
    dated: Option<&str>,
    author: Option<&CommitAuthor>,
    timeout: Duration,
) -> Result<String, CapabilityError> {
    let mut add = vec!["add".to_string(), "-f".to_string(), "--".to_string()];
    add.extend(changed.iter().map(|path| path.as_str().to_string()));
    run(workspace, add, timeout).await?;

    let mut commit = committer(author);
    commit.extend([
        "commit".to_string(),
        "-q".to_string(),
        "-m".to_string(),
        subject.to_string(),
    ]);
    if let Some(body) = body.map(str::trim).filter(|body| !body.is_empty()) {
        commit.extend(["-m".to_string(), body.to_string()]);
    }
    run_dated(workspace, commit, dated, timeout).await?;

    Ok(run(
        workspace,
        vec!["rev-parse".to_string(), "HEAD".to_string()],
        timeout,
    )
    .await?
    .trim()
    .to_string())
}

pub(super) async fn run(
    workspace: &Workspace,
    args: Vec<String>,
    timeout: Duration,
) -> Result<String, CapabilityError> {
    run_dated(workspace, args, None, timeout).await
}

async fn run_dated(
    workspace: &Workspace,
    args: Vec<String>,
    dated: Option<&str>,
    timeout: Duration,
) -> Result<String, CapabilityError> {
    let command = WorkspaceCommand {
        program: "git".to_string(),
        args: args.clone(),
        timeout,
    };
    let result = workspace.run_dated(&command, dated).await?;
    match result.exit_code {
        0 => Ok(result.stdout),
        _ => Err(CapabilityError::Workspace(WorkspaceError::Git {
            command: args.join(" "),
            stderr: result.stderr,
        })),
    }
}
