pub mod checks;
pub mod cli;
pub mod comments;
pub mod pulls;
pub mod ready;
pub mod refs;

pub use checks::{
    check_request_target, classify, observe_checks, observe_genuine_failure, run_name, BlamedCheck,
    CheckState, EnsureCheckRequested, GenuineFailure, Settlement, WorkflowRun,
};
pub use cli::{GhCli, GhError, GhResponse, RetryAdvice};
pub use comments::{
    read_conversation, read_line_comments, read_one_comment, read_reviews, Annotated,
    HumanResponse, Reviewed, CHANGES_REQUESTED,
};
pub use pulls::{
    find_labelled_pull_request, pull_request_body_target, pull_request_target,
    read_pull_request_body, EnsurePullRequest, EnsurePullRequestBody, PullRequest, PullRequestBody,
    SharedPullRequest,
};
pub use ready::{pull_request_ready_target, EnsurePullRequestReady, ReadyPullRequest};
pub use refs::{branch_name, branch_target, BranchRef, EnsureBranchPublished};

pub(crate) fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}
