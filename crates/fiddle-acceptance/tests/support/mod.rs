//! Process runner and fixture builder shared by the black-box acceptance tests.
//!
//! Every helper here drives the compiled `fiddle` binary as a subprocess. None
//! of them calls a library function, so what the tests observe is exactly what a
//! caller at a shell would observe: an exit code, stdout, and stderr.
//!
//! Because the observable surface is the whole contract, the M0 scenario has a
//! second, external expression: `scenarios/m0_skeleton.sh` in the public
//! `peel/fiddle-acceptance` repository asserts the same six properties as a
//! plain shell script, so the milestone is provable against a released binary
//! by someone holding neither these sources nor a Rust toolchain. The two lanes
//! are kept in step by hand; see `docs/technical/acceptance-repository.md`. A
//! change to what a helper here observes should be reflected there, or the
//! external lane quietly becomes the weaker proof.

// This file is compiled once per test binary, and no single scenario needs every
// helper — a builder used only by the assessment tests is not dead code, it is
// simply not used by the observation tests.
#![allow(dead_code)]

use assert_cmd::Command;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tempfile::TempDir;

/// The `fiddle` binary every scenario launches, built from the sources under
/// test.
///
/// Worth the code because the obvious alternative is silently wrong.
/// `assert_cmd`'s `cargo_bin` resolves a *path* under the target directory and
/// trusts that something already put a binary there. Nothing does:
/// `fiddle-acceptance` is a separate package from `fiddle-cli` and does not
/// depend on it, so `cargo test --workspace` compiles `main.rs` only as a test
/// harness under `deps/` and never produces `target/debug/fiddle`. On a clean
/// checkout that path is absent and every acceptance test fails; on a developer
/// machine it holds whatever the last `cargo build` left, which may predate the
/// change under test — and a suite that passes against last week's binary is
/// not evidence about anything.
///
/// So the harness builds the binary itself, once per test process, and uses the
/// path cargo *reports* for the artefact rather than reconstructing one from
/// assumptions about the target layout.
pub fn fiddle_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let mut build = std::process::Command::new(env!("CARGO"));
        build
            .current_dir(repo_root())
            .args([
                "build",
                "--bin",
                "fiddle",
                "--message-format",
                "json-render-diagnostics",
            ])
            // Match the profile these tests were themselves built under, so a
            // `cargo test --release` run exercises the release binary rather
            // than quietly falling back to a debug one.
            .args(if cfg!(debug_assertions) {
                &[][..]
            } else {
                &["--release"][..]
            });
        let out = build
            .output()
            .expect("could not run cargo to build the fiddle binary");
        assert!(
            out.status.success(),
            "building the fiddle binary failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        executable_from(&out.stdout).unwrap_or_else(|| {
            panic!(
                "cargo built no `fiddle` executable: {}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
    })
}

/// `assert_cmd`'s wrapper around the built binary, ready for arguments.
pub fn fiddle_command() -> Command {
    Command::new(fiddle_binary())
}

/// The `fiddle` executable path out of a `--message-format json` build log.
///
/// Cargo emits one JSON object per line; the one worth having is the
/// `compiler-artifact` for the `fiddle` binary, whose `executable` field is the
/// path it landed at. Lines that are not JSON — a stray warning, a future
/// message kind — are skipped rather than fatal, because the only thing this
/// needs from the log is that one field.
fn executable_from(build_log: &[u8]) -> Option<PathBuf> {
    String::from_utf8_lossy(build_log)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter(|message| message["target"]["name"] == "fiddle")
        .find_map(|message| Some(PathBuf::from(message["executable"].as_str()?)))
}

/// The credential-shaped environment variables the M0 lane must never need.
///
/// [`Scenario`] removes every one of these from each subprocess it launches.
/// That is the point of this milestone's acceptance lane: it must be green on a
/// machine that holds no secrets, so the milestone is never gated on one. Doing
/// it by removal rather than by assuming the environment is clean means the
/// guarantee also holds on a CI runner that happens to define `GITHUB_TOKEN`
/// for its own reasons — there, an accidental dependency on a token would
/// otherwise pass here and fail for the next person.
///
/// The external lane removes these same four names before invoking `fiddle`, so
/// shortening this list means shortening `scenarios/m0_skeleton.sh` in
/// `peel/fiddle-acceptance` too.
pub const CREDENTIAL_VARS: [&str; 4] = [
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "ANTHROPIC_API_KEY",
    "JIRA_API_TOKEN",
];

/// The `project.name` every scenario's configuration declares.
///
/// Named once because the correlation key is derived from it: a scenario that
/// wrote a marker for a different project name would be asserting the wrong
/// thing.
pub const PROJECT_NAME: &str = "icecube";

/// A disposable project: a temporary directory holding a `fiddle.toml`, the
/// stub fixture root it names, and the report directory it names.
///
/// The configuration document points at *absolute* paths inside the temporary
/// directory, so a scenario is independent of the working directory the test
/// binary happens to be launched from.
pub struct Scenario {
    dir: TempDir,
}

impl Scenario {
    /// A project whose stub root exists but holds no fixture state yet.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let scenario = Scenario { dir };
        std::fs::create_dir_all(scenario.stub_root().join("work")).unwrap();
        std::fs::create_dir_all(scenario.stub_root().join("changes")).unwrap();
        std::fs::write(
            scenario.config_path(),
            format!(
                "[project]\nname = \"icecube\"\n\n[stub]\nroot = {}\n\n[report]\ndir = {}\n",
                toml_string(&scenario.stub_root()),
                toml_string(&scenario.dir.path().join("reports")),
            ),
        )
        .unwrap();
        scenario
    }

    /// The `fiddle` binary with this scenario's credential-free environment
    /// already applied, ready for arguments.
    ///
    /// Every subprocess a scenario launches is built here rather than from
    /// [`fiddle_command`] directly, so no helper can opt out of the guarantee
    /// by accident: adding a command means inheriting the removals, and
    /// removing them means editing this one place.
    pub fn command(&self) -> Command {
        let mut command = fiddle_command();
        for name in CREDENTIAL_VARS {
            command.env_remove(name);
        }
        command
    }

    /// The `--config` argument every command in this scenario is given.
    pub fn config_path(&self) -> PathBuf {
        self.dir.path().join("fiddle.toml")
    }

    /// Run `fiddle config check --config <this scenario's document> --json` and
    /// hand back the whole process result, unjudged.
    pub fn config_check(&self) -> std::process::Output {
        self.command()
            .args([
                "config",
                "check",
                "--config",
                self.config_path().to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap()
    }

    /// The fixture directory both stub ports read.
    pub fn stub_root(&self) -> PathBuf {
        self.dir.path().join("stub-state")
    }

    /// Record `<stub.root>/work/<work_id>.json`, the work item the stub work
    /// port observes.
    pub fn write_work_item(&self, work_id: &str, status: &str) {
        std::fs::write(
            self.stub_root().join(format!("work/{work_id}.json")),
            format!("{{\"id\":\"{work_id}\",\"status\":\"{status}\"}}"),
        )
        .unwrap();
    }

    /// Record `<stub.root>/changes/<work_id>.json`, the change set the stub
    /// change port observes, carrying `marker`.
    pub fn write_change_marker(&self, work_id: &str, marker: &str) {
        std::fs::write(
            self.stub_root().join(format!("changes/{work_id}.json")),
            format!("{{\"marker\":\"{marker}\"}}"),
        )
        .unwrap();
    }

    /// The directory `report.dir` names.
    ///
    /// Nothing in this harness creates it unless a scenario asks: its absence
    /// is what proves a read-only command published no evidence bundle.
    pub fn report_dir(&self) -> PathBuf {
        self.dir.path().join("reports")
    }

    /// Create `<report.dir>` readable and listable but not writable, so a run
    /// can reach publication and fail there.
    ///
    /// Readable rather than absent on purpose: an absent directory would be
    /// created by the run, and a wholly inaccessible one would fail earlier and
    /// differently. Mode `0o500` isolates the one failure the atomicity
    /// criterion is about — the run observed, derived, and executed
    /// successfully, and could not publish what it did.
    #[cfg(unix)]
    pub fn make_report_dir_unwritable(&self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(self.report_dir()).unwrap();
        std::fs::set_permissions(self.report_dir(), std::fs::Permissions::from_mode(0o500))
            .unwrap();
    }

    /// Restore `<report.dir>` so the test can inspect what the failed run left
    /// behind — and so the temporary directory can be removed on drop.
    #[cfg(unix)]
    pub fn make_report_dir_writable(&self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(self.report_dir(), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }

    /// Every file under `<stub.root>` as `(relative path, bytes)`, sorted.
    ///
    /// Byte-level and exhaustive on purpose: comparing two snapshots catches a
    /// command that rewrote a fixture with identical-looking content, added a
    /// file, or removed one, which a spot check of one path would miss.
    pub fn stub_snapshot(&self) -> Vec<(String, Vec<u8>)> {
        let root = self.stub_root();
        let mut files = Vec::new();
        collect_files(&root, &root, &mut files);
        files.sort();
        files
    }

    /// The correlation key this scenario's project and `invocation_ref` must
    /// produce.
    ///
    /// Derived here from the design's own definition — `blake3(project + NUL +
    /// invocation_ref)`, first 16 hex characters — rather than by calling
    /// `fiddle_core::correlation_key`, so the acceptance lane still checks the
    /// binary against the specification instead of against itself.
    pub fn expected_marker(&self, invocation_ref: &str) -> String {
        blake3::hash(format!("{PROJECT_NAME}\0{invocation_ref}").as_bytes()).to_hex()[..16]
            .to_string()
    }

    /// Every file under `<stub.root>/changes` that belongs to `work_id`.
    ///
    /// Prefix-matched rather than looking up the one path the stub writes, so a
    /// second execution that wrote a *differently named* change set — a
    /// `<id>-1.json`, a leftover `<id>.json.tmp` — is counted rather than
    /// silently ignored. "Exactly one marker file exists" is only a claim worth
    /// making if a second one would be seen.
    pub fn change_files(&self, work_id: &str) -> Vec<PathBuf> {
        walkdir_files(self.stub_root().join("changes"))
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(work_id))
            })
            .collect()
    }

    /// The report bundle a `run --json` payload points at, parsed.
    ///
    /// The path is taken from the payload's `report` key and resolved against
    /// `<report.dir>`, which is how a downstream reader would find it: the test
    /// never reconstructs the attempt path itself, so a run whose payload
    /// pointed somewhere unreadable fails here instead of being papered over.
    pub fn read_bundle(&self, run_payload: &serde_json::Value) -> serde_json::Value {
        let relative = run_payload["report"]
            .as_str()
            .unwrap_or_else(|| panic!("the run payload must name its bundle: {run_payload}"));
        let path = self.report_dir().join(relative);
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("could not read {} ({e})", path.display()));
        serde_json::from_slice(&bytes).unwrap_or_else(|e| {
            panic!(
                "{} is not JSON ({e}): {}",
                path.display(),
                String::from_utf8_lossy(&bytes)
            )
        })
    }

    /// Take the whole fixture root away, so every source the ports name becomes
    /// unobservable.
    pub fn remove_stub_root(&self) {
        std::fs::remove_dir_all(self.stub_root()).unwrap();
    }

    /// The marker recorded at `<stub.root>/changes/<work_id>.json`, or `None`
    /// when no change set was written there.
    ///
    /// Reads the fixture the way the stub change port does — as JSON with a
    /// `marker` field — so a capability that wrote a file fiddle could not read
    /// back fails this helper rather than passing on the file's mere existence.
    pub fn read_change_marker(&self, work_id: &str) -> Option<String> {
        let path = self.stub_root().join(format!("changes/{work_id}.json"));
        let text = std::fs::read_to_string(&path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| {
            panic!("{} is not JSON ({e}): {text}", path.display());
        });
        value["marker"].as_str().map(str::to_string)
    }

    /// Run `fiddle run <invocation_ref> --json`, require `code`, and return the
    /// parsed payload.
    pub fn run_json(&self, invocation_ref: &str, code: i32) -> serde_json::Value {
        self.run_json_with(&[], invocation_ref, code)
    }

    /// As [`Scenario::run_json`], with additional flags placed before `--json`.
    pub fn run_json_with(
        &self,
        extra: &[&str],
        invocation_ref: &str,
        code: i32,
    ) -> serde_json::Value {
        let mut args = extra.to_vec();
        args.push("--json");
        let out = self.run_raw_with(&args, invocation_ref);
        assert_eq!(
            out.status.code(),
            Some(code),
            "stderr = {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout is not JSON ({e}): {}",
                String::from_utf8_lossy(&out.stdout)
            )
        })
    }

    /// Run `fiddle run <invocation_ref>` and hand back the whole process
    /// result — exit code, stdout, and stderr — unjudged.
    pub fn run_raw(&self, invocation_ref: &str) -> std::process::Output {
        self.run_raw_with(&[], invocation_ref)
    }

    /// Run `fiddle run <invocation_ref>` with `extra` flags and hand back the
    /// whole process result — exit code, stdout, and stderr — unjudged, for the
    /// cases that are about the diagnostic rather than the payload.
    pub fn run_raw_with(&self, extra: &[&str], invocation_ref: &str) -> std::process::Output {
        let mut command = self.command();
        command.args([
            "run",
            invocation_ref,
            "--config",
            self.config_path().to_str().unwrap(),
        ]);
        command.args(extra);
        command.output().unwrap()
    }

    /// Run `fiddle run <invocation_ref> --json` with `env` restored to the
    /// child, overriding this scenario's removals.
    ///
    /// The mirror image of the credential-free default, and the half of the
    /// guarantee removal alone cannot make: removing a variable shows fiddle
    /// does not *need* it, while supplying one and getting the same answer
    /// shows fiddle does not *consult* it.
    pub fn run_raw_with_env(
        &self,
        env: &[(&str, &str)],
        invocation_ref: &str,
    ) -> std::process::Output {
        let mut command = self.command();
        command.args([
            "run",
            invocation_ref,
            "--config",
            self.config_path().to_str().unwrap(),
            "--json",
        ]);
        for (name, value) in env {
            command.env(name, value);
        }
        command.output().unwrap()
    }

    /// Run `fiddle inspect <invocation_ref> --json`, require exit 0, and return
    /// the parsed payload.
    pub fn inspect_json(&self, invocation_ref: &str) -> serde_json::Value {
        self.inspect_json_expect_code(invocation_ref, 0)
    }

    /// Run `fiddle inspect <invocation_ref>` without `--json`, require exit 0,
    /// and return what a reader at a terminal would see on stdout.
    pub fn inspect_human(&self, invocation_ref: &str) -> String {
        let out = self
            .command()
            .args([
                "inspect",
                invocation_ref,
                "--config",
                self.config_path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "stderr = {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    /// Run `fiddle inspect <invocation_ref> --json`, require `code`, and return
    /// the parsed payload.
    pub fn inspect_json_expect_code(&self, invocation_ref: &str, code: i32) -> serde_json::Value {
        let out = self
            .command()
            .args([
                "inspect",
                invocation_ref,
                "--config",
                self.config_path().to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(code),
            "stderr = {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout is not JSON ({e}): {}",
                String::from_utf8_lossy(&out.stdout)
            )
        })
    }
}

/// Every file under `root`, recursively, as absolute paths.
///
/// A missing `root` yields an empty list rather than panicking, so "the command
/// created nothing at all" and "the command created nothing under a directory
/// that exists" are both expressible as an empty result.
pub fn walkdir_files(root: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.as_ref().to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Every directory under `root`, recursively, as absolute paths.
///
/// Publication stages a *directory*, so proving it left nothing behind means
/// looking at directories, not only at the files inside them: an empty
/// `.<attempt>.tmp` is exactly as much of a partial artefact as a full one.
pub fn walkdir_dirs(root: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.as_ref().to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.push(path.clone());
                stack.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Every file under `dir`, recursively, as a path relative to `root` paired
/// with its bytes.
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else {
            let relative = path.strip_prefix(root).unwrap().display().to_string();
            out.push((relative, std::fs::read(&path).unwrap()));
        }
    }
}

/// A path as a TOML basic string. Written by hand rather than through `toml`'s
/// serializer so the acceptance crate keeps its single-purpose dependency set.
fn toml_string(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

/// The repository root, two levels above this package.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
