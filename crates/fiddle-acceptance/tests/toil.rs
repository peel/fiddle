mod support;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use support::{Scenario, StubGateway};

const TICKET: &str = "ISP-42";

const REFERENCE: &str = "jira:ISP-42";

const TRIGGER_LABEL: &str = "fiddle/toil";

const ISSUE_TYPE: &str = "Task";

const SUMMARY: &str = "Rename the deprecated helper";

const DESCRIPTION: &str =
    "Rename the deprecated helper in src/lib.rs so the last index is one less than the length.";

const SITE: &str = "https://icecube.atlassian.net";

const REPO: &str = "acme/icecube";

const BASE: &str = "main";

const MODEL_CREDENTIAL: &str = "LITELLM_API_KEY";

const FORGE_CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";

const JIRA_USER: &str = "JIRA_USER_EMAIL";

const JIRA_TOKEN: &str = "JIRA_API_TOKEN";

const MODEL_SENTINEL: &str = "sk-toil-sentinel-must-never-be-printed-1a2b";

const FORGE_SENTINEL: &str = "ghp_toil_sentinel_must_never_be_printed_3c4d";

const JIRA_SENTINEL: &str = "a-tracker-token-no-site-would-honour";

const REVISION_SENT: &str = "2026-08-30T10:00:00.000+0000";

const REVISION_MOVED: &str = "2026-08-30T11:30:00.000+0000";

const MARKER: &str = "fiddle-effect:";

const LINK_EFFECT: &str = "jira.pull_request_linked";

const PULL_REQUEST_EFFECT: &str = "ensure_pull_request";

const BRANCH_EFFECT: &str = "ensure_branch_published";

const TRIGGER_LABEL_RULE: &str = "the trigger label is present";

const ISSUE_TYPE_RULE: &str = "the issue type is one the toil agent works";

const UNWORKED_ISSUE_TYPE: &str = "Bug";

struct Posted {
    issue: String,
    body: String,
}

struct Held {
    issue_type: String,
    summary: String,
    description: String,
    labels: Vec<String>,
    revisions: Vec<String>,
}

struct Recorded {
    lines: Vec<String>,
    ticket: Option<Held>,
    comments: Vec<Posted>,
    issue_reads: usize,
    comments_refused: bool,
}

pub struct ToilJira {
    port: u16,
    state: Arc<Mutex<Recorded>>,
}

impl ToilJira {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let port = listener.local_addr().unwrap().port();
        let state = Arc::new(Mutex::new(Recorded {
            lines: Vec::new(),
            ticket: None,
            comments: Vec::new(),
            issue_reads: 0,
            comments_refused: false,
        }));
        let serving = Arc::clone(&state);
        std::thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                let _ = answer(stream, &serving);
            }
        });
        ToilJira { port, state }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Recorded> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn holds_eligible_ticket(&self, key: &str) {
        assert_eq!(
            key, TICKET,
            "this world serves one ticket, and the invocation this test runs names it"
        );
        self.held().ticket = Some(Held {
            issue_type: ISSUE_TYPE.to_string(),
            summary: SUMMARY.to_string(),
            description: DESCRIPTION.to_string(),
            labels: vec![TRIGGER_LABEL.to_string()],
            revisions: vec![REVISION_SENT.to_string()],
        });
    }

    pub fn moves_after_it_is_qualified(&self) {
        self.held()
            .ticket
            .as_mut()
            .expect("a ticket this site holds is what moves")
            .revisions = vec![REVISION_SENT.to_string(), REVISION_MOVED.to_string()];
    }

    pub fn settles_at_the_revision_it_moved_to(&self) {
        self.held()
            .ticket
            .as_mut()
            .expect("a ticket this site holds is what settles")
            .revisions = vec![REVISION_MOVED.to_string()];
    }

    pub fn holds_a_ticket_without_the_trigger_label(&self, key: &str) {
        self.holds_eligible_ticket(key);
        self.held()
            .ticket
            .as_mut()
            .expect("the ticket was just written")
            .labels = vec!["backend".to_string()];
    }

    pub fn holds_a_ticket_of_an_unworked_type(&self, key: &str) {
        self.holds_eligible_ticket(key);
        self.held()
            .ticket
            .as_mut()
            .expect("the ticket was just written")
            .issue_type = UNWORKED_ISSUE_TYPE.to_string();
    }

    pub fn refuses_every_comment(&self) {
        self.held().comments_refused = true;
    }

    pub fn last_comment_on(&self, key: &str) -> Option<String> {
        self.held()
            .comments
            .iter()
            .rfind(|posted| posted.issue == key)
            .map(|posted| posted.body.clone())
    }

    pub fn links_for(&self, key: &str) -> Vec<String> {
        let names_a_pull_request = format!("https://github.com/{REPO}/pull/");
        self.held()
            .comments
            .iter()
            .filter(|posted| {
                posted.issue == key
                    && posted.body.contains(MARKER)
                    && posted.body.contains(&names_a_pull_request)
            })
            .map(|posted| posted.body.clone())
            .collect()
    }

    fn comment_posts(&self) -> usize {
        self.request_lines()
            .iter()
            .filter(|line| {
                line.starts_with("POST ") && line.contains(&format!("/issue/{TICKET}/comment"))
            })
            .count()
    }

    fn request_lines(&self) -> Vec<String> {
        self.held().lines.clone()
    }

    fn writes(&self) -> Vec<String> {
        self.request_lines()
            .into_iter()
            .filter(|line| !line.starts_with("GET "))
            .collect()
    }

    fn comment_reads(&self) -> usize {
        self.request_lines()
            .iter()
            .filter(|line| line.starts_with("GET ") && line.contains("fields=comment"))
            .count()
    }
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
        ("GET", path) if path == read => {
            let asked = asked_for(target);
            let comments = std::mem::take(&mut held.comments);
            let answered = match &held.ticket {
                None => (404, unrouted()),
                Some(ticket) => {
                    let at = held.issue_reads.min(ticket.revisions.len() - 1);
                    (
                        200,
                        issue_of(ticket, &ticket.revisions[at], &comments, &asked),
                    )
                }
            };
            held.comments = comments;
            if answered.0 == 200 && asked != vec!["comment".to_string()] {
                held.issue_reads += 1;
            }
            answered
        }
        ("POST", path) if path == write && held.comments_refused => (403, comment_refused()),
        ("POST", path) if path == write => {
            held.comments.push(Posted {
                issue: TICKET.to_string(),
                body: sent.to_string(),
            });
            (
                201,
                serde_json::json!({ "id": format!("40{:03}", held.comments.len()) }).to_string(),
            )
        }
        _ => (404, unrouted()),
    }
}

fn comment_refused() -> String {
    serde_json::json!({
        "errorMessages": ["this caller may not comment on that issue"],
        "errors": {},
    })
    .to_string()
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

fn issue_of(ticket: &Held, revision: &str, comments: &[Posted], asked: &[String]) -> String {
    let listed: Vec<serde_json::Value> = comments
        .iter()
        .enumerate()
        .map(|(at, posted)| {
            serde_json::json!({
                "id": format!("40{:03}", at + 1),
                "author": {
                    "accountId": "70121:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
                    "displayName": "fiddle",
                },
                "body": serde_json::from_str::<serde_json::Value>(&posted.body)
                    .ok()
                    .and_then(|sent| sent.get("body").cloned())
                    .unwrap_or_else(|| serde_json::json!(posted.body)),
                "created": REVISION_SENT,
                "updated": REVISION_SENT,
            })
        })
        .collect();
    let held: Vec<(&str, serde_json::Value)> = vec![
        ("updated", serde_json::json!(revision)),
        ("summary", serde_json::json!(ticket.summary)),
        (
            "issuetype",
            serde_json::json!({ "id": "10001", "name": ticket.issue_type }),
        ),
        (
            "status",
            serde_json::json!({
                "id": "10002",
                "name": "Ready",
                "statusCategory": { "id": 2, "key": "new", "name": "To Do" },
            }),
        ),
        ("labels", serde_json::json!(ticket.labels)),
        (
            "description",
            serde_json::json!({
                "type": "doc",
                "version": 1,
                "content": [{
                    "type": "paragraph",
                    "content": [{"type": "text", "text": ticket.description}],
                }],
            }),
        ),
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
        403 => "Forbidden",
        404 => "Not Found",
        _ => "Unassigned",
    }
}

pub struct ToilForge {
    stub: PathBuf,
    remote: PathBuf,
}

impl ToilForge {
    pub fn pull_requests(&self) -> Vec<serde_json::Value> {
        self.landed("pulls")
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

    fn reads_naming(&self, needle: &str) -> usize {
        support::walkdir_files(self.stub.join("requests"))
            .iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .filter_map(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .filter(|request| request["argv"].to_string().contains(needle))
            .count()
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

    fn only_branch(&self) -> String {
        let branches = self.branches();
        assert_eq!(
            branches.len(),
            1,
            "this world publishes one branch, and it holds {branches:?}"
        );
        branches[0].clone()
    }

    fn head_of(&self, branch: &str) -> String {
        support::git_says(&self.remote, &["rev-parse", branch])
    }

    fn delete_branch(&self, branch: &str) {
        support::git(
            &self.remote,
            &["update-ref", "-d", &format!("refs/heads/{branch}")],
        );
    }
}

pub struct ToilWorld {
    scenario: Scenario,
    forge: ToilForge,
    jira: ToilJira,
    gateway: StubGateway,
}

const REPAIRED: &str = support::REPAIRED_FIXTURE;

fn a_review_that_reads_a_change() -> support::Reply {
    support::accepted(support::reports(serde_json::json!({
        "verdict": "asks_for_a_change",
        "quoting": DESCRIPTION,
        "certainty": 0.92,
    })))
}

fn an_accepted_change() -> Vec<support::Reply> {
    vec![
        a_review_that_reads_a_change(),
        support::accepted(support::calls(
            "write_file",
            serde_json::json!({ "path": "src/lib.rs", "contents": REPAIRED }),
        )),
        support::accepted(support::reports(serde_json::json!({
            "changed_files": ["src/lib.rs"],
            "summary": "corrected the off-by-one the ticket named",
            "claimed_complete": true,
        }))),
        support::accepted(support::reports(serde_json::json!({
            "verdict": "accepted",
        }))),
    ]
}

impl ToilWorld {
    pub fn start() -> Self {
        ToilWorld::serving(
            an_accepted_change()
                .into_iter()
                .chain(an_accepted_change())
                .collect(),
        )
    }

    pub fn start_paying_for_a_qualification_that_earns_nothing() -> Self {
        ToilWorld::serving(
            std::iter::once(a_review_that_reads_a_change())
                .chain(an_accepted_change())
                .collect(),
        )
    }

    fn serving(script: Vec<support::Reply>) -> Self {
        let scenario = Scenario::new();
        let fixture = scenario.write_fixture_repo();

        let stub = scenario.dir().join("gh-stub");
        std::fs::create_dir_all(stub.join("script")).unwrap();
        std::fs::create_dir_all(stub.join("config")).unwrap();

        let remote = stub.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        support::git(&remote, &["init", "-q", "--bare", "."]);
        support::git(
            &fixture,
            &["remote", "add", "origin", &remote.display().to_string()],
        );

        ship_the_workflow(scenario.dir());

        let jira = ToilJira::start();
        let gateway = StubGateway::serving(script);
        let world = ToilWorld {
            forge: ToilForge {
                stub: stub.clone(),
                remote,
            },
            jira,
            gateway,
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
             [jira.labels]\n\
             toil_trigger = \"{TRIGGER_LABEL}\"\n",
            gh = support::toml_string(support::gh_stub_binary()),
            stub = support::toml_string(stub),
            config_dir = support::toml_string(&stub.join("config")),
            base_url = self.gateway.base_url(),
            workspaces = support::toml_string(&self.scenario.dir().join("workspaces")),
            fixture = support::toml_string(fixture),
            jira = self.jira.base_url(),
        )
    }

    pub fn denies_the_refusal_comment(&self) {
        self.scenario
            .append_config("[github.policy]\n\"jira.comment_added\" = \"deny\"\n");
    }

    pub fn jira(&self) -> &ToilJira {
        &self.jira
    }

    pub fn expected_marker(&self) -> String {
        self.scenario.expected_marker(REFERENCE)
    }

    fn completion_record(&self) -> PathBuf {
        self.scenario
            .stub_root()
            .join(format!("changes/{TICKET}.json"))
    }

    pub fn recorded_marker(&self) -> Option<String> {
        self.scenario.read_change_marker(TICKET)
    }

    pub fn forgets_that_the_work_was_completed(&self) {
        let path = self.completion_record();
        std::fs::remove_file(&path)
            .unwrap_or_else(|error| panic!("could not remove {} ({error})", path.display()));
    }

    pub fn github(&self) -> &ToilForge {
        &self.forge
    }

    pub fn run_toil(&self, invocation_ref: &str) -> std::process::Output {
        let mut command = self.scenario.spawnable_run_command(invocation_ref);
        for name in support::CREDENTIAL_VARS {
            command.env_remove(name);
        }
        command
            .args(["--json"])
            .env(MODEL_CREDENTIAL, MODEL_SENTINEL)
            .env(FORGE_CREDENTIAL, FORGE_SENTINEL)
            .env(JIRA_USER, "nobody@example.com")
            .env(JIRA_TOKEN, JIRA_SENTINEL)
            .output()
            .unwrap()
    }

    fn model_calls(&self) -> usize {
        self.gateway.served()
    }

    fn published_files_holding(&self, secret: &str) -> Vec<String> {
        support::walkdir_files(self.scenario.report_dir())
            .into_iter()
            .filter(|path| {
                std::fs::read(path)
                    .map(|bytes| String::from_utf8_lossy(&bytes).contains(secret))
                    .unwrap_or(false)
            })
            .map(|path| path.display().to_string())
            .collect()
    }
}

fn ship_the_workflow(into: &Path) {
    let from = support::repo_root().join("workflows");
    let to = into.join("workflows");
    std::fs::create_dir_all(to.join("prompts")).unwrap();
    std::fs::copy(from.join("toil.toml"), to.join("toil.toml")).unwrap();
    for prompt in support::walkdir_files(from.join("prompts")) {
        let name = prompt.file_name().expect("a prompt is a file");
        std::fs::copy(&prompt, to.join("prompts").join(name)).unwrap();
    }
}

fn payload_of(out: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr = {stderr}"))
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

fn external_ref_of(evidence: &str) -> String {
    let fields: Vec<&str> = evidence.split(':').collect();
    assert!(
        fields.len() > 4,
        "an effect evidence line carries kind, id, outcome and external reference: {evidence}"
    );
    fields[4].to_string()
}

#[test]
fn an_eligible_ticket_produces_one_pull_request_and_one_jira_link() {
    let world = ToilWorld::start();
    world.jira().holds_eligible_ticket(TICKET);

    let run = world.run_toil(REFERENCE);
    let payload = payload_of(&run);

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "toil",
        "the run this test measures is the toil capability and no other: {payload}"
    );
    assert_eq!(
        payload["capability_executions"][0]["status"], "completed",
        "the capability ran to the end of the shipped document: {payload}"
    );
    let expected = world.expected_marker();
    let marker = payload
        .pointer("/observations/changes/available/value/marker")
        .unwrap_or_else(|| {
            panic!(
                "the post-execution observation of the change set is available and carries \
                 a marker field, which is the only input the assessment reads: {payload}"
            )
        });
    assert_eq!(
        marker.as_str(),
        Some(expected.as_str()),
        "the shipped document writes the correlation marker this invocation is judged \
         by, and writes that one and no other: {payload}"
    );
    assert_eq!(
        world.recorded_marker().as_deref(),
        Some(expected.as_str()),
        "and the marker the run reports is the marker a later reader finds on disk, so \
         the observation above is a file this run wrote and not a value it carried: {payload}"
    );
    assert_eq!(
        run.status.code(),
        Some(0),
        "so the post-execution assessment reads the marker it expects, finds the work \
         accounted for, and the run reports success; 11 would be a retry, 12 a rejected \
         evaluation and 20 a failure: {payload}"
    );
    assert_eq!(
        payload["outcome"], "completed",
        "and the outcome the bundle carries is the same verdict: {payload}"
    );
    assert_eq!(
        payload
            .pointer("/progress/0/summary")
            .and_then(|s| s.as_str()),
        Some(format!("wrote correlation marker {expected}").as_str()),
        "and the summary names the marker the change set carries afterwards, rather \
         than the one the run expected: {payload}"
    );

    assert_eq!(
        world.github().pull_requests().len(),
        1,
        "exactly one pull request was created, counted from the requests the forge \
         stub received: {payload}"
    );
    assert_eq!(
        world.jira().links_for(TICKET).len(),
        1,
        "exactly one link comment reached the ticket, counted from the requests the \
         tracker stub received: {:?}",
        world.jira().request_lines()
    );

    assert_eq!(
        world.github().branches().len(),
        1,
        "the branch the pull request was opened on is on the remote, and there is one \
         of it: {:?}",
        world.github().branches()
    );
    assert!(
        world.jira().links_for(TICKET)[0].contains(&format!("{REPO}#")),
        "the link names the pull request a reader can follow: {:?}",
        world.jira().links_for(TICKET)
    );
    assert_eq!(
        world.model_calls(),
        4,
        "the run paid for the ambiguity review, the agent's write, the agent's report \
         and the evaluation, so the counts above are not a run that never started"
    );
    assert_eq!(
        effects_of(&payload)
            .iter()
            .map(|line| line.split(':').nth(1).unwrap_or_default().to_string())
            .collect::<Vec<String>>(),
        vec![BRANCH_EFFECT, PULL_REQUEST_EFFECT, LINK_EFFECT],
        "the three effect steps of the shipped document ran, in the order it names \
         them: {payload}"
    );
    assert_eq!(
        external_ref_of(&effect_named(&payload, LINK_EFFECT)),
        "40001",
        "the link receipt names the comment the tracker stub recorded, so the one \
         counted above and the one the run reports are the same comment: {payload}"
    );
    assert_eq!(
        world.jira().comment_reads(),
        2,
        "and the link step read the ticket's comments twice, once to look for a \
         comment it had already posted and once to settle the one it posted, so a \
         prior link would have been found by the first of those reads: {:?}",
        world.jira().request_lines()
    );
}

#[test]
fn a_second_run_over_the_same_ticket_adds_no_second_pull_request_and_no_second_link() {
    let world = ToilWorld::start();
    world.jira().holds_eligible_ticket(TICKET);

    let first = payload_of(&world.run_toil(REFERENCE));
    assert_eq!(
        world.github().pull_requests().len(),
        1,
        "the row's own premise: the first run opened one pull request: {first}"
    );
    assert_eq!(
        world.jira().links_for(TICKET).len(),
        1,
        "the row's own premise: the first run linked it once: {first}"
    );
    assert_eq!(
        world.recorded_marker(),
        Some(world.expected_marker()),
        "and the row's own premise: the first run recorded that this invocation is \
         accounted for: {first}"
    );
    let branch = world.github().only_branch();
    let published = world.github().head_of(&branch);
    let writes_after_one = world.jira().writes().len();

    let rerun = world.run_toil(REFERENCE);
    let second = payload_of(&rerun);

    assert_eq!(
        second["capability_executions"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        0,
        "the second run read the marker the first run recorded, found the work \
         accounted for, and never executed the document: {second}"
    );
    assert_eq!(
        second["next_action"], "complete",
        "which is the action the assessment derived, and not a step that ran and \
         stopped: {second}"
    );
    assert_eq!(
        world.model_calls(),
        5,
        "so it paid for the eligibility review alone, and for no agent turn, no \
         report and no evaluation: {second}"
    );

    assert_eq!(
        world.github().pull_requests().len(),
        1,
        "no second pull request was created, counted from the requests the forge \
         stub received: {second}"
    );
    assert_eq!(
        world.jira().links_for(TICKET).len(),
        1,
        "no second link comment reached the ticket, counted from the requests the \
         tracker stub received: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        world.jira().writes().len(),
        writes_after_one,
        "the second run wrote nothing at all onto the ticket: {:?}",
        world.jira().writes()
    );
    assert_eq!(
        world.github().branches(),
        vec![branch.clone()],
        "and it published no second branch: {second}"
    );
    assert_eq!(
        world.github().head_of(&branch),
        published,
        "and it left the branch the first run published where the first run left it, \
         so nothing was overwritten either: {second}"
    );
    assert_eq!(
        effects_of(&second),
        Vec::<String>::new(),
        "so the second run earned no effect receipt at all: {second}"
    );
    assert_eq!(
        rerun.status.code(),
        Some(0),
        "and it reports the work as done rather than as a retry: {second}"
    );
}

#[test]
fn a_run_reported_retryable_reaches_a_terminal_state_when_it_is_retried() {
    let world = ToilWorld::start_paying_for_a_qualification_that_earns_nothing();
    world.jira().holds_eligible_ticket(TICKET);
    world.jira().moves_after_it_is_qualified();

    let stopped = world.run_toil(REFERENCE);
    let first = payload_of(&stopped);
    assert_eq!(
        stopped.status.code(),
        Some(11),
        "the row's own premise: the ticket moved between the qualification and the \
         first effect, so this run reports a retry: {first}"
    );
    let reason = first["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("and it says what it wants retried: {first}"));
    assert!(
        reason.contains("qualified at revision") && reason.contains("now reads revision"),
        "which is the recheck refusing, and it names both revisions it compared: {reason}"
    );
    assert_eq!(
        world.recorded_marker(),
        None,
        "a run that reports a retry records no completion, so the retry below runs \
         the document rather than reading a marker: {first}"
    );
    assert!(
        world.github().pull_requests().is_empty() && world.github().branches().is_empty(),
        "and it published nothing, so the retry starts from a world this run did not \
         move: {first}"
    );

    world.jira().settles_at_the_revision_it_moved_to();
    let retried = world.run_toil(REFERENCE);
    let second = payload_of(&retried);

    assert_eq!(
        retried.status.code(),
        Some(0),
        "the retry reached a terminal state rather than reporting a retry again: {second}"
    );
    assert_eq!(
        second["outcome"], "completed",
        "and the terminal state it reached is completion: {second}"
    );
    assert_eq!(
        second["capability_executions"][0]["status"], "completed",
        "which it reached by running the document to its end: {second}"
    );
    assert_eq!(
        world.model_calls(),
        5,
        "the first run paid for its qualification alone and the retry paid for a \
         qualification, a write, a report and an evaluation, so the retry did the \
         work rather than reading a record of it: {second}"
    );
    assert_eq!(
        world.github().pull_requests().len(),
        1,
        "so the retry opened the pull request the first run never opened: {second}"
    );
    assert_eq!(
        world.jira().links_for(TICKET).len(),
        1,
        "and linked it on the ticket once: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        world.recorded_marker(),
        Some(world.expected_marker()),
        "and recorded the completion the next run would read: {second}"
    );
}

#[test]
fn a_retry_over_a_branch_this_invocation_already_published_does_not_converge() {
    let world = ToilWorld::start();
    world.jira().holds_eligible_ticket(TICKET);

    let first = payload_of(&world.run_toil(REFERENCE));
    let branch = world.github().only_branch();
    let published = world.github().head_of(&branch);
    assert_eq!(
        world.github().pull_requests().len(),
        1,
        "the row's own premise: the first run published a branch and opened one pull \
         request on it: {first}"
    );

    world.forgets_that_the_work_was_completed();
    assert_eq!(
        world.recorded_marker(),
        None,
        "and the row's second premise: no completion is recorded, which is the world a \
         run that published a branch and then reported a retry leaves behind"
    );
    let ref_reads_after_one = world.github().reads_naming("git/ref/heads");

    let retried = world.run_toil(REFERENCE);
    let second = payload_of(&retried);

    assert!(
        world.github().reads_naming("git/ref/heads") > ref_reads_after_one,
        "the retry asked the forge for the branch its own identity names, so what \
         follows is a step that looked at the prior work: {second}"
    );
    assert_eq!(
        world.model_calls(),
        8,
        "and it paid for its own review, write, report and evaluation, so it is a run \
         that did the work again and not one that refused before starting: {second}"
    );

    assert_eq!(
        retried.status.code(),
        Some(11),
        "the retry reports a retry again, so this pair does not converge: {second}"
    );
    let stopped = second["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("and this build reports where it stopped: {second}"));
    assert!(
        stopped.contains(BRANCH_EFFECT) && stopped.contains(&branch),
        "it stopped at the branch step, and the step names the branch the first run \
         published, which is the prior work it found: {stopped}"
    );
    assert!(
        stopped.contains("not an ancestor") && stopped.contains("not forced"),
        "and it stopped because it refused to overwrite that branch, rather than \
         because it could not reach the forge: {stopped}"
    );
    assert!(
        !stopped.contains(PULL_REQUEST_EFFECT) && !stopped.contains(LINK_EFFECT),
        "the pull request and the link are not what refused; they were never \
         reached, and neither was written a second time: {stopped}"
    );
    assert_eq!(
        world.github().head_of(&branch),
        published,
        "and the branch still points where the first run left it: {second}"
    );
    assert_ne!(
        second["capability_executions"][0]["status"], "completed",
        "the retry produced the same tree and a different commit, because nothing \
         fixes the commit dates, so the branch guard refuses it on every attempt; \
         `fiddle-buu6` carries that, and this row reds when it lands: {second}"
    );
    assert_eq!(
        world.recorded_marker(),
        None,
        "so the retry recorded no completion either, and a third run would stop in \
         the same place: {second}"
    );
}

#[test]
fn a_ticket_without_the_trigger_label_opens_no_pull_request_and_writes_no_link() {
    let world = ToilWorld::start();
    world
        .jira()
        .holds_a_ticket_without_the_trigger_label(TICKET);

    let run = world.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();

    assert!(
        stderr.contains("the trigger label is present"),
        "the gate refused on the rule that failed and named it: {stderr}"
    );
    assert!(
        stderr.contains(TRIGGER_LABEL),
        "and named the label the document asks for: {stderr}"
    );
    assert!(
        world.github().pull_requests().is_empty(),
        "an ineligible ticket opens no pull request: {stderr}"
    );
    assert!(
        world.jira().links_for(TICKET).is_empty(),
        "and writes no link: {stderr}"
    );
    assert!(
        world.github().branches().is_empty(),
        "and publishes no branch: {stderr}"
    );
    assert_eq!(
        world.model_calls(),
        0,
        "the gate refused before the ambiguity review, so no model call was paid \
         for: {stderr}"
    );
}

#[test]
fn a_ticket_that_moves_after_it_is_qualified_opens_no_pull_request() {
    let world = ToilWorld::start();
    world.jira().holds_eligible_ticket(TICKET);
    world.jira().moves_after_it_is_qualified();

    let payload = payload_of(&world.run_toil(REFERENCE));

    let reason = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "a ticket that moved between the qualification and the first effect is \
                 work to qualify again, which this build reports as a retry: {payload}"
            )
        });
    assert!(
        reason.contains("qualified at revision") && reason.contains("now reads revision"),
        "the recheck refused, and it names both revisions it compared: {reason}"
    );
    assert_eq!(
        payload["capability_executions"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        0,
        "the capability never executed, so the recheck bit before the first effect and \
         not after it: {payload}"
    );
    assert_eq!(
        world.model_calls(),
        1,
        "the run paid for the ambiguity review and for nothing else, so the ticket was \
         qualified and the agent step was never reached: {payload}"
    );
    assert!(
        world.github().pull_requests().is_empty()
            && world.github().branches().is_empty()
            && world.jira().links_for(TICKET).is_empty(),
        "and it published nothing at all: {payload}"
    );
}

#[test]
fn a_rerun_whose_branch_is_gone_finds_its_own_pull_request_and_its_own_link() {
    let world = ToilWorld::start();
    world.jira().holds_eligible_ticket(TICKET);

    let first = payload_of(&world.run_toil(REFERENCE));
    let branch = world.github().only_branch();
    let opened = external_ref_of(&effect_named(&first, PULL_REQUEST_EFFECT));
    let linked = external_ref_of(&effect_named(&first, LINK_EFFECT));
    let writes_after_one = world.jira().writes().len();
    world.github().delete_branch(&branch);
    assert!(
        world.github().branches().is_empty(),
        "the row's own premise: the branch the first run published is gone, so the \
         second run publishes a branch rather than refusing to overwrite one"
    );
    world.forgets_that_the_work_was_completed();
    assert_eq!(
        world.recorded_marker(),
        None,
        "and the row's second premise: the completion the first run recorded is gone \
         too, so the second run works the ticket again rather than reading a marker \
         and stopping"
    );

    let second = payload_of(&world.run_toil(REFERENCE));

    assert_eq!(
        second["capability_executions"][0]["status"], "completed",
        "the second run reached the end of the same document: {second}"
    );
    assert_eq!(
        effects_of(&second)
            .iter()
            .map(|line| line.split(':').nth(1).unwrap_or_default().to_string())
            .collect::<Vec<String>>(),
        vec![BRANCH_EFFECT, PULL_REQUEST_EFFECT, LINK_EFFECT],
        "and it ran all three effect steps a second time, so what follows is what \
         those steps did and not a run that stopped short: {second}"
    );

    assert_eq!(
        world.github().pull_requests().len(),
        1,
        "no second pull request was created, counted from the requests the forge \
         stub received: {second}"
    );
    assert_eq!(
        external_ref_of(&effect_named(&second, PULL_REQUEST_EFFECT)),
        opened,
        "and the second run's pull request step names the pull request the first run \
         opened, so it found its own prior work: {second}"
    );

    assert_eq!(
        world.jira().links_for(TICKET).len(),
        1,
        "no second link comment reached the ticket, counted from the requests the \
         tracker stub received: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        world.jira().writes().len(),
        writes_after_one,
        "the second run wrote nothing onto the ticket: {:?}",
        world.jira().writes()
    );
    assert_eq!(
        external_ref_of(&effect_named(&second, LINK_EFFECT)),
        linked,
        "and its link step names the comment the first run posted, so the link it did \
         not write is one it looked for and found: {second}"
    );
    assert_eq!(
        world.model_calls(),
        8,
        "both runs paid for a review, a write, a report and an evaluation, so neither \
         of the counts above is a run that refused before starting"
    );
}

#[test]
fn no_credential_this_lane_exports_reaches_a_surface_a_reader_of_the_run_reaches() {
    let world = ToilWorld::start();
    world.jira().holds_eligible_ticket(TICKET);

    let run = world.run_toil(REFERENCE);
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();

    assert_eq!(
        world.github().pull_requests().len(),
        1,
        "the row's own premise: this run reached the forge and the tracker with all \
         three credentials, so the surfaces below carried a whole run: {stderr}"
    );
    assert!(
        !world.published_files_holding(TICKET).is_empty(),
        "and the run published a bundle naming its ticket, so the search below reads \
         files that exist"
    );

    for (named, secret) in [
        ("the model", MODEL_SENTINEL),
        ("the forge", FORGE_SENTINEL),
        ("the tracker", JIRA_SENTINEL),
    ] {
        assert!(
            !stdout.contains(secret),
            "{named}'s credential reached stdout, which is the payload a caller reads"
        );
        assert!(
            !stderr.contains(secret),
            "{named}'s credential reached stderr, which is where a diagnostic goes"
        );
        assert_eq!(
            world.published_files_holding(secret),
            Vec::<String>::new(),
            "{named}'s credential was written into the run's own report"
        );
    }
}

#[test]
fn a_refused_ticket_is_told_why_on_its_own_issue() {
    let world = ToilWorld::start();
    world
        .jira()
        .holds_a_ticket_without_the_trigger_label(TICKET);

    let run = world.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();

    assert_eq!(
        world.jira().comment_posts(),
        1,
        "one comment request reached the tracker stub, so the ticket was written to and \
         this is not a run that stopped before it published: {:?}",
        world.jira().request_lines()
    );
    let comment = world
        .jira()
        .last_comment_on(TICKET)
        .expect("the refusal was published");
    assert!(
        comment.contains(TRIGGER_LABEL_RULE),
        "the refusal names the rule that failed: {comment}"
    );
    assert!(
        comment.contains(TRIGGER_LABEL),
        "and names the label the document asks for, which is the remedy a reader acts \
         on: {comment}"
    );
    assert_eq!(
        run.status.code(),
        Some(2),
        "and the run itself refused, which this build reports as exit 2: {stderr}"
    );
    assert!(
        world.github().pull_requests().is_empty(),
        "no pull request was opened, counted from the requests the forge stub \
         received: {stderr}"
    );
    assert!(
        world.github().branches().is_empty(),
        "and no branch was published: {stderr}"
    );
    assert!(
        world.jira().links_for(TICKET).is_empty(),
        "and the one comment the ticket received is a refusal and not a link to a pull \
         request: {comment}"
    );
    assert_eq!(
        world.model_calls(),
        0,
        "the gate refused before the ambiguity review, so the refusal cost no model \
         call: {stderr}"
    );
    let carried = comment
        .split(MARKER)
        .nth(1)
        .map(|tail| {
            tail.chars()
                .take_while(|held| held.is_ascii_hexdigit())
                .collect::<String>()
        })
        .expect("the published comment carries this build's effect marker");
    assert!(
        stderr.contains(&format!("effect_id   = {carried}")),
        "the run reports a receipt for the identity the published comment carries, so the \
         write was performed under an effect identity and not as a bare request: {stderr}"
    );
}

#[test]
fn a_deployment_that_denies_the_comment_effect_publishes_nothing_and_still_refuses() {
    let world = ToilWorld::start();
    world
        .jira()
        .holds_a_ticket_without_the_trigger_label(TICKET);
    world.denies_the_refusal_comment();

    let run = world.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();

    assert_eq!(
        world.jira().comment_posts(),
        0,
        "the deployment denied `jira.comment_added` and nothing was sent, so the refusal \
         is written by the executor that reads that rule and not by a raw adapter call \
         that never sees it: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        world.jira().last_comment_on(TICKET),
        None,
        "and the ticket holds no refusal: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        run.status.code(),
        Some(2),
        "a refusal a deployment will not publish is still a refusal: {stderr}"
    );
    assert!(
        stderr.contains(TRIGGER_LABEL_RULE),
        "and it still names the rule that failed: {stderr}"
    );
    assert!(
        stderr.contains(TICKET) && stderr.contains("was not published"),
        "and the operator is told the ticket was never reached: {stderr}"
    );
    assert_eq!(
        world.model_calls(),
        0,
        "the denied comment bought no model call: {stderr}"
    );
    assert!(
        world.github().pull_requests().is_empty(),
        "and opened no pull request: {stderr}"
    );
}

#[test]
fn a_refusal_for_a_different_rule_names_that_rule_and_not_the_first() {
    let world = ToilWorld::start();
    world.jira().holds_a_ticket_of_an_unworked_type(TICKET);

    let run = world.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();

    assert_eq!(
        world.jira().comment_posts(),
        1,
        "the row's own premise: this ticket was refused on a later rule and told why \
         once: {:?}",
        world.jira().request_lines()
    );
    let comment = world
        .jira()
        .last_comment_on(TICKET)
        .expect("the refusal was published");
    assert!(
        comment.contains(ISSUE_TYPE_RULE),
        "the refusal names the rule this ticket failed: {comment}"
    );
    assert!(
        comment.contains(UNWORKED_ISSUE_TYPE),
        "and quotes the issue type it read, which is the finding the rule rests \
         on: {comment}"
    );
    assert!(
        !comment.contains(TRIGGER_LABEL_RULE),
        "and it does not name the rule the other refusal named, so a refusal comment \
         reads off the rule that failed and is not one constant string: {comment}"
    );
    assert_eq!(
        run.status.code(),
        Some(2),
        "and this ticket is refused too: {stderr}"
    );
    assert!(
        world.github().pull_requests().is_empty(),
        "and it opened no pull request either: {stderr}"
    );
}

#[test]
fn a_site_that_refuses_the_comment_still_refuses_the_ticket() {
    let world = ToilWorld::start();
    world
        .jira()
        .holds_a_ticket_without_the_trigger_label(TICKET);
    world.jira().refuses_every_comment();

    let run = world.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();

    assert_eq!(
        world.jira().comment_posts(),
        1,
        "the row's own premise: the run asked the tracker to publish the refusal, so what \
         follows is a comment the site answered and not a comment nobody sent: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        world.jira().last_comment_on(TICKET),
        None,
        "and the site kept none of it, so this ticket was never told why: {:?}",
        world.jira().request_lines()
    );

    assert_eq!(
        run.status.code(),
        Some(2),
        "a ticket that could not be told why is still refused: {stderr}"
    );
    assert!(
        stderr.contains(TRIGGER_LABEL_RULE),
        "and the refusal still names the rule that failed: {stderr}"
    );
    assert_eq!(
        world.model_calls(),
        0,
        "the refused comment bought no model call, so it did not turn the refusal into a \
         run: {stderr}"
    );
    assert!(
        world.github().pull_requests().is_empty(),
        "and opened no pull request: {stderr}"
    );
    assert!(
        world.github().branches().is_empty(),
        "and published no branch: {stderr}"
    );

    assert!(
        stderr.contains(TICKET) && stderr.contains("was not published"),
        "and the operator is told the ticket was never reached, because a refusal nobody \
         received is not a refusal delivered: {stderr}"
    );
    assert!(
        !stderr.contains(JIRA_SENTINEL),
        "the note about the refused comment carries no tracker credential: {stderr}"
    );
    assert!(
        !stderr.contains(DESCRIPTION),
        "and it quotes no ticket prose onto the terminal, which is the surface the \
         refusal keeps prose off: {stderr}"
    );
}

#[test]
fn a_second_run_over_one_ineligible_ticket_adds_no_second_refusal() {
    let world = ToilWorld::start();
    world
        .jira()
        .holds_a_ticket_without_the_trigger_label(TICKET);

    let first = world.run_toil(REFERENCE);
    assert_eq!(
        world.jira().comment_posts(),
        1,
        "the row's own premise: the first run told the ticket why once: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let told = world.jira().last_comment_on(TICKET);
    let reads_after_one = world.jira().comment_reads();

    let second = world.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&second.stderr).to_string();

    assert_eq!(
        world.jira().comment_posts(),
        1,
        "the second run posted no second refusal, counted from the requests the tracker \
         stub received: {:?}",
        world.jira().request_lines()
    );
    assert!(
        world.jira().comment_reads() > reads_after_one,
        "and it read the ticket's comments again, so the comment it did not write is one \
         it looked for and found: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        world.jira().last_comment_on(TICKET),
        told,
        "and it left the refusal the first run published where the first run left \
         it: {stderr}"
    );
    assert_eq!(
        second.status.code(),
        Some(2),
        "and it refused the ticket a second time: {stderr}"
    );
}
