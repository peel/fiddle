//! Everything that reaches GitHub.
//!
//! One module, so that "what can this process do to a repository?" is answered
//! by reading one directory rather than by grepping for a hostname. Today it
//! holds [`cli`], the single credential-carrying `gh` construction, and the
//! operations built on top of it: [`refs`] for the branch, [`pulls`] for the
//! pull request and [`checks`] for CI. All of them go through that one
//! construction rather than spawning their own.
//!
//! [`refs`] is the exception that proves it: its *read* is a `gh` call like
//! every other, and its *write* is the one `git push` in [`crate::git`], because
//! a ref can only be created pointing at an object the remote already holds. The
//! operation lives here, beside the read that decides what it means, rather than
//! next to the transport that performs it.

pub mod checks;
pub mod cli;
pub mod pulls;
pub mod refs;

pub use checks::{
    check_request_target, classify, observe_checks, run_name, CheckState, EnsureCheckRequested,
    WorkflowRun,
};
pub use cli::{GhCli, GhError, GhResponse, RetryAdvice};
pub use pulls::{pull_request_target, EnsurePullRequest, PullRequest};
pub use refs::{branch_name, branch_target, BranchRef, EnsureBranchPublished};

/// Percent-encode one query parameter value.
///
/// Written here rather than pulled in, because the whole need is a handful of
/// values in a query string and a dependency added to the impure crate is a
/// dependency the boundary test has to walk. Everything outside RFC 3986's
/// unreserved set is escaped — which includes the `:` of an owner-qualified head
/// and the `/` a namespaced branch carries, the two characters that would
/// otherwise be read as structure by something between here and GitHub.
///
/// One copy for the whole module, because two would be free to disagree about
/// which characters are structure, and the two operations that call it are
/// filtering *reads* whose answers decide whether a write happens.
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
