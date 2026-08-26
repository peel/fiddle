mod support;

use std::time::{Duration, Instant};
use support::stub_jira::{client_for, StubJira, ISSUE};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

const ANSWERED: Duration = Duration::from_millis(500);

#[tokio::test]
async fn the_stub_answers_a_real_issue_body() {
    let server = StubJira::start().await;
    server
        .holds_issue("IDENT-1", "10001", "In Review", "In Progress", 7)
        .await;
    let answered = reqwest::get(format!("{}/rest/api/3/issue/IDENT-1", server.base_url()))
        .await
        .expect("the stub answers");
    assert_eq!(answered.status(), 200);
    let body: serde_json::Value = answered.json().await.unwrap();
    assert_eq!(body["fields"]["status"]["name"], "In Review");
    assert_eq!(body["fields"]["versions"], serde_json::Value::Null);
}

#[tokio::test]
async fn the_issue_body_carries_the_status_category_and_the_updated_time() {
    let server = StubJira::start().await;
    server
        .holds_issue("IDENT-1", "10001", "In Review", "In Progress", 7)
        .await;

    let body = read(&server, ISSUE).await;

    assert_eq!(body["key"], "IDENT-1");
    assert_eq!(body["fields"]["status"]["id"], "10001");
    assert_eq!(
        body["fields"]["status"]["statusCategory"]["name"],
        "In Progress"
    );
    assert_eq!(body["fields"]["updated"], "2026-08-26T07:00:00.000+0000");
}

#[tokio::test]
async fn the_updated_time_is_the_offset_jira_sends_and_not_an_rfc_3339_one() {
    let server = StubJira::start().await;
    server
        .holds_issue("IDENT-1", "10001", "In Review", "In Progress", 7)
        .await;

    let updated = read(&server, ISSUE).await["fields"]["updated"]
        .as_str()
        .expect("the stub sends a string")
        .to_string();

    assert!(
        updated.ends_with("+0000") && !updated.contains("+00:00"),
        "jira cloud sends a colonless offset, which is not what rfc 3339 spells: {updated}"
    );
}

#[tokio::test]
async fn an_explicit_updated_time_reaches_the_wire_unchanged() {
    let server = StubJira::start().await;
    server
        .holds_issue_updated_at(
            "IDENT-1",
            "10001",
            "In Review",
            "In Progress",
            "2026-08-26T11:15:00+02:00",
        )
        .await;

    assert_eq!(
        read(&server, ISSUE).await["fields"]["updated"],
        "2026-08-26T11:15:00+02:00"
    );
}

#[tokio::test]
async fn a_second_holds_issue_replaces_the_first_answer() {
    let server = StubJira::start().await;
    server
        .holds_issue("IDENT-1", "10001", "Ready", "To Do", 7)
        .await;
    let first = read(&server, ISSUE).await;
    server
        .holds_issue("IDENT-1", "10002", "In Review", "In Progress", 8)
        .await;
    let second = read(&server, ISSUE).await;

    assert_eq!(first["fields"]["status"]["name"], "Ready");
    assert_eq!(second["fields"]["status"]["name"], "In Review");
    assert_ne!(first["fields"]["updated"], second["fields"]["updated"]);
}

#[tokio::test]
async fn the_query_string_is_split_off_before_the_path_is_matched() {
    let server = StubJira::start().await;
    server
        .holds_issue("IDENT-1", "10001", "In Review", "In Progress", 7)
        .await;

    let answered = reqwest::get(format!(
        "{}{ISSUE}?fields=status,updated",
        server.base_url()
    ))
    .await
    .expect("the stub answers");

    assert_eq!(answered.status(), 200);
    assert_eq!(
        server.request_lines().await,
        [format!("GET {ISSUE}?fields=status,updated HTTP/1.1")],
        "the query reached the stub and was stripped only for matching"
    );
}

#[tokio::test]
async fn an_issue_the_stub_does_not_hold_is_a_404_and_not_the_one_it_holds() {
    let server = StubJira::start().await;
    server
        .holds_issue("IDENT-1", "10001", "In Review", "In Progress", 7)
        .await;
    let jira = client_for(&server);

    let held = jira
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect("the stub answers the issue it holds");
    let other = jira
        .api(
            "GET",
            "/rest/api/3/issue/IDENT-2",
            None,
            &CancellationToken::new(),
        )
        .await
        .expect("a 404 is an answer");

    assert_eq!(held.status, 200);
    assert_eq!(other.status, 404);
    assert_eq!(
        other.body["errorMessages"][0],
        "Issue does not exist or you do not have permission to see it.",
        "the path was matched exactly, so a neighbouring key is absent"
    );
}

#[tokio::test]
async fn an_issue_the_stub_holds_nothing_for_is_a_404_with_a_json_body() {
    let server = StubJira::start().await;
    server.holds_nothing().await;

    let answered = client_for(&server)
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect("a 404 with a json body parses");

    assert_eq!(answered.status, 404);
    assert!(
        answered.body["errorMessages"][0].is_string(),
        "the body is jira's error shape: {}",
        answered.body
    );
}

#[tokio::test]
async fn a_refusal_reaches_the_caller_as_its_status_and_not_as_a_malformed_body() {
    let server = StubJira::start().await;
    server.refuses_with(401).await;

    let answered = client_for(&server)
        .api("GET", ISSUE, None, &CancellationToken::new())
        .await
        .expect("a refusal with a json body is an answer and not a parse failure");

    assert_eq!(
        answered.status, 401,
        "the status survived the round trip, so a caller can map 401 rather than Malformed"
    );
    assert_eq!(
        answered.body["errorMessages"][0], "the site refused this request",
        "the refusal body is jira's error shape: {}",
        answered.body
    );
}

#[tokio::test]
async fn a_path_the_stub_does_not_route_is_a_404_with_a_json_body() {
    let server = StubJira::start().await;

    let answered = client_for(&server)
        .api("GET", "/rest/api/3/myself", None, &CancellationToken::new())
        .await
        .expect("a 404 with a json body parses");

    assert_eq!(answered.status, 404);
    assert_eq!(
        answered.body["errorMessages"][0],
        "the site serves no resource at that path"
    );
}

#[tokio::test]
async fn a_method_the_stub_does_not_serve_is_a_405() {
    let server = StubJira::start().await;

    let answered = raw(
        server.address(),
        &format!("DELETE {ISSUE} HTTP/1.1\r\n\r\n"),
    )
    .await;

    assert!(
        answered.starts_with("HTTP/1.1 405 Method Not Allowed\r\n"),
        "the stub serves GET and PUT only: {answered}"
    );
}

#[tokio::test]
async fn a_request_line_the_stub_cannot_parse_is_a_400() {
    let server = StubJira::start().await;

    let answered = raw(server.address(), "NOT-A-REQUEST-LINE\r\n\r\n").await;

    assert!(
        answered.starts_with("HTTP/1.1 400 Bad Request\r\n"),
        "an unparseable request line is refused rather than answered: {answered}"
    );
    assert!(
        answered.contains("the request line could not be parsed"),
        "the refusal says why: {answered}"
    );
    assert_eq!(
        server.request_lines().await,
        ["NOT-A-REQUEST-LINE"],
        "the stub recorded what it could not parse"
    );
}

#[tokio::test]
async fn a_body_that_arrives_without_a_content_length_is_a_411() {
    let server = StubJira::start().await;

    let answered = raw(
        server.address(),
        &format!("PUT {ISSUE} HTTP/1.1\r\nHost: stub\r\n\r\n{{\"fields\":{{}}}}"),
    )
    .await;

    assert!(
        answered.starts_with("HTTP/1.1 411 Length Required\r\n"),
        "the stub implements no chunked encoding and says so: {answered}"
    );
}

#[tokio::test]
async fn a_body_measured_by_its_content_length_is_served() {
    let server = StubJira::start().await;
    let sent = r#"{"fields":{"summary":"one"}}"#;

    let answered = raw(
        server.address(),
        &format!(
            "PUT {ISSUE} HTTP/1.1\r\nHost: stub\r\nContent-Length: {}\r\n\r\n{sent}",
            sent.len()
        ),
    )
    .await;

    assert!(
        answered.starts_with("HTTP/1.1 200 OK\r\n"),
        "a measured body is served: {answered}"
    );
}

#[tokio::test]
async fn two_stubs_bind_two_ports() {
    let one = StubJira::start().await;
    let other = StubJira::start().await;

    assert_ne!(
        one.address(),
        other.address(),
        "port 0 was read back twice, so two tests never collide"
    );
    for server in [&one, &other] {
        assert!(
            !raw(server.address(), &format!("GET {ISSUE} HTTP/1.1\r\n\r\n"))
                .await
                .is_empty(),
            "both stubs answer on their own port"
        );
    }
}

#[tokio::test]
async fn a_dropped_stub_stops_answering_on_its_port() {
    let address = {
        let server = StubJira::start().await;
        assert!(
            raw(server.address(), &format!("GET {ISSUE} HTTP/1.1\r\n\r\n"))
                .await
                .starts_with("HTTP/1.1 200 OK\r\n"),
            "the stub answered while it was alive"
        );
        server.address().to_string()
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    while answers(&address).await {
        assert!(
            Instant::now() < deadline,
            "drop cancelled nothing: {address} is still answering"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn answers(address: &str) -> bool {
    let request = format!("GET {ISSUE} HTTP/1.1\r\n\r\n");
    let spoken = async {
        let mut socket = TcpStream::connect(address).await.ok()?;
        socket.write_all(request.as_bytes()).await.ok()?;
        socket.flush().await.ok()?;
        let mut answered = String::new();
        socket.read_to_string(&mut answered).await.ok()?;
        Some(answered)
    };
    match tokio::time::timeout(ANSWERED, spoken).await {
        Ok(Some(answered)) => !answered.is_empty(),
        Ok(None) | Err(_) => false,
    }
}

async fn raw(address: &str, request: &str) -> String {
    let mut socket = TcpStream::connect(address)
        .await
        .expect("the stub accepts a connection");
    socket
        .write_all(request.as_bytes())
        .await
        .expect("the request is sent");
    socket.flush().await.expect("the request is flushed");
    let mut answered = String::new();
    tokio::time::timeout(ANSWERED, socket.read_to_string(&mut answered))
        .await
        .expect("the stub answers inside its deadline")
        .expect("the answer is read");
    answered
}

async fn read(server: &StubJira, path: &str) -> serde_json::Value {
    client_for(server)
        .api("GET", path, None, &CancellationToken::new())
        .await
        .expect("the stub answers")
        .body
}
