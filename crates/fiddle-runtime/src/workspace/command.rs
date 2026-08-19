use super::{Workspace, WorkspaceError};
use crate::process::{run_bounded, Bounded};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct WorkspaceCommand {
    pub program: String,
    pub args: Vec<String>,
    pub timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

static TOOL_PATH: LazyLock<String> = LazyLock::new(|| match std::env::var("PATH") {
    Ok(path) if !path.is_empty() => path,
    _ => MINIMUM_PATH.to_string(),
});

const MINIMUM_PATH: &str = "/usr/bin:/bin";

const RUSTUP_HOME: &str = "RUSTUP_HOME";

impl Workspace {
    pub async fn run(&self, cmd: &WorkspaceCommand) -> Result<CommandResult, WorkspaceError> {
        if self.cancel.is_cancelled() {
            return Err(WorkspaceError::Cancelled);
        }

        let mut command = tokio::process::Command::new(&cmd.program);
        command
            .args(&cmd.args)
            .current_dir(&self.root)
            .env_clear()
            .env("HOME", self.home())
            .env("PATH", &*TOOL_PATH)
            .env("LANG", "C");
        if let Ok(rustup_home) = std::env::var(RUSTUP_HOME) {
            command.env(RUSTUP_HOME, rustup_home);
        }

        let bounded = run_bounded(&mut command, None, cmd.timeout, &self.cancel)
            .await
            .map_err(|source| WorkspaceError::Io {
                path: PathBuf::from(&cmd.program),
                source,
            })?;

        match bounded {
            Bounded::CancelledAfterSpawn => Err(WorkspaceError::Cancelled),
            Bounded::TimedOut => Err(WorkspaceError::Timeout {
                program: cmd.program.clone(),
                timeout: cmd.timeout,
            }),
            Bounded::Finished(out) => Ok(CommandResult {
                exit_code: out.status.code().unwrap_or(-1),
                stdout: relativised(&String::from_utf8_lossy(&out.stdout), &self.root),
                stderr: relativised(&String::from_utf8_lossy(&out.stderr), &self.root),
            }),
        }
    }
}

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
