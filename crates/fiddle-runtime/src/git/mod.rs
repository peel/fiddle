//! Everything this runtime does with `git` against a remote it must
//! authenticate to.
//!
//! Kept apart from [`crate::github`] on purpose, even though both end up at the
//! same host. `github` is the `gh` client: one credential in one environment,
//! speaking the REST API. This is a different program, a different credential
//! channel and a different environment, and folding them together would mean one
//! module whose "what can the child see?" question has two answers.
//!
//! Kept apart from [`crate::workspace::command`] for the stronger version of the
//! same reason: a workspace check runs under four names built around a scratch
//! `HOME` and must never see a credential, while a push must see exactly one and
//! must not see a `HOME` at all. Three spawn sites, three environments, each
//! argued for where it is built — and one shared bound, in [`crate::process`],
//! which is the only thing they have in common.

pub mod publish;

pub use publish::{GitCli, GitError, PublishedBranch};
