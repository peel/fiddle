mod support;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use support::{
    accepted, body_of, calls, check_stub_binary, completion, gh_stub_binary, git, git_says,
    reports, toml_string, walkdir_files, wiz_stub_binary, Reply, Scenario, StubGateway,
    CREDENTIAL_VARS,
};
use tempfile::TempDir;

const MODULE: &str = "golang.org/x/crypto";
const VULNERABLE_VERSION: &str = "v0.31.0";
const FIXED_VERSION: &str = "v0.35.0";

const VULNERABLE: &str = "cve-vulnerable";
const FIXED: &str = "cve-fixed";

const REGISTRY: &str = "cve-registry";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the manifest directory is two levels below the repository root")
}

fn fixture(name: &str) -> PathBuf {
    repo_root().join("tests/fixtures").join(name)
}

fn tracked_files(tree: &Path) -> BTreeMap<String, Vec<u8>> {
    let out = Command::new("git")
        .args(["ls-files", "-z", "--"])
        .arg(tree)
        .current_dir(repo_root())
        .output()
        .expect("git is on PATH");
    assert!(
        out.status.success(),
        "git ls-files failed for {}: {}",
        tree.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let listing = String::from_utf8(out.stdout).expect("git prints paths as UTF-8 here");
    let prefix = tree
        .strip_prefix(repo_root())
        .expect("a fixture tree is inside the repository")
        .to_str()
        .expect("the fixture path is UTF-8");

    let mut files = BTreeMap::new();
    for path in listing.split('\0').filter(|entry| !entry.is_empty()) {
        let relative = path
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or_else(|| panic!("git listed {path}, which is not under {prefix}"));
        let bytes = std::fs::read(repo_root().join(path))
            .unwrap_or_else(|source| panic!("git tracks {path} but it does not read: {source}"));
        files.insert(relative.to_string(), bytes);
    }
    assert!(
        !files.is_empty(),
        "{} tracks no files: the fixture is absent, or it was never committed",
        tree.display()
    );
    files
}

fn changed_paths(a: &BTreeMap<String, Vec<u8>>, b: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut changed: Vec<String> = a
        .keys()
        .chain(b.keys())
        .filter(|path| a.get(*path) != b.get(*path))
        .cloned()
        .collect();
    changed.sort();
    changed.dedup();
    changed
}

struct Registry {
    root: TempDir,
}

impl Registry {
    fn serve() -> Self {
        let registry = Self {
            root: TempDir::new().expect("a temporary directory"),
        };
        let source = fixture(REGISTRY).join(MODULE);
        let mut versions: Vec<String> = std::fs::read_dir(&source)
            .unwrap_or_else(|source_err| {
                panic!("no releases in {}: {source_err}", source.display())
            })
            .map(|entry| entry.expect("a readable directory entry").file_name())
            .map(|name| {
                name.into_string()
                    .expect("a release directory is named for its version, in UTF-8")
            })
            .collect();
        versions.sort();

        let at = registry.root.path().join("proxy").join(MODULE).join("@v");
        std::fs::create_dir_all(&at).expect("the temporary directory is writable");
        for version in &versions {
            let release = source.join(version);
            let files = files_under(&release);
            let go_mod = files
                .get("go.mod")
                .unwrap_or_else(|| panic!("{} has no go.mod", release.display()));

            std::fs::write(
                at.join(format!("{version}.info")),
                format!(r#"{{"Version":"{version}","Time":"2026-01-01T00:00:00Z"}}"#),
            )
            .expect("the temporary directory is writable");
            std::fs::write(at.join(format!("{version}.mod")), go_mod)
                .expect("the temporary directory is writable");
            std::fs::write(
                at.join(format!("{version}.zip")),
                module_zip(MODULE, version, &files),
            )
            .expect("the temporary directory is writable");
        }
        std::fs::write(
            at.join("list"),
            versions
                .iter()
                .map(|version| format!("{version}\n"))
                .collect::<String>(),
        )
        .expect("the temporary directory is writable");

        registry
    }

    fn go<I, S>(&self, dir: &Path, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.go_with_proxy(dir, &self.proxy_url(), args)
    }

    fn go_with_proxy<I, S>(&self, dir: &Path, proxy: &str, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let scratch = self.root.path();
        Command::new(go_binary())
            .args(args)
            .current_dir(dir)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", scratch)
            .env("GOPROXY", proxy)
            .env("GOFLAGS", "-mod=readonly")
            .env("GOSUMDB", "off")
            .env("GOTOOLCHAIN", "local")
            .env("GOENV", "off")
            .env("GOWORK", "off")
            .env("CGO_ENABLED", "0")
            .env("GOMODCACHE", scratch.join("modcache"))
            .env("GOCACHE", scratch.join("buildcache"))
            .env("GOPATH", scratch.join("gopath"))
            .env("GOTMPDIR", scratch)
            .output()
            .expect("go runs")
    }

    fn proxy_url(&self) -> String {
        format!("file://{}", self.root.path().join("proxy").display())
    }

    fn scratch(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        let cache = self.root.path().join("modcache");
        if cache.exists() {
            let _ = self.go(self.root.path(), ["clean", "-modcache"]);
        }
    }
}

fn go_binary() -> PathBuf {
    which("go").unwrap_or_else(|| {
        panic!(
            "no `go` on PATH: this suite compiles the CVE fixtures, and the toolchain \
             is in the development shell — run under `nix develop -c`"
        )
    })
}

fn which(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    })
}

fn files_under(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|source| panic!("{} does not read: {source}", dir.display()))
        {
            let entry = entry.expect("a readable directory entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .expect("the walk started at root")
                .to_str()
                .expect("fixture paths are UTF-8")
                .replace('\\', "/");
            files.insert(relative, std::fs::read(&path).expect("a readable file"));
        }
    }
    files
}

fn module_zip(module: &str, version: &str, files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    const DOS_TIME: u16 = 0;
    const DOS_DATE: u16 = 0x0021;

    let mut body: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    let mut count = 0u16;

    for (relative, bytes) in files {
        let name = format!("{module}@{version}/{relative}");
        let offset = body.len() as u32;
        let crc = crc32(bytes);
        let size = bytes.len() as u32;

        body.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        body.extend_from_slice(&20u16.to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(&DOS_TIME.to_le_bytes());
        body.extend_from_slice(&DOS_DATE.to_le_bytes());
        body.extend_from_slice(&crc.to_le_bytes());
        body.extend_from_slice(&size.to_le_bytes());
        body.extend_from_slice(&size.to_le_bytes());
        body.extend_from_slice(&(name.len() as u16).to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(name.as_bytes());
        body.extend_from_slice(bytes);

        directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        directory.extend_from_slice(&20u16.to_le_bytes());
        directory.extend_from_slice(&20u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&DOS_TIME.to_le_bytes());
        directory.extend_from_slice(&DOS_DATE.to_le_bytes());
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&(name.len() as u16).to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes());
        directory.extend_from_slice(&0u32.to_le_bytes());
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());

        count += 1;
    }

    let directory_offset = body.len() as u32;
    let directory_size = directory.len() as u32;
    body.extend_from_slice(&directory);
    body.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body.extend_from_slice(&count.to_le_bytes());
    body.extend_from_slice(&count.to_le_bytes());
    body.extend_from_slice(&directory_size.to_le_bytes());
    body.extend_from_slice(&directory_offset.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());
    body
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn build(registry: &Registry, tree: &Path, output: &str) -> Result<PathBuf, String> {
    let binary = registry.scratch(output);
    let out = registry.go(
        tree,
        [
            OsStr::new("build"),
            OsStr::new("-o"),
            binary.as_os_str(),
            OsStr::new("."),
        ],
    );
    if out.status.success() {
        Ok(binary)
    } else {
        Err(format!(
            "`go build` in {} exited {:?}\nstdout: {}\nstderr: {}",
            tree.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

fn run(binary: &Path) -> String {
    let out = Command::new(binary)
        .env_clear()
        .output()
        .unwrap_or_else(|source| panic!("{} does not run: {source}", binary.display()));
    assert!(
        out.status.success(),
        "{} exited {:?}: {}",
        binary.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("the fixture prints UTF-8")
}

#[test]
fn the_two_fixtures_differ_only_in_the_dependency_under_remediation() {
    let (a, b) = (fixture(VULNERABLE), fixture(FIXED));
    let changed = changed_paths(&tracked_files(&a), &tracked_files(&b));
    assert_eq!(
        changed,
        ["go.mod", "go.sum"],
        "a fixture pair that differs elsewhere cannot isolate the mitigation"
    );

    let registry = Registry::serve();
    if let Err(why) = build(&registry, &a, "vulnerable") {
        panic!("both must build, or a failing check proves nothing about the fix: {why}");
    }
    if let Err(why) = build(&registry, &b, "fixed") {
        panic!("both must build, or a passing check proves nothing about the fix: {why}");
    }
}

#[test]
fn the_only_difference_inside_go_mod_is_the_version_of_the_module_under_remediation() {
    let vulnerable = read_fixture_file(VULNERABLE, "go.mod");
    let fixed = read_fixture_file(FIXED, "go.mod");

    let differing: Vec<(&str, &str)> = vulnerable
        .lines()
        .zip(fixed.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(
        vulnerable.lines().count(),
        fixed.lines().count(),
        "the two manifests are the same file with one version changed, so they \
         have the same number of lines"
    );
    assert_eq!(
        differing,
        [(
            format!("require {MODULE} {VULNERABLE_VERSION}").as_str(),
            format!("require {MODULE} {FIXED_VERSION}").as_str()
        )],
        "exactly one line differs, and it is the requirement under remediation"
    );
}

#[test]
fn the_dependency_is_load_bearing_so_the_pair_is_two_programs() {
    let registry = Registry::serve();
    let vulnerable = build(&registry, &fixture(VULNERABLE), "vulnerable").expect("it builds");
    let fixed = build(&registry, &fixture(FIXED), "fixed").expect("it builds");

    let (before, after) = (run(&vulnerable), run(&fixed));
    assert_ne!(
        before, after,
        "the two fixtures print the same thing, so the requirement they differ \
         in changes nothing the program does: the dependency is decoration and \
         the pair proves only that two manifests parse"
    );
    assert!(
        before.contains("buffering"),
        "the vulnerable fixture must exhibit the unbounded buffering the \
         advisory is about, and it printed: {before}"
    );
    assert!(
        after.contains("refused"),
        "the fixed fixture must exhibit the bound the fix introduced, and it \
         printed: {after}"
    );
}

#[test]
fn the_build_is_satisfied_by_the_checked_in_registry_and_not_by_a_warm_module_cache() {
    let registry = Registry::serve();
    let out = registry.go_with_proxy(
        &fixture(VULNERABLE),
        "off",
        ["build", "-o", "/dev/null", "."],
    );
    assert!(
        !out.status.success(),
        "a build with GOPROXY=off succeeded, so something outside this fixture \
         is supplying {MODULE}: the module cache is not the fresh one this \
         suite creates, and the offline lanes are proving nothing"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("GOPROXY=off"),
        "the build had to fail for want of a module lookup, and not for some \
         other reason: {stderr}"
    );
}

#[test]
fn a_build_leaves_the_fixture_exactly_as_it_found_it() {
    let tree = fixture(VULNERABLE);
    let before = tracked_files(&tree);
    let registry = Registry::serve();
    build(&registry, &tree, "vulnerable").expect("it builds");
    assert_eq!(
        tracked_files(&tree),
        before,
        "the build rewrote the fixture it was meant to be checking"
    );
}

#[test]
fn the_pair_is_pinned_to_the_module_and_versions_the_shared_scanner_document_names() {
    let table = std::fs::read_to_string(
        repo_root().join("crates/fiddle-runtime/tests/support/document.rs"),
    )
    .expect("the shared scanner documents are where this suite says they are");
    let entry = format!(r#"("{MODULE}", "{VULNERABLE_VERSION}", "{FIXED_VERSION}")"#);
    assert!(
        table.contains(&entry),
        "the fixture pair moves {MODULE} from {VULNERABLE_VERSION} to \
         {FIXED_VERSION}, and document.rs's library table no longer has the row \
         {entry} that says so: a scanner document built from that table would \
         report a finding this fixture pair is not about"
    );

    assert!(
        read_fixture_file(VULNERABLE, "go.mod")
            .contains(&format!("require {MODULE} {VULNERABLE_VERSION}")),
        "the vulnerable fixture must require the version the document reports"
    );
    assert!(
        read_fixture_file(FIXED, "go.mod").contains(&format!("require {MODULE} {FIXED_VERSION}")),
        "the fixed fixture must require the version the document names as fixed"
    );
}

#[test]
fn the_two_library_fixture_is_pinned_to_what_its_world_publishes() {
    let table = std::fs::read_to_string(
        repo_root().join("crates/fiddle-runtime/tests/support/document.rs"),
    )
    .expect("the shared scanner documents are where this suite says they are");
    let entry =
        format!(r#"("{SECOND_MODULE}", "{SECOND_VULNERABLE_VERSION}", "{SECOND_FIXED_VERSION}")"#);
    assert!(
        table.contains(&entry),
        "a document naming two library advisories puts the second one in \
         {SECOND_MODULE}, and document.rs's library table no longer has the row \
         {entry} that says so"
    );

    let proxy = std::fs::read_to_string(
        repo_root().join("crates/fiddle-runtime/tests/support/go_proxy.rs"),
    )
    .expect("the offline module proxy is where this suite says it is");
    for (name, version) in [
        ("INDIRECT_VERSION", SECOND_VULNERABLE_VERSION),
        ("INDIRECT_FIXED", SECOND_FIXED_VERSION),
    ] {
        assert!(
            proxy.contains(&format!(r#"pub const {name}: &str = "{version}";"#)),
            "go_proxy.rs's {name} has to be {version}, which is what the \
             document names for {SECOND_MODULE}"
        );
        assert!(
            proxy.contains(&format!(
                "module: INDIRECT_MODULE,\n        version: {name},"
            )),
            "and the offline upstream has to have *published* it, or the two \
             halves of this world disagree about which releases exist — see \
             this lane's own doc for what that does and does not still guard"
        );
    }

    let manifest = read_fixture_file(TWO_LIBRARIES, "go.mod");
    for (module, version) in [
        (MODULE, VULNERABLE_VERSION),
        (SECOND_MODULE, SECOND_VULNERABLE_VERSION),
    ] {
        assert!(
            manifest.contains(&format!("require {module} {version}")),
            "the two-library fixture must require {module} at the version the \
             document reports as current: {manifest}"
        );
    }
}

#[test]
fn each_dockerfile_copies_only_files_its_fixture_has() {
    for name in [VULNERABLE, FIXED] {
        let dockerfile = read_fixture_file(name, "Dockerfile");
        let mut copied = 0;
        for line in dockerfile.lines() {
            let Some(rest) = line.trim().strip_prefix("COPY ") else {
                continue;
            };
            if rest.contains("--from=") {
                continue;
            }
            let words: Vec<&str> = rest
                .split_whitespace()
                .filter(|word| !word.starts_with("--"))
                .collect();
            assert!(
                words.len() >= 2,
                "{name}/Dockerfile has a COPY with no source and destination: {line}"
            );
            for source in &words[..words.len() - 1] {
                assert!(
                    fixture(name).join(source).exists(),
                    "{name}/Dockerfile copies {source}, which the fixture does not have"
                );
                copied += 1;
            }
        }
        assert!(
            copied > 0,
            "{name}/Dockerfile copies nothing, so it cannot be the recipe for \
             an image of this fixture"
        );
    }
}

fn read_fixture_file(name: &str, file: &str) -> String {
    let path = fixture(name).join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|source| panic!("{} does not read: {source}", path.display()))
}

#[test]
fn the_mitigating_capability_is_selectable_and_an_unknown_one_is_not() {
    let scenario = Scenario::new();

    let selected = scenario.run_raw_with(&["--capability", "cve_mitigate"], "cve");
    let refused = String::from_utf8_lossy(&selected.stderr).to_string();
    assert_eq!(
        selected.status.code(),
        Some(2),
        "stdout = {}",
        String::from_utf8_lossy(&selected.stdout)
    );
    assert!(
        !refused.contains("unknown capability"),
        "a registered id must reach its builder, and this one was rejected at the \
         flag: {refused}"
    );
    assert!(
        refused.contains("cve_mitigate") && refused.contains("[github]"),
        "the refusal must name the capability and the table its deployment has \
         not described: {refused}"
    );

    let mistyped = scenario.run_raw_with(&["--capability", "cve_mitigat"], "cve");
    let unknown = String::from_utf8_lossy(&mistyped.stderr).to_string();
    assert_eq!(mistyped.status.code(), Some(2));
    assert!(
        unknown.contains("unknown capability"),
        "an id this build cannot run must be refused as one: {unknown}"
    );
    assert!(
        unknown.contains("cve_mitigate"),
        "the diagnostic lists every id this build can execute, and the new one \
         is among them: {unknown}"
    );
}

#[test]
fn a_run_over_a_trackerless_reference_is_not_a_failed_run() {
    let scenario = Scenario::new();

    let payload = scenario.run_json_with(&["--capability", "stub_mark"], "cve", 0);

    assert_eq!(
        payload["outcome"], "completed",
        "a reference that names no work item is a run fiddle can act on: {payload}"
    );
    let work_item = &payload["observations"]["work_item"];
    assert!(
        work_item["not_applicable"]["reason"].is_string(),
        "the work item must be reported as a question that does not apply: {payload}"
    );
    assert!(
        work_item.get("available").is_none() && work_item.get("unavailable").is_none(),
        "a question that does not apply has no answer and no failure: {payload}"
    );
    assert!(
        !payload.to_string().contains("stub:work/"),
        "no port may be asked to read a work id that does not exist: {payload}"
    );

    assert_eq!(
        scenario.read_change_marker("cve"),
        Some(scenario.expected_marker("cve")),
        "the marker is filed under the slug, because the empty value is not a name"
    );
}

#[test]
fn inspecting_a_trackerless_reference_derives_an_action_rather_than_a_block() {
    let scenario = Scenario::new();

    let payload = scenario.inspect_json("cve");

    assert_eq!(payload["invocation_ref"], "cve", "{payload}");
    assert!(
        payload["next_action"]["execute"].is_object(),
        "a trackerless reference with no change set recorded is work to do: {payload}"
    );
    assert!(
        payload["assessment"].get("blocked").is_none(),
        "nothing about this world is unobservable: {payload}"
    );
    assert!(
        !payload.to_string().contains("stub:work/"),
        "the work-item port names no source, because it was not consulted: {payload}"
    );
}

const SWEEP_REF: &str = "cve";

const SWEEP_REPO: &str = "acme/r";
const SWEEP_BASE: &str = "main";

const WIZ_ID: &str = "WIZ_CLIENT_ID";
const WIZ_SECRET: &str = "WIZ_CLIENT_SECRET";
const FORGE_TOKEN: &str = "FIDDLE_GITHUB_TOKEN";
const MODEL_KEY: &str = "LITELLM_API_KEY";

const SENTINEL_SECRET: &str = "fiddle-secret-3b8e51d0";

const SWEEP_IMAGE: &str = "ghcr.io/acme/icecube:latest";

const SCANNED_DIGEST: &str =
    "sha256:6f1b0d2c9a4e7385bd1c05fa9e37642c8b0d5713ae629f04c8d17b6a3e59042d";

const SCAN_OK: &str = "ok";

const SCAN_CLEAN: &str = "clean-image";

const SCAN_LIBRARY_ONLY: &str = "library-only";

const SCAN_ONLY_ADVISORY_HAS_NO_PUBLISHED_FIX: &str = "no-published-fix";

const SCAN_TWO_OS: &str = "two-os-advisories";

const RESCAN_CLEAN: &str = "library-clean";

const RESCAN_SECOND_LIBRARY_OPEN: &str = "second-library-still-open";

const PASSING_CHECK: &[&str] = &[];
const FAILING_CHECK: &[&str] = &["--exit", "1"];

const LIBRARY_CVE: &str = "CVE-2026-0001";
const OS_CVE: &str = "CVE-2026-0002";

const SECOND_OS_CVE: &str = "CVE-2026-0005";

const SCAN_MEDIUM_LIBRARY: &str = "medium-library-advisory";

const GRADES_INCLUDING_MEDIUM: &[&str] = &["CRITICAL", "HIGH", "MEDIUM"];

const SCAN_TWO_LIBRARIES: &str = "two-library-advisories";

const SECOND_LIBRARY_CVE: &str = "CVE-2026-0003";

const SECOND_MODULE: &str = "golang.org/x/net";
const SECOND_VULNERABLE_VERSION: &str = "v0.24.0";
const SECOND_FIXED_VERSION: &str = "v0.28.0";

const TWO_LIBRARIES: &str = "cve-two-libraries";

struct Sweep {
    scenario: Scenario,
    stub: PathBuf,
    remote: PathBuf,
    tree: PathBuf,
    gateway: StubGateway,
}

impl Sweep {
    fn scanning(fixture: &str, scan: &str, findings: usize, script: Vec<Reply>) -> Self {
        Sweep::scanning_rescanning(fixture, scan, RESCAN_CLEAN, findings, script)
    }

    fn scanning_grades(
        fixture: &str,
        scan: &str,
        rescan: &str,
        grades: &[&str],
        findings: usize,
        script: Vec<Reply>,
    ) -> Self {
        Sweep::deployment(
            fixture,
            scan,
            rescan,
            findings,
            script,
            PASSING_CHECK,
            Some(grades),
        )
    }

    fn scanning_rescanning(
        fixture: &str,
        scan: &str,
        rescan: &str,
        findings: usize,
        script: Vec<Reply>,
    ) -> Self {
        Sweep::world(fixture, scan, rescan, findings, script, PASSING_CHECK)
    }

    fn scanning_with_a_failing_check(
        fixture: &str,
        scan: &str,
        findings: usize,
        script: Vec<Reply>,
    ) -> Self {
        Sweep::world(fixture, scan, RESCAN_CLEAN, findings, script, FAILING_CHECK)
    }

    fn world(
        fixture: &str,
        scan: &str,
        rescan: &str,
        findings: usize,
        script: Vec<Reply>,
        check_args: &[&str],
    ) -> Self {
        Sweep::deployment(fixture, scan, rescan, findings, script, check_args, None)
    }

    #[allow(clippy::too_many_arguments)]
    fn deployment(
        fixture: &str,
        scan: &str,
        rescan: &str,
        findings: usize,
        script: Vec<Reply>,
        check_args: &[&str],
        grades: Option<&[&str]>,
    ) -> Self {
        let scenario = Scenario::new();

        let stub = scenario.dir().join("gh-stub");
        std::fs::create_dir_all(stub.join("script")).unwrap();
        std::fs::create_dir_all(stub.join("config")).unwrap();
        let remote = stub.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare", "-b", SWEEP_BASE, "."]);

        let sweep = Sweep {
            tree: seed_repository(scenario.dir(), &remote, fixture),
            scenario,
            stub,
            remote,
            gateway: StubGateway::serving(script),
        };
        let tables = sweep.tables(scan, rescan, findings, check_args, grades);
        sweep.scenario.append_config(&tables);
        sweep
    }

    fn tables(
        &self,
        scan: &str,
        rescan: &str,
        findings: usize,
        check_args: &[&str],
        grades: Option<&[&str]>,
    ) -> String {
        format!(
            "[github]\n\
             repo = \"{SWEEP_REPO}\"\n\
             base = \"{SWEEP_BASE}\"\n\
             token = {{ env = \"{FORGE_TOKEN}\" }}\n\
             cli = {{ program = {gh}, args = [\"--stub-dir\", {stub}] }}\n\
             git = \"git\"\n\
             config_dir = {config_dir}\n\
             timeout = \"120s\"\n\
             \n\
             [agent]\n\
             model = \"a-model\"\n\
             base_url = \"{base_url}\"\n\
             api_key = {{ env = \"{MODEL_KEY}\" }}\n\
             max_turns = 4\n\
             max_tokens = 512\n\
             max_changed_files = 4\n\
             deadline = \"300s\"\n\
             tool_timeout = \"300s\"\n\
             \n\
             [scanner]\n\
             cli = {{ program = {wiz}, args = [\"{scan}\"] }}\n\
             client_id = {{ env = \"{WIZ_ID}\" }}\n\
             client_secret = {{ env = \"{WIZ_SECRET}\" }}\n\
             timeout = \"300s\"\n\
             \n\
             [orchestration.cve]\n\
             image = \"{SWEEP_IMAGE}\"\n\
             {severities}\
             max_findings = {findings}\n\
             \n\
             [workspace]\n\
             root = {workspaces}\n\
             fixture = {tree}\n\
             command_timeout = \"300s\"\n\
             \n\
             [[workspace.checks]]\n\
             program = {check}\n\
             args = [{check_args}]\n\
             success = \"exit-zero\"\n\
             \n\
             [[workspace.checks]]\n\
             program = {wiz}\n\
             args = [\"{rescan}\"]\n\
             success = \"artefact-written\"\n",
            severities = grades
                .map(|grades| {
                    let named = grades
                        .iter()
                        .map(|grade| format!("{grade:?}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("severities = [{named}]\n")
                })
                .unwrap_or_default(),
            gh = toml_string(gh_stub_binary()),
            wiz = toml_string(wiz_stub_binary()),
            check = toml_string(check_stub_binary()),
            check_args = check_args
                .iter()
                .map(|argument| format!("{argument:?}"))
                .collect::<Vec<_>>()
                .join(", "),
            stub = toml_string(&self.stub),
            config_dir = toml_string(&self.stub.join("config")),
            base_url = self.gateway.base_url(),
            workspaces = toml_string(&self.workspace_root()),
            tree = toml_string(&self.tree),
        )
    }

    fn run(&self) -> Output {
        self.run_with(&[])
    }

    fn run_with(&self, extra: &[&str]) -> Output {
        self.run_selecting(&["--capability", "cve_mitigate"], extra)
    }

    fn run_unqualified(&self) -> Output {
        self.run_selecting(&[], &["--mode", "unattended"])
    }

    fn run_plain(&self) -> Output {
        self.command_selecting(&["--capability", "cve_mitigate"], &[])
            .output()
            .unwrap()
    }

    fn run_selecting(&self, selection: &[&str], extra: &[&str]) -> Output {
        self.command_selecting(selection, extra)
            .arg("--json")
            .output()
            .unwrap()
    }

    fn command_selecting(&self, selection: &[&str], extra: &[&str]) -> std::process::Command {
        let mut command = std::process::Command::new(support::fiddle_binary());
        for name in CREDENTIAL_VARS
            .iter()
            .chain([FORGE_TOKEN, MODEL_KEY, WIZ_ID, WIZ_SECRET].iter())
        {
            command.env_remove(name);
        }
        command
            .args(["run", SWEEP_REF])
            .args(selection)
            .args(["--config", self.scenario.config_path().to_str().unwrap()])
            .args(extra)
            .env(FORGE_TOKEN, "ghp_forge_token_for_the_sweep")
            .env(MODEL_KEY, "sk-model-key-for-the-sweep")
            .env(WIZ_ID, "wiz-client-id-for-the-sweep")
            .env(WIZ_SECRET, SENTINEL_SECRET);
        command
    }

    fn change_marker(&self) -> Option<String> {
        self.scenario.read_change_marker(SWEEP_REF)
    }

    fn payload(&self, run: &Output) -> serde_json::Value {
        serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout is not JSON ({e}): {}\nstderr: {}",
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr)
            )
        })
    }

    fn bundle(&self, run: &Output) -> serde_json::Value {
        self.scenario.read_bundle(&self.payload(run))
    }

    fn verdicts(&self) -> serde_json::Value {
        let path = self.scenario.report_dir().join("verdicts.json");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("no verdict report at {} ({e})", path.display()));
        serde_json::from_slice(&bytes).unwrap()
    }

    fn disposition(&self, run: &Output) -> serde_json::Value {
        let bundle = self.bundle(run);
        bundle
            .get("disposition")
            .cloned()
            .unwrap_or_else(|| panic!("this run published no disposition at all: {bundle}"))
    }

    fn evidence(&self, run: &Output) -> Vec<String> {
        let bundle = self.bundle(run);
        ["capability_executions", "progress"]
            .iter()
            .filter_map(|key| bundle[key].as_array().cloned())
            .flatten()
            .filter_map(|entry| entry["evidence"].as_array().cloned())
            .flatten()
            .filter_map(|reference| reference.as_str().map(str::to_string))
            .collect()
    }

    fn assert_every_receipt_is_logical(&self, run: &Output) {
        let root = self.scenario.dir().display().to_string();
        for reference in self.evidence(run) {
            let locator = reference
                .split_once(':')
                .map(|(_, locator)| locator)
                .unwrap_or(&reference);
            assert!(
                !locator.starts_with('/'),
                "a published receipt quotes a host absolute path, which names                  nothing on any other machine: {reference}"
            );
            assert!(
                !reference.contains(&root),
                "a published receipt quotes this deployment's own scratch                  directory ({root}): {reference}"
            );
        }
    }

    fn has_verdict(&self, cve: &str) -> bool {
        self.verdicts()
            .as_array()
            .expect("the verdict report is an array")
            .iter()
            .any(|verdict| verdict["cve"] == cve)
    }

    fn pull_requests(&self) -> Vec<serde_json::Value> {
        let out = std::process::Command::new(gh_stub_binary())
            .args(["--stub-dir", self.stub.to_str().unwrap()])
            .args([
                "api",
                "--method",
                "GET",
                &format!("/repos/{SWEEP_REPO}/pulls?state=open"),
            ])
            .output()
            .unwrap();
        body_of(&String::from_utf8_lossy(&out.stdout))
    }

    fn remote_branches(&self) -> Vec<String> {
        let refs = git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        );
        match refs.is_empty() {
            true => Vec::new(),
            false => refs.lines().map(str::to_string).collect(),
        }
    }

    fn requested_paths(&self) -> Vec<String> {
        walkdir_files(self.stub.join("requests"))
            .iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .filter_map(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .filter_map(|request| {
                request["argv"].as_array()?.iter().find_map(|arg| {
                    let arg = arg.as_str()?;
                    arg.starts_with('/').then(|| arg.to_string())
                })
            })
            .collect()
    }

    fn workspace_root(&self) -> PathBuf {
        self.scenario.dir().join("workspaces")
    }

    fn files_holding(&self, needle: &str) -> Vec<String> {
        self.scenario
            .project_tree()
            .into_iter()
            .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(needle))
            .map(|(path, _)| path)
            .collect()
    }
}

fn is_fixture_recording(path: &str) -> bool {
    path.starts_with("gh-stub/requests/")
        || path == "reports/scan/child.json"
        || path == "reports/rescan/child.json"
}

fn seed_repository(root: &Path, remote: &Path, name: &str) -> PathBuf {
    let tree = root.join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    for (path, bytes) in tracked_files(&fixture(name)) {
        let destination = tree.join(&path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(destination, bytes).unwrap();
    }
    git(&tree, &["init", "-q", "-b", SWEEP_BASE, "."]);
    git(&tree, &["add", "-A"]);
    git(
        &tree,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "the fixture under remediation",
        ],
    );
    git(
        &tree,
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    git(&tree, &["push", "-q", "origin", SWEEP_BASE]);
    tree
}

fn a_script_no_attempt_consumes() -> Vec<Reply> {
    vec![accepted(completion(
        serde_json::json!({
            "role": "assistant",
            "content": serde_json::json!({
                "changed_files": [],
                "summary": "the requirement already resolves to the fixed release",
                "claimed_complete": true,
                "findings": [{
                    "cve": LIBRARY_CVE,
                    "attempted": true,
                    "note": "the requirement already resolves to the fixed release",
                }],
            }).to_string(),
        }),
        "stop",
    ))]
}

fn a_repair_moving_the_requirement() -> Vec<Reply> {
    an_attempt(
        &[
            ("go.mod", vulnerable_manifest()),
            ("go.sum", vulnerable_sums()),
        ],
        &[LIBRARY_CVE],
        &[],
    )
}

fn an_attempt_finding_the_tree_already_at_the_fix(shown: &[&str]) -> Vec<Reply> {
    vec![accepted(completion(
        serde_json::json!({
            "role": "assistant",
            "content": serde_json::json!({
                "changed_files": [],
                "summary": "the requirements already resolve to the fixed releases",
                "claimed_complete": true,
                "findings": shown.iter().map(|cve| serde_json::json!({
                    "cve": cve,
                    "attempted": true,
                    "note": "the requirement already resolves to the fixed release",
                })).collect::<Vec<_>>(),
            }).to_string(),
        }),
        "stop",
    ))]
}

fn two_nights(first: Vec<Reply>, second: Vec<Reply>) -> Vec<Reply> {
    first.into_iter().chain(second).collect()
}

fn an_attempt_declining(shown: &[&str]) -> Vec<Reply> {
    an_attempt(&[], &[], shown)
}

fn vulnerable_manifest() -> String {
    read_fixture_file(VULNERABLE, "go.mod").replace(VULNERABLE_VERSION, FIXED_VERSION)
}

fn vulnerable_sums() -> String {
    read_fixture_file(VULNERABLE, "go.sum").replace(VULNERABLE_VERSION, FIXED_VERSION)
}

fn an_attempt(edits: &[(&str, String)], fixed: &[&str], declined: &[&str]) -> Vec<Reply> {
    let mut script: Vec<Reply> = edits
        .iter()
        .map(|(path, contents)| {
            accepted(calls(
                "write_file",
                serde_json::json!({ "path": path, "contents": contents }),
            ))
        })
        .collect();

    let dispositions: Vec<serde_json::Value> = fixed
        .iter()
        .map(|cve| {
            serde_json::json!({
                "cve": cve,
                "attempted": true,
                "note": "moved the requirement to the release that carries the fix",
            })
        })
        .chain(declined.iter().map(|cve| {
            serde_json::json!({
                "cve": cve,
                "attempted": false,
                "note": "no fix I can apply to this project without reading a registry",
            })
        }))
        .collect();

    script.push(accepted(reports(serde_json::json!({
        "changed_files": edits.iter().map(|(path, _)| *path).collect::<Vec<_>>(),
        "summary": "the requirements this project pins were moved to the releases that carry the fixes",
        "claimed_complete": declined.is_empty(),
        "findings": dispositions,
    }))));
    script
}

fn two_libraries_manifest() -> String {
    read_fixture_file(TWO_LIBRARIES, "go.mod")
        .replace(VULNERABLE_VERSION, FIXED_VERSION)
        .replace(SECOND_VULNERABLE_VERSION, SECOND_FIXED_VERSION)
}

fn two_libraries_sums() -> String {
    read_fixture_file(TWO_LIBRARIES, "go.sum")
        .replace(VULNERABLE_VERSION, FIXED_VERSION)
        .replace(SECOND_VULNERABLE_VERSION, SECOND_FIXED_VERSION)
}

fn a_repair_moving_only_the_first_of_two_requirements() -> Vec<(&'static str, String)> {
    vec![
        (
            "go.mod",
            read_fixture_file(TWO_LIBRARIES, "go.mod").replace(VULNERABLE_VERSION, FIXED_VERSION),
        ),
        (
            "go.sum",
            read_fixture_file(TWO_LIBRARIES, "go.sum").replace(VULNERABLE_VERSION, FIXED_VERSION),
        ),
    ]
}

fn the_one_new_branch(sweep: &Sweep) -> String {
    let branches = sweep.remote_branches();
    let new: Vec<&String> = branches.iter().filter(|it| *it != SWEEP_BASE).collect();
    assert!(
        branches.contains(&SWEEP_BASE.to_string()),
        "the base branch must survive the run: {branches:?}"
    );
    assert_eq!(
        new.len(),
        1,
        "a sweep opens exactly one branch for its shared pull request: {branches:?}"
    );
    assert!(
        new[0].starts_with("security/cve-remediation-"),
        "the branch is named for what it carries and the day it was opened: {branches:?}"
    );
    new[0].clone()
}

fn pushed_file(sweep: &Sweep, branch: &str, path: &str) -> String {
    git_says(&sweep.remote, &["show", &format!("{branch}:{path}")])
}

fn pushed_commits(sweep: &Sweep, branch: &str) -> Vec<(String, Vec<String>)> {
    let revisions = git_says(
        &sweep.remote,
        &["rev-list", "--reverse", &format!("{SWEEP_BASE}..{branch}")],
    );
    revisions
        .lines()
        .map(|sha| {
            let body = git_says(&sweep.remote, &["log", "-1", "--format=%B", sha]);
            let changed = git_says(
                &sweep.remote,
                &["diff-tree", "--no-commit-id", "--name-only", "-r", sha],
            );
            let changed = match changed.is_empty() {
                true => Vec::new(),
                false => changed.lines().map(str::to_string).collect(),
            };
            (body, changed)
        })
        .collect()
}

#[test]
fn a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    let pulls = sweep.pull_requests();
    assert_eq!(pulls.len(), 1, "exactly one pull request: {pulls:?}");
    let pull = &pulls[0];
    assert_eq!(pull["base"]["ref"], SWEEP_BASE, "{pull}");
    assert!(
        pull["labels"]
            .as_array()
            .is_some_and(|labels| labels.iter().any(|it| it["name"] == "security/cve")),
        "the label is how the next run finds this object again: {pull}"
    );

    let branch = the_one_new_branch(&sweep);
    assert_eq!(pull["head"]["ref"], branch, "{pull}");

    let landed = pushed_file(&sweep, &branch, "go.mod");
    assert!(
        landed.contains(&format!("{MODULE} {FIXED_VERSION}")),
        "the branch must carry the requirement at the fixed release: {landed}"
    );
    assert!(
        !landed.contains(VULNERABLE_VERSION),
        "and must not still carry the vulnerable one: {landed}"
    );

    let bundle = sweep.bundle(&run);
    assert_eq!(
        bundle["observations"]["tree"],
        serde_json::json!({
            "base_revision": git_says(&sweep.remote, &["rev-parse", SWEEP_BASE]),
            "pr_head": serde_json::Value::Null,
            "attempt_tree": "base_revision",
            "scanned_image_digest": SCANNED_DIGEST,
        }),
        "{bundle}"
    );
    let published = bundle["observations"]["tree"]["scanned_image_digest"]
        .as_str()
        .unwrap_or_default();
    assert_ne!(
        published, SWEEP_IMAGE,
        "the bundle must name what the scan resolved, not the tag it was \
         handed: {bundle}"
    );
    assert!(
        published.starts_with("sha256:"),
        "and it must be a digest rather than any other spelling of the image: \
         {bundle}"
    );

    assert_eq!(bundle["progress"][0]["stage"], "mitigate", "{bundle}");

    let reached = sweep.disposition(&run);
    assert_eq!(reached["reason"], "pull_request", "{reached}");
    assert_eq!(reached["branch"], branch, "{reached}");
    assert_eq!(reached["pull_request"], pulls[0]["number"], "{reached}");
    assert_eq!(
        reached["attempts"],
        serde_json::json!([{
            "cves": [LIBRARY_CVE],
            "status": "clean",
            "claimed_complete": true,
            "forbidden": [],
        }]),
        "{reached}"
    );
    sweep.assert_every_receipt_is_logical(&run);

    let verdicts = sweep.verdicts();
    assert_eq!(
        verdicts.as_array().map(Vec::len),
        Some(0),
        "the advisory this run took was fixed, and the one it did not take was \
         never judged: {verdicts}"
    );
    assert_eq!(
        reached["deferred"],
        serde_json::json!([{ "cve": OS_CVE, "bound": 1 }]),
        "and the advisory over the bound is on the record with the number that \
         put it there, rather than silently missing: {reached}"
    );
}

#[test]
fn the_documented_invocation_with_no_capability_flag_reaches_the_sweep() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());

    let run = sweep.run_unqualified();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    let executed = &payload["capability_executions"][0]["capability_id"];
    assert_ne!(
        executed, "stub_mark",
        "`fiddle run cve --mode unattended` ran M0's deterministic stub, which is \
         the defect: the documented invocation reports `completed` having scanned \
         nothing, and files the marker that accounts the sweep as done: {payload}"
    );
    assert_eq!(
        executed, "cve_mitigate",
        "an absent `--capability` over a `cve` reference must select the sweep: \
         {payload}"
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    let pulls = sweep.pull_requests();
    assert_eq!(
        pulls.len(),
        1,
        "the documented invocation must reach the same publication the flagged \
         one does: {pulls:?}"
    );
    let branch = the_one_new_branch(&sweep);
    assert_eq!(pulls[0]["head"]["ref"], branch, "{}", pulls[0]);
    let landed = pushed_file(&sweep, &branch, "go.mod");
    assert!(
        landed.contains(&format!("{MODULE} {FIXED_VERSION}")),
        "the branch must carry the requirement at the fixed release: {landed}"
    );

    let bundle = sweep.bundle(&run);
    assert_eq!(
        bundle["progress"][0]["stage"], "mitigate",
        "the record must be written in the vocabulary of what ran: {bundle}"
    );
}

#[test]
fn a_marker_against_a_trackerless_reference_does_not_account_the_sweep_as_done() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());

    let foreign_marker = sweep.scenario.expected_marker(SWEEP_REF);
    sweep
        .scenario
        .write_change_marker(SWEEP_REF, &foreign_marker);
    assert_eq!(
        sweep.scenario.read_change_marker(SWEEP_REF),
        Some(foreign_marker),
        "the reference must really be marked, and readable the way the change \
         port reads it, or this lane asserts nothing"
    );

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "cve_mitigate",
        "a marker some other capability wrote must not account the sweep as \
         done: the assessment read `satisfied` from it, `derive_next` returned \
         `complete` before the capability was consulted, and the run reported \
         success having executed nothing: {payload}"
    );
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    let pulls = sweep.pull_requests();
    assert_eq!(
        pulls.len(),
        1,
        "the sweep must reach the publication an unmarked reference reaches: \
         {pulls:?}"
    );
    let branch = the_one_new_branch(&sweep);
    assert_eq!(pulls[0]["head"]["ref"], branch, "{}", pulls[0]);
    let landed = pushed_file(&sweep, &branch, "go.mod");
    assert!(
        landed.contains(&format!("{MODULE} {FIXED_VERSION}")),
        "the branch must carry the requirement at the fixed release: {landed}"
    );
}

#[test]
fn a_reference_that_is_not_cve_still_selects_the_deterministic_capability() {
    let scenario = Scenario::new();
    scenario.write_work_item("fiddle-m0-demo", "open");

    let payload = scenario.run_json("beans:fiddle-m0-demo", 0);

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "stub_mark",
        "a scheme-derived default must move the default for `cve` alone: \
         {payload}"
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");
    assert_eq!(
        scenario.read_change_marker("fiddle-m0-demo").as_deref(),
        Some(scenario.expected_marker("beans:fiddle-m0-demo").as_str()),
        "and the deterministic capability writes the marker it always wrote: \
         {payload}"
    );
}

#[test]
fn an_advisory_nothing_can_move_leaves_the_whole_attempt_needing_direction() {
    let sweep = Sweep::scanning(
        FIXED,
        SCAN_OK,
        2,
        an_attempt(&[], &[LIBRARY_CVE], &[OS_CVE]),
    );

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "an attempt that could not move one of its advisories is not a failed \
         run - stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    let scanned = std::fs::read_to_string(sweep.scenario.report_dir().join("scan/scan.json"))
        .expect("the scanner left no artefact, so this outcome is not evidence about an image");
    for cve in [LIBRARY_CVE, OS_CVE] {
        assert!(
            scanned.contains(cve),
            "the document this run was answering does not name {cve}, so nothing \
             below is evidence about the tree: {scanned}"
        );
    }

    let verdicts = sweep.verdicts();
    for cve in [LIBRARY_CVE, OS_CVE] {
        assert!(
            sweep.has_verdict(cve),
            "one advisory did not clear, so the whole attempt needs work and \
             every advisory in it is reported - {cve} is not: {verdicts}"
        );
    }

    assert!(
        sweep.pull_requests().is_empty(),
        "a run whose attempt needs work opens nothing: {:?}",
        sweep.pull_requests()
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and leaves the remote exactly as it found it"
    );
    let evidence = payload["capability_executions"][0]["evidence"].to_string();
    assert!(
        !evidence.contains("/tree/") && !evidence.contains("/pull/"),
        "a run that published nothing must quote no branch and no pull request: \
         {evidence}"
    );
    sweep.assert_every_receipt_is_logical(&run);

    let disposition = sweep.disposition(&run);
    assert_eq!(
        disposition,
        serde_json::json!({
            "reason": "unsafe_without_direction",
            "verdicts": 2,
            "already_fixed": [],
            "deferred": [],
            "attempts": [{
                "cves": [LIBRARY_CVE, OS_CVE],
                "status": "needs_work",
                "claimed_complete": false,
                "forbidden": [],
            }],
            "branch": serde_json::Value::Null,
            "pull_request": serde_json::Value::Null,
        }),
        "the row, and the evidence for it, whole"
    );
}

fn nothing_to_do_publishes() -> serde_json::Value {
    serde_json::json!({
        "reason": "nothing_to_do",
        "verdicts": 0,
        "already_fixed": [],
        "deferred": [],
        "attempts": [],
        "branch": serde_json::Value::Null,
        "pull_request": serde_json::Value::Null,
    })
}

fn already_fixed_publishes() -> serde_json::Value {
    serde_json::json!({
        "reason": "already_fixed",
        "verdicts": 0,
        "already_fixed": [LIBRARY_CVE],
        "deferred": [],
        "attempts": [],
        "branch": serde_json::Value::Null,
        "pull_request": serde_json::Value::Null,
    })
}

fn verdicts_only_publishes() -> serde_json::Value {
    serde_json::json!({
        "reason": "verdicts_only",
        "verdicts": 1,
        "already_fixed": [],
        "deferred": [],
        "attempts": [],
        "branch": serde_json::Value::Null,
        "pull_request": serde_json::Value::Null,
    })
}

fn already_in_progress_publishes() -> serde_json::Value {
    serde_json::json!({
        "reason": "already_in_progress",
        "verdicts": 0,
        "already_fixed": [LIBRARY_CVE],
        "deferred": [],
        "attempts": [],
        "branch": serde_json::Value::Null,
        "pull_request": SHARED_PR,
    })
}

fn the_old_surface_says_nothing(sweep: &Sweep, run: &Output, world: &str) {
    assert_eq!(
        run.status.code(),
        Some(0),
        "{world} is not a failed run — stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(sweep.payload(run)["outcome"], "completed", "{world}");
    assert_eq!(
        sweep.verdicts(),
        serde_json::json!([]),
        "{world} must leave the same empty verdict report as its neighbours, or \
         the three are being told apart by something other than the row"
    );
    sweep.assert_every_receipt_is_logical(run);
}

fn the_row_both_surfaces_agree_on(sweep: &Sweep, run: &Output, world: &str) -> serde_json::Value {
    let document = sweep.disposition(run);
    assert_eq!(
        sweep.payload(run)["disposition"],
        document,
        "{world}: stdout and the bundle disagree about the row"
    );
    document
}

#[test]
fn a_scan_of_an_empty_image_reaches_nothing_to_do() {
    let sweep = Sweep::scanning(FIXED, SCAN_CLEAN, 2, a_script_no_attempt_consumes());

    let run = sweep.run();
    the_old_surface_says_nothing(&sweep, &run, "nothing to do");

    assert_eq!(
        the_row_both_surfaces_agree_on(&sweep, &run, "nothing to do"),
        nothing_to_do_publishes(),
        "the row that is *nothing*: every list empty, and the reason is the \
         whole of it"
    );
}

#[test]
fn a_tree_that_settles_every_finding_reaches_already_fixed() {
    let sweep = Sweep::scanning_rescanning(
        FIXED,
        SCAN_LIBRARY_ONLY,
        SCAN_CLEAN,
        2,
        an_attempt_finding_the_tree_already_at_the_fix(&[LIBRARY_CVE]),
    );

    let run = sweep.run();
    the_old_surface_says_nothing(&sweep, &run, "already fixed in the tree");

    assert_eq!(
        the_row_both_surfaces_agree_on(&sweep, &run, "already fixed in the tree"),
        already_fixed_publishes(),
        "row 3 names the advisory somebody else already dealt with, which is \
         exactly what row 1 cannot say - and nothing pre-filtered it: the \
         attempt was made, it changed nothing, and the rescan is what cleared it"
    );
    assert!(
        sweep.pull_requests().is_empty(),
        "an attempt that committed nothing publishes nothing, so no pull \
         request stands for a branch with no commit on it: {:?}",
        sweep.pull_requests()
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and it cut no branch either"
    );
}

#[test]
fn an_open_pull_request_covering_the_rest_reaches_already_in_progress() {
    let sweep = Sweep::scanning_rescanning(
        FIXED,
        SCAN_LIBRARY_ONLY,
        SCAN_CLEAN,
        2,
        an_attempt_finding_the_tree_already_at_the_fix(&[LIBRARY_CVE]),
    );
    sweep.seed_shared_pull_request_saying(
        STALE_BODY,
        &format!("move the requirement, fixes {LIBRARY_CVE}"),
    );

    let run = sweep.run();
    the_old_surface_says_nothing(&sweep, &run, "an open pull request covers it");

    assert_eq!(
        the_row_both_surfaces_agree_on(&sweep, &run, "an open pull request covers it"),
        already_in_progress_publishes(),
        "row 7's remedy is to go and merge a numbered pull request, so the \
         number is the evidence — and nothing landed on a branch this run made, \
         so it names none"
    );
}

#[test]
fn the_plain_rendering_names_the_row_a_run_reached_and_its_pull_request() {
    let sweep = Sweep::scanning_rescanning(
        FIXED,
        SCAN_LIBRARY_ONLY,
        SCAN_CLEAN,
        2,
        an_attempt_finding_the_tree_already_at_the_fix(&[LIBRARY_CVE]),
    );
    sweep.seed_shared_pull_request_saying(
        STALE_BODY,
        &format!("move the requirement, fixes {LIBRARY_CVE}"),
    );

    let run = sweep.run_plain();
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    assert_eq!(run.status.code(), Some(0), "stderr: {stderr}");
    assert!(
        stdout.starts_with("run "),
        "this must be the plain rendering, not a payload: {stdout}"
    );

    assert!(
        stdout.contains(
            "disposition = already_in_progress \
             (0 unfixed, 1 already fixed, 0 deferred, 0 attempted), \
             pull request #41"
        ),
        "an operator at a terminal must be told which of the seven rows this run \
         reached and that the remedy is to go and merge #{SHARED_PR}: {stdout}"
    );

    let relative = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("report      = "))
        .unwrap_or_else(|| panic!("the plain rendering must name its bundle: {stdout}"));
    let bytes = std::fs::read(sweep.scenario.report_dir().join(relative)).unwrap();
    let bundle: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        bundle["disposition"],
        already_in_progress_publishes(),
        "stdout and the bundle must not be two documents"
    );

    assert!(
        !stdout.contains(SENTINEL_SECRET),
        "a credential reached stdout: {stdout}"
    );
    assert!(
        !stderr.contains(SENTINEL_SECRET),
        "a credential reached a diagnostic: {stderr}"
    );
}

#[test]
fn the_three_rows_that_used_to_publish_one_document_publish_three() {
    let rows = [
        nothing_to_do_publishes(),
        already_fixed_publishes(),
        already_in_progress_publishes(),
    ];

    let named: std::collections::HashSet<String> =
        rows.iter().map(serde_json::Value::to_string).collect();
    assert_eq!(
        named.len(),
        3,
        "two of the three silent rows publish one document: {rows:?}"
    );

    let anonymous: std::collections::HashSet<String> = rows
        .iter()
        .map(|row| {
            let mut row = row.clone();
            row.as_object_mut()
                .expect("a disposition is an object")
                .remove("reason")
                .expect("every row publishes its reason");
            row.to_string()
        })
        .collect();
    assert_eq!(
        anonymous.len(),
        3,
        "a row must be checkable from its evidence and not from its own name \
         alone, and two of these carry the same evidence: {rows:?}"
    );
}

#[test]
fn an_advisory_with_no_published_fix_reaches_verdicts_only_with_nothing_attempted() {
    let sweep = Sweep::scanning(
        VULNERABLE,
        SCAN_ONLY_ADVISORY_HAS_NO_PUBLISHED_FIX,
        2,
        a_script_no_attempt_consumes(),
    );

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "an advisory with no published fix is not a failed run - stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let scan_artefact = std::fs::read_to_string(sweep.scenario.report_dir().join("scan/scan.json"))
        .expect("the scanner left no artefact, so nothing below is about a document");
    assert!(
        scan_artefact.contains(LIBRARY_CVE),
        "the scan does not name {LIBRARY_CVE}, so a verdict for it would be a \
         row about a finding nobody reported: {scan_artefact}"
    );
    assert!(
        !scan_artefact.contains("fixedVersion"),
        "no advisory in this document may name a fix: one that did would be \
         fixable, would be the attempt's, and the run would reach row 5 \
         instead: {scan_artefact}"
    );

    assert_eq!(
        the_row_both_surfaces_agree_on(&sweep, &run, "an advisory with no published fix"),
        verdicts_only_publishes(),
        "row 2 is a verdict count that is not zero beside an attempt list that \
         is empty, and either half alone is a row this run is not on"
    );

    let verdicts = sweep.verdicts();
    assert!(
        sweep.has_verdict(LIBRARY_CVE),
        "the advisory the scanner reported has no row: {verdicts}"
    );
    assert_eq!(
        verdicts[0]["verdict"],
        serde_json::json!("upstream_blocked"),
        "the verdict has to come from the projection rather than from an \
         attempt, or the empty attempt list above is hiding one: {verdicts}"
    );

    assert!(
        sweep.pull_requests().is_empty(),
        "a run that attempted nothing opens nothing: {:?}",
        sweep.pull_requests()
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and it pushes nothing, so the remote still holds only its base branch"
    );
    sweep.assert_every_receipt_is_logical(&run);
}

#[test]
fn an_unusable_scanner_exits_eleven_and_reaches_no_forge() {
    let sweep = Sweep::scanning(
        VULNERABLE,
        "exit-nonzero-no-file",
        2,
        a_script_no_attempt_consumes(),
    );

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(11),
        "a scan that produced no document is retryable, never a successful \
         no-change — stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let payload = sweep.payload(&run);
    assert_ne!(
        payload["outcome"], "completed",
        "a watcher reading the payload must reach the same conclusion as one \
         reading the status line: {payload}"
    );

    assert_eq!(
        sweep.requested_paths(),
        Vec::<String>::new(),
        "the scan is asked before the forge is, so a scan with no document \
         reaches no forge at all"
    );
    assert!(
        sweep.pull_requests().is_empty(),
        "and opens nothing: {:?}",
        sweep.pull_requests()
    );
    assert!(
        !sweep.workspace_root().exists(),
        "a scan with no document creates no worktree: {:?}",
        walkdir_files(sweep.workspace_root())
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and leaves the remote exactly as it found it"
    );

    let reached = sweep.disposition(&run);
    assert_eq!(
        reached,
        serde_json::json!({
            "reason": "scan_unusable",
            "verdicts": 0,
            "already_fixed": [],
            "deferred": [],
            "attempts": [],
            "branch": serde_json::Value::Null,
            "pull_request": serde_json::Value::Null,
        }),
        "every list is empty because nothing was observed, which is what makes \
         the reason the whole of the claim"
    );
    let bundle = sweep.bundle(&run);
    assert!(
        bundle["outcome"]["retryable"]["reason"]
            .as_str()
            .is_some_and(|why| why.contains("wizcli")),
        "the row's diagnostic lives on the outcome, and has to actually be \
         there: {bundle}"
    );
    assert!(
        bundle["observations"].get("tree").is_none(),
        "a run with no scan document chose no revision and measured nothing, so \
         it must publish neither half of the pair: {bundle}"
    );
}

#[test]
fn an_unprovable_repair_is_reverted_and_filed_as_needing_direction() {
    let sweep = Sweep::scanning_rescanning(
        VULNERABLE,
        SCAN_OK,
        SCAN_OK,
        2,
        an_attempt(
            &[
                ("go.mod", vulnerable_manifest()),
                ("go.sum", vulnerable_sums()),
            ],
            &[LIBRARY_CVE],
            &[OS_CVE],
        ),
    );

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "a group left for a person is not a failed run — stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert!(
        sweep.pull_requests().is_empty(),
        "an unproved repair opens nothing: {:?}",
        sweep.pull_requests()
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and leaves the remote exactly as it found it"
    );

    let reached = sweep.disposition(&run);
    assert_eq!(reached["reason"], "unsafe_without_direction", "{reached}");
    assert_eq!(
        reached["attempts"],
        serde_json::json!([{
            "cves": [LIBRARY_CVE, OS_CVE],
            "status": "needs_work",
            "claimed_complete": false,
            "forbidden": [],
        }]),
        "the row's evidence is that something was attempted, and the model's own \
         claim beside the judgement that overruled it — one row for the one \
         attempt, naming every advisory it was shown: {reached}"
    );
    assert!(
        reached["branch"].is_null() && reached["pull_request"].is_null(),
        "nothing landed, so there is nothing to point at: {reached}"
    );
    sweep.assert_every_receipt_is_logical(&run);

    let verdicts = sweep.verdicts();
    assert!(
        sweep.has_verdict(LIBRARY_CVE) && sweep.has_verdict(OS_CVE),
        "both advisories are still unfixed and both get a row: {verdicts}"
    );
}

#[test]
fn a_check_that_says_no_reverts_the_attempt_and_publishes_nothing() {
    let sweep = Sweep::scanning_with_a_failing_check(
        VULNERABLE,
        SCAN_OK,
        1,
        a_repair_moving_the_requirement(),
    );

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "an attempt left for a person is not a failed run — stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert!(
        sweep.pull_requests().is_empty(),
        "a repair a check refused opens nothing: {:?}",
        sweep.pull_requests()
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and leaves the remote exactly as it found it"
    );

    let reached = sweep.disposition(&run);
    assert_eq!(reached["reason"], "unsafe_without_direction", "{reached}");
    assert_eq!(
        reached["attempts"],
        serde_json::json!([{
            "cves": [LIBRARY_CVE],
            "status": "needs_work",
            "claimed_complete": true,
            "forbidden": [],
        }]),
        "one attempt, refused — and `claimed_complete` is the model's own claim \
         beside the check that overruled it: {reached}"
    );
    assert!(
        reached["branch"].is_null() && reached["pull_request"].is_null(),
        "nothing landed, so there is nothing to point at: {reached}"
    );

    let verdicts = sweep.verdicts();
    assert!(
        sweep.has_verdict(LIBRARY_CVE),
        "the advisory the attempt was shown is still unfixed and gets a row: \
         {verdicts}"
    );
    sweep.assert_every_receipt_is_logical(&run);
}

#[test]
fn two_findings_in_different_files_are_one_attempt_and_one_commit() {
    let sweep = Sweep::scanning(
        TWO_LIBRARIES,
        SCAN_TWO_LIBRARIES,
        2,
        [
            an_attempt(
                &[
                    ("go.mod", two_libraries_manifest()),
                    ("go.sum", two_libraries_sums()),
                ],
                &[LIBRARY_CVE, SECOND_LIBRARY_CVE],
                &[],
            ),
            an_attempt(&[], &[], &[LIBRARY_CVE, SECOND_LIBRARY_CVE]),
        ]
        .into_iter()
        .flatten()
        .collect(),
    );

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    let branch = the_one_new_branch(&sweep);
    let commits = pushed_commits(&sweep, &branch);
    assert_eq!(
        commits.len(),
        1,
        "every selected finding is one attempt, so the branch carries one \
         commit: {commits:?}"
    );
    let (body, paths) = &commits[0];

    assert_eq!(
        paths,
        &["go.mod".to_string(), "go.sum".to_string()],
        "one attempt lands its whole diff, however many files it spans: {body}"
    );

    for cve in [LIBRARY_CVE, SECOND_LIBRARY_CVE] {
        assert!(
            body.contains(cve),
            "a commit body naming one of two advisories leaves the other for the \
             next run to re-propose: {body}"
        );
    }

    let landed = pushed_file(&sweep, &branch, "go.mod");
    for (module, fixed) in [
        (MODULE, FIXED_VERSION),
        (SECOND_MODULE, SECOND_FIXED_VERSION),
    ] {
        assert!(
            landed.contains(&format!("{module} {fixed}")),
            "the branch must carry {module} at {fixed}: {landed}"
        );
    }

    let pulls = sweep.pull_requests();
    assert_eq!(pulls.len(), 1, "exactly one pull request: {pulls:?}");
    let reached = sweep.disposition(&run);
    assert_eq!(
        reached["attempts"],
        serde_json::json!([{
            "cves": [LIBRARY_CVE, SECOND_LIBRARY_CVE],
            "status": "clean",
            "claimed_complete": true,
            "forbidden": [],
        }]),
        "one row, naming every finding it covered: {reached}"
    );
    assert_eq!(
        sweep.gateway.served(),
        3,
        "two edits and a report is one attempt's whole turn, and a second \
         answer was waiting for a build that made a second attempt"
    );
}

#[test]
fn a_finding_that_does_not_clear_reverts_the_whole_commit() {
    let sweep = Sweep::scanning_rescanning(
        TWO_LIBRARIES,
        SCAN_TWO_LIBRARIES,
        RESCAN_SECOND_LIBRARY_OPEN,
        2,
        an_attempt(
            &a_repair_moving_only_the_first_of_two_requirements(),
            &[LIBRARY_CVE, SECOND_LIBRARY_CVE],
            &[],
        ),
    );

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "an attempt left for a person is not a failed run — stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "one finding the rescan still reports takes the whole commit back, so \
         there is no branch to publish"
    );
    assert!(
        sweep.pull_requests().is_empty(),
        "and nothing to open: {:?}",
        sweep.pull_requests()
    );

    let reached = sweep.disposition(&run);
    assert_eq!(reached["reason"], "unsafe_without_direction", "{reached}");
    assert_eq!(
        reached["attempts"],
        serde_json::json!([{
            "cves": [LIBRARY_CVE, SECOND_LIBRARY_CVE],
            "status": "needs_work",
            "claimed_complete": true,
            "forbidden": [],
        }]),
        "one row for the one attempt, needing work although the rescan cleared \
         one of the two findings it was shown: {reached}"
    );
    assert!(
        reached["branch"].is_null() && reached["pull_request"].is_null(),
        "nothing landed, so there is nothing to point at: {reached}"
    );

    let verdicts = sweep.verdicts();
    assert!(
        sweep.has_verdict(LIBRARY_CVE),
        "the finding the rescan no longer reports is still unfixed, because the \
         edit that cleared it went back with everything else: {verdicts}"
    );
    assert!(
        sweep.has_verdict(SECOND_LIBRARY_CVE),
        "and so is the one that did not clear: {verdicts}"
    );

    assert_eq!(
        walkdir_files(sweep.workspace_root()),
        Vec::<PathBuf>::new(),
        "the reverted attempt leaves no worktree behind"
    );
    assert_eq!(
        git_says(&sweep.tree, &["status", "--porcelain"]),
        "",
        "and leaves the checkout it was pointed at exactly as it found it"
    );
    sweep.assert_every_receipt_is_logical(&run);
}

#[test]
fn a_declined_finding_reads_differently_from_one_that_was_attempted_and_failed() {
    let sweep = Sweep::scanning_rescanning(
        TWO_LIBRARIES,
        SCAN_TWO_LIBRARIES,
        SCAN_TWO_LIBRARIES,
        2,
        an_attempt(
            &a_repair_moving_only_the_first_of_two_requirements(),
            &[LIBRARY_CVE],
            &[SECOND_LIBRARY_CVE],
        ),
    );

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "declining is an honest answer, not the model breaking its contract — \
         stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        payload["outcome"], "completed",
        "a declined finding is a verdict, so the run completes rather than \
         reporting a protocol failure: {payload}"
    );

    let reached = sweep.disposition(&run);
    assert_eq!(
        reached["reason"], "unsafe_without_direction",
        "and it still leaves the finding unfixed, so the commit goes back: \
         {reached}"
    );
    assert!(
        sweep.pull_requests().is_empty(),
        "opening nothing: {:?}",
        sweep.pull_requests()
    );

    let verdicts = sweep.verdicts();
    let row = |cve: &str| {
        verdicts
            .as_array()
            .expect("the verdict report is an array")
            .iter()
            .find(|row| row["cve"] == cve)
            .cloned()
            .unwrap_or_else(|| panic!("{cve} has no verdict row: {verdicts}"))
    };

    let failed = row(LIBRARY_CVE);
    let declined = row(SECOND_LIBRARY_CVE);
    assert_eq!(
        failed["attempted"], true,
        "the rescan still reports this one too, but the attempt did work on it \
         and the row says so: {failed}"
    );
    assert_eq!(
        declined["attempted"], false,
        "the report must say the declined finding was never attempted: \
         {declined}"
    );
    assert!(
        declined["note"]
            .as_str()
            .is_some_and(|it| !it.trim().is_empty()),
        "a declined finding carries the attempt's reason: {declined}"
    );
    assert_ne!(
        failed["note"], declined["note"],
        "a reader of one report tells the two apart by what each row says: \
         {verdicts}"
    );
}

#[test]
fn no_credential_reaches_stdout_a_diagnostic_or_a_published_bundle() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let planted = sweep.scenario.report_dir().join("scan").join("child.json");
    assert!(
        std::fs::read_to_string(&planted)
            .unwrap_or_default()
            .contains(SENTINEL_SECRET),
        "the scanner's record does not hold the credential, so its absence \
         elsewhere is not evidence: {}",
        planted.display()
    );

    assert!(
        !String::from_utf8_lossy(&run.stdout).contains(SENTINEL_SECRET),
        "a credential reached stdout: {}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert!(
        !String::from_utf8_lossy(&run.stderr).contains(SENTINEL_SECRET),
        "a credential reached a diagnostic: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        !sweep.bundle(&run).to_string().contains(SENTINEL_SECRET),
        "a credential reached the published bundle"
    );

    let leaked: Vec<String> = sweep
        .files_holding(SENTINEL_SECRET)
        .into_iter()
        .filter(|path| !is_fixture_recording(path))
        .collect();
    assert_eq!(
        leaked,
        Vec::<String>::new(),
        "the credential reached a file that is not a fixture's record of its own \
         environment"
    );
}

#[test]
fn the_bound_the_document_sets_is_the_bound_the_sweep_applies() {
    let bounded = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());

    let run = bounded.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert!(
        !bounded.has_verdict(OS_CVE),
        "the second fixable finding is over the bound, so it was deferred rather \
         than judged, and a deferred finding is not a verdict: {}",
        bounded.verdicts()
    );
    assert_eq!(
        bounded.verdicts().as_array().map(Vec::len),
        Some(0),
        "and nothing else took its place: {}",
        bounded.verdicts()
    );

    assert_eq!(bounded.pull_requests().len(), 1);
    let branch = the_one_new_branch(&bounded);
    assert!(
        pushed_file(&bounded, &branch, "go.mod").contains(&format!("{MODULE} {FIXED_VERSION}")),
        "the advisory within the bound is still mitigated"
    );

    let reached = bounded.disposition(&run);
    assert_eq!(
        reached["deferred"],
        serde_json::json!([{ "cve": OS_CVE, "bound": 1 }]),
        "the advisory over the bound, and the number that put it over: {reached}"
    );
    assert_eq!(
        reached["reason"], "pull_request",
        "deferring a finding does not change what the run came to: {reached}"
    );
}

#[test]
fn the_grades_the_document_named_are_the_grades_the_run_acted_on() {
    let by_default = Sweep::scanning_rescanning(
        VULNERABLE,
        SCAN_MEDIUM_LIBRARY,
        SCAN_CLEAN,
        2,
        a_script_no_attempt_consumes(),
    );
    let run = by_default.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "a document with nothing this deployment acts on is not a failed run —          stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        by_default.disposition(&run),
        nothing_to_do_publishes(),
        "a MEDIUM finding is outside what a document naming no grades means, so          this run had nothing to act on"
    );
    assert_eq!(
        by_default.pull_requests().len(),
        0,
        "and it proposed nothing: {:?}",
        by_default.pull_requests()
    );

    let widened = Sweep::scanning_grades(
        VULNERABLE,
        SCAN_MEDIUM_LIBRARY,
        SCAN_CLEAN,
        GRADES_INCLUDING_MEDIUM,
        2,
        a_repair_moving_the_requirement(),
    );
    let run = widened.run();
    let payload = widened.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        widened.disposition(&run)["reason"],
        "pull_request",
        "the deployment named MEDIUM, so the MEDIUM finding is work: {}",
        widened.disposition(&run)
    );
    assert_eq!(
        widened.pull_requests().len(),
        1,
        "exactly one pull request: {:?}",
        widened.pull_requests()
    );
    let branch = the_one_new_branch(&widened);
    let landed = pushed_file(&widened, &branch, "go.mod");
    assert!(
        landed.contains(&format!("{MODULE} {FIXED_VERSION}")),
        "and the branch carries the requirement at the fixed release, which is          the whole of what acting on a finding means here: {landed}"
    );
    assert!(
        !landed.contains(VULNERABLE_VERSION),
        "and no longer at the vulnerable one: {landed}"
    );
}

#[test]
fn a_deferred_finding_is_in_neither_the_verdict_set_nor_the_already_fixed_set() {
    let sweep = Sweep::scanning_rescanning(
        VULNERABLE,
        SCAN_TWO_OS,
        SCAN_TWO_OS,
        1,
        an_attempt_declining(&[LIBRARY_CVE]),
    );

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let scanned = std::fs::read_to_string(sweep.scenario.report_dir().join("scan/scan.json"))
        .expect("the scanner left no artefact, so nothing below is about a document");
    for cve in [LIBRARY_CVE, OS_CVE, SECOND_OS_CVE] {
        assert!(
            scanned.contains(cve),
            "the scan does not name {cve}, so its absence from a set below is \
             the scanner's silence and not this run's decision: {scanned}"
        );
    }

    let reached = sweep.disposition(&run);
    assert_eq!(
        reached,
        serde_json::json!({
            "reason": "unsafe_without_direction",
            "verdicts": 1,
            "already_fixed": [],
            "deferred": [
                { "cve": OS_CVE, "bound": 1 },
                { "cve": SECOND_OS_CVE, "bound": 1 },
            ],
            "attempts": [{
                "cves": [LIBRARY_CVE],
                "status": "needs_work",
                "claimed_complete": false,
                "forbidden": [],
            }],
            "branch": serde_json::Value::Null,
            "pull_request": serde_json::Value::Null,
        }),
        "three findings, three sets, and which advisory is in which"
    );

    let verdicts = sweep.verdicts();
    assert!(
        sweep.has_verdict(LIBRARY_CVE),
        "the finding inside the bound was judged, so it has a row: {verdicts}"
    );
    for cve in [OS_CVE, SECOND_OS_CVE] {
        assert!(
            !sweep.has_verdict(cve),
            "the finding past the bound was never judged, and a row for {cve} \
             would be this build claiming an opinion it does not have — beside a \
             report that does hold a row, so this is not an empty report \
             passing: {verdicts}"
        );
    }

    assert!(
        sweep.pull_requests().is_empty(),
        "a run whose only attempt declined opens nothing: {:?}",
        sweep.pull_requests()
    );
    sweep.assert_every_receipt_is_logical(&run);
}

const SHARED_BRANCH: &str = "security/cve-remediation-2026-01-02";

const SHARED_PR: u64 = 41;

const PRIOR_RUN_MARKER: &str = "EARLIER-RUN.md";

const STALE_BODY: &str = "fiddle attempted 1 advisory for this repository's \
     container image in one bounded attempt and committed nothing.\n\nEvery \
     advisory this run did not fix is in the verdict report published beside this \
     run's bundle, with the sentence that decided it.";

const RUN_BODY: &str = "fiddle attempted 1 advisory for this repository's \
     container image in one bounded attempt and committed what it changed.\n\nEvery \
     advisory this run did not fix is in the verdict report published beside this \
     run's bundle, with the sentence that decided it.";

impl Sweep {
    fn seed_shared_pull_request(&self, body: &str) -> String {
        self.seed_shared_pull_request_saying(body, "an earlier night's work on the shared branch")
    }

    fn seed_shared_pull_request_saying(&self, body: &str, commit: &str) -> String {
        git(&self.tree, &["checkout", "-q", "-b", SHARED_BRANCH]);
        std::fs::write(
            self.tree.join(PRIOR_RUN_MARKER),
            "An earlier run's notes, which only this branch carries.\n",
        )
        .unwrap();
        git(&self.tree, &["add", "-A"]);
        git(
            &self.tree,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                commit,
            ],
        );
        git(&self.tree, &["push", "-q", "origin", SHARED_BRANCH]);
        let head = git_says(&self.tree, &["rev-parse", "HEAD"]);
        git(&self.tree, &["checkout", "-q", SWEEP_BASE]);

        let owner = SWEEP_REPO.split('/').next().unwrap();
        std::fs::write(
            self.stub.join("pulls_seed"),
            serde_json::json!([{
                "number": SHARED_PR,
                "state": "open",
                "head": format!("{owner}:{SHARED_BRANCH}"),
                "base": SWEEP_BASE,
                "title": "acme: dependency advisories",
                "body": body,
                "labels": ["security/cve"],
            }])
            .to_string(),
        )
        .unwrap();
        head
    }

    fn pull_request(&self, number: u64) -> serde_json::Value {
        let out = Command::new(gh_stub_binary())
            .args(["--stub-dir", self.stub.to_str().unwrap()])
            .args([
                "api",
                "--method",
                "GET",
                &format!("/repos/{SWEEP_REPO}/pulls/{number}"),
            ])
            .output()
            .unwrap();
        support::object_of(&String::from_utf8_lossy(&out.stdout))
            .unwrap_or_else(|| panic!("the forge holds no pull request #{number}"))
    }

    fn mutations(&self) -> Vec<String> {
        std::fs::read_to_string(self.stub.join("world"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter_map(|landed| landed["key"].as_str().map(str::to_string))
            .collect()
    }
}

#[test]
fn a_second_run_over_a_shared_pull_request_rewrites_its_body_and_works_in_its_tree() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());
    let seeded_head = sweep.seed_shared_pull_request(STALE_BODY);

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    let pulls = sweep.pull_requests();
    assert_eq!(
        pulls.len(),
        1,
        "the shared pull request is shared: {pulls:?}"
    );
    assert_eq!(pulls[0]["number"], SHARED_PR, "{:?}", pulls[0]);

    let reached = sweep.disposition(&run);
    assert_eq!(
        reached["reason"], "pull_request",
        "this run landed a clean group onto the shared branch, so it is row 4 \
         and not the row for work somebody else's pull request already carries: \
         {reached}"
    );
    assert_eq!(reached["pull_request"], SHARED_PR, "{reached}");
    assert_eq!(reached["branch"], SHARED_BRANCH, "{reached}");

    let shared = sweep.pull_request(SHARED_PR);
    assert_eq!(
        shared["body"].as_str(),
        Some(RUN_BODY),
        "the shared pull request must describe tonight's run: {shared}"
    );
    assert_eq!(
        sweep.mutations(),
        vec![format!("PATCH_repos_acme_r_pulls_{SHARED_PR}")],
        "exactly one forge mutation landed, and it is the body rewrite — no \
         create, because the postcondition for a pull request on this head and \
         base already held"
    );

    let base_revision = git_says(&sweep.remote, &["rev-parse", SWEEP_BASE]);
    let bundle = sweep.bundle(&run);
    assert_eq!(
        bundle["observations"]["tree"],
        serde_json::json!({
            "base_revision": base_revision,
            "pr_head": seeded_head,
            "attempt_tree": "pr_head",
            "scanned_image_digest": SCANNED_DIGEST,
        }),
        "{bundle}"
    );
    assert_ne!(
        seeded_head, base_revision,
        "the seeded head has to be a commit the base does not have, or \
         `attempt_tree` could be either and the assertion above would not say \
         which tree was used"
    );

    assert!(
        pushed_file(&sweep, SHARED_BRANCH, PRIOR_RUN_MARKER).contains("An earlier run's notes"),
        "the work was built on the pull request's tip, so its history survives"
    );
    assert!(
        pushed_file(&sweep, SHARED_BRANCH, "go.mod").contains(&format!("{MODULE} {FIXED_VERSION}")),
        "and the branch carries the requirement at the fixed release"
    );
}

#[test]
fn a_run_whose_shared_body_is_unchanged_dispatches_no_rewrite() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());
    sweep.seed_shared_pull_request(RUN_BODY);

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert_eq!(
        sweep.mutations(),
        Vec::<String>::new(),
        "a pull request that already says what this run did is a postcondition \
         that already holds, and an effect the world satisfies is not dispatched"
    );
    assert_eq!(
        sweep.pull_request(SHARED_PR)["body"].as_str(),
        Some(RUN_BODY),
        "and the description is left exactly as it was found"
    );

    assert!(
        pushed_file(&sweep, SHARED_BRANCH, "go.mod").contains(&format!("{MODULE} {FIXED_VERSION}")),
        "the sweep still did its work"
    );
    let reached = sweep.disposition(&run);
    assert_eq!(
        reached["reason"], "pull_request",
        "no rewrite because the description was already right, and not because \
         the run decided there was nothing to land: {reached}"
    );
    assert_eq!(
        sweep.pull_requests().len(),
        1,
        "and opened nothing beside the pull request it was given"
    );
}

#[test]
fn a_second_run_opens_no_second_pull_request() {
    let sweep = Sweep::scanning_rescanning(
        VULNERABLE,
        SCAN_LIBRARY_ONLY,
        SCAN_CLEAN,
        2,
        two_nights(
            a_repair_moving_the_requirement(),
            an_attempt_finding_the_tree_already_at_the_fix(&[LIBRARY_CVE]),
        ),
    );

    let first = sweep.run();
    assert_eq!(
        first.status.code(),
        Some(0),
        "the first night must land its work, or the second has nothing to \
         deduplicate against - stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let opened = the_row_both_surfaces_agree_on(&sweep, &first, "the first night");
    assert_eq!(
        opened["reason"], "pull_request",
        "the first night has to cut a branch and open a pull request, or this \
         lane passes because there was never one to duplicate: {opened}"
    );
    let number = opened["pull_request"]
        .as_u64()
        .unwrap_or_else(|| panic!("the first night opened no pull request: {opened}"));
    assert_eq!(
        sweep.pull_requests().len(),
        1,
        "the first night must open exactly one: {:?}",
        sweep.pull_requests()
    );

    let branch = the_one_new_branch(&sweep);
    let commits = pushed_commits(&sweep, &branch);
    assert_eq!(commits.len(), 1, "one attempt is one commit: {commits:?}");
    assert!(
        commits[0].0.contains(&format!("Fixes: {LIBRARY_CVE}")),
        "the first night's commit names the advisory, which is what the second \
         night's log read finds: {commits:?}"
    );

    let second = sweep.run();
    assert_eq!(
        second.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );

    let reached = the_row_both_surfaces_agree_on(&sweep, &second, "the second night");
    assert_eq!(
        reached["reason"], "already_in_progress",
        "the second night reached the row for work that is already open, which \
         is the commit log's answer and nothing else's: {reached}"
    );
    assert_eq!(
        reached["pull_request"], number,
        "the second night must say why it opened nothing, and the reason is the \
         number it found already open: {reached}"
    );
    assert_eq!(
        sweep.pull_requests().len(),
        1,
        "the open labelled pull request is the dedup: {:?}",
        sweep.pull_requests()
    );
    assert_eq!(
        sweep
            .mutations()
            .iter()
            .filter(|key| *key == "POST_repos_acme_r_pulls")
            .count(),
        1,
        "exactly one create across both nights - a second would be a second \
          pull request whatever the listing says: {:?}",
        sweep.mutations()
    );

    assert_eq!(
        pushed_commits(&sweep, &branch).len(),
        1,
        "and the branch still carries the one commit the first night made, so \
         the second landed nothing rather than landing it twice"
    );

    let bundle = sweep.bundle(&second);
    assert_eq!(
        bundle["observations"]["tree"]["attempt_tree"], "pr_head",
        "the second night works in the pull request's tree, which is how it \
         reads the first night's commit at all: {bundle}"
    );
    assert_eq!(
        sweep.change_marker(),
        Some(sweep.scenario.expected_marker(SWEEP_REF)),
        "the second night records itself too - the marker is a record that a run \
         happened, and only what it found changed"
    );
}

#[test]
fn help_names_the_bare_form_the_grammar_accepts() {
    for command in ["run", "inspect"] {
        let out = support::fiddle_command()
            .args([command, "--help"])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{command} --help failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let help = String::from_utf8_lossy(&out.stdout);
        assert!(
            help.contains("`cve`"),
            "{command} --help never mentions the bare `cve` form:\n{help}"
        );
        assert!(
            help.contains("stands\n") || help.contains("stands alone"),
            "{command} --help mentions `cve` without saying it takes no value:\n{help}"
        );
    }
}
