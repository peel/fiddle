use fiddle_runtime::jira::JiraHttp;
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
pub const ISSUE: &str = "/rest/api/3/issue/IDENT-1";
pub const PATIENT: Duration = Duration::from_secs(30);
pub const HELD_DAY: &str = "2026-08-26";

const SETTLES: Duration = Duration::from_millis(100);

const ABSENT: &str = r#"{"errorMessages":["Issue does not exist or you do not have permission to see it."],"errors":{}}"#;
const REFUSED: &str = r#"{"errorMessages":["the site refused this request"],"errors":{}}"#;
const UNROUTED: &str =
    r#"{"errorMessages":["the site serves no resource at that path"],"errors":{}}"#;
const NOT_ALLOWED: &str =
    r#"{"errorMessages":["the site does not serve that method here"],"errors":{}}"#;
const UNPARSED: &str = r#"{"errorMessages":["the request line could not be parsed"],"errors":{}}"#;
const NO_LENGTH: &str =
    r#"{"errorMessages":["a body must arrive with a content length"],"errors":{}}"#;
const HTML_REFUSAL: &str = "<!DOCTYPE html><html><head><title>Sign in</title></head><body>\
                            <h1>You are not authenticated</h1></body></html>";

const JSON: &str = "application/json";
const HTML: &str = "text/html;charset=UTF-8";

enum Answer {
    Body(String),
    Issue { path: String, body: String },
    Absent,
    Refusal(u16),
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
}

enum Length {
    Given(usize),
    Absent,
    Chunked,
}

struct StubState {
    answer: Answer,
    silent: bool,
    authorizations: Vec<String>,
    request_lines: Vec<String>,
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
        let state = Arc::new(Mutex::new(StubState {
            answer: Answer::Body(r#"{"key":"IDENT-1"}"#.to_string()),
            silent: false,
            authorizations: Vec::new(),
            request_lines: Vec::new(),
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
            base_url: format!("http://{address}"),
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

    pub async fn answer_with_body(&self, body: &str) {
        self.state.lock().await.answer = Answer::Body(body.to_string());
    }

    pub async fn refuses_with(&self, status: u16) {
        self.state.lock().await.answer = Answer::Refusal(status);
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

    let (answering, silent) = {
        let mut held = state.lock().await;
        held.authorizations.push(authorization);
        held.request_lines.push(request_line.clone());
        (routed(&request_line, &held.answer), held.silent)
    };

    if silent {
        cancel.cancelled().await;
        return;
    }

    let answering = match measured(&request_line, length) {
        Some(length) => {
            let mut body = vec![0u8; length];
            if length > 0 && socket.read_exact(&mut body).await.is_err() {
                return;
            }
            answering
        }
        None => {
            let mut unmeasured = vec![0u8; 8192];
            let _ = tokio::time::timeout(SETTLES, socket.read(&mut unmeasured)).await;
            Served::json(411, NO_LENGTH)
        }
    };

    let Served {
        status,
        body,
        content_type,
    } = answering;
    let response = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        reason(status),
        body.len()
    );
    let _ = socket.write_all(response.as_bytes()).await;
    let _ = socket.flush().await;
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

fn routed(request_line: &str, answer: &Answer) -> Served {
    let mut parts = request_line.split_whitespace();
    let (Some(method), Some(target), Some(version), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Served::json(400, UNPARSED);
    };
    if !version.starts_with("HTTP/") || !target.starts_with('/') {
        return Served::json(400, UNPARSED);
    }
    let path = target.split('?').next().unwrap_or(target);
    if !matches!(method, "GET" | "PUT") {
        return Served::json(405, NOT_ALLOWED);
    }
    match answer {
        Answer::Refusal(status) => Served::json(*status, REFUSED),
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

fn absent_or_unrouted(path: &str) -> String {
    match names_an_issue(path) {
        true => ABSENT.to_string(),
        false => UNROUTED.to_string(),
    }
}

fn names_an_issue(path: &str) -> bool {
    path.len() > ISSUE_ROUTE.len() && path.starts_with(ISSUE_ROUTE)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
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
