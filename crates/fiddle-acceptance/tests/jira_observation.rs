mod support;

use support::{Scenario, StubJira};

const TOKEN_CREDENTIAL: &str = "JIRA_API_TOKEN";

const USER_CREDENTIAL: &str = "JIRA_USER_EMAIL";

const THROWAWAY_USER: &str = "nobody@example.com";

const THROWAWAY_TOKEN: &str = "a-token-no-site-would-honour";

const REFERENCE: &str = "jira:IDENT-1";

const REVISION: &str = "2026-08-25T20:00:00Z";

const SOURCE: &str = "jira:https://icecube.atlassian.net/IDENT-1";

#[test]
fn inspect_reports_a_jira_issue_through_the_public_cli() {
    let stub = StubJira::holding_the_issue();
    let project = disposable_project_reading(&stub.base_url());
    let run = read_the_issue(&project);

    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let reported: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}",
            String::from_utf8_lossy(&run.stdout)
        )
    });
    let observed = &reported["observations"]["work_item"]["available"];

    assert_eq!(
        stub.served(),
        1,
        "the reported issue has to have been read off the wire, or the values below \
         are this test's own fixture read back from somewhere else"
    );
    assert_eq!(
        observed["value"]["status"],
        support::JIRA_ISSUE_STATUS,
        "the status is reported in the site's own words: {observed}"
    );
    assert_eq!(
        observed["value"]["projected_status"]["state"],
        "in_review",
        "the document names `In Review` as its in_review status, and the issue's \
         category `{}` would project to in_progress on its own, so this is the \
         configured name deciding: {observed}",
        support::JIRA_ISSUE_CATEGORY
    );
    assert_eq!(
        observed["revision"], REVISION,
        "the revision is `fields.updated` canonicalised to UTC: {observed}"
    );
    assert_ne!(
        support::JIRA_ISSUE_UPDATED,
        REVISION,
        "the site sends `{}`, a colonless offset five and a half hours ahead of UTC \
         and a day later, so the assertion above reds for a build that carries \
         `fields.updated` through raw",
        support::JIRA_ISSUE_UPDATED
    );
}

fn disposable_project_reading(base_url: &str) -> Scenario {
    let project = Scenario::new();
    project.append_config(&format!(
        "[jira]\n\
         site = \"https://icecube.atlassian.net\"\n\
         project = \"IDENT\"\n\
         user = {{ env = \"{USER_CREDENTIAL}\" }}\n\
         token = {{ env = \"{TOKEN_CREDENTIAL}\" }}\n\
         base_url = \"{base_url}\"\n\
         timeout = \"30s\"\n\
         \n\
         [jira.workflow]\n\
         in_review = \"{}\"\n",
        support::JIRA_ISSUE_STATUS
    ));
    project
}

#[test]
fn inspect_asks_the_site_for_the_issue_at_the_documented_path_and_no_more_fields() {
    let stub = StubJira::holding_the_issue();
    let project = disposable_project_reading(&stub.base_url());
    let run = read_the_issue(&project);

    assert!(
        run.status.success(),
        "the read has to reach the site for a request line to have been received: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        stub.request_lines(),
        vec![format!(
            "GET /rest/api/3/issue/{}?fields=status,updated,labels,description,comment HTTP/1.1",
            support::JIRA_ISSUE_KEY
        )],
        "the CLI reads one issue with one GET, and it names the five fields it uses, \
         so a build that asks for the whole issue or asks at another path reds here"
    );
}

#[test]
fn the_site_answers_nothing_at_a_path_or_a_method_the_cli_never_asks_for() {
    let stub = StubJira::holding_the_issue();
    let held = format!("/rest/api/3/issue/{}", support::JIRA_ISSUE_KEY);

    for (request_line, expected) in [
        (
            format!("GET {held}?fields=status,updated,labels,description,comment HTTP/1.1"),
            "200",
        ),
        (format!("GET {held} HTTP/1.1"), "200"),
        (format!("GET {held}/transitions HTTP/1.1"), "404"),
        ("GET /rest/api/3/issue/OTHER-9 HTTP/1.1".to_string(), "404"),
        ("GET /rest/api/3/myself HTTP/1.1".to_string(), "404"),
        (format!("POST {held} HTTP/1.1"), "405"),
        ("GET nonsense HTTP/1.1".to_string(), "400"),
    ] {
        let said = ask_over_tcp(&stub.base_url(), &request_line);
        assert!(
            said.starts_with(&format!("HTTP/1.1 {expected}")),
            "`{request_line}` has to be answered {expected}, or this site answers an \
             issue to requests the CLI never sends and every assertion made through \
             it holds for a build that asks for something else: {said}"
        );
    }
}

#[test]
fn the_human_reading_names_the_typed_state_beside_the_status_the_site_sent() {
    let stub = StubJira::holding_the_issue();
    let project = disposable_project_reading(&stub.base_url());
    let run = read_the_issue_for_a_reader(&project);

    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let out = String::from_utf8(run.stdout).expect("the human reading is text");
    let line = out
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("work item   = "))
        .unwrap_or_else(|| panic!("the human reading carries a work item line: {out}"));

    let verbatim_alone = format!("status \"{}\" (from {SOURCE})", support::JIRA_ISSUE_STATUS);

    assert_eq!(
        line,
        format!(
            "status \"{}\", state in review (from {SOURCE})",
            support::JIRA_ISSUE_STATUS
        ),
        "a caller at a terminal gets this line and no JSON, so the typed state has \
         to reach it beside the site's own words: {out}"
    );
    assert_ne!(
        line,
        verbatim_alone,
        "`{}` and `in review` differ only in case, so this states what the \
         assertion above rests on: a build that prints the site's words alone \
         cannot satisfy it",
        support::JIRA_ISSUE_STATUS
    );
}

fn read_the_issue(project: &Scenario) -> std::process::Output {
    let mut command = std::process::Command::new(support::fiddle_binary());
    for name in support::CREDENTIAL_VARS {
        command.env_remove(name);
    }
    command.env_remove(USER_CREDENTIAL);
    command
        .args(["inspect", REFERENCE, "--json"])
        .current_dir(project.dir())
        .env(USER_CREDENTIAL, THROWAWAY_USER)
        .env(TOKEN_CREDENTIAL, THROWAWAY_TOKEN)
        .output()
        .expect("fiddle runs")
}

fn read_the_issue_for_a_reader(project: &Scenario) -> std::process::Output {
    let mut command = std::process::Command::new(support::fiddle_binary());
    for name in support::CREDENTIAL_VARS {
        command.env_remove(name);
    }
    command.env_remove(USER_CREDENTIAL);
    command
        .args(["inspect", REFERENCE])
        .current_dir(project.dir())
        .env(USER_CREDENTIAL, THROWAWAY_USER)
        .env(TOKEN_CREDENTIAL, THROWAWAY_TOKEN)
        .output()
        .expect("fiddle runs")
}

fn ask_over_tcp(base_url: &str, request_line: &str) -> String {
    use std::io::{Read, Write};

    let address = base_url.trim_start_matches("http://").to_string();
    let mut stream =
        std::net::TcpStream::connect(&address).expect("the stub site accepts a connection");
    stream
        .write_all(
            format!("{request_line}\r\nhost: {address}\r\nconnection: close\r\n\r\n").as_bytes(),
        )
        .expect("the stub site reads a request");
    stream.flush().expect("the request leaves");
    let mut said = String::new();
    stream
        .read_to_string(&mut said)
        .expect("the stub site answers");
    said.lines().next().unwrap_or_default().to_string()
}
