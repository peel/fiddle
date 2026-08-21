mod support;

use std::path::{Path, PathBuf};
use std::process::Output;

use support::{
    accepted, body_of, check_stub_binary, completion, gh_stub_binary, git, object_of, repo_root,
    toml_string, walkdir_files, wiz_stub_binary, Reply, Scenario, StubGateway, CREDENTIAL_VARS,
};

const FEEDBACK_REF: &str = "cve";

const REPO: &str = "acme/r";

const BASE: &str = "main";

const IMAGE: &str = "ghcr.io/acme/icecube:latest";

const FIXTURE: &str = "cve-vulnerable";

const SCAN_LIBRARY_ONLY: &str = "library-only";

const RESCAN_CLEAN: &str = "library-clean";

const LIBRARY_CVE: &str = "CVE-2026-0001";

const DECLINED_NOTE: &str = "no published fix I can apply without a registry";

const SHARED_BRANCH: &str = "security/cve-remediation-2026-01-02";

const SHARED_PR: u64 = 41;

const PRIOR_RUN_MARKER: &str = "EARLIER-RUN.md";

const ATTEMPTS_START: &str = "<!-- fiddle-attempts:start -->";

const ATTEMPTS_END: &str = "<!-- fiddle-attempts:end -->";

const SHARED_PROSE: &str = "fiddle attempted 1 advisory for this repository's \
     container image in one bounded attempt and committed nothing.";

const FORGE_TOKEN: &str = "FIDDLE_GITHUB_TOKEN";

const MODEL_KEY: &str = "LITELLM_API_KEY";

const WIZ_ID: &str = "WIZ_CLIENT_ID";

const WIZ_SECRET: &str = "WIZ_CLIENT_SECRET";

const SENTINEL_SECRET: &str = "fiddle-secret-3b8e51d0";

fn body_counting(attempts: u32) -> String {
    format!("{SHARED_PROSE}\n\n{ATTEMPTS_START}\nAttempts: {attempts}\n{ATTEMPTS_END}")
}

struct Feedback {
    scenario: Scenario,
    stub: PathBuf,
    remote: PathBuf,
    tree: PathBuf,
    gateway: StubGateway,
}

impl Feedback {
    fn bounded_at(bound: usize, script: Vec<Reply>) -> Self {
        let scenario = Scenario::new();

        let stub = scenario.dir().join("gh-stub");
        std::fs::create_dir_all(stub.join("script")).unwrap();
        std::fs::create_dir_all(stub.join("config")).unwrap();
        let remote = stub.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare", "-b", BASE, "."]);

        let feedback = Feedback {
            tree: seed_repository(scenario.dir(), &remote),
            scenario,
            stub,
            remote,
            gateway: StubGateway::serving(script),
        };
        let tables = feedback.tables(bound);
        feedback.scenario.append_config(&tables);
        feedback
    }

    fn tables(&self, bound: usize) -> String {
        format!(
            "[github]\n\
             repo = \"{REPO}\"\n\
             base = \"{BASE}\"\n\
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
             max_capability_attempts = {bound}\n\
             deadline = \"300s\"\n\
             tool_timeout = \"300s\"\n\
             \n\
             [scanner]\n\
             cli = {{ program = {wiz}, args = [\"{SCAN_LIBRARY_ONLY}\"] }}\n\
             client_id = {{ env = \"{WIZ_ID}\" }}\n\
             client_secret = {{ env = \"{WIZ_SECRET}\" }}\n\
             timeout = \"300s\"\n\
             \n\
             [orchestration.cve]\n\
             image = \"{IMAGE}\"\n\
             max_findings = 1\n\
             \n\
             [workspace]\n\
             root = {workspaces}\n\
             fixture = {tree}\n\
             command_timeout = \"300s\"\n\
             \n\
             [[workspace.checks]]\n\
             program = {check}\n\
             args = []\n\
             success = \"exit-zero\"\n\
             \n\
             [[workspace.checks]]\n\
             program = {wiz}\n\
             args = [\"{RESCAN_CLEAN}\"]\n\
             success = \"artefact-written\"\n",
            gh = toml_string(gh_stub_binary()),
            wiz = toml_string(wiz_stub_binary()),
            check = toml_string(check_stub_binary()),
            stub = toml_string(&self.stub),
            config_dir = toml_string(&self.stub.join("config")),
            base_url = self.gateway.base_url(),
            workspaces = toml_string(&self.scenario.dir().join("workspaces")),
            tree = toml_string(&self.tree),
        )
    }

    fn seed_shared_pull_request(&self, body: &str) {
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
                "an earlier night's work on the shared branch",
            ],
        );
        git(&self.tree, &["push", "-q", "origin", SHARED_BRANCH]);
        git(&self.tree, &["checkout", "-q", BASE]);

        let owner = REPO.split('/').next().unwrap();
        std::fs::write(
            self.stub.join("pulls_seed"),
            serde_json::json!([{
                "number": SHARED_PR,
                "state": "open",
                "head": format!("{owner}:{SHARED_BRANCH}"),
                "base": BASE,
                "title": "acme: dependency advisories",
                "body": body,
                "labels": ["security/cve"],
            }])
            .to_string(),
        )
        .unwrap();
    }

    fn run(&self) -> Output {
        let mut command = std::process::Command::new(support::fiddle_binary());
        for name in CREDENTIAL_VARS
            .iter()
            .chain([FORGE_TOKEN, MODEL_KEY, WIZ_ID, WIZ_SECRET].iter())
        {
            command.env_remove(name);
        }
        command
            .args(["run", FEEDBACK_REF])
            .args(["--capability", "cve_mitigate"])
            .args(["--config", self.scenario.config_path().to_str().unwrap()])
            .arg("--json")
            .env(FORGE_TOKEN, "ghp_forge_token_for_the_sweep")
            .env(MODEL_KEY, "sk-model-key-for-the-sweep")
            .env(WIZ_ID, "wiz-client-id-for-the-sweep")
            .env(WIZ_SECRET, SENTINEL_SECRET)
            .output()
            .unwrap()
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

    fn disposition(&self, run: &Output) -> serde_json::Value {
        let bundle = self.scenario.read_bundle(&self.payload(run));
        bundle
            .get("disposition")
            .cloned()
            .unwrap_or_else(|| panic!("this run published no disposition at all: {bundle}"))
    }

    fn pull_request(&self, number: u64) -> serde_json::Value {
        let out = std::process::Command::new(gh_stub_binary())
            .args(["--stub-dir", self.stub.to_str().unwrap()])
            .args([
                "api",
                "--method",
                "GET",
                &format!("/repos/{REPO}/pulls/{number}"),
            ])
            .output()
            .unwrap();
        object_of(&String::from_utf8_lossy(&out.stdout))
            .unwrap_or_else(|| panic!("the forge holds no pull request #{number}"))
    }

    fn open_pull_requests(&self) -> Vec<serde_json::Value> {
        let out = std::process::Command::new(gh_stub_binary())
            .args(["--stub-dir", self.stub.to_str().unwrap()])
            .args([
                "api",
                "--method",
                "GET",
                &format!("/repos/{REPO}/pulls?state=open"),
            ])
            .output()
            .unwrap();
        body_of(&String::from_utf8_lossy(&out.stdout))
    }

    fn mutations(&self) -> Vec<String> {
        std::fs::read_to_string(self.stub.join("world"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter_map(|landed| landed["key"].as_str().map(str::to_string))
            .collect()
    }

    fn remote_branches(&self) -> Vec<String> {
        let out = std::process::Command::new("git")
            .current_dir(&self.remote)
            .args(["for-each-ref", "--format=%(refname:short)", "refs/heads/"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_string)
            .collect()
    }
}

fn seed_repository(root: &Path, remote: &Path) -> PathBuf {
    let fixture = repo_root().join("tests/fixtures").join(FIXTURE);
    let tree = root.join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    for path in walkdir_files(&fixture) {
        let relative = path.strip_prefix(&fixture).unwrap();
        let destination = tree.join(relative);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::copy(&path, &destination).unwrap();
    }
    git(&tree, &["init", "-q", "-b", BASE, "."]);
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
    git(&tree, &["push", "-q", "origin", BASE]);
    tree
}

fn an_attempt_declining() -> Vec<Reply> {
    vec![accepted(completion(
        serde_json::json!({
            "role": "assistant",
            "content": serde_json::json!({
                "changed_files": [],
                "summary": DECLINED_NOTE,
                "claimed_complete": false,
                "findings": [{
                    "cve": LIBRARY_CVE,
                    "attempted": false,
                    "note": DECLINED_NOTE,
                }],
            }).to_string(),
        }),
        "stop",
    ))]
}

#[test]
fn a_pull_request_at_the_bound_is_left_for_a_human() {
    let feedback = Feedback::bounded_at(2, an_attempt_declining());
    feedback.seed_shared_pull_request(&body_counting(2));

    let run = feedback.run();
    let payload = feedback.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "the bound is a stopping place and not a failure\nstderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    assert_eq!(
        feedback.gateway.served(),
        0,
        "a run at the bound calls no model, so it made no attempt"
    );

    let reached = feedback.disposition(&run);
    assert_eq!(
        reached["reason"], "attempt_bound_reached",
        "the disposition has to say why this run did nothing: {reached}"
    );
    assert_eq!(
        reached["pull_request"], SHARED_PR,
        "and which pull request a person should look at: {reached}"
    );

    assert_eq!(
        feedback.mutations(),
        Vec::<String>::new(),
        "nothing was closed, and the body was not rewritten"
    );
    let shared = feedback.pull_request(SHARED_PR);
    assert_eq!(shared["state"], "open", "the pull request is left open");
    assert_eq!(
        shared["body"].as_str(),
        Some(body_counting(2).as_str()),
        "and its count is left exactly as it was found: {shared}"
    );
    assert_eq!(
        feedback.open_pull_requests().len(),
        1,
        "and no second pull request was opened"
    );
    assert_eq!(
        feedback.remote_branches(),
        vec![BASE.to_string(), SHARED_BRANCH.to_string()],
        "and no new branch was pushed, so nothing was reverted on one either"
    );
}

#[test]
fn a_pull_request_below_the_bound_is_attempted() {
    let feedback = Feedback::bounded_at(3, an_attempt_declining());
    feedback.seed_shared_pull_request(&body_counting(2));

    let run = feedback.run();
    let payload = feedback.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert_eq!(
        feedback.gateway.served(),
        1,
        "the same body under a bound of three is one attempt short of it, so \
         this run attempts, and the configured number is what the two runs \
         differ by"
    );
    let reached = feedback.disposition(&run);
    assert_ne!(
        reached["reason"], "attempt_bound_reached",
        "a run that attempted did not stop at the bound: {reached}"
    );
}

#[test]
fn a_count_that_cannot_be_read_makes_no_attempt() {
    let feedback = Feedback::bounded_at(2, an_attempt_declining());
    let edited = format!("{SHARED_PROSE}\n\n{ATTEMPTS_START}\nAttempts: two\n{ATTEMPTS_END}");
    feedback.seed_shared_pull_request(&edited);

    let run = feedback.run();
    assert_ne!(
        run.status.code(),
        Some(0),
        "a run that cannot read the count carries no bound, and attempting \
         without one is worse than not attempting\nstdout: {}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert_eq!(feedback.gateway.served(), 0, "so it called no model at all");
    assert_eq!(
        feedback.mutations(),
        Vec::<String>::new(),
        "and left the body a person edited alone"
    );
    let refused = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        refused.contains("body") && refused.contains("two"),
        "the refusal names the body it could not read: {refused}"
    );
}
