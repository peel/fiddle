//! The one place attribution turns into a running `go`.
//!
//! [`crate::cve::attribute`] decides which module a finding is fixed by editing,
//! and two of the questions it asks — *what does the build list resolve this to*
//! and *does bumping the parent inside its own minor change that* — have no
//! answer anywhere but a Go toolchain with a module proxy behind it. This is the
//! adapter that asks one, and it stands to `go` exactly as
//! [`crate::scanner::Wizcli`] stands to `wizcli`: one construction site, one
//! environment, one bound.
//!
//! # The environment is three names, and this is the statement of it
//!
//! `PATH`, inherited from this process or [`MINIMUM_PATH`] when it has none,
//! because `go` shells out to `git` to fetch a module and has to find its own
//! toolchain; `HOME`, pointed at a directory the caller owns, because the module
//! cache and the build cache default underneath it and a `go` with no `HOME` at
//! all refuses to start; and `LANG`, fixed to `C`, because `go list -m -json`'s
//! output is parsed.
//!
//! Those are the workspace check runner's four names minus `RUSTUP_HOME`, which
//! locates a Rust toolchain and has no business in a Go child. That is not a
//! coincidence and it is not a shared constant either: this is a different spawn
//! site, and `crate::workspace::command`'s header gives the reason the sets are
//! stated separately rather than reconciled. What they share is the rule — **a
//! locator may be inherited, an authority may not** — and the bound, which lives
//! in [`crate::process`].
//!
//! In production `home` is [`crate::workspace::Workspace::home`], so a `go` run
//! for an attempt caches into the same throwaway directory beside the worktree
//! that every other child of that attempt does, and writes nothing into the tree
//! whose diff is the evidence. Nothing credential-shaped is passed, and nothing
//! needs to be: the module proxy this reaches is public, and a private one would
//! be a change to this list argued for on its own terms rather than a variable
//! that arrived by inheritance.
//!
//! # A non-zero `go` is an answer, not a failure
//!
//! `go list -m -json` exits non-zero for a path outside the build list, and that
//! refusal is how attribution reaches rules 3 and 4. `go get` exits non-zero when
//! a version query matches nothing, and that is a parent which cannot be bumped —
//! which the probe's confirm is about to conclude anyway. So [`Go::run`] hands the
//! child's own words back as text and reserves [`ResolverError`] for the cases
//! where there was no answer at all: no `go` to run, a deadline, a cancellation.
//! The port's header states the same rule from the other side.

use super::attribute::{Manifest, ModuleGraph, ResolverError};
use crate::process::{run_bounded, Bounded};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

/// The `PATH` a child gets when this process has none.
///
/// Its own constant here rather than a shared one, matching every spawn site that
/// came before: each states its own environment in full, and a constant reached
/// across module boundaries would make the statement in one header depend on an
/// edit made in another.
const MINIMUM_PATH: &str = "/usr/bin:/bin";

/// The two files a viability probe writes, and the only two it may.
///
/// Named once and read by both [`Go::manifest`] and [`Go::restore`], so the set
/// that is captured and the set that is put back cannot come apart.
const GO_MOD: &str = "go.mod";
const GO_SUM: &str = "go.sum";

/// A Go module graph, reached as a subprocess.
///
/// `program` and `args` are the operator seam — the same shape [`crate::GhCli`]
/// and [`crate::scanner::Wizcli`] carry, and for the same reason: an operator who
/// must pin a toolchain version or wrap the binary in a launcher has somewhere to
/// do it, and the offline gate substitutes a scripted `go` through it rather than
/// through the environment, which is an allowlist.
pub struct Go {
    program: PathBuf,
    args: Vec<String>,
    /// The module root. Every command runs with this as its working directory,
    /// because that is how `go` is told which module it is being asked about —
    /// there is no flag for it.
    root: PathBuf,
    /// What `HOME` points at. Supplied rather than created here, so the caches a
    /// toolchain writes live exactly as long as the attempt that produced them.
    home: PathBuf,
    timeout: Duration,
    /// Held rather than passed per call, for [`crate::scanner::Wizcli`]'s reason:
    /// this adapter is built for one attempt, and the token that ends that
    /// attempt is the token that must end its children. It is the only channel a
    /// `^C` has to a child in a process group of its own — see
    /// [`crate::process`].
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

    /// The one `go` this module builds: an empty environment, the three names it
    /// is allowed, the module root as the working directory, and the operator's
    /// own arguments ahead of the subcommand.
    ///
    /// `env_clear` then an explicit allowlist, which is what every other spawn
    /// site in this runtime does and for the same reason: a credential added to
    /// the runner tomorrow is excluded by default rather than by somebody
    /// remembering to deny it. `std::env::remove_var` would mutate this process
    /// and is wrong for a concurrent runtime.
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.program);
        command
            .current_dir(&self.root)
            .env_clear()
            // A locator may be inherited, an authority may not. The fallback is
            // not defensive: `go` reaches `git` through `PATH` to fetch a module,
            // so a child without one fails at the first download.
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

    /// Run `go args…` and hand back what it said.
    ///
    /// stdout when there is any, and otherwise stderr — the two are not
    /// concatenated. `go list -m -json` prints its document on stdout and its
    /// progress (`go: downloading …`) on stderr, so a reader handed the pair would
    /// fail to parse a perfectly good record; and the answers this build matches
    /// rules against — `go: module …: not a known dependency`, a `go mod tidy`
    /// that says nothing — arrive on stderr with an empty stdout. Picking one is
    /// what makes both readable, and it is the rule the scripted toolchain's
    /// `Answer::text` mirrors so the two stand-ins cannot disagree about what `go`
    /// "said".
    async fn run(&self, args: &[&str]) -> Result<String, ResolverError> {
        let mut command = self.command(args);
        let spelled = format!("{} {}", self.program.display(), args.join(" "));
        let failed = |message: String| ResolverError {
            command: spelled.clone(),
            message,
        };

        // No stdin: a module query is unattended, and a program that waited on a
        // terminal it does not have would hang until the deadline rather than
        // report anything. The bound below — the deadline, the process group and
        // the group kill — is `process`'s, shared with every other child this
        // runtime starts.
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

    /// Read one of the probe's two files, or nothing where it is absent.
    ///
    /// A missing `go.sum` is an ordinary tree — a module with no requirements has
    /// none — and is not a failure to capture anything.
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

    /// Put one file back, or take it away where the capture had none.
    ///
    /// The removal arm is the half that is easy to leave out and would be wrong
    /// to: a probe that created a `go.sum` in a tree that had none leaves the
    /// tree changed after a "restore" that only ever wrote files it was given.
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

    /// The module root, so a caller can name the tree this adapter answers about.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every release of `module` the proxy will admit, newest last, in the
    /// spelling `go` printed.
    ///
    /// # Why this is inherent and not a [`ModuleGraph`] method
    ///
    /// [`ModuleGraph`] is the port *attribution* is written against, and
    /// attribution never asks this: its four rules are about the build list and
    /// the requirement chain, and none of them needs to know what else has been
    /// published. Widening the port would make every stand-in in the suite
    /// implement a method the subject under test does not call — which is the
    /// same objection [`crate::cve::dedup::library_is_at_the_fix`] makes to
    /// adding a `version` method beside `list`.
    ///
    /// The caller is [`select_target_version`](crate::cve::group::select_target_version),
    /// whose `available` argument had no producer at all until this existed. That
    /// is the gap this closes: the selection's three bounds — no downgrade, no
    /// major crossed, the minor as ceiling and floor — are arithmetic over a
    /// release list, and a build that could not obtain one could not apply them.
    ///
    /// # The spelling is `go`'s, and it is handed on unaltered
    ///
    /// `go list -m -versions <module>` prints the module path and then its
    /// versions, space-separated, on one line: `example.com/m v0.1.0 v0.2.0`. The
    /// first field is dropped and nothing else is touched — no `v` is stripped
    /// and nothing is re-sorted — because `select_target_version` answers *in the
    /// spelling the release list used* and a `go get` has to be written with the
    /// `v`. Ordering is not assumed either: that function takes the highest
    /// candidate rather than the last one.
    ///
    /// A module the proxy knows nothing about prints its path and no versions,
    /// which is an empty list rather than a failure — *nothing is published to
    /// move to* is an answer, and it is the answer
    /// [`GroupError::NoRelease`](crate::cve::group::GroupError::NoRelease)
    /// exists to report.
    ///
    /// # Nothing here filters the words, and nothing needs to
    ///
    /// [`Go::run`] answers with stderr when stdout is empty, so a `go` that
    /// complained rather than listed hands back prose — and this splits prose
    /// into "versions" just as happily as it splits a release line. That is safe
    /// rather than merely tolerable: `select_target_version` refuses every
    /// candidate whose major and minor it cannot read, so a word that is not a
    /// version cannot *become* the answer; it can only fail to be one, which is
    /// the same outcome as the empty list above. A filter here would be a second
    /// opinion about what a version is, and this crate keeps exactly one — in
    /// [`crate::cve::version`].
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
            // A tree with no `go.mod` is not a module, so there is nothing here
            // to have captured and nothing the caller could restore. Reported as
            // the resolver failing rather than as an empty manifest, because an
            // empty one would be handed back by a failed probe and would delete
            // the file it was supposed to protect.
            go_mod: self.read(GO_MOD)?.ok_or_else(|| ResolverError {
                command: format!("read {}", self.root.join(GO_MOD).display()),
                message: "the tree is not a Go module".to_string(),
            })?,
            go_sum: self.read(GO_SUM)?,
        })
    }

    async fn get(&self, module: &str, query: &str) -> Result<String, ResolverError> {
        // One argument, `path@version`, because that is `go get`'s own form.
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
