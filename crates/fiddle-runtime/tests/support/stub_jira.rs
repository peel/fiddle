use fiddle_runtime::jira::JiraHttp;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub const USER: &str = "bot@example.com";
pub const TOKEN: &str = "s3cr3t";
pub const ENCODED: &str = "Ym90QGV4YW1wbGUuY29tOnMzY3IzdA==";
pub const ISSUE_ROUTE: &str = "/rest/api/3/issue/";
pub const CREATE_ROUTE: &str = "/rest/api/3/issue";
pub const SEARCH_ROUTE: &str = "/rest/api/3/search/jql";
pub const ISSUE: &str = "/rest/api/3/issue/IDENT-1";
pub const MYSELF: &str = "/rest/api/3/myself";
pub const PATIENT: Duration = Duration::from_secs(30);
pub const HELD_DAY: &str = "2026-08-26";
pub const SEEDED_PROJECT: &str = "IDENT";

const SETTLES: Duration = Duration::from_millis(100);
const PAGE_CAP: usize = 50;
const PAGE_WALK_BOUND: usize = 1000;

const ABSENT: &str = r#"{"errorMessages":["Issue does not exist or you do not have permission to see it."],"errors":{}}"#;
const REFUSED: &str = r#"{"errorMessages":["the site refused this request"],"errors":{}}"#;
const UNROUTED: &str =
    r#"{"errorMessages":["the site serves no resource at that path"],"errors":{}}"#;
const NOT_ALLOWED: &str =
    r#"{"errorMessages":["the site does not serve that method here"],"errors":{}}"#;
const UNPARSED: &str = r#"{"errorMessages":["the request line could not be parsed"],"errors":{}}"#;
const NO_LENGTH: &str =
    r#"{"errorMessages":["a body must arrive with a content length"],"errors":{}}"#;
const UNCHECKED: &str =
    r#"{"errorMessages":["the site could not say whether this credential is good"],"errors":{}}"#;
const WHO: &str = r#"{"accountId":"5b10a2844c20165700ede21g","displayName":"the bot"}"#;
const HTML_REFUSAL: &str = "<!DOCTYPE html><html><head><title>Sign in</title></head><body>\
                            <h1>You are not authenticated</h1></body></html>";

const JSON: &str = "application/json";
const HTML: &str = "text/html;charset=UTF-8";

enum Credential {
    Accepted,
    Refused,
    Unreadable(u16),
}

enum Answer {
    Body(String),
    Issue { path: String, body: String },
    Absent,
    Refusal { status: u16, body: String },
    HtmlRefusal(u16),
}

struct Served {
    status: u16,
    body: String,
    content_type: &'static str,
}

impl Served {
    fn json(status: u16, body: &str) -> Self {
        Served {
            status,
            body: body.to_string(),
            content_type: JSON,
        }
    }

    fn refusal(status: u16, message: &str) -> Self {
        Served::json(status, &refusal(message))
    }
}

enum Reply {
    Answered(Served),
    Unanswered,
}

enum Length {
    Given(usize),
    Absent,
    Chunked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteRoute {
    CreateIssue,
    EditIssue,
    AddComment,
    TransitionIssue,
}

#[derive(Clone, Debug)]
pub struct RecordedWrite {
    pub route: WriteRoute,
    pub issue: String,
    pub body: Value,
    pub committed: bool,
}

#[derive(Clone, Debug)]
pub struct Answered {
    pub status: u16,
    pub body: Value,
}

struct StoredIssue {
    id: String,
    key: String,
    fields: Value,
    found_by_search: bool,
}

impl StoredIssue {
    fn body(&self, base_url: &str) -> Value {
        let mut fields = self.fields.clone();
        if fields["comment"].is_null() {
            merged(&mut fields, &commentless());
        }
        json!({
            "id": self.id,
            "key": self.key,
            "self": format!("{base_url}{ISSUE_ROUTE}{}", self.key),
            "fields": fields,
        })
    }
}

fn commentless() -> Value {
    json!({"comment": {"comments": [], "maxResults": 0, "total": 0, "startAt": 0}})
}

fn with_comment(fields: &mut Value, id: &str, body: &Value) {
    let mut comments = fields["comment"]["comments"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    comments.push(json!({"id": id, "body": body.clone()}));
    let total = comments.len();
    merged(
        fields,
        &json!({
            "comment": {
                "comments": comments,
                "maxResults": total,
                "total": total,
                "startAt": 0,
            }
        }),
    );
}

enum WriteAnswer {
    Sent,
    LostAfterTheWriteCommits,
}

enum SearchIndex {
    Current,
    LaggingBehindNewIssues,
}

struct IssuedPageToken {
    token: String,
    jql: String,
    offset: usize,
}

struct OfferedTransition {
    issue: String,
    id: String,
    leads_to: String,
}

struct StubState {
    base_url: String,
    answer: Answer,
    credential: Credential,
    silent: bool,
    authorizations: Vec<String>,
    request_lines: Vec<String>,
    issues: Vec<StoredIssue>,
    writes: Vec<RecordedWrite>,
    write_answer: WriteAnswer,
    search_index: SearchIndex,
    offered: Vec<OfferedTransition>,
    page_cap: usize,
    page_tokens: Vec<IssuedPageToken>,
    minted: u32,
    ticks: u32,
}

impl StubState {
    fn holding(&mut self, key: &str) -> Option<&mut StoredIssue> {
        self.issues.iter_mut().find(|issue| issue.key == key)
    }

    fn holds(&self, key: &str) -> bool {
        self.issues.iter().any(|issue| issue.key == key)
    }

    fn stamp(&mut self) -> String {
        self.ticks += 1;
        let elapsed = self.ticks % 3600;
        format!(
            "{HELD_DAY}T09:{:02}:{:02}.000+0000",
            elapsed / 60,
            elapsed % 60
        )
    }

    fn mint(&mut self, project: &str) -> String {
        loop {
            self.minted += 1;
            let key = format!("{project}-{}", self.minted);
            if !self.holds(&key) {
                return key;
            }
        }
    }

    fn seed(&mut self, key: &str, fields: Value) {
        let id = format!("1{:04}", self.issues.len() + 1);
        self.issues.push(StoredIssue {
            id,
            key: key.to_string(),
            fields,
            found_by_search: true,
        });
    }

    fn record(&mut self, route: WriteRoute, issue: &str, body: &Value, committed: bool) {
        self.writes.push(RecordedWrite {
            route,
            issue: issue.to_string(),
            body: body.clone(),
            committed,
        });
    }

    fn wrote(&self, route: WriteRoute) -> usize {
        self.writes
            .iter()
            .filter(|write| write.route == route)
            .count()
    }
}

pub struct StubJira {
    address: String,
    base_url: String,
    state: Arc<Mutex<StubState>>,
    cancel: CancellationToken,
}

impl StubJira {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let address = listener
            .local_addr()
            .expect("an accepted listener has an address")
            .to_string();
        let base_url = format!("http://{address}");
        let state = Arc::new(Mutex::new(StubState {
            base_url: base_url.clone(),
            answer: Answer::Body(r#"{"key":"IDENT-1"}"#.to_string()),
            credential: Credential::Accepted,
            silent: false,
            authorizations: Vec::new(),
            request_lines: Vec::new(),
            issues: Vec::new(),
            writes: Vec::new(),
            write_answer: WriteAnswer::Sent,
            search_index: SearchIndex::Current,
            offered: Vec::new(),
            page_cap: PAGE_CAP,
            page_tokens: Vec::new(),
            minted: 0,
            ticks: 0,
        }));
        let cancel = CancellationToken::new();
        let accepting = (state.clone(), cancel.clone());
        tokio::spawn(async move {
            let (state, cancel) = accepting;
            loop {
                let accepted = tokio::select! {
                    _ = cancel.cancelled() => return,
                    accepted = listener.accept() => accepted,
                };
                match accepted {
                    Ok((socket, _)) => {
                        tokio::spawn(answer(socket, state.clone(), cancel.clone()));
                    }
                    Err(_) => return,
                }
            }
        });
        Self {
            base_url,
            address,
            state,
            cancel,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    pub fn site(&self) -> &str {
        &self.address
    }

    pub async fn holds_issue(
        &self,
        key: &str,
        status_id: &str,
        status_name: &str,
        status_category: &str,
        updated_hour: u32,
    ) {
        self.holds_issue_updated_at(
            key,
            status_id,
            status_name,
            status_category,
            &format!("{HELD_DAY}T{updated_hour:02}:00:00.000+0000"),
        )
        .await
    }

    pub async fn holds_issue_updated_at(
        &self,
        key: &str,
        status_id: &str,
        status_name: &str,
        status_category: &str,
        updated: &str,
    ) {
        let body = issue(
            &self.base_url,
            key,
            status_id,
            status_name,
            status_category,
            updated,
        );
        self.state.lock().await.answer = Answer::Issue {
            path: format!("{ISSUE_ROUTE}{key}"),
            body,
        };
    }

    pub async fn holds_nothing(&self) {
        self.state.lock().await.answer = Answer::Absent;
    }

    pub async fn refuses_the_credential_and_so_answers_no_issue(&self) {
        let mut held = self.state.lock().await;
        held.credential = Credential::Refused;
        held.answer = Answer::Absent;
    }

    pub async fn cannot_check_the_credential_and_answers_no_issue(&self, status: u16) {
        let mut held = self.state.lock().await;
        held.credential = Credential::Unreadable(status);
        held.answer = Answer::Absent;
    }

    pub async fn answer_with_body(&self, body: &str) {
        self.state.lock().await.answer = Answer::Body(body.to_string());
    }

    pub async fn refuses_with(&self, status: u16) {
        self.refuses_with_body(status, REFUSED).await
    }

    pub async fn refuses_with_body(&self, status: u16, body: &str) {
        self.state.lock().await.answer = Answer::Refusal {
            status,
            body: body.to_string(),
        };
    }

    pub async fn refuses_in_html_with(&self, status: u16) {
        self.state.lock().await.answer = Answer::HtmlRefusal(status);
    }

    pub async fn stays_silent(&self) {
        self.state.lock().await.silent = true;
    }

    pub async fn last_authorization(&self) -> String {
        self.state
            .lock()
            .await
            .authorizations
            .last()
            .cloned()
            .expect("the stub was asked something")
    }

    pub async fn request_lines(&self) -> Vec<String> {
        self.state.lock().await.request_lines.clone()
    }

    pub async fn loses_the_answer_to_a_committed_write(&self) {
        self.state.lock().await.write_answer = WriteAnswer::LostAfterTheWriteCommits;
    }

    pub async fn answers_every_committed_write(&self) {
        self.state.lock().await.write_answer = WriteAnswer::Sent;
    }

    pub async fn withholds_new_issues_from_search(&self) {
        self.state.lock().await.search_index = SearchIndex::LaggingBehindNewIssues;
    }

    pub async fn admits_the_withheld_issues_to_search(&self) {
        let mut held = self.state.lock().await;
        held.search_index = SearchIndex::Current;
        for issue in held.issues.iter_mut() {
            issue.found_by_search = true;
        }
    }

    pub async fn holds_issue_labelled(&self, key: &str, labels: &[&str]) {
        let updated = self.state.lock().await.stamp();
        self.state.lock().await.seed(
            key,
            json!({
                "project": {"key": project_of(key)},
                "summary": format!("a seeded issue {key}"),
                "labels": labels,
                "updated": updated,
            }),
        );
    }

    pub async fn holds_two_issues_labelled(&self, label: &str) {
        self.holds_issue_labelled(&format!("{SEEDED_PROJECT}-901"), &[label])
            .await;
        self.holds_issue_labelled(&format!("{SEEDED_PROJECT}-902"), &[label])
            .await;
    }

    pub async fn offers_transition(&self, key: &str, id: &str, leads_to: &str) {
        self.state.lock().await.offered.push(OfferedTransition {
            issue: key.to_string(),
            id: id.to_string(),
            leads_to: leads_to.to_string(),
        });
    }

    pub async fn caps_search_pages_at(&self, issues: usize) {
        assert!(issues > 0, "a page holds at least one issue");
        self.state.lock().await.page_cap = issues;
    }

    pub async fn writes(&self) -> Vec<RecordedWrite> {
        self.state.lock().await.writes.clone()
    }

    pub async fn create_requests(&self) -> usize {
        self.state.lock().await.wrote(WriteRoute::CreateIssue)
    }

    pub async fn edit_requests(&self) -> usize {
        self.state.lock().await.wrote(WriteRoute::EditIssue)
    }

    pub async fn comment_requests(&self) -> usize {
        self.state.lock().await.wrote(WriteRoute::AddComment)
    }

    pub async fn transition_requests(&self) -> usize {
        self.state.lock().await.wrote(WriteRoute::TransitionIssue)
    }

    pub async fn issues_that_exist(&self) -> usize {
        self.state.lock().await.issues.len()
    }

    pub async fn last_create(&self) -> Value {
        self.last_write(WriteRoute::CreateIssue)
            .await
            .expect("the stub was asked to create an issue")
    }

    pub async fn comment_requests_on(&self, key: &str) -> usize {
        self.state
            .lock()
            .await
            .writes
            .iter()
            .filter(|write| write.route == WriteRoute::AddComment && write.issue == key)
            .count()
    }

    pub async fn last_comment_on(&self, key: &str) -> Value {
        self.state
            .lock()
            .await
            .writes
            .iter()
            .rfind(|write| write.route == WriteRoute::AddComment && write.issue == key)
            .map(|write| write.body.clone())
            .unwrap_or_else(|| panic!("the stub was asked to comment on {key}"))
    }

    pub async fn issue_keys(&self) -> Vec<String> {
        self.state
            .lock()
            .await
            .issues
            .iter()
            .map(|issue| issue.key.clone())
            .collect()
    }

    async fn last_write(&self, route: WriteRoute) -> Option<Value> {
        self.state
            .lock()
            .await
            .writes
            .iter()
            .rfind(|write| write.route == route)
            .map(|write| write.body.clone())
    }

    pub async fn attempt(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Result<Answered, String> {
        let method =
            reqwest::Method::from_bytes(method.as_bytes()).map_err(|error| error.to_string())?;
        let mut request =
            reqwest::Client::new().request(method, format!("{}{path}", self.base_url));
        if let Some(body) = body {
            request = request.json(&body);
        }
        answered_by(request).await
    }

    pub async fn post_issue(&self, body: Value) -> Answered {
        self.attempt("POST", CREATE_ROUTE, Some(body))
            .await
            .expect("the stub answers the create")
    }

    pub async fn put_issue(&self, key: &str, body: Value) -> Answered {
        self.attempt("PUT", &format!("{ISSUE_ROUTE}{key}"), Some(body))
            .await
            .expect("the stub answers the edit")
    }

    pub async fn post_comment(&self, key: &str, body: Value) -> Answered {
        self.attempt("POST", &format!("{ISSUE_ROUTE}{key}/comment"), Some(body))
            .await
            .expect("the stub answers the comment")
    }

    pub async fn post_transition(&self, key: &str, body: Value) -> Answered {
        self.attempt(
            "POST",
            &format!("{ISSUE_ROUTE}{key}/transitions"),
            Some(body),
        )
        .await
        .expect("the stub answers the transition")
    }

    pub async fn get_issue(&self, key: &str) -> Answered {
        self.attempt("GET", &format!("{ISSUE_ROUTE}{key}"), None)
            .await
            .expect("the stub answers the read")
    }

    pub async fn search_answer_with(&self, params: &[(&str, &str)]) -> Answered {
        let query: Vec<String> = params
            .iter()
            .map(|(name, value)| format!("{name}={}", percent_encoded(value)))
            .collect();
        let request = reqwest::Client::new().get(format!(
            "{}{SEARCH_ROUTE}?{}",
            self.base_url,
            query.join("&")
        ));
        answered_by(request)
            .await
            .expect("the stub answers the search")
    }

    pub async fn search_answer(&self, jql: &str) -> Answered {
        self.search_answer_with(&[("jql", jql)]).await
    }

    pub async fn search_page_answer_after(&self, jql: &str, token: &str) -> Answered {
        self.search_answer_with(&[("jql", jql), ("nextPageToken", token)])
            .await
    }

    pub async fn search_page(&self, jql: &str) -> Value {
        served_page(jql, self.search_answer(jql).await)
    }

    pub async fn search_page_after(&self, jql: &str, token: &str) -> Value {
        served_page(jql, self.search_page_answer_after(jql, token).await)
    }

    pub async fn all_search_matches(&self, jql: &str) -> Vec<Value> {
        let mut matched = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..PAGE_WALK_BOUND {
            let page = match &token {
                None => self.search_page(jql).await,
                Some(held) => self.search_page_after(jql, held).await,
            };
            matched.extend(page["issues"].as_array().cloned().unwrap_or_default());
            match page["nextPageToken"].as_str() {
                Some(next) => token = Some(next.to_string()),
                None => return matched,
            }
        }
        panic!("the search for `{jql}` never reported a last page within {PAGE_WALK_BOUND} pages");
    }
}

fn served_page(jql: &str, answered: Answered) -> Value {
    assert_eq!(
        answered.status, 200,
        "the stub refused the search for `{jql}`, so a count taken from this answer would be a \
         count of nothing rather than a count of matches: {}",
        answered.body
    );
    answered.body
}

impl Drop for StubJira {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

pub fn client_for(server: &StubJira) -> JiraHttp {
    client_waiting(server, PATIENT)
}

pub fn client_waiting(server: &StubJira, timeout: Duration) -> JiraHttp {
    JiraHttp::new(server.base_url(), USER, TOKEN, timeout).expect("the client builds")
}

async fn answered_by(request: reqwest::RequestBuilder) -> Result<Answered, String> {
    let answered = request.send().await.map_err(|error| error.to_string())?;
    let status = answered.status().as_u16();
    let text = answered.text().await.map_err(|error| error.to_string())?;
    Ok(Answered {
        status,
        body: serde_json::from_str(&text).unwrap_or(Value::Null),
    })
}

fn refusal(message: &str) -> String {
    json!({"errorMessages": [message], "errors": {}}).to_string()
}

fn project_of(key: &str) -> &str {
    key.split_once('-')
        .map(|(project, _)| project)
        .unwrap_or(key)
}

fn issue(
    base_url: &str,
    key: &str,
    status_id: &str,
    status_name: &str,
    status_category: &str,
    updated: &str,
) -> String {
    let (category_id, category_key) = category(status_category);
    serde_json::json!({
        "id": "10000",
        "key": key,
        "self": format!("{base_url}{ISSUE_ROUTE}{key}"),
        "fields": {
            "updated": updated,
            "status": {
                "id": status_id,
                "name": status_name,
                "self": format!("{base_url}/rest/api/3/status/{status_id}"),
                "statusCategory": {
                    "id": category_id,
                    "key": category_key,
                    "name": status_category,
                },
            },
        },
    })
    .to_string()
}

fn category(name: &str) -> (u32, &'static str) {
    match name {
        "To Do" => (2, "new"),
        "In Progress" => (4, "indeterminate"),
        "Done" => (3, "done"),
        _ => (1, "undefined"),
    }
}

async fn answer(mut socket: TcpStream, state: Arc<Mutex<StubState>>, cancel: CancellationToken) {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        match socket.read(&mut byte).await {
            Ok(0) | Err(_) => return,
            Ok(_) => head.push(byte[0]),
        }
    }
    let head = String::from_utf8_lossy(&head).to_string();
    let mut lines = head.lines();
    let request_line = lines.next().unwrap_or_default().to_string();
    let mut authorization = String::new();
    let mut length = Length::Absent;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.to_ascii_lowercase().as_str() {
            "authorization" => authorization = value.trim().to_string(),
            "content-length" => length = Length::Given(value.trim().parse().unwrap_or(0)),
            "transfer-encoding" => length = Length::Chunked,
            _ => {}
        }
    }

    let silent = {
        let mut held = state.lock().await;
        held.authorizations.push(authorization);
        held.request_lines.push(request_line.clone());
        held.silent
    };

    if silent {
        cancel.cancelled().await;
        return;
    }

    let sent = match measured(&request_line, length) {
        Some(length) => {
            let mut raw = vec![0u8; length];
            if length > 0 && socket.read_exact(&mut raw).await.is_err() {
                return;
            }
            serde_json::from_slice(&raw).unwrap_or(Value::Null)
        }
        None => {
            let mut unmeasured = vec![0u8; 8192];
            let _ = tokio::time::timeout(SETTLES, socket.read(&mut unmeasured)).await;
            let Served {
                status,
                body,
                content_type,
            } = Served::json(411, NO_LENGTH);
            let _ = socket
                .write_all(rendered(status, &body, content_type).as_bytes())
                .await;
            let _ = socket.flush().await;
            return;
        }
    };

    let reply = {
        let mut held = state.lock().await;
        routed(&request_line, &sent, &mut held)
    };

    let Reply::Answered(Served {
        status,
        body,
        content_type,
    }) = reply
    else {
        return;
    };
    let _ = socket
        .write_all(rendered(status, &body, content_type).as_bytes())
        .await;
    let _ = socket.flush().await;
}

fn rendered(status: u16, body: &str, content_type: &'static str) -> String {
    format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        reason(status),
        body.len()
    )
}

fn measured(request_line: &str, length: Length) -> Option<usize> {
    match length {
        Length::Given(length) => Some(length),
        Length::Chunked => None,
        Length::Absent => match request_line.split_whitespace().next() {
            Some("POST") | Some("PUT") | Some("PATCH") => None,
            _ => Some(0),
        },
    }
}

fn routed(request_line: &str, sent: &Value, state: &mut StubState) -> Reply {
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Reply::Answered(Served::json(400, UNPARSED));
    };
    if !version.starts_with("HTTP/") || !target.starts_with('/') {
        return Reply::Answered(Served::json(400, UNPARSED));
    }
    let path = target.split('?').next().unwrap_or(target);
    if !matches!(method, "GET" | "PUT" | "POST") {
        return Reply::Answered(Served::json(405, NOT_ALLOWED));
    }
    if path == MYSELF {
        return Reply::Answered(match state.credential {
            Credential::Accepted => Served::json(200, WHO),
            Credential::Refused => Served::json(401, REFUSED),
            Credential::Unreadable(status) => Served::json(status, UNCHECKED),
        });
    }
    if matches!(
        state.answer,
        Answer::Refusal { .. } | Answer::HtmlRefusal(_)
    ) {
        return Reply::Answered(scripted(&state.answer, path));
    }
    if method == "GET" && path == SEARCH_ROUTE {
        return Reply::Answered(searched(target, state));
    }
    if method == "POST" && path == CREATE_ROUTE {
        return committed(created(sent, state), state);
    }
    if method == "PUT" {
        if let Some(key) = issue_key(path) {
            if state.holds(key) {
                return committed(edited(key, sent, state), state);
            }
            state.record(WriteRoute::EditIssue, key, sent, false);
        }
    }
    if method == "POST" {
        if let Some(key) = issue_sub(path, "comment") {
            if state.holds(key) {
                return committed(commented(key, sent, state), state);
            }
            state.record(WriteRoute::AddComment, key, sent, false);
            return Reply::Answered(Served::json(404, ABSENT));
        }
        if let Some(key) = issue_sub(path, "transitions") {
            if state.holds(key) {
                return committed(transitioned(key, sent, state), state);
            }
            state.record(WriteRoute::TransitionIssue, key, sent, false);
            return Reply::Answered(Served::json(404, ABSENT));
        }
    }
    if method == "GET" {
        if let Some(key) = issue_key(path) {
            if let Some(held) = state.issues.iter().find(|issue| issue.key == key) {
                let body = held.body(&state.base_url).to_string();
                return Reply::Answered(Served::json(200, &body));
            }
        }
    }
    if method == "POST" {
        return Reply::Answered(Served::json(404, UNROUTED));
    }
    Reply::Answered(scripted(&state.answer, path))
}

fn scripted(answer: &Answer, path: &str) -> Served {
    match answer {
        Answer::Refusal { status, body } => Served::json(*status, body),
        Answer::HtmlRefusal(status) => Served {
            status: *status,
            body: HTML_REFUSAL.to_string(),
            content_type: HTML,
        },
        Answer::Absent => Served::json(404, &absent_or_unrouted(path)),
        Answer::Issue { path: held, body } => match path == held {
            true => Served::json(200, body),
            false => Served::json(404, &absent_or_unrouted(path)),
        },
        Answer::Body(body) => match names_an_issue(path) {
            true => Served::json(200, body),
            false => Served::json(404, UNROUTED),
        },
    }
}

fn committed(served: Served, state: &StubState) -> Reply {
    match (&state.write_answer, served.status) {
        (WriteAnswer::LostAfterTheWriteCommits, 200..=299) => Reply::Unanswered,
        _ => Reply::Answered(served),
    }
}

fn created(sent: &Value, state: &mut StubState) -> Served {
    let Some(project) = sent["fields"]["project"]["key"].as_str().map(String::from) else {
        state.record(WriteRoute::CreateIssue, "", sent, false);
        return Served::refusal(400, "a create must name fields.project.key");
    };
    if !sent["fields"]["labels"].is_null() && !sent["fields"]["labels"].is_array() {
        state.record(WriteRoute::CreateIssue, "", sent, false);
        return Served::refusal(400, "fields.labels must be an array of strings");
    }
    let key = state.mint(&project);
    let updated = state.stamp();
    let mut fields = sent["fields"].clone();
    merged(&mut fields, &json!({"updated": updated}));
    let found_by_search = matches!(state.search_index, SearchIndex::Current);
    let id = format!("1{:04}", state.issues.len() + 1);
    state.issues.push(StoredIssue {
        id: id.clone(),
        key: key.clone(),
        fields,
        found_by_search,
    });
    state.record(WriteRoute::CreateIssue, &key, sent, true);
    let body = json!({
        "id": id,
        "key": key,
        "self": format!("{}{ISSUE_ROUTE}{key}", state.base_url),
    })
    .to_string();
    Served::json(201, &body)
}

fn edited(key: &str, sent: &Value, state: &mut StubState) -> Served {
    if !sent["fields"].is_object() {
        state.record(WriteRoute::EditIssue, key, sent, false);
        return Served::refusal(400, "an edit must carry a fields object");
    }
    let updated = state.stamp();
    let sent_fields = sent["fields"].clone();
    let Some(held) = state.holding(key) else {
        return Served::json(404, ABSENT);
    };
    merged(&mut held.fields, &sent_fields);
    merged(&mut held.fields, &json!({"updated": updated}));
    state.record(WriteRoute::EditIssue, key, sent, true);
    Served::json(204, "")
}

fn commented(key: &str, sent: &Value, state: &mut StubState) -> Served {
    if sent["body"].is_null() {
        state.record(WriteRoute::AddComment, key, sent, false);
        return Served::refusal(400, "a comment must carry a body");
    }
    let updated = state.stamp();
    let id = format!("2{:04}", state.writes.len() + 1);
    let Some(held) = state.holding(key) else {
        return Served::json(404, ABSENT);
    };
    merged(&mut held.fields, &json!({"updated": updated}));
    with_comment(&mut held.fields, &id, &sent["body"]);
    state.record(WriteRoute::AddComment, key, sent, true);
    let body = json!({"id": id, "body": sent["body"]}).to_string();
    Served::json(201, &body)
}

fn transitioned(key: &str, sent: &Value, state: &mut StubState) -> Served {
    let Some(id) = sent["transition"]["id"].as_str().map(String::from) else {
        state.record(WriteRoute::TransitionIssue, key, sent, false);
        return Served::refusal(400, "a transition must name transition.id");
    };
    let leads_to = state
        .offered
        .iter()
        .find(|offered| offered.issue == key && offered.id == id)
        .map(|offered| offered.leads_to.clone());
    let Some(leads_to) = leads_to else {
        state.record(WriteRoute::TransitionIssue, key, sent, false);
        return Served::refusal(
            400,
            &format!(
                "the stub was never told what transition {id} on {key} leads to, so it cannot \
                 say what this write did"
            ),
        );
    };
    let updated = state.stamp();
    let (category_id, category_key) = category(&leads_to);
    let Some(held) = state.holding(key) else {
        return Served::json(404, ABSENT);
    };
    merged(
        &mut held.fields,
        &json!({
            "updated": updated,
            "status": {
                "id": id,
                "name": leads_to,
                "statusCategory": {"id": category_id, "key": category_key, "name": leads_to},
            },
        }),
    );
    state.record(WriteRoute::TransitionIssue, key, sent, true);
    Served::json(204, "")
}

fn searched(target: &str, state: &mut StubState) -> Served {
    let Some(jql) = query_value(target, "jql") else {
        return Served::refusal(400, "a search must carry a jql query parameter");
    };
    if query_value(target, "startAt").is_some() {
        return Served::refusal(
            400,
            "this endpoint pages by nextPageToken and not by startAt; the offset-paged search \
             endpoint is withdrawn",
        );
    }
    let clauses = match clauses_of(&jql) {
        Ok(clauses) => clauses,
        Err(reason) => return Served::refusal(400, &reason),
    };
    let size = match query_value(target, "maxResults") {
        None => state.page_cap,
        Some(asked) => match asked.parse::<usize>() {
            Ok(asked) if asked > 0 => asked.min(state.page_cap),
            _ => {
                return Served::refusal(
                    400,
                    &format!("`{asked}` is not a page size this endpoint can serve"),
                )
            }
        },
    };
    let offset =
        match query_value(target, "nextPageToken") {
            None => 0,
            Some(token) => match state
                .page_tokens
                .iter()
                .find(|issued| issued.token == token)
            {
                None => return Served::refusal(
                    400,
                    "a page token is opaque and must be one this site issued; it is not an offset",
                ),
                Some(issued) if issued.jql != jql => {
                    return Served::refusal(
                        400,
                        "that page token names a position in a different query's result",
                    )
                }
                Some(issued) => issued.offset,
            },
        };
    let matched: Vec<Value> = state
        .issues
        .iter()
        .filter(|issue| issue.found_by_search)
        .filter(|issue| clauses.iter().all(|clause| selects(clause, issue)))
        .map(|issue| issue.body(&state.base_url))
        .collect();
    let beyond = (offset + size).min(matched.len());
    let page: Vec<Value> = matched[offset.min(matched.len())..beyond].to_vec();
    let mut answer = json!({"issues": page, "isLast": beyond >= matched.len()});
    if beyond < matched.len() {
        let token = format!("tok-{}", state.page_tokens.len() + 1);
        state.page_tokens.push(IssuedPageToken {
            token: token.clone(),
            jql: jql.clone(),
            offset: beyond,
        });
        merged(&mut answer, &json!({"nextPageToken": token}));
    }
    Served::json(200, &answer.to_string())
}

struct Clause {
    field: String,
    value: String,
}

fn clauses_of(jql: &str) -> Result<Vec<Clause>, String> {
    let mut clauses = Vec::new();
    for part in split_on_and(jql) {
        let part = part.trim();
        if part.is_empty() {
            return Err(format!("`{jql}` holds an empty clause"));
        }
        let Some((field, value)) = part.split_once('=') else {
            return Err(format!(
                "the stub parses `field = value` clauses joined by AND and nothing else; \
                 `{part}` is not one"
            ));
        };
        let field = field.trim().to_ascii_lowercase();
        if !matches!(field.as_str(), "labels" | "project" | "key") {
            return Err(format!(
                "the stub selects on labels, project and key only; `{field}` is none of them"
            ));
        }
        let (value, quoted) = unquoted(value.trim());
        if value.is_empty() {
            return Err(format!("`{part}` compares against nothing"));
        }
        if !quoted && value.contains(char::is_whitespace) {
            return Err(format!(
                "the stub reads an unquoted value as one token, so it cannot tell how \
                 much of `{value}` is the value; quote it or drop what follows it"
            ));
        }
        clauses.push(Clause { field, value });
    }
    match clauses.is_empty() {
        true => Err("a search must name at least one clause".to_string()),
        false => Ok(clauses),
    }
}

fn split_on_and(jql: &str) -> Vec<&str> {
    let lowered = jql.to_ascii_lowercase();
    let mut parts = Vec::new();
    let mut from = 0;
    let mut at = 0;
    while let Some(found) = lowered[at..].find(" and ") {
        let start = at + found;
        parts.push(&jql[from..start]);
        from = start + 5;
        at = from;
    }
    parts.push(&jql[from..]);
    parts
}

fn unquoted(value: &str) -> (String, bool) {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return (inner.to_string(), true);
        }
    }
    (value.to_string(), false)
}

fn selects(clause: &Clause, issue: &StoredIssue) -> bool {
    match clause.field.as_str() {
        "key" => issue.key == clause.value,
        "project" => issue.fields["project"]["key"].as_str() == Some(clause.value.as_str()),
        "labels" => issue.fields["labels"]
            .as_array()
            .is_some_and(|labels| labels.iter().any(|label| label == &json!(clause.value))),
        _ => false,
    }
}

fn query_value(target: &str, name: &str) -> Option<String> {
    let (_, query) = target.split_once('?')?;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| decoded(value))
    })
}

fn percent_encoded(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for byte in raw.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            byte => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn decoded(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'+' => {
                out.push(b' ');
                at += 1;
            }
            b'%' if at + 3 <= bytes.len() => match u8::from_str_radix(&raw[at + 1..at + 3], 16) {
                Ok(byte) => {
                    out.push(byte);
                    at += 3;
                }
                Err(_) => {
                    out.push(bytes[at]);
                    at += 1;
                }
            },
            byte => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn merged(into: &mut Value, from: &Value) {
    let (Some(into), Some(from)) = (into.as_object_mut(), from.as_object()) else {
        return;
    };
    for (name, value) in from {
        into.insert(name.clone(), value.clone());
    }
}

fn absent_or_unrouted(path: &str) -> String {
    match names_an_issue(path) {
        true => ABSENT.to_string(),
        false => UNROUTED.to_string(),
    }
}

fn names_an_issue(path: &str) -> bool {
    issue_key(path).is_some()
}

fn issue_key(path: &str) -> Option<&str> {
    let key = path.strip_prefix(ISSUE_ROUTE)?;
    (!key.is_empty() && !key.contains('/')).then_some(key)
}

fn issue_sub<'p>(path: &'p str, leaf: &str) -> Option<&'p str> {
    let rest = path.strip_prefix(ISSUE_ROUTE)?;
    let key = rest.strip_suffix(leaf)?.strip_suffix('/')?;
    (!key.is_empty() && !key.contains('/')).then_some(key)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        411 => "Length Required",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Unassigned",
    }
}
