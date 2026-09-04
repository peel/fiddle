mod support;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use support::{Answering, Scenario, StubGateway};

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

const TRANSITION_EFFECT: &str = "jira.issue_transitioned";

const READY: &str = "Ready";

const IN_REVIEW: &str = "In Review";

const A_ROUTE_TO_REVIEW: &str = "31";

const PULL_REQUEST_EFFECT: &str = "ensure_pull_request";

const BRANCH_EFFECT: &str = "ensure_branch_published";

const TRIGGER_LABEL_RULE: &str = "the trigger label is present";

const ISSUE_TYPE_RULE: &str = "the issue type is one the toil agent works";

const UNWORKED_ISSUE_TYPE: &str = "Bug";

const QUOTES_THE_TICKET_RULE: &str = "a judgement quotes the ticket text it rests on";

const OPERATOR_ACCOUNT: &str = "70121:11111111-2222-3333-4444-555555555555";

const A_STRANGER_ACCOUNT: &str = "70121:99999999-8888-7777-6666-555555555555";

const THE_OPEN_QUESTION: &str =
    "Keep the old helper beside the new one, or remove it? Either is defensible.";

const THE_DECISION: &str = "Remove it. Nothing calls the old helper any more.";

const THE_SUGGESTION: &str =
    "Suggested: keep the old helper beside the new one, and remove it later if nothing calls it.";

const THE_COMMENTS_CHOICE: &str = "pub fn last_index(len: usize) -> usize { len - 1 }\n";

const THE_DESCRIPTIONS_CHOICE: &str = "pub fn last_index(len: usize) -> usize { len - 1 }\n\
                                       pub fn deprecated_last(len: usize) -> usize { \
                                       last_index(len) }\n";

const NEITHER_TEXT_REACHED_THE_IMPLEMENTER: &str =
    "// neither the comment's decision nor the description's suggestion was in the prompt\n";

const THE_DESCRIPTION_THAT_NAMES_NO_NEW_NAME: &str =
    "Two ways to fix this. First, keep the old helper and guard its one caller. Second, rename \
     the helper, which is the correct fix and for which nothing here gives a new name. \
     Suggested: the first, as the immediate fix.";

const A_DECISION_THE_TICKET_DID_NOT_SPECIFY: &str = "The second one. Renaming is what lasts.";

const THE_QUESTION_THAT_STOPPED_IT: &str =
    "The second option renames the helper and this ticket never says what the new name is.";

const ISP_263_WEIGHS_A_AND_B: &str =
    "The gauge merge_graph_size_max always reports 0. Option A, no downstream risk: guard the \
     report site so the gauge is only set when the value is above zero. Option B, correct but \
     involves a rename: emit as a Sample rather than a Gauge, and because the metric is already \
     named merge_graph_size_max, emitting it as a sample would produce merge_graph_size_max_max, \
     so the base name needs to change to merge_graph_size. The ServeGauges entry at \
     metrics.go:41 must be removed in the same change, otherwise the old name keeps being \
     zero-initialised at startup. Downstream consumers checked: no references to \
     merge_graph_size_max in any .md, .json or .yaml in the repo, and neither the AWS Identity \
     nor the GCP Identity dashboard queries it. The secondary TTLBufferEntriesMax finding is a \
     separate pass. Suggested: A as the immediate fix, B as a follow-up.";

const ISP_263_SUGGESTS_A: &str = "Suggested: A as the immediate fix, B as a follow-up.";

const ISP_263_CHOOSES_OPTION_B: &str =
    "Option B. More-reliable long-term. The bare metrics should be still type-compatible as \
     described.";

const OPTION_B_AS_THE_TICKET_SPECIFIES_IT: &str =
    "pub const MERGE_GRAPH_SIZE: &str = \"merge_graph_size\";\n\
     pub fn report(size: usize) { sample(MERGE_GRAPH_SIZE, size) }\n";

const OPTION_A_A_RUN_SUBSTITUTED: &str =
    "pub const MERGE_GRAPH_SIZE_MAX: &str = \"merge_graph_size_max\";\n\
     pub fn report(size: usize) { if size > 0 { gauge(MERGE_GRAPH_SIZE_MAX, size) } }\n";

const NEITHER_OPTION_REACHED_THE_IMPLEMENTER: &str =
    "// neither option's own text was in the prompt\n";

const THE_RUNS_OWN_ACCOUNT_OF_WHY_IT_BUILT_A: &str =
    "Option B's exact shape (new field name, whether to keep old field, how consumers reference \
     it) is not fully specified in the ticket, so implementing Option B correctly requires \
     decisions not settled by the ticket (e.g., the exact new metric field name/type in the \
     Metrics struct). To stay bounded and not guess a rename or type change across the metrics \
     registration system, I implemented the safe, explicitly-described Option A guard.";

const THE_OBJECTION_ISP_263_ANSWERS: &str =
    "the exact new metric field name/type in the Metrics struct";

const A_FINDING_THAT_NAMES_THE_SUBSTITUTION: &str =
    "src/lib.rs still names merge_graph_size_max and still emits it as a gauge";

const THE_EVALUATION_WAS_ASKED: &str = "judge one change against the ticket that asked for it";

const THE_SCHEMA_ADMITS_A_NAMED_OPTION_IT_DOES_NOT_SPECIFY: &str =
    "it names the option it wants and does not specify that option enough to build.";

const THE_TASK_FORBIDS_THE_SUBSTITUTION: &str =
    "Making the other change instead is the one response that is never open to you.";

const THE_TASK_ADMITS_A_NAMED_OPTION_IT_DOES_NOT_SPECIFY: &str =
    "or it names the one it wants and does not give enough of it to build.";

const THE_PREAMBLE_ADMITS_A_DECIDED_OPTION_CAN_BE_UNSPECIFIED: &str =
    "Before you call a decided option underspecified";

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
    status: String,
    offered: Vec<(String, String)>,
    conversation: Vec<(String, String)>,
}

struct Recorded {
    lines: Vec<String>,
    ticket: Option<Held>,
    comments: Vec<Posted>,
    issue_reads: usize,
    comments_refused: bool,
    reads_refused_with: Option<u16>,
    transitions: Vec<String>,
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
            reads_refused_with: None,
            transitions: Vec::new(),
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
            status: READY.to_string(),
            offered: vec![(A_ROUTE_TO_REVIEW.to_string(), IN_REVIEW.to_string())],
            conversation: Vec::new(),
        });
    }

    pub fn holds_a_ticket_whose_description_leaves_a_question_open(&self, key: &str) {
        self.holds_eligible_ticket(key);
        self.held()
            .ticket
            .as_mut()
            .expect("the ticket was just written")
            .description = format!("{DESCRIPTION} {THE_OPEN_QUESTION}");
    }

    pub fn holds_a_ticket_whose_description_suggests_keeping_the_helper(&self, key: &str) {
        self.holds_eligible_ticket(key);
        self.held()
            .ticket
            .as_mut()
            .expect("the ticket was just written")
            .description = format!("{DESCRIPTION} {THE_OPEN_QUESTION} {THE_SUGGESTION}");
    }

    pub fn holds_a_ticket_whose_second_option_it_never_names(&self, key: &str) {
        self.holds_eligible_ticket(key);
        self.held()
            .ticket
            .as_mut()
            .expect("the ticket was just written")
            .description = THE_DESCRIPTION_THAT_NAMES_NO_NEW_NAME.to_string();
    }

    pub fn holds_the_description_isp_263_held(&self, key: &str) {
        self.holds_eligible_ticket(key);
        self.held()
            .ticket
            .as_mut()
            .expect("the ticket was just written")
            .description = ISP_263_WEIGHS_A_AND_B.to_string();
    }

    pub fn is_commented_on_by(&self, author: &str) {
        self.is_commented_on_by_saying(author, THE_DECISION);
    }

    pub fn is_commented_on_by_saying(&self, author: &str, said: &str) {
        self.held()
            .ticket
            .as_mut()
            .expect("a ticket this site holds is what carries the conversation")
            .conversation
            .push((author.to_string(), said.to_string()));
    }

    pub fn offers_no_route_to_in_review(&self) {
        self.held()
            .ticket
            .as_mut()
            .expect("a ticket this site holds is what offers the routes")
            .offered = vec![("41".to_string(), "Done".to_string())];
    }

    pub fn transition_requests(&self) -> usize {
        self.held().transitions.len()
    }

    pub fn status_now(&self) -> String {
        self.held()
            .ticket
            .as_ref()
            .map(|ticket| ticket.status.clone())
            .unwrap_or_default()
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

    pub fn answers_every_read_with(&self, status: u16, key: &str) {
        self.holds_eligible_ticket(key);
        self.held().reads_refused_with = Some(status);
    }

    fn issue_read_requests(&self) -> usize {
        self.request_lines()
            .iter()
            .filter(|line| line.starts_with("GET ") && line.contains(&format!("/issue/{TICKET}")))
            .count()
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
    let moves = format!("/rest/api/3/issue/{TICKET}/transitions");
    match (method, path) {
        ("GET", path) if path == read && held.reads_refused_with.is_some() => {
            let status = held.reads_refused_with.unwrap_or_default();
            (status, site_unreachable(status))
        }
        ("GET", path) if path == moves => match &held.ticket {
            None => (404, unrouted()),
            Some(ticket) => (200, offered_by(ticket)),
        },
        ("POST", path) if path == moves => {
            held.transitions.push(sent.to_string());
            let asked = serde_json::from_str::<serde_json::Value>(sent)
                .ok()
                .and_then(|body| body["transition"]["id"].as_str().map(str::to_string));
            let leads_to = held
                .ticket
                .as_ref()
                .zip(asked.as_ref())
                .and_then(|(ticket, id)| {
                    ticket
                        .offered
                        .iter()
                        .find(|(offered, _)| offered == id)
                        .map(|(_, leads_to)| leads_to.clone())
                });
            match leads_to {
                None => (404, unrouted()),
                Some(leads_to) => {
                    held.ticket
                        .as_mut()
                        .expect("the route was found on a ticket this site holds")
                        .status = leads_to;
                    (204, String::new())
                }
            }
        }
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

fn site_unreachable(status: u16) -> String {
    serde_json::json!({
        "errorMessages": [format!("the site is not serving reads right now ({status})")],
        "errors": {},
    })
    .to_string()
}

fn offered_by(ticket: &Held) -> String {
    let transitions: Vec<serde_json::Value> = ticket
        .offered
        .iter()
        .map(|(id, leads_to)| {
            serde_json::json!({
                "id": id,
                "name": format!("Move to {leads_to}"),
                "to": named_status(leads_to),
            })
        })
        .collect();
    serde_json::json!({ "expand": "transitions", "transitions": transitions }).to_string()
}

fn named_status(name: &str) -> serde_json::Value {
    let (id, category) = match name {
        READY => ("10002", ("2", "new", "To Do")),
        IN_REVIEW => ("10004", ("4", "indeterminate", "In Progress")),
        _ => ("10005", ("3", "done", "Done")),
    };
    serde_json::json!({
        "id": id,
        "name": name,
        "statusCategory": { "id": category.0, "key": category.1, "name": category.2 },
    })
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
    let written: Vec<serde_json::Value> = ticket
        .conversation
        .iter()
        .enumerate()
        .map(|(at, (author, text))| {
            serde_json::json!({
                "id": format!("39{:03}", at + 1),
                "author": { "accountId": author, "displayName": "a person" },
                "body": {
                    "type": "doc",
                    "version": 1,
                    "content": [{
                        "type": "paragraph",
                        "content": [{"type": "text", "text": text}],
                    }],
                },
                "created": REVISION_SENT,
                "updated": REVISION_SENT,
            })
        })
        .collect();
    let listed: Vec<serde_json::Value> = written
        .into_iter()
        .chain(comments.iter().enumerate().map(|(at, posted)| {
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
        }))
        .collect();
    let held: Vec<(&str, serde_json::Value)> = vec![
        ("updated", serde_json::json!(revision)),
        ("summary", serde_json::json!(ticket.summary)),
        (
            "issuetype",
            serde_json::json!({ "id": "10001", "name": ticket.issue_type }),
        ),
        ("status", named_status(&ticket.status)),
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

    fn file_at(&self, commit: &str, path: &str) -> String {
        support::git_says(&self.remote, &["show", &format!("{commit}:{path}")])
    }

    fn date_of(&self, commit: &str) -> String {
        support::git_says(&self.remote, &["log", "-1", "--format=%cI%n%aI", commit])
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
    fixture: PathBuf,
    forge: ToilForge,
    jira: ToilJira,
    gateway: StubGateway,
}

const REPAIRED: &str = support::REPAIRED_FIXTURE;

const THE_EVALUATIONS_DOCUMENTED_BOUND: usize = 12;

const A_REQUEST_THAT_OBLIGES_A_TOOL_CALL: &str = "\"tool_choice\":\"required\"";

const A_REQUEST_THAT_PERMITS_AN_ANSWER: &str = "\"tool_choice\":\"auto\"";

const REPAIRED_ANOTHER_WAY: &str =
    "pub fn last_index(len: usize) -> usize {\n    len.saturating_sub(1)\n}\n";

fn a_review_that_reads_a_change() -> support::Reply {
    support::accepted(support::reports(serde_json::json!({
        "verdict": "asks_for_a_change",
        "quoting": DESCRIPTION,
        "certainty": 0.92,
    })))
}

fn a_review_that_reads_the_decision_a_comment_made() -> support::Reply {
    support::accepted(support::reports(serde_json::json!({
        "verdict": "asks_for_a_change",
        "quoting": THE_DECISION,
        "certainty": 0.92,
    })))
}

fn a_review_that_reads(quoting: &str) -> support::Reply {
    support::accepted(support::reports(serde_json::json!({
        "verdict": "asks_for_a_change",
        "quoting": quoting,
        "certainty": 0.92,
    })))
}

fn an_accepted_verdict() -> support::Reply {
    support::accepted(support::reports(
        serde_json::json!({ "verdict": "accepted" }),
    ))
}

fn a_verdict_that_rejects(finding: &str) -> support::Reply {
    support::accepted(support::reports(serde_json::json!({
        "verdict": "rejected",
        "findings": [finding],
    })))
}

fn reading_the_file_before_it_answers() -> support::Reply {
    support::accepted(support::calls(
        "read_file",
        serde_json::json!({ "path": "src/lib.rs" }),
    ))
}

fn a_report_that_built_the_option_the_description_suggested() -> support::Reply {
    support::accepted(support::reports(serde_json::json!({
        "changed_files": ["src/lib.rs"],
        "summary": THE_RUNS_OWN_ACCOUNT_OF_WHY_IT_BUILT_A,
        "claimed_complete": true,
    })))
}

fn a_report_that_changed_nothing_and_asked(question: &str) -> support::Reply {
    support::accepted(support::reports(serde_json::json!({
        "changed_files": [],
        "summary": "the ticket names an option it does not specify, so this attempt wrote nothing",
        "claimed_complete": false,
        "stopped_by_this_question": question,
    })))
}

fn writing(contents: &str) -> support::Reply {
    support::accepted(support::calls(
        "write_file",
        serde_json::json!({ "path": "src/lib.rs", "contents": contents }),
    ))
}

fn a_change_the_ticket_chooses() -> Vec<Answering> {
    vec![
        support::on_reading(
            THE_DECISION,
            a_review_that_reads_the_decision_a_comment_made(),
            a_review_that_reads_a_change(),
        ),
        support::choosing(
            vec![
                (THE_DECISION, writing(THE_COMMENTS_CHOICE)),
                (THE_SUGGESTION, writing(THE_DESCRIPTIONS_CHOICE)),
            ],
            writing(NEITHER_TEXT_REACHED_THE_IMPLEMENTER),
        ),
        Answering::Always(support::accepted(support::reports(serde_json::json!({
            "changed_files": ["src/lib.rs"],
            "summary": "made the change the ticket decided on",
            "claimed_complete": true,
        })))),
        Answering::Always(support::accepted(support::reports(serde_json::json!({
            "verdict": "accepted",
        })))),
    ]
}

fn an_accepted_change_whose_judge_reads_before_it_answers() -> Vec<support::Reply> {
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
        support::accepted(support::calls(
            "read_file",
            serde_json::json!({ "path": "src/lib.rs" }),
        )),
        support::accepted(support::reports(serde_json::json!({
            "verdict": "accepted",
        }))),
    ]
}

fn an_accepted_change() -> Vec<support::Reply> {
    an_accepted_change_writing(REPAIRED)
}

fn an_accepted_change_writing(contents: &str) -> Vec<support::Reply> {
    vec![
        a_review_that_reads_a_change(),
        support::accepted(support::calls(
            "write_file",
            serde_json::json!({ "path": "src/lib.rs", "contents": contents }),
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

    pub fn start_writing_something_else_the_second_time(second: &str) -> Self {
        ToilWorld::serving(
            an_accepted_change()
                .into_iter()
                .chain(an_accepted_change_writing(second))
                .collect(),
        )
    }

    pub fn start_reviewing_a_ticket_a_comment_decided() -> Self {
        ToilWorld::serving(
            std::iter::once(a_review_that_reads_the_decision_a_comment_made())
                .chain(an_accepted_change().into_iter().skip(1))
                .collect(),
        )
    }

    pub fn start_letting_the_ticket_choose_the_change() -> Self {
        ToilWorld::built(a_change_the_ticket_chooses(), true)
    }

    pub fn start_over_a_decision_the_ticket_did_not_specify() -> Self {
        ToilWorld::built(
            vec![
                Answering::Always(a_review_that_reads(A_DECISION_THE_TICKET_DID_NOT_SPECIFY)),
                Answering::Always(reading_the_file_before_it_answers()),
                support::on_reading(
                    THE_SCHEMA_ADMITS_A_NAMED_OPTION_IT_DOES_NOT_SPECIFY,
                    a_report_that_changed_nothing_and_asked(THE_QUESTION_THAT_STOPPED_IT),
                    writing(THE_DESCRIPTIONS_CHOICE),
                ),
                Answering::Always(a_report_that_built_the_option_the_description_suggested()),
                Answering::Always(an_accepted_verdict()),
            ],
            true,
        )
    }

    pub fn start_letting_isp_263_choose_between_its_options() -> Self {
        ToilWorld::built(
            vec![
                Answering::Always(a_review_that_reads(ISP_263_CHOOSES_OPTION_B)),
                support::choosing(
                    vec![
                        (
                            ISP_263_CHOOSES_OPTION_B,
                            writing(OPTION_B_AS_THE_TICKET_SPECIFIES_IT),
                        ),
                        (ISP_263_SUGGESTS_A, writing(OPTION_A_A_RUN_SUBSTITUTED)),
                    ],
                    writing(NEITHER_OPTION_REACHED_THE_IMPLEMENTER),
                ),
                Answering::Always(support::accepted(support::reports(serde_json::json!({
                    "changed_files": ["src/lib.rs"],
                    "summary": "emitted the sample under the name the ticket named",
                    "claimed_complete": true,
                    "quoted_from_a_comment": ISP_263_CHOOSES_OPTION_B,
                })))),
                Answering::Always(an_accepted_verdict()),
            ],
            true,
        )
    }

    pub fn start_over_a_run_that_substitutes_option_a() -> Self {
        ToilWorld::built(
            vec![
                Answering::Always(a_review_that_reads(ISP_263_CHOOSES_OPTION_B)),
                Answering::Always(writing(OPTION_A_A_RUN_SUBSTITUTED)),
                Answering::Always(support::accepted(support::reports(serde_json::json!({
                    "changed_files": ["src/lib.rs"],
                    "summary": THE_RUNS_OWN_ACCOUNT_OF_WHY_IT_BUILT_A,
                    "claimed_complete": true,
                    "quoted_from_a_comment": ISP_263_CHOOSES_OPTION_B,
                    "stopped_by_this_question": THE_OBJECTION_ISP_263_ANSWERS,
                })))),
                Answering::Always(a_verdict_that_rejects(
                    A_FINDING_THAT_NAMES_THE_SUBSTITUTION,
                )),
            ],
            true,
        )
    }

    pub fn start_with_a_gateway_that_obeys_the_tool_choice_it_is_sent() -> Self {
        let mut script = vec![
            Answering::Always(a_review_that_reads_a_change()),
            Answering::Always(support::accepted(support::calls(
                "write_file",
                serde_json::json!({ "path": "src/lib.rs", "contents": REPAIRED }),
            ))),
            Answering::Always(support::accepted(support::reports(serde_json::json!({
                "changed_files": ["src/lib.rs"],
                "summary": "corrected the off-by-one the ticket named",
                "claimed_complete": true,
            })))),
        ];
        for _ in 0..THE_EVALUATIONS_DOCUMENTED_BOUND {
            script.push(support::on_reading(
                A_REQUEST_THAT_OBLIGES_A_TOOL_CALL,
                support::accepted(support::calls(
                    "read_file",
                    serde_json::json!({ "path": "src/lib.rs" }),
                )),
                support::accepted(support::reports(serde_json::json!({
                    "verdict": "accepted",
                }))),
            ));
        }
        ToilWorld::built(script, true)
    }

    pub fn start_with_a_judge_that_reads_before_it_answers() -> Self {
        ToilWorld::built(
            support::always(an_accepted_change_whose_judge_reads_before_it_answers()),
            true,
        )
    }

    pub fn start_reviewing_once() -> Self {
        ToilWorld::serving(vec![a_review_that_reads_the_decision_a_comment_made()])
    }

    pub fn start_paying_for_a_qualification_that_earns_nothing() -> Self {
        ToilWorld::serving(
            std::iter::once(a_review_that_reads_a_change())
                .chain(an_accepted_change())
                .collect(),
        )
    }

    pub fn start_on_a_deployment_that_configured_no_model() -> Self {
        ToilWorld::built(support::always(an_accepted_change()), false)
    }

    fn serving(script: Vec<support::Reply>) -> Self {
        ToilWorld::built(support::always(script), true)
    }

    fn built(script: Vec<Answering>, with_an_agent_table: bool) -> Self {
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
        let gateway = StubGateway::deciding(script);
        let world = ToilWorld {
            forge: ToilForge {
                stub: stub.clone(),
                remote,
            },
            jira,
            gateway,
            scenario,
            fixture: fixture.clone(),
        };
        let tables = world.tables(&fixture, &stub, with_an_agent_table);
        world.scenario.append_config(&tables);
        world
    }

    fn tables(&self, fixture: &Path, stub: &Path, with_an_agent_table: bool) -> String {
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
             {agent}\
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
            agent = match with_an_agent_table {
                false => String::new(),
                true => format!(
                    "[agent]\n\
                     model = \"a-model\"\n\
                     base_url = \"{base_url}\"\n\
                     api_key = {{ env = \"{MODEL_CREDENTIAL}\" }}\n\
                     max_turns = 4\n\
                     max_tokens = 512\n\
                     max_changed_files = 4\n\
                     deadline = \"300s\"\n\
                     tool_timeout = \"300s\"\n\
                     \n",
                    base_url = self.gateway.base_url(),
                ),
            },
            workspaces = support::toml_string(&self.scenario.dir().join("workspaces")),
            fixture = support::toml_string(fixture),
            jira = self.jira.base_url(),
        )
    }

    pub fn authorizes_the_account(&self, account: &str) {
        self.scenario
            .append_config(&format!("[jira.decision]\nauthorized = [\"{account}\"]\n"));
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

    pub fn stamps_its_base_revision(&self, date: &str) {
        let status = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=fixture",
                "-c",
                "user.email=fixture@invalid",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "the base, at a date no clock will read again",
            ])
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date)
            .current_dir(&self.fixture)
            .status()
            .expect("git runs");
        assert!(status.success(), "the fixture takes a dated base commit");
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

    pub fn inspect_human(&self, invocation_ref: &str) -> String {
        let out = self.inspecting(invocation_ref, &[]);
        assert_eq!(
            out.status.code(),
            Some(0),
            "stderr = {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    pub fn inspect_toil(&self, invocation_ref: &str) -> std::process::Output {
        self.inspecting(invocation_ref, &["--json"])
    }

    fn inspecting(&self, invocation_ref: &str, extra: &[&str]) -> std::process::Output {
        let mut command = std::process::Command::new(support::fiddle_binary());
        command.args([
            "inspect",
            invocation_ref,
            "--config",
            self.scenario.config_path().to_str().unwrap(),
        ]);
        command.args(extra);
        for name in support::CREDENTIAL_VARS {
            command.env_remove(name);
        }
        command.env_remove(MODEL_CREDENTIAL);
        command
            .env(JIRA_USER, "nobody@example.com")
            .env(JIRA_TOKEN, JIRA_SENTINEL)
            .output()
            .unwrap()
    }

    fn model_calls(&self) -> usize {
        self.gateway.served()
    }

    fn model_prompts(&self) -> Vec<String> {
        self.gateway.request_bodies()
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

fn findings_of(payload: &serde_json::Value) -> Vec<String> {
    payload["outcome"]["rejected"]["findings"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|finding| finding.as_str())
        .map(str::to_string)
        .collect()
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

fn flattened(text: &str) -> String {
    text.replace('\u{2502}', " ")
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ")
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
        vec![
            BRANCH_EFFECT,
            PULL_REQUEST_EFFECT,
            LINK_EFFECT,
            TRANSITION_EFFECT
        ],
        "the four effect steps of the shipped document ran, in the order it names \
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

const COMBINERS: [&str; 3] = ["oneOf", "allOf", "anyOf"];

fn combiners_at_the_top_of(named: &str, schema: &serde_json::Value) -> Vec<String> {
    COMBINERS
        .iter()
        .filter(|combiner| schema.get(*combiner).is_some())
        .map(|combiner| format!("{named}.{combiner}: {schema}"))
        .collect()
}

fn schemas_sent_in(body: &str) -> Vec<(String, serde_json::Value)> {
    let request: serde_json::Value =
        serde_json::from_str(body).unwrap_or_else(|why| panic!("a request body is JSON: {why}"));
    let mut sent: Vec<(String, serde_json::Value)> = request["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .map(|tool| {
                    (
                        format!("tool {}", tool["function"]["name"]),
                        tool["function"]["parameters"].clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if let Some(schema) = request
        .get("response_format")
        .and_then(|format| format.get("json_schema"))
        .and_then(|json_schema| json_schema.get("schema"))
    {
        sent.push((
            format!(
                "response_format {}",
                request["response_format"]["json_schema"]["name"]
            ),
            schema.clone(),
        ));
    }
    sent
}

#[test]
fn no_schema_a_toil_run_sends_carries_a_combiner_at_the_top_of_itself() {
    let world = ToilWorld::start_with_a_judge_that_reads_before_it_answers();
    world.jira().holds_eligible_ticket(TICKET);

    let run = world.run_toil(REFERENCE);
    let payload = payload_of(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "the run must reach every step, or the schemas below are not the schemas a whole \
         toil run sends: {payload}"
    );

    let bodies = world.model_prompts();
    assert!(
        bodies.len() >= 4,
        "a whole toil run asks the review, the implementer and the judge, and this one sent \
         {} requests: {payload}",
        bodies.len()
    );

    let mut sent = 0;
    let mut structured = 0;
    let mut refused: Vec<String> = Vec::new();
    for (turn, body) in bodies.iter().enumerate() {
        for (named, schema) in schemas_sent_in(body) {
            sent += 1;
            structured += usize::from(named.starts_with("response_format"));
            refused.extend(combiners_at_the_top_of(
                &format!("turn {turn} {named}"),
                &schema,
            ));
        }
    }
    assert!(
        sent > 0,
        "no request carried a schema at all, so an assertion over their shapes proved nothing"
    );
    assert_eq!(
        structured, 2,
        "the implementer's report and the judge's verdict are the two structured-output \
         schemas a toil run sends, and this run sent {structured} of them. A judge that \
         answers on its first turn is never sent one, so this lane would pass over a \
         verdict schema no gateway accepts: {payload}"
    );
    assert!(
        refused.is_empty(),
        "a gateway that fronts Anthropic refuses `oneOf`, `allOf` or `anyOf` at the top of a \
         tool's input_schema, and it lifts `response_format` into a prepended tool, so each \
         of these {} schemas of {sent} makes the whole request a 400 before the model is \
         reached:\n{}",
        refused.len(),
        refused.join("\n")
    );
}

#[test]
fn the_read_only_evaluation_answers_a_gateway_that_obeys_the_tool_choice_the_run_sends_it() {
    let world = ToilWorld::start_with_a_gateway_that_obeys_the_tool_choice_it_is_sent();
    world.jira().holds_eligible_ticket(TICKET);

    let run = world.run_toil(REFERENCE);
    let payload = payload_of(&run);
    let bodies = world.model_prompts();
    let obliged: Vec<usize> = bodies
        .iter()
        .enumerate()
        .filter(|(_, body)| body.contains(A_REQUEST_THAT_OBLIGES_A_TOOL_CALL))
        .map(|(turn, _)| turn)
        .collect();
    let permitted: Vec<usize> = bodies
        .iter()
        .enumerate()
        .filter(|(_, body)| body.contains(A_REQUEST_THAT_PERMITS_AN_ANSWER))
        .map(|(turn, _)| turn)
        .collect();

    assert_eq!(
        run.status.code(),
        Some(0),
        "this gateway answers a request that obliges a tool call with a tool call, which is \
         what an OpenAI-compatible gateway is supposed to do. The evaluation is read-only, so \
         under `required` it can only read again, and the run burns \
         {THE_EVALUATIONS_DOCUMENTED_BOUND} turns without a verdict. It sent {} requests, \
         {} obliging a call and {} permitting an answer: {payload}",
        bodies.len(),
        obliged.len(),
        permitted.len()
    );
    assert_eq!(
        permitted.len(),
        1,
        "exactly one step permits a text answer, and it is the read-only evaluation. The \
         review asks for no tool at all and the implementer is still obliged. {} did: \
         {payload}",
        permitted.len()
    );
    assert!(
        !obliged.is_empty() && obliged.iter().all(|turn| *turn < permitted[0]),
        "and the implementer's turns come before it, so this lane is reading the evaluation's \
         own request and not the review's. Obliged {obliged:?}, permitted {permitted:?}: \
         {payload}"
    );
    assert!(
        bodies.len() < 3 + THE_EVALUATIONS_DOCUMENTED_BOUND,
        "the verdict has to arrive as termination and not as a spent budget, and this run made \
         {} model calls: {payload}",
        bodies.len()
    );
}

#[test]
fn an_authorized_comment_directs_the_change_the_description_suggested_against() {
    let decided = ToilWorld::start_letting_the_ticket_choose_the_change();
    decided.authorizes_the_account(OPERATOR_ACCOUNT);
    decided
        .jira()
        .holds_a_ticket_whose_description_suggests_keeping_the_helper(TICKET);
    decided.jira().is_commented_on_by(OPERATOR_ACCOUNT);

    let run = decided.run_toil(REFERENCE);
    let payload = payload_of(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "the ticket suggests one option, an authorized comment chooses the other, and the \
         run takes it on: {payload}"
    );

    let told = decided.model_prompts();
    let implementer = &told[1];
    assert!(
        implementer.contains(THE_SUGGESTION),
        "the description's own suggestion reached the implementer, or this lane is not the \
         contest it claims to be: {implementer}"
    );
    assert!(
        implementer.contains(THE_DECISION),
        "and so did the comment that overrides it: {implementer}"
    );

    let branch = decided.github().only_branch();
    assert_eq!(
        decided
            .github()
            .file_at(&decided.github().head_of(&branch), "src/lib.rs"),
        THE_COMMENTS_CHOICE.trim_end(),
        "so the change that reached the forge is the one the comment chose, with the \
         description's suggestion in the same prompt and losing to it: {payload}"
    );
}

#[test]
fn with_no_authorized_comment_the_description_directs_the_change_it_suggested() {
    let undecided = ToilWorld::start_letting_the_ticket_choose_the_change();
    undecided.authorizes_the_account(OPERATOR_ACCOUNT);
    undecided
        .jira()
        .holds_a_ticket_whose_description_suggests_keeping_the_helper(TICKET);

    let run = undecided.run_toil(REFERENCE);
    let payload = payload_of(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "the same ticket with nobody commenting on it is eligible on its description \
         alone: {payload}"
    );

    let told = undecided.model_prompts();
    let implementer = &told[1];
    assert!(
        implementer.contains(THE_SUGGESTION),
        "the description's suggestion is what must direct this run, so it has to be in the \
         text the implementer received; a lane that only checks the comment is absent would \
         pass with no description at all: {implementer}"
    );
    assert!(
        !implementer.contains(THE_DECISION),
        "and no decision was written, so none reached it: {implementer}"
    );

    let branch = undecided.github().only_branch();
    let written = undecided
        .github()
        .file_at(&undecided.github().head_of(&branch), "src/lib.rs");
    assert_ne!(
        written,
        NEITHER_TEXT_REACHED_THE_IMPLEMENTER.trim_end(),
        "the implementer was given neither the decision nor the suggestion, so what it \
         built rests on nothing this ticket says: {payload}"
    );
    assert_eq!(
        written,
        THE_DESCRIPTIONS_CHOICE.trim_end(),
        "the change that reached the forge is the one the description suggested, which is \
         what makes the row above the comment directing a change rather than this build \
         writing one thing whatever it reads: {payload}"
    );
}

#[test]
fn a_comment_decides_a_ticket_the_description_leaves_open_and_a_stranger_decides_nothing() {
    let decided = ToilWorld::start_reviewing_a_ticket_a_comment_decided();
    decided.authorizes_the_account(OPERATOR_ACCOUNT);
    decided
        .jira()
        .holds_a_ticket_whose_description_leaves_a_question_open(TICKET);
    decided.jira().is_commented_on_by(OPERATOR_ACCOUNT);

    let run = decided.run_toil(REFERENCE);
    let payload = payload_of(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "the operator answered the open question in a comment and the run took the ticket \
         on: {payload}"
    );
    assert_eq!(
        decided.github().pull_requests().len(),
        1,
        "and it produced the one pull request an eligible ticket produces: {payload}"
    );
    let asked = decided.model_prompts();
    assert!(
        asked[0].contains(THE_DECISION),
        "the first model call is the ambiguity review, and the comment reached it: {}",
        asked[0]
    );
    assert!(
        asked[0].contains("is DATA"),
        "inside the frame that names the quotation data: {}",
        asked[0]
    );

    let stranger = ToilWorld::start_reviewing_once();
    stranger.authorizes_the_account(OPERATOR_ACCOUNT);
    stranger
        .jira()
        .holds_a_ticket_whose_description_leaves_a_question_open(TICKET);
    stranger.jira().is_commented_on_by(A_STRANGER_ACCOUNT);

    let refused = stranger.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&refused.stderr).to_string();
    assert_eq!(
        refused.status.code(),
        Some(2),
        "the same site, the same document and the same words, written by an account the \
         deployment did not authorize, and the ticket is refused: {stderr}"
    );
    assert!(
        stderr.contains(QUOTES_THE_TICKET_RULE),
        "the review rested on a span the ticket does not carry, because the words it rested \
         on never reached it: {stderr}"
    );
    assert!(
        stranger.github().pull_requests().is_empty(),
        "and no pull request was opened: {stderr}"
    );
    let unasked = stranger.model_prompts();
    assert!(
        !unasked[0].contains(THE_DECISION),
        "no word of the unauthorized comment reached the review: {}",
        unasked[0]
    );
    assert_eq!(
        stranger.jira().comment_posts(),
        1,
        "and the refusal is published on the ticket, as every refusal is: {:?}",
        stranger.jira().request_lines()
    );
}

#[test]
fn a_decision_the_ticket_never_specified_refuses_with_the_question_and_reaches_no_evaluation() {
    let stopped = ToilWorld::start_over_a_decision_the_ticket_did_not_specify();
    stopped.authorizes_the_account(OPERATOR_ACCOUNT);
    stopped
        .jira()
        .holds_a_ticket_whose_second_option_it_never_names(TICKET);
    stopped
        .jira()
        .is_commented_on_by_saying(OPERATOR_ACCOUNT, A_DECISION_THE_TICKET_DID_NOT_SPECIFY);

    let run = stopped.run_toil(REFERENCE);
    let payload = payload_of(&run);
    let told = stopped.model_prompts();
    assert!(
        told[1].contains(A_DECISION_THE_TICKET_DID_NOT_SPECIFY),
        "the comment's decision reached the implementer, so what stopped this run is the \
         ticket's silence about the option it chose and not a missing decision: {}",
        told[1]
    );
    let answering = &told[2];
    assert!(
        answering.contains(THE_SCHEMA_ADMITS_A_NAMED_OPTION_IT_DOES_NOT_SPECIFY),
        "the report schema tells the attempt that a ticket which names its option can still \
         fail to specify that option, which is the case ISP-263 was and the case an earlier \
         wording of this field excluded. This world serves the question-naming report only \
         on reading that sentence and builds the option the description suggested without \
         it, so a build whose schema narrows back to a ticket that never chose reds here and \
         reds again at every row below: {answering}"
    );
    assert!(
        answering.contains(THE_TASK_ADMITS_A_NAMED_OPTION_IT_DOES_NOT_SPECIFY),
        "the task the step carries names that same second case, in its own words rather than \
         the schema's, so neither of these two rows can be satisfied by the other surface's \
         sentence: {answering}"
    );
    assert!(
        answering.contains(THE_TASK_FORBIDS_THE_SUBSTITUTION),
        "and the task says the substitution is never open: {answering}"
    );
    assert!(
        answering.contains(THE_PREAMBLE_ADMITS_A_DECIDED_OPTION_CAN_BE_UNSPECIFIED),
        "and the preamble says the thing that can be underspecified is a decided option, so \
         the three surfaces this one request carries name the same case rather than two of \
         them naming a narrower one: {answering}"
    );

    let findings = findings_of(&payload);
    assert_eq!(
        findings.len(),
        1,
        "a run stopped by one question refuses with one finding: {payload}"
    );
    assert!(
        findings[0].contains(THE_QUESTION_THAT_STOPPED_IT),
        "and the finding carries the question the attempt named, word for word, because that \
         sentence is the whole of what a person has to answer: {}",
        findings[0]
    );
    assert_eq!(
        run.status.code(),
        Some(12),
        "and the run refused: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert!(
        !told
            .iter()
            .any(|prompt| prompt.contains(THE_EVALUATION_WAS_ASKED)),
        "the evaluation was never asked, so this refusal is the attempt's own question and \
         not a verdict on an unchanged tree. This world scripts an accepting verdict as its \
         third answer, so a build that ran the evaluation here would have been told the \
         empty tree is fine. The lane below finds this same fragment in a prompt, so its \
         absence here is the evaluation not running and not a fragment nothing carries: \
         {told:?}"
    );
    assert_eq!(
        stopped.model_calls(),
        3,
        "the review, the attempt's read, and the report it answered with. This world scripts \
         two further answers — a report naming a changed file and an accepting verdict — \
         which together carry a run to a published branch, so their staying unserved is the \
         refusal happening rather than the script running out"
    );

    assert!(
        stopped.github().branches().is_empty(),
        "no branch was published: {payload}"
    );
    assert!(
        stopped.github().pull_requests().is_empty(),
        "and no pull request opened, so the run produced no work nobody asked for: {payload}"
    );
    assert!(
        stopped.jira().links_for(TICKET).is_empty(),
        "and the ticket carries no link: {:?}",
        stopped.jira().request_lines()
    );
    assert_eq!(
        stopped.jira().transition_requests(),
        0,
        "and the ticket was not moved: {:?}",
        stopped.jira().request_lines()
    );
    assert!(
        stopped.recorded_marker().is_none(),
        "and the run recorded no completion, so the same ticket is worked again once a person \
         answers the question in a comment: {payload}"
    );
}

#[test]
fn the_option_isp_263s_comment_chose_is_the_one_that_reaches_the_forge() {
    let chosen = ToilWorld::start_letting_isp_263_choose_between_its_options();
    chosen.authorizes_the_account(OPERATOR_ACCOUNT);
    chosen.jira().holds_the_description_isp_263_held(TICKET);
    chosen
        .jira()
        .is_commented_on_by_saying(OPERATOR_ACCOUNT, ISP_263_CHOOSES_OPTION_B);

    let run = chosen.run_toil(REFERENCE);
    let payload = payload_of(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "ISP-263's description weighs two options and suggests one, an authorized comment \
         chooses the other, and the run takes the ticket on rather than refusing it: {payload}"
    );

    let told = chosen.model_prompts();
    let implementer = &told[1];
    assert!(
        implementer.contains(ISP_263_SUGGESTS_A),
        "the description's own suggestion reached the implementer, or this lane is not the \
         contest it claims to be: {implementer}"
    );
    assert!(
        implementer.contains(ISP_263_CHOOSES_OPTION_B),
        "and so did the comment that overrides it: {implementer}"
    );

    assert_eq!(
        chosen.github().pull_requests().len(),
        1,
        "the run produced the one pull request an eligible ticket produces: {payload}"
    );
    let branch = chosen.github().only_branch();
    let written = chosen
        .github()
        .file_at(&chosen.github().head_of(&branch), "src/lib.rs");
    assert_ne!(
        written,
        NEITHER_OPTION_REACHED_THE_IMPLEMENTER.trim_end(),
        "the implementer was given neither option's text, so what it built rests on nothing \
         this ticket says: {payload}"
    );
    assert_ne!(
        written,
        OPTION_A_A_RUN_SUBSTITUTED.trim_end(),
        "and it is not Option A, which is the change the live run of 2026-09-04 made against \
         this same text: {payload}"
    );
    assert_eq!(
        written,
        OPTION_B_AS_THE_TICKET_SPECIFIES_IT.trim_end(),
        "the change that reached the forge is Option B, named by the comment and specified by \
         the description down to the type, the new name and the registration to remove: \
         {payload}"
    );
}

#[test]
fn a_run_that_builds_the_option_the_comment_refused_is_judged_however_it_explains_itself() {
    let substituted = ToilWorld::start_over_a_run_that_substitutes_option_a();
    substituted.authorizes_the_account(OPERATOR_ACCOUNT);
    substituted
        .jira()
        .holds_the_description_isp_263_held(TICKET);
    substituted
        .jira()
        .is_commented_on_by_saying(OPERATOR_ACCOUNT, ISP_263_CHOOSES_OPTION_B);

    let run = substituted.run_toil(REFERENCE);
    let payload = payload_of(&run);
    let told = substituted.model_prompts();
    assert!(
        told.iter()
            .any(|prompt| prompt.contains(THE_EVALUATION_WAS_ASKED)),
        "this attempt named a question and changed a file, so it is a substitution and not a \
         decline, and the evaluation judged it. The lane above gives the same words over an \
         unchanged tree and the evaluation is never asked, so the field is what an attempt \
         says and the tree is what decides which route it takes: {told:?}"
    );

    let findings = findings_of(&payload);
    assert_eq!(
        findings,
        vec![A_FINDING_THAT_NAMES_THE_SUBSTITUTION.to_string()],
        "and the run refuses on what the evaluation read in the project: {payload}"
    );
    assert!(
        !findings
            .iter()
            .any(|finding| finding.contains(THE_OBJECTION_ISP_263_ANSWERS)),
        "and not on the objection the report raised, which ISP-263 answers in the sentence \
         that names merge_graph_size and the type to emit it as: {findings:?}"
    );
    assert_eq!(
        run.status.code(),
        Some(12),
        "so the run does not report success: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert!(
        substituted.github().pull_requests().is_empty(),
        "no pull request carries Option A: {payload}"
    );
    assert!(
        substituted.github().branches().is_empty(),
        "and no branch does: {payload}"
    );
    assert!(
        substituted.jira().links_for(TICKET).is_empty(),
        "and the ticket carries no link to one: {:?}",
        substituted.jira().request_lines()
    );
    assert!(
        substituted.recorded_marker().is_none(),
        "and the run recorded no completion, so a rerun works the ticket again: {payload}"
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

const A_BASE_DATE: &str = "2021-02-03T04:05:06+02:00";

#[test]
fn a_retry_over_a_branch_this_invocation_already_published_reaches_the_effect_tail() {
    let world = ToilWorld::start();
    world.jira().holds_eligible_ticket(TICKET);
    world.stamps_its_base_revision(A_BASE_DATE);

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
        Some(0),
        "the retry reached a terminal state over the branch the first run left \
         standing: {second}"
    );
    assert_eq!(
        second["outcome"], "completed",
        "and the terminal state it reached is completion: {second}"
    );
    assert_eq!(
        effects_of(&second)
            .iter()
            .map(|line| line.split(':').nth(1).unwrap_or_default().to_string())
            .collect::<Vec<String>>(),
        vec![
            BRANCH_EFFECT,
            PULL_REQUEST_EFFECT,
            LINK_EFFECT,
            TRANSITION_EFFECT
        ],
        "so it reached the pull request, the link and the transition rather than \
         stopping at the branch step: {second}"
    );
    assert_eq!(
        external_ref_of(&effect_named(&second, BRANCH_EFFECT)),
        published,
        "the retry rebuilt the same tree and named the same commit, because the commit \
         dates are stamped from the base revision rather than from the wall clock: \
         {second}"
    );
    assert_eq!(
        world.github().head_of(&branch),
        published,
        "and the branch still points there, so nothing was forced over it: {second}"
    );
    assert_eq!(
        world.github().date_of(&format!("{published}^")),
        format!("{A_BASE_DATE}\n{A_BASE_DATE}"),
        "the row's own premise: the revision the workspace was cut at carries a date \
         in 2021, which no wall clock in this run will read"
    );
    assert_eq!(
        world.github().date_of(&published),
        format!("{A_BASE_DATE}\n{A_BASE_DATE}"),
        "and the commit the run published carries that same 2021 committer and author \
         date rather than the second the run fell in, which is why the two runs agree \
         on a sha at all; a clock-dated commit cannot match this by coincidence"
    );
    assert_eq!(
        world.github().pull_requests().len(),
        1,
        "no second pull request was created: {second}"
    );
    assert_eq!(
        world.jira().links_for(TICKET).len(),
        1,
        "and no second link comment reached the ticket: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        world.recorded_marker(),
        Some(world.expected_marker()),
        "and the retry recorded the completion the next run would read, so a third \
         run stops on the marker rather than on the branch: {second}"
    );
}

#[test]
fn a_rerun_whose_tree_changed_publishes_the_commit_that_tree_makes() {
    let world = ToilWorld::start_writing_something_else_the_second_time(REPAIRED_ANOTHER_WAY);
    world.jira().holds_eligible_ticket(TICKET);

    let first = payload_of(&world.run_toil(REFERENCE));
    let branch = world.github().only_branch();
    let published = world.github().head_of(&branch);
    assert_eq!(
        external_ref_of(&effect_named(&first, BRANCH_EFFECT)),
        published,
        "the row's own premise: the first run published the commit it built: {first}"
    );

    world.github().delete_branch(&branch);
    world.forgets_that_the_work_was_completed();

    let rerun = world.run_toil(REFERENCE);
    let second = payload_of(&rerun);

    assert_eq!(
        rerun.status.code(),
        Some(0),
        "the second run reached a terminal state: {second}"
    );
    let republished = world.github().head_of(&branch);
    assert_ne!(
        republished, published,
        "and it published a different commit, because its tree is a different tree; a \
         stamp that froze the sha for every tree would fail here: {second}"
    );
    assert_eq!(
        external_ref_of(&effect_named(&second, BRANCH_EFFECT)),
        republished,
        "the branch step names the commit the forge now holds: {second}"
    );
    assert_eq!(
        world.github().file_at(&republished, "src/lib.rs"),
        REPAIRED_ANOTHER_WAY.trim_end(),
        "and that commit carries the second write rather than the first: {second}"
    );
}

#[test]
fn a_rerun_whose_tree_changed_is_not_forced_over_the_branch_the_first_run_published() {
    let world = ToilWorld::start_writing_something_else_the_second_time(REPAIRED_ANOTHER_WAY);
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

    let rerun = world.run_toil(REFERENCE);
    let second = payload_of(&rerun);

    assert_eq!(
        rerun.status.code(),
        Some(11),
        "the second run built a tree the branch does not carry, so the guard refused \
         it rather than overwriting work it does not recognise: {second}"
    );
    let stopped = second["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| panic!("and this build reports where it stopped: {second}"));
    assert!(
        stopped.contains(BRANCH_EFFECT) && stopped.contains(&branch),
        "it stopped at the branch step, and the step names the branch the first run \
         published: {stopped}"
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
    assert_eq!(
        world.github().pull_requests().len(),
        1,
        "no second pull request was created: {second}"
    );
    assert_eq!(
        world.jira().links_for(TICKET).len(),
        1,
        "and no second link comment reached the ticket: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        world.recorded_marker(),
        None,
        "so this run recorded no completion, which is what a run that reached no \
         terminal state must leave behind: {second}"
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
fn an_eligible_ticket_reaches_in_review_and_a_ticket_the_gate_refuses_reaches_no_status() {
    let opened = ToilWorld::start();
    opened.jira().holds_eligible_ticket(TICKET);
    assert_eq!(
        opened.jira().status_now(),
        READY,
        "the row's own premise: the site holds the ticket in the status the run finds \
         it in"
    );

    let payload = payload_of(&opened.run_toil(REFERENCE));

    assert_eq!(
        opened.github().pull_requests().len(),
        1,
        "the row's own premise: this run opened one pull request, which is what \
         requirement 21 puts the transition after: {payload}"
    );
    assert_eq!(
        opened.jira().transition_requests(),
        1,
        "exactly one transition reached the ticket, counted from the requests the \
         tracker stub received: {:?}",
        opened.jira().request_lines()
    );
    assert_eq!(
        opened.jira().status_now(),
        IN_REVIEW,
        "and the ticket the site holds is In Review, so the count above is a write \
         that landed: {payload}"
    );
    assert_eq!(
        payload
            .pointer("/observations/work_item/available/value/status")
            .and_then(|status| status.as_str()),
        Some(IN_REVIEW),
        "and the run's own post-execution read of the ticket sees the status it set, \
         rather than the one it was qualified at: {payload}"
    );

    let refused = ToilWorld::start();
    refused
        .jira()
        .holds_a_ticket_without_the_trigger_label(TICKET);

    let stderr = String::from_utf8_lossy(&refused.run_toil(REFERENCE).stderr).to_string();

    assert!(
        refused.github().pull_requests().is_empty(),
        "the row's own premise: a ticket the gate refuses opens no pull request: \
         {stderr}"
    );
    assert_eq!(
        refused.jira().transition_requests(),
        0,
        "a run that opened no pull request sent no transition, so the one counted \
         above is not a step that fires whatever the run did: {:?}",
        refused.jira().request_lines()
    );
    assert_eq!(
        refused.jira().status_now(),
        READY,
        "and the ticket is in the status the run found it in: {stderr}"
    );
}

#[test]
fn a_site_that_offers_no_route_to_in_review_fails_the_run_and_records_no_completion() {
    let world = ToilWorld::start();
    world.jira().holds_eligible_ticket(TICKET);
    world.jira().offers_no_route_to_in_review();

    let run = world.run_toil(REFERENCE);
    let payload = payload_of(&run);

    assert_eq!(
        world.github().pull_requests().len(),
        1,
        "the row's own premise: the pull request was opened before the transition was \
         tried: {payload}"
    );
    assert_eq!(
        world.jira().links_for(TICKET).len(),
        1,
        "and the row's own premise: it was linked onto the ticket: {:?}",
        world.jira().request_lines()
    );

    let stopped = payload["outcome"]["retryable"]["reason"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "a document runs to an end or it fails, and a run that left the ticket \
                 behind reports where it stopped rather than a completion: {payload}"
            )
        });
    assert!(
        stopped.contains(TRANSITION_EFFECT) && stopped.contains(IN_REVIEW),
        "the reason names the step that could not be taken and the state it asked \
         for: {stopped}"
    );
    assert!(
        stopped.contains("41 to `Done`"),
        "and it names what this site's workflow does offer, so an operator is told \
         what to change rather than that something went wrong: {stopped}"
    );
    assert_eq!(
        run.status.code(),
        Some(11),
        "so the run reports a state to return to and not success; 0 would be a \
         completion that left the ticket in the status it started in: {payload}"
    );
    assert_eq!(
        world.recorded_marker(),
        None,
        "and it recorded no correlation marker, so a rerun works the ticket again \
         rather than reading this run as done: {payload}"
    );
    assert_eq!(
        world.jira().transition_requests(),
        0,
        "the route was resolved before the write, so the refusal sent nothing to the \
         site: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        world.jira().status_now(),
        READY,
        "and the ticket is in the status the run found it in: {payload}"
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
        vec![
            BRANCH_EFFECT,
            PULL_REQUEST_EFFECT,
            LINK_EFFECT,
            TRANSITION_EFFECT
        ],
        "and it ran all four effect steps a second time, so what follows is what \
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
fn a_ticket_the_deterministic_rules_refuse_is_refused_on_a_deployment_that_configured_no_model() {
    let world = ToilWorld::start_on_a_deployment_that_configured_no_model();
    world
        .jira()
        .holds_a_ticket_without_the_trigger_label(TICKET);

    let run = world.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();

    assert!(
        stderr.contains("fiddle::toil::ineligible"),
        "the trigger label is read off the tracker's own labels array and needs no model, \
         so a deployment that configured none still answers the ticket with a \
         refusal: {stderr}"
    );
    assert!(
        !stderr.contains("fiddle::config::capability_unconfigured"),
        "and it is not told about the missing `[agent]` table, which is the answer this \
         build used to give and which names a deployment fault the person who filed the \
         ticket cannot act on: {stderr}"
    );
    assert!(
        stderr.contains(TRIGGER_LABEL_RULE),
        "and the refusal still names the rule that failed: {stderr}"
    );
    assert_eq!(
        run.status.code(),
        Some(2),
        "a refusal is exit 2 here as it is on a deployment that configured a model: {stderr}"
    );
    assert_eq!(
        world.jira().comment_posts(),
        1,
        "and the person who filed the ticket was told on the ticket: {:?}",
        world.jira().request_lines()
    );
    assert_eq!(
        world.model_calls(),
        0,
        "the deterministic rules reached no model, which is what makes this \
         deployment answerable at all: {stderr}"
    );
}

#[test]
fn a_ticket_the_deterministic_rules_admit_still_needs_the_agent_table() {
    let world = ToilWorld::start_on_a_deployment_that_configured_no_model();
    world.jira().holds_eligible_ticket(TICKET);

    let run = world.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();

    assert!(
        stderr.contains("fiddle::config::capability_unconfigured") && stderr.contains("[agent]"),
        "the nine deterministic rules held, so the ambiguity review is next and it needs \
         a model; the row above is therefore about which rule failed and not about this \
         build having stopped resolving `[agent]` at all: {stderr}"
    );
    assert_eq!(
        world.jira().comment_posts(),
        0,
        "and a deployment fault is not published onto somebody's ticket: {:?}",
        world.jira().request_lines()
    );
}

#[test]
fn inspect_reports_the_refusal_for_a_ticket_the_gate_refuses_and_previews_the_run_for_one_it_admits(
) {
    let refusing = ToilWorld::start();
    refusing
        .jira()
        .holds_a_ticket_without_the_trigger_label(TICKET);

    let out = refusing.inspect_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(
        out.status.code(),
        Some(0),
        "inspect reads and reports, so it exits 0 over a ticket `run` would refuse, and \
         it resolved no model credential because this invocation exported none: {stderr}"
    );
    let payload = payload_of(&out);
    let reason = payload["would_refuse"].as_str().unwrap_or_else(|| {
        panic!(
            "inspect no longer previews a run the gate refuses in silence: {}",
            payload["would_refuse"]
        )
    });
    assert!(
        reason.contains(TRIGGER_LABEL_RULE) && reason.contains(TICKET),
        "and it names the rule the gate would fail and the ticket it read it off: {reason}"
    );
    assert_eq!(
        payload["next_action"]["execute"]["capability_id"], "toil",
        "and it still names the capability `run` selects, because what `run` refuses is \
         this ticket and not the capability: {}",
        payload["next_action"]
    );
    assert_eq!(
        refusing.model_calls(),
        0,
        "inspect stays credential-free: it reached no model: {stderr}"
    );
    let human = refusing.inspect_human(REFERENCE);
    assert!(
        human.contains("would refuse =") && human.contains(TRIGGER_LABEL_RULE),
        "and a reader of the human rendering is told the same thing: {human}"
    );

    let admitting = ToilWorld::start();
    admitting.jira().holds_eligible_ticket(TICKET);

    let out = admitting.inspect_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert_eq!(
        out.status.code(),
        Some(0),
        "and a labelled ticket is inspected the same way: {stderr}"
    );
    let payload = payload_of(&out);
    assert_eq!(
        payload["next_action"]["execute"]["capability_id"], "toil",
        "a ticket the deterministic rules admit still previews the run: {}",
        payload["next_action"]
    );
    assert_eq!(
        payload["would_refuse"],
        serde_json::Value::Null,
        "and nothing says the run would refuse it, so the row above is not `inspect` \
         reporting a refusal for every jira reference: {}",
        payload["would_refuse"]
    );
    assert_eq!(
        admitting.model_calls(),
        0,
        "and it too reached no model: {stderr}"
    );
    let human = admitting.inspect_human(REFERENCE);
    assert!(
        human.contains("next action = execute toil") && !human.contains("would refuse"),
        "and the human rendering carries no refusal line at all: {human}"
    );
}

#[test]
fn the_command_line_and_the_ticket_comment_state_one_refusal_the_same_way() {
    let world = ToilWorld::start();
    world
        .jira()
        .holds_a_ticket_without_the_trigger_label(TICKET);

    let run = world.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    let comment = world
        .jira()
        .last_comment_on(TICKET)
        .expect("the refusal was published");

    let printed = flattened(&stderr);
    let published = flattened(&comment);
    let stated = format!("The rule that failed: {TRIGGER_LABEL_RULE}");
    assert!(
        printed.contains(&stated),
        "the command line frames the failed rule as the one that failed: {stderr}"
    );
    assert!(
        published.contains(&stated),
        "and the comment on the ticket says the same sentence, so the two surfaces cannot \
         diverge again: {comment}"
    );
    assert!(
        !printed.contains(&format!("takes on: {TRIGGER_LABEL_RULE} —")),
        "and the rendering that spliced the failed rule into a bare failure sentence, so \
         that every refusal asserted the rule it failed had held, is gone: {stderr}"
    );
    let found = format!("What the gate found: {TICKET} carries 1 labels");
    assert!(
        printed.contains(&found) && published.contains(&found),
        "both surfaces name the finding the same way: {stderr} / {comment}"
    );
    assert!(
        published.contains(&format!(
            "What would change that: add the label `{TRIGGER_LABEL}`"
        )),
        "and the remedy the ticket is given is the one the command line prints as its \
         help: {comment}"
    );
    assert!(
        printed.contains(&format!("add the label `{TRIGGER_LABEL}` to {TICKET}")),
        "which the command line prints too: {stderr}"
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

const UNREADABLE_STATUS: u16 = 503;

const A_MALFORMED_REFERENCE: &str = "jira:";

const AN_OBSTACLE_EXIT: i32 = 11;

const AN_INVALID_INPUT_EXIT: i32 = 2;

#[test]
fn a_tracker_that_cannot_be_read_exits_on_the_obstacle_row_and_a_malformed_reference_does_not() {
    let unreadable = ToilWorld::start();
    unreadable
        .jira()
        .answers_every_read_with(UNREADABLE_STATUS, TICKET);

    let obstructed = unreadable.run_toil(REFERENCE);
    let obstructed_stderr = String::from_utf8_lossy(&obstructed.stderr).to_string();

    assert!(
        unreadable.jira().issue_read_requests() > 0,
        "the run has to have asked the tracker for the ticket, or this exit code is one \
         it reached without ever meeting the obstacle: {:?}",
        unreadable.jira().request_lines()
    );
    assert_eq!(
        obstructed.status.code(),
        Some(AN_OBSTACLE_EXIT),
        "a tracker that answers {UNREADABLE_STATUS} is an obstacle in front of the \
         request, and an orchestrator reads exit {AN_OBSTACLE_EXIT} as one it may send \
         again: {obstructed_stderr}"
    );
    assert!(
        obstructed_stderr.contains(TICKET),
        "and the operator is told which ticket could not be read: {obstructed_stderr}"
    );
    assert!(
        obstructed_stderr.contains("could not be read"),
        "and that reading it is what failed: {obstructed_stderr}"
    );
    assert!(
        obstructed_stderr.contains(&UNREADABLE_STATUS.to_string()),
        "and the status the site answered, so the reason is the tracker's own and not a \
         guess: {obstructed_stderr}"
    );
    assert_eq!(
        unreadable.model_calls(),
        0,
        "the gate stopped at the unread ticket, so no model was paid to qualify \
         nothing: {obstructed_stderr}"
    );

    let malformed = ToilWorld::start();
    malformed.jira().holds_eligible_ticket(TICKET);

    let rejected = malformed.run_toil(A_MALFORMED_REFERENCE);
    let rejected_stderr = String::from_utf8_lossy(&rejected.stderr).to_string();

    assert_eq!(
        malformed.jira().issue_read_requests(),
        0,
        "a reference this build cannot parse reaches no tracker, which is what makes it \
         a different failure from the one above: {:?}",
        malformed.jira().request_lines()
    );
    assert_eq!(
        rejected.status.code(),
        Some(AN_INVALID_INPUT_EXIT),
        "and `{A_MALFORMED_REFERENCE}` is invalid input before a run begins, which no \
         retry corrects: {rejected_stderr}"
    );
    assert_ne!(
        obstructed.status.code(),
        rejected.status.code(),
        "the two must not share an exit code, or an orchestrator retrying the transient \
         one also retries the malformed one and an orchestrator abandoning the malformed \
         one also abandons the transient one"
    );
}

#[test]
fn the_same_tracker_read_that_refuses_succeeds_when_the_site_answers() {
    let answering = ToilWorld::start();
    answering.jira().holds_eligible_ticket(TICKET);

    let ran = answering.run_toil(REFERENCE);
    let stderr = String::from_utf8_lossy(&ran.stderr).to_string();

    assert_eq!(
        ran.status.code(),
        Some(0),
        "this world differs from the unreadable one only in what the tracker answers, so \
         a run that fails here would make the exit 11 above a fact about the harness \
         rather than about the read: {stderr}"
    );
    assert!(
        answering.jira().issue_read_requests() > 0,
        "and it read the ticket over the same route the refusing site refused: {:?}",
        answering.jira().request_lines()
    );
}
