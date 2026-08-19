mod support;

use std::path::{Path, PathBuf};
use support::Scenario;

const WORK_ID: &str = "fiddle-m0-demo";
const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

const CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";

const SENTINEL: &str = "ghp_m2_sentinel_must_never_be_printed_4b71";

const REPO: &str = "peel/r";
const WORKFLOW: &str = "verify.yml";
const BASE: &str = "main";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Object {
    Branch,
    PullRequest,
    Check,
}

impl Object {
    const ALL: [Object; 3] = [Object::Branch, Object::PullRequest, Object::Check];

    fn as_str(self) -> &'static str {
        match self {
            Object::Branch => "branch",
            Object::PullRequest => "pull_request",
            Object::Check => "check",
        }
    }
}

struct ScriptedWorld {
    scenario: Scenario,
    stub: PathBuf,
    remote: PathBuf,
    work: PathBuf,
}

impl ScriptedWorld {
    fn new() -> Self {
        let scenario = Scenario::new();
        scenario.write_work_item(WORK_ID, "open");
        let work = scenario.write_fixture_repo();

        let stub = scenario.dir().join("gh-stub");
        std::fs::create_dir_all(stub.join("script")).unwrap();
        std::fs::create_dir_all(stub.join("config")).unwrap();

        let remote = stub.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare", "."]);
        git(
            &work,
            &["remote", "add", "origin", &remote.display().to_string()],
        );

        let world = ScriptedWorld {
            scenario,
            stub,
            remote,
            work,
        };
        world.scenario.append_config(&world.forge_table());
        world
    }

    fn forge_table(&self) -> String {
        format!(
            "[github]\n\
             repo = \"{REPO}\"\n\
             base = \"{BASE}\"\n\
             token = {{ env = \"{CREDENTIAL}\" }}\n\
             cli = {{ program = {gh}, args = [\"--stub-dir\", {stub}] }}\n\
             git = {git}\n\
             work = {work}\n\
             workflow = \"{WORKFLOW}\"\n\
             config_dir = {config_dir}\n\
             timeout = \"120s\"\n",
            gh = toml_path(support::gh_stub_binary()),
            stub = toml_path(&self.stub),
            git = toml_path(support::git_stub_binary()),
            work = toml_path(&self.work),
            config_dir = toml_path(&self.stub.join("config")),
        )
    }

    fn make_ambiguous_by_cancellation(&self, object: Object) {
        match object {
            Object::Branch => self.push_mode("push_then_waits"),
            Object::PullRequest => {
                self.push_mode("delegated");
                self.script(&pulls_key(), "201 0 commit_then_wait");
            }
            Object::Check => {
                self.push_mode("delegated");
                self.script(&dispatch_key(), "204 0 commit_then_wait");
            }
        }
    }

    fn landing_marker(&self, object: Object) -> PathBuf {
        match object {
            Object::Branch => self.work.join("pushed_then_waited"),
            Object::PullRequest | Object::Check => self.stub.join("landed_and_waiting"),
        }
    }

    fn recover_from_cancellation(&self, object: Object) {
        let marker = self.landing_marker(object);
        if marker.exists() {
            std::fs::remove_file(&marker).unwrap();
        }
        match object {
            Object::Branch => self.push_mode("delegated"),
            Object::PullRequest => self.script(&pulls_key(), "201 0 normal"),
            Object::Check => self.script(&dispatch_key(), "204 0 normal"),
        }
    }

    fn publish_then_interrupt(&self, object: Object) -> std::process::Output {
        let marker = self.landing_marker(object);
        let child = self
            .scenario
            .spawnable_run_command(INVOCATION_REF)
            .args(["--capability", "publish_change", "--json"])
            .env(CREDENTIAL, SENTINEL)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "{}: the fixture never recorded a landed mutation, so there was \
                 nothing to make ambiguous",
                object.as_str()
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        interrupt(child.id());
        child.wait_with_output().unwrap()
    }

    fn make_ambiguous(&self, object: Object) {
        match object {
            Object::Branch => self.push_mode("push_then_killed"),
            Object::PullRequest => {
                self.push_mode("delegated");
                self.script(&pulls_key(), "201 0 commit_then_die");
            }
            Object::Check => {
                self.push_mode("delegated");
                self.script(&dispatch_key(), "204 0 commit_then_die");
            }
        }
    }

    fn interrupt_after(&self, object: Object) {
        match object {
            Object::Branch => self.script(&pulls_key(), "500 1 normal"),
            Object::PullRequest => self.script(&dispatch_key(), "500 1 normal"),
            Object::Check => self.scenario.make_changes_dir_unwritable(),
        }
    }

    fn recover_from(&self, object: Object) {
        match object {
            Object::Branch => self.script(&pulls_key(), "201 0 normal"),
            Object::PullRequest => self.script(&dispatch_key(), "204 0 normal"),
            Object::Check => self.scenario.make_changes_dir_writable(),
        }
    }

    fn make_the_settling_read_fail(&self, status: u16) {
        std::fs::write(
            self.stub.join("runs_unreadable_after_a_dispatch"),
            status.to_string(),
        )
        .unwrap();
    }

    fn let_the_settling_read_succeed(&self) {
        let marker = self.stub.join("runs_unreadable_after_a_dispatch");
        if marker.exists() {
            std::fs::remove_file(marker).unwrap();
        }
    }

    fn script(&self, key: &str, spec: &str) {
        std::fs::write(self.stub.join("script").join(key), spec).unwrap();
    }

    fn push_mode(&self, mode: &str) {
        std::fs::write(self.work.join("mode"), mode).unwrap();
    }

    fn use_gh(&self, gh: &Path) {
        let before = self.scenario.config_text();
        let after = before.replace(
            &format!("program = {}", toml_path(support::gh_stub_binary())),
            &format!("program = {}", toml_path(gh)),
        );
        assert_ne!(
            before, after,
            "the document must name the scripted `gh` for it to be replaced"
        );
        std::fs::write(self.scenario.config_path(), after).unwrap();
    }

    fn gh_requests(&self) -> Vec<serde_json::Value> {
        let mut paths = support::walkdir_files(self.stub.join("requests"));
        paths.sort();
        paths
            .iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .filter_map(|text| serde_json::from_str(&text).ok())
            .collect()
    }

    fn publish(&self) -> std::process::Output {
        self.publish_with_token(SENTINEL)
    }

    fn publish_with_token(&self, token: &str) -> std::process::Output {
        self.scenario
            .run_command(INVOCATION_REF)
            .args(["--capability", "publish_change", "--json"])
            .env(CREDENTIAL, token)
            .output()
            .unwrap()
    }

    fn branches(&self) -> Vec<String> {
        git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    fn landed(&self, needle: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.stub.join("world"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|entry| {
                entry["key"]
                    .as_str()
                    .is_some_and(|key| key.starts_with("POST") && key.contains(needle))
            })
            .collect()
    }

    fn pull_requests(&self) -> Vec<serde_json::Value> {
        self.landed("pulls")
    }

    fn workflow_runs(&self) -> Vec<serde_json::Value> {
        self.landed("dispatches")
    }

    fn pushes(&self) -> usize {
        std::fs::read_to_string(self.work.join("pushes"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    fn push_died_after_landing(&self) -> bool {
        self.work.join("pushed_then_died").exists()
    }

    fn forget_the_push_death(&self) {
        let marker = self.work.join("pushed_then_died");
        if marker.exists() {
            std::fs::remove_file(marker).unwrap();
        }
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
        || path == "fixture/push.json"
        || path == "fixture/rev-parse.json"
}

impl Drop for ScriptedWorld {
    fn drop(&mut self) {
        if self.scenario.stub_root().join("changes").exists() {
            self.scenario.make_changes_dir_writable();
        }
        if self.scenario.report_dir().exists() {
            self.scenario.make_report_dir_writable();
        }
    }
}

fn payload_of(out: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {stdout}\nstderr = {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn evidence_of(payload: &serde_json::Value) -> Vec<String> {
    payload["progress"][0]["evidence"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn effect_identity(payload: &serde_json::Value, kind: &str) -> Option<String> {
    evidence_of(payload)
        .into_iter()
        .find(|reference| reference.starts_with(&format!("effect:{kind}:")))
        .map(|reference| {
            reference
                .splitn(4, ':')
                .take(3)
                .collect::<Vec<_>>()
                .join(":")
        })
}

fn summary_of(payload: &serde_json::Value) -> String {
    payload["progress"][0]["summary"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn an_ambiguous_write_then_a_fresh_process_leaves_exactly_one_of_each() {
    for object in Object::ALL {
        let at = object.as_str();
        let world = ScriptedWorld::new();
        world.make_ambiguous(object);
        world.interrupt_after(object);

        let first = world.publish();
        let first_payload = payload_of(&first);
        assert_ne!(
            first.status.code(),
            Some(0),
            "{at}: the interrupted attempt must not claim success: {first_payload}"
        );
        assert!(
            world.scenario.read_change_marker(WORK_ID).is_none(),
            "{at}: the interrupted attempt must not have accounted for the work"
        );

        assert_the_answer_was_lost(&world, object);

        let first_branch = effect_identity(&first_payload, "ensure_branch_published")
            .unwrap_or_else(|| {
                panic!("{at}: the branch receipt must reach the bundle: {first_payload}")
            });

        world.scenario.remove_local_records();
        world.forget_the_push_death();
        world.recover_from(object);

        let second = world.publish();
        let second_payload = payload_of(&second);
        assert_eq!(
            second.status.code(),
            Some(0),
            "{at}: the retry must complete: {second_payload}\nstderr = {}",
            String::from_utf8_lossy(&second.stderr)
        );

        let branches = world.branches();
        assert_eq!(
            branches.len(),
            1,
            "{at}: exactly one branch, got {branches:?}"
        );
        assert!(
            branches[0].starts_with("fiddle/"),
            "{at}: the branch is the one fiddle names, got {branches:?}"
        );
        assert_eq!(
            world.pull_requests().len(),
            1,
            "{at}: exactly one pull request, got {:?}",
            world.pull_requests()
        );
        assert_eq!(
            world.workflow_runs().len(),
            1,
            "{at}: exactly one requested check, got {:?}",
            world.workflow_runs()
        );

        assert_eq!(
            world.pushes(),
            1,
            "{at}: exactly one push was ever dispatched"
        );
        assert!(
            !world.push_died_after_landing(),
            "{at}: the retry must not have pushed at all — the branch was already there"
        );

        assert_eq!(
            effect_identity(&second_payload, "ensure_branch_published").as_deref(),
            Some(first_branch.as_str()),
            "{at}: the retry must derive the identity the first attempt derived, \
             with nothing local left to read it from: {second_payload}"
        );
    }
}

fn assert_the_answer_was_lost(world: &ScriptedWorld, object: Object) {
    let at = object.as_str();
    match object {
        Object::Branch => {
            assert!(
                world.push_died_after_landing(),
                "{at}: the recording `git` must have pushed the ref and *then* died"
            );
            assert_eq!(
                world.branches().len(),
                1,
                "{at}: and the ref must really be on the remote, or nothing was lost"
            );
            assert_eq!(
                world.pushes(),
                1,
                "{at}: the lost answer must be resolved by looking, never by \
                 re-dispatching the push"
            );
        }
        Object::PullRequest => assert_landed_under(world, "pulls", "commit_then_die"),
        Object::Check => assert_landed_under(world, "dispatches", "commit_then_die"),
    }
}

#[test]
fn a_cancellation_after_the_write_lands_is_unresolved_and_never_duplicated() {
    for object in Object::ALL {
        let at = object.as_str();
        let world = ScriptedWorld::new();
        world.make_ambiguous_by_cancellation(object);

        let first = world.publish_then_interrupt(object);
        let first_payload = payload_of(&first);
        let first_stderr = String::from_utf8_lossy(&first.stderr).to_string();
        assert_ne!(
            first.status.code(),
            Some(0),
            "{at}: an interrupted attempt must not claim success: {first_payload}"
        );

        assert!(
            first_stderr.contains("interrupted; stopping the attempt"),
            "{at}: the interrupt must have reached the binary's own handler, or the \
             deadline ended this request and no cancellation was classified: \
             {first_stderr}"
        );
        assert_the_answer_was_lost_to_a_cancellation(&world, object);

        let summary = summary_of(&first_payload);
        assert!(
            summary.contains("unresolved outcome"),
            "{at}: a write whose answer was lost to a cancellation must be reported \
             unresolved, never as a settled failure: {summary:?} in {first_payload}"
        );
        assert!(
            summary.contains("cancelled after"),
            "{at}: the unresolved write must be the one cancelled *after* it was \
             started, not a deadline and not the pre-spawn refusal that followed \
             it: {summary:?}"
        );

        world.scenario.remove_local_records();
        world.recover_from_cancellation(object);

        let second = world.publish();
        let second_payload = payload_of(&second);
        assert_eq!(
            second.status.code(),
            Some(0),
            "{at}: the retry must complete: {second_payload}\nstderr = {}",
            String::from_utf8_lossy(&second.stderr)
        );

        let branches = world.branches();
        assert_eq!(
            branches.len(),
            1,
            "{at}: exactly one branch, got {branches:?}"
        );
        assert_eq!(
            world.pull_requests().len(),
            1,
            "{at}: exactly one pull request, got {:?}",
            world.pull_requests()
        );
        assert_eq!(
            world.workflow_runs().len(),
            1,
            "{at}: exactly one requested check, got {:?}",
            world.workflow_runs()
        );
        assert_eq!(
            world.pushes(),
            1,
            "{at}: exactly one push was ever dispatched"
        );
    }
}

#[test]
fn a_killed_child_whose_settling_read_fails_is_unresolved_and_never_duplicated() {
    let world = ScriptedWorld::new();
    world.push_mode("delegated");
    world.script(&dispatch_key(), "204 0 commit_then_die");
    world.make_the_settling_read_fail(500);

    let first = world.publish();
    let first_payload = payload_of(&first);
    assert_ne!(
        first.status.code(),
        Some(0),
        "an unsettled write must not be reported as success: {first_payload}"
    );

    assert_landed_under(&world, "dispatches", "commit_then_die");

    let summary = summary_of(&first_payload);
    assert!(
        summary.contains("unresolved outcome"),
        "a write whose answer was lost and whose postcondition could not be read \
         must be reported unresolved, never as a settled failure: {summary:?} in \
         {first_payload}"
    );
    assert!(
        summary.contains("(gh was killed"),
        "the unresolved write must be named as the killed child it was: {summary:?}"
    );

    world.scenario.remove_local_records();
    world.script(&dispatch_key(), "204 0 normal");
    world.let_the_settling_read_succeed();

    let second = world.publish();
    let second_payload = payload_of(&second);
    assert_eq!(
        second.status.code(),
        Some(0),
        "the retry must complete: {second_payload}\nstderr = {}",
        String::from_utf8_lossy(&second.stderr)
    );

    assert_eq!(world.branches().len(), 1, "exactly one branch");
    assert_eq!(
        world.pull_requests().len(),
        1,
        "exactly one pull request, got {:?}",
        world.pull_requests()
    );
    assert_eq!(
        world.workflow_runs().len(),
        1,
        "exactly one requested check, got {:?}",
        world.workflow_runs()
    );
    assert_eq!(world.pushes(), 1, "exactly one push was ever dispatched");
}

fn assert_the_answer_was_lost_to_a_cancellation(world: &ScriptedWorld, object: Object) {
    let at = object.as_str();
    match object {
        Object::Branch => {
            assert_eq!(
                world.branches().len(),
                1,
                "{at}: the ref must really be on the remote, or nothing was lost"
            );
            assert_eq!(
                world.pushes(),
                1,
                "{at}: and the lost answer must not have been resolved by pushing \
                 again"
            );
        }
        Object::PullRequest => assert_landed_under(world, "pulls", "commit_then_wait"),
        Object::Check => assert_landed_under(world, "dispatches", "commit_then_wait"),
    }
}

fn interrupt(pid: u32) {
    let status = std::process::Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .status()
        .expect("kill is on the PATH");
    assert!(status.success(), "could not interrupt {pid}");
}

fn assert_landed_under(world: &ScriptedWorld, needle: &str, mode: &str) {
    let landed = world.landed(needle);
    assert_eq!(
        landed.len(),
        1,
        "the interrupted attempt must have landed exactly one {needle}: {landed:?}"
    );
    assert_eq!(
        landed[0]["mode"], mode,
        "the mutation must have landed under a `gh` that then died: {landed:?}"
    );
}

#[test]
fn the_retry_carries_a_distinct_attempt_id_and_the_same_work_ref() {
    let world = ScriptedWorld::new();
    world.make_ambiguous(Object::PullRequest);
    world.interrupt_after(Object::PullRequest);

    let first = payload_of(&world.publish());
    let first_bundle = world.scenario.read_bundle(&first);

    world.scenario.remove_local_records();
    world.recover_from(Object::PullRequest);

    let second = payload_of(&world.publish());
    let second_bundle = world.scenario.read_bundle(&second);

    assert_ne!(
        first_bundle["attempt_id"], second_bundle["attempt_id"],
        "two attempts, two identities: {first_bundle} / {second_bundle}"
    );
    assert_eq!(
        first_bundle["work_ref"], second_bundle["work_ref"],
        "one piece of work: {first_bundle} / {second_bundle}"
    );
    assert_eq!(
        first_bundle["invocation_ref"], second_bundle["invocation_ref"],
        "addressed the same way both times"
    );
}

#[test]
fn the_github_token_appears_in_no_bundle_no_stdout_and_no_diagnostic() {
    let world = ScriptedWorld::new();
    world.push_mode("delegated");
    world.script(&pulls_key(), "422 1 echo_token");

    let out = world.publish();
    let payload = payload_of(&out);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "publish_change",
        "the credential was resolved and the capability ran: {payload}"
    );
    assert_eq!(
        world.branches().len(),
        1,
        "the credential-carrying `git push` really ran: {payload}"
    );
    assert!(
        summary_of(&payload).contains("422"),
        "the token-bearing response must have been read and reported: {payload}"
    );
    assert!(
        world.gh_requests().iter().any(|request| {
            request["env"].as_array().is_some_and(|env| {
                env.iter()
                    .any(|name| name == &format!("GH_TOKEN={SENTINEL}"))
            })
        }),
        "the scripted `gh` must have received the credential it echoed back"
    );

    assert!(
        !stdout.contains(SENTINEL),
        "the token reached stdout: {stdout}"
    );
    assert!(
        !stderr.contains(SENTINEL),
        "the token reached a diagnostic: {stderr}"
    );
    assert!(
        payload["report"].is_string(),
        "this run must have published a bundle for the search to be about: {payload}"
    );
    let holding = world.files_holding(SENTINEL);
    assert!(
        !holding.is_empty(),
        "the scan found the token nowhere at all, not even in the fixtures' own \
         recordings of the environment they were handed — so it is looking at \
         nothing and would pass on a real leak"
    );
    let leaked: Vec<&String> = holding
        .iter()
        .filter(|path| !is_fixture_recording(path))
        .collect();
    assert!(leaked.is_empty(), "the token was written to {leaked:?}");
}

#[test]
fn an_unreachable_github_publishes_nothing_and_reports_an_unread_forge() {
    let world = ScriptedWorld::new();
    world.use_gh(&unreachable_gh(world.scenario.dir()));

    let out = world.publish();
    let payload = payload_of(&out);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    assert_eq!(
        out.status.code(),
        Some(11),
        "an unreachable forge fails the run, retryably: {payload}\nstderr = {stderr}"
    );

    assert!(
        payload["observations"]["review"]["unavailable"].is_object(),
        "an unreadable forge must be Unavailable and never an empty Available: {payload}"
    );
    assert!(
        payload["observations"]["verification"]["unavailable"].is_object(),
        "and so must the verification: {payload}"
    );
    assert!(
        payload["observations"]["review"]["unavailable"]["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "an unread forge must say why it was not read: {payload}"
    );

    assert!(world.branches().is_empty(), "no branch was created");
    assert!(
        world.pull_requests().is_empty(),
        "no pull request was created"
    );
    assert!(world.workflow_runs().is_empty(), "no check was requested");
    assert_eq!(
        world.pushes(),
        0,
        "the push must not even be dispatched: the branch's postcondition was \
         never read, so nothing licensed one"
    );
    assert_eq!(
        payload["capability_executions"][0]["status"], "failed",
        "the capability ran and failed, which is how the unreadable forge was \
         discovered at all: {payload}"
    );
}

#[cfg(unix)]
fn unreachable_gh(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("unreachable-gh");
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         echo 'dial tcp: connect: connection refused' >&2\n\
         exit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn the_effect_steps_of_a_real_run_reach_the_attempt_journal() {
    let world = ScriptedWorld::new();
    world.push_mode("delegated");
    world.scenario.prepare_journal_dir();
    world.scenario.make_report_dir_unwritable();

    let out = world.publish();
    world.scenario.make_report_dir_writable();
    let payload = payload_of(&out);

    assert_eq!(world.branches().len(), 1, "{payload}");
    assert_eq!(world.pull_requests().len(), 1, "{payload}");
    assert_eq!(world.workflow_runs().len(), 1, "{payload}");

    let records = world.scenario.journal_records();
    assert_eq!(
        records.len(),
        1,
        "one attempt, one journal, and it must have survived an unpublished \
         bundle: {records:?}"
    );
    let recorded: Vec<serde_json::Value> = std::fs::read_to_string(&records[0])
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    const ORDER: [&str; 7] = [
        "validate_capability",
        "derive_identity",
        "inspect_postcondition",
        "combine_policy",
        "authorize",
        "apply",
        "observe_postcondition",
    ];
    for kind in [
        "ensure_branch_published",
        "ensure_pull_request",
        "ensure_check_requested",
    ] {
        let steps: Vec<&str> = recorded
            .iter()
            .filter(|record| record["record"] == "effect_step" && record["kind"] == kind)
            .filter_map(|record| record["step"].as_str())
            .collect();
        assert_eq!(
            steps, ORDER,
            "the journal must hold {kind}'s whole walk, in order"
        );
    }

    assert!(
        recorded.iter().any(|record| record["record"] == "intent"),
        "the intent record must still be the first thing written: {recorded:?}"
    );
}

fn pulls_key() -> String {
    script_key("POST", &format!("/repos/{REPO}/pulls"))
}

fn dispatch_key() -> String {
    script_key(
        "POST",
        &format!("/repos/{REPO}/actions/workflows/{WORKFLOW}/dispatches"),
    )
}

fn script_key(method: &str, path: &str) -> String {
    format!(
        "{method}_{}",
        path.trim_start_matches('/')
            .replace(['/', '?', '&', '=', '%'], "_")
    )
}

fn toml_path(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
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

fn git_says(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("could not run git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
