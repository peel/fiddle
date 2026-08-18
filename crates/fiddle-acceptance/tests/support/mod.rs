//! Process runner and fixture builder shared by the black-box acceptance tests.
//!
//! Every helper here drives the compiled `fiddle` binary as a subprocess. None
//! of them calls a library function, so what the tests observe is exactly what a
//! caller at a shell would observe: an exit code, stdout, and stderr.
//!
//! Because the observable surface is the whole contract, the M0 scenario has a
//! second, external expression: `scenarios/m0_skeleton.sh` in the public
//! `peel/fiddle-acceptance` repository asserts the same seven properties as a
//! plain shell script, so the milestone is provable against a released binary
//! by someone holding neither these sources nor a Rust toolchain. The two lanes
//! are kept in step by hand; see `docs/technical/acceptance-repository.md`. A
//! change to what a helper here observes should be reflected there, or the
//! external lane quietly becomes the weaker proof.
//!
//! # What is here, in three parts
//!
//! [`Scenario`] is the disposable project every lane runs inside: a `fiddle.toml`,
//! the fixture root it names, the report directory it names, and the credential
//! removals no helper can opt out of.
//!
//! [`StubGateway`] is the loopback endpoint that answers for the model. It lives
//! here rather than in the suite that established it — `binary_repair` — because
//! two lanes now need it, and a second endpoint speaking its own idea of the
//! chat-completions wire format would be free to disagree with the first.
//!
//! [`World`] is the scripted world a *decision* walk runs against: the scenario,
//! a bare repository reached through the scripted `gh`, a conversation that can be
//! posted to and edited and paged, and the helpers that take a run's local past
//! away. Those last ones are load-bearing for M3's central proof and each is
//! tested rather than trusted; `tests/human_direction.rs` is where, and why.

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
///
/// One constraint follows from how it does that, and it is why this is not the
/// only acceptance lane: `env!("CARGO")` bakes in the absolute path of the cargo
/// that compiled these tests, so this lane can only run where that toolchain and
/// these sources are. It can never run as a relocated prebuilt test artefact on
/// a machine holding neither — which is exactly the proof the external lane,
/// `peel/fiddle-acceptance`, exists to give. The nested cargo invocation itself
/// costs little: cargo runs test binaries sequentially and the `OnceLock`
/// memoises per process, so a clean run waits on the build lock zero times.
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
            .args(profile_args(
                &std::env::current_exe().expect("could not locate this test binary"),
            ));
        let out = build
            .output()
            .expect("could not run cargo to build the fiddle binary");
        assert!(
            out.status.success(),
            "building the fiddle binary failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        executable_from(&out.stdout, "fiddle").unwrap_or_else(|| {
            panic!(
                "cargo built no `fiddle` executable: {}",
                String::from_utf8_lossy(&out.stderr)
            )
        })
    })
}

/// The cargo flags that build `fiddle` under the same profile as `test_exe`, the
/// test binary asking for it.
///
/// Read off where the test binary actually lives — cargo puts it at
/// `<target>/<profile>/deps/<name>` — rather than inferred from
/// `cfg!(debug_assertions)`. The cfg is only a proxy for the profile, and an
/// accurate one solely while the workspace declares no `[profile]` sections: a
/// `[profile.release] debug-assertions = true` would invert it, so
/// `cargo test --release` would build and drive a *debug* binary. That is the
/// same class of mismatch this module exists to prevent, so it should not be left
/// resting on a manifest section nobody has written yet. The directory cargo
/// chose cannot disagree with the profile cargo chose.
///
/// Only the built-in profiles have directory names differing from their own —
/// `dev` and `test` build into `debug`, `release` and `bench` into `release` —
/// and each is asked for by the flag cargo documents. Every custom profile's
/// directory carries its name, so it can be named straight back.
fn profile_args(test_exe: &Path) -> Vec<String> {
    let profile_dir = test_exe
        .parent()
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or_else(|| {
            panic!(
                "could not read a build profile off this test binary's location: {}",
                test_exe.display()
            )
        });
    match profile_dir {
        "debug" => Vec::new(),
        "release" => vec!["--release".to_string()],
        custom => vec!["--profile".to_string(), custom.to_string()],
    }
}

/// `assert_cmd`'s wrapper around the built binary, ready for arguments.
pub fn fiddle_command() -> Command {
    Command::new(fiddle_binary())
}

/// The scripted `gh` the deterministic GitHub suite drives, built from the
/// sources under test.
///
/// It is `fiddle-runtime`'s own fixture rather than a second one written here,
/// and that is the point: the exactly-once property is stated once, and the
/// world a black-box scenario asserts against is the same world the runtime's
/// effect suites assert against. A shell script written here would be a second
/// model of GitHub, free to disagree with the first — and two suites proving the
/// same property against two subtly different worlds prove less than one does.
///
/// `[github] cli = { program, args }` is the product seam it arrives through, the
/// one that exists for operators who must pin or wrap `gh`. Nothing fake enters
/// the product to make that possible.
pub fn gh_stub_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| runtime_fixture("gh_stub", "gh-stub"))
}

/// The recording `git` the deterministic publish suite drives, built from the
/// sources under test. Everything [`gh_stub_binary`] argues for applies here
/// unchanged; `[github] git` is the seam.
pub fn git_stub_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| runtime_fixture("git_stub", "git-stub"))
}

/// The scripted `wizcli` the CVE gate scans through, built from the sources
/// under test. `[scanner] cli` is the seam, and `[[workspace.checks]]`'s
/// artefact entry is the second one — a rescan is the check's own program run as
/// a scanner.
///
/// Everything [`gh_stub_binary`] argues for applies, with one addition of its
/// own: this fixture is **permanent**. Wiz is testable only where the tenant
/// credentials are, so no offline lane will ever call a real `wizcli`, and the
/// arm list is therefore the adapter's contract rather than a convenience. Its
/// own header says so at greater length.
pub fn wiz_stub_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| runtime_fixture("wiz_stub", "wiz-stub"))
}

/// The scripted `go` a sweep resolves and bumps a module graph through, built
/// from the sources under test. `[orchestration.cve] go` is the seam.
///
/// Permanent for [`wiz_stub_binary`]'s reason and a sharper one: this project's
/// development shell declares a Rust toolchain and no Go one, so the *production*
/// adapter — which really spawns a `go`, in an environment built from nothing —
/// can only be driven end to end against this. What is scripted is the toolchain
/// and the upstream it resolves against; the spawn, the deadline, the
/// environment and the `go.mod`/`go.sum` it leaves behind are the real ones.
pub fn go_stub_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| runtime_fixture("go_stub", "go-stub"))
}

/// The scripted check a contract is judged by, built from the sources under
/// test. A `[[workspace.checks]]` entry's own `program` is the seam.
///
/// The same argument again, for the same reason: §2.6's five checks are
/// `go build`, `go fmt`, `go vet`, `docker build` and a rescan, and this shell
/// has none of the first four. What an evaluation needs is not those programs —
/// it needs *a program*, started by the adapter, whose exit status and output the
/// adapter reads back.
pub fn check_stub_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| runtime_fixture("check_stub", "check-stub"))
}

/// Build one of `fiddle-runtime`'s scripted fixtures and hand back the path
/// cargo reports for it.
///
/// The same mechanism as [`fiddle_binary`], for the same reason, and with one
/// addition: each fixture is declared with `required-features`, so it does not
/// exist unless the feature is asked for — which is what keeps it out of
/// `cargo build --release`. The feature is therefore named here rather than
/// assumed on, and asking for it in this nested build grants it to nothing else.
fn runtime_fixture(name: &str, feature: &str) -> PathBuf {
    let mut build = std::process::Command::new(env!("CARGO"));
    build
        .current_dir(repo_root())
        .args([
            "build",
            "-p",
            "fiddle-runtime",
            "--bin",
            name,
            "--features",
            feature,
            "--message-format",
            "json-render-diagnostics",
        ])
        .args(profile_args(
            &std::env::current_exe().expect("could not locate this test binary"),
        ));
    let out = build
        .output()
        .unwrap_or_else(|e| panic!("could not run cargo to build the {name} fixture: {e}"));
    assert!(
        out.status.success(),
        "building the {name} fixture failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    executable_from(&out.stdout, name).unwrap_or_else(|| {
        panic!(
            "cargo built no `{name}` executable: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// The executable path for target `name` out of a `--message-format json` build
/// log.
///
/// Cargo emits one JSON object per line; the one worth having is the
/// `compiler-artifact` for the named binary, whose `executable` field is the
/// path it landed at. Lines that are not JSON — a stray warning, a future
/// message kind — are skipped rather than fatal, because the only thing this
/// needs from the log is that one field.
fn executable_from(build_log: &[u8], name: &str) -> Option<PathBuf> {
    String::from_utf8_lossy(build_log)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter(|message| message["target"]["name"] == name)
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

/// The defect [`Scenario::write_fixture_repo`] ships: an off-by-one that is
/// perfectly good Rust and is simply wrong, so repairing it is a real edit
/// rather than a syntax fix.
///
/// Public because a scenario that drives a repair has to say what the repaired
/// contents are, and the two spellings must differ by exactly the defect.
pub const BROKEN_FIXTURE: &str = "pub fn last_index(len: usize) -> usize { len }\n";

/// The edit that removes it, and the only content a check over this fixture
/// should accept.
pub const REPAIRED_FIXTURE: &str = "pub fn last_index(len: usize) -> usize { len - 1 }\n";

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
        Command::from_std(self.std_command())
    }

    /// The same binary and the same removals, as a plain [`std::process::Command`].
    ///
    /// `assert_cmd::Command` can only run a child to completion, and one scenario
    /// has to do something else with it: interrupt it while it is running. That
    /// needs a pid, which needs `spawn`, which is not on the wrapper.
    ///
    /// This is the *lower* half of [`Scenario::command`] rather than a second
    /// builder beside it, and deliberately so — the credential removals are the
    /// guarantee every acceptance scenario inherits, and a sibling that applied
    /// them from its own copy of the list is exactly how one of the two goes stale.
    fn std_command(&self) -> std::process::Command {
        let mut command = std::process::Command::new(fiddle_binary());
        for name in CREDENTIAL_VARS {
            command.env_remove(name);
        }
        command
    }

    /// The `--config` argument every command in this scenario is given.
    pub fn config_path(&self) -> PathBuf {
        self.dir.path().join("fiddle.toml")
    }

    /// The whole disposable project, the directory both configured roots live
    /// inside.
    ///
    /// Exposed so a containment assertion can look *above* `<report.dir>` and
    /// `<stub.root>`, which is where an escape from either of them lands.
    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

    /// Every file *and* every directory under the whole project, as
    /// `(relative path, bytes)` with directories carrying none, sorted.
    ///
    /// Wider than [`Scenario::stub_snapshot`] on purpose: that one answers
    /// "did the fixture change", while this one answers "did anything appear
    /// anywhere at all". Directories are included because an escape that
    /// created only `<report.dir>/beans-..` and no file inside it is still an
    /// artefact written where none was asked for.
    pub fn project_tree(&self) -> Vec<(String, Vec<u8>)> {
        let root = self.dir.path();
        let mut entries: Vec<(String, Vec<u8>)> = walkdir_dirs(root)
            .into_iter()
            .chain(walkdir_files(root))
            .map(|path| {
                let relative = path.strip_prefix(root).unwrap().display().to_string();
                let bytes = if path.is_dir() {
                    Vec::new()
                } else {
                    std::fs::read(&path).unwrap()
                };
                (relative, bytes)
            })
            .collect();
        entries.sort();
        entries
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

    /// The text of this scenario's own configuration document.
    pub fn config_text(&self) -> String {
        std::fs::read_to_string(self.config_path()).unwrap()
    }

    /// Append `text` to this scenario's own configuration document.
    ///
    /// A scenario starts with the M0-shaped document — three tables, no
    /// `[agent]`, no `[workspace]` — because that is the document the milestone
    /// baseline runs against. A scenario about a capability that needs a model
    /// adds the tables it needs on top, so the two documents differ by exactly
    /// what is under test rather than by having been written separately.
    pub fn append_config(&self, text: &str) {
        let mut document = self.config_text();
        document.push('\n');
        document.push_str(text);
        std::fs::write(self.config_path(), document).unwrap();
    }

    /// A one-commit git repository inside this scenario, as the repository a
    /// repairing capability is pointed at.
    ///
    /// Real git rather than a bare directory: `Workspace::create` branches a
    /// detached worktree, so a scenario over a non-repository would fail before
    /// the capability got anywhere near a model — and would then prove nothing
    /// about which capability was selected.
    ///
    /// The one source file it holds carries [`BROKEN_FIXTURE`], the defect a
    /// repair scenario exists to remove, so a check pointed at this repository
    /// can genuinely tell a repaired tree from this one.
    ///
    /// The committer identity is passed on the command line because a CI runner
    /// has none configured and `git commit` refuses without one.
    pub fn write_fixture_repo(&self) -> PathBuf {
        let repo = self.dir.path().join("fixture");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::write(repo.join("src/lib.rs"), BROKEN_FIXTURE).unwrap();
        std::fs::write(repo.join(".gitignore"), "target/\nCargo.lock\n").unwrap();
        git(&repo, &["init", "-q", "."]);
        git(&repo, &["add", "-A"]);
        git(
            &repo,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                "the fixture",
            ],
        );
        repo
    }

    /// A sibling of this scenario's configuration document holding `text`, for
    /// the assertions that are about a *rejected* document rather than the good
    /// one.
    ///
    /// A sibling rather than a replacement, so the scenario that wrote it can
    /// carry on using its own valid document afterwards.
    pub fn write_config_variant(&self, name: &str, text: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    /// Run `fiddle config check --config <config>` without `--json` and hand back
    /// the whole process result, unjudged, for the cases that are about the
    /// diagnostic a reader sees rather than the payload.
    pub fn config_check_raw(&self, config: &Path) -> std::process::Output {
        self.command()
            .args(["config", "check", "--config", config.to_str().unwrap()])
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

    /// Create the directory a run records its attempts in, leaving it writable,
    /// so sealing `<report.dir>` afterwards fails the *publication* rather than
    /// the record that precedes it.
    ///
    /// That is the state a `<report.dir>` sealed after an earlier attempt is in,
    /// and it is the only way to reach "the capability succeeded and its bundle
    /// could not be published" from outside the process: with nowhere at all to
    /// record the attempt, fiddle refuses to execute, so the interesting case
    /// never happens.
    ///
    /// The name is spelled here rather than read from `fiddle_runtime`, like
    /// [`Scenario::expected_marker`] and for the same reason: the acceptance lane
    /// checks the binary against the documented layout instead of against
    /// itself. Design §4.9 names it.
    pub fn prepare_journal_dir(&self) {
        std::fs::create_dir_all(self.report_dir().join(".attempts")).unwrap();
    }

    /// Take away everything the runs so far recorded locally: every published
    /// bundle *and* every attempt journal, since the journals live under
    /// `<report.dir>/.attempts`.
    ///
    /// This is how "the identity is recomputed, not remembered" is made a claim
    /// about the binary rather than about a code reading. A run that consulted
    /// any local record of what an earlier attempt did would, after this, have
    /// nothing to consult — so it either derives the same names from its
    /// canonical inputs or it creates a second set of objects, and the world is
    /// what says which.
    ///
    /// Tolerates an absent directory, so a scenario can call it before its first
    /// run without having to know whether one happened.
    pub fn remove_local_records(&self) {
        match std::fs::remove_dir_all(self.report_dir()) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("could not remove {} ({e})", self.report_dir().display()),
        }
        assert!(
            !self.report_dir().exists(),
            "no local record of an earlier attempt may survive"
        );
    }

    /// Seal `<stub.root>/changes`, so a capability that reached the outside world
    /// cannot record that it accounted for the work.
    ///
    /// The one failure that leaves an attempt having *changed something out
    /// there* with nothing local saying so, reachable from outside the process:
    /// the three effects commit, and the correlation marker they earn cannot be
    /// written. That is the state a fresh retry has to survive without
    /// duplicating anything, and it is the reason it is arranged rather than
    /// hoped for.
    #[cfg(unix)]
    pub fn make_changes_dir_unwritable(&self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            self.stub_root().join("changes"),
            std::fs::Permissions::from_mode(0o500),
        )
        .unwrap();
    }

    /// Put it back, so the retry can record what it accounted for — and so the
    /// temporary directory can be removed on drop.
    #[cfg(unix)]
    pub fn make_changes_dir_writable(&self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            self.stub_root().join("changes"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    /// Every attempt record under `<report.dir>`, however deep.
    ///
    /// This is what "an executed capability is always recorded" is asserted
    /// against: a file an operator can open, found by walking the report
    /// directory rather than by reconstructing a path.
    pub fn journal_records(&self) -> Vec<PathBuf> {
        walkdir_files(self.report_dir().join(".attempts"))
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

    /// Move the whole fixture root aside, so every source the ports name becomes
    /// unobservable *without* losing the state it held.
    ///
    /// Moved rather than emptied on purpose, and the distinction is the point of
    /// the assertion it serves: an emptied root is still readable, so it would
    /// exercise "the world is empty" rather than "I cannot see the world". Moved
    /// rather than removed because a cumulative scenario has to carry on with
    /// the same fixture afterwards — see [`Scenario::restore_stub_root`].
    pub fn hide_stub_root(&self) {
        std::fs::rename(self.stub_root(), self.hidden_stub_root()).unwrap();
    }

    /// Put back what [`Scenario::hide_stub_root`] moved aside, byte for byte, so
    /// the steps that follow observe the world the earlier steps left.
    pub fn restore_stub_root(&self) {
        std::fs::rename(self.hidden_stub_root(), self.stub_root()).unwrap();
    }

    /// Where [`Scenario::hide_stub_root`] parks the fixture root: a sibling of
    /// it, so the move stays within one filesystem and cannot fail across a
    /// device boundary.
    fn hidden_stub_root(&self) -> PathBuf {
        self.dir.path().join("stub-state.hidden")
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
        self.run_command(invocation_ref)
            .args(extra)
            .output()
            .unwrap()
    }

    /// `fiddle run <invocation_ref> --config <this scenario's document>`, with
    /// this scenario's credential-free environment applied and nothing else
    /// decided.
    ///
    /// Handed back unlaunched so a scenario can add its own flags *and* its own
    /// environment. Every helper above builds its command through here, so the
    /// subcommand and the `--config` argument are spelled once.
    pub fn run_command(&self, invocation_ref: &str) -> Command {
        Command::from_std(self.spawnable_run_command(invocation_ref))
    }

    /// The same invocation as [`Scenario::run_command`], spawnable.
    ///
    /// For the one scenario that interrupts a run in flight rather than waiting for
    /// it: a `SIGINT` needs a pid, and a pid needs `spawn`. It is the lower half of
    /// `run_command` rather than a copy of it, so the subcommand and the `--config`
    /// argument stay spelled once — a second builder that drifted would let a
    /// scenario interrupt a differently-configured run than the one it asserts
    /// about.
    pub fn spawnable_run_command(&self, invocation_ref: &str) -> std::process::Command {
        let mut command = self.std_command();
        command.args([
            "run",
            invocation_ref,
            "--config",
            self.config_path().to_str().unwrap(),
        ]);
        command
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
        let mut command = self.run_command(invocation_ref);
        command.arg("--json");
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

    /// `fiddle inspect <invocation_ref> --config <this scenario's document>`,
    /// with this scenario's credential-free environment already applied and
    /// nothing else decided.
    ///
    /// Handed back unlaunched, the same shape as [`Scenario::run_command`], so a
    /// scenario can add its own flags and its own environment. Every `inspect`
    /// helper builds its command through here, so the subcommand and the
    /// `--config` argument are spelled once.
    pub fn inspect_command(&self, invocation_ref: &str) -> Command {
        let mut command = self.command();
        command.args([
            "inspect",
            invocation_ref,
            "--config",
            self.config_path().to_str().unwrap(),
        ]);
        command
    }

    /// Run `fiddle inspect <invocation_ref>` with `extra` flags and hand back
    /// the whole process result — exit code, stdout, and stderr — unjudged, for
    /// the cases that are about the diagnostic rather than the payload.
    pub fn inspect_raw_with(&self, extra: &[&str], invocation_ref: &str) -> std::process::Output {
        self.inspect_command(invocation_ref)
            .args(extra)
            .output()
            .unwrap()
    }

    /// As [`Scenario::inspect_json`], with additional flags placed before
    /// `--json`. Requires exit 0.
    pub fn inspect_json_with(&self, extra: &[&str], invocation_ref: &str) -> serde_json::Value {
        let mut args = extra.to_vec();
        args.push("--json");
        let out = self.inspect_raw_with(&args, invocation_ref);
        assert_eq!(
            out.status.code(),
            Some(0),
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
pub fn toml_string(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

/// Run git in `dir`, panicking with its stderr if it fails.
pub fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|source| panic!("could not run git {args:?}: {source}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// What git in `dir` printed on stdout, trimmed, panicking with its stderr if it
/// failed.
///
/// The reading half of [`git`]. Public here rather than copied into a test
/// binary because two suites already hold private copies of it, and a third would
/// be the one that drifted.
pub fn git_says(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|source| panic!("could not run git {args:?}: {source}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
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

// ===========================================================================
// A model gateway that answers
// ===========================================================================
//
// Moved here from `binary_repair.rs`, which established it and remains its
// largest caller. It is shared rather than copied for the reason
// [`gh_stub_binary`] gives about the scripted `gh`: a second endpoint speaking
// its own idea of the chat-completions wire format would be free to disagree
// with the first, and two suites proving a property against two subtly
// different gateways prove less than one does. The move is behaviour-preserving
// and `binary_repair`'s own scenarios are what say so.

/// One scripted answer: the status line it is sent under, and its body.
///
/// The status is scripted rather than fixed at 200 because the interesting
/// half of a gateway's behaviour is the half that refuses. A gateway that
/// answers `401` with a body quoting the key it was sent is an ordinary
/// deployment accident — and until it could be scripted here, nothing in the
/// suite could reach the code that decides what a refusal is allowed to say.
pub struct Reply {
    pub status: u16,
    pub phrase: &'static str,
    pub body: serde_json::Value,
}

/// A reply the client is meant to accept.
pub fn accepted(body: serde_json::Value) -> Reply {
    Reply {
        status: 200,
        phrase: "OK",
        body,
    }
}

/// A reply that refuses, carrying whatever the gateway felt like saying.
pub fn refused(status: u16, phrase: &'static str, body: serde_json::Value) -> Reply {
    Reply {
        status,
        phrase,
        body,
    }
}

/// A loopback endpoint that answers `POST <base>/chat/completions` from a fixed
/// script of replies.
///
/// One reply per connection, in order, and then the listener is dropped. A run
/// that asked for more turns than the script holds therefore fails at the socket
/// with a diagnostic rather than hanging, and [`StubGateway::served`] says how
/// many turns were actually taken — which is how a scenario asserts that the
/// binary really dialled out rather than concluding some other way.
///
/// Written against `TcpStream` rather than an HTTP crate on purpose: the
/// acceptance package depends on nothing that could let a scenario reach inside
/// the binary it is testing, and one request-response exchange over HTTP/1.1 is
/// smaller than the dependency would be.
pub struct StubGateway {
    port: u16,
    served: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    bodies: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl StubGateway {
    /// An endpoint that will answer with each of `script` in turn.
    pub fn serving(script: Vec<Reply>) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let served = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&served);
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&bodies);
        // Detached rather than joined. A scenario that drove fewer turns than
        // the script holds leaves this thread blocked in `accept`, and joining
        // it would turn "the binary stopped early" — a perfectly good assertion
        // failure — into a hang.
        std::thread::spawn(move || {
            for reply in script {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let Ok(body) = answer(stream, &reply) else {
                    return;
                };
                // Recorded before the count, so a scenario that reads both
                // cannot see a turn counted without its request beside it.
                recorder
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(String::from_utf8_lossy(&body).into_owned());
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        StubGateway {
            port,
            served,
            bodies,
        }
    }

    /// The `agent.base_url` a document must name to reach this endpoint.
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    /// How many completions this endpoint has answered.
    pub fn served(&self) -> usize {
        self.served.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The **request bodies** this endpoint received, in order.
    ///
    /// The bodies and not the whole requests, deliberately. The credential
    /// belongs in the `authorization` header and is sent there on every turn,
    /// so a search over the full request would find it and prove nothing. What
    /// a scenario asks of this is the other question: whether anything the
    /// *model* is shown — preamble, message history, tool definitions, tool
    /// arguments and tool results — carries a host fact. That is the body.
    pub fn request_bodies(&self) -> Vec<String> {
        self.bodies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

/// Read one whole HTTP request off `stream`, answer it with `reply`, and hand
/// back the **request body** that was received.
///
/// The request is drained in full — headers *and* body — before anything is
/// written back, because a server that replies while the client is still
/// sending gets its answer thrown away with a connection reset on some
/// platforms. Draining it was always necessary; returning it costs nothing and
/// is what lets a scenario assert against the bytes the client actually put on
/// the wire rather than against the builder that produced them.
///
/// `connection: close` on the response, so the client opens a fresh connection
/// per turn and this function never has to multiplex one.
fn answer(mut stream: std::net::TcpStream, reply: &Reply) -> std::io::Result<Vec<u8>> {
    use std::io::{Read, Write};

    let mut request = Vec::new();
    let mut chunk = [0u8; 4096];

    let head = loop {
        if let Some(at) = find(&request, b"\r\n\r\n") {
            break at + 4;
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(Vec::new());
        }
        request.extend_from_slice(&chunk[..read]);
    };

    let length = content_length(&request[..head]);
    while request.len() < head + length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
    }
    let received = request[head..].to_vec();

    let body = reply.body.to_string();
    stream.write_all(
        format!(
            "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\n\
             content-length: {}\r\nconnection: close\r\n\r\n{body}",
            reply.status,
            reply.phrase,
            body.len(),
        )
        .as_bytes(),
    )?;
    stream.flush()?;
    let _ = stream.shutdown(std::net::Shutdown::Write);
    Ok(received)
}

/// The `content-length` a request's head declares, or zero when it declares
/// none.
fn content_length(head: &[u8]) -> usize {
    String::from_utf8_lossy(head)
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0)
}

/// The index of `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// One chat-completions response carrying `message`.
///
/// Every field the wire format declares is present, including the ones a real
/// gateway may omit, so this stays a description of the protocol rather than of
/// which fields one client happens to tolerate.
pub fn completion(message: serde_json::Value, finish_reason: &str) -> serde_json::Value {
    serde_json::json!({
        "id": "chatcmpl-stub",
        "object": "chat.completion",
        "created": 0,
        "model": "a-model",
        "system_fingerprint": null,
        "choices": [{
            "index": 0,
            "message": message,
            "logprobs": null,
            "finish_reason": finish_reason,
        }],
        "usage": null,
    })
}

/// A turn in which the model calls one tool.
pub fn calls(tool: &str, arguments: serde_json::Value) -> serde_json::Value {
    completion(
        serde_json::json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": "call-1",
                "type": "function",
                "function": { "name": tool, "arguments": arguments },
            }],
        }),
        "tool_calls",
    )
}

/// A turn in which the model returns its final report.
pub fn reports(report: serde_json::Value) -> serde_json::Value {
    completion(
        serde_json::json!({ "role": "assistant", "content": report.to_string() }),
        "stop",
    )
}

/// The script: write the fix, then report it.
///
/// Two turns, and the first one is the whole point — the repair has to be made
/// through the binary's own `write_file` tool, into the binary's own ephemeral
/// worktree, or the check that follows has nothing to pass over.
pub fn a_real_repair() -> Vec<Reply> {
    vec![
        accepted(calls(
            "write_file",
            serde_json::json!({
                "path": "src/lib.rs",
                "contents": REPAIRED_FIXTURE,
            }),
        )),
        accepted(reports(serde_json::json!({
            "changed_files": ["src/lib.rs"],
            "summary": "corrected the off-by-one",
            "claimed_complete": true,
        }))),
    ]
}

/// What a **second** attempt writes, once somebody has said to do it differently.
///
/// It has to satisfy two things at once and they pull in opposite directions. It
/// must still pass [`CHECK`] — which greps for `len - 1` — because a redirected
/// attempt whose check failed publishes nothing, and the scenario would then be
/// about a failed check rather than about a redirect. And it must **differ from
/// [`REPAIRED_FIXTURE`] in its bytes**, because "a genuinely different change" is
/// asserted by reading the pushed tree and comparing it with what was there before.
///
/// A comment carrying the instruction's own words is what makes the difference
/// legible in a diff, and it is also the honest shape of what a redirect produces: a
/// second attempt at the same defect, done the way somebody asked for.
pub const REDIRECTED_FIXTURE: &str = "pub fn last_index(len: usize) -> usize {\n    \
     // the other crate's convention, per the instruction\n    len - 1\n}\n";

/// The script a second attempt spends: write the other version, then report it.
///
/// [`a_real_repair`]'s sibling, and two turns for the same reason — the write has to
/// go through the binary's own `write_file` into the binary's own worktree, or the
/// check that follows has nothing to pass over and the push has nothing to publish.
pub fn a_second_repair() -> Vec<Reply> {
    vec![
        accepted(calls(
            "write_file",
            serde_json::json!({
                "path": "src/lib.rs",
                "contents": REDIRECTED_FIXTURE,
            }),
        )),
        accepted(reports(serde_json::json!({
            "changed_files": ["src/lib.rs"],
            "summary": "did it the other way",
            "claimed_complete": true,
        }))),
    ]
}

/// A turn in which the interpreting model reads the reply as a **redirect**, and
/// says what to do instead.
///
/// # The instruction is a separate argument from the evidence, and that is the point
///
/// `evidence` has to be a span really present in the reply — `interpret::decide`
/// refuses a model that quoted what it was not shown — and `instruction` has **no
/// such anchor**. Task 9's substring check constrains `evidence` alone, so a model
/// may return an `instruction` the person never wrote, and the length cap is the
/// specified mitigation rather than a provenance check.
///
/// Two arguments rather than one is therefore the fixture telling the truth about
/// the seam: the text that reaches a later attempt's prompt is **model-authored**,
/// influenced by whoever wrote the comment and not equal to it. A scenario that
/// wants the realistic attacker — somebody who writes a hostile comment and a model
/// that faithfully copies it — passes the same string twice, and one that wants the
/// weaker-provenance case passes two different ones. Both are reachable, and a
/// single-argument helper could only express one of them.
pub fn redirects(instruction: &str, evidence: &str) -> Reply {
    accepted(reports(serde_json::json!({
        "decision": "redirect",
        "redirect": instruction,
        "evidence": evidence,
    })))
}

/// The script a suspension, a redirect, and the fresh attempt it causes spend
/// between them.
///
/// **Five replies, and the count is the assertion.** Two for the first attempt, one
/// for the interpretation that reads the reply as a redirect, and two for the second
/// attempt. The gateway drops its listener when the script runs out, so a run that
/// took a sixth turn fails at the socket rather than being answered something
/// plausible — and, the direction that matters more here, a run that took only three
/// leaves two unspent, which [`World::model_calls`] is what makes visible.
///
/// That last point is why this constant exists rather than the callers concatenating:
/// **a redirect that never reached its attempt is indistinguishable from a redirect
/// the model declined** — `interpret` collapses every transport failure to `Unclear`,
/// which is `AwaitingDecision`, exit 10, nothing mutated. The script's length and the
/// served count are how a scenario tells the two apart.
pub fn a_suspension_and_its_redirect(instruction: &str, evidence: &str) -> Vec<Reply> {
    let mut script = a_real_repair();
    script.push(redirects(instruction, evidence));
    script.extend(a_second_repair());
    script
}

/// The script a redirect spends when the attempt it causes **changes nothing**.
///
/// **Four replies, and the fourth is the whole scenario.** Two for the first attempt,
/// one for the interpretation, and then a redirected attempt that calls no tool and
/// reports completion anyway. One reply rather than two, because a turn that makes no
/// tool call is the attempt's last: `agent::attempt` prompts once and reads the typed
/// report off whatever turn stops.
///
/// # Why the check passes over the tree this attempt leaves
///
/// It has to, or the scenario is about a failed check instead. A redirected attempt is
/// branched at the **published head** — [`ProposeChange::redirect`] passes `head_sha`
/// to `produce_from`, not the fixture's `HEAD` — and that commit already carries
/// [`REPAIRED_FIXTURE`], which contains the `len - 1` the check greps for. So an
/// attempt that writes nothing leaves a tree the check is happy with and git saw no
/// change in, which is exactly the pair `CapabilityError::NothingProposed` is for.
///
/// `claimed_complete: true` and an empty `changed_files`, deliberately: the model
/// claims it finished, and the refusal has to come from git rather than from the
/// model's own account of itself. A script whose report claimed nothing would let the
/// scenario pass against a build that believed reports.
pub fn a_redirect_whose_attempt_changes_nothing(instruction: &str, evidence: &str) -> Vec<Reply> {
    let mut script = a_real_repair();
    script.push(redirects(instruction, evidence));
    script.push(accepted(reports(serde_json::json!({
        "changed_files": [],
        "summary": "it was already the way you asked for",
        "claimed_complete": true,
    }))));
    script
}

/// A turn in which the model answers the one interpretation question: is this
/// reply an approval of this request?
///
/// The shape is the wire format `human::interpret` declares and nothing wider: a
/// `decision`, an `evidence` span the model claims to have copied out of the reply,
/// and — absent here — a `redirect`. Written out rather than reached for from
/// `fiddle-runtime`, because the acceptance crate depends on neither library and a
/// fixture that imported the reply type would be scripting the product's own idea
/// of its wire format instead of a provider's.
///
/// `evidence` has to be a span that is really in the reply. It is checked rather
/// than trusted — `interpret::decide` refuses a model that quoted something the
/// input does not contain — so a scenario passes the same words it wrote on the
/// conversation and a scenario testing that refusal passes different ones.
pub fn interprets(verdict: &str, evidence: &str) -> Reply {
    accepted(reports(serde_json::json!({
        "decision": verdict,
        "redirect": serde_json::Value::Null,
        "evidence": evidence,
    })))
}

/// The script a suspension and the process that continues it spend between them:
/// one repair, then one interpretation.
///
/// Three replies, and the count is the assertion this constant makes about the
/// walks. **Process A spends two** — a `write_file` and a report, which is one
/// bounded attempt — and then publishes, asks, and stops without another turn.
/// **Process B spends one**, on step 7 of the validation order, and it is step
/// *seven*: the six deterministic steps before it happen without a model, which is
/// why a run whose reply is unauthorized or whose head has moved never reaches this
/// reply at all. **Process C spends none**, because its walk refuses at step 6 —
/// the pull request is no longer a draft — and the transition it was about has
/// already happened.
///
/// So a run that took a turn it should not have exhausts this script and fails at
/// the socket, loudly, rather than being answered something plausible. That is the
/// script's real work: the gateway drops its listener when the script runs out, so
/// the count here is a bound on model calls that no test has to remember to assert.
pub fn a_suspension_and_its_approval(approval: &str) -> Vec<Reply> {
    let mut script = a_real_repair();
    script.push(interprets("approve", approval));
    script
}

/// An interpretation naming **fields outside the schema** beside a perfectly good
/// approval.
///
/// # What a document like this would be if it were read
///
/// `effect` and `payload` are the two identities the marker carries and the two the
/// walk recomputes for itself. A model that could name either would be naming *which
/// change gets spent* — it would be choosing the effect a person's approval is applied
/// to, from a document a provider returned. That is the blast radius
/// `#[serde(deny_unknown_fields)]` on `interpret::Reply` exists to bound: the reply is
/// the one document in this walk that comes from outside and is not a person's words,
/// so a field the schema does not name is refused rather than ignored.
///
/// # Why this belongs at the acceptance tier and not only beside `interpret`
///
/// An inversion measured it. Removing `deny_unknown_fields` fails **2 of 8** in
/// `fiddle-runtime --test interpretation` and **nothing at all** in
/// `--test decision_protocol` (19/19) or in `fiddle-acceptance --test human_direction`
/// (29/29 at the time), because every scripted reply in those two suites is a document
/// this build authored. So the property was asserted against `interpret` and not
/// against the **walk** that calls it, and not against the binary at all.
///
/// The verdict is a real `"approve"` with a real quoted span, so nothing but the extra
/// fields can be the reason it is refused. A scenario built on a *malformed* verdict
/// would be refused by the schema either way.
pub fn a_suspension_and_a_hostile_interpretation(approval: &str) -> Vec<Reply> {
    let mut script = a_real_repair();
    script.push(accepted(reports(serde_json::json!({
        "decision": "approve",
        "redirect": serde_json::Value::Null,
        "evidence": approval,
        // Sixteen lowercase hex characters each, which is the shape the marker's own
        // grammar requires — so a build that read these would have values it could act
        // on rather than ones it would reject for their form.
        "effect": "dead0beef0dead00",
        "payload": "0feed0dad0cafe00",
    }))));
    script
}

// ===========================================================================
// The scripted world a decision walk runs against
// ===========================================================================

/// The work every decision-walk scenario is about, and the reference that
/// addresses it.
pub const WORK_ID: &str = "m3-demo";
pub const INVOCATION_REF: &str = "beans:m3-demo";

/// A second piece of work in the same project, for the one scenario that needs
/// two runs to have happened.
///
/// It exists because M2's property is working: a run that accounted for
/// [`WORK_ID`] leaves a correlation marker, and a second run addressed the same
/// way therefore finds nothing to do and executes no capability — so it would
/// branch no worktree and record no attempt. A second piece of work is the
/// honest way to have a second attempt, rather than deleting the first one's
/// marker to make the fixture cooperate.
pub const SECOND_WORK_ID: &str = "m3-demo-again";
pub const SECOND_INVOCATION_REF: &str = "beans:m3-demo-again";

/// The repository the question is asked in, and the branch a publication is
/// proposed into.
pub const REPO: &str = "acme/r";
pub const BASE: &str = "main";

/// The immutable numeric id this deployment nominated as able to decide.
///
/// An id and not a login, which is the allowlist's whole design: a login can be
/// changed and the vacated name reclaimed, and a numeric id cannot. The same
/// value `fiddle-runtime`'s `propose_capability` suite nominates, so a scenario
/// read across the two tiers is about one person.
pub const AUTHORIZED: u64 = 505_401;

/// A numeric id nobody nominated. Whatever this account writes, it is not an
/// answer a run may act on.
pub const STRANGER: u64 = 999_999;

/// The variables the decision-walk documents name. Never values.
pub const FORGE_CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";
pub const MODEL_CREDENTIAL: &str = "LITELLM_API_KEY";

/// Every variable [`World`]'s own document names, as one list.
///
/// # Why these are not in [`CREDENTIAL_VARS`], and must not be added to it
///
/// The two lists answer different questions, and conflating them would be wrong in
/// both directions.
///
/// [`CREDENTIAL_VARS`] is the four credential-shaped names that exist in real
/// deployments and that fiddle must never *need*. It is a **cross-repo contract**:
/// `scenarios/m0_skeleton.sh` in the public `peel/fiddle-acceptance` repository
/// removes the same four before invoking the binary, and its own document names no
/// forge credential at all. Adding `FIDDLE_GITHUB_TOKEN` there would export a
/// detail of *this fixture's* document into a contract the external lane cannot
/// mirror meaningfully.
///
/// These two are the opposite kind of thing: the variables this world's `[github]`
/// and `[agent]` tables happen to name, which the credentialled path must
/// **export** and the credential-free path must **remove**. They are a property of
/// the fixture, so they live with the fixture.
///
/// # And why it is a list rather than two `env_remove` calls
///
/// The defect this closes was exactly a missing line: `MODEL_CREDENTIAL` was
/// removed and `FORGE_CREDENTIAL` was not, so a credential-free run inherited the
/// one variable this world's own document names — and `.env` in this worktree
/// declares it, which made the read-only guarantee a claim about the machine rather
/// than about the binary. One list that both halves iterate cannot disagree with
/// itself; two hand-written call sites already did.
pub const WORLD_CREDENTIAL_VARS: [&str; 2] = [FORGE_CREDENTIAL, MODEL_CREDENTIAL];

/// The numeric id the scripted `gh` lists fiddle's own comments under.
///
/// Spelled here rather than imported because `gh_stub`'s constant is private to a
/// binary. It is load-bearing in one direction only: a value that disagreed would
/// make an assertion about who asked a question fail loudly rather than pass
/// wrongly.
pub const FIDDLE_BOT: u64 = 1_000_001;

/// What is exported as the forge credential: a string that authenticates nothing
/// — the `gh` it reaches is a scripted program and the remote is a path — and
/// that must appear on no surface a reader can reach.
pub const SENTINEL: &str = "ghp_m3_sentinel_must_never_be_printed_7c04";

/// The GraphQL node id the forge gives the pull request a proposal opens.
///
/// **The one value in this fixture that says the transition was spent on the object
/// it was read from.** `markPullRequestReadyForReview` is addressed by node id and
/// by nothing else, and the product carries this string from
/// `EnsurePullRequestReady::inspect` — which refuses to fetch one inside `apply`,
/// because a fetch there is a second chance to decide which object an approval was
/// for — through to the mutation. The scripted `gh` then takes a pull request out of
/// draft only when a landed mutation names *its* node id.
///
/// So the value is discriminating in a way a bare count is not: a run that invented
/// a node id, or carried one from another pull request, dispatches a mutation this
/// world applies to nothing, and the by-number read still answers `draft: true`.
///
/// Shaped like GitHub's own — `PR_` and an opaque tail — and not like a number, for
/// [`CONVERSATION_ISSUE`]'s reason: a value that could be an index or a count is one
/// a test could match by accident.
///
/// # Its *value* is unclosable, and that is worth saying instead of implying otherwise
///
/// Replacing this string with any other broke **no test in the acceptance crate**,
/// and could not: the product reads the node id out of the seed and writes the same
/// one back into the mutation, so both sides of every comparison move together.
/// Reading the field and inventing it are indistinguishable by construction, exactly
/// as [`SEEDED_AT`] is for `created_at`.
///
/// **Its presence is not unclosable, and that is where the discrimination lives.**
/// Removing `node_id` from the seed fails
/// `a_suspension_then_a_fresh_process_acts_only_on_what_the_conversation_says`,
/// because `EnsurePullRequestReady` refuses an answer it cannot read a node id out
/// of rather than fetching one — a fetch inside `apply` being a second chance to
/// decide which object an approval was for. So the field is tested and the string is
/// not, and no check that cannot fail has been added to pretend otherwise.
pub const PULL_REQUEST_NODE_ID: &str = "PR_kwDOm3demoNode7";

/// The issue number the conversation is read and written under.
///
/// Seven because that is the number the scripted `gh` assigns the first pull
/// request in a world, and the conversation a question is published to is that
/// pull request's. It matters that the two agree: the stub merges the comments
/// a run posted onto the listing **keyed on the exact path**, so a listing read
/// under a different number would not show them and a test asserting "the
/// question is on the conversation" would fail against a run that had published
/// it correctly.
pub const CONVERSATION_ISSUE: u64 = 7;

/// The timestamp a comment this fixture seeded carries in both of its fields —
/// which is what says nobody has edited it.
///
/// The same value the scripted `gh` stamps the comments a run posted with, so a
/// seeded comment and a posted one are indistinguishable in this respect. A
/// value that disagreed would let a test tell them apart by their clocks, which
/// is not a distinction GitHub offers.
pub const SEEDED_AT: &str = "2026-08-11T00:00:00Z";

/// The `updated_at` [`World::edit_comment`] moves a comment to.
///
/// Strictly later than [`SEEDED_AT`], because that ordering is the fact an edit
/// consists of: `validate` refuses a comment whose two stamps differ, and it can
/// only do so if the fixture can produce one.
pub const EDITED_AT: &str = "2026-08-11T12:00:00Z";

/// The `created_at` a comment carries when it was written **before** an edit that has
/// already happened — strictly earlier than [`SEEDED_AT`], which is then its
/// `updated_at`.
///
/// # Which `created_at` this makes discriminating, and which it does not
///
/// **Two surfaces share this field's name and only one of them is closed by this
/// constant.** An earlier version of this doc said "the value that makes `created_at` a
/// discriminating field", which is too broad and contradicts an accurate note in
/// `human_direction.rs` — a reader would get opposite answers about one field name
/// depending on which file they opened first.
///
/// **The product's `HumanResponse::created_at` is discriminating, and this value is what
/// made it so.** [`World::show_as_edited_before_the_listing`] writes this into the by-id
/// re-read file, and step 5 of the validation order compares the two stamps it finds
/// there; deleting that comparison now fails
/// `an_edited_request_comment_is_refused_rather_than_recomputed_around`, which it did not
/// before.
///
/// **The fixture's own `comment_from`-`created_at` is still not.** That accessor reads
/// the *listing*, which never carries this value — `write_listed_comment` stamps both
/// fields `SEEDED_AT` — so hardcoding `comment_from`'s `created_at` still breaks nothing.
/// `fiddle-pwyi`'s finding stands unchanged for that surface, and
/// `the_scripted_conversation_is_mutable_and_ordered_by_id` records it at the assertion.
///
/// The durable form of such a claim names the condition rather than stating the verdict
/// absolutely: *unclosable while `SEEDED_AT` is the only value written to this field
/// **through this accessor***. Stated absolutely it reads as settled and outlives its
/// justification, which is how the over-broad version above got written — a second value
/// existed, and nobody checked which surface it was observable through.
pub const WRITTEN_BEFORE_AN_EDIT: &str = "2026-08-10T00:00:00Z";

/// The id the first inline review comment this world holds is numbered from.
///
/// **Above [`FIRST_POSTED_COMMENT`]'s 9000 on purpose, and the choice is what makes
/// "the review-comment endpoint is never consulted" a property with a way of
/// failing.** `validate::select_candidates` silently skips any comment whose id is
/// *below* the request comment's — it is not a reply to a question that did not
/// exist yet — so a review comment numbered under 9000 could be merged onto the
/// conversation by a future defect and still change nothing, and the test asserting
/// the endpoint was never asked for would keep passing for the wrong reason.
///
/// Numbered above it, an approval sitting in that collection is a comment a walk
/// *would* accept if it ever reached one: authorized author, a person, later than
/// the question. So the row asserting the pull request stayed a draft is refuted by
/// the defect it is about, rather than surviving it.
pub const FIRST_REVIEW_COMMENT: u64 = 9_500;

/// The app a comment can be attributed to, which is the second of the two
/// spellings of not being a person.
///
/// `HumanResponse::is_bot` is `user.type == "Bot" || performed_via_github_app is
/// not null` — a disjunction, at `github/comments.rs:147`. A fixture that could
/// only express the first spelling could not tell a walk checking **both** from one
/// checking **either**, so both are expressible and the matrix uses each once.
///
/// Shaped like GitHub's own rather than a bare `true`, because the product's field
/// is a `Value` whose *nullness* is the fact — declared that way so that a payload
/// which never mentioned an app could not be read as one denying it — and a fixture
/// writing a non-object would be agreeing with a narrower rule than the one under
/// test.
///
/// # Its *contents* are unclosable, and saying so beats adding a check that cannot fail
///
/// `is_bot` consults `performed_via_github_app.is_null()` and nothing inside it, so
/// replacing every field here with different values breaks no test and could not:
/// three keys nobody reads are indistinguishable from three other keys nobody reads.
/// **Its non-nullness is what discriminates**, and that *is* closed — the app row of
/// the matrix fails the moment this is `Null`, because the comment becomes an ordinary
/// authorized person's reply and mutates.
///
/// So the object is shaped like a real one for the reader's sake and is not asserted
/// about, on the same footing as [`SEEDED_AT`] and [`PULL_REQUEST_NODE_ID`]. Recorded
/// here so a later reader does not mistake the absence of an assertion for an
/// oversight.
pub const POSTING_APP: fn() -> serde_json::Value = || {
    serde_json::json!({
        "id": 77_001,
        "slug": "some-automation",
        "name": "Some Automation",
    })
};

/// The binding a request comment's marker carries: the four identities a person is
/// being asked about.
///
/// `Eq` because the whole use of it is one comparison — the binding a continuing
/// process validated against is the binding the suspending process published — and
/// that comparison is over all four fields at once. Field by field would let three
/// of them be checked and the fourth quietly ignored.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    /// The question's own id, and the only one a later process can find the
    /// question by.
    pub request: String,
    /// The gated effect this question is about.
    pub effect: String,
    /// The digest of the payload that effect would carry.
    pub payload: String,
    /// The revision the pull request's head was at when the question was asked.
    pub head_sha: String,
}

/// Read the marker one request comment carries, **re-derived from the design's own
/// grammar**.
///
/// # This deliberately does not call `fiddle_core::parse_marker`
///
/// The acceptance crate depends on neither `fiddle-core` nor `fiddle-runtime`, and
/// that is not tidiness: every helper in this module drives the compiled binary as a
/// subprocess, *"so what the tests observe is exactly what a caller at a shell would
/// observe"*. Reaching into the library for the parser would break that in the one
/// place it matters most. **A wrong `parse_marker` would pass**, because the test
/// and the product would share the defect and neither could see it — the lane would
/// stop being a second opinion and become a mirror.
///
/// So the grammar is taken from the design, which states it outright:
///
/// ```text
/// <!-- fiddle:decision v1 request=<16hex> effect=<16hex> payload=<16hex> head=<40hex> -->
/// ```
///
/// This is `Cargo.toml`'s reason for carrying `blake3` as a dev-dependency, applied
/// to the other half of the same format: [`Scenario::expected_marker`] re-derives a
/// correlation key from the design's definition rather than from `fiddle-core`'s
/// implementation of it, and this re-derives a marker's shape the same way.
///
/// # As strict as the design says, and no stricter
///
/// The design's own words: *"the exact key order, the exact lengths, lowercase hex,
/// and no extra keys"*. Each of those is checked, because each is a way a body can
/// resemble a request comment without being one — and a lenient parser here would
/// let a scenario assert that a question was published against a comment that only
/// looked like one. What is **not** checked is the version-token diagnosis the
/// product makes, which distinguishes a marker from a later build from a mangled
/// body: that is a refusal taxonomy for an operator, and this returns a reason
/// string because a scenario's next move is to print it.
pub fn parse_marker(body: &str) -> Result<Binding, String> {
    const OPENING: &str = "<!-- fiddle:decision ";
    const CLOSING: &str = " -->";
    /// Every field, in the one order a marker may spell them, with the width each
    /// value must have. One statement rather than four, so the order and the widths
    /// cannot disagree with each other.
    const FIELDS: [(&str, usize); 4] = [
        ("request", 16),
        ("effect", 16),
        ("payload", 16),
        ("head", 40),
    ];

    let mut openings = body.match_indices(OPENING);
    let (start, _) = openings
        .next()
        .ok_or_else(|| format!("no fiddle decision marker in this body: {body:?}"))?;
    if openings.next().is_some() {
        return Err("a body carrying two markers is not a body to choose between".to_string());
    }
    let rest = &body[start + OPENING.len()..];
    let end = rest
        .find(CLOSING)
        .ok_or_else(|| format!("a marker opens and is never closed by {CLOSING:?}"))?;

    // Split on a single space and never on whitespace, so a doubled space or an
    // embedded newline is a malformed marker rather than something this parser
    // silently tidies up.
    let tokens: Vec<&str> = rest[..end].split(' ').collect();
    let [version, fields @ ..] = tokens.as_slice() else {
        return Err(format!("an empty marker: {body:?}"));
    };
    if *version != "v1" {
        return Err(format!("marker version {version:?} is not v1"));
    }
    if fields.len() != FIELDS.len() {
        return Err(format!(
            "a marker spells {} fields and the format has {}: {tokens:?}",
            fields.len(),
            FIELDS.len()
        ));
    }

    let mut values = Vec::with_capacity(FIELDS.len());
    for ((key, width), token) in FIELDS.iter().zip(fields) {
        let value = token
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix('='))
            .ok_or_else(|| format!("expected {key}=… in position, got {token:?}"))?;
        if value.len() != *width
            || !value
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        {
            return Err(format!(
                "{key} must be {width} lowercase hex characters, and is {value:?}"
            ));
        }
        values.push(value.to_string());
    }
    let mut values = values.into_iter();
    Ok(Binding {
        request: values.next().unwrap(),
        effect: values.next().unwrap(),
        payload: values.next().unwrap(),
        head_sha: values.next().unwrap(),
    })
}

/// Write a binding back out as the marker line a request comment carries.
///
/// [`parse_marker`]'s inverse, from the same statement of the design's grammar and for
/// the same reason it is not `fiddle_core::render_marker`: a fixture that rendered with
/// the product's own function could not express a marker the product would not write,
/// which is the only kind worth forging.
///
/// It exists so a scenario can edit **one field** of a published marker and leave every
/// other byte of the comment alone. That is the case step 3 of the validation order is
/// an authentication rather than a formality for: a marker naming the right request id
/// proves only that its author could read the conversation, and the effect id is
/// derived from four values the conversation does not carry. A scenario that rewrote
/// the whole body instead would be refused for whichever difference the product noticed
/// first.
///
/// [`World::rewrite_the_published_marker`] asserts that what this renders is
/// byte-for-byte what the body already held, which is what keeps this grammar and the
/// product's from drifting apart unnoticed.
pub fn render_marker(binding: &Binding) -> String {
    format!(
        "<!-- fiddle:decision v1 request={} effect={} payload={} head={} -->",
        binding.request, binding.effect, binding.payload, binding.head_sha
    )
}

/// The **identity of the gated effect** one question is about, re-derived from the
/// design's own definition.
///
/// # Why this exists at all, and what it replaced
///
/// A published marker's `effect` and `request` were asserted only to **differ** from
/// an earlier question's — `assert_ne!(second.request, first.request)`. The design's
/// claim is stronger and is a different claim: a moved head **derives** a new
/// question. An implementation numbering questions from a counter, or hashing a
/// clock, produces differing ids and satisfies every `assert_ne!` ever written about
/// them, so the two forms are not the same property. *Any outcome two different
/// causes produce identically is not an assertion about either of them* — and
/// "differing ids" is produced identically by a derivation over the head and by a
/// counter that knows nothing about it.
///
/// What is already known, and it is not nothing: deriving the id over a **fixed**
/// revision fails 21 tests across the workspace, so the id is demonstrably not
/// constant. What no assertion said before this helper is that it is a *function of
/// the head*.
///
/// # This deliberately does not call `fiddle_core::effect_id`
///
/// [`parse_marker`]'s argument, applied to the arithmetic rather than to the grammar,
/// and it is sharper here. A test computing the expected id with the product's own
/// function **passes on a wrong `effect_id`**: test and product share the defect and
/// neither can see it. The acceptance crate depends on neither `fiddle-core` nor
/// `fiddle-runtime` (`Cargo.toml:7-9`) and carries `blake3` as a dev-dependency
/// *specifically* so derivations come from the design (`Cargo.toml:15-18`). A
/// black-box lane that reaches into the library stops being a second opinion and
/// becomes a mirror.
///
/// [`Scenario::expected_marker`] is the same move on the other identity in the same
/// format. Note that the two are **different objects** and one cannot serve for the
/// other: that one derives the *correlation key*, `blake3(project + NUL +
/// invocation_ref)`, which does not name a head at all.
///
/// # The definition, as the design states it
///
/// ```text
/// target = {repo}#{pr}@{head_sha}
/// effect = blake3(lp[project, invocation_ref, "ensure_pull_request_ready", target])[..16]
/// ```
///
/// where `lp` is the length-prefixed framing [`length_prefixed`] spells out. The kind
/// is written as the literal string the design gives it rather than imported, for the
/// reason every other constant in this file is spelled out: a value that drifted fails
/// here, loudly, instead of agreeing with whatever the product now says.
///
/// Every input is an argument, which is the design's own property of this derivation —
/// nothing is read from outside, so the recomputation is checkable rather than merely
/// plausible. [`World::expected_effect_id`] is the convenience that supplies this
/// world's project and repository.
pub fn expected_effect_id(
    project: &str,
    invocation_ref: &str,
    repo: &str,
    pr: u64,
    head_sha: &str,
) -> String {
    let target = format!("{repo}#{pr}@{head_sha}");
    truncated_digest(&length_prefixed([
        project,
        invocation_ref,
        "ensure_pull_request_ready",
        &target,
    ]))
}

/// The **identity of the question** that gates the effect at `pr`'s `head_sha`,
/// re-derived from the design's own definition.
///
/// ```text
/// request = blake3(lp[project, invocation_ref, effect])[..16]
/// ```
///
/// The head reaches this value only through the effect, which is the design's whole
/// argument for deriving one from the other: *"the gated `EffectId` covers the
/// effect's target, so a moved branch head derives a different effect, which derives a
/// different request id, which is a different question"*. Staleness is then free
/// rather than a rule somebody had to write — and this is the assertion that says the
/// chain really runs through the head rather than around it.
///
/// See [`expected_effect_id`] for why the library is not called, and
/// [`World::expected_request_id`] for the convenience form.
pub fn expected_request_id(
    project: &str,
    invocation_ref: &str,
    repo: &str,
    pr: u64,
    head_sha: &str,
) -> String {
    let effect = expected_effect_id(project, invocation_ref, repo, pr, head_sha);
    truncated_digest(&length_prefixed([project, invocation_ref, &effect]))
}

/// The design's framing: each field becomes its **byte** length, a colon, then the
/// field — `["ab", "c"]` is `2:ab1:c`.
///
/// Restated here rather than joined with a separator byte, because the framing is
/// half of what these two derivations are. The design's reason for it is that a
/// separator whose exclusion rests on convention can be violated by input: with a NUL
/// join, `("a\0b", "c")` and `("a", "b\0c")` name one identity for two different
/// effects. A test that framed its fields the easy way would agree with the product
/// on every well-behaved input and disagree on exactly the inputs the framing exists
/// for.
///
/// **Byte** length and not character count, because the digest is taken over bytes and
/// the two differ for any non-ASCII field. Nothing this lane passes is non-ASCII
/// today, so this line is a value appearing where its value cannot matter — recorded
/// as such rather than presented as tested.
fn length_prefixed<const N: usize>(fields: [&str; N]) -> String {
    let mut material = String::new();
    for field in fields {
        material.push_str(&field.len().to_string());
        material.push(':');
        material.push_str(field);
    }
    material
}

/// The 16-hex-character rendering both identities are truncated to.
///
/// One definition rather than two, so an effect id and a request id derived here
/// cannot drift into different widths — which is also the reason the product has one.
/// [`parse_marker`] checks each field against a fixed width, so a width that moved
/// here would produce an expectation the marker's own grammar rejects.
fn truncated_digest(material: &str) -> String {
    blake3::hash(material.as_bytes()).to_hex()[..16].to_string()
}

/// One comment on the conversation, as the listing returns it.
///
/// A struct rather than a `serde_json::Value` because every field here is one a
/// validation rule turns on, and a typo in a key name should be a compile error
/// rather than a `null` that silently satisfies an assertion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Comment {
    pub id: u64,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    /// The numeric id of whoever wrote it — the field an allowlist matches, and
    /// deliberately not the login beside it.
    pub author: u64,
    /// Whether the author is a bot. fiddle's own question is written by one, and
    /// `validate::select_candidates` refuses a bot as a candidate answer, so this
    /// is what tells a question from a reply.
    pub is_bot: bool,
}

/// What one invocation of the binary did: the exit code, and the two streams.
///
/// Strings rather than bytes because every assertion made against them is a
/// substring search, and a `Vec<u8>` at each call site would be four
/// `from_utf8_lossy` calls that all say the same thing.
pub struct Run {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// One disposable project, one bare "GitHub" reached through the scripted `gh`, a
/// conversation that can be posted to and edited, and a loopback endpoint
/// answering for the model.
///
/// It is the M2 [`Scenario`] with the three things a *decision* walk needs added
/// on top, and it is deliberately a third world rather than a widening of
/// `exactly_once`'s: that one exists to make a mutation land and lose its answer,
/// and every knob it carries — push modes, `commit_then_die`, ambiguity
/// injection — is machinery this lane has no use for. What is shared is shared
/// properly, in `support`: the scripted `gh`, the model gateway, the credential
/// removals, and the project layout.
///
/// # Where each answer comes from
///
/// Every count is read out of the world rather than out of a report. The
/// branches come from a real bare repository's refs; the conversation is read
/// **through the scripted `gh`**, so what a test sees is what a client following
/// `rel="next"` would see rather than what this file happened to write; the
/// requests come from the log the stub keeps of what it was asked.
pub struct World {
    scenario: Scenario,
    /// The scripted `gh`'s scratch directory: its script, its request log, its
    /// world log, the conversation's pages, and the bare repository it answers
    /// ref reads out of.
    stub: PathBuf,
    /// The bare repository that stands in for the remote.
    remote: PathBuf,
    /// The repository a change is produced in and published from.
    work: PathBuf,
    /// The endpoint the model is reached at.
    gateway: StubGateway,
    /// What is exported as the forge credential for this world.
    token: String,
}

impl World {
    /// An empty remote, a conversation that really is empty, an unrepaired
    /// fixture, and a gateway that will drive **two repairs**.
    ///
    /// The empty conversation is said on purpose rather than defaulted. The
    /// scripted `gh` panics on an unscripted page instead of answering an empty
    /// one, because an absent file is an oversight — and a fixture that defaulted
    /// it would let a test assert "no question has been asked" against a world it
    /// never built.
    ///
    /// The script is the deletion scenario's, which is why it is two repairs and
    /// not one; [`World::with_model_script`] is where that choice is argued, and a
    /// scenario driving a *decision* walk gives its own script there instead.
    pub fn new() -> Self {
        World::with_model_script(a_real_repair().into_iter().chain(a_real_repair()).collect())
    }

    /// The same world, with the model answering from `script` instead.
    ///
    /// # Why the script has to be a parameter
    ///
    /// The gateway answers one reply per connection **in order**, and the two walks
    /// this world drives spend their turns on entirely different things: a repair
    /// spends two — a `write_file` call and a report — while a *continuation* spends
    /// one, on interpreting a person's reply, and expects an object with a
    /// `decision` in it. A single script cannot serve both, and the failure is not a
    /// clean one: a continuation handed a repair's first reply reads a tool call
    /// where a verdict should be, and the run refuses for a reason that has nothing
    /// to do with what the test was about.
    ///
    /// So the script is named by the scenario that knows which walk it is driving.
    /// [`World::new`]'s is the deletion scenario's two repairs, and
    /// [`a_suspension_and_its_approval`] is the decision walk's.
    ///
    /// It is the lower half of [`World::new`] rather than a second constructor
    /// beside it, for [`Scenario::std_command`]'s reason: everything else here is
    /// the world both walks share, and a sibling that drifted would let a scenario
    /// assert against a differently-built world than the one it thinks it has.
    pub fn with_model_script(script: Vec<Reply>) -> Self {
        let scenario = Scenario::new();
        scenario.write_work_item(WORK_ID, "open");
        scenario.write_work_item(SECOND_WORK_ID, "open");
        let work = scenario.write_fixture_repo();

        let stub = scenario.dir().join("gh-stub");
        std::fs::create_dir_all(stub.join("script")).unwrap();
        // Empty, and it stays empty: it is what a real `gh` would be pinned to,
        // and beside an absent `HOME` it is what makes an operator's keyring
        // unreachable.
        std::fs::create_dir_all(stub.join("config")).unwrap();
        std::fs::create_dir_all(stub.join(CONVERSATION)).unwrap();
        std::fs::write(stub.join(CONVERSATION).join("page-1.json"), "[]").unwrap();

        // `remote.git` beside the scratch directory is the name the scripted `gh`
        // looks for; see `fiddle-runtime/tests/gh_stub/gh_stub.rs`.
        let remote = stub.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare", "."]);
        git(
            &work,
            &["remote", "add", "origin", &remote.display().to_string()],
        );

        let world = World {
            scenario,
            stub,
            remote,
            work,
            gateway: StubGateway::serving(script),
            token: SENTINEL.to_string(),
        };
        let tables = world.tables();
        world.scenario.append_config(&tables);
        world
    }

    /// Export `token` as the forge credential instead of [`SENTINEL`].
    ///
    /// The builder the bean's sentinel scenario asks for. It changes only what is
    /// *exported*: the document names the variable and never a value, which is
    /// the property `config_check` asserts elsewhere and which this must not
    /// weaken.
    pub fn with_token_sentinel(mut self, token: &str) -> Self {
        self.token = token.to_string();
        self
    }

    /// The `[github]`, `[github.decision]`, `[agent]` and `[workspace]` tables a
    /// decision walk runs against, appended to the M0-shaped document
    /// [`Scenario::new`] wrote.
    ///
    /// Written as one string so the document differs from the milestone baseline
    /// by exactly what this lane adds.
    fn tables(&self) -> String {
        format!(
            "[github]\n\
             repo = \"{REPO}\"\n\
             base = \"{BASE}\"\n\
             token = {{ env = \"{FORGE_CREDENTIAL}\" }}\n\
             cli = {{ program = {gh}, args = [\"--stub-dir\", {stub}] }}\n\
             git = \"git\"\n\
             work = {work}\n\
             config_dir = {config_dir}\n\
             timeout = \"120s\"\n\
             \n\
             [github.decision]\n\
             authorized = [{AUTHORIZED}]\n\
             \n\
             [agent]\n\
             model = \"a-model\"\n\
             base_url = \"{base_url}\"\n\
             api_key = {{ env = \"{MODEL_CREDENTIAL}\" }}\n\
             max_turns = 4\n\
             max_tokens = 512\n\
             max_changed_files = 4\n\
             deadline = \"300s\"\n\
             tool_timeout = \"300s\"\n\
             \n\
             [workspace]\n\
             root = {workspaces}\n\
             fixture = {fixture}\n\
             check = {CHECK}\n\
             command_timeout = \"300s\"\n",
            gh = toml_string(gh_stub_binary()),
            stub = toml_string(&self.stub),
            work = toml_string(&self.work),
            config_dir = toml_string(&self.stub.join("config")),
            base_url = self.gateway.base_url(),
            workspaces = toml_string(&self.workspace_root()),
            fixture = toml_string(&self.work),
        )
    }

    // -- driving the binary --------------------------------------------------

    /// `fiddle <args> --config <this world's document>`, with both credentials
    /// exported, run to completion and handed back unjudged.
    pub fn fiddle<const N: usize>(&self, args: [&str; N]) -> Run {
        self.launch(args, true)
    }

    /// The same, with **no** credential in the environment at all.
    ///
    /// The stronger half of the read-only guarantee: removal shows a command does
    /// not *need* a credential, which is what makes it safe to run against
    /// anything.
    pub fn fiddle_without_credentials<const N: usize>(&self, args: [&str; N]) -> Run {
        self.launch(args, false)
    }

    fn launch<const N: usize>(&self, args: [&str; N], credentialled: bool) -> Run {
        let mut command = self.command(args, credentialled);
        let out = command.output().unwrap();
        Run {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    /// The invocation [`World::fiddle`] runs, spawnable.
    ///
    /// The lower half rather than a second builder, so the `--config` argument and
    /// the credential decision are spelled once. One scenario needs a pid — see
    /// [`World::interrupt_a_repair_inside_its_worktree`].
    fn command<const N: usize>(
        &self,
        args: [&str; N],
        credentialled: bool,
    ) -> std::process::Command {
        let mut command = std::process::Command::new(fiddle_binary());
        // The four credential-shaped names no lane may need, and then the two this
        // world's own document names. Removed unconditionally and *before* the
        // conditional export below, so a credentialled run still ends up with
        // exactly the two values it means to hand over — and a credential-free one
        // inherits neither from the environment the tests were launched in.
        for name in CREDENTIAL_VARS.iter().chain(WORLD_CREDENTIAL_VARS.iter()) {
            command.env_remove(name);
        }
        command.args(args);
        command.args(["--config", self.scenario.config_path().to_str().unwrap()]);
        if credentialled {
            for name in WORLD_CREDENTIAL_VARS {
                command.env(name, &self.token);
            }
        }
        command
    }

    /// What [`World::command`] does to the child's environment, as a map from name
    /// to the value it sets — or `None` where it removes one.
    ///
    /// Exposed so the removal can be asserted at all. It is the harness's own
    /// construction rather than a running child's environment, and
    /// `a_credential_free_run_removes_every_variable_this_worlds_document_names`
    /// says why that is the right half to pin: the only thing that can observe a
    /// child's variables from outside is the scripted `gh`'s environment recorder,
    /// and no capability this build can execute reaches it.
    pub fn credential_environment(
        &self,
        credentialled: bool,
    ) -> std::collections::BTreeMap<String, Option<String>> {
        self.command(["--version"], credentialled)
            .get_envs()
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.map(|value| value.to_string_lossy().into_owned()),
                )
            })
            .collect()
    }

    /// The text of this world's own configuration document, so a test can tie a
    /// fixture value to the table that gives it meaning.
    pub fn config_text(&self) -> String {
        self.scenario.config_text()
    }

    /// The report bundle one run published, parsed.
    ///
    /// Reached the way a downstream reader would: the `--json` payload names its
    /// bundle in `report`, and the path is resolved against `<report.dir>` rather
    /// than reconstructed here. A run whose payload pointed somewhere unreadable
    /// fails in [`Scenario::read_bundle`] instead of being papered over.
    ///
    /// The **bundle** and not the payload, because the two carry different things:
    /// `run --json` prints the outcome, the next action, the executions and the
    /// progress, and it does not print `attempt_id` or `work_ref` — those are the
    /// bundle's. So a test asking which attempt this was has to open what the run
    /// published, which is also the honest place to ask it from.
    pub fn bundle(&self, run: &Run) -> serde_json::Value {
        let payload: serde_json::Value = serde_json::from_str(&run.stdout).unwrap_or_else(|e| {
            panic!("stdout is not JSON ({e}): {}", run.stdout);
        });
        self.scenario.read_bundle(&payload)
    }

    /// The correlation key this world's project and `invocation_ref` must produce.
    ///
    /// Delegated to [`Scenario::expected_marker`] rather than recomputed, so the
    /// derivation has one home — and that home computes it from the design's own
    /// definition instead of calling `fiddle_core::correlation_key`, which is what
    /// keeps an assertion about a marker a check against the specification rather
    /// than against the binary's own arithmetic.
    ///
    /// The reference is an argument and not the module constant, because a world can
    /// be handed more than one and a helper that quietly assumed which would be the
    /// same shape of mistake it exists to catch.
    pub fn expected_marker(&self, invocation_ref: &str) -> String {
        self.scenario.expected_marker(invocation_ref)
    }

    /// The gated effect's identity for `pr` at `head_sha`, in this world.
    ///
    /// [`expected_effect_id`] with this world's project and repository supplied and
    /// everything a scenario chooses left as an argument. The free function carries
    /// the derivation and the argument for not importing it; this exists so a call
    /// site reads as the three facts the scenario is making a claim about — which
    /// pull request, at which revision, under which reference — rather than as five
    /// arguments of which two are always the same.
    ///
    /// `pr` and `head_sha` are arguments and never read out of the world here, for
    /// [`World::expected_marker`]'s reason: the whole point is to compare the
    /// published marker against a revision the *scenario* names from its own
    /// observation of the remote, and a helper that fetched the current head would
    /// be asserting that the world agrees with itself.
    pub fn expected_effect_id(&self, invocation_ref: &str, pr: u64, head_sha: &str) -> String {
        expected_effect_id(PROJECT_NAME, invocation_ref, REPO, pr, head_sha)
    }

    /// The question's identity for `pr` at `head_sha`, in this world.
    ///
    /// [`expected_request_id`]'s convenience form; see [`World::expected_effect_id`]
    /// for why the two constants are supplied and the rest is not.
    pub fn expected_request_id(&self, invocation_ref: &str, pr: u64, head_sha: &str) -> String {
        expected_request_id(PROJECT_NAME, invocation_ref, REPO, pr, head_sha)
    }

    /// Which attempt one run turned out to be.
    ///
    /// Minted inside `fiddle_runtime::attempt`, once per process, so two runs over
    /// one piece of work carry two of these — which is the half of M2's neighbouring
    /// property a three-process walk restates. Read off the bundle, because that is
    /// the only place it is published.
    pub fn attempt_id(&self, run: &Run) -> String {
        self.bundle(run)["attempt_id"]
            .as_str()
            .unwrap_or_else(|| panic!("a bundle names its attempt: {}", self.bundle(run)))
            .to_string()
    }

    /// Which piece of work one run was about.
    ///
    /// The other half of the same property, and the one that must **not** move: the
    /// two processes are two attempts against one work ref. `Option` on the bundle
    /// and a panic here, because every run this accessor is asked about observed its
    /// work — a `null` would be a run that saw nothing, which is a different
    /// scenario and should not be silently compared equal to another one.
    pub fn work_ref(&self, run: &Run) -> String {
        let bundle = self.bundle(run);
        bundle["work_ref"]
            .as_str()
            .unwrap_or_else(|| panic!("a bundle over observed work names it: {bundle}"))
            .to_string()
    }

    /// `fiddle run … --capability fixture_repair --json`, handed back unjudged.
    ///
    /// The one capability this build can execute that produces all three kinds of
    /// local record a suspension would — a published bundle, an attempt journal
    /// entry, and a workspace — which is why the deletion helpers are proven
    /// against it rather than against a suspension they cannot yet reach.
    pub fn repair(&self) -> Run {
        self.fiddle([
            "run",
            "--capability",
            "fixture_repair",
            INVOCATION_REF,
            "--json",
        ])
    }

    // -- what the world holds ------------------------------------------------

    /// Every branch the remote holds, in ref order.
    ///
    /// Read out of a real bare repository's refs rather than out of a report: a
    /// bundle saying "one branch" is fiddle's opinion, and this is the world's.
    pub fn remote_branches(&self) -> Vec<String> {
        let refs = git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        );
        match refs.is_empty() {
            true => Vec::new(),
            false => refs.lines().map(str::to_string).collect(),
        }
    }

    /// Every request the scripted `gh` recorded, in arrival order — argv, request
    /// body and the whole environment the child received.
    pub fn requests(&self) -> Vec<serde_json::Value> {
        walkdir_files(self.stub.join("requests"))
            .iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .filter_map(|text| serde_json::from_str(&text).ok())
            .collect()
    }

    /// The REST path of every request the forge received, in arrival order.
    ///
    /// This is the recorder a test asserts a **negative** with: that an endpoint
    /// was never consulted. So it is deliberately the widest record available —
    /// every call the stub was launched for, whatever it asked and whatever came
    /// back — rather than the writes that landed.
    ///
    /// **What a test using this cannot distinguish:** a GraphQL call. Every one is
    /// a `POST /graphql` carrying its question in a `-f query=…` argument and no
    /// path at all, so none appears here; [`World::graphql_calls`] is that
    /// route's counter, and it is a separate one for the same reason the stub
    /// answers the route from a separate script.
    pub fn requested_paths(&self) -> Vec<String> {
        self.requests()
            .iter()
            .filter_map(|request| {
                request["argv"].as_array()?.iter().find_map(|arg| {
                    let arg = arg.as_str()?;
                    arg.starts_with('/').then(|| arg.to_string())
                })
            })
            .collect()
    }

    /// The bodies of the comments this world was asked to post, oldest first.
    ///
    /// # Why the name says `_bodies` instead of matching the runtime crate
    ///
    /// `posted_comments` is already overloaded there, and not by accident of one
    /// author: `decision_request_effect.rs:285` returns **`usize`**,
    /// `propose_capability.rs:415` returns **`Vec<String>`**, and `gh_stub.rs:1104`
    /// returns **`Vec<serde_json::Value>`**. Three meanings, one name. So "match
    /// the runtime crate" is not a rule that can be followed — the runtime crate
    /// does not agree with itself — and this bean's Step 1, which writes
    /// `assert_eq!(world.posted_comments(), 0)` as though it were a count, is what
    /// that ambiguity costs a reader.
    ///
    /// Naming the return value resolves it instead of picking a side, and it means
    /// a reader of a *later* bean's tests does not have to know which crate they
    /// are in to know what the call answers. Cardinality is `.len()` at the call
    /// site, which is one word and cannot be misread.
    ///
    /// Read from the **requests** rather than from the conversation, because that
    /// is the number a run which asked twice would move: a listing merges what
    /// landed, and a post whose answer was lost landed all the same.
    pub fn posted_comment_bodies(&self) -> Vec<String> {
        self.requests()
            .iter()
            .filter(|request| {
                let argv = argv_of(request);
                argv.iter().any(|a| a == "POST")
                    && argv.iter().any(|a| a.trim_end().ends_with("/comments"))
            })
            .filter_map(|request| {
                let body: serde_json::Value =
                    serde_json::from_str(request["body"].as_str().unwrap_or("{}")).ok()?;
                Some(body["body"].as_str().unwrap_or_default().to_string())
            })
            .collect()
    }

    // -- the conversation ----------------------------------------------------

    /// Post a comment as `author`, and hand back the id it was given.
    ///
    /// This is a *person* writing, not fiddle: it goes straight onto the pages the
    /// listing is answered from, because no capability in this build posts on
    /// somebody else's behalf and a fixture that faked one through the forge's
    /// write path would be recording a request nobody made.
    ///
    /// # The id
    ///
    /// One above the highest id the conversation currently shows, fiddle's own
    /// posted comments included. That is what makes the collection ordered by id
    /// in *time* order, which every candidate rule in the validation order depends
    /// on: `validate::select_candidates` decides what is a reply by comparing ids,
    /// so a reply seeded below the question it answers would be invisible and one
    /// seeded above a comment that predates it would be a false candidate.
    ///
    /// **Corrected: this used to carry a "what it cannot promise" clause** — that the
    /// stub numbered its own comments from a fixed base and knew nothing about what
    /// was seeded, *"so a run that wrongly posted a second question after a reply was
    /// seeded could collide with it"*. Two things about that were wrong. The collision
    /// needed no misbehaving run: a **legitimate** redirect posts a second question
    /// after a reply, and the converged
    /// `human_direction::a_redirect_produces_a_different_change_and_asks_again_about_it`
    /// was already leaving such a world behind. And it was not harmless: `gh_stub`'s
    /// `comment_by_id` reports a duplicate rather than choosing from it, and step 5 of
    /// the decision walk re-reads by id, so a third process could not be driven at all.
    ///
    /// `gh_stub::apply_effect` now mints a posted comment's id **at post time**, above
    /// everything the world holds, so this rule and that one are one rule and there is
    /// nothing left to promise around. [`World::posted_comment_bodies`], counted off the
    /// request log, remains the accessor for "did a run ask twice" — not because ids
    /// are unreliable but because a listing merges what landed and a post whose answer
    /// was lost landed all the same.
    pub fn post_comment(&self, author: u64, body: &str) -> u64 {
        self.write_listed_comment(author, body, "User", serde_json::Value::Null)
    }

    /// Post a comment whose author is an account of type `Bot`, and hand back its
    /// id.
    ///
    /// The first of the two spellings of not being a person, and it takes an
    /// `author` rather than assuming one **because the id is what must not be the
    /// reason it is refused.** A bot reply written by [`STRANGER`] would be declined
    /// by the allowlist, so the row would pass with
    /// `Ignored::NotAPerson` deleted from the product entirely. Written by
    /// [`AUTHORIZED`], the only thing standing between it and a mutation is its
    /// type, which is the claim.
    ///
    /// Distinct from [`World::seed_question`], which also writes a `Bot`: that one
    /// is fiddle's own question and carries a marker, and it is refused as
    /// `Ignored::RequestComment` before personhood is ever consulted. These two
    /// must not be collapsed — the request comment's exclusion is a different rule
    /// from a bot's.
    pub fn post_bot_comment(&self, author: u64, body: &str) -> u64 {
        self.write_listed_comment(author, body, "Bot", serde_json::Value::Null)
    }

    /// Post a comment a `User` wrote but an **app** is recorded as having performed,
    /// and hand back its id.
    ///
    /// The second spelling, and the one a reader is most likely to think is a
    /// person: `user.type` is `User`, the login is a person's, and the id is on the
    /// allowlist. Only `performed_via_github_app` says otherwise. That is the case
    /// [`POSTING_APP`] exists for — an automation writing through somebody's
    /// credential — and it is why the two spellings are two rows of the matrix
    /// rather than one.
    pub fn post_app_comment(&self, author: u64, body: &str) -> u64 {
        self.write_listed_comment(author, body, "User", POSTING_APP())
    }

    /// The one writer behind [`World::post_comment`] and its two non-person
    /// siblings.
    ///
    /// One function rather than three near-copies, because the three differ in
    /// exactly the two fields the product's `is_bot` is computed from and everything
    /// else about them — the id rule, the page they land on, the equal timestamps —
    /// is a property the matrix depends on being identical. Three hand-written
    /// bodies that drifted would let a row be refused for a reason its scenario did
    /// not choose.
    fn write_listed_comment(
        &self,
        author: u64,
        body: &str,
        kind: &str,
        app: serde_json::Value,
    ) -> u64 {
        let id = self.conversation().iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let page = self.last_page();
        let mut listed = self.page(page);
        listed.push(serde_json::json!({
            "id": id,
            "body": body,
            "created_at": SEEDED_AT,
            "updated_at": SEEDED_AT,
            "author_association": "COLLABORATOR",
            "user": {"login": format!("user-{author}"), "id": author, "type": kind},
            "performed_via_github_app": app,
        }));
        self.write_page(page, &listed);
        id
    }

    /// Put a comment in the **inline review** collection — `/pulls/{n}/comments` —
    /// and hand back its id.
    ///
    /// A different collection from the conversation, and the whole point is that
    /// nothing reads it. `github/comments.rs` offers no route that names it, so an
    /// approval typed there is unreachable rather than filtered, and the assertion a
    /// scenario makes about it is that the endpoint was **never asked for**.
    ///
    /// Numbered from [`FIRST_REVIEW_COMMENT`], above the question, so that a defect
    /// which did merge this collection onto the conversation would produce a
    /// *candidate* and mutate. See that constant for why numbering it below would
    /// make the row unfalsifiable.
    pub fn post_review_comment(&self, author: u64, body: &str) -> u64 {
        let dir = self.stub.join(REVIEW_COMMENTS);
        std::fs::create_dir_all(&dir).unwrap();
        let page = dir.join("page-1.json");
        let mut listed: Vec<serde_json::Value> = std::fs::read_to_string(&page)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        let id = FIRST_REVIEW_COMMENT + listed.len() as u64;
        listed.push(serde_json::json!({
            "id": id,
            "body": body,
            "created_at": SEEDED_AT,
            "updated_at": SEEDED_AT,
            "author_association": "COLLABORATOR",
            "user": {"login": format!("user-{author}"), "id": author, "type": "User"},
            "performed_via_github_app": serde_json::Value::Null,
        }));
        std::fs::write(&page, serde_json::Value::Array(listed).to_string()).unwrap();
        id
    }

    /// Seed a comment on the conversation **as fiddle**, and hand back its id.
    ///
    /// # This constructs a scripted value; it does not observe one
    ///
    /// Said plainly because the neighbouring mistake has already been made twice
    /// on this milestone: `propose_capability`'s `readied()` is a constructor of a
    /// GraphQL response and three beans read its name as an observer of what had
    /// been readied. This is a constructor. A question seeded here was asked by
    /// nobody — [`World::posted_comment_bodies`], read off the request log, is the
    /// accessor that says what a *run* really asked.
    ///
    /// It exists because [`World::the_only_request_comment`] is a cardinality
    /// assertion and `run --capability propose_change` cannot yet publish a
    /// question, so without it that accessor would ship to `fiddle-565u` with no
    /// test able to reach it at all. An inversion confirmed exactly that: degrading
    /// it to "the first comment in the conversation" broke nothing.
    ///
    /// The shape is the one the scripted `gh` lists a run's own comment under — a
    /// `Bot` author, equal timestamps — because `is_bot` is the field
    /// `validate::select_candidates` refuses a candidate answer on, and a seeded
    /// question that looked like a person's would be a different object.
    pub fn seed_question(&self, body: &str) -> u64 {
        let id = self.conversation().iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let page = self.last_page();
        let mut listed = self.page(page);
        listed.push(serde_json::json!({
            "id": id,
            "body": body,
            "created_at": SEEDED_AT,
            "updated_at": SEEDED_AT,
            "author_association": "OWNER",
            "user": {"login": "fiddle[bot]", "id": FIDDLE_BOT, "type": "Bot"},
            "performed_via_github_app": serde_json::Value::Null,
        }));
        self.write_page(page, &listed);
        id
    }

    /// Rewrite the comment `id` names, moving its `updated_at` away from its
    /// `created_at`.
    ///
    /// The divergence is the point rather than the new body: `validate` refuses a
    /// comment whose two stamps differ, because a reply edited after it was read
    /// is no longer the reply that was read. Nothing in fiddle edits a comment, so
    /// this is a person's action and it is the fixture that has to be able to
    /// express it.
    ///
    /// Panics on an id the pages do not hold — including one the *stub* assigned
    /// to a comment a run posted. Those live in the world log and are stamped by
    /// the stub with two equal times, so an edit of one could not be expressed
    /// here and silently editing nothing would be the failure this whole bean is
    /// about.
    pub fn edit_comment(&self, id: u64, body: &str) {
        for page in 1..=self.last_page() {
            let mut listed = self.page(page);
            let found = listed
                .iter_mut()
                .find(|comment| comment["id"].as_u64() == Some(id));
            if let Some(comment) = found {
                comment["body"] = serde_json::Value::String(body.to_string());
                comment["updated_at"] = serde_json::Value::String(EDITED_AT.to_string());
                self.write_page(page, &listed);
                return;
            }
        }
        panic!(
            "no seeded comment {id} on the conversation; the pages hold {:?}",
            self.conversation().iter().map(|c| c.id).collect::<Vec<_>>()
        );
    }

    /// The whole conversation, oldest page first, **as a client reading it would
    /// see it**.
    ///
    /// Read through the scripted `gh` and paged by following `rel="next"` to the
    /// end, rather than by concatenating the files this file wrote. That costs a
    /// subprocess per page and buys the thing that matters: the comments a *run*
    /// posted are merged onto the listing by the stub, so a `conversation` built
    /// from the page files would not show fiddle's own question and a test
    /// asserting it was published would fail against a run that had published it.
    ///
    /// **What a test using this cannot distinguish:** the order the listing
    /// returns from the order the ids imply. They are allowed to disagree — the
    /// stub merges a run's posts onto the *last* page whatever their ids — and
    /// that disagreement is deliberate, because it is what keeps "after the
    /// question" and "later in the vector" from being the same observation.
    /// Assertions about order should be made about `id`.
    pub fn conversation(&self) -> Vec<Comment> {
        self.listed_comments().iter().map(comment_from).collect()
    }

    /// The whole conversation as the listing really returned it, unparsed.
    ///
    /// [`World::conversation`]'s own source, and separate because two callers need
    /// two different things from one read. A scenario asserting a property wants
    /// [`Comment`], where a mistyped key is a compile error; a scenario *rewriting* a
    /// comment needs every field the product deserializes — `author_association`,
    /// `user`, `performed_via_github_app` — including the ones no assertion here
    /// mentions. [`World::edit_comment_on_next_read`] builds its file from this, so a
    /// re-read differs from its listing in the field the scenario chose and in no
    /// other: a by-id file assembled from a `Comment` would silently drop three keys
    /// and the product would refuse it as unreadable, which is a refusal about the
    /// fixture rather than about an edit.
    fn listed_comments(&self) -> Vec<serde_json::Value> {
        let mut found = Vec::new();
        let mut page = 1;
        loop {
            let response = self.listing(page);
            found.extend(body_of(&response));
            if !response.contains("rel=\"next\"") {
                return found;
            }
            page += 1;
        }
    }

    /// Make the **re-read** of comment `id` answer `body`, edited, while the listing
    /// goes on saying what it said.
    ///
    /// # The one thing step 5 exists to catch, and the only way to express it
    ///
    /// `validate`'s `reread` refuses when a comment's `updated_at` differs from the
    /// one the listing carried, because a reply rewritten after it was selected is
    /// not the reply that was selected. That is a disagreement *between two reads*,
    /// so no single-source fixture can produce it: [`World::edit_comment`] rewrites
    /// the page, which both reads then agree about, and a walk over it sees a comment
    /// that was simply composed in two passes — which a person is entitled to do.
    ///
    /// The scripted `gh` answers a by-id read from `<conversation>/by-id/{id}.json`
    /// **in preference to** the listing, precisely so a scenario can say *this is
    /// what the re-read returns, whatever the listing says*. This writes that file.
    ///
    /// It is built from the listing's own entry rather than from a fresh object, so
    /// the two reads differ in `body` and `updated_at` and are identical everywhere
    /// else. That is what makes the refusal attributable: a file differing in five
    /// fields would refuse for whichever the product happened to check first.
    ///
    /// Panics on an id the conversation does not list — including one this world
    /// never held — because silently scripting a re-read nobody performs is the
    /// vacuous version of this whole scenario.
    pub fn edit_comment_on_next_read(&self, id: u64, body: &str) {
        self.script_the_re_read(id, |comment| {
            comment["body"] = serde_json::Value::String(body.to_string());
            comment["updated_at"] = serde_json::Value::String(EDITED_AT.to_string());
        });
    }

    /// Make the re-read of comment `id` report that it was edited **before** this walk
    /// began, without the window between the two reads having moved at all.
    ///
    /// # Why this is a second capability rather than a variation of the first
    ///
    /// The product has two refusals about fiddle's own question and they are different
    /// claims. `reread` refuses when `updated_at` moved *between the listing and the
    /// re-read*; a separate check refuses when `created_at != updated_at` on the
    /// re-read, which is a claim about the comment's **whole history** — the rule's own
    /// documentation says "an edit made long before this walk started fails it too".
    ///
    /// **An inversion proved the second rule was untested.** Deleting the `created_at
    /// != updated_at` check broke nothing, because [`World::edit_comment_on_next_read`]
    /// moves `updated_at` and is therefore caught by the *first* rule first — the same
    /// mechanism the edited-approval scenario already exercises. So the scenario meant
    /// to cover fiddle's question being rewritten was covering the reply rule twice.
    ///
    /// This isolates the second rule by leaving `updated_at` **equal to the listing's**
    /// and moving `created_at` back instead. The window did not move, so the first rule
    /// has nothing to say; the stamps disagree, so only the second can refuse.
    ///
    /// It also makes the **product's** `HumanResponse::created_at` discriminating for the
    /// first time — and **not** the fixture's own. [`WRITTEN_BEFORE_AN_EDIT`] reaches only
    /// the by-id re-read file, which is what step 5 compares; `comment_from` reads the
    /// *listing*, where both stamps are still `SEEDED_AT`, so `fiddle-pwyi`'s null on that
    /// accessor stands unchanged. See that constant for why the distinction is recorded
    /// rather than glossed.
    pub fn show_as_edited_before_the_listing(&self, id: u64) {
        self.script_the_re_read(id, |comment| {
            comment["created_at"] = serde_json::Value::String(WRITTEN_BEFORE_AN_EDIT.to_string());
        });
    }

    /// Write the by-id file the next re-read of `id` is answered from, starting from
    /// the entry the listing really holds.
    ///
    /// The shared half of the two capabilities above. It starts from the listing so the
    /// two reads differ in exactly the fields the caller touched and are identical
    /// everywhere else — a file assembled from scratch would refuse for whichever key
    /// the product happened to miss, which is a refusal about the fixture.
    ///
    /// The listing's `updated_at` is asserted unedited first: both callers are
    /// expressing a divergence *from* the unedited state, and one built on a comment
    /// this file had already edited would be measuring against the wrong baseline.
    fn script_the_re_read(&self, id: u64, edit: impl FnOnce(&mut serde_json::Value)) {
        let listed = self.listed_comments();
        let mut comment = listed
            .iter()
            .find(|comment| comment["id"].as_u64() == Some(id))
            .unwrap_or_else(|| {
                panic!(
                    "the conversation lists no comment {id}, so nothing would re-read \
                     one; it lists {:?}",
                    listed
                        .iter()
                        .filter_map(|c| c["id"].as_u64())
                        .collect::<Vec<_>>()
                )
            })
            .clone();
        assert_eq!(
            comment["updated_at"].as_str(),
            Some(SEEDED_AT),
            "the listing must still carry the unedited stamp, or the divergence this \
             writes is not a divergence: {comment}"
        );
        edit(&mut comment);

        let dir = self.stub.join(CONVERSATION).join(BY_ID);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.json")), comment.to_string()).unwrap();
    }

    /// Tell the forge its pull request is at a different revision, and hand back the
    /// one it used to answer with.
    ///
    /// # A moved head is not refused at all, and this comment used to say it was
    ///
    /// **Corrected: an earlier version of this doc claimed the walk refuses at step 2
    /// with `DecisionError::RequestAbsent`, "before a single sha is compared".** That is
    /// wrong, and it was wrong in the file a reader consults first while the test that
    /// drives this helper had the right account. Two independent inversions refuted it —
    /// `panic!` on entry to `human::validate::resolve` fails 7 of 22 tests in
    /// `human_direction` and leaves the moved-head scenario **passing** — so the
    /// validation order is never entered and neither step 2's `RequestAbsent` nor step
    /// 6's `HeadMoved` is reached.
    ///
    /// What actually happens is upstream of `resolve`. The gated effect's target is
    /// `{repo}#{pr}@{head}` and the request id derives over that target, so a run
    /// reading a different head derives a different request id;
    /// `PublishDecisionRequest::inspect` finds no comment carrying *that* marker,
    /// answers `None`, and the capability takes the **first** walk — it publishes a
    /// second question and suspends at exit 10.
    ///
    /// That is a stronger property than a sha check would be, and it is why the scenario
    /// is worth having: the old approval is not weighed and declined, it is
    /// **unrecognisable** as an answer to any question this run knows how to ask.
    ///
    /// `an_approval_for_a_head_that_has_moved_is_unrecognisable_not_merely_rejected`
    /// carries the measured account and asserts it. Prefer it to this summary, and note
    /// that the same claim is *true* of `resolve` called directly — which is what
    /// `fiddle-runtime`'s `decision_protocol` asserts — and false of this path. That
    /// difference is what made the wrong version plausible for three rounds.
    ///
    /// The previous revision is returned, and the two are asserted to differ, because
    /// a "move" to the same value is the check that cannot fail.
    pub fn move_pull_request_head(&self, number: u64, sha: &str) -> String {
        let path = self
            .stub
            .join("pulls_by_number")
            .join(format!("{number}.json"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "no seeded answer for pull request {number} at {} ({error}); a head \
                 cannot move away from a revision nothing ever answered",
                path.display()
            )
        });
        let mut seeded: serde_json::Value = serde_json::from_str(&text).unwrap();
        let was = seeded["head"]["sha"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        assert_ne!(
            was, sha,
            "a head that moved to where it already was is not a case"
        );
        seeded["head"]["sha"] = serde_json::Value::String(sha.to_string());
        std::fs::write(&path, seeded.to_string()).unwrap();
        was
    }

    /// Rewrite the body of the question **this world's own run posted**, as every
    /// read of it will now see.
    ///
    /// # Not [`World::edit_comment`], and not [`World::edit_comment_on_next_read`]
    ///
    /// Three different acts, and the matrix needs this one distinctly. `edit_comment`
    /// rewrites a page file and **panics on a comment a run posted** — fiddle's
    /// question lives in the stub's world log, not in the pages, so it cannot reach
    /// it at all. `edit_comment_on_next_read` makes the two reads disagree, which is
    /// refused on a *timestamp* without the rewritten bytes ever being weighed.
    ///
    /// This rewrites the log entry the listing and the by-id fallback are both derived
    /// from, so both reads agree and the timestamps stay equal. The only thing wrong
    /// with the comment is **what it says** — which is the case a marker somebody
    /// rewrote actually is, and the one where the product has to refuse on its own
    /// recomputation rather than on evidence of an edit.
    ///
    /// Panics unless the world holds exactly one posted question, because a rewrite
    /// that picked between two would be choosing which run's history to falsify.
    pub fn rewrite_the_published_question(&self, body: &str) {
        let log = self.stub.join("world");
        let text = std::fs::read_to_string(&log)
            .unwrap_or_else(|error| panic!("no world log at {} ({error})", log.display()));
        let mut rewritten = Vec::new();
        let mut found = 0;
        for line in text.lines() {
            let Ok(mut landed) = serde_json::from_str::<serde_json::Value>(line) else {
                rewritten.push(line.to_string());
                continue;
            };
            let posted_a_comment = landed["key"]
                .as_str()
                .is_some_and(|key| key.starts_with("POST_repos_") && key.ends_with("_comments"));
            if !posted_a_comment {
                rewritten.push(line.to_string());
                continue;
            }
            found += 1;
            // The log stores the request body as the *string* that was sent, and
            // `posted_comments` re-parses it, so the rewrite has to go back through
            // the same encoding rather than replacing a substring of the line.
            let mut sent: serde_json::Value =
                serde_json::from_str(landed["body"].as_str().unwrap_or("{}")).unwrap();
            sent["body"] = serde_json::Value::String(body.to_string());
            landed["body"] = serde_json::Value::String(sent.to_string());
            rewritten.push(landed.to_string());
        }
        assert_eq!(
            found, 1,
            "exactly one posted question may be rewritten, and this world holds \
             {found}"
        );
        std::fs::write(&log, format!("{}\n", rewritten.join("\n"))).unwrap();
    }

    /// Rewrite **one field of the marker** on the question this world's run published,
    /// and hand back the binding a reader will now find there.
    ///
    /// # Why a field and not a body
    ///
    /// [`World::rewrite_the_published_question`] can already replace the whole comment,
    /// and that is the wrong instrument for the case step 3 exists for. A marker whose
    /// **effect** field was edited while its **request** field was left alone is still
    /// found: `PublishDecisionRequest::is_this_request` compares only the request id, so
    /// `inspect` recognises the comment, the walk enters the validation order, and step 3
    /// — the recomputation of the effect from four values the conversation does not carry
    /// — is what refuses. A scenario that rewrote more than one field would be refused
    /// for whichever difference the product happened to check first, and the attribution
    /// is the whole claim.
    ///
    /// So the caller is handed the parsed [`Binding`] and edits exactly what it means to.
    ///
    /// # Two assertions, and each closes a way this could quietly do nothing
    ///
    /// The edit must **change** the binding, because a marker rewritten to what it
    /// already said is the check that cannot fail. And the marker this file renders from
    /// the old binding must be **present in the body byte for byte**, which is what says
    /// [`render_marker`]'s grammar and the product's rendering still agree: without it, a
    /// drift in either would make this replace nothing and the scenario would pass
    /// against an untouched question.
    pub fn rewrite_the_published_marker(&self, edit: impl FnOnce(&mut Binding)) -> Binding {
        let comment = self.the_only_request_comment();
        let was = parse_marker(&comment.body)
            .unwrap_or_else(|reason| panic!("the published question carries a marker: {reason}"));
        let mut now = was.clone();
        edit(&mut now);
        assert_ne!(
            now, was,
            "a marker rewritten to what it already said is not a case"
        );
        let rendered = render_marker(&was);
        assert!(
            comment.body.contains(&rendered),
            "the body must carry the marker this file renders, or the grammar here and \
             the product's have drifted and this would rewrite nothing.\nrendered: \
             {rendered}\nbody: {}",
            comment.body
        );
        self.rewrite_the_published_question(&comment.body.replace(&rendered, &render_marker(&now)));
        now
    }

    /// Make the `POST` that publishes the question **land and then lose its answer**.
    ///
    /// # The provenance this world could not produce
    ///
    /// The scripted `gh` has carried `commit_then_die` since M2 — the write is applied
    /// and *then* the process ends without answering, in that order, because a stub that
    /// exited first would be testing a failed write. Nothing in this world could reach
    /// it: `commit_then_*` is scripted per REST key, and no accessor here wrote a script
    /// file at all. So the milestone's central rule — settle a lost answer by reading,
    /// never by asking again — was asserted for the *question* only in
    /// `fiddle-runtime`'s `decision_request_effect`, which drives the operation through
    /// the executor and therefore cannot see a caller that goes around it.
    ///
    /// The key is derived from the same repository and conversation the capability
    /// addresses, mangled the way `gh_stub::script_key` mangles a path, so a script
    /// cannot come to name a path nothing requests. `201` because that is what the
    /// endpoint answers; the exit field is unused for a `commit_then_*` mode, which ends
    /// the process itself.
    ///
    /// **It stays armed for every later process**, deliberately. A continuation that
    /// wrongly posted a second question would die the same way, and the world would then
    /// hold two comments — which is what [`World::request_comments`] is read for. A knob
    /// that disarmed itself would make "the second run did not ask again" a claim about
    /// the fixture's bookkeeping instead of about the conversation.
    pub fn lose_the_answer_to_the_question(&self) {
        let key = format!(
            "POST_{}",
            format!("repos/{REPO}/issues/{CONVERSATION_ISSUE}/comments").replace('/', "_")
        );
        std::fs::write(self.stub.join("script").join(key), "201 0 commit_then_die").unwrap();
    }

    /// Script the answer to GraphQL call `n` to **land and then lose its answer**.
    ///
    /// The same provenance as [`World::lose_the_answer_to_the_question`] on the other
    /// write route, and it has to be scripted separately for the reason
    /// [`World::script_graphql`] states: every GraphQL call is one `POST /graphql`, so
    /// the REST script derives one key for all of them and could not tell a refusal from
    /// a lost answer. The ending rides in `graphql/{n}.json` beside the status and the
    /// body.
    ///
    /// The body is the accepted one, because that is what "the mutation landed" means
    /// here — a GraphQL verdict lives in the body, so a mutation that lands and loses its
    /// answer is an accepted body the client never got to read.
    pub fn lose_the_answer_to_the_ready_mutation(&self) {
        let dir = self.stub.join("graphql");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("0.json"),
            serde_json::json!({
                "status": 200,
                "body": {
                    "data": { "markPullRequestReadyForReview": { "clientMutationId": null } }
                },
                "mode": "commit_then_die",
            })
            .to_string(),
        )
        .unwrap();
    }

    /// The keys of every write that landed **under a `gh` that then failed to answer**,
    /// in arrival order.
    ///
    /// Read out of the stub's world log, where the mode is recorded beside the mutation,
    /// and it earns its place for `gh_stub::apply_effect`'s stated reason: it is how a
    /// test asserts, *of the world it is making its claims about*, that a write landed
    /// ambiguously. Without it a scenario could only script the ambiguity and hope the
    /// run under test took that route — and a test that would pass on a request which
    /// simply succeeded is not yet a test of the ambiguous one.
    pub fn landed_ambiguously(&self) -> Vec<String> {
        std::fs::read_to_string(self.stub.join("world"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|landed| {
                landed["mode"]
                    .as_str()
                    .is_some_and(|mode| mode.starts_with("commit_then_"))
            })
            .filter_map(|landed| landed["key"].as_str().map(str::to_string))
            .collect()
    }

    /// The comments on the conversation that fiddle wrote, which are its
    /// questions.
    ///
    /// Told apart by their author being a bot and not by their content. That is
    /// the field `validate::select_candidates` refuses a candidate on, so a
    /// fixture that distinguished them by looking for a marker in the body would
    /// be agreeing with the code under test rather than checking it.
    pub fn request_comments(&self) -> Vec<Comment> {
        self.conversation()
            .into_iter()
            .filter(|comment| comment.is_bot)
            .collect()
    }

    /// The one question a suspended run published — and an assertion that there
    /// is exactly one.
    ///
    /// The cardinality is the accessor rather than something a caller remembers to
    /// check, because "the *only* request comment" is the claim: a run that asked
    /// twice is the defect a continuation exists to not be, and a helper that
    /// answered with the first of two would let that walk pass.
    pub fn the_only_request_comment(&self) -> Comment {
        let mut questions = self.request_comments();
        assert_eq!(
            questions.len(),
            1,
            "the conversation must hold exactly one question fiddle asked, and it \
             holds {}: {:?}",
            questions.len(),
            questions
        );
        questions.remove(0)
    }

    /// Every comment on the conversation whose body names `text`.
    ///
    /// The accessor a test asks "was the question asked twice" with, and it reads
    /// the **conversation** rather than the request log on purpose: two posts of one
    /// question are two entries in the log by definition, while what a person and a
    /// later process see is the listing. A run that posted twice and a run that
    /// posted once are told apart here by what is *there*.
    ///
    /// A substring and not a parse, because the caller already holds the identity it
    /// is looking for — a request id out of a marker it parsed — and asking whether
    /// any comment names it is a different question from asking whether a comment is
    /// a well-formed request. [`parse_marker`] is the second question.
    pub fn comments_naming(&self, text: &str) -> Vec<Comment> {
        self.conversation()
            .into_iter()
            .filter(|comment| comment.body.contains(text))
            .collect()
    }

    /// Redistribute the conversation into pages of `per_page`, so a read has to
    /// follow `rel="next"` to see all of it.
    ///
    /// Pages are whole files in this fixture — the stub answers `page=k` from
    /// `page-k.json` and offers `rel="next"` when `page-(k+1).json` exists — so
    /// paginating means moving comments between files rather than passing a
    /// parameter. Stale pages beyond the new last are removed, because a
    /// `page-3.json` nobody meant to keep would make the header advertise a page
    /// the fixture no longer describes.
    pub fn paginate_conversation(&self, per_page: usize) {
        assert!(per_page > 0, "a page holds at least one comment");
        let all: Vec<serde_json::Value> = (1..=self.last_page())
            .flat_map(|page| self.page(page))
            .collect();
        let pages = all.chunks(per_page).count().max(1);
        for (index, chunk) in all.chunks(per_page).enumerate() {
            self.write_page(index as u64 + 1, chunk);
        }
        if all.is_empty() {
            self.write_page(1, &[]);
        }
        let mut stale = pages as u64 + 1;
        while self.page_path(stale).exists() {
            std::fs::remove_file(self.page_path(stale)).unwrap();
            stale += 1;
        }
    }

    /// What the scripted `gh` answers for one page of the conversation: the
    /// status line, the headers, and the body, exactly as it printed them.
    ///
    /// The raw response rather than a parsed one, because the header is half of
    /// what a caller is asking about. `Link: …; rel="next"` is not something a
    /// parsed body could carry.
    pub fn listing(&self, page: u64) -> String {
        self.gh(&[
            "api",
            "--method",
            "GET",
            &format!("/repos/{REPO}/issues/{CONVERSATION_ISSUE}/comments?per_page=100&page={page}"),
        ])
    }

    // -- the graphql route ---------------------------------------------------

    /// Script the answer to GraphQL call `n`, status and body separately.
    ///
    /// Separate arguments because for GraphQL they are separate facts: a refusal
    /// arrives as **200** carrying an `errors[]`, so a fixture whose status field
    /// had to carry the verdict could not express one.
    ///
    /// `n` is **zero-based** — call one is answered from `graphql/0.json` — which
    /// is the numbering the stub reads and the numbering
    /// `propose_capability::script_graphql` and `ready_effect`'s already use. It
    /// is not the numbering a reader would guess, which is why it is said here.
    pub fn script_graphql(&self, n: usize, status: u16, body: serde_json::Value) {
        let dir = self.stub.join("graphql");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{n}.json")),
            serde_json::json!({"status": status, "body": body}).to_string(),
        )
        .unwrap();
    }

    /// How many completions the model endpoint answered, across every process this
    /// world has run.
    ///
    /// # The observation that tells a refused reply from an unreadable one
    ///
    /// Written because an inversion came back **null** without it, and the null was in
    /// the matrix's two most important rows. A walk that wrongly accepted a bot's
    /// reply as a candidate reaches step 7, and if the script has no interpretation
    /// left the model call *fails* — `interpret` collapses **every** transport failure
    /// to `Unclear` (`human/interpret.rs:266-271`), and `Unclear` is
    /// `AwaitingDecision`, exit 10, nothing mutated. Which is bit-for-bit the outcome
    /// of the reply having been refused for not being a person.
    ///
    /// So "the script ran out" is not an assertion that the model was never asked; it
    /// is an outcome indistinguishable from success. This counter is the assertion, and
    /// it is read from the endpoint rather than inferred from an exit code.
    pub fn model_calls(&self) -> usize {
        self.gateway.served()
    }

    /// How many GraphQL calls this world was asked to answer.
    ///
    /// Counted by the stub in a file rather than derived from the request log,
    /// because it is the position the *next* answer is chosen by — so the count
    /// and the choice cannot disagree.
    pub fn graphql_calls(&self) -> usize {
        std::fs::read_to_string(self.stub.join("graphql_calls"))
            .ok()
            .and_then(|count| count.trim().parse().ok())
            .unwrap_or(0)
    }

    /// Ask the scripted `gh` one GraphQL question, in the argument shape
    /// `GhCli::graphql` uses, and hand back what it printed.
    pub fn graphql(&self, query: &str) -> String {
        self.gh(&["api", "graphql", "-f", &format!("query={query}")])
    }

    /// Script the answer to the one GraphQL mutation a decision walk dispatches:
    /// `markPullRequestReadyForReview`, accepted.
    ///
    /// It has to be scripted, and that is the fixture working as intended rather
    /// than a chore. The GraphQL route has **no unscripted default**, deliberately:
    /// a route whose omission answered a success is a route where forgetting to
    /// script one looks exactly like meaning it, and a GraphQL 200 is the one answer
    /// whose verdict lives in its body — so a fabricated one is a fabricated
    /// verdict, and this is the mutation a person's approval is spent on.
    ///
    /// Accepted means `200` and **no `errors[]`**. The stub reads the verdict off
    /// the body rather than off the status line, so a refusal is scripted by putting
    /// an `errors[]` in a 200 and not by changing the number.
    ///
    /// Call zero, because the numbering is zero-based and this is the only GraphQL
    /// call a whole suspension-and-continuation makes. A run that dispatched a
    /// second one would find call one unscripted and fail naming the file, which is
    /// the loud version of "the mutation was repeated".
    pub fn accept_the_ready_mutation(&self) {
        self.script_graphql(
            0,
            200,
            serde_json::json!({
                "data": { "markPullRequestReadyForReview": { "clientMutationId": null } }
            }),
        );
    }

    /// Send the one ready mutation **by hand**, addressed at
    /// [`PULL_REQUEST_NODE_ID`], and hand back what the forge answered.
    ///
    /// # This exists to be the positive half of a negative assertion
    ///
    /// *"The pull request is still a draft"* has two ways of being true and only one
    /// of them is a property: the mutation did not happen, or this world could not
    /// have shown it if it had. A scenario asserting the draft survived a walk it
    /// expected to mutate nothing needs the second reading closed **in its own
    /// world** — that the answer scripted at call zero really accepts, and that the
    /// stub really takes a pull request out of draft when one lands. Both hold here
    /// or neither does, and the caller reads the draft again afterwards to say so.
    ///
    /// It is deliberately not a third process, and the reason has changed. **It used to
    /// be that it could not be one:** *"after a redirect has asked again this world's
    /// comment ids are not distinct — `gh_stub`'s posted comments are numbered
    /// positionally within a path and know nothing of what a test seeded, so the second
    /// question can collide with a reply."* That was true and is no longer — ids are
    /// minted at post time now, and
    /// [`an_approval_of_the_earlier_change_is_read_and_superseded_rather_than_spent`](../human_direction.rs)
    /// drives a third process through exactly that world.
    ///
    /// It stays a hand-sent mutation because a third process is a *different and weaker*
    /// answer to the question this accessor asks. What a scenario needs from here is the
    /// narrow claim that **the arming was live** — that the answer scripted at call zero
    /// really accepts and the stub really takes a pull request out of draft. A process
    /// would have to reach that through a whole walk, so a failure anywhere in the walk
    /// would look like the arming being dead.
    ///
    /// **This is a fixture write and not an observation.** It moves the world, so a
    /// caller must make every assertion about what the run did *before* calling it —
    /// including the ones off [`World::requests`], which records this invocation like
    /// any other. The mutation text is spelled here rather than imported for the
    /// reason every other constant in this file is: the acceptance package depends on
    /// neither library. The stub keys on the substring
    /// `markPullRequestReadyForReview` and on the `id` field, which is the whole of
    /// what it reads.
    pub fn dispatch_the_ready_mutation(&self) -> String {
        self.gh(&[
            "api",
            "graphql",
            "-f",
            "query=mutation($id: ID!) { markPullRequestReadyForReview(input: \
             {pullRequestId: $id}) { clientMutationId } }",
            "-f",
            &format!("id={PULL_REQUEST_NODE_ID}"),
        ])
    }

    // -- the pull requests ---------------------------------------------------

    /// Every open pull request the forge holds, read through the scripted `gh`.
    ///
    /// Read through `gh` and not off the stub's files, for [`World::conversation`]'s
    /// reason: the listing is answered from the world the writes built, so a pull
    /// request a *run* created appears here and one this file wrote by hand would
    /// too. A helper that read `pulls_seed` would see only the second kind, which is
    /// the only kind no scenario in this lane creates.
    ///
    /// `state=open` and no other parameter, so this is the collection and not a
    /// lookup: the product's own read constrains `head` and `base` as well, and a
    /// test asking "how many pull requests are there" must not ask the narrower
    /// question — a second pull request for a *different* head is exactly the
    /// duplicate a continuation must not create, and a filtered read would not see
    /// it.
    pub fn open_pull_requests(&self) -> Vec<serde_json::Value> {
        body_of(&self.gh(&[
            "api",
            "--method",
            "GET",
            &format!("/repos/{REPO}/pulls?state=open"),
        ]))
    }

    /// One pull request by its own number, read through the scripted `gh`.
    ///
    /// # The number is never `1`, and that is the fixture's design rather than a
    /// quirk
    ///
    /// The scripted `gh` numbers pull requests from **7**, because "numbers are
    /// positional and start at 7 rather than 1, so a test asserting on an external
    /// reference cannot pass by accident against an index or a count". So
    /// [`CONVERSATION_ISSUE`] is 7 as well — the conversation a question is
    /// published to is the first pull request's — and a test passing `1` here is
    /// asking about nothing.
    ///
    /// This is the by-number read, which is a **different answer** from the listing:
    /// it carries `draft` and `node_id`, and those two are the whole reason
    /// `EnsurePullRequestReady` addresses it. It is also where a landed ready
    /// transition becomes visible, because the stub applies the mutations that
    /// really happened over the seeded body — so `["draft"]` read here after a
    /// continuation is the world's word on whether the transition occurred, not
    /// fiddle's.
    pub fn pull_request(&self, number: u64) -> serde_json::Value {
        let response = self.gh(&[
            "api",
            "--method",
            "GET",
            &format!("/repos/{REPO}/pulls/{number}"),
        ]);
        object_of(&response)
            .unwrap_or_else(|| panic!("the forge answered no pull request {number}: {response}"))
    }

    /// Give the forge its own answer for pull request `number`: a draft, at the
    /// revision the remote really holds for `branch`, with a node id. Hands back
    /// that revision.
    ///
    /// # Why a scenario has to do this, and why it does not weaken anything
    ///
    /// The scripted `gh` derives its pull request *listing* from the creates that
    /// landed, and it cannot derive the by-number answer the same way: a create
    /// carries a head **label**, a base and a title, and **no revision at all**. So
    /// the one fact `EnsurePullRequestReady` and the validation order both turn on —
    /// which commit this pull request's head is at — is not something the create
    /// could have told it.
    ///
    /// What that fact is taken from is therefore the load-bearing part, and it is
    /// the **remote's own ref**: [`World::remote_head`] reads what the push really
    /// put there, with git, out of a real bare repository. Not from fiddle's report,
    /// not from its stdout, and not from a value this file invented. That is what
    /// keeps this a statement about the world — *GitHub says its pull request is at
    /// the commit the branch is at* — rather than a fixture agreeing with the thing
    /// under test.
    ///
    /// **It is a discriminating value, and a scenario should prove that.** Every
    /// identity a continuation recomputes runs through this revision: the gated
    /// effect's target is `{repo}#{pr}@{head}`, the request id is derived over that
    /// target, and the marker names the head outright. So a seed carrying the wrong
    /// commit makes a continuation refuse — it derives a request id no comment on
    /// the conversation names — and a scenario that inverts this value should see
    /// exactly that rather than a pass.
    pub fn answer_pull_request_by_number(&self, number: u64, branch: &str) -> String {
        let head_sha = self.remote_head(branch);
        let dir = self.stub.join("pulls_by_number");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{number}.json")),
            serde_json::json!({
                "number": number,
                "state": "open",
                // A draft, because that is what `propose_change` opens and what the
                // gated transition is *from*. The stub rewrites this to `false` once
                // a `markPullRequestReadyForReview` for this node id has landed, so
                // the value here is the world before the decision and never after.
                "draft": true,
                "node_id": PULL_REQUEST_NODE_ID,
                "head": { "ref": branch, "sha": &head_sha },
                "base": { "ref": BASE },
            })
            .to_string(),
        )
        .unwrap();
        head_sha
    }

    /// One file of the commit `branch` points at, as the **remote** holds it.
    ///
    /// # This is the observation a fixture-published head cannot fake
    ///
    /// Read with `git show` out of the bare repository, so it is the tree a
    /// reviewer opening the pull request would see — not what fiddle reported, not
    /// what a scenario wrote, and not the worktree the attempt worked in, which is
    /// gone by the time anything asks.
    ///
    /// It is the accessor "a genuinely *different* change was published" has to be
    /// stated in. Counting attempts, counting model calls, or reading a moved sha
    /// all leave one gap in common: a second attempt that wrote the same bytes moves
    /// the sha — a commit's identity includes the moment it was made — and would
    /// satisfy every one of them. Only the bytes say the change is different.
    ///
    /// `None` when the branch or the path is absent, which are two different
    /// failures and are deliberately not distinguished: every caller is asking about
    /// a file a run has just published, and either absence is the same defect.
    ///
    /// The branch is a parameter and not [`World::remote_branches`]`[0]`, for
    /// [`World::expected_marker`]'s reason: a world can hold more than one, and a
    /// helper that quietly picked would be the mistake it exists to catch.
    pub fn pushed_file(&self, branch: &str, path: &str) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["show", &format!("refs/heads/{branch}:{path}")])
            .current_dir(&self.remote)
            .output()
            .unwrap();
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Whether the remote holds `descendant` with `ancestor` behind it.
    ///
    /// # The world's own answer to "was that a fast-forward or a rewrite"
    ///
    /// `[github] git` in this world is the **real** `git`, so there is no recorder to
    /// filter a `--force` out of — and an argv check would be the weaker claim in any
    /// case. It says what was typed; this says what the remote now holds. A force
    /// push that rewrote the branch leaves a head the previous one is not an ancestor
    /// of, whatever the command line said, and a push that fast-forwarded cannot.
    ///
    /// `merge-base --is-ancestor` answers by **exit code**, so a `false` here is a
    /// real negative rather than a parse of empty output — which is what makes it
    /// safe to assert in the negative direction, and callers should: a version
    /// answering `true` unconditionally satisfies the direction that matters on its
    /// own, and the reverse direction is its denominator.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        std::process::Command::new("git")
            .args(["merge-base", "--is-ancestor", ancestor, descendant])
            .current_dir(&self.remote)
            .status()
            .unwrap()
            .success()
    }

    /// The text of every prompt the model endpoint was shown, in order.
    ///
    /// # Why this is decoded rather than handed over as the request body
    ///
    /// [`StubGateway::request_bodies`] already records what went on the wire, and a
    /// caller asserting against it directly would be asserting against **JSON**: a
    /// person's words carrying a quote, a newline or a backslash appear there
    /// escaped, so `body.contains(what_they_wrote)` is false for exactly the inputs
    /// a hostile one is made of. A scenario written that way would report *the
    /// instruction never arrived* about a prompt that carries it, and — far worse —
    /// would report *nothing escaped the fence* about a prompt where something had.
    ///
    /// So the messages are parsed out and concatenated, which is the text the model
    /// is shown. Content that is an array of parts is flattened, because that is the
    /// other shape an OpenAI-compatible message takes and a client that switched
    /// shapes must not silently start reading nothing.
    ///
    /// One entry per completion, including the turns of a tool loop — so a redirected
    /// attempt contributes several and the instruction is in each, the whole
    /// conversation being resent every turn. Assertions should therefore be about
    /// *which* prompts carry a thing and never about a count of occurrences.
    pub fn model_prompts(&self) -> Vec<String> {
        self.gateway
            .request_bodies()
            .iter()
            .map(|body| {
                let sent: serde_json::Value = serde_json::from_str(body).unwrap_or_else(|error| {
                    panic!("a request to the model endpoint is JSON ({error}): {body}")
                });
                let mut shown = String::new();
                for message in sent["messages"].as_array().into_iter().flatten() {
                    match &message["content"] {
                        serde_json::Value::String(text) => shown.push_str(text),
                        serde_json::Value::Array(parts) => {
                            for part in parts {
                                if let Some(text) = part["text"].as_str() {
                                    shown.push_str(text);
                                }
                            }
                        }
                        _ => {}
                    }
                    shown.push('\n');
                }
                shown
            })
            .collect()
    }

    /// What the remote's `refs/heads/<branch>` really points at.
    ///
    /// Asked of git in the bare repository, which is the world's own record of what
    /// a push put there. Panics when the branch is absent, because every caller is
    /// asking about a branch a run has just published and an absent one is a failed
    /// publication rather than a case.
    pub fn remote_head(&self, branch: &str) -> String {
        git_says(
            &self.remote,
            &["rev-parse", &format!("refs/heads/{branch}")],
        )
    }

    // -- denominators for the accessors that are only ever asserted empty ----
    //
    // `remote_branches` and `posted_comment_bodies` are read by the read-only scenario,
    // which asserts both are empty. Two inversions showed that a version of
    // either which answered empty *unconditionally* broke no test in the lane, so
    // both negatives were passing for free. The two helpers below exist so a test
    // can show the accessor sees something when there is something to see — the
    // same reason `requested_paths` has its own recorder test.

    /// Push the work repository's `HEAD` to the remote as `branch`, with the real
    /// `git` and over a path.
    ///
    /// A fixture action, not fiddle publishing: nothing in this build's reachable
    /// capabilities pushes a branch for a decision walk. What it buys is a remote
    /// that really holds a ref, so [`World::remote_branches`] can be shown to read
    /// one.
    pub fn push_branch(&self, branch: &str) {
        git(
            &self.work,
            &["push", "-q", "origin", &format!("HEAD:{branch}")],
        );
    }

    /// Post a comment through the scripted `gh`'s own write route, the way the
    /// product's client would, and hand back what it answered.
    ///
    /// A fixture action for the same reason as [`World::push_branch`], and the
    /// distinction matters more here: this is **not** fiddle asking a question.
    /// [`World::posted_comment_bodies`] counted afterwards says the *recorder* sees a
    /// write, and says nothing about any run having made one.
    pub fn post_comment_through_the_forge(&self, body: &str) -> String {
        self.gh_sending(
            &[
                "api",
                "--method",
                "POST",
                &format!("/repos/{REPO}/issues/{CONVERSATION_ISSUE}/comments"),
                "--input",
                "-",
            ],
            &serde_json::json!({"body": body}).to_string(),
        )
    }

    /// Run the scripted `gh` directly and hand back its stdout.
    ///
    /// Direct rather than through a fiddle run, and only for the routes no
    /// capability this build can execute reaches. It is a weaker observation than
    /// a run would be — it proves the fixture offers something, not that fiddle
    /// consumes it — and each caller says so.
    fn gh(&self, args: &[&str]) -> String {
        self.gh_sending(args, "")
    }

    /// As [`World::gh`], with `body` on the child's stdin — which is where the
    /// scripted `gh` reads a request body from, exactly as the real one does under
    /// `--input -`.
    fn gh_sending(&self, args: &[&str], body: &str) -> String {
        use std::io::Write;

        let mut child = std::process::Command::new(gh_stub_binary())
            .args(["--stub-dir", self.stub.to_str().unwrap()])
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    // -- the local past ------------------------------------------------------

    /// The directory `report.dir` names.
    pub fn report_dir(&self) -> PathBuf {
        self.scenario.report_dir()
    }

    /// Every bundle this world's runs published: every file under `<report.dir>`
    /// that is **not** part of the attempt journal.
    ///
    /// The exclusion is what makes [`World::delete_attempt_journal`] a testable
    /// helper rather than a no-op. See [`World::delete_report_bundles`].
    pub fn report_bundles(&self) -> Vec<PathBuf> {
        let journal = self.report_dir().join(ATTEMPTS);
        walkdir_files(self.report_dir())
            .into_iter()
            .filter(|path| !path.starts_with(&journal))
            .collect()
    }

    /// Every attempt record, at `<report.dir>/.attempts`.
    pub fn journal_records(&self) -> Vec<PathBuf> {
        self.scenario.journal_records()
    }

    /// The directory `workspace.root` names: where an attempt branches its
    /// ephemeral worktree.
    pub fn workspace_root(&self) -> PathBuf {
        self.scenario.dir().join("workspaces")
    }

    /// Everything the workspace root holds, however deep.
    ///
    /// Files *and* directories, because an attempt's worktree is a directory and a
    /// helper that counted only files would report an empty one as absent — which
    /// is exactly the state a torn-down worktree leaves and exactly the state a
    /// half-torn-down one does not.
    pub fn worktrees(&self) -> Vec<PathBuf> {
        walkdir_dirs(self.workspace_root())
            .into_iter()
            .chain(walkdir_files(self.workspace_root()))
            .collect()
    }

    /// Every byte of every document this world's runs published, concatenated.
    ///
    /// The credential search a suspended path has to survive, and the reason it is
    /// scoped to `<report.dir>` rather than to the whole project is worth stating:
    /// the scripted `gh` and the recording `git` each write down the entire
    /// environment they were handed, and the credential is one of them **by
    /// design** — that recording is how the five-name and seven-name environment
    /// assertions are made. So a concatenation of the project tree would contain
    /// the sentinel against a perfectly correct implementation, and an assertion
    /// over it would have to grow an exemption list to mean anything.
    /// `exactly_once::is_fixture_recording` is that list, and it exists because
    /// that suite searches the wider tree on purpose.
    ///
    /// **What a test using this cannot distinguish:** where in the bundle a leak
    /// is. It answers whether the bytes a downstream reader receives hold the
    /// string, which is the question; `exactly_once`'s path-returning search is
    /// the one that answers *which file*.
    pub fn all_published_bytes(&self) -> String {
        let mut all = String::new();
        for path in walkdir_files(self.report_dir()) {
            all.push_str(&String::from_utf8_lossy(&std::fs::read(&path).unwrap()));
        }
        all
    }

    /// Take away every bundle this world's runs published, and **only** those:
    /// the attempt journal under `<report.dir>/.attempts` is left standing.
    ///
    /// # Why it is not `remove_dir_all(<report.dir>)`
    ///
    /// Because the journal lives inside the report directory, so it would be, and
    /// then [`World::delete_attempt_journal`] could never fail: it would run
    /// against an already-empty tree, delete nothing, and report success. One
    /// helper would be silently doing the work of two and the second would be
    /// untested *by construction* — which is precisely the vacuous proof this
    /// bean's criterion exists to prevent, arrived at by being tidy rather than by
    /// being careless.
    ///
    /// [`Scenario::remove_local_records`] is the union of the two and stays as it
    /// is; it belongs to a milestone whose claim is about local records as a
    /// whole. This one is split because M3's claim is about each of them.
    pub fn delete_report_bundles(&self) {
        let journal = self.report_dir().join(ATTEMPTS);
        let Ok(entries) = std::fs::read_dir(self.report_dir()) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path == journal {
                continue;
            }
            let removed = match path.is_dir() {
                true => std::fs::remove_dir_all(&path),
                false => std::fs::remove_file(&path),
            };
            removed.unwrap_or_else(|e| panic!("could not remove {} ({e})", path.display()));
        }
        assert!(
            self.report_bundles().is_empty(),
            "no published bundle may survive: {:?}",
            self.report_bundles()
        );
    }

    /// Take away the record every executed attempt left behind.
    pub fn delete_attempt_journal(&self) {
        remove_tree(&self.report_dir().join(ATTEMPTS));
        assert!(
            self.journal_records().is_empty(),
            "no attempt record may survive: {:?}",
            self.journal_records()
        );
    }

    /// Take away every workspace an attempt branched, and the root they live
    /// under.
    ///
    /// The root goes too, and the run that follows recreates it — which is what
    /// makes this a deletion rather than a tidy-up. A root left in place is a
    /// directory a continuation could have found a checkout in.
    pub fn delete_workspaces(&self) {
        remove_tree(&self.workspace_root());
        assert!(
            self.worktrees().is_empty(),
            "no workspace may survive: {:?}",
            self.worktrees()
        );
    }

    /// Whether nothing local remains for a second process to read.
    ///
    /// True only when all three are absent or empty: the bundles under
    /// `<report.dir>`, the attempt journal at `<report.dir>/.attempts`, and the
    /// workspace root. Three and not one, because they are deleted by three
    /// helpers and a single predicate over the report directory would go true
    /// while a worktree still stood.
    ///
    /// **What a test using this cannot distinguish:** state outside the project.
    /// It says nothing about the conversation, the remote, or the forge's world —
    /// which is the point. Those are what a continuation is *supposed* to read.
    pub fn local_state_is_empty(&self) -> bool {
        self.report_bundles().is_empty()
            && self.journal_records().is_empty()
            && self.worktrees().is_empty()
    }

    // -- producing a worktree that outlives its run --------------------------

    /// Start a repair, wait until its check is running inside the ephemeral
    /// worktree, kill the process outright, and hand back what the worktree
    /// root then holds.
    ///
    /// The state exists because a *completed* attempt takes its own worktree down,
    /// so after a clean run there is nothing for [`World::delete_workspaces`] to
    /// delete and a test of it there would pass against a helper that did nothing.
    ///
    /// `kill -9` rather than the `SIGINT` `exactly_once` delivers, and the
    /// difference is deliberate: that suite is asserting what the *handler* does,
    /// and this one needs a process that got no chance to tidy up. A crashed or
    /// out-of-memory run is how an operator really acquires a leftover worktree,
    /// and it is the state 565u's third deletion is protecting against.
    ///
    /// Synchronised on the worktree being **checked out** rather than on a sleep, so
    /// the kill cannot arrive before there is anything to leave behind.
    ///
    /// # Why the condition is a file and not the directory
    ///
    /// It was the directory, and that was a race with a name. `git worktree add` is
    /// a child process, and it *creates* `<root>/<name>` early and populates it
    /// afterwards — so a poll that waited only for the directory could kill fiddle
    /// while its `git` was still writing. The `git` is not in fiddle's process group
    /// and outlives a `kill -9` of its parent, so it carried on creating entries
    /// under a root the test was already deleting, and `delete_workspaces` failed
    /// with `Directory not empty`.
    ///
    /// It surfaced when this file gained the decision-walk scenarios: three more
    /// tests in one binary, each driving a real attempt and real `git`, moved the
    /// timing enough to lose that race. It was never reproducible on its own, which
    /// is the shape of the bug rather than an excuse — the window is the duration of
    /// a checkout.
    ///
    /// The fixture's own source file is the condition because its presence is what
    /// says the checkout finished. Everything that writes into the worktree after
    /// that point is either in fiddle's own process — the model's `write_file`, which
    /// dies with it — or the check, which is a `sleep` that writes nothing.
    ///
    /// **This change is reasoned and not measured, and an inversion says so.**
    /// Restoring the old condition and running this binary eight times under CPU
    /// load did not reproduce the failure, so the narrowing is a diagnosis. The
    /// robustness that holds whatever the writer turns out to be is in
    /// [`remove_tree`], which waits the writer out rather than failing; this one
    /// closes the window instead of tolerating it, and neither weakens an assertion.
    ///
    /// Addressed to [`SECOND_INVOCATION_REF`] and not to [`INVOCATION_REF`], for
    /// the reason that constant records: a run that already accounted for the
    /// first piece of work leaves a marker, so a second run addressed the same way
    /// executes nothing and there is no attempt to interrupt.
    #[cfg(unix)]
    pub fn interrupt_a_repair_inside_its_worktree(&self) -> Vec<PathBuf> {
        self.make_the_check_wait();
        let mut child = self
            .command(
                [
                    "run",
                    "--capability",
                    "fixture_repair",
                    SECOND_INVOCATION_REF,
                ],
                true,
            )
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        while !self.a_worktree_is_checked_out() {
            assert!(
                std::time::Instant::now() < deadline,
                "the attempt never checked a worktree out under {}, so there was \
                 nothing to leave behind; it holds {:?}",
                self.workspace_root().display(),
                self.worktrees()
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Read before the kill, so what is reported is what the running attempt
        // had made rather than whatever survived the race with its death.
        let leftover = self.worktrees();

        let status = std::process::Command::new("kill")
            .args(["-9", &child.id().to_string()])
            .status()
            .expect("kill is on the PATH");
        assert!(status.success(), "could not kill {}", child.id());
        child.wait().unwrap();
        leftover
    }

    /// Whether some worktree under the workspace root holds the fixture's own source
    /// file — which is what says `git worktree add` finished rather than started.
    ///
    /// Matched on the file name and at any depth, so it is a claim about the checkout
    /// rather than about a path this file reconstructed: the worktree's directory is
    /// [`attempt_worktree`]'s to derive, and a fixture that spelled it out would be a
    /// second derivation of the one thing that must not have two.
    ///
    /// [`attempt_worktree`]: https://docs.rs/fiddle-runtime
    fn a_worktree_is_checked_out(&self) -> bool {
        walkdir_files(self.workspace_root())
            .iter()
            .any(|path| path.file_name().and_then(|name| name.to_str()) == Some("lib.rs"))
    }

    /// Rewrite `workspace.check` so it waits instead of deciding.
    ///
    /// The substitution is checked, so a scenario cannot silently keep the
    /// deciding check and then wait ninety seconds for a worktree that came and
    /// went.
    ///
    /// Sixty seconds: long enough that the poll above always wins — it finds the
    /// worktree within a second — and short enough that the `sleep` orphaned by
    /// the `kill -9` is gone well before anybody notices it. A killed parent
    /// cannot take its process group down with it, which is the price of needing a
    /// process that got no chance to tidy up.
    fn make_the_check_wait(&self) {
        let before = self.scenario.config_text();
        let after = before.replace(
            &format!("check = {CHECK}"),
            "check = { program = \"sleep\", args = [\"60\"] }",
        );
        assert_ne!(
            before, after,
            "the document must name the deciding check for it to be replaced"
        );
        std::fs::write(self.scenario.config_path(), after).unwrap();
    }

    // -- the conversation's pages, on disk -----------------------------------

    fn page_path(&self, page: u64) -> PathBuf {
        self.stub
            .join(CONVERSATION)
            .join(format!("page-{page}.json"))
    }

    /// The comments one page file holds, or none when there is no such page.
    fn page(&self, page: u64) -> Vec<serde_json::Value> {
        let Ok(text) = std::fs::read_to_string(self.page_path(page)) else {
            return Vec::new();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    fn write_page(&self, page: u64, listed: &[serde_json::Value]) {
        std::fs::write(
            self.page_path(page),
            serde_json::Value::Array(listed.to_vec()).to_string(),
        )
        .unwrap();
    }

    /// The highest page number the conversation has a file for. At least one,
    /// because [`World::new`] writes an empty first page on purpose.
    fn last_page(&self) -> u64 {
        let mut page = 1;
        while self.page_path(page + 1).exists() {
            page += 1;
        }
        page
    }
}

impl Default for World {
    fn default() -> Self {
        World::new()
    }
}

/// The check `World`'s document names: `grep` for the repaired text.
///
/// An ordinary external program named by the document, run by the binary inside
/// the worktree, whose exit code decides the outcome. `binary_repair`'s own
/// reasoning for not using `cargo test` here applies unchanged — the nested
/// compiler cannot find its SDK inside `Workspace::run`'s four-name environment —
/// and nothing is given up, because which program a check is remains the
/// operator's business.
const CHECK: &str = "{ program = \"grep\", args = [\"-q\", \"len - 1\", \"src/lib.rs\"] }";

/// The directory the conversation's pages live in, under the scripted `gh`'s
/// scratch directory.
///
/// Spelled here rather than imported, because `gh_stub`'s own constant is private
/// to a binary. It is load-bearing in one direction only: a value that disagreed
/// would make every listing panic on an unscripted page, which is loud.
const CONVERSATION: &str = "issue-comments";

/// The directory the **inline review** comments live in, which is a different
/// collection from the conversation and the one nothing reads.
///
/// `gh_stub`'s `comment_answer` routes `/repos/o/r/pulls/{n}/comments` here and
/// `/repos/o/r/issues/{n}/comments` to [`CONVERSATION`], on the key's *shape* rather
/// than on a substring — the two differ by where the number sits. So a page written
/// here is reachable only through the review route, which is what makes
/// [`World::post_review_comment`] a decoy a walk has to fail to consult.
const REVIEW_COMMENTS: &str = "review-comments";

/// The by-id subdirectory of the conversation, where a re-read is answered from
/// **in preference to the listing**.
///
/// That precedence is the fixture's only way to express an edit *between* the two
/// reads, which is the whole subject of step 5 of the validation order: the listing
/// says one thing, the comment's own id says another, and the walk is entitled to
/// refuse rather than pick. `gh_stub`'s `comment_by_id` consults this file first and
/// falls back to the listing, so a scenario writes here exactly when it means "the
/// re-read disagrees".
const BY_ID: &str = "by-id";

/// The attempt journal's directory name under `<report.dir>`.
///
/// Design §4.9 names it, and it is spelled here rather than read from
/// `fiddle_runtime` for [`Scenario::prepare_journal_dir`]'s reason: the
/// acceptance lane checks the binary against the documented layout instead of
/// against itself.
const ATTEMPTS: &str = ".attempts";

/// Remove `path` and everything under it, tolerating its absence and waiting out a
/// writer that has not finished.
///
/// Absence is tolerated so a helper can be called before the run that would have
/// created something, which is what lets `local_state_is_empty` be asserted on a
/// world nothing has happened in yet.
///
/// # Why it retries, and why that is not a weakening
///
/// One tree this is asked to remove is a workspace root a **killed** run left
/// behind, and a killed run can leave a child of its own still writing there:
/// `kill -9` reaches one process, and the `git` that was checking a worktree out is
/// not in that process's group. `remove_dir_all` walks and unlinks, so an entry
/// created behind the walk makes the final `rmdir` fail with `ENOTEMPTY` — which is
/// a race with a writer and not a tree that cannot be removed.
///
/// The retry waits for that writer to finish rather than pretending the tree is
/// gone. **Nothing is softened:** every caller asserts emptiness afterwards, so a
/// removal that never succeeds still fails the test, and a partial removal still
/// fails it. What changes is only that a fixture step is allowed to take a second
/// rather than to fail because a child was mid-write.
///
/// # What is measured and what is reasoned, said plainly
///
/// **Measured:** one run of the full workspace gate failed here with `Directory not
/// empty` on the workspace root, and it was the run in which this file's three
/// decision-walk scenarios first shared a test binary with the killed-repair one.
/// **Reasoned:** that the writer is the orphaned checkout. It could not be
/// reproduced — not under CPU load, and not with the earlier synchronisation
/// restored, over eight runs each — so the mechanism above is a diagnosis and not an
/// observation, and it is recorded that way rather than as a fact. The neighbouring
/// change to what the interrupt waits for is the same diagnosis applied at the
/// source; this one holds whatever the writer turns out to be.
fn remove_tree(path: &Path) {
    // A second is far longer than a checkout of this fixture takes and far shorter
    // than any test timeout, so a genuinely unremovable tree still fails promptly.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) if std::time::Instant::now() < deadline => {
                // Reported only if it never succeeds, so a passing run stays quiet
                // and a failing one says how long it waited and what for.
                std::thread::sleep(std::time::Duration::from_millis(20));
                let _ = e;
            }
            Err(e) => panic!(
                "could not remove {} after waiting a second for whatever is still \
                 writing there ({e}); it holds {:?}",
                path.display(),
                walkdir_files(path)
            ),
        }
    }
}

/// The `argv` one recorded request holds.
fn argv_of(request: &serde_json::Value) -> Vec<String> {
    request["argv"]
        .as_array()
        .map(|argv| {
            argv.iter()
                .filter_map(|a| a.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The JSON array a scripted `gh` response carries, from the raw response the
/// stub printed.
///
/// The body is whatever follows the blank line that ends the headers, which is
/// HTTP's own rule and the one the product's client applies. Parsed loosely
/// enough that a response carrying no array — a refusal, say — yields nothing
/// rather than panicking, so a caller can ask what a failed read returned.
pub fn body_of(response: &str) -> Vec<serde_json::Value> {
    let Some((_, body)) = response.split_once("\r\n\r\n") else {
        return Vec::new();
    };
    match serde_json::from_str(body) {
        Ok(serde_json::Value::Array(listed)) => listed,
        _ => Vec::new(),
    }
}

/// The single JSON object a scripted `gh` response carries, from the raw response.
///
/// [`body_of`]'s sibling, and separate rather than one function returning a
/// `Value`: the collections answer with arrays and the by-number reads answer with
/// objects, and a caller that had to check which it got would be checking something
/// the endpoint already decided. `None` means the response carried no object — a
/// refusal, or a `gh` that answered something this fixture cannot read — so a caller
/// can say which endpoint disappointed it rather than unwrapping a `null`.
fn object_of(response: &str) -> Option<serde_json::Value> {
    let (_, body) = response.split_once("\r\n\r\n")?;
    match serde_json::from_str(body) {
        Ok(value @ serde_json::Value::Object(_)) => Some(value),
        _ => None,
    }
}

/// One listed comment as a [`Comment`].
///
/// Panics on a missing `id`, because a listing entry without one is a fixture
/// defect rather than a case, and a default of zero would make two of them
/// compare equal.
fn comment_from(value: &serde_json::Value) -> Comment {
    Comment {
        id: value["id"]
            .as_u64()
            .unwrap_or_else(|| panic!("a listed comment carries an id: {value}")),
        body: value["body"].as_str().unwrap_or_default().to_string(),
        created_at: value["created_at"].as_str().unwrap_or_default().to_string(),
        updated_at: value["updated_at"].as_str().unwrap_or_default().to_string(),
        author: value["user"]["id"].as_u64().unwrap_or_default(),
        is_bot: value["user"]["type"].as_str() == Some("Bot"),
    }
}
