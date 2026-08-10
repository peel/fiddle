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
fn git(dir: &Path, args: &[&str]) {
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

/// The repository root, two levels above this package.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
