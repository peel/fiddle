use super::CapabilityError;
use crate::workspace::{Workspace, WorkspaceCommand, WorkspaceError, WorkspacePath};
use std::time::Duration;

pub(super) const COMMITTER: [&str; 2] = ["user.name=fiddle", "user.email=fiddle@invalid"];

pub(super) fn message(project: &str, invocation_ref: &str) -> String {
    format!("{project}: {invocation_ref}")
}

pub(super) async fn commit_changed(
    workspace: &Workspace,
    changed: &[WorkspacePath],
    message: &str,
    timeout: Duration,
) -> Result<String, CapabilityError> {
    let mut add = vec!["add".to_string(), "-f".to_string(), "--".to_string()];
    add.extend(changed.iter().map(|path| path.as_str().to_string()));
    run(workspace, add, timeout).await?;

    let mut commit: Vec<String> = COMMITTER
        .iter()
        .flat_map(|setting| ["-c".to_string(), (*setting).to_string()])
        .collect();
    commit.extend([
        "commit".to_string(),
        "-q".to_string(),
        "-m".to_string(),
        message.to_string(),
    ]);
    run(workspace, commit, timeout).await?;

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
    let command = WorkspaceCommand {
        program: "git".to_string(),
        args: args.clone(),
        timeout,
    };
    let result = workspace.run(&command).await?;
    match result.exit_code {
        0 => Ok(result.stdout),
        _ => Err(CapabilityError::Workspace(WorkspaceError::Git {
            command: args.join(" "),
            stderr: result.stderr,
        })),
    }
}
