mod support;

use std::path::{Path, PathBuf};
use std::process::Output;

use support::{
    accepted, body_of, calls, check_stub_binary, completion, gh_stub_binary, git, git_says,
    object_of, repo_root, reports, toml_string, walkdir_files, wiz_stub_binary, Reply, Scenario,
    StubGateway, CREDENTIAL_VARS,
};

const FEEDBACK_REF: &str = "cve";

const REPO: &str = "acme/r";

const BASE: &str = "main";

const IMAGE: &str = "ghcr.io/acme/icecube:latest";

const FIXTURE: &str = "cve-vulnerable";

const SCAN_LIBRARY_ONLY: &str = "library-only";

const RESCAN_CLEAN: &str = "library-clean";

const RESCAN_CLEARED: &str = "clean-image";

const LIBRARY_CVE: &str = "CVE-2026-0001";

const DECLINED_NOTE: &str = "no published fix I can apply without a registry";

const FIXED_NOTE: &str = "moved the requirement to the release that carries the fix";

const VULNERABLE_VERSION: &str = "v0.31.0";

const FIXED_VERSION: &str = "v0.35.0";

const MANIFEST: &str = "go.mod";

const SHARED_BRANCH: &str = "security/cve-remediation-2026-01-02";

const SHARED_PR: u64 = 41;

const PRIOR_RUN_MARKER: &str = "EARLIER-RUN.md";

const SEEDED_SUBJECT: &str = "an earlier night's work on the shared branch";

const ATTEMPTS_START: &str = "<!-- fiddle-attempts:start -->";

const ATTEMPTS_END: &str = "<!-- fiddle-attempts:end -->";

const SHARED_PROSE: &str = "fiddle attempted 1 advisory for this repository's \
     container image in one bounded attempt and committed nothing.";

const FORGE_TOKEN: &str = "FIDDLE_GITHUB_TOKEN";

const MODEL_KEY: &str = "LITELLM_API_KEY";

const WIZ_ID: &str = "WIZ_CLIENT_ID";

const WIZ_SECRET: &str = "WIZ_CLIENT_SECRET";

const SENTINEL_SECRET: &str = "fiddle-secret-3b8e51d0";

const VERIFY_CHECK: &str = "cve-verify";

const RESCAN_CHECK: &str = "cve-rescan";

fn body_counting(attempts: u32) -> String {
    format!("{SHARED_PROSE}\n\n{ATTEMPTS_START}\nAttempts: {attempts}\n{ATTEMPTS_END}")
}

struct Feedback {
    scenario: Scenario,
    stub: PathBuf,
    remote: PathBuf,
    tree: PathBuf,
    gateway: StubGateway,
    login: tempfile::TempDir,
}

impl Feedback {
    fn bounded_at(bound: usize, script: Vec<Reply>) -> Self {
        Feedback::rescanning(bound, RESCAN_CLEAN, script)
    }

    fn rescanning(bound: usize, rescan: &str, script: Vec<Reply>) -> Self {
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
            login: support::caller_logged_in(),
        };
        let tables = feedback.tables(bound, rescan);
        feedback.scenario.append_config(&tables);
        feedback
    }

    fn tables(&self, bound: usize, rescan: &str) -> String {
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
             args = [\"{rescan}\"]\n\
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

    fn seed_shared_pull_request(&self, body: &str) -> String {
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
                SEEDED_SUBJECT,
            ],
        );
        git(&self.tree, &["push", "-q", "origin", SHARED_BRANCH]);
        let head_sha = git_says(&self.tree, &["rev-parse", "HEAD"]);
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
        head_sha
    }

    fn seed_checks(&self, head_sha: &str, concluding: &[(&str, &str)]) {
        let runs: Vec<serde_json::Value> = concluding
            .iter()
            .map(|(name, conclusion)| {
                serde_json::json!({
                    "name": name,
                    "status": "completed",
                    "conclusion": conclusion,
                    "head_sha": head_sha,
                    "details_url": format!("https://github.com/{REPO}/runs/{name}"),
                })
            })
            .collect();
        std::fs::write(
            self.stub.join("checks_seed"),
            serde_json::Value::Array(runs).to_string(),
        )
        .unwrap();
    }

    fn refuse_the_check_read(&self, status: u16) {
        std::fs::write(self.stub.join("checks_unreadable"), status.to_string()).unwrap();
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
            .env(support::WIZ_CONFIG_DIR, self.login.path())
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

    fn remote_head(&self) -> String {
        git_says(&self.remote, &["rev-parse", SHARED_BRANCH])
    }

    fn pushed_subjects(&self) -> Vec<String> {
        git_says(
            &self.remote,
            &["log", "--format=%s", &format!("{BASE}..{SHARED_BRANCH}")],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    fn briefed(&self) -> String {
        self.gateway.request_bodies().join("\n")
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

fn an_attempt_moving_the_requirement() -> Vec<Reply> {
    let fixed = std::fs::read_to_string(
        repo_root()
            .join("tests/fixtures")
            .join(FIXTURE)
            .join(MANIFEST),
    )
    .unwrap()
    .replace(VULNERABLE_VERSION, FIXED_VERSION);
    vec![
        accepted(calls(
            "write_file",
            serde_json::json!({ "path": MANIFEST, "contents": fixed }),
        )),
        accepted(reports(serde_json::json!({
            "changed_files": [MANIFEST],
            "summary": FIXED_NOTE,
            "claimed_complete": true,
            "findings": [{
                "cve": LIBRARY_CVE,
                "attempted": true,
                "note": FIXED_NOTE,
            }],
        }))),
    ]
}

#[test]
fn a_pull_request_at_the_bound_is_left_for_a_human() {
    let feedback = Feedback::bounded_at(2, an_attempt_declining());
    let head_sha = feedback.seed_shared_pull_request(&body_counting(2));
    feedback.seed_checks(&head_sha, &[(VERIFY_CHECK, "failure")]);

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
        reached["attempt_bound"],
        serde_json::json!({ "spent": 2, "bound": 2 }),
        "the row name alone cannot tell 2 of 2 from 5 of 5, so the two numbers \
         a person needs to raise the bound are published: {reached}"
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
    let head_sha = feedback.seed_shared_pull_request(&body_counting(2));
    feedback.seed_checks(&head_sha, &[(VERIFY_CHECK, "failure")]);

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

    let shared = feedback.pull_request(SHARED_PR);
    assert_eq!(
        shared["body"].as_str(),
        Some(body_counting(3).as_str()),
        "the run above left the count at two and this one raised it to three, \
         and the two runs differ only in the configured bound: {shared}"
    );

    let briefed = feedback.briefed();
    assert!(
        briefed.contains(VERIFY_CHECK),
        "the one check that blamed the candidate has to reach the prompt by \
         name: {briefed}"
    );
    assert!(
        !briefed.contains(RESCAN_CHECK),
        "and no check the forge did not report may reach it, or the prompt \
         carries a sentence this build wrote and not what CI said: {briefed}"
    );
}

#[test]
fn a_count_that_cannot_be_read_makes_no_attempt() {
    let feedback = Feedback::bounded_at(2, an_attempt_declining());
    let edited = format!("{SHARED_PROSE}\n\n{ATTEMPTS_START}\nAttempts: two\n{ATTEMPTS_END}");
    let head_sha = feedback.seed_shared_pull_request(&edited);
    feedback.seed_checks(&head_sha, &[(VERIFY_CHECK, "failure")]);

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

#[test]
fn two_checks_blaming_the_candidate_are_one_fresh_attempt() {
    let feedback = Feedback::bounded_at(3, an_attempt_declining());
    let head_sha = feedback.seed_shared_pull_request(&body_counting(0));
    feedback.seed_checks(
        &head_sha,
        &[(VERIFY_CHECK, "failure"), (RESCAN_CHECK, "failure")],
    );

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
        "CI blamed the candidate twice and a run makes one fresh attempt, not \
         one per check that failed"
    );
    assert_eq!(
        feedback.open_pull_requests().len(),
        1,
        "and it worked on the pull request the checks blamed"
    );

    let briefed = feedback.briefed();
    for named in [VERIFY_CHECK, RESCAN_CHECK] {
        assert!(
            briefed.contains(named),
            "the run seeded two failing checks and the prompt has to name both, \
             because the run above seeded one and named one: {briefed}"
        );
        assert!(
            briefed.contains(&format!("https://github.com/{REPO}/runs/{named}")),
            "and carry the link the forge published for {named}, so a person \
             reading the prompt can read the log CI wrote: {briefed}"
        );
    }
    assert!(
        briefed.contains(&head_sha),
        "and name the commit the checks ran against: {briefed}"
    );
}

#[test]
fn two_checks_blaming_nothing_are_no_fresh_attempt() {
    let feedback = Feedback::bounded_at(3, an_attempt_declining());
    let head_sha = feedback.seed_shared_pull_request(&body_counting(0));
    feedback.seed_checks(
        &head_sha,
        &[(VERIFY_CHECK, "success"), (RESCAN_CHECK, "success")],
    );

    let run = feedback.run();
    let payload = feedback.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "a pull request nothing blames is a stopping place and not a failure\n\
         stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert_eq!(
        feedback.gateway.served(),
        0,
        "this run and the one above differ only in what the two checks \
         concluded. Nothing here blames the candidate sha, so this run calls \
         no model"
    );
    assert_eq!(
        feedback.mutations(),
        Vec::<String>::new(),
        "and it left the pull request alone"
    );
    assert_eq!(
        feedback.remote_branches(),
        vec![BASE.to_string(), SHARED_BRANCH.to_string()],
        "and pushed nothing"
    );
    assert_ne!(
        feedback.disposition(&run)["reason"],
        "checks_unreadable",
        "and it read the checks, so it must not report that it could not"
    );
}

#[test]
fn a_refused_check_read_is_reported_rather_than_read_as_no_blame() {
    let feedback = Feedback::bounded_at(3, an_attempt_declining());
    let head_sha = feedback.seed_shared_pull_request(&body_counting(0));
    feedback.seed_checks(
        &head_sha,
        &[(VERIFY_CHECK, "success"), (RESCAN_CHECK, "success")],
    );
    feedback.refuse_the_check_read(403);

    let run = feedback.run();
    let payload = feedback.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(11),
        "this run and the one above differ only in whether the check read was \
         answered. A run that could not look has not finished cleanly, and an \
         operator who grants Checks: read and runs it again gets further\n\
         stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );

    let disposition = feedback.disposition(&run);
    assert_eq!(
        disposition["reason"], "checks_unreadable",
        "and it says which of the two it was, rather than publishing the row a \
         clean sweep publishes: {disposition}"
    );

    assert_eq!(
        feedback.gateway.served(),
        0,
        "not attempting is still the safe direction: a run that cannot read \
         what CI said must not brief a model on a guess"
    );
    assert_eq!(
        feedback.mutations(),
        Vec::<String>::new(),
        "and it left the pull request alone"
    );
    assert_eq!(
        feedback.remote_branches(),
        vec![BASE.to_string(), SHARED_BRANCH.to_string()],
        "and pushed nothing"
    );

    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        said.contains("check-runs") && said.contains(&head_sha),
        "and it names the read it was refused and the commit it asked about, \
         because a 403 here is a missing permission and nothing else in the \
         run says so: {said}"
    );
}

#[test]
fn an_attempt_that_commits_raises_the_count() {
    let feedback = Feedback::rescanning(3, RESCAN_CLEARED, an_attempt_moving_the_requirement());
    let head_sha = feedback.seed_shared_pull_request(&body_counting(2));
    feedback.seed_checks(&head_sha, &[(VERIFY_CHECK, "failure")]);

    let run = feedback.run();
    let payload = feedback.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );

    let reached = feedback.disposition(&run);
    assert_eq!(
        reached["reason"], "pull_request",
        "the rescan cleared this attempt, so it committed what it changed: \
         {reached}"
    );
    assert_ne!(
        feedback.remote_head(),
        head_sha,
        "and the commit log carries it"
    );
    assert_eq!(
        feedback.pushed_subjects(),
        vec![
            format!("fix: mitigate 1 advisory"),
            SEEDED_SUBJECT.to_string(),
        ],
        "one attempt is one commit, above the one the seed made: {:?}",
        feedback.pushed_subjects()
    );

    let shared = feedback.pull_request(SHARED_PR);
    let body = shared["body"].as_str().unwrap_or_default();
    assert!(
        body.contains("Attempts: 3"),
        "and the body it published counts this attempt: {shared}"
    );
    assert!(
        body.contains("committed what it changed"),
        "beside the prose for a run that landed work: {shared}"
    );
}

#[test]
fn an_attempt_that_reverts_raises_the_count_the_commit_log_cannot_show() {
    let feedback = Feedback::rescanning(3, SCAN_LIBRARY_ONLY, an_attempt_moving_the_requirement());
    let head_sha = feedback.seed_shared_pull_request(&body_counting(2));
    feedback.seed_checks(&head_sha, &[(VERIFY_CHECK, "failure")]);

    let run = feedback.run();
    let payload = feedback.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );

    let reached = feedback.disposition(&run);
    assert_ne!(
        reached["reason"], "pull_request",
        "this run and the one above differ only in what the rescan reported. \
         The rescan still names the advisory here, so the attempt reverted: \
         {reached}"
    );
    assert_eq!(
        feedback.remote_head(),
        head_sha,
        "a reverted attempt pushes nothing, so the commit log cannot show it"
    );
    assert_eq!(
        feedback.pushed_subjects(),
        vec![SEEDED_SUBJECT.to_string()],
        "only the commit the seed made is on the branch: {:?}",
        feedback.pushed_subjects()
    );

    let shared = feedback.pull_request(SHARED_PR);
    assert_eq!(
        shared["body"].as_str(),
        Some(body_counting(3).as_str()),
        "and the count still rose, which is the case the bound exists for: \
         {shared}"
    );
}
