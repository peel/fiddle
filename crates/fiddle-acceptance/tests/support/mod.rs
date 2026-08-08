//! Process runner and fixture builder shared by the black-box acceptance tests.
//!
//! Every helper here drives the compiled `fiddle` binary as a subprocess. None
//! of them calls a library function, so what the tests observe is exactly what a
//! caller at a shell would observe: an exit code, stdout, and stderr.

use assert_cmd::Command;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

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

    /// Take the whole fixture root away, so every source the ports name becomes
    /// unobservable.
    pub fn remove_stub_root(&self) {
        std::fs::remove_dir_all(self.stub_root()).unwrap();
    }

    /// Run `fiddle inspect <invocation_ref> --json`, require exit 0, and return
    /// the parsed payload.
    pub fn inspect_json(&self, invocation_ref: &str) -> serde_json::Value {
        self.inspect_json_expect_code(invocation_ref, 0)
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
