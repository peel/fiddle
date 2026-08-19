use super::attribute::{Manifest, ModuleGraph, ResolverError};
use crate::process::{run_bounded, Bounded};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const MINIMUM_PATH: &str = "/usr/bin:/bin";

const GO_MOD: &str = "go.mod";
const GO_SUM: &str = "go.sum";

pub struct Go {
    program: PathBuf,
    args: Vec<String>,
    root: PathBuf,
    home: PathBuf,
    timeout: Duration,
    cancel: CancellationToken,
}

impl Go {
    pub fn new(
        program: PathBuf,
        args: Vec<String>,
        root: PathBuf,
        home: PathBuf,
        timeout: Duration,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            program,
            args,
            root,
            home,
            timeout,
            cancel,
        }
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.program);
        command
            .current_dir(&self.root)
            .env_clear()
            .env(
                "PATH",
                std::env::var_os("PATH")
                    .filter(|path| !path.is_empty())
                    .unwrap_or_else(|| MINIMUM_PATH.into()),
            )
            .env("HOME", &self.home)
            .env("LANG", "C")
            .args(&self.args)
            .args(args);
        command
    }

    async fn run(&self, args: &[&str]) -> Result<String, ResolverError> {
        let mut command = self.command(args);
        let spelled = format!("{} {}", self.program.display(), args.join(" "));
        let failed = |message: String| ResolverError {
            command: spelled.clone(),
            message,
        };

        match run_bounded(&mut command, None, self.timeout, &self.cancel).await {
            Err(source) => Err(failed(source.to_string())),
            Ok(Bounded::TimedOut) => Err(failed(format!("killed after {:?}", self.timeout))),
            Ok(Bounded::CancelledAfterSpawn) => {
                Err(failed("cancelled while it was running".to_string()))
            }
            Ok(Bounded::Finished(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                Ok(match stdout.trim().is_empty() {
                    true => String::from_utf8_lossy(&output.stderr).into_owned(),
                    false => stdout,
                })
            }
        }
    }

    fn read(&self, name: &str) -> Result<Option<String>, ResolverError> {
        match std::fs::read_to_string(self.root.join(name)) {
            Ok(contents) => Ok(Some(contents)),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(ResolverError {
                command: format!("read {}", self.root.join(name).display()),
                message: source.to_string(),
            }),
        }
    }

    fn put_back(&self, name: &str, contents: Option<&str>) -> Result<(), ResolverError> {
        let path = self.root.join(name);
        let outcome = match contents {
            Some(contents) => std::fs::write(&path, contents),
            None => match std::fs::remove_file(&path) {
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other,
            },
        };
        outcome.map_err(|source| ResolverError {
            command: format!("restore {}", path.display()),
            message: source.to_string(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn versions(&self, module: &str) -> Result<Vec<String>, ResolverError> {
        let printed = self.run(&["list", "-m", "-versions", module]).await?;
        Ok(printed
            .split_whitespace()
            .skip(1)
            .map(str::to_string)
            .collect())
    }
}

#[async_trait]
impl ModuleGraph for Go {
    async fn list(&self, module: &str) -> Result<String, ResolverError> {
        self.run(&["list", "-m", "-json", module]).await
    }

    async fn why(&self, module: &str) -> Result<String, ResolverError> {
        self.run(&["mod", "why", "-m", module]).await
    }

    async fn manifest(&self) -> Result<Manifest, ResolverError> {
        Ok(Manifest {
            go_mod: self.read(GO_MOD)?.ok_or_else(|| ResolverError {
                command: format!("read {}", self.root.join(GO_MOD).display()),
                message: "the tree is not a Go module".to_string(),
            })?,
            go_sum: self.read(GO_SUM)?,
        })
    }

    async fn get(&self, module: &str, query: &str) -> Result<String, ResolverError> {
        self.run(&["get", &format!("{module}@{query}")]).await
    }

    async fn tidy(&self) -> Result<String, ResolverError> {
        self.run(&["mod", "tidy"]).await
    }

    async fn restore(&self, manifest: &Manifest) -> Result<(), ResolverError> {
        self.put_back(GO_MOD, Some(&manifest.go_mod))?;
        self.put_back(GO_SUM, manifest.go_sum.as_deref())
    }
}
