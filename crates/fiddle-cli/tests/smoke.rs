use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const CREDENTIAL_VAR: &str = "LITELLM_API_KEY";

const MODEL_VAR: &str = "FIDDLE_TIER1_MODEL";
const BASE_URL_VAR: &str = "FIDDLE_TIER1_BASE_URL";

const DEFAULT_MODEL: &str = "bedrock/moonshotai.kimi-k2.5";

const DEFAULT_BASE_URL: &str = "https://litellm.firn.snplow.net/v1";

const PROJECT: &str = "icecube";
const WORK_ID: &str = "fiddle-m1-smoke";
const INVOCATION_REF: &str = "beans:fiddle-m1-smoke";

const BROKEN: &str = "pub fn last_index(len: usize) -> usize { len }\n";

#[test]
#[ignore = "tier 1: requires LITELLM_API_KEY; run with --ignored"]
fn the_agent_loop_still_works_against_a_real_model() {
    let credential = match std::env::var(CREDENTIAL_VAR) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!(
            "tier 1 requires {CREDENTIAL_VAR}; it is opt-in, not skipped \
             silently. Load it without printing it:\n  \
             ( set -a; . .env; set +a; cargo test -p fiddle-cli -- --ignored --nocapture )"
        ),
    };
    let model = env_or(MODEL_VAR, DEFAULT_MODEL);
    let base_url = env_or(BASE_URL_VAR, DEFAULT_BASE_URL);

    let project = Project::new(&model, &base_url);

    let started = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_fiddle"))
        .args([
            "run",
            INVOCATION_REF,
            "--config",
            project.config_path().to_str().unwrap(),
            "--capability",
            "fixture_repair",
            "--json",
        ])
        .output()
        .expect("could not launch the fiddle binary");
    let latency = started.elapsed();

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert!(
        !stdout.contains(&credential),
        "the credential reached stdout"
    );
    assert!(
        !stderr.contains(&credential),
        "the credential reached stderr"
    );
    let leaked: Vec<String> = project
        .files()
        .into_iter()
        .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(&credential))
        .map(|(path, _)| path)
        .collect();
    assert!(
        leaked.is_empty(),
        "the credential was written to {leaked:?}"
    );

    let payload: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!("stdout is not the `--json` payload ({e}):\nstdout = {stdout}\nstderr = {stderr}")
    });

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "fixture_repair",
        "the run must execute the capability it was asked for: {payload}"
    );

    let evidence: Vec<String> = payload["capability_executions"][0]["evidence"]
        .as_array()
        .unwrap_or_else(|| panic!("an execution must carry evidence: {payload}"))
        .iter()
        .map(|reference| reference.as_str().unwrap_or_default().to_string())
        .collect();
    let tool_calls = evidence
        .iter()
        .find_map(|reference| reference.strip_prefix("tools:"))
        .unwrap_or_else(|| panic!("the execution must report what its tools did: {evidence:?}"))
        .parse::<usize>()
        .unwrap_or_else(|e| panic!("`tools:` must carry a count ({e}): {evidence:?}"));
    assert!(
        tool_calls > 0,
        "the model called no tools at all, so the agent loop did not run. That \
         is the wiring, not the model: a run that answers with the structured \
         report on its first turn has not been given a working tool loop. \
         Evidence: {evidence:?}"
    );

    let conclusion = Conclusion::read(&payload, &stderr);
    assert_eq!(
        out.status.code(),
        Some(conclusion.exit_code()),
        "the exit code must be the one the exit-code table gives this outcome; \
         stderr = {stderr}"
    );

    let bundle_path = project.report_dir().join(
        payload["report"]
            .as_str()
            .unwrap_or_else(|| panic!("the run published no bundle: {payload}")),
    );
    let bundle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&bundle_path)
            .unwrap_or_else(|e| panic!("could not read {} ({e})", bundle_path.display())),
    )
    .unwrap_or_else(|e| panic!("{} is not JSON ({e})", bundle_path.display()));
    assert_eq!(
        bundle["capability_executions"][0]["capability_id"], "fixture_repair",
        "the published bundle must agree with stdout about what ran: {bundle}"
    );
    assert_eq!(
        bundle["invocation_ref"], INVOCATION_REF,
        "the bundle must be about the invocation that was made: {bundle}"
    );

    let marker = project.change_marker();
    match conclusion {
        Conclusion::Repaired => {
            let key = marker
                .as_deref()
                .unwrap_or_else(|| panic!("a completed run must have written its marker"));
            assert!(
                key.len() == 16 && key.chars().all(|c| c.is_ascii_hexdigit()),
                "the correlation key is 16 hex characters (design §4.3), got {key:?}"
            );
            assert_eq!(
                payload["next_action"], "complete",
                "the marker the run wrote must be the one its own re-derivation \
                 accepts as accounting for the work: {payload}"
            );
        }
        Conclusion::NotRepaired { .. } => {
            assert_eq!(
                marker, None,
                "a repair that did not pass its check earned nothing"
            );
            assert_eq!(
                payload["next_action"]["execute"]["capability_id"], "fixture_repair",
                "an unearned run leaves the work still to do: {payload}"
            );
        }
    }

    assert_eq!(
        project.fixture_status(),
        Vec::<String>::new(),
        "the attempt wrote to the repository it was supposed to branch from"
    );
    assert_eq!(
        dirs_under(&project.workspace_root()),
        Vec::<PathBuf>::new(),
        "the attempt left its worktree behind"
    );

    println!("\n─── tier 1 observation ────────────────────────────────────");
    println!("  model            = {model}");
    println!("  gateway          = {base_url}");
    println!("  latency          = {:.1}s", latency.as_secs_f64());
    println!("  exit code        = {:?}", out.status.code());
    println!("  outcome          = {}", payload["outcome"]);
    println!(
        "  execution        = {}",
        payload["capability_executions"][0]["status"]
    );
    println!(
        "  evidence         = {}",
        payload["capability_executions"][0]["evidence"]
    );
    println!("  next action      = {}", payload["next_action"]);
    match &conclusion {
        Conclusion::Repaired => println!("  repair landed    = yes"),
        Conclusion::NotRepaired { reason } => {
            println!("  repair landed    = no");
            println!("  reason           = {reason}");
        }
    }
    println!("  marker written   = {}", marker.is_some());
    println!("  tool calls       = {tool_calls}");
    for reference in evidence.iter().filter(|e| e.starts_with("tool:")) {
        println!("    {reference}");
    }
    println!("  bundle           = {}", bundle_path.display());
    println!("───────────────────────────────────────────────────────────");
    println!(
        "  `repair landed` is data, never a verdict: `no` is correct behaviour \
         and cannot\n  fail this test. `tool calls` is the opposite — it is \
         protocol, it is asserted,\n  and a zero there means the agent loop is \
         not wired up."
    );
}

enum Conclusion {
    Repaired,
    NotRepaired { reason: String },
}

impl Conclusion {
    fn exit_code(&self) -> i32 {
        match self {
            Conclusion::Repaired => 0,
            Conclusion::NotRepaired { .. } => 11,
        }
    }

    fn read(payload: &serde_json::Value, stderr: &str) -> Self {
        if payload["outcome"] == "completed" {
            return Conclusion::Repaired;
        }
        let Some(reason) = payload["outcome"]["retryable"]["reason"].as_str() else {
            panic!(
                "the run concluded on a row this test cannot interpret. \
                 `Completed` and `Retryable` are the two correct answers; \
                 anything else means the run did not reach the capability at \
                 all: {payload}\nstderr = {stderr}"
            );
        };
        classify(reason);
        Conclusion::NotRepaired {
            reason: reason.to_string(),
        }
    }
}

fn classify(reason: &str) {
    const REACHED_THE_MODEL: [&str; 3] = [
        "the check exited",
        "the attempt produced no report: the model did not hold up its end",
        "the attempt produced no report: the attempt was stopped by a bound",
    ];
    if REACHED_THE_MODEL
        .iter()
        .any(|prefix| reason.starts_with(prefix))
    {
        return;
    }
    panic!(
        "INCONCLUSIVE — the run did not reach a model turn, so it says nothing \
         about whether the agent loop is wired up. This is not the model \
         failing; it is the run failing to happen. Check the gateway, the \
         credential, and the toolchain the check needs.\n  reason = {reason}"
    );
}

fn env_or(name: &str, fallback: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => fallback.to_string(),
    }
}

struct Project {
    dir: tempfile::TempDir,
}

impl Project {
    fn new(model: &str, base_url: &str) -> Self {
        let project = Project {
            dir: tempfile::tempdir().expect("a temporary directory"),
        };

        std::fs::create_dir_all(project.stub_root().join("work")).unwrap();
        std::fs::create_dir_all(project.stub_root().join("changes")).unwrap();
        std::fs::write(
            project.stub_root().join(format!("work/{WORK_ID}.json")),
            format!("{{\"id\":\"{WORK_ID}\",\"status\":\"open\"}}"),
        )
        .unwrap();

        project.write_broken_crate();

        std::fs::write(
            project.config_path(),
            format!(
                "[project]\n\
                 name = \"{PROJECT}\"\n\
                 \n\
                 [stub]\n\
                 root = {stub}\n\
                 \n\
                 [report]\n\
                 dir = {reports}\n\
                 \n\
                 [agent]\n\
                 model = \"{model}\"\n\
                 base_url = \"{base_url}\"\n\
                 api_key = {{ env = \"{CREDENTIAL_VAR}\" }}\n\
                 max_turns = 16\n\
                 max_tokens = 4096\n\
                 deadline = \"5m\"\n\
                 tool_timeout = \"4m\"\n\
                 \n\
                 [workspace]\n\
                 root = {workspaces}\n\
                 fixture = {fixture}\n\
                 check = {{ program = \"cargo\", args = [\"test\", \"--offline\"] }}\n\
                 command_timeout = \"4m\"\n",
                stub = toml_path(&project.stub_root()),
                reports = toml_path(&project.report_dir()),
                workspaces = toml_path(&project.workspace_root()),
                fixture = toml_path(&project.fixture()),
            ),
        )
        .unwrap();

        project
    }

    fn write_broken_crate(&self) {
        let repo = self.fixture();
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(repo.join("tests")).unwrap();
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n",
        )
        .unwrap();
        std::fs::write(repo.join("src/lib.rs"), BROKEN).unwrap();
        std::fs::write(
            repo.join("tests/repair.rs"),
            "#[test]\nfn the_last_index_is_one_before_the_length() {\n    \
             assert_eq!(fixture::last_index(3), 2);\n}\n",
        )
        .unwrap();
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
                "the broken fixture",
            ],
        );
    }

    fn config_path(&self) -> PathBuf {
        self.dir.path().join("fiddle.toml")
    }

    fn stub_root(&self) -> PathBuf {
        self.dir.path().join("stub-state")
    }

    fn report_dir(&self) -> PathBuf {
        self.dir.path().join("reports")
    }

    fn workspace_root(&self) -> PathBuf {
        self.dir.path().join("workspaces")
    }

    fn fixture(&self) -> PathBuf {
        self.dir.path().join("fixture")
    }

    fn change_marker(&self) -> Option<String> {
        let path = self.stub_root().join(format!("changes/{WORK_ID}.json"));
        let text = std::fs::read_to_string(path).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        value["marker"].as_str().map(str::to_string)
    }

    fn fixture_status(&self) -> Vec<String> {
        let out = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(self.fixture())
            .output()
            .expect("could not run git status");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(|line| line[3..].trim().to_string())
            .collect()
    }

    fn files(&self) -> Vec<(String, Vec<u8>)> {
        let root = self.dir.path();
        files_under(root)
            .into_iter()
            .map(|path| {
                let relative = path.strip_prefix(root).unwrap().display().to_string();
                (relative, std::fs::read(&path).unwrap_or_default())
            })
            .collect()
    }
}

fn toml_path(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("could not run git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn dirs_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
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
