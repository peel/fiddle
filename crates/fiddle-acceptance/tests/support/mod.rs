//! Process runner and fixture builder shared by the black-box acceptance tests.
//!
//! Every helper here drives the compiled `fiddle` binary as a subprocess. None
//! of them calls a library function, so what the tests observe is exactly what a
//! caller at a shell would observe: an exit code, stdout, and stderr.

// This file is compiled once per test binary, and no single scenario needs every
// helper — a builder used only by the assessment tests is not dead code, it is
// simply not used by the observation tests.
#![allow(dead_code)]

use assert_cmd::Command;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

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

    /// The `--config` argument every command in this scenario is given.
    pub fn config_path(&self) -> PathBuf {
        self.dir.path().join("fiddle.toml")
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
    /// Nothing in this harness creates it: its absence is what proves a
    /// read-only command published no evidence bundle.
    pub fn report_dir(&self) -> PathBuf {
        self.dir.path().join("reports")
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

    /// Run `fiddle run <invocation_ref>` with `extra` flags and hand back the
    /// whole process result — exit code, stdout, and stderr — unjudged, for the
    /// cases that are about the diagnostic rather than the payload.
    pub fn run_raw_with(&self, extra: &[&str], invocation_ref: &str) -> std::process::Output {
        let mut command = Command::cargo_bin("fiddle").unwrap();
        command.args([
            "run",
            invocation_ref,
            "--config",
            self.config_path().to_str().unwrap(),
        ]);
        command.args(extra);
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
        let out = Command::cargo_bin("fiddle")
            .unwrap()
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
        let out = Command::cargo_bin("fiddle")
            .unwrap()
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
