use super::{ScanError, ScanReport, Scanner};
use crate::process::{run_bounded, Bounded};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const REPORT_FILE: &str = "scan.json";

const CONFIG_DIR: &str = "wiz-config";

pub const REDACTED: &str = "[redacted]";

const MINIMUM_PATH: &str = "/usr/bin:/bin";

#[derive(Clone, Debug)]
pub struct WizCredential {
    pub client_id: String,
    pub client_secret: String,
}

pub struct Wizcli {
    program: PathBuf,
    args: Vec<String>,
    scratch: PathBuf,
    timeout: Duration,
    cancel: CancellationToken,
    credential: WizCredential,
}

impl Wizcli {
    pub fn new(
        program: PathBuf,
        args: Vec<String>,
        scratch: PathBuf,
        timeout: Duration,
        cancel: CancellationToken,
        credential: WizCredential,
    ) -> Self {
        Self {
            program,
            args,
            scratch,
            timeout,
            cancel,
            credential,
        }
    }

    fn report_path(&self) -> PathBuf {
        self.scratch.join(REPORT_FILE)
    }

    fn command(&self, image: &str, report: &Path) -> Result<Command, ScanError> {
        let mut command = Command::new(&self.program);

        command.env_clear();
        command.env(
            "PATH",
            std::env::var_os("PATH")
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| MINIMUM_PATH.into()),
        );
        command.env("NO_COLOR", "1");
        self.authenticate(&mut command)?;

        command
            .args(&self.args)
            .arg("--by-policy-hits=DISABLED")
            .arg("--json-output-file")
            .arg(report)
            .arg(image);

        Ok(command)
    }

    fn authenticate(&self, command: &mut Command) -> Result<(), ScanError> {
        let config = self.scratch.join(CONFIG_DIR);
        if let Err(source) = std::fs::create_dir_all(&config) {
            return Err(ScanError::Failed {
                status: format!(
                    "the scanner's configuration directory {} could not be created",
                    config.display()
                ),
                stderr: source.to_string(),
            });
        }

        command.env("WIZ_CLIENT_ID", &self.credential.client_id);
        command.env("WIZ_CLIENT_SECRET", &self.credential.client_secret);
        command.env("WIZ_CONFIG_DIR", &config);
        Ok(())
    }

    fn redact(&self, text: &str) -> String {
        match self.credential.client_secret.is_empty() {
            true => text.to_string(),
            false => text.replace(&self.credential.client_secret, REDACTED),
        }
    }

    fn diagnostic(&self, stderr: &str) -> String {
        snippet(&self.redact(stderr))
    }
}

#[async_trait]
impl Scanner for Wizcli {
    async fn scan(&self, image: &str) -> Result<ScanReport, ScanError> {
        let report = self.report_path();
        if let Err(source) = remove_if_present(&report) {
            return Err(ScanError::Failed {
                status: "the previous report could not be cleared".to_string(),
                stderr: source.to_string(),
            });
        }

        let mut command = self.command(image, &report)?;

        let bounded = run_bounded(&mut command, None, self.timeout, &self.cancel).await;
        let output = match bounded {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(ScanError::Missing {
                    program: self.program.clone(),
                    reason: source.to_string(),
                })
            }
            Err(source) => {
                return Err(ScanError::Failed {
                    status: "the scanner could not be run".to_string(),
                    stderr: source.to_string(),
                })
            }
            Ok(Bounded::TimedOut) => {
                return Err(ScanError::Failed {
                    status: format!("killed after {:?}", self.timeout),
                    stderr: String::new(),
                })
            }
            Ok(Bounded::CancelledAfterSpawn) => {
                return Err(ScanError::Failed {
                    status: "cancelled while the scanner was running".to_string(),
                    stderr: String::new(),
                })
            }
            Ok(Bounded::Finished(output)) => output,
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let image_digest = image_digest(&stdout);

        let raw = match std::fs::read_to_string(&report) {
            Ok(raw) => raw,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(if names_an_unreachable_daemon(&stderr) {
                    ScanError::DaemonUnreachable {
                        stderr: self.diagnostic(&stderr),
                    }
                } else if names_an_absent_image(&stderr) {
                    ScanError::ImageAbsent {
                        image: image.to_string(),
                        stderr: self.diagnostic(&stderr),
                    }
                } else {
                    ScanError::Failed {
                        status: describe(&output.status),
                        stderr: self.diagnostic(&stderr),
                    }
                });
            }
            Err(source) => {
                return Err(ScanError::Unparseable {
                    path: report,
                    reason: source.to_string(),
                })
            }
        };

        if raw.trim().is_empty() {
            return Err(ScanError::NoOutput { path: report });
        }
        let document: serde_json::Value =
            serde_json::from_str(&raw).map_err(|source| ScanError::Unparseable {
                path: report,
                reason: source.to_string(),
            })?;

        Ok(ScanReport {
            scanner_version: scanner_version(&document),
            document,
            image_digest,
        })
    }
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

fn scanner_version(document: &serde_json::Value) -> String {
    document["extraInfo"]["clientVersion"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn image_digest(stdout: &str) -> String {
    stdout
        .split_whitespace()
        .find(|word| word.starts_with("sha256:"))
        .unwrap_or_default()
        .to_string()
}

fn names_an_absent_image(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    ["no such image", "manifest unknown", "not found in registry"]
        .iter()
        .any(|phrase| stderr.contains(phrase))
}

fn names_an_unreachable_daemon(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    [
        "cannot connect to the docker daemon",
        "is the docker daemon running",
        "error during connect",
    ]
    .iter()
    .any(|phrase| stderr.contains(phrase))
}

fn describe(status: &std::process::ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit {code}"),
        None => "killed by a signal".to_string(),
    }
}

fn snippet(text: &str) -> String {
    const LIMIT: usize = 120;
    let text = text.trim();
    match text.char_indices().nth(LIMIT) {
        Some((end, _)) => format!("{:?}…", &text[..end]),
        None => format!("{text:?}"),
    }
}
