mod support;

use fiddle_core::{Observation, WorkItemState, WorkState};
use fiddle_runtime::jira::http::CLAMP;
use fiddle_runtime::jira::{ConfiguredNames, JiraWorkItemPort};
use fiddle_runtime::ports::contract::{work_item_port_contract, WorkItemWorlds};
use fiddle_runtime::ports::WorkItemPort;
use fiddle_runtime::REDACTED;
use std::time::{Duration, Instant};
use support::stub_jira::{client_for, StubJira, ENCODED, ISSUE, TOKEN};
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

const KEY: &str = "IDENT-1";
const ZONED: &str = "2026-08-26T11:15:00.000+0200";
const AS_UTC: &str = "2026-08-26T09:15:00Z";

fn names(configured: &[(&str, &str)]) -> ConfiguredNames {
    let of = |key: &str| {
        configured
            .iter()
            .find(|(configured_key, _)| *configured_key == key)
            .map(|(_, jira_name)| (*jira_name).to_string())
    };
    ConfiguredNames::new(
        of("ready"),
        of("in_progress"),
        of("in_review"),
        of("blocked"),
        of("done"),
    )
}

fn port_for(server: &StubJira) -> JiraWorkItemPort {
    JiraWorkItemPort::new(client_for(server), names(&[]), server.site())
}

async fn observe_from(server: &StubJira) -> Observation<WorkItemState> {
    port_for(server).observe(KEY).await
}

fn revision_of(observed: &Observation<WorkItemState>) -> String {
    match observed {
        Observation::Available { revision, .. } => revision
            .clone()
            .expect("a readable issue carries a revision"),
        other => panic!("a readable issue must be Available, got {other:?}"),
    }
}

fn error_body(said: &str) -> String {
    serde_json::json!({"errorMessages": [said], "errors": {}}).to_string()
}

fn reason_of(observed: &Observation<WorkItemState>) -> String {
    match observed {
        Observation::Unavailable { reason, .. } => reason.clone(),
        other => panic!("an unreadable issue must be Unavailable, got {other:?}"),
    }
}

struct JiraWorlds {
    absent: StubJira,
    malformed: StubJira,
    open: StubJira,
}

impl JiraWorlds {
    async fn start() -> Self {
        let absent = StubJira::start().await;
        absent.holds_nothing().await;
        let malformed = StubJira::start().await;
        malformed
            .answer_with_body("<html>this is not an issue</html>")
            .await;
        let open = StubJira::start().await;
        open.holds_issue_updated_at(KEY, "10001", "open", "In Progress", ZONED)
            .await;
        Self {
            absent,
            malformed,
            open,
        }
    }
}

impl WorkItemWorlds for JiraWorlds {
    type Port = JiraWorkItemPort;

    fn work_id(&self) -> &str {
        KEY
    }

    fn origin(&self) -> &str {
        "jira"
    }

    fn source_absent(&self) -> Self::Port {
        port_for(&self.absent)
    }

    fn source_malformed(&self) -> Self::Port {
        port_for(&self.malformed)
    }

    fn source_open(&self) -> Self::Port {
        port_for(&self.open)
    }
}

#[tokio::test]
async fn the_port_reports_the_status_verbatim_and_the_updated_time_as_the_revision() {
    let server = StubJira::start().await;
    server
        .holds_issue_updated_at(
            KEY,
            "10001",
            "Awaiting Security Review",
            "In Progress",
            ZONED,
        )
        .await;

    match observe_from(&server).await {
        Observation::Available {
            value,
            source,
            revision,
        } => {
            assert_eq!(
                value.status, "Awaiting Security Review",
                "verbatim, never normalised"
            );
            assert_eq!(
                revision.as_deref(),
                Some(AS_UTC),
                "fields.updated, canonicalised to UTC, is the revision"
            );
            assert_eq!(
                source.0,
                format!("jira:{}/{KEY}", server.site()),
                "the source names its origin and the issue it read"
            );
            assert_eq!(value.id, KEY);
            assert_eq!(
                value.projected_status.expect("a read issue projects").state,
                WorkState::InProgress,
                "no name is configured, so the status category decides"
            );
        }
        other => panic!("a readable issue must be Available, got {other:?}"),
    }
}

#[tokio::test]
async fn the_contract_holds_over_http() {
    let worlds = JiraWorlds::start().await;
    work_item_port_contract(&worlds).await;
}

#[tokio::test]
async fn the_configured_names_the_port_was_built_with_reach_its_projection() {
    let server = StubJira::start().await;
    server
        .holds_issue(KEY, "10001", "Awaiting Security Review", "In Progress", 7)
        .await;
    let port = JiraWorkItemPort::new(
        client_for(&server),
        names(&[("in_review", "Awaiting Security Review")]),
        server.site(),
    );

    let observed = port.observe(KEY).await;

    let value = observed.value().expect("a readable issue is available");
    assert_eq!(
        value.status, "Awaiting Security Review",
        "the projection changed, the reported status did not"
    );
    assert_eq!(
        value
            .projected_status
            .as_ref()
            .expect("a read issue projects")
            .state,
        WorkState::InReview,
        "the port handed its own configured names to the projection, so the category lost"
    );
}

#[tokio::test]
async fn a_refused_credential_and_a_missing_issue_do_not_read_alike() {
    let refused = StubJira::start().await;
    refused.refuses_with(401).await;
    let missing = StubJira::start().await;
    missing.holds_nothing().await;

    let mut said = Vec::new();
    for (server, expected, other) in [
        (&refused, "credential", "no issue"),
        (&missing, "no issue", "credential"),
    ] {
        let observed = observe_from(server).await;
        match &observed {
            Observation::Unavailable { reason, source } => {
                assert!(
                    reason.contains(expected),
                    "the reason must say `{expected}`: {reason}"
                );
                assert!(
                    !reason.contains(other),
                    "the reason must not also say `{other}`, or one reason answers both: {reason}"
                );
                assert!(
                    reason.contains(server.site()),
                    "the reason must name the site it could not read: {reason}"
                );
                assert_eq!(
                    source.0,
                    format!("jira:{}/{KEY}", server.site()),
                    "an unreadable issue names its origin too"
                );
                said.push(reason.clone());
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    assert_ne!(
        said[0], said[1],
        "a refused credential and a missing issue read as two reasons: {said:?}"
    );
}

#[tokio::test]
async fn a_refusal_answered_in_html_reads_as_the_refusal_it_is_and_not_as_a_malformed_body() {
    for (status, expected) in [
        (401, "the site refused the credential with 401"),
        (403, "the credential may not read this issue: 403"),
    ] {
        let server = StubJira::start().await;
        server.refuses_in_html_with(status).await;

        let reason = reason_of(&observe_from(&server).await);

        assert_eq!(
            reason,
            format!("{}: {expected}", server.site()),
            "jira cloud answers some refusals with an html login page, and the status is still the fact"
        );
        assert!(
            !reason.contains("not an issue"),
            "an html refusal must not arrive as a malformed answer: {reason}"
        );
    }
}

#[tokio::test]
async fn one_instant_sent_in_three_zones_canonicalises_to_one_revision() {
    let sent = [
        ZONED,
        "2026-08-26T09:15:00.000+0000",
        "2026-08-26T04:15:00.000-0500",
    ];
    let mut distinct = sent.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        sent.len(),
        "the fixtures are three different strings, so the equality below is not one value compared with itself: {sent:?}"
    );

    let server = StubJira::start().await;
    let mut revisions = Vec::new();
    for updated in sent {
        server
            .holds_issue_updated_at(KEY, "10001", "In Review", "In Progress", updated)
            .await;
        revisions.push(revision_of(&observe_from(&server).await));
    }

    assert_eq!(
        revisions,
        vec![AS_UTC.to_string(); sent.len()],
        "atlassian answers in the reading user's zone, so one instant read three ways is one revision"
    );
}

#[tokio::test]
async fn the_colonless_offset_jira_sends_canonicalises_and_is_not_carried_through_raw() {
    let server = StubJira::start().await;
    server
        .holds_issue(KEY, "10001", "In Review", "In Progress", 7)
        .await;

    let revision = revision_of(&observe_from(&server).await);

    assert_eq!(
        revision, "2026-08-26T07:00:00Z",
        "the stub sends `2026-08-26T07:00:00.000+0000`, which rfc 3339 cannot spell, so a raw pass-through reds here"
    );
}

#[tokio::test]
async fn an_updated_time_the_port_cannot_read_is_unavailable_and_never_a_revision_of_its_own_text()
{
    for updated in [
        "yesterday",
        "",
        "2026-08-26 11:15:00+0200",
        "2026-08-26T11:15:00",
    ] {
        let server = StubJira::start().await;
        server
            .holds_issue_updated_at(KEY, "10001", "In Review", "In Progress", updated)
            .await;

        let reason = reason_of(&observe_from(&server).await);

        assert!(
            reason.contains("`fields.updated`"),
            "the reason must name the field it could not read as a time: {reason}"
        );
        assert!(
            reason.contains(server.site()),
            "the reason must name the site it could not read: {reason}"
        );
    }
}

#[tokio::test]
async fn an_issue_that_carries_no_updated_field_is_unavailable() {
    let server = StubJira::start().await;
    server
        .answer_with_body(
            &serde_json::json!({
                "key": KEY,
                "fields": {
                    "status": {
                        "id": "10001",
                        "name": "In Review",
                        "statusCategory": {"name": "In Progress"},
                    },
                },
            })
            .to_string(),
        )
        .await;

    let reason = reason_of(&observe_from(&server).await);

    assert!(
        reason.contains("no `fields.updated`"),
        "a revision is what makes a later target identity name a state, so its absence is not a readable issue: {reason}"
    );
}

#[tokio::test]
async fn the_port_sees_a_status_that_changed_between_two_reads() {
    let server = StubJira::start().await;
    server.holds_issue(KEY, "10001", "Ready", "To Do", 7).await;
    let port = port_for(&server);

    let first = port.observe(KEY).await;
    server
        .holds_issue(KEY, "10002", "In Review", "In Progress", 8)
        .await;
    let second = port.observe(KEY).await;

    assert_eq!(
        first.value().expect("the first read is available").status,
        "Ready"
    );
    assert_eq!(
        second.value().expect("the second read is available").status,
        "In Review",
        "one port instance read the world twice, so nothing was answered from the first read"
    );
    assert_ne!(
        revision_of(&first),
        revision_of(&second),
        "the second read carries the second updated time"
    );
}

#[tokio::test]
async fn seven_ways_of_failing_to_read_name_the_site_and_read_as_seven_reasons() {
    let absent = StubJira::start().await;
    absent.holds_nothing().await;
    let malformed = StubJira::start().await;
    malformed
        .answer_with_body("<html>not an issue</html>")
        .await;
    let refused = StubJira::start().await;
    refused.refuses_with(401).await;
    let forbidden = StubJira::start().await;
    forbidden.refuses_in_html_with(403).await;
    let unreadable_time = StubJira::start().await;
    unreadable_time
        .holds_issue_updated_at(KEY, "10001", "In Review", "In Progress", "yesterday")
        .await;
    let out = StubJira::start().await;
    out.refuses_with(503).await;
    let limited = StubJira::start().await;
    limited.refuses_with(429).await;

    let mut reasons = Vec::new();
    for server in [
        &absent,
        &malformed,
        &refused,
        &forbidden,
        &unreadable_time,
        &out,
        &limited,
    ] {
        let reason = reason_of(&observe_from(server).await);
        assert!(
            reason.starts_with(server.site()),
            "every reason names the site it could not read: {reason}"
        );
        reasons.push(reason.replacen(server.site(), "<site>", 1));
    }

    let spoken = reasons.len();
    reasons.sort();
    reasons.dedup();
    assert_eq!(
        reasons.len(),
        spoken,
        "two ways of failing to read read the same: {reasons:?}"
    );
}

#[tokio::test]
async fn an_outage_and_a_rate_limit_are_reported_as_what_they_are_and_not_as_a_broken_answer() {
    for (status, expected) in [
        (500, "the site could not be reached: HTTP 500"),
        (502, "the site could not be reached: HTTP 502"),
        (503, "the site could not be reached: HTTP 503"),
        (
            429,
            "the site limited this request and it can be sent again: HTTP 429",
        ),
    ] {
        let server = StubJira::start().await;
        server
            .refuses_with_body(status, &error_body("the site said this"))
            .await;

        let reason = reason_of(&observe_from(&server).await);

        assert_eq!(
            reason,
            format!("{}: {expected}: the site said this", server.site()),
            "a {status} says what happened and carries the site's own words"
        );
        assert!(
            !reason.contains("not an issue"),
            "the port never parsed a body here, so a {status} is not a malformed answer: {reason}"
        );
    }
}

#[tokio::test]
async fn an_outage_answered_in_html_reads_as_unreachable_and_quotes_no_body() {
    let server = StubJira::start().await;
    server.refuses_in_html_with(503).await;

    let reason = reason_of(&observe_from(&server).await);

    assert_eq!(
        reason,
        format!("{}: the site could not be reached: HTTP 503", server.site()),
        "a body the client could not read is quoted back as nothing, and the status is still the fact"
    );
}

#[tokio::test]
async fn a_credential_planted_in_the_sites_error_body_is_redacted_before_a_reader_sees_it() {
    for planted in [TOKEN, ENCODED] {
        for status in [429, 503] {
            let server = StubJira::start().await;
            server
                .refuses_with_body(
                    status,
                    &error_body(&format!("the gateway wrote {planted} to its log")),
                )
                .await;

            let reason = reason_of(&observe_from(&server).await);

            assert!(
                !reason.contains(planted),
                "carrying the site's words must not carry the credential out of a {status}: {reason}"
            );
            assert!(
                reason.contains(REDACTED),
                "the credential was in the body, so the reason must show it was replaced: {reason}"
            );
            assert!(
                reason.contains("the gateway wrote"),
                "the rest of the site's words survived, so the two assertions above did not pass on an empty reason: {reason}"
            );
        }
    }
}

#[tokio::test]
async fn an_error_body_longer_than_the_clamp_reaches_the_reason_bounded() {
    let long = "x".repeat(CLAMP * 4);
    let server = StubJira::start().await;
    server.refuses_with_body(503, &error_body(&long)).await;

    let reason = reason_of(&observe_from(&server).await);

    assert!(
        reason.contains(&"x".repeat(1_000)) && reason.len() > CLAMP,
        "the head of the site's words reached the reason: {} bytes",
        reason.len()
    );
    assert!(
        reason.contains("elided"),
        "the clamp must say it clamped: {} bytes",
        reason.len()
    );
    assert!(
        reason.len() < CLAMP + 512,
        "the reason is bounded by the clamp and not by the answered body of {} bytes, got {} bytes",
        long.len(),
        reason.len()
    );
}
