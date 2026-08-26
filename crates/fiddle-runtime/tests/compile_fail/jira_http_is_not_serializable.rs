use fiddle_runtime::jira::JiraHttp;
use std::time::Duration;

fn main() {
    let jira = JiraHttp::new(
        "http://127.0.0.1:1",
        "bot@example.com",
        "s3cr3t",
        Duration::from_secs(5),
    )
    .unwrap();
    let _ = serde_json::to_string(&jira);
}
