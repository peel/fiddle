use fiddle_runtime::effect::Effect;

#[derive(Effect)]
#[effect(
    name = "jira.issue_transitioned",
    minimum = "automatic",
    target = "{issue_key}@{issue_updated}",
    state = (),
    error = ()
)]
struct JiraTargetWithoutRevision {
    #[payload]
    issue_key: String,
}

fn main() {}
