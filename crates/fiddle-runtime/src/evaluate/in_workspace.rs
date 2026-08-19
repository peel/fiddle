use super::{Answered, Check, Tree, Unanswered};
use crate::scanner::{ScanError, ScanReport, Scanner, WizCredential, Wizcli};
use crate::workspace::{Workspace, WorkspaceCommand, WorkspaceError};
use async_trait::async_trait;
use std::path::PathBuf;
use std::time::Duration;

pub struct Rescan {
    pub scratch: PathBuf,

    pub credential: WizCredential,

    pub image: String,
}

pub struct InWorkspace<'a> {
    workspace: &'a Workspace,
    timeout: Duration,
    rescan: Rescan,
}

impl<'a> InWorkspace<'a> {
    pub fn new(workspace: &'a Workspace, timeout: Duration, rescan: Rescan) -> Self {
        Self {
            workspace,
            timeout,
            rescan,
        }
    }
}

#[async_trait]
impl Tree for InWorkspace<'_> {
    async fn run(&self, check: &Check) -> Result<Answered, Unanswered> {
        let command = WorkspaceCommand {
            program: check.program.clone(),
            args: check.args.clone(),
            timeout: self.timeout,
        };
        match self.workspace.run(&command).await {
            Ok(result) => Ok(Answered {
                exit_code: result.exit_code,
                stdout: result.stdout,
                stderr: result.stderr,
            }),

            Err(WorkspaceError::Cancelled) => Err(Unanswered::Cancelled),

            Err(WorkspaceError::Timeout { program, timeout }) => {
                Err(Unanswered::TimedOut { program, timeout })
            }

            Err(WorkspaceError::Io { source, .. }) => Err(Unanswered::NotStarted {
                program: check.program.clone(),
                source,
            }),

            Err(unreachable) => Err(Unanswered::NotStarted {
                program: check.program.clone(),
                source: std::io::Error::other(unreachable.to_string()),
            }),
        }
    }

    async fn scan(&self, check: &Check) -> Result<ScanReport, ScanError> {
        if self.workspace.cancel().is_cancelled() {
            return Err(ScanError::Failed {
                status: "the attempt was cancelled before the scanner started".to_string(),
                stderr: String::new(),
            });
        }

        Wizcli::new(
            PathBuf::from(&check.program),
            check.args.clone(),
            self.rescan.scratch.clone(),
            self.timeout,
            self.workspace.cancel().clone(),
            self.rescan.credential.clone(),
        )
        .scan(&self.rescan.image)
        .await
    }
}
