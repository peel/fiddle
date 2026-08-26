use fiddle_runtime::jira::http::CLAMP;
use fiddle_runtime::jira::{JiraError, JiraHttp};
use fiddle_runtime::REDACTED;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const USER: &str = "bot@example.com";
const TOKEN: &str = "s3cr3t";
const ENCODED: &str = "Ym90QGV4YW1wbGUuY29tOnMzY3IzdA==";
const ISSUE: &str = "/rest/api/3/issue/IDENT-1";
const PATIENT: Duration = Duration::from_secs(30);

struct StubState {
    body: String,
    silent: bool,
    authorizations: Vec<String>,
    request_lines: Vec<String>,
}

struct StubJira {
    base_url: String,
    state: Arc<Mutex<StubState>>,
    cancel: CancellationToken,
}

impl StubJira {
    async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port is available");
        let port = listener
            .local_addr()
            .expect("an accepted listener has an address")
            .port();
        let state = Arc::new(Mutex::new(StubState {
            body: r#"{"key":"IDENT-1"}"#.to_string(),
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
            base_url: format!("http://127.0.0.1:{port}"),
            state,
            cancel,
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn answer_with_body(&self, body: &str) {
        self.state.lock().await.body = body.to_string();
    }

    async fn stays_silent(&self) {
        self.state.lock().await.silent = true;
    }

    async fn last_authorization(&self) -> String {
        self.state
            .lock()
            .await
            .authorizations
            .last()
            .cloned()
            .expect("the stub was asked something")
    }

    async fn request_lines(&self) -> Vec<String> {
        self.state.lock().await.request_lines.clone()
    }
}

impl Drop for StubJira {
    fn drop(&mut self) {
        self.cancel.cancel();
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
    let mut length = 0usize;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        match name.to_ascii_lowercase().as_str() {
            "authorization" => authorization = value.trim().to_string(),
            "content-length" => length = value.trim().parse().unwrap_or(0),
            _ => {}
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 && socket.read_exact(&mut body).await.is_err() {
        return;
    }

    let (answering, silent) = {
        let mut held = state.lock().await;
        held.authorizations.push(authorization);
        held.request_lines.push(request_line);
        (held.body.clone(), held.silent)
    };

    if silent {
        cancel.cancelled().await;
        return;
    }

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{answering}",
        answering.len()
    );
    let _ = socket.write_all(response.as_bytes()).await;
    let _ = socket.flush().await;
}

fn client(server: &StubJira, timeout: Duration) -> JiraHttp {
    JiraHttp::new(server.base_url(), USER, TOKEN, timeout).expect("the client builds")
}

#[tokio::test]
async fn the_request_carries_a_basic_header_and_the_client_never_prints_it() {
    let server = StubJira::start().await;
    let jira = client(&server, PATIENT);

    let answer = jira
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect("the stub answers");

    assert_eq!(answer.status, 200);
    assert_eq!(answer.body["key"], "IDENT-1");
    assert_eq!(
        server.last_authorization().await,
        format!("Basic {ENCODED}"),
        "the wire header is the pinned encoding of {USER}:{TOKEN}, read off the socket"
    );
    assert_eq!(
        server.request_lines().await,
        [format!("GET {ISSUE} HTTP/1.1")],
        "one round trip reached the site, so the header above was really sent"
    );
}

#[tokio::test]
async fn a_body_echoing_the_token_never_reaches_an_error_string() {
    let server = StubJira::start().await;
    server
        .answer_with_body(&format!("{TOKEN} is not json"))
        .await;
    let jira = client(&server, PATIENT);

    let error = jira
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect_err("a non-json body is malformed");

    let said = format!("{error}");
    assert!(
        said.contains("is not json"),
        "the answered body reached the error, so the absence below is redaction: {said}"
    );
    assert!(
        said.contains(REDACTED),
        "the token was replaced rather than dropped: {said}"
    );
    assert!(
        !said.contains(TOKEN),
        "the error must not carry the token: {said}"
    );
}

#[tokio::test]
async fn a_body_echoing_the_sent_header_never_reaches_an_error_string() {
    let server = StubJira::start().await;
    server
        .answer_with_body(&format!("Basic {ENCODED} was rejected"))
        .await;
    let jira = client(&server, PATIENT);

    let error = jira
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect_err("a non-json body is malformed");

    let said = format!("{error}");
    assert!(
        said.contains("was rejected"),
        "the answered body reached the error: {said}"
    );
    assert!(
        !said.contains(ENCODED),
        "the encoded credential must not survive as its own header value: {said}"
    );
}

#[tokio::test]
async fn an_oversized_multibyte_body_is_clamped_on_a_character_boundary() {
    let server = StubJira::start().await;
    server.answer_with_body(&"é".repeat(50_000)).await;
    let jira = client(&server, PATIENT);

    let error = jira
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect_err("a non-json body is malformed");

    let said = format!("{error}");
    assert!(
        said.len() < 4096,
        "the error must be bounded, got {} bytes",
        said.len()
    );
    assert!(
        said.contains("elided"),
        "the clamp must say it clamped: {said}"
    );
    assert!(
        said.contains('é'),
        "the clamp kept the head of the answered body: {said}"
    );
}

#[tokio::test]
async fn a_token_straddling_the_clamp_is_replaced_before_the_cut_and_not_after() {
    let planted = "s3cr3t-continues-past-the-cut";
    let server = StubJira::start().await;
    server
        .answer_with_body(&format!("{}{planted}", "x".repeat(CLAMP - 6)))
        .await;
    let jira = JiraHttp::new(server.base_url(), USER, planted, PATIENT).expect("the client builds");

    let error = jira
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect_err("a non-json body is malformed");

    let said = format!("{error}");
    assert!(
        said.contains(&"x".repeat(1_000)) && said.contains("elided"),
        "the answered body reached the error and was clamped: {} bytes",
        said.len()
    );
    assert!(
        !said.contains("s3cr3t"),
        "clamping before redacting would leave the token's first six bytes at the cut: {said}"
    );
}

#[tokio::test]
async fn an_empty_credential_half_redacts_nothing_it_was_not_given() {
    let server = StubJira::start().await;
    server.answer_with_body("plain text").await;
    let jira = JiraHttp::new(server.base_url(), USER, "", PATIENT).expect("the client builds");

    let error = jira
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect_err("a non-json body is malformed");

    let said = format!("{error}");
    assert!(
        said.contains("plain text"),
        "an empty token replaces no byte of an answer: {said}"
    );
    assert!(
        !said.contains(&format!("{REDACTED}p")),
        "an empty pattern must not be handed to replace: {said}"
    );
}

#[tokio::test]
async fn a_read_that_is_never_answered_ends_at_its_timeout() {
    let server = StubJira::start().await;
    server.stays_silent().await;
    let jira = client(&server, Duration::from_millis(300));

    let started = Instant::now();
    let error = jira
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect_err("a silent site cannot answer");
    let waited = started.elapsed();

    assert!(
        matches!(error, JiraError::Unreachable(_)),
        "a site that never answers could not be reached: {error}"
    );
    assert!(
        waited >= Duration::from_millis(300),
        "the configured timeout is what ended the call, not something faster: {waited:?}"
    );
    assert!(
        waited < Duration::from_secs(5),
        "the call is bounded and did not hang: {waited:?}"
    );
    assert_eq!(
        server.request_lines().await.len(),
        1,
        "the request did reach the site, so the timeout above is a read timeout"
    );
}

#[tokio::test]
async fn a_read_cancelled_while_it_waits_returns_before_its_timeout() {
    let server = StubJira::start().await;
    server.stays_silent().await;
    let jira = client(&server, PATIENT);
    let cancel = CancellationToken::new();

    let started = Instant::now();
    let (answered, ()) = tokio::join!(jira.api("GET", ISSUE, None, &cancel), async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
    });
    let waited = started.elapsed();

    let error = answered.expect_err("a cancelled read is a failure");
    assert_eq!(
        format!("{error}"),
        "the site could not be reached: cancelled",
        "a cancelled read says so"
    );
    assert!(
        waited < PATIENT / 2,
        "cancellation ended the call well inside its {PATIENT:?} timeout: {waited:?}"
    );
    assert_eq!(
        server.request_lines().await.len(),
        1,
        "the call was in flight when it was cancelled"
    );
}

#[tokio::test]
async fn a_read_cancelled_before_it_starts_sends_nothing() {
    let server = StubJira::start().await;
    let jira = client(&server, PATIENT);
    let cancel = CancellationToken::new();
    cancel.cancel();

    let error = jira
        .api("GET", ISSUE, None, &cancel)
        .await
        .expect_err("a cancelled read is a failure");

    assert_eq!(
        format!("{error}"),
        "the site could not be reached: cancelled"
    );
    assert!(
        server.request_lines().await.is_empty(),
        "a cancelled read reaches the site with nothing: {:?}",
        server.request_lines().await
    );
}

#[tokio::test]
async fn a_method_the_client_cannot_send_is_refused_before_the_site_is_reached() {
    let server = StubJira::start().await;
    let jira = client(&server, PATIENT);

    let error = jira
        .api("GET ISSUE", ISSUE, None, &CancellationToken::new())
        .await
        .expect_err("a method with a space is not a method");

    assert!(
        matches!(error, JiraError::Malformed(_)),
        "an unsendable method is malformed: {error}"
    );
    assert!(
        server.request_lines().await.is_empty(),
        "nothing was sent: {:?}",
        server.request_lines().await
    );
}

#[tokio::test]
async fn a_body_the_caller_supplies_reaches_the_site_as_json() {
    let server = StubJira::start().await;
    let jira = client(&server, PATIENT);

    let answer = jira
        .api(
            "PUT",
            ISSUE,
            Some(&serde_json::json!({"fields": {"summary": "one"}})),
            &CancellationToken::new(),
        )
        .await
        .expect("the stub answers");

    assert_eq!(answer.status, 200);
    assert_eq!(
        server.request_lines().await,
        [format!("PUT {ISSUE} HTTP/1.1")]
    );
}

#[tokio::test]
async fn an_empty_answer_is_a_null_body_and_not_a_malformed_one() {
    let server = StubJira::start().await;
    server.answer_with_body("").await;
    let jira = client(&server, PATIENT);

    let answer = jira
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect("an empty answer is an answer");

    assert_eq!(answer.status, 200);
    assert_eq!(answer.body, serde_json::Value::Null);
}

#[tokio::test]
async fn a_base_url_with_a_trailing_slash_reaches_the_same_path() {
    let server = StubJira::start().await;
    let jira = JiraHttp::new(&format!("{}/", server.base_url()), USER, TOKEN, PATIENT)
        .expect("the client builds");

    jira.api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect("the stub answers");

    assert_eq!(
        server.request_lines().await,
        [format!("GET {ISSUE} HTTP/1.1")],
        "one slash, not two"
    );
}
