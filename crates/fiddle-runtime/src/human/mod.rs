//! Where a question reaches a person, and how a later reader finds it again.
//!
//! The module the epic's `## Contracts` addresses [`InteractionRef`] to. It is
//! deliberately thin at this point in the milestone: the port that publishes a
//! question and reads the replies back is still to come. What lives here now is
//! the one thing the outcome layer needs before it exists — a name for the
//! conversation a suspended run is waiting on, and a single rendering of it.
//!
//! [`interpret`] is the other half already present: the one bounded model call in
//! the decision walk, whose whole output is one enum and one string.

pub mod interpret;

/// The conversation a question was put on, and where an answer will be found.
///
/// One variant. The RFC's Jira and attended arms are not written, because a
/// variant nothing constructs is the inert surface M2's `RequireHumanDecision`
/// was criticised for being — and adding one later is a line of code, while
/// removing one consumers have matched on is not.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub enum InteractionRef {
    GitHubPullRequestComment { repo: String, pr: u64, comment: u64 },
}

/// One spelling of a conversation, so nothing can disagree about how it is
/// named.
///
/// A suspended run names its conversation in three places — the `--json`
/// outcome's reason, the published bundle's progress entry, and the line a
/// person reads at a terminal — and each of them is produced by different code.
/// Three `format!`s would be three chances for one of them to drift, and the
/// consequence of drifting is an operator who cannot find the pull request a
/// run told them to go and look at. There is one implementation, and every one
/// of those surfaces reaches it.
impl std::fmt::Display for InteractionRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InteractionRef::GitHubPullRequestComment { repo, pr, comment } => {
                write!(f, "{repo}#{pr} comment {comment}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conversation() -> InteractionRef {
        InteractionRef::GitHubPullRequestComment {
            repo: "peel/fiddle-effects-acceptance".to_string(),
            pr: 4,
            comment: 2_147_483_647,
        }
    }

    /// The rendering is the contract, not an incidental of `Debug`. A reader
    /// holding this string can open the pull request and find the comment
    /// without being told the shape of a GitHub URL.
    #[test]
    fn a_conversation_renders_as_the_repository_the_pull_request_and_the_comment() {
        assert_eq!(
            conversation().to_string(),
            "peel/fiddle-effects-acceptance#4 comment 2147483647"
        );
    }

    /// Every component is present, checked separately, so a rendering that
    /// dropped one — the comment id being the easy one to lose, since the pull
    /// request alone looks like enough — fails here rather than in an operator's
    /// hands.
    #[test]
    fn no_component_of_the_conversation_is_dropped() {
        let rendered = conversation().to_string();
        for part in ["peel/fiddle-effects-acceptance", "#4", "2147483647"] {
            assert!(rendered.contains(part), "{part} is missing from {rendered}");
        }
    }
}
