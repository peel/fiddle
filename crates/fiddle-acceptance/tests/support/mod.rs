#![allow(dead_code)]

use assert_cmd::Command;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tempfile::TempDir;

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

pub fn fiddle_command() -> Command {
    Command::new(fiddle_binary())
}

pub fn gh_stub_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| runtime_fixture("gh_stub", "gh-stub"))
}

pub fn git_stub_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| runtime_fixture("git_stub", "git-stub"))
}

pub fn wiz_stub_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| runtime_fixture("wiz_stub", "wiz-stub"))
}

pub const WIZ_CONFIG_DIR: &str = "WIZ_CONFIG_DIR";

pub const WIZ_LOGIN_FILE: &str = "auth.json";

pub fn caller_logged_in() -> TempDir {
    let dir = TempDir::new().expect("a temporary home for the caller's wizcli login");
    std::fs::write(
        dir.path().join(WIZ_LOGIN_FILE),
        "{\"serviceAccount\":\"the-account-wizcli-auth-recorded\"}",
    )
    .expect("the login `wizcli auth` leaves for a later scan");
    dir
}

pub fn check_stub_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| runtime_fixture("check_stub", "check-stub"))
}

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

fn executable_from(build_log: &[u8], name: &str) -> Option<PathBuf> {
    String::from_utf8_lossy(build_log)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| message["reason"] == "compiler-artifact")
        .filter(|message| message["target"]["name"] == name)
        .find_map(|message| Some(PathBuf::from(message["executable"].as_str()?)))
}

pub const CREDENTIAL_VARS: [&str; 4] = [
    "GITHUB_TOKEN",
    "GH_TOKEN",
    "ANTHROPIC_API_KEY",
    "JIRA_API_TOKEN",
];

pub const BROKEN_FIXTURE: &str = "pub fn last_index(len: usize) -> usize { len }\n";

pub const REPAIRED_FIXTURE: &str = "pub fn last_index(len: usize) -> usize { len - 1 }\n";

pub const PROJECT_NAME: &str = "icecube";

pub struct Scenario {
    dir: TempDir,
}

impl Scenario {
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

    pub fn command(&self) -> Command {
        Command::from_std(self.std_command())
    }

    fn std_command(&self) -> std::process::Command {
        let mut command = std::process::Command::new(fiddle_binary());
        for name in CREDENTIAL_VARS {
            command.env_remove(name);
        }
        command
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir.path().join("fiddle.toml")
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }

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
                    std::fs::read(&path).unwrap_or_else(|source| {
                        panic!(
                            "`{relative}` was listed by the walk of the scenario directory \
                             and could not be read a moment later: {source}. A process the \
                             test does not wait for is writing inside the scenario directory \
                             while it is being snapshotted, so every byte-for-byte comparison \
                             of that directory is racy until that process is stopped"
                        )
                    })
                };
                (relative, bytes)
            })
            .collect();
        entries.sort();
        entries
    }

    pub fn assert_tree_unchanged(&self, before: &[(String, Vec<u8>)], property: &str) {
        assert_tree_unchanged(before, &self.project_tree(), property);
    }

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

    pub fn config_text(&self) -> String {
        std::fs::read_to_string(self.config_path()).unwrap()
    }

    pub fn append_config(&self, text: &str) {
        let mut document = self.config_text();
        document.push('\n');
        document.push_str(text);
        std::fs::write(self.config_path(), document).unwrap();
    }

    pub fn write_fixture_repo(&self) -> PathBuf {
        self.write_repo_of(&[
            ("src/lib.rs", BROKEN_FIXTURE),
            (".gitignore", "target/\nCargo.lock\n"),
        ])
    }

    pub fn write_repo_of(&self, files: &[(&str, &str)]) -> PathBuf {
        let repo = self.dir.path().join("fixture");
        for (path, contents) in files {
            let at = repo.join(path);
            if let Some(parent) = at.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(at, contents).unwrap();
        }
        git(&repo, &["init", "-q", "."]);
        git(&repo, &["config", "maintenance.auto", "false"]);
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

    pub fn write_config_variant(&self, name: &str, text: &str) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        path
    }

    pub fn config_check_raw(&self, config: &Path) -> std::process::Output {
        self.command()
            .args(["config", "check", "--config", config.to_str().unwrap()])
            .output()
            .unwrap()
    }

    pub fn stub_root(&self) -> PathBuf {
        self.dir.path().join("stub-state")
    }

    pub fn write_work_item(&self, work_id: &str, status: &str) {
        std::fs::write(
            self.stub_root().join(format!("work/{work_id}.json")),
            format!("{{\"id\":\"{work_id}\",\"status\":\"{status}\"}}"),
        )
        .unwrap();
    }

    pub fn write_change_marker(&self, work_id: &str, marker: &str) {
        std::fs::write(
            self.stub_root().join(format!("changes/{work_id}.json")),
            format!("{{\"marker\":\"{marker}\"}}"),
        )
        .unwrap();
    }

    pub fn report_dir(&self) -> PathBuf {
        self.dir.path().join("reports")
    }

    #[cfg(unix)]
    pub fn make_report_dir_unwritable(&self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(self.report_dir()).unwrap();
        std::fs::set_permissions(self.report_dir(), std::fs::Permissions::from_mode(0o500))
            .unwrap();
    }

    pub fn prepare_journal_dir(&self) {
        std::fs::create_dir_all(self.report_dir().join(".attempts")).unwrap();
    }

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

    #[cfg(unix)]
    pub fn make_changes_dir_unwritable(&self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            self.stub_root().join("changes"),
            std::fs::Permissions::from_mode(0o500),
        )
        .unwrap();
    }

    #[cfg(unix)]
    pub fn make_changes_dir_writable(&self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            self.stub_root().join("changes"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }

    pub fn journal_records(&self) -> Vec<PathBuf> {
        walkdir_files(self.report_dir().join(".attempts"))
    }

    #[cfg(unix)]
    pub fn make_report_dir_writable(&self) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(self.report_dir(), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }

    pub fn stub_snapshot(&self) -> Vec<(String, Vec<u8>)> {
        let root = self.stub_root();
        let mut files = Vec::new();
        collect_files(&root, &root, &mut files);
        files.sort();
        files
    }

    pub fn expected_marker(&self, invocation_ref: &str) -> String {
        blake3::hash(format!("{PROJECT_NAME}\0{invocation_ref}").as_bytes()).to_hex()[..16]
            .to_string()
    }

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

    pub fn remove_stub_root(&self) {
        std::fs::remove_dir_all(self.stub_root()).unwrap();
    }

    pub fn hide_stub_root(&self) {
        std::fs::rename(self.stub_root(), self.hidden_stub_root()).unwrap();
    }

    pub fn restore_stub_root(&self) {
        std::fs::rename(self.hidden_stub_root(), self.stub_root()).unwrap();
    }

    fn hidden_stub_root(&self) -> PathBuf {
        self.dir.path().join("stub-state.hidden")
    }

    pub fn read_change_marker(&self, work_id: &str) -> Option<String> {
        let path = self.stub_root().join(format!("changes/{work_id}.json"));
        let text = std::fs::read_to_string(&path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| {
            panic!("{} is not JSON ({e}): {text}", path.display());
        });
        value["marker"].as_str().map(str::to_string)
    }

    pub fn run_json(&self, invocation_ref: &str, code: i32) -> serde_json::Value {
        self.run_json_with(&[], invocation_ref, code)
    }

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

    pub fn run_raw(&self, invocation_ref: &str) -> std::process::Output {
        self.run_raw_with(&[], invocation_ref)
    }

    pub fn run_raw_with(&self, extra: &[&str], invocation_ref: &str) -> std::process::Output {
        self.run_command(invocation_ref)
            .args(extra)
            .output()
            .unwrap()
    }

    pub fn run_command(&self, invocation_ref: &str) -> Command {
        Command::from_std(self.spawnable_run_command(invocation_ref))
    }

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

    pub fn inspect_json(&self, invocation_ref: &str) -> serde_json::Value {
        self.inspect_json_expect_code(invocation_ref, 0)
    }

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

    pub fn inspect_raw_with(&self, extra: &[&str], invocation_ref: &str) -> std::process::Output {
        self.inspect_command(invocation_ref)
            .args(extra)
            .output()
            .unwrap()
    }

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

pub fn toml_string(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

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

pub fn tree_difference(before: &[(String, Vec<u8>)], after: &[(String, Vec<u8>)]) -> Vec<String> {
    let index = |entries: &[(String, Vec<u8>)]| -> std::collections::BTreeMap<String, Vec<u8>> {
        entries
            .iter()
            .map(|(path, bytes)| (path.clone(), bytes.clone()))
            .collect()
    };
    let was = index(before);
    let is = index(after);
    let mut difference = Vec::new();
    for (path, bytes) in &was {
        match is.get(path) {
            None => difference.push(format!("removed `{path}` ({} bytes)", bytes.len())),
            Some(now) if now != bytes => difference.push(format!(
                "changed `{path}` ({} bytes -> {} bytes)",
                bytes.len(),
                now.len()
            )),
            Some(_) => {}
        }
    }
    for (path, bytes) in &is {
        if !was.contains_key(path) {
            difference.push(format!("added `{path}` ({} bytes)", bytes.len()));
        }
    }
    difference.sort();
    difference
}

pub fn assert_tree_unchanged(
    before: &[(String, Vec<u8>)],
    after: &[(String, Vec<u8>)],
    property: &str,
) {
    let difference = tree_difference(before, after);
    assert!(
        difference.is_empty(),
        "{property}; the scenario directory differs by:\n  {}",
        difference.join("\n  ")
    );
    assert_eq!(
        before.len(),
        after.len(),
        "{property}; the two snapshots hold a different number of entries although no \
         path was added, removed or changed, which means one path was listed twice and \
         the difference above cannot be trusted"
    );
}

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

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

pub struct Reply {
    pub status: u16,
    pub phrase: &'static str,
    pub body: serde_json::Value,
}

pub fn accepted(body: serde_json::Value) -> Reply {
    Reply {
        status: 200,
        phrase: "OK",
        body,
    }
}

pub fn refused(status: u16, phrase: &'static str, body: serde_json::Value) -> Reply {
    Reply {
        status,
        phrase,
        body,
    }
}

pub struct StubGateway {
    port: u16,
    served: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    bodies: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl StubGateway {
    pub fn serving(script: Vec<Reply>) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let served = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&served);
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let recorder = Arc::clone(&bodies);
        std::thread::spawn(move || {
            for reply in script {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let Ok(body) = answer(stream, &reply) else {
                    return;
                };
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

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    pub fn served(&self) -> usize {
        self.served.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn request_bodies(&self) -> Vec<String> {
        self.bodies
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

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

fn content_length(head: &[u8]) -> usize {
    String::from_utf8_lossy(head)
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub const JIRA_ISSUE_KEY: &str = "IDENT-1";

pub const JIRA_ISSUE_STATUS_ID: &str = "10001";

pub const JIRA_ISSUE_STATUS: &str = "In Review";

pub const JIRA_ISSUE_CATEGORY: &str = "In Progress";

pub const JIRA_ISSUE_UPDATED: &str = "2026-08-26T01:30:00.000+0530";

const JIRA_ISSUE_ROUTE: &str = "/rest/api/3/issue/";

const JIRA_CREATE_ROUTE: &str = "/rest/api/3/issue";

const JIRA_SEARCH_ROUTE: &str = "/rest/api/3/search/jql";

const JIRA_UNCLAIMED: &str =
    r#"{"errorMessages":["the issue carries no property with that key"],"errors":{}}"#;

const JIRA_UNROUTED: &str =
    r#"{"errorMessages":["the site serves no resource at that path"],"errors":{}}"#;

const JIRA_NOT_ALLOWED: &str =
    r#"{"errorMessages":["the site does not serve that method here"],"errors":{}}"#;

const JIRA_UNPARSED: &str =
    r#"{"errorMessages":["the request line could not be parsed"],"errors":{}}"#;

enum Answer {
    Issue {
        path: String,
        body: String,
    },
    Refusal {
        status: u16,
        body: String,
    },
    Filing {
        key: String,
        properties: std::collections::HashMap<String, String>,
    },
}

struct Recorded {
    authorizations: Vec<String>,
    request_lines: Vec<String>,
    request_bodies: Vec<String>,
    answer: Answer,
}

pub struct StubJira {
    port: u16,
    state: std::sync::Arc<std::sync::Mutex<Recorded>>,
}

impl StubJira {
    pub fn holding_the_issue() -> Self {
        StubJira::serving(Answer::Issue {
            path: format!("{JIRA_ISSUE_ROUTE}{JIRA_ISSUE_KEY}"),
            body: serde_json::json!({
                "id": "10000",
                "key": JIRA_ISSUE_KEY,
                "fields": {
                    "updated": JIRA_ISSUE_UPDATED,
                    "status": {
                        "id": JIRA_ISSUE_STATUS_ID,
                        "name": JIRA_ISSUE_STATUS,
                        "statusCategory": {
                            "id": 4,
                            "key": "indeterminate",
                            "name": JIRA_ISSUE_CATEGORY,
                        },
                    },
                },
            })
            .to_string(),
        })
    }

    pub fn refusing_the_credential() -> Self {
        StubJira::serving(Answer::Refusal {
            status: 401,
            body: serde_json::json!({
                "errorMessages": ["the site refused this request"],
                "errors": {},
            })
            .to_string(),
        })
    }

    pub fn filing_as(key: &str) -> Self {
        StubJira::serving(Answer::Filing {
            key: key.to_string(),
            properties: std::collections::HashMap::new(),
        })
    }

    fn serving(answer: Answer) -> Self {
        use std::sync::{Arc, Mutex};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(Recorded {
            authorizations: Vec::new(),
            request_lines: Vec::new(),
            request_bodies: Vec::new(),
            answer,
        }));
        let serving = Arc::clone(&state);
        std::thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                let _ = answer_recording(stream, &serving);
            }
        });
        StubJira { port, state }
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Recorded> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn served(&self) -> usize {
        self.held().authorizations.len()
    }

    pub fn request_lines(&self) -> Vec<String> {
        self.held().request_lines.clone()
    }

    pub fn request_bodies(&self) -> Vec<String> {
        self.held().request_bodies.clone()
    }

    pub fn the_only_authorization(&self) -> String {
        let held = self.held();
        let mut distinct = held.authorizations.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            1,
            "one credential reached this site, so it received one header: \
             {distinct:?}"
        );
        distinct.remove(0)
    }
}

fn answer_recording(
    mut stream: std::net::TcpStream,
    state: &std::sync::Arc<std::sync::Mutex<Recorded>>,
) -> std::io::Result<()> {
    use std::io::{Read, Write};

    let mut request = Vec::new();
    let mut chunk = [0u8; 4096];
    let boundary = loop {
        if let Some(at) = find(&request, b"\r\n\r\n") {
            break at + 4;
        }
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            return Ok(());
        }
        request.extend_from_slice(&chunk[..read]);
    };

    let length = content_length(&request[..boundary]);
    while request.len() < boundary + length {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
    }
    let sent = String::from_utf8_lossy(&request[boundary..]).into_owned();

    let head = String::from_utf8_lossy(&request[..boundary]).into_owned();
    let request_line = head.lines().next().unwrap_or_default().to_string();
    let authorization = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("authorization"))
        .map(|(_, value)| value.trim().to_string())
        .unwrap_or_default();

    let (status, body) = {
        let mut held = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        held.authorizations.push(authorization);
        held.request_lines.push(request_line.clone());
        held.request_bodies.push(sent.clone());
        routed(&request_line, &sent, &mut held.answer)
    };

    stream.write_all(
        format!(
            "HTTP/1.1 {status} {}\r\ncontent-type: application/json\r\n\
             content-length: {}\r\nconnection: close\r\n\r\n{body}",
            reason(status),
            body.len(),
        )
        .as_bytes(),
    )?;
    stream.flush()?;
    let _ = stream.shutdown(std::net::Shutdown::Write);
    Ok(())
}

fn routed(request_line: &str, sent: &str, answer: &mut Answer) -> (u16, String) {
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return (400, JIRA_UNPARSED.to_string());
    };
    if !version.starts_with("HTTP/") || !target.starts_with('/') {
        return (400, JIRA_UNPARSED.to_string());
    }
    let path = target.split('?').next().unwrap_or(target);
    if let Answer::Filing { key, properties } = answer {
        return filed(method, path, sent, key, properties);
    }
    if method != "GET" {
        return (405, JIRA_NOT_ALLOWED.to_string());
    }
    match answer {
        Answer::Refusal { status, body } => (*status, body.clone()),
        Answer::Issue { path: held, body } => match path == held {
            true => (200, body.clone()),
            false => (404, JIRA_UNROUTED.to_string()),
        },
        Answer::Filing { .. } => unreachable!("a filing site answers above"),
    }
}

fn filed(
    method: &str,
    path: &str,
    sent: &str,
    key: &str,
    properties: &mut std::collections::HashMap<String, String>,
) -> (u16, String) {
    let claim = path.starts_with(JIRA_ISSUE_ROUTE) && path.contains("/properties/");
    match (method, path) {
        ("GET", _) if claim => match properties.get(path) {
            Some(held) => (200, format!(r#"{{"key":"{path}","value":{held}}}"#)),
            None => (404, JIRA_UNCLAIMED.to_string()),
        },
        ("PUT", _) if claim => {
            properties.insert(path.to_string(), sent.to_string());
            (200, "{}".to_string())
        }
        ("DELETE", _) if claim => {
            properties.remove(path);
            (200, "{}".to_string())
        }
        ("GET", JIRA_SEARCH_ROUTE) => (200, r#"{"issues":[]}"#.to_string()),
        ("POST", JIRA_CREATE_ROUTE) => (
            201,
            serde_json::json!({ "id": "10100", "key": key }).to_string(),
        ),
        _ => (404, JIRA_UNROUTED.to_string()),
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Unassigned",
    }
}

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

pub fn reports(report: serde_json::Value) -> serde_json::Value {
    completion(
        serde_json::json!({ "role": "assistant", "content": report.to_string() }),
        "stop",
    )
}

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

pub const REDIRECTED_FIXTURE: &str = "pub fn last_index(len: usize) -> usize {\n    \
     // the other crate's convention, per the instruction\n    len - 1\n}\n";

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

pub fn redirects(instruction: &str, evidence: &str) -> Reply {
    accepted(reports(serde_json::json!({
        "decision": "redirect",
        "redirect": instruction,
        "evidence": evidence,
    })))
}

pub fn a_suspension_and_its_redirect(instruction: &str, evidence: &str) -> Vec<Reply> {
    let mut script = a_real_repair();
    script.push(redirects(instruction, evidence));
    script.extend(a_second_repair());
    script
}

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

pub fn interprets(verdict: &str, evidence: &str) -> Reply {
    accepted(reports(serde_json::json!({
        "decision": verdict,
        "redirect": serde_json::Value::Null,
        "evidence": evidence,
    })))
}

pub fn a_suspension_and_its_approval(approval: &str) -> Vec<Reply> {
    let mut script = a_real_repair();
    script.push(interprets("approve", approval));
    script
}

pub fn a_suspension_and_a_hostile_interpretation(approval: &str) -> Vec<Reply> {
    let mut script = a_real_repair();
    script.push(accepted(reports(serde_json::json!({
        "decision": "approve",
        "redirect": serde_json::Value::Null,
        "evidence": approval,
        "effect": "dead0beef0dead00",
        "payload": "0feed0dad0cafe00",
    }))));
    script
}

pub const WORK_ID: &str = "m3-demo";
pub const INVOCATION_REF: &str = "beans:m3-demo";

pub const SECOND_WORK_ID: &str = "m3-demo-again";
pub const SECOND_INVOCATION_REF: &str = "beans:m3-demo-again";

pub const REPO: &str = "acme/r";
pub const BASE: &str = "main";

pub const AUTHORIZED: u64 = 505_401;

pub const STRANGER: u64 = 999_999;

pub const FORGE_CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";
pub const MODEL_CREDENTIAL: &str = "LITELLM_API_KEY";

pub const WORLD_CREDENTIAL_VARS: [&str; 2] = [FORGE_CREDENTIAL, MODEL_CREDENTIAL];

pub const FIDDLE_BOT: u64 = 1_000_001;

pub const SENTINEL: &str = "ghp_m3_sentinel_must_never_be_printed_7c04";

pub const PULL_REQUEST_NODE_ID: &str = "PR_kwDOm3demoNode7";

pub const CONVERSATION_ISSUE: u64 = 7;

pub const SEEDED_AT: &str = "2026-08-11T00:00:00Z";

pub const EDITED_AT: &str = "2026-08-11T12:00:00Z";

pub const WRITTEN_BEFORE_AN_EDIT: &str = "2026-08-10T00:00:00Z";

pub const FIRST_REVIEW_COMMENT: u64 = 9_500;

pub const POSTING_APP: fn() -> serde_json::Value = || {
    serde_json::json!({
        "id": 77_001,
        "slug": "some-automation",
        "name": "Some Automation",
    })
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    pub request: String,
    pub effect: String,
    pub payload: String,
    pub head_sha: String,
}

pub fn parse_marker(body: &str) -> Result<Binding, String> {
    const OPENING: &str = "<!-- fiddle:decision ";
    const CLOSING: &str = " -->";
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

pub fn render_marker(binding: &Binding) -> String {
    format!(
        "<!-- fiddle:decision v1 request={} effect={} payload={} head={} -->",
        binding.request, binding.effect, binding.payload, binding.head_sha
    )
}

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

pub fn expected_ticket_marker(
    project: &str,
    invocation_ref: &str,
    project_key: &str,
    cve: &str,
) -> String {
    let target = format!("{project_key}/{cve}");
    let identity = truncated_digest(&length_prefixed([
        project,
        invocation_ref,
        "jira.issue_filed",
        &target,
    ]));
    format!("fiddle-cve-{identity}")
}

fn length_prefixed<const N: usize>(fields: [&str; N]) -> String {
    let mut material = String::new();
    for field in fields {
        material.push_str(&field.len().to_string());
        material.push(':');
        material.push_str(field);
    }
    material
}

fn truncated_digest(material: &str) -> String {
    blake3::hash(material.as_bytes()).to_hex()[..16].to_string()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Comment {
    pub id: u64,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    pub author: u64,
    pub is_bot: bool,
}

pub struct Run {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub struct World {
    scenario: Scenario,
    stub: PathBuf,
    remote: PathBuf,
    work: PathBuf,
    gateway: StubGateway,
    token: String,
}

impl World {
    pub fn new() -> Self {
        World::with_model_script(a_real_repair().into_iter().chain(a_real_repair()).collect())
    }

    pub fn with_model_script(script: Vec<Reply>) -> Self {
        let scenario = Scenario::new();
        scenario.write_work_item(WORK_ID, "open");
        scenario.write_work_item(SECOND_WORK_ID, "open");
        let work = scenario.write_fixture_repo();

        let stub = scenario.dir().join("gh-stub");
        std::fs::create_dir_all(stub.join("script")).unwrap();
        std::fs::create_dir_all(stub.join("config")).unwrap();
        std::fs::create_dir_all(stub.join(CONVERSATION)).unwrap();
        std::fs::write(stub.join(CONVERSATION).join("page-1.json"), "[]").unwrap();

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

    pub fn with_token_sentinel(mut self, token: &str) -> Self {
        self.token = token.to_string();
        self
    }

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

    pub fn fiddle<const N: usize>(&self, args: [&str; N]) -> Run {
        self.launch(args, true)
    }

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

    fn command<const N: usize>(
        &self,
        args: [&str; N],
        credentialled: bool,
    ) -> std::process::Command {
        let mut command = std::process::Command::new(fiddle_binary());
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

    pub fn config_text(&self) -> String {
        self.scenario.config_text()
    }

    pub fn bundle(&self, run: &Run) -> serde_json::Value {
        let payload: serde_json::Value = serde_json::from_str(&run.stdout).unwrap_or_else(|e| {
            panic!("stdout is not JSON ({e}): {}", run.stdout);
        });
        self.scenario.read_bundle(&payload)
    }

    pub fn expected_marker(&self, invocation_ref: &str) -> String {
        self.scenario.expected_marker(invocation_ref)
    }

    pub fn expected_effect_id(&self, invocation_ref: &str, pr: u64, head_sha: &str) -> String {
        expected_effect_id(PROJECT_NAME, invocation_ref, REPO, pr, head_sha)
    }

    pub fn expected_request_id(&self, invocation_ref: &str, pr: u64, head_sha: &str) -> String {
        expected_request_id(PROJECT_NAME, invocation_ref, REPO, pr, head_sha)
    }

    pub fn attempt_id(&self, run: &Run) -> String {
        self.bundle(run)["attempt_id"]
            .as_str()
            .unwrap_or_else(|| panic!("a bundle names its attempt: {}", self.bundle(run)))
            .to_string()
    }

    pub fn work_ref(&self, run: &Run) -> String {
        let bundle = self.bundle(run);
        bundle["work_ref"]
            .as_str()
            .unwrap_or_else(|| panic!("a bundle over observed work names it: {bundle}"))
            .to_string()
    }

    pub fn repair(&self) -> Run {
        self.fiddle([
            "run",
            "--capability",
            "fixture_repair",
            INVOCATION_REF,
            "--json",
        ])
    }

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

    pub fn requests(&self) -> Vec<serde_json::Value> {
        walkdir_files(self.stub.join("requests"))
            .iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .filter_map(|text| serde_json::from_str(&text).ok())
            .collect()
    }

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

    pub fn post_comment(&self, author: u64, body: &str) -> u64 {
        self.write_listed_comment(author, body, "User", serde_json::Value::Null)
    }

    pub fn post_bot_comment(&self, author: u64, body: &str) -> u64 {
        self.write_listed_comment(author, body, "Bot", serde_json::Value::Null)
    }

    pub fn post_app_comment(&self, author: u64, body: &str) -> u64 {
        self.write_listed_comment(author, body, "User", POSTING_APP())
    }

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

    pub fn conversation(&self) -> Vec<Comment> {
        self.listed_comments().iter().map(comment_from).collect()
    }

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

    pub fn edit_comment_on_next_read(&self, id: u64, body: &str) {
        self.script_the_re_read(id, |comment| {
            comment["body"] = serde_json::Value::String(body.to_string());
            comment["updated_at"] = serde_json::Value::String(EDITED_AT.to_string());
        });
    }

    pub fn show_as_edited_before_the_listing(&self, id: u64) {
        self.script_the_re_read(id, |comment| {
            comment["created_at"] = serde_json::Value::String(WRITTEN_BEFORE_AN_EDIT.to_string());
        });
    }

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

    pub fn lose_the_answer_to_the_question(&self) {
        let key = format!(
            "POST_{}",
            format!("repos/{REPO}/issues/{CONVERSATION_ISSUE}/comments").replace('/', "_")
        );
        std::fs::write(self.stub.join("script").join(key), "201 0 commit_then_die").unwrap();
    }

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

    pub fn request_comments(&self) -> Vec<Comment> {
        self.conversation()
            .into_iter()
            .filter(|comment| comment.is_bot)
            .collect()
    }

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

    pub fn comments_naming(&self, text: &str) -> Vec<Comment> {
        self.conversation()
            .into_iter()
            .filter(|comment| comment.body.contains(text))
            .collect()
    }

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

    pub fn listing(&self, page: u64) -> String {
        self.gh(&[
            "api",
            "--method",
            "GET",
            &format!("/repos/{REPO}/issues/{CONVERSATION_ISSUE}/comments?per_page=100&page={page}"),
        ])
    }

    pub fn script_graphql(&self, n: usize, status: u16, body: serde_json::Value) {
        let dir = self.stub.join("graphql");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{n}.json")),
            serde_json::json!({"status": status, "body": body}).to_string(),
        )
        .unwrap();
    }

    pub fn model_calls(&self) -> usize {
        self.gateway.served()
    }

    pub fn graphql_calls(&self) -> usize {
        std::fs::read_to_string(self.stub.join("graphql_calls"))
            .ok()
            .and_then(|count| count.trim().parse().ok())
            .unwrap_or(0)
    }

    pub fn graphql(&self, query: &str) -> String {
        self.gh(&["api", "graphql", "-f", &format!("query={query}")])
    }

    pub fn accept_the_ready_mutation(&self) {
        self.script_graphql(
            0,
            200,
            serde_json::json!({
                "data": { "markPullRequestReadyForReview": { "clientMutationId": null } }
            }),
        );
    }

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

    pub fn open_pull_requests(&self) -> Vec<serde_json::Value> {
        body_of(&self.gh(&[
            "api",
            "--method",
            "GET",
            &format!("/repos/{REPO}/pulls?state=open"),
        ]))
    }

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

    pub fn answer_pull_request_by_number(&self, number: u64, branch: &str) -> String {
        let head_sha = self.remote_head(branch);
        let dir = self.stub.join("pulls_by_number");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{number}.json")),
            serde_json::json!({
                "number": number,
                "state": "open",
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

    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> bool {
        std::process::Command::new("git")
            .args(["merge-base", "--is-ancestor", ancestor, descendant])
            .current_dir(&self.remote)
            .status()
            .unwrap()
            .success()
    }

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

    pub fn remote_head(&self, branch: &str) -> String {
        git_says(
            &self.remote,
            &["rev-parse", &format!("refs/heads/{branch}")],
        )
    }

    pub fn push_branch(&self, branch: &str) {
        git(
            &self.work,
            &["push", "-q", "origin", &format!("HEAD:{branch}")],
        );
    }

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

    fn gh(&self, args: &[&str]) -> String {
        self.gh_sending(args, "")
    }

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

    pub fn report_dir(&self) -> PathBuf {
        self.scenario.report_dir()
    }

    pub fn report_bundles(&self) -> Vec<PathBuf> {
        let journal = self.report_dir().join(ATTEMPTS);
        walkdir_files(self.report_dir())
            .into_iter()
            .filter(|path| !path.starts_with(&journal))
            .collect()
    }

    pub fn journal_records(&self) -> Vec<PathBuf> {
        self.scenario.journal_records()
    }

    pub fn workspace_root(&self) -> PathBuf {
        self.scenario.dir().join("workspaces")
    }

    pub fn worktrees(&self) -> Vec<PathBuf> {
        walkdir_dirs(self.workspace_root())
            .into_iter()
            .chain(walkdir_files(self.workspace_root()))
            .collect()
    }

    pub fn all_published_bytes(&self) -> String {
        let mut all = String::new();
        for path in walkdir_files(self.report_dir()) {
            all.push_str(&String::from_utf8_lossy(&std::fs::read(&path).unwrap()));
        }
        all
    }

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

    pub fn delete_attempt_journal(&self) {
        remove_tree(&self.report_dir().join(ATTEMPTS));
        assert!(
            self.journal_records().is_empty(),
            "no attempt record may survive: {:?}",
            self.journal_records()
        );
    }

    pub fn delete_workspaces(&self) {
        remove_tree(&self.workspace_root());
        assert!(
            self.worktrees().is_empty(),
            "no workspace may survive: {:?}",
            self.worktrees()
        );
    }

    pub fn local_state_is_empty(&self) -> bool {
        self.report_bundles().is_empty()
            && self.journal_records().is_empty()
            && self.worktrees().is_empty()
    }

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
        let leftover = self.worktrees();

        let status = std::process::Command::new("kill")
            .args(["-9", &child.id().to_string()])
            .status()
            .expect("kill is on the PATH");
        assert!(status.success(), "could not kill {}", child.id());
        child.wait().unwrap();
        leftover
    }

    fn a_worktree_is_checked_out(&self) -> bool {
        walkdir_files(self.workspace_root())
            .iter()
            .any(|path| path.file_name().and_then(|name| name.to_str()) == Some("lib.rs"))
    }

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

    fn page_path(&self, page: u64) -> PathBuf {
        self.stub
            .join(CONVERSATION)
            .join(format!("page-{page}.json"))
    }

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

const CHECK: &str = "{ program = \"grep\", args = [\"-q\", \"len - 1\", \"src/lib.rs\"] }";

const CONVERSATION: &str = "issue-comments";

const REVIEW_COMMENTS: &str = "review-comments";

const BY_ID: &str = "by-id";

const ATTEMPTS: &str = ".attempts";

fn remove_tree(path: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        match std::fs::remove_dir_all(path) {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) if std::time::Instant::now() < deadline => {
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

pub fn body_of(response: &str) -> Vec<serde_json::Value> {
    let Some((_, body)) = response.split_once("\r\n\r\n") else {
        return Vec::new();
    };
    match serde_json::from_str(body) {
        Ok(serde_json::Value::Array(listed)) => listed,
        _ => Vec::new(),
    }
}

pub fn object_of(response: &str) -> Option<serde_json::Value> {
    let (_, body) = response.split_once("\r\n\r\n")?;
    match serde_json::from_str(body) {
        Ok(value @ serde_json::Value::Object(_)) => Some(value),
        _ => None,
    }
}

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
