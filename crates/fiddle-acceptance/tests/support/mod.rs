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

/// What is exported as the forge credential: a string that authenticates nothing
/// — the `gh` it reaches is a scripted program and the remote is a path — and
/// that must appear on no surface a reader can reach.
pub const SENTINEL: &str = "ghp_m3_sentinel_must_never_be_printed_7c04";

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

impl Run {
    /// The `--json` payload this run printed, with the whole result in the panic
    /// message when it is not JSON — which is where a run that died early says
    /// why.
    pub fn payload(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout is not JSON ({e}): {}\nstderr = {}",
                self.stdout, self.stderr
            )
        })
    }
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
    /// fixture, and a gateway that will drive one repair.
    ///
    /// The empty conversation is said on purpose rather than defaulted. The
    /// scripted `gh` panics on an unscripted page instead of answering an empty
    /// one, because an absent file is an oversight — and a fixture that defaulted
    /// it would let a test assert "no question has been asked" against a world it
    /// never built.
    pub fn new() -> Self {
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
            // Two repairs' worth of turns rather than one, because the deletion
            // scenario drives two runs: one that finishes and publishes a bundle,
            // and one that is killed inside its worktree. A gateway holding a
            // single script would refuse the second run's first turn, and it would
            // then fail and tear its worktree down before there was anything to
            // leave behind — a hang or a flake in place of an assertion.
            gateway: StubGateway::serving(
                a_real_repair().into_iter().chain(a_real_repair()).collect(),
            ),
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
        for name in CREDENTIAL_VARS {
            command.env_remove(name);
        }
        command.env_remove(MODEL_CREDENTIAL);
        command.args(args);
        command.args(["--config", self.scenario.config_path().to_str().unwrap()]);
        if credentialled {
            command.env(FORGE_CREDENTIAL, &self.token);
            command.env(MODEL_CREDENTIAL, &self.token);
        }
        command
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
    /// # The id, and the one thing it cannot promise
    ///
    /// One above the highest id the conversation currently shows, fiddle's own
    /// posted comments included. That is what makes the collection ordered by id
    /// in *time* order, which every candidate rule in the validation order depends
    /// on: `validate::select_candidates` decides what is a reply by comparing ids,
    /// so a reply seeded below the question it answers would be invisible and one
    /// seeded above a comment that predates it would be a false candidate.
    ///
    /// **What it cannot promise** is that a comment *fiddle* posts afterwards gets
    /// an id above this one. The stub numbers its own from a fixed base
    /// (`FIRST_POSTED_COMMENT`, 9000) and knows nothing about what was seeded, so
    /// a run that wrongly posted a *second* question after a reply was seeded
    /// could collide with it. The collision is not silent — two entries would
    /// share an id — but a test asserting "no second question" should not rest on
    /// ids for it. [`World::posted_comment_bodies`], counted off the request log, is the
    /// accessor that cannot be confused this way.
    pub fn post_comment(&self, author: u64, body: &str) -> u64 {
        let id = self.conversation().iter().map(|c| c.id).max().unwrap_or(0) + 1;
        let page = self.last_page();
        let mut listed = self.page(page);
        listed.push(serde_json::json!({
            "id": id,
            "body": body,
            "created_at": SEEDED_AT,
            "updated_at": SEEDED_AT,
            "author_association": "COLLABORATOR",
            "user": {"login": format!("user-{author}"), "id": author, "type": "User"},
            "performed_via_github_app": serde_json::Value::Null,
        }));
        self.write_page(page, &listed);
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
            "user": {"login": "fiddle[bot]", "id": 1_000_001, "type": "Bot"},
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
        let mut found = Vec::new();
        let mut page = 1;
        loop {
            let response = self.listing(page);
            for value in body_of(&response) {
                found.push(comment_from(&value));
            }
            if !response.contains("rel=\"next\"") {
                return found;
            }
            page += 1;
        }
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
    /// Synchronised on the worktree appearing rather than on a sleep, so the kill
    /// cannot arrive before there is anything to leave behind.
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
        while self.worktrees().is_empty() {
            assert!(
                std::time::Instant::now() < deadline,
                "the attempt never branched a worktree under {}, so there was \
                 nothing to leave behind",
                self.workspace_root().display()
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

/// The attempt journal's directory name under `<report.dir>`.
///
/// Design §4.9 names it, and it is spelled here rather than read from
/// `fiddle_runtime` for [`Scenario::prepare_journal_dir`]'s reason: the
/// acceptance lane checks the binary against the documented layout instead of
/// against itself.
const ATTEMPTS: &str = ".attempts";

/// Remove `path` and everything under it, tolerating its absence.
///
/// Absence is tolerated so a helper can be called before the run that would have
/// created something, which is what lets `local_state_is_empty` be asserted on a
/// world nothing has happened in yet.
fn remove_tree(path: &Path) {
    match std::fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => panic!("could not remove {} ({e})", path.display()),
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
fn body_of(response: &str) -> Vec<serde_json::Value> {
    let Some((_, body)) = response.split_once("\r\n\r\n") else {
        return Vec::new();
    };
    match serde_json::from_str(body) {
        Ok(serde_json::Value::Array(listed)) => listed,
        _ => Vec::new(),
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
