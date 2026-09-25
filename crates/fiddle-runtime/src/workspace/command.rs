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

pub const NOT_ON_PATH: &str = "it is not on the PATH this deployment gives its commands";

pub const NOT_EXECUTABLE: &str = "it is not an executable file";

pub const NOT_THERE: &str = "no file is at that path";

pub fn tool_path() -> &'static str {
    &TOOL_PATH
}

fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

const RUSTUP_HOME: &str = "RUSTUP_HOME";

const GIT_AUTHOR_DATE: &str = "GIT_AUTHOR_DATE";

const GIT_COMMITTER_DATE: &str = "GIT_COMMITTER_DATE";

impl Workspace {
    pub fn locate(&self, program: &str) -> Result<PathBuf, WorkspaceError> {
        let unstartable = |why| WorkspaceError::Unstartable {
            program: program.to_string(),
            why,
        };
        if program.contains('/') {
            let path = self.root.join(program);
            return match executable(&path) {
                true => Ok(path),
                false if path.exists() => Err(unstartable(NOT_EXECUTABLE)),
                false => Err(unstartable(NOT_THERE)),
            };
        }
        tool_path()
            .split(':')
            .filter(|dir| !dir.is_empty())
            .map(|dir| Path::new(dir).join(program))
            .find(|path| executable(path))
            .ok_or_else(|| unstartable(NOT_ON_PATH))
    }

    pub async fn run(&self, cmd: &WorkspaceCommand) -> Result<CommandResult, WorkspaceError> {
        if self.cancel.is_cancelled() {
            return Err(WorkspaceError::Cancelled);
        }
        self.locate(&cmd.program)?;

        let mut command = tokio::process::Command::new(&cmd.program);
        command
            .args(&cmd.args)
            .current_dir(&self.root)
            .env_clear()
            .env("HOME", self.home())
            .env("PATH", &*TOOL_PATH)
            .env("LANG", "C")
            .env(GIT_AUTHOR_DATE, self.stamp())
            .env(GIT_COMMITTER_DATE, self.stamp());
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
