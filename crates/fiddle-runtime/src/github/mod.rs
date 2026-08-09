//! Everything that reaches GitHub.
//!
//! One module, so that "what can this process do to a repository?" is answered
//! by reading one directory rather than by grepping for a hostname. Today it
//! holds [`cli`], the single credential-carrying `gh` construction, and the
//! operations built on top of it: [`refs`] for the branch and [`pulls`] for the
//! pull request. The check operation lands beside them, and all of them go
//! through that one construction rather than spawning their own.
//!
//! [`refs`] is the exception that proves it: its *read* is a `gh` call like
//! every other, and its *write* is the one `git push` in [`crate::git`], because
//! a ref can only be created pointing at an object the remote already holds. The
//! operation lives here, beside the read that decides what it means, rather than
//! next to the transport that performs it.

pub mod cli;
pub mod pulls;
pub mod refs;

pub use cli::{GhCli, GhError, GhResponse};
pub use pulls::{pull_request_target, EnsurePullRequest, PullRequest};
pub use refs::{branch_name, branch_target, BranchRef, EnsureBranchPublished};
