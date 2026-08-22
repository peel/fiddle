use fiddle_core::{AdvisoryId, AttemptId, PackageType, ProjectedFinding, Severities, Severity};
use fiddle_runtime::agent::AgentBudget;
use fiddle_runtime::capability::{CapabilityError, Git, MigrationConfig};
use fiddle_runtime::cve::dedup::{DedupError, Local, Ran, Spawn};
use fiddle_runtime::cve::project::project;
use fiddle_runtime::evaluate::{Answered, Check, Contract, Repair, Success, Tree, Unanswered};
use fiddle_runtime::scanner::{ScanError, ScanReport, Scanner, Wizcli};
use fiddle_runtime::workspace::{Workspace, WorkspaceCommand, WorkspaceError, WorkspacePath};
use fiddle_runtime::Redaction;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

#[path = "document.rs"]
mod document;
pub use document::*;

#[path = "go_proxy.rs"]
mod go_proxy;
#[allow(unused_imports)]
pub use go_proxy::{
    run as offline_go, Answer as OfflineGo, SWEEP_FIXED, SWEEP_MODULE, SWEEP_VULNERABLE,
};
use go_proxy::{FIXTURE_PARENT, GO_VERSION, HOST_MODULE, INDIRECT_MODULE, INDIRECT_VERSION};

pub const SENTINEL: &str = "fiddle-sentinel-9f14c2a7";

pub const SENTINEL_SECRET: &str = "fiddle-secret-3b8e51d0";

pub const HOST_ROOT: &str = "/fiddle-host-root-5d2b8e13";

pub const ALL_SENTINELS: [&str; 4] = [SENTINEL, SENTINEL_SECRET, SENTINEL_PROSE, HOST_ROOT];

#[derive(Debug, Clone)]
pub struct ProgramRef {
    pub program: String,
    pub args: Vec<String>,
}

pub fn wiz_stub(arm: &str) -> ProgramRef {
    ProgramRef {
        program: env!("CARGO_BIN_EXE_wiz_stub").to_string(),
        args: vec![arm.to_string()],
    }
}

pub fn absent_scanner() -> ProgramRef {
    let program = format!("{}-which-is-not-installed", env!("CARGO_BIN_EXE_wiz_stub"));
    assert!(
        !Path::new(&program).exists(),
        "{program} exists, so it cannot stand for a scanner that is not installed"
    );
    ProgramRef {
        program,
        args: Vec::new(),
    }
}

const DIRECT_MODULE: &str = "golang.org/x/crypto";
const DIRECT_VERSION: &str = "v0.31.0";

const PARENT_A_MINOR_BEHIND: &str = "v1.2.0";

const SHIPPED_VERSION: &str = "v0.54.1";

#[derive(Debug, Clone)]
pub enum Shape {
    Direct,
    IndirectVia(String),
    Stdlib,
    Shipped { module: String, version: String },
}

pub fn direct() -> Shape {
    Shape::Direct
}

pub fn indirect_via(parent: &str) -> Shape {
    Shape::IndirectVia(parent.to_string())
}

pub fn stdlib() -> Shape {
    Shape::Stdlib
}

pub fn shipped(module: &str, version: &str) -> Shape {
    Shape::Shipped {
        module: module.to_string(),
        version: version.to_string(),
    }
}

const SHAPES: usize = 4;

impl Shape {
    pub fn index(&self) -> usize {
        match self {
            Shape::Direct => 0,
            Shape::IndirectVia(_) => 1,
            Shape::Stdlib => 2,
            Shape::Shipped { .. } => 3,
        }
    }

    fn requirements(&self) -> Vec<(String, String, bool)> {
        let require =
            |module: &str, version: &str| (module.to_string(), version.to_string(), false);
        match self {
            Shape::Direct => vec![require(DIRECT_MODULE, DIRECT_VERSION)],
            Shape::IndirectVia(parent) => vec![
                require(parent, PARENT_A_MINOR_BEHIND),
                (
                    INDIRECT_MODULE.to_string(),
                    INDIRECT_VERSION.to_string(),
                    true,
                ),
            ],
            Shape::Stdlib => Vec::new(),
            Shape::Shipped { module, version } => vec![require(module, version)],
        }
    }

    fn go_mod(&self) -> String {
        let mut text = format!("module {HOST_MODULE}\n\ngo {GO_VERSION}\n");
        if matches!(self, Shape::Stdlib) {
            text.push_str(&format!("\ntoolchain go{GO_VERSION}.0\n"));
        }
        let requirements = self.requirements();
        for (module, version, _) in requirements.iter().filter(|(_, _, i)| !*i) {
            text.push_str(&format!("\nrequire {module} {version}\n"));
        }
        for (module, version, _) in requirements.iter().filter(|(_, _, i)| *i) {
            text.push_str(&format!("\nrequire {module} {version} // indirect\n"));
        }
        text
    }

    fn go_sum(&self) -> Option<String> {
        go_proxy::sum_for(&self.requirements())
    }
}

pub fn all_shapes() -> [Shape; SHAPES] {
    [
        direct(),
        indirect_via(FIXTURE_PARENT),
        stdlib(),
        shipped(DIRECT_MODULE, SHIPPED_VERSION),
    ]
}

pub struct GoWorkspace {
    root: TempDir,
    repo: PathBuf,
    calls: Mutex<Vec<String>>,
}

impl GoWorkspace {
    pub fn path(&self) -> &Path {
        &self.repo
    }

    pub fn go_mod(&self) -> String {
        std::fs::read_to_string(self.repo.join("go.mod"))
            .unwrap_or_else(|source| panic!("no go.mod in {}: {source}", self.repo.display()))
    }

    pub fn is_clean(&self) -> bool {
        run_git(&self.repo, &["status", "--porcelain"]).is_empty()
    }

    pub fn is_clean_at(&self, paths: &[&str]) -> bool {
        let mut args = vec!["status", "--porcelain", "--"];
        args.extend_from_slice(paths);
        run_git(&self.repo, &args).is_empty()
    }

    pub fn staged_paths(&self) -> Vec<String> {
        run_git(
            &self.repo,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"],
        )
        .lines()
        .map(|line| line.to_string())
        .collect()
    }

    pub fn head_commit_body(&self) -> String {
        run_git(&self.repo, &["log", "-1", "--format=%B"])
    }

    pub fn all_commit_bodies(&self) -> String {
        run_git(&self.repo, &["log", "--format=%B"])
    }

    pub fn git(&self, args: &[&str]) -> String {
        self.try_git(args)
            .unwrap_or_else(|why| panic!("git {args:?} in {} failed: {why}", self.repo.display()))
    }

    pub fn try_git(&self, args: &[&str]) -> Result<String, String> {
        self.calls.lock().unwrap().push(args.join(" "));
        try_run_git(&self.repo, args)
    }

    pub fn git_calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

pub fn go(shape: Shape) -> GoWorkspace {
    let root = TempDir::new().expect("a temporary directory for a fixture tree");
    let repo = write_tree(root.path(), "host", &shape);
    commit_tree(&repo, &shape, "the fixture tree");
    GoWorkspace {
        repo: canonical(&repo),
        root,
        calls: Mutex::new(Vec::new()),
    }
}

pub fn go_with_shipped(module: &str, version: &str) -> GoWorkspace {
    go(shipped(module, version))
}

pub fn shallow_clone() -> GoWorkspace {
    let root = TempDir::new().expect("a temporary directory for a fixture tree");
    let shape = direct();
    let origin = write_tree(root.path(), "origin", &shape);
    commit_tree(&origin, &shape, "the fixture tree");
    std::fs::write(origin.join("README.md"), "the host repository\n").unwrap();
    commit_paths(&origin, &["README.md"], "chore: earlier work");

    let url = format!("file://{}", canonical(&origin).display());
    run_git(
        root.path(),
        &["clone", "--depth", "1", "--quiet", &url, "host"],
    );
    GoWorkspace {
        repo: canonical(&root.path().join("host")),
        root,
        calls: Mutex::new(Vec::new()),
    }
}

pub fn full_clone(bodies: &[&str]) -> GoWorkspace {
    let root = TempDir::new().expect("a temporary directory for a fixture tree");
    let shape = direct();
    let origin = write_tree(root.path(), "origin", &shape);
    commit_tree(&origin, &shape, "the fixture tree");

    let url = format!("file://{}", canonical(&origin).display());
    run_git(root.path(), &["clone", "--quiet", &url, "host"]);

    let repo = root.path().join("host");
    for body in bodies {
        run_git(
            &repo,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "--allow-empty",
                "--quiet",
                "-m",
                body,
            ],
        );
    }
    GoWorkspace {
        repo: canonical(&repo),
        root,
        calls: Mutex::new(Vec::new()),
    }
}

fn write_tree(parent: &Path, name: &str, shape: &Shape) -> PathBuf {
    let repo = parent.join(name);
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("go.mod"), shape.go_mod()).unwrap();
    if let Some(go_sum) = shape.go_sum() {
        std::fs::write(repo.join("go.sum"), go_sum).unwrap();
    }
    repo
}

fn commit_tree(repo: &Path, shape: &Shape, message: &str) {
    run_git(
        repo,
        &["-c", "init.defaultBranch=main", "init", "--quiet", "."],
    );
    let mut paths = vec!["go.mod"];
    if shape.go_sum().is_some() {
        paths.push("go.sum");
    }
    commit_paths(repo, &paths, message);
}

fn commit_paths(repo: &Path, paths: &[&str], message: &str) {
    let mut add = vec!["add", "--"];
    add.extend_from_slice(paths);
    run_git(repo, &add);
    run_git(
        repo,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    );
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|source| panic!("could not resolve {}: {source}", path.display()))
}

fn run_git(dir: &Path, args: &[&str]) -> String {
    try_run_git(dir, args)
        .unwrap_or_else(|why| panic!("git {args:?} in {} failed: {why}", dir.display()))
}

fn try_run_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|source| panic!("could not run git {args:?}: {source}"));
    let stdout = String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string();
    match output.status.success() {
        true => Ok(stdout),
        false => Err(String::from_utf8_lossy(&output.stderr)
            .trim_end()
            .to_string()),
    }
}

const FIXTURE_ADVISORY: &str = "CVE-2026-0008";

const FINDING_CURRENT: &str = "0.24.0";
const FINDING_FIXED: &str = "0.33.0";

pub fn finding(package: &str, package_type: PackageType) -> ProjectedFinding {
    finding_under(FIXTURE_ADVISORY, package, package_type, FINDING_FIXED)
}

const OS_PACKAGE: &str = "openssl";

pub fn finding_fixed_at(package: &str, fixed: &str) -> ProjectedFinding {
    finding_under(FIXTURE_ADVISORY, package, PackageType::Library, fixed)
}

pub fn os_finding(cve: &str) -> ProjectedFinding {
    finding_under(cve, OS_PACKAGE, PackageType::Os, FINDING_FIXED)
}

fn finding_under(
    cve: &str,
    package: &str,
    package_type: PackageType,
    fixed: &str,
) -> ProjectedFinding {
    ProjectedFinding {
        cve: AdvisoryId::parse(cve).expect("a fixture advisory id parses"),
        package: package.to_string(),
        current: FINDING_CURRENT.to_string(),
        fixed_version: Some(fixed.to_string()),
        severity: Severity::Critical,
        package_type,
    }
}

pub fn go_stub() -> ProgramRef {
    ProgramRef {
        program: env!("CARGO_BIN_EXE_go_stub").to_string(),
        args: Vec::new(),
    }
}

pub struct CommitLog {
    root: TempDir,
    repo: PathBuf,
    raw: String,
}

impl CommitLog {
    pub fn raw(&self) -> &str {
        &self.raw
    }

    pub fn path(&self) -> &Path {
        &self.repo
    }
}

pub fn log_of(bodies: &[&str]) -> CommitLog {
    let root = TempDir::new().expect("a temporary directory for a fixture history");
    let repo = root.path().join("history");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(
        &repo,
        &["-c", "init.defaultBranch=main", "init", "--quiet", "."],
    );
    for body in bodies {
        run_git(
            &repo,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "--allow-empty",
                "--quiet",
                "-m",
                body,
            ],
        );
    }
    let raw = match bodies.is_empty() {
        true => String::new(),
        false => run_git(&repo, &["log", "--format=%B"]),
    };
    CommitLog {
        repo: canonical(&repo),
        root,
        raw,
    }
}

pub struct RecordedCalls {
    calls: Mutex<Vec<String>>,
}

impl RecordedCalls {
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl Spawn for RecordedCalls {
    fn run(&self, program: &str, args: &[&str], dir: &Path) -> Result<Ran, DedupError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{program} {}", args.join(" ")));
        Local.run(program, args, dir)
    }
}

pub fn forge_recording_calls() -> RecordedCalls {
    RecordedCalls {
        calls: Mutex::new(Vec::new()),
    }
}

pub fn image() -> String {
    "ghcr.io/acme/widget:fiddle-fixture".to_string()
}

const SCRIPTED_SCAN_TIMEOUT: Duration = Duration::from_secs(60);

pub const FIXTURE_CLIENT_ID: &str = "fiddle-client-1c93f0a5";

pub const ARMS: [&str; 12] = [
    "ok",
    "library-clean",
    "no-client-version",
    "blank-client-version",
    "no-scan-origin",
    "blank-scan-origin",
    "exit-nonzero-with-file",
    "exit-nonzero-no-file",
    "empty-file",
    "unparseable-file",
    "no-such-image",
    "no-daemon",
];

pub fn scanner_with(program: ProgramRef) -> ScriptedScanner {
    let named = std::env::var_os("WIZ_CONFIG_DIR").filter(|value| !value.is_empty());
    if let Some(directory) = &named {
        let holds_a_login =
            std::fs::read_dir(directory).is_ok_and(|mut entries| entries.next().is_some());
        assert!(
            holds_a_login,
            "WIZ_CONFIG_DIR names {directory:?}, which holds no login, and the \
             adapter passes that variable through, so the scripted scanner would \
             refuse every arm. Unset it, or log in there."
        );
    }
    let scratch = TempDir::new().expect("a temporary directory for a scan's report");
    ScriptedScanner {
        wizcli: Wizcli::new(
            PathBuf::from(program.program),
            program.args,
            scratch.path().to_path_buf(),
            SCRIPTED_SCAN_TIMEOUT,
            CancellationToken::new(),
        ),
        scratch,
    }
}

pub fn scanner_recording_env() -> ScriptedScanner {
    scanner_with(wiz_stub("ok"))
}

pub struct ScriptedScanner {
    wizcli: Wizcli,
    scratch: TempDir,
}

#[async_trait::async_trait]
impl Scanner for ScriptedScanner {
    async fn scan(&self, image: &str) -> Result<ScanReport, ScanError> {
        self.wizcli.scan(image).await
    }
}

const CHILD_RECORD: &str = "child.json";

impl ScriptedScanner {
    pub fn scratch(&self) -> &str {
        self.scratch
            .path()
            .to_str()
            .expect("a temporary directory whose path is UTF-8")
    }

    pub fn child_env(&self) -> BTreeMap<String, String> {
        self.child()["env"]
            .as_array()
            .expect("the scripted scanner records its environment as an array")
            .iter()
            .map(|entry| {
                let entry = entry.as_str().expect("an environment entry is a string");
                let (name, value) = entry
                    .split_once('=')
                    .unwrap_or_else(|| panic!("{entry} is not a NAME=VALUE entry"));
                (name.to_string(), value.to_string())
            })
            .collect()
    }

    pub fn child_env_names(&self) -> Vec<String> {
        self.child_env().into_keys().collect()
    }

    pub fn child_argv(&self) -> Vec<String> {
        self.child()["argv"]
            .as_array()
            .expect("the scripted scanner records its argv as an array")
            .iter()
            .map(|argument| {
                argument
                    .as_str()
                    .expect("an argument is a string")
                    .to_string()
            })
            .collect()
    }

    fn child(&self) -> serde_json::Value {
        let record = self.scratch.path().join(CHILD_RECORD);
        let raw = std::fs::read_to_string(&record).unwrap_or_else(|source| {
            panic!(
                "no record at {}, so no child of this scan was observed: {source}",
                record.display()
            )
        });
        serde_json::from_str(&raw)
            .unwrap_or_else(|source| panic!("{} is not a record: {source}", record.display()))
    }
}

pub fn arm_was_exercised(arm: &str, outcome: &Result<ScanReport, ScanError>) -> bool {
    match arm {
        "ok" | "library-clean" | "exit-nonzero-with-file" => outcome.is_ok(),
        "exit-nonzero-no-file" => matches!(outcome, Err(ScanError::Failed { .. })),
        "empty-file" => matches!(outcome, Err(ScanError::NoOutput { .. })),
        "unparseable-file"
        | "no-client-version"
        | "blank-client-version"
        | "no-scan-origin"
        | "blank-scan-origin" => {
            matches!(outcome, Err(ScanError::Unparseable { .. }))
        }
        "no-such-image" => matches!(outcome, Err(ScanError::ImageAbsent { .. })),
        "no-daemon" => matches!(outcome, Err(ScanError::DaemonUnreachable { .. })),
        other => panic!("{other} is not an arm the scripted wizcli has; see ARMS"),
    }
}

pub fn arm_exits_with(arm: &str) -> i32 {
    match arm {
        "ok"
        | "library-clean"
        | "no-client-version"
        | "blank-client-version"
        | "no-scan-origin"
        | "blank-scan-origin"
        | "empty-file"
        | "unparseable-file" => 0,
        "exit-nonzero-with-file" | "exit-nonzero-no-file" | "no-such-image" | "no-daemon" => 3,
        other => panic!("{other} is not an arm the scripted wizcli has; see ARMS"),
    }
}

pub fn observed_exit(arm: &str) -> i32 {
    let scratch = TempDir::new().expect("a temporary directory for a scan's report");
    let stub = wiz_stub(arm);
    let output = std::process::Command::new(&stub.program)
        .args(&stub.args)
        .env_remove("WIZ_CONFIG_DIR")
        .arg("--json-output-file")
        .arg(scratch.path().join("scan.json"))
        .arg(image())
        .output()
        .unwrap_or_else(|source| panic!("could not run the scripted wizcli for {arm}: {source}"));
    output.status.code().unwrap_or_else(|| {
        panic!(
            "the scripted wizcli died by signal on {arm}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

pub const GO_BUILD: &str = "go build ./...";

pub const GO_FMT: &str = "go fmt ./...";

pub const GO_VET: &str = "go vet ./...";

pub const DOCKER_BUILD: &str = "docker build .";

pub const WIZCLI_RESCAN: &str = "wizcli docker scan";

pub const WRAPPER: &str = "/opt/acme/bin/tidy-sources --check";

pub fn contract() -> Contract {
    Contract::of(vec![
        declared(GO_BUILD, Success::ExitZero),
        declared(GO_FMT, Success::ExitZeroAndNoOutput),
        declared(GO_VET, Success::ExitZero),
        declared(DOCKER_BUILD, Success::ExitZero),
        declared(WIZCLI_RESCAN, Success::ArtefactWritten),
    ])
}

pub fn contract_with(name: &str, command_line: &str, success: Success) -> Contract {
    let mut contract = contract();
    let at = contract
        .checks
        .iter()
        .position(|check| check.name() == name)
        .unwrap_or_else(|| panic!("{name} is not one of the five checks in the contract"));
    contract.checks[at] = declared(command_line, success);
    contract
}

const FIXTURE_SCANNER_VERSION: &str = "1.2.3";

const REPAIRED_ADVISORY: &str = "CVE-2026-4242";

pub fn contract_for(cves: &[&str]) -> Contract {
    let mut contract = contract();
    contract.repair = Some(Repair {
        must_clear: advisories(cves),
        input: advisories(cves),
        scanned_at: FIXTURE_SCANNER_VERSION.to_string(),
    });
    contract
}

pub fn and_the_input_also_reported(mut contract: Contract, cves: &[&str]) -> Contract {
    let repair = contract
        .repair
        .as_mut()
        .expect("a contract with a repair premise to widen");
    repair.input.extend(advisories(cves));
    contract
}

pub fn contract_scanned_by(version: &str) -> Contract {
    let mut contract = contract_for(&[REPAIRED_ADVISORY]);
    contract
        .repair
        .as_mut()
        .expect("contract_for supplies a repair premise")
        .scanned_at = version.to_string();
    contract
}

pub fn contract_for_a_partially_reported_rescan() -> Contract {
    let mut already_reported: Vec<&str> = DEFAULT_LIBRARY_CVES.to_vec();
    already_reported.extend(DEFAULT_OS_CVES);
    and_the_input_also_reported(contract_for(&[REPAIRED_ADVISORY]), &already_reported)
}

fn advisories(cves: &[&str]) -> Vec<AdvisoryId> {
    cves.iter()
        .map(|cve| AdvisoryId::parse(cve).expect("a fixture advisory id parses"))
        .collect()
}

fn declared(command_line: &str, success: Success) -> Check {
    let mut words = command_line.split_whitespace().map(str::to_string);
    let program = words
        .next()
        .unwrap_or_else(|| panic!("a check needs a program, and {command_line:?} names none"));
    Check {
        program,
        args: words.collect(),
        success,
    }
}

#[derive(Debug)]
pub struct Exit(i32);

pub fn exit(code: i32) -> Exit {
    Exit(code)
}

#[derive(Debug)]
pub struct Stdout(String);

pub fn stdout(text: &str) -> Stdout {
    Stdout(text.to_string())
}

#[derive(Debug)]
enum Scripted {
    Answered { exit_code: i32, stdout: String },
    CannotStart,
}

enum Scanned {
    ByProgram(ScriptedScanner),
    AsReport(ScanReport),
}

pub struct ScriptedTree {
    scripted: BTreeMap<String, Scripted>,
    scanner: Scanned,
    ran: Mutex<Vec<String>>,
}

pub fn green_tree() -> ScriptedTree {
    ScriptedTree {
        scripted: BTreeMap::new(),
        scanner: Scanned::ByProgram(scanner_with(wiz_stub("ok"))),
        ran: Mutex::new(Vec::new()),
    }
}

pub fn tree_whose_rescan_reports(cves: &[&str]) -> ScriptedTree {
    tree_reporting(
        report_with(libraries(cves), os_packages(&[])),
        FIXTURE_SCANNER_VERSION,
    )
}

pub fn tree_whose_rescan_reports_in_os_array(cves: &[&str]) -> ScriptedTree {
    tree_reporting(
        report_with(libraries(&[]), os_packages(cves)),
        FIXTURE_SCANNER_VERSION,
    )
}

pub fn tree_rescanned_by(version: &str) -> ScriptedTree {
    tree_reporting(report_with(libraries(&[]), os_packages(&[])), version)
}

pub fn tree_whose_rescan_omits_the_os_array() -> ScriptedTree {
    tree_reporting(report_with_os_absent(), FIXTURE_SCANNER_VERSION)
}

pub fn tree_whose_rescan_reports_no_os_packages() -> ScriptedTree {
    tree_reporting(report_with_os_empty(), FIXTURE_SCANNER_VERSION)
}

pub fn tree_whose_rescan_omits_the_library_array() -> ScriptedTree {
    tree_reporting(report_with_libraries_absent(), FIXTURE_SCANNER_VERSION)
}

pub fn tree_whose_rescan_omits_the_os_array_and_reports(cves: &[&str]) -> ScriptedTree {
    let mut report = rescan_report(
        report_with(libraries(cves), os_packages(&[])),
        FIXTURE_SCANNER_VERSION,
    );
    report.document["result"]
        .as_object_mut()
        .expect("a fixture scanner document's result is an object")
        .remove("osPackages");
    ScriptedTree {
        scripted: BTreeMap::new(),
        scanner: Scanned::AsReport(report),
        ran: Mutex::new(Vec::new()),
    }
}

pub fn tree_whose_rescan_is_unreadable() -> ScriptedTree {
    let mut report = rescan_report(
        report_with(libraries(&[]), os_packages(&[])),
        FIXTURE_SCANNER_VERSION,
    );
    report.document["result"]["libraries"] = serde_json::json!({});
    ScriptedTree {
        scripted: BTreeMap::new(),
        scanner: Scanned::AsReport(report),
        ran: Mutex::new(Vec::new()),
    }
}

fn tree_reporting(document: Report, version: &str) -> ScriptedTree {
    ScriptedTree {
        scripted: BTreeMap::new(),
        scanner: Scanned::AsReport(rescan_report(document, version)),
        ran: Mutex::new(Vec::new()),
    }
}

fn rescan_report(document: Report, version: &str) -> ScanReport {
    ScanReport {
        document: serde_json::from_str(document.raw())
            .expect("a fixture scanner document is valid JSON"),
        scanner_version: version.to_string(),
        image_digest: FIXTURE_IMAGE_DIGEST.to_string(),
    }
}

pub fn tree_where(name: &str, exit: Exit, stdout: Stdout) -> ScriptedTree {
    green_tree().where_check(name, exit, stdout)
}

impl ScriptedTree {
    pub fn where_check(mut self, name: &str, exit: Exit, stdout: Stdout) -> Self {
        self.script(
            name,
            Scripted::Answered {
                exit_code: exit.0,
                stdout: stdout.0,
            },
        );
        self
    }

    pub fn where_check_cannot_start(mut self, name: &str) -> Self {
        self.script(name, Scripted::CannotStart);
        self
    }

    pub fn scanned_by(mut self, arm: &str) -> Self {
        self.scanner = Scanned::ByProgram(scanner_with(wiz_stub(arm)));
        self
    }

    pub fn ran(&self) -> Vec<String> {
        self.ran.lock().unwrap().clone()
    }

    fn script(&mut self, name: &str, scripted: Scripted) {
        if let Some(already) = self.scripted.insert(name.to_string(), scripted) {
            panic!("{name} was already scripted as {already:?}");
        }
    }
}

#[async_trait::async_trait]
impl Tree for ScriptedTree {
    async fn run(&self, check: &Check) -> Result<Answered, Unanswered> {
        self.ran.lock().unwrap().push(check.name());
        match self.scripted.get(&check.name()) {
            Some(Scripted::CannotStart) => Err(Unanswered::NotStarted {
                program: check.program.clone(),
                source: std::io::Error::from(std::io::ErrorKind::NotFound),
            }),
            Some(Scripted::Answered { exit_code, stdout }) => Ok(Answered {
                exit_code: *exit_code,
                stdout: stdout.clone(),
                stderr: String::new(),
            }),
            None => Ok(Answered {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            }),
        }
    }

    async fn scan(&self, check: &Check) -> Result<ScanReport, ScanError> {
        self.ran.lock().unwrap().push(check.name());
        match &self.scanner {
            Scanned::ByProgram(scanner) => scanner.scan(&image()).await,
            Scanned::AsReport(report) => Ok(report.clone()),
        }
    }
}

pub fn findings_for(cves: &[&str]) -> Vec<ProjectedFinding> {
    cves.iter()
        .map(|cve| {
            finding_under(
                cve,
                &format!("package-for-{cve}"),
                PackageType::Library,
                FINDING_FIXED,
            )
        })
        .collect()
}

pub fn advisories_of(findings: &[ProjectedFinding]) -> Vec<AdvisoryId> {
    let mut advisories: Vec<AdvisoryId> = Vec::new();
    for finding in findings {
        if !advisories.contains(&finding.cve) {
            advisories.push(finding.cve.clone());
        }
    }
    advisories
}

pub fn every_fixture_grade() -> Severities {
    Severities::default()
}

pub fn document_of(report: &Report) -> serde_json::Value {
    serde_json::from_str(report.raw()).expect("a fixture document is JSON")
}

pub fn scan_of(document: serde_json::Value) -> ScanReport {
    ScanReport {
        document,
        scanner_version: "wizcli 0.0.0-fixture".to_string(),
        image_digest: "sha256:fixture".to_string(),
    }
}

pub fn scanned(report: &Report) -> ScanReport {
    scan_of(document_of(report))
}

pub const MIGRATION_ATTEMPT: &str = "01JQZX00000000000000000M4";

const MIGRATION_CHECK_TIMEOUT: Duration = Duration::from_secs(60);

pub const MIGRATION_SOURCE: &str = "main.go";

pub const MIGRATION_SOURCE_BEFORE: &str = "\
package main

func main() {
\tlegacyName()
}

func legacyName() {}
";

pub const MIGRATION_TEST_SOURCE: &str = "main_test.go";

pub const MIGRATION_TEST_BEFORE: &str = "\
package main

import \"testing\"

func TestLegacyName(t *testing.T) {
\tlegacyName()
\tif testing.Short() {
\t\tt.Errorf(\"this test must run even in short mode\")
\t}
}
";

pub struct MigrationWorld {
    pub tree: GoWorkspace,

    pub report: Report,

    pub findings: Vec<ProjectedFinding>,

    workspaces: TempDir,
}

pub async fn migration_world() -> MigrationWorld {
    let report = report_with_advisory_description(SENTINEL_PROSE);
    assert!(
        report.raw().contains(SENTINEL_PROSE),
        "the document a migration's findings come from has to carry the prose, \
         or no exclusion asserted downstream of it means anything"
    );

    let projection =
        project(&scanned(&report), &every_fixture_grade()).expect("a fixture document projects");
    let fixable: Vec<ProjectedFinding> = projection.all().cloned().collect();
    assert!(
        !fixable.is_empty(),
        "a migration is about findings there is a fix to write, and this \
         document produced none"
    );

    let tree = go_with_shipped(&fixable[0].package, &fixable[0].current);
    std::fs::write(tree.path().join(MIGRATION_SOURCE), MIGRATION_SOURCE_BEFORE)
        .expect("the fixture tree is writable");
    std::fs::write(
        tree.path().join(MIGRATION_TEST_SOURCE),
        MIGRATION_TEST_BEFORE,
    )
    .expect("the fixture tree is writable");
    tree.git(&["add", "--", MIGRATION_SOURCE, MIGRATION_TEST_SOURCE]);
    tree.git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "the call site a migration rewrites",
    ]);

    MigrationWorld {
        findings: fixable,
        tree,
        report,
        workspaces: TempDir::new().expect("a temporary directory for worktrees"),
    }
}

impl MigrationWorld {
    pub fn workspace_root(&self) -> PathBuf {
        self.workspaces
            .path()
            .join(HOST_ROOT.trim_start_matches('/'))
    }

    pub fn config(&self) -> MigrationConfig {
        let go = go_stub();
        let mut args = go.args.clone();
        args.extend(
            ["list", "-m", "-json", self.checked_package().as_str()]
                .iter()
                .map(|arg| arg.to_string()),
        );
        MigrationConfig {
            check: WorkspaceCommand {
                program: go.program,
                args,
                timeout: MIGRATION_CHECK_TIMEOUT,
            },
            commands: std::sync::Arc::new(Vec::new()),
            command_timeout: MIGRATION_CHECK_TIMEOUT,
            budget: AgentBudget {
                max_turns: 8,
                max_tokens: 4096,
                deadline: Duration::from_secs(300),
                max_changed_files: 16,
                tool_timeout: MIGRATION_CHECK_TIMEOUT,
            },
            redaction: Redaction::unknown(),
            cancel: CancellationToken::new(),
        }
    }

    pub fn checked_package(&self) -> String {
        self.findings[0].package.clone()
    }

    pub fn attempt(&self) -> AttemptId {
        AttemptId(MIGRATION_ATTEMPT.to_string())
    }

    pub fn workspace(&self) -> Arc<Workspace> {
        Arc::new(
            Workspace::create(
                self.tree.path(),
                &self.workspace_root(),
                &self.attempt(),
                CancellationToken::new(),
            )
            .expect("a worktree of the migration world's tree"),
        )
    }
}

const LANDING_BUMPED_VERSION: &str = "v0.40.0";

pub const LANDING_UNRELATED: &str = "notes.txt";

const LANDING_UNRELATED_BEFORE: &str = "the host repository\n";

pub const LANDING_CREATED: &str = "vendor_notes.md";

pub struct LandingWorld {
    pub tree: GoWorkspace,

    pub findings: Vec<ProjectedFinding>,

    pub changed: Vec<WorkspacePath>,

    pub history_before: String,
}

pub fn landing_world(cves: &[&str]) -> LandingWorld {
    let tree = go(direct());

    std::fs::write(
        tree.path().join(LANDING_UNRELATED),
        LANDING_UNRELATED_BEFORE,
    )
    .expect("the fixture tree is writable");
    commit_paths(
        tree.path(),
        &[LANDING_UNRELATED],
        "chore: a file no bump touches",
    );

    let bumped = shipped(DIRECT_MODULE, LANDING_BUMPED_VERSION);
    std::fs::write(tree.path().join("go.mod"), bumped.go_mod())
        .expect("the fixture tree is writable");
    std::fs::write(
        tree.path().join("go.sum"),
        bumped
            .go_sum()
            .expect("a tree with a requirement has a go.sum"),
    )
    .expect("the fixture tree is writable");
    std::fs::write(
        tree.path().join(LANDING_UNRELATED),
        format!("{LANDING_UNRELATED_BEFORE}and a line nobody asked the attempt about\n"),
    )
    .expect("the fixture tree is writable");

    assert!(
        !tree.is_clean_at(&["go.mod", "go.sum"]),
        "a landing world's bump has to have changed the tree"
    );
    assert!(
        !tree.is_clean_at(&[LANDING_UNRELATED]),
        "{LANDING_UNRELATED} has to be dirty, or staging by name and staging by \
         directory produce the same commit"
    );

    LandingWorld {
        findings: findings_for(cves),
        changed: workspace_paths(&["go.mod", "go.sum"]),
        history_before: tree.all_commit_bodies(),
        tree,
    }
}

impl LandingWorld {
    pub fn and_a_created_file(mut self) -> Self {
        std::fs::write(
            self.tree.path().join(LANDING_CREATED),
            "vendored, by the attempt\n",
        )
        .expect("the fixture tree is writable");
        self.changed = workspace_paths(&["go.mod", "go.sum", LANDING_CREATED]);
        self
    }
}

fn workspace_paths(paths: &[&str]) -> Vec<WorkspacePath> {
    paths
        .iter()
        .map(|path| WorkspacePath::parse(path).expect("a fixture path is inside the workspace"))
        .collect()
}

pub struct LandingWorktree {
    pub workspace: Workspace,

    pub changed: Vec<WorkspacePath>,

    _root: TempDir,
}

pub fn landing_worktree(world: &LandingWorld) -> LandingWorktree {
    let root = TempDir::new().expect("a temporary directory for worktrees");
    let workspace = Workspace::create(
        world.tree.path(),
        root.path(),
        &AttemptId(MIGRATION_ATTEMPT.to_string()),
        CancellationToken::new(),
    )
    .expect("a worktree of the fixture tree");

    let bumped = shipped(DIRECT_MODULE, LANDING_BUMPED_VERSION);
    std::fs::write(workspace.root().join("go.mod"), bumped.go_mod())
        .expect("the worktree is writable");
    std::fs::write(
        workspace.root().join("go.sum"),
        bumped
            .go_sum()
            .expect("a tree with a requirement has a go.sum"),
    )
    .expect("the worktree is writable");
    assert_eq!(
        workspace
            .changed_files()
            .expect("git can describe the worktree")
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>(),
        ["go.mod", "go.sum"],
        "the premise: the bump really reached the worktree, and only it did"
    );

    LandingWorktree {
        changed: workspace_paths(&["go.mod", "go.sum"]),
        workspace,
        _root: root,
    }
}

pub fn ask_git(dir: &Path, args: &[&str]) -> String {
    run_git(dir, args)
}

pub fn try_ask_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    try_run_git(dir, args)
}

#[async_trait::async_trait]
impl Git for GoWorkspace {
    async fn run(&self, args: &[&str]) -> Result<String, CapabilityError> {
        self.try_git(args).map_err(|stderr| {
            CapabilityError::Workspace(WorkspaceError::Git {
                command: args.join(" "),
                stderr,
            })
        })
    }

    async fn fetch(&self, branch: &str) -> Result<(), CapabilityError> {
        let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
        self.run(&["fetch", "--no-tags", "--quiet", "origin", &refspec])
            .await
            .map(|_output| ())
    }
}

pub const ON_THE_SHARED_BRANCH: &str = "shared_branch_marker.txt";

pub const ONLY_ON_THE_REMOTE_BASE: &str = "moved_on.txt";

pub struct RemoteWorld {
    pub tree: GoWorkspace,

    pub findings: Vec<ProjectedFinding>,

    pub base_revision: String,

    pub pr_head: Option<String>,

    pub stale_main: String,

    pub stale_head: Option<String>,
}

pub fn remote_world(remote: &Path, head_branch: Option<&str>, cves: &[&str]) -> RemoteWorld {
    run_git(
        remote.parent().expect("the remote has a parent directory"),
        &[
            "-c",
            "init.defaultBranch=main",
            "init",
            "--quiet",
            "--bare",
            &remote.display().to_string(),
        ],
    );

    let seed_root = TempDir::new().expect("a temporary directory for the seed repository");
    let seed = write_tree(seed_root.path(), "seed", &direct());
    commit_tree(
        &seed,
        &direct(),
        "the base, as it was when the clone was taken",
    );
    let cloned_from = run_git(&seed, &["rev-parse", "HEAD"]);
    run_git(
        &seed,
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    run_git(
        &seed,
        &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
    );

    let root = TempDir::new().expect("a temporary directory for the clone");
    let repo = root.path().join("clone");
    run_git(
        root.path(),
        &[
            "clone",
            "--quiet",
            &remote.display().to_string(),
            &repo.display().to_string(),
        ],
    );

    std::fs::write(seed.join(ONLY_ON_THE_REMOTE_BASE), "the base moved on\n")
        .expect("the seed repository is writable");
    commit_paths(
        &seed,
        &[ONLY_ON_THE_REMOTE_BASE],
        "chore: the base moved on",
    );
    let base_revision = run_git(&seed, &["rev-parse", "HEAD"]);
    run_git(
        &seed,
        &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
    );

    let pr_head = head_branch.map(|branch| {
        run_git(
            &seed,
            &["checkout", "--quiet", "-b", "shared", &cloned_from],
        );
        std::fs::write(
            seed.join(ON_THE_SHARED_BRANCH),
            "opened by an earlier run\n",
        )
        .expect("the seed repository is writable");
        commit_paths(&seed, &[ON_THE_SHARED_BRANCH], "fix: an earlier run's bump");
        let head = run_git(&seed, &["rev-parse", "HEAD"]);
        run_git(
            &seed,
            &[
                "push",
                "--quiet",
                "origin",
                &format!("HEAD:refs/heads/{branch}"),
            ],
        );
        head
    });

    std::fs::write(repo.join("stale.txt"), "left behind by an earlier run\n")
        .expect("the clone is writable");
    commit_paths(&repo, &["stale.txt"], "chore: a commit only this clone has");
    let stale_main = run_git(&repo, &["rev-parse", "HEAD"]);

    let stale_head = head_branch.map(|branch| {
        run_git(
            &repo,
            &["branch", "--no-track", branch, cloned_from.as_str()],
        );
        run_git(&repo, &["rev-parse", &format!("refs/heads/{branch}")])
    });

    let distinct: Vec<&String> = [
        Some(&base_revision),
        pr_head.as_ref(),
        Some(&stale_main),
        stale_head.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut deduped = distinct.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        distinct.len(),
        "the remote's tips and the clone's stale refs must all differ, or a \
         checkout that took the wrong one would be indistinguishable: {distinct:?}"
    );
    assert_ne!(
        run_git(&repo, &["rev-parse", "refs/remotes/origin/main"]),
        base_revision,
        "the clone's idea of origin/main has to be stale before the run fetches, \
         or the fetch is doing nothing observable"
    );

    RemoteWorld {
        tree: GoWorkspace {
            repo: canonical(&repo),
            root,
            calls: Mutex::new(Vec::new()),
        },
        findings: findings_for(cves),
        base_revision,
        pr_head,
        stale_main,
        stale_head,
    }
}

impl RemoteWorld {
    pub fn bump_into(&self, worktree: &Path) -> Vec<WorkspacePath> {
        let bumped = shipped(DIRECT_MODULE, LANDING_BUMPED_VERSION);
        std::fs::write(worktree.join("go.mod"), bumped.go_mod()).expect("the worktree is writable");
        std::fs::write(
            worktree.join("go.sum"),
            bumped
                .go_sum()
                .expect("a tree with a requirement has a go.sum"),
        )
        .expect("the worktree is writable");
        assert!(
            !run_git(
                worktree,
                &["status", "--porcelain", "--", "go.mod", "go.sum"]
            )
            .is_empty(),
            "the bump has to have changed the worktree, or the landing commits nothing"
        );
        workspace_paths(&["go.mod", "go.sum"])
    }
}
