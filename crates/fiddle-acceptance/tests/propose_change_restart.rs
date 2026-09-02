mod support;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use support::{Scenario, StubGateway};

const TICKET: &str = "ISP-42";

const REFERENCE: &str = "jira:ISP-42";

const SITE: &str = "https://icecube.atlassian.net";

const REPO: &str = "acme/icecube";

const BASE: &str = "main";

const REVISION: &str = "2026-08-30T10:00:00.000+0000";

const ISSUE_TYPE: &str = "Task";

const SUMMARY: &str = "Rename the deprecated helper";

const DESCRIPTION: &str =
    "Rename the deprecated helper in src/lib.rs so the last index is one less than the length.";

const TRIGGER_LABEL: &str = "fiddle/toil";

const FIDDLE_ACCOUNT: &str = "70121:00000000-0000-0000-0000-0000000f1dd1";

const DECIDER_ACCOUNT: &str = "70121:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

const STRANGER_ACCOUNT: &str = "70121:99999999-9999-9999-9999-999999999999";

const APPROVAL: &str = "yes, rename it";

const MODEL_CREDENTIAL: &str = "LITELLM_API_KEY";

const FORGE_CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";

const JIRA_USER: &str = "JIRA_USER_EMAIL";

const JIRA_TOKEN: &str = "JIRA_API_TOKEN";

const MODEL_SENTINEL: &str = "sk-restart-sentinel-must-never-be-printed-1a2b";

const FORGE_SENTINEL: &str = "ghp_restart_sentinel_must_never_be_printed_3c4d";

const JIRA_SENTINEL: &str = "a-tracker-token-no-site-would-honour";

const NODE_ID: &str = "PR_kwDOtoilRestartNode7";

const DECISION_MARKER: &str = "<!-- fiddle:decision v1 request=";

const WROTE_THE_CHANGE: &str = "write_file";

struct Wrote {
    author: String,
    body: serde_json::Value,
}

struct Recorded {
    lines: Vec<String>,
    posted: Vec<String>,
    comments: Vec<Wrote>,
    holds_the_ticket: bool,
}

pub struct RestartJira {
    port: u16,
    state: Arc<Mutex<Recorded>>,
}

impl RestartJira {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(Recorded {
            lines: Vec::new(),
            posted: Vec::new(),
            comments: Vec::new(),
            holds_the_ticket: false,
        }));
        let serving = Arc::clone(&state);
        std::thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                let _ = answer(stream, &serving);
            }
        });
        RestartJira { port, state }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Recorded> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn holds_the_ticket(&self, key: &str) {
        assert_eq!(
            key, TICKET,
            "this world serves one issue, and the invocation this test runs names it"
        );
        self.held().holds_the_ticket = true;
    }

    pub fn human_replies_on(&self, key: &str, text: &str) {
        self.replies_on(key, DECIDER_ACCOUNT, text);
    }

    pub fn a_stranger_replies_on(&self, key: &str, text: &str) {
        self.replies_on(key, STRANGER_ACCOUNT, text);
    }

    fn replies_on(&self, key: &str, account: &str, text: &str) {
        assert_eq!(key, TICKET, "this world serves one issue");
        self.held().comments.push(Wrote {
            author: account.to_string(),
            body: paragraph(text),
        });
    }

    pub fn comments_on(&self, key: &str) -> usize {
        assert_eq!(key, TICKET, "this world serves one issue");
        self.held().comments.len()
    }

    pub fn questions_on(&self, key: &str) -> Vec<String> {
        assert_eq!(key, TICKET, "this world serves one issue");
        self.held()
            .posted
            .iter()
            .filter(|body| written_in(body).contains(DECISION_MARKER))
            .map(|body| written_in(body))
            .collect()
    }

    fn post_through_the_site(&self, body: &str) -> String {
        use std::io::{Read, Write};

        let mut stream = std::net::TcpStream::connect(("127.0.0.1", self.port))
            .expect("the site this world serves accepts a connection");
        stream
            .write_all(
                format!(
                    "POST /rest/api/3/issue/{TICKET}/comment HTTP/1.1\r\n\
                     host: 127.0.0.1\r\ncontent-type: application/json\r\n\
                     content-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .unwrap();
        let mut answered = String::new();
        stream.read_to_string(&mut answered).unwrap();
        answered
    }

    fn request_lines(&self) -> Vec<String> {
        self.held().lines.clone()
    }

    fn comment_posts(&self) -> Vec<String> {
        self.request_lines()
            .into_iter()
            .filter(|line| line.starts_with("POST ") && line.contains("/comment"))
            .collect()
    }

    fn comment_reads(&self) -> usize {
        self.request_lines()
            .iter()
            .filter(|line| line.starts_with("GET ") && line.contains("comment"))
            .count()
    }
}

fn paragraph(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "doc",
        "version": 1,
        "content": [{
            "type": "paragraph",
            "content": [{"type": "text", "text": text}],
        }],
    })
}

fn flattened(node: &serde_json::Value) -> String {
    match node {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Object(fields) => match fields.get("text") {
            Some(serde_json::Value::String(text)) => text.clone(),
            _ => match fields.get("content") {
                Some(held) => flattened(held),
                None => String::new(),
            },
        },
        serde_json::Value::Array(held) => held
            .iter()
            .map(flattened)
            .filter(|read| !read.is_empty())
            .collect::<Vec<String>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn written_in(posted: &str) -> String {
    serde_json::from_str::<serde_json::Value>(posted)
        .ok()
        .and_then(|sent| sent.get("body").cloned())
        .map(|body| flattened(&body))
        .unwrap_or_default()
}

fn answer(mut stream: std::net::TcpStream, state: &Arc<Mutex<Recorded>>) -> std::io::Result<()> {
    use std::io::{Read, Write};

    let mut request = Vec::new();
    let mut chunk = [0u8; 4096];
    let boundary = loop {
        if let Some(at) = index_of(&request, b"\r\n\r\n") {
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
    let line = head.lines().next().unwrap_or_default().to_string();

    let (status, body) = {
        let mut held = state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        held.lines.push(line.clone());
        routed(&line, &sent, &mut held)
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

fn routed(line: &str, sent: &str, held: &mut Recorded) -> (u16, String) {
    let mut parts = line.split_whitespace();
    let (Some(method), Some(target), Some(_)) = (parts.next(), parts.next(), parts.next()) else {
        return (400, unrouted());
    };
    let path = target.split('?').next().unwrap_or(target);
    let read = format!("/rest/api/3/issue/{TICKET}");
    let write = format!("/rest/api/3/issue/{TICKET}/comment");
    match (method, path) {
        ("GET", path) if path == read && held.holds_the_ticket => {
            (200, issue_of(&asked_for(target), &held.comments))
        }
        ("POST", path) if path == write && held.holds_the_ticket => {
            held.posted.push(sent.to_string());
            held.comments.push(Wrote {
                author: FIDDLE_ACCOUNT.to_string(),
                body: serde_json::from_str::<serde_json::Value>(sent)
                    .ok()
                    .and_then(|body| body.get("body").cloned())
                    .unwrap_or_else(|| serde_json::json!(sent)),
            });
            (
                201,
                serde_json::json!({ "id": comment_id(held.comments.len()) }).to_string(),
            )
        }
        _ => (404, unrouted()),
    }
}

fn comment_id(at: usize) -> String {
    format!("40{at:03}")
}

fn unrouted() -> String {
    serde_json::json!({
        "errorMessages": ["the site serves no resource at that path"],
        "errors": {},
    })
    .to_string()
}

fn asked_for(target: &str) -> Vec<String> {
    target
        .split_once("fields=")
        .map(|(_, asked)| {
            asked
                .split('&')
                .next()
                .unwrap_or_default()
                .split(',')
                .filter(|field| !field.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn issue_of(asked: &[String], comments: &[Wrote]) -> String {
    let listed: Vec<serde_json::Value> = comments
        .iter()
        .enumerate()
        .map(|(at, wrote)| {
            serde_json::json!({
                "id": comment_id(at + 1),
                "author": {
                    "accountId": wrote.author,
                    "displayName": match wrote.author.as_str() {
                        FIDDLE_ACCOUNT => "fiddle",
                        DECIDER_ACCOUNT => "the person who may decide",
                        _ => "somebody else",
                    },
                },
                "body": wrote.body,
                "created": REVISION,
                "updated": REVISION,
            })
        })
        .collect();
    let held: Vec<(&str, serde_json::Value)> = vec![
        ("updated", serde_json::json!(REVISION)),
        ("summary", serde_json::json!(SUMMARY)),
        (
            "issuetype",
            serde_json::json!({ "id": "10001", "name": ISSUE_TYPE }),
        ),
        (
            "status",
            serde_json::json!({
                "id": "10002",
                "name": "Ready",
                "statusCategory": { "id": 2, "key": "new", "name": "To Do" },
            }),
        ),
        ("labels", serde_json::json!([TRIGGER_LABEL])),
        ("description", paragraph(DESCRIPTION)),
        (
            "comment",
            serde_json::json!({
                "comments": listed,
                "total": listed.len(),
                "maxResults": listed.len(),
                "startAt": 0,
            }),
        ),
    ];
    let fields: serde_json::Map<String, serde_json::Value> = held
        .into_iter()
        .filter(|(name, _)| asked.iter().any(|wanted| wanted == name))
        .map(|(name, value)| (name.to_string(), value))
        .collect();
    serde_json::json!({
        "id": "10000",
        "key": TICKET,
        "fields": fields,
    })
    .to_string()
}

fn index_of(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn content_length(head: &[u8]) -> usize {
    String::from_utf8_lossy(head)
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "Unassigned",
    }
}

pub struct Ran {
    pid: u32,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Ran {
    fn payload(&self) -> serde_json::Value {
        serde_json::from_str(&self.stdout).unwrap_or_else(|error| {
            panic!(
                "stdout is not JSON ({error}): {}\nstderr = {}",
                self.stdout, self.stderr
            )
        })
    }
}

pub struct RestartWorld {
    scenario: Scenario,
    stub: PathBuf,
    remote: PathBuf,
    jira: RestartJira,
    gateway: StubGateway,
}

impl RestartWorld {
    fn start() -> Self {
        let scenario = Scenario::new();
        let fixture = scenario.write_fixture_repo();

        let stub = scenario.dir().join("gh-stub");
        std::fs::create_dir_all(stub.join("script")).unwrap();
        std::fs::create_dir_all(stub.join("config")).unwrap();
        std::fs::create_dir_all(stub.join("graphql")).unwrap();

        let remote = stub.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        support::git(&remote, &["init", "-q", "--bare", "."]);
        support::git(
            &fixture,
            &["remote", "add", "origin", &remote.display().to_string()],
        );

        let world = RestartWorld {
            stub: stub.clone(),
            remote,
            jira: RestartJira::start(),
            gateway: StubGateway::serving(support::a_suspension_and_its_approval(APPROVAL)),
            scenario,
        };
        let tables = world.tables(&fixture, &stub);
        world.scenario.append_config(&tables);
        world
    }

    fn tables(&self, fixture: &Path, stub: &Path) -> String {
        format!(
            "[github]\n\
             repo = \"{REPO}\"\n\
             base = \"{BASE}\"\n\
             token = {{ env = \"{FORGE_CREDENTIAL}\" }}\n\
             cli = {{ program = {gh}, args = [\"--stub-dir\", {stub}] }}\n\
             git = \"git\"\n\
             config_dir = {config_dir}\n\
             timeout = \"120s\"\n\
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
             check = {{ program = \"true\" }}\n\
             command_timeout = \"300s\"\n\
             \n\
             [jira]\n\
             site = \"{SITE}\"\n\
             project = \"ISP\"\n\
             user = {{ env = \"{JIRA_USER}\" }}\n\
             token = {{ env = \"{JIRA_TOKEN}\" }}\n\
             base_url = \"{jira}\"\n\
             timeout = \"30s\"\n\
             \n\
             [jira.decision]\n\
             authorized = [\"{DECIDER_ACCOUNT}\"]\n",
            gh = support::toml_string(support::gh_stub_binary()),
            stub = support::toml_string(stub),
            config_dir = support::toml_string(&stub.join("config")),
            base_url = self.gateway.base_url(),
            workspaces = support::toml_string(&self.workspace_root()),
            fixture = support::toml_string(fixture),
            jira = self.jira.base_url(),
        )
    }

    fn workspace_root(&self) -> PathBuf {
        self.scenario.dir().join("workspaces")
    }

    pub fn jira(&self) -> &RestartJira {
        &self.jira
    }

    fn run(&self) -> Ran {
        let mut command = self.scenario.spawnable_run_command(REFERENCE);
        for name in support::CREDENTIAL_VARS {
            command.env_remove(name);
        }
        let child = command
            .args(["--capability", "propose_change", "--json"])
            .env(MODEL_CREDENTIAL, MODEL_SENTINEL)
            .env(FORGE_CREDENTIAL, FORGE_SENTINEL)
            .env(JIRA_USER, "nobody@example.com")
            .env(JIRA_TOKEN, JIRA_SENTINEL)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the shipped binary starts");
        let pid = child.id();
        let out = child.wait_with_output().expect("the process ends");
        Ran {
            pid,
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        }
    }

    fn next_process(&self) -> Ran {
        self.delete_local_records();
        assert_eq!(
            self.local_records(),
            Vec::<String>::new(),
            "every process this world starts begins with no serialized run"
        );
        self.run()
    }

    fn gh(&self, args: &[&str]) -> String {
        let out = std::process::Command::new(support::gh_stub_binary())
            .args(["--stub-dir", self.stub.to_str().unwrap()])
            .args(args)
            .output()
            .expect("the forge stub answers");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn open_pull_requests(&self) -> Vec<serde_json::Value> {
        support::body_of(&self.gh(&[
            "api",
            "--method",
            "GET",
            &format!("/repos/{REPO}/pulls?state=open"),
        ]))
    }

    fn pull_request(&self, number: u64) -> serde_json::Value {
        let answered = self.gh(&[
            "api",
            "--method",
            "GET",
            &format!("/repos/{REPO}/pulls/{number}"),
        ]);
        support::object_of(&answered)
            .unwrap_or_else(|| panic!("the forge answered no pull request {number}: {answered}"))
    }

    fn branches(&self) -> Vec<String> {
        let refs = support::git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        );
        match refs.is_empty() {
            true => Vec::new(),
            false => refs.lines().map(str::to_string).collect(),
        }
    }

    fn remote_head(&self, branch: &str) -> String {
        support::git_says(&self.remote, &["rev-parse", branch])
    }

    fn answer_pull_request_by_number(&self, number: u64, branch: &str) -> String {
        let head_sha = self.remote_head(branch);
        let dir = self.stub.join("pulls_by_number");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{number}.json")),
            serde_json::json!({
                "number": number,
                "state": "open",
                "draft": true,
                "node_id": NODE_ID,
                "head": { "ref": branch, "sha": &head_sha },
                "base": { "ref": BASE },
            })
            .to_string(),
        )
        .unwrap();
        head_sha
    }

    fn accept_the_ready_mutation(&self) {
        std::fs::write(
            self.stub.join("graphql").join("0.json"),
            serde_json::json!({
                "status": 200,
                "body": {
                    "data": { "markPullRequestReadyForReview": { "clientMutationId": null } }
                },
            })
            .to_string(),
        )
        .unwrap();
    }

    fn graphql_calls(&self) -> usize {
        std::fs::read_to_string(self.stub.join("graphql_calls"))
            .ok()
            .and_then(|seen| seen.trim().parse().ok())
            .unwrap_or(0)
    }

    fn the_world_and_not_fiddles_memory(&self) -> Vec<PathBuf> {
        vec![
            self.stub.clone(),
            self.scenario.dir().join("fixture"),
            self.scenario.config_path(),
        ]
    }

    fn local_records(&self) -> Vec<String> {
        let root = self.scenario.dir();
        let world = self.the_world_and_not_fiddles_memory();
        support::walkdir_files(root)
            .into_iter()
            .filter(|path| !world.iter().any(|kept| path.starts_with(kept)))
            .map(|path| path.strip_prefix(root).unwrap().display().to_string())
            .collect()
    }

    fn delete_local_records(&self) {
        let root = self.scenario.dir();
        let world = self.the_world_and_not_fiddles_memory();
        for entry in std::fs::read_dir(root).unwrap().flatten() {
            let path = entry.path();
            if world.contains(&path) {
                continue;
            }
            let removed = match path.is_dir() {
                true => std::fs::remove_dir_all(&path),
                false => std::fs::remove_file(&path),
            };
            removed.unwrap_or_else(|error| panic!("could not remove {} ({error})", path.display()));
        }
        for empty in ["changes", "work"] {
            std::fs::create_dir_all(self.scenario.stub_root().join(empty)).unwrap();
        }
    }

    fn deployment_document(&self) -> String {
        std::fs::read_to_string(self.scenario.config_path())
            .expect("the document this world's runs are steered by")
    }

    fn model_calls(&self) -> usize {
        self.gateway.served()
    }

    fn model_requests(&self) -> Vec<String> {
        self.gateway.request_bodies()
    }
}

fn effects_of(payload: &serde_json::Value) -> Vec<String> {
    payload["capability_executions"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .flat_map(|execution| {
            execution["evidence"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .filter_map(|line| line.as_str())
                .filter(|line| line.starts_with("effect:"))
                .map(str::to_string)
                .collect::<Vec<String>>()
        })
        .collect()
}

fn effect_named(payload: &serde_json::Value, kind: &str) -> String {
    let lines = effects_of(payload);
    let matched: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with(&format!("effect:{kind}:")))
        .collect();
    assert_eq!(
        matched.len(),
        1,
        "the run performed `{kind}` exactly once and its receipt is one line: {lines:?}"
    );
    matched[0].clone()
}

fn number_of(pull_requests: &[serde_json::Value]) -> u64 {
    assert_eq!(
        pull_requests.len(),
        1,
        "this world opens one pull request and the forge lists {pull_requests:?}"
    );
    pull_requests[0]["number"]
        .as_u64()
        .unwrap_or_else(|| panic!("a listed pull request carries a number: {pull_requests:?}"))
}

#[test]
fn a_second_process_reads_the_reply_the_first_asked_for() {
    let world = RestartWorld::start();
    world.jira().holds_the_ticket(TICKET);
    let document = world.deployment_document();
    assert!(
        document.contains("[jira]"),
        "this world holds a `[jira]` table: {document}"
    );
    assert!(
        !document.contains("[jira.filing]"),
        "and it files no tickets, so the question this run publishes below is carried by \
         the issue because the invocation names one and not because the deployment asked \
         for a filing client: {document}"
    );

    let first = world.run();
    assert_eq!(
        first.code,
        Some(10),
        "the first run asked a question on the issue and suspended: stdout={} stderr={}",
        first.stdout,
        first.stderr
    );
    let asked = first.payload();
    let published = effect_named(&asked, "jira.comment_added");
    assert!(
        published.contains(&format!("{TICKET} comment")),
        "and it published the question onto the issue the invocation named, which is the \
         only channel a later process can read it back from: {asked}"
    );
    assert!(
        effects_of(&asked)
            .iter()
            .all(|line| !line.starts_with("effect:publish_decision_request:")),
        "and it published no question onto the pull request, so exactly one channel \
         carries this request: {asked}"
    );

    let branches = world.branches();
    assert_eq!(
        branches.len(),
        1,
        "the row's own premise: the first run published one branch: {branches:?}"
    );
    let pull_request = number_of(&world.open_pull_requests());
    let head_sha = world.answer_pull_request_by_number(pull_request, &branches[0]);
    world.accept_the_ready_mutation();

    let questions = world.jira().questions_on(TICKET);
    assert_eq!(
        questions.len(),
        1,
        "the first run asked once, counted from the comment posts the tracker stub \
         received: {:?}",
        world.jira().comment_posts()
    );
    let binding = support::parse_marker(&questions[0]).expect("the question carries its marker");
    assert_eq!(
        binding.request,
        support::expected_request_id(
            support::PROJECT_NAME,
            REFERENCE,
            REPO,
            pull_request,
            &head_sha
        ),
        "and the question it asked is the one this project, this invocation and this \
         commit derive, rebuilt here from those inputs rather than read back out of \
         the comment"
    );
    assert_eq!(
        world.model_calls(),
        2,
        "the first process paid for the agent's write and the agent's report, so what \
         follows is a run that did the work and not one that refused before starting"
    );

    let held = world.local_records();
    assert!(
        held.iter().any(|path| path.ends_with("report.json")),
        "the first process published its bundle, so the deletion below has something \
         to be about: {held:?}"
    );
    assert_eq!(
        held.len(),
        1,
        "and that bundle is the whole durable trace a suspended run leaves under this \
         world's root: {held:?}"
    );
    world.delete_local_records();
    assert_eq!(
        world.local_records(),
        Vec::<String>::new(),
        "the second process must have no serialized run to read. This is every file \
         under this world's root that is not the forge stub, the project tree or the \
         deployment document, so it is not a list of the places this test remembered \
         to look: it was left {:?} of the {held:?} the first process wrote",
        world.local_records()
    );

    world.jira().human_replies_on(TICKET, APPROVAL);
    let reads_after_one = world.jira().comment_reads();

    let second = world.run();

    assert_ne!(
        second.pid, first.pid,
        "the two runs are two operating-system processes, and this is their proof"
    );
    assert!(
        first.pid != std::process::id() && second.pid != std::process::id(),
        "and neither of them is this test process, which is what makes the loss of \
         the first process real rather than a struct that was cleared"
    );
    assert_eq!(
        second.code,
        Some(0),
        "the second process read the reply off the issue and ran to the end: stdout={} \
         stderr={}",
        second.stdout,
        second.stderr
    );

    let continued = second.payload();
    assert_ne!(
        continued["report"], asked["report"],
        "it published its own bundle under its own attempt rather than pointing at the \
         one this test deleted, so nothing it reports was read back out of the first \
         process's record: {continued}"
    );
    assert_eq!(
        world.local_records(),
        vec![
            format!(
                "reports/{}",
                continued["report"].as_str().unwrap_or_default()
            ),
            format!("stub-state/changes/{TICKET}.json"),
        ],
        "and its own bundle and its own change set are the whole durable record under \
         this world's root, so the first process's bundle is still gone"
    );
    assert_eq!(
        continued["capability_executions"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        1,
        "it executed the capability once, so the counts below are a run that continued: \
         {continued}"
    );
    assert_eq!(
        world.graphql_calls(),
        1,
        "and it dispatched the gated effect exactly once, which is the act the question \
         was about: {continued}"
    );
    assert_eq!(
        world.pull_request(pull_request)["draft"],
        serde_json::json!(false),
        "the forge is what says the pull request is no longer a draft: {continued}"
    );

    assert_eq!(
        world.jira().questions_on(TICKET).len(),
        1,
        "the second process asked no second question, counted from the comment posts the \
         tracker stub received: {:?}",
        world.jira().comment_posts()
    );
    assert_eq!(
        world.jira().comment_posts().len(),
        1,
        "and it wrote nothing else onto the issue either: {:?}",
        world.jira().comment_posts()
    );
    assert!(
        world.jira().comment_reads() > reads_after_one,
        "and the question it did not ask is one it looked for: the second process read \
         the issue's comments after the first had finished, counted against the {reads_after_one} \
         reads the first process had made: {:?}",
        world.jira().request_lines()
    );

    assert_eq!(
        world.branches(),
        branches,
        "it published no second branch: {continued}"
    );
    assert_eq!(
        number_of(&world.open_pull_requests()),
        pull_request,
        "and opened no second pull request: {continued}"
    );

    assert_eq!(
        world.model_calls(),
        3,
        "the second process paid for one model call, which is the interpretation of the \
         reply, and never for a second attempt at the change"
    );
    let interpreting = world
        .model_requests()
        .last()
        .cloned()
        .expect("the second process asked the model something");
    assert!(
        interpreting.contains(APPROVAL),
        "and what it sent the model is the words the person wrote on the issue: \
         {interpreting}"
    );
    assert!(
        !interpreting.contains(WROTE_THE_CHANGE),
        "and it carried no turn of the first process's agent loop, so the second \
         process reconstructed its input from the issue rather than from a conversation \
         it inherited: {interpreting}"
    );
}

#[test]
fn between_two_processes_only_what_the_issue_holds_moves_the_run_on() {
    let world = RestartWorld::start();
    world.jira().holds_the_ticket(TICKET);

    let first = world.run();
    assert_eq!(
        first.code,
        Some(10),
        "the row's own premise: the first run asked and suspended: stdout={} stderr={}",
        first.stdout,
        first.stderr
    );
    let branch = world.branches();
    assert_eq!(branch.len(), 1, "one branch: {branch:?}");
    let pull_request = number_of(&world.open_pull_requests());
    world.answer_pull_request_by_number(pull_request, &branch[0]);
    world.accept_the_ready_mutation();

    let unanswered = world.next_process();
    assert_eq!(
        unanswered.code,
        Some(10),
        "an issue holding no reply moves nothing on, so the process suspends again: \
         stdout={} stderr={}",
        unanswered.stdout,
        unanswered.stderr
    );
    let silence = unanswered.payload();
    let waiting = silence["outcome"]["suspended"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a suspended run says what it is waiting for: {silence}"));
    assert!(
        waiting.contains("nobody who may decide has answered it yet"),
        "and it says the issue holds no answer, rather than that it has just asked: \
         {waiting}"
    );

    world.jira().a_stranger_replies_on(TICKET, APPROVAL);
    let declined = world.next_process();
    assert_eq!(
        declined.code,
        Some(10),
        "and the same words from an account this document did not name move nothing on \
         either: stdout={} stderr={}",
        declined.stdout,
        declined.stderr
    );
    let standing = declined.payload();
    let why = standing["outcome"]["suspended"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("a suspended run says what it is waiting for: {standing}"));
    assert!(
        why.contains(STRANGER_ACCOUNT),
        "and it names the reply it declined and the account that wrote it: {why}"
    );

    assert_eq!(
        world.jira().questions_on(TICKET).len(),
        1,
        "through both of those processes the question was asked once: {:?}",
        world.jira().comment_posts()
    );
    assert_eq!(
        world.graphql_calls(),
        0,
        "and the gated effect was never dispatched, so a count of one question is not \
         by itself evidence that a process continued"
    );
    assert_eq!(
        world.pull_request(pull_request)["draft"],
        serde_json::json!(true),
        "and the forge still says the pull request is a draft"
    );

    world.jira().human_replies_on(TICKET, APPROVAL);
    let continued = world.next_process();
    assert_eq!(
        continued.code,
        Some(0),
        "the only thing that changed between these four processes is what the issue \
         holds, and a reply from the account the document named is what moved the run \
         on: stdout={} stderr={}",
        continued.stdout,
        continued.stderr
    );
    assert_eq!(
        world.graphql_calls(),
        1,
        "the gated effect was dispatched once: {}",
        continued.stdout
    );
    assert_eq!(
        world.pull_request(pull_request)["draft"],
        serde_json::json!(false),
        "and the forge is what says the pull request is no longer a draft"
    );

    assert_eq!(
        world.jira().questions_on(TICKET).len(),
        1,
        "across four processes the question was asked once: {:?}",
        world.jira().comment_posts()
    );
    assert_eq!(
        world.jira().comments_on(TICKET),
        3,
        "counted out of three comments on the issue, so the count above is a filter \
         that tells a question from a reply rather than one that matches everything"
    );
    assert_eq!(
        world.model_calls(),
        3,
        "and four processes paid for one write, one report and one interpretation: the \
         two that found no answer they could act on paid for nothing"
    );
}

#[test]
fn the_question_count_reads_the_marker_and_not_merely_a_comment() {
    let world = RestartWorld::start();
    world.jira().holds_the_ticket(TICKET);
    let marked = |request: &str| {
        serde_json::json!({
            "body": paragraph(&format!(
                "May fiddle mark it ready?\n\n<!-- fiddle:decision v1 request={request} \
                 effect=0000000000000000 payload=0000000000000000 \
                 head=0000000000000000000000000000000000000000 -->"
            )),
        })
        .to_string()
    };

    world
        .jira()
        .post_through_the_site(&marked("1111111111111111"));
    world
        .jira()
        .post_through_the_site(&marked("2222222222222222"));
    world.jira().post_through_the_site(
        &serde_json::json!({ "body": paragraph("I am looking at it") }).to_string(),
    );

    assert_eq!(
        world.jira().comments_on(TICKET),
        3,
        "three comments reached the issue: {:?}",
        world.jira().comment_posts()
    );
    assert_eq!(
        world.jira().questions_on(TICKET).len(),
        2,
        "and two of them are questions, so a count of one elsewhere in this file is a \
         filter that can read two and does not match every comment: {:?}",
        world.jira().questions_on(TICKET)
    );
    assert_eq!(
        world
            .jira()
            .questions_on(TICKET)
            .iter()
            .filter(|question| question.contains("1111111111111111"))
            .count(),
        1,
        "and the two it counted are the two distinct requests, not one body counted \
         twice: {:?}",
        world.jira().questions_on(TICKET)
    );
}
