mod support;

use support::{Scenario, StubJira};

const TOKEN_CREDENTIAL: &str = "JIRA_API_TOKEN";

const USER_CREDENTIAL: &str = "JIRA_USER_EMAIL";

const THROWAWAY_USER: &str = "nobody@example.com";

const THROWAWAY_TOKEN: &str = "a-token-no-site-would-honour";

const REFERENCE: &str = "jira:IDENT-1";

const REVISION: &str = "2026-08-25T20:00:00Z";

#[test]
fn inspect_reports_a_jira_issue_through_the_public_cli() {
    let stub = StubJira::holding_the_issue();
    let project = disposable_project_reading(&stub.base_url());

    let mut command = std::process::Command::new(support::fiddle_binary());
    for name in support::CREDENTIAL_VARS {
        command.env_remove(name);
    }
    command.env_remove(USER_CREDENTIAL);
    let run = command
        .args(["inspect", REFERENCE, "--json"])
        .current_dir(project.dir())
        .env(USER_CREDENTIAL, THROWAWAY_USER)
        .env(TOKEN_CREDENTIAL, THROWAWAY_TOKEN)
        .output()
        .expect("fiddle runs");

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
        observed["value"]["projected"]["state"],
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
