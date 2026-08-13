//! The conversation of a pull request, read whole or not at all.
//!
//! A decision this system acts on is a sentence somebody typed into a pull
//! request, and this module is the only way that sentence reaches the process.
//! Everything below follows from that: what is read, how much of it is read,
//! and what happens when it cannot be.
//!
//! # One collection is the conversation, and the other one is not
//!
//! GitHub keeps two comment collections for a pull request.
//! `/issues/{n}/comments` is the conversation — the timeline a person types
//! into — and `/pulls/{n}/comments` is the inline review comments, each pinned
//! to a line of a diff. They do not overlap: a comment in one is not returned by
//! the other.
//!
//! Only the first is read here, and the RFC's reason is that an inline comment
//! is about a line rather than about the work. That makes the choice a
//! *reachability* property rather than a filtering one: this module has no path
//! that names `/pulls/{n}/comments`, so an approval typed there is not ignored,
//! it is unreachable. `inline_review_comments_are_never_read` asserts the
//! endpoint was never requested, which is the only form of that claim a filter
//! could not also satisfy.
//!
//! # Every page, or an error
//!
//! A conversation long enough to paginate is exactly where a late approval
//! sits, so a read that stops early is a read that can miss the answer while
//! looking like it found none. Pages are followed while the response's `Link`
//! header carries `rel="next"` — the only thing that actually says another page
//! exists. Counting what came back does not: the API's page size is its choice
//! and a short page is not an end. Every RFC 8288 spelling of that relation is
//! read as one, and a header [`has_a_next_page`] cannot interpret at all is
//! reported as a further page rather than as an end — because the doubt has to
//! resolve somewhere, and only one of the two answers is a failure anybody sees.
//!
//! The bound is [`read_conversation`]'s `max_pages`, and reaching it is an
//! error rather than a truncation. "I read everything and found no approval"
//! and "I read as much as I was allowed and found no approval" are different
//! facts, and only the first of them is a decision.
//!
//! # And an unreadable conversation is never an empty one
//!
//! The same rule [`observe_checks`](super::checks::observe_checks) applies to
//! CI, for the same reason. An empty conversation means nobody has answered,
//! which is a fact this system acts on by continuing to wait; a conversation
//! that could not be read is not that fact. So every failure — the call, a body
//! that is not a list of comments, one element missing a field — refuses the
//! whole read. A partial list is the worst of the three: it is the shape that
//! silently answers the question with the half of the conversation that
//! happened to parse.
//!
//! And §5.6 is why it is the worst. The decision belongs to the *last*
//! authorized reply, chosen so that an approval followed by "wait, no" does not
//! mutate. Truncate between those two comments and the read succeeds carrying the
//! approval alone, so the run acts on information already known to be superseded:
//! the one case that rule exists to protect is the case a silent truncation
//! defeats. Every choice about pagination here follows from that, including which
//! way an ambiguous `Link` header is resolved.

use crate::github::{GhCli, GhError};
use fiddle_core::ActorRef;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

/// GitHub's maximum, so a conversation costs as few calls as it can.
///
/// It is not a bound on what is read — [`read_conversation`]'s `max_pages` is —
/// and nothing here decides anything from a page's size.
const PER_PAGE: u32 = 100;

/// One comment, as much of it as a decision needs.
///
/// Everything here comes off the listing. That is worth stating because the
/// alternative was real: an actor check needs the author's immutable id, their
/// account type, whether an app posted on their behalf and their association
/// with the repository, and a client that fetched any of those per comment
/// would turn one call into one call per comment. The listing returns all four,
/// so it does not.
///
/// `created_at` and `updated_at` are both carried, and they are not redundant:
/// they are equal on a comment nobody has touched, so an inequality is the
/// evidence that a comment was edited. Step 5 of the validation order re-reads
/// a candidate by its own id and compares — an approval that was rewritten
/// after it was listed is not an approval this run may act on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanResponse {
    /// The comment's own id, which is what a re-read is addressed by.
    pub comment: u64,
    /// Who wrote it, in the domain's own spelling rather than this adapter's.
    /// [`ActorRef`] lives in `fiddle-core` because the allowlist that consults
    /// it is domain logic; what this module does is fill it in from a listing.
    pub author: ActorRef,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
    /// Not a person. Both of GitHub's ways of being one: an account whose type
    /// is `Bot`, and a comment an app posted through a user's credential.
    ///
    /// The comment is still read and still recorded. What it is not is a human
    /// decision, and keeping the flag rather than dropping the comment is what
    /// lets a refusal say which comment it declined to count.
    pub is_bot: bool,
    /// `OWNER`, `MEMBER`, `COLLABORATOR`, `CONTRIBUTOR`, `NONE` and the rest.
    /// Carried as GitHub spells it rather than parsed into an enum, because
    /// this module's job is to report what the listing said.
    pub author_association: String,
}

/// The fields of a comment this milestone reads, and no others.
///
/// Narrow on purpose and without `deny_unknown_fields`: GitHub adds keys to
/// this payload and a client that refused an unfamiliar one would break on a
/// change that concerns it not at all. What it will not do is default a field
/// it needs — every one below is required, so a payload missing `created_at`
/// is an error rather than a comment that was created at the empty string.
///
/// `performed_via_github_app` is a `Value` rather than an `Option`, and that is
/// the same rule rather than an exception to it. Serde lets an `Option` field
/// be absent, so declaring it one would read a payload that never mentioned an
/// app as a payload that denied one — which is a human decision inferred from a
/// field that was not there. As a `Value` the key must be present, and the
/// `null` GitHub actually sends is what says no app was involved.
#[derive(Debug, Deserialize)]
struct ListedComment {
    id: u64,
    body: String,
    created_at: String,
    updated_at: String,
    author_association: String,
    user: ListedUser,
    performed_via_github_app: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ListedUser {
    login: String,
    id: u64,
    /// `User`, `Bot`, `Organization`. `type` is a keyword, hence the rename.
    #[serde(rename = "type")]
    kind: String,
}

impl From<ListedComment> for HumanResponse {
    fn from(listed: ListedComment) -> Self {
        Self {
            comment: listed.id,
            is_bot: listed.user.kind == "Bot" || !listed.performed_via_github_app.is_null(),
            author: ActorRef {
                login: listed.user.login,
                id: listed.user.id,
            },
            body: listed.body,
            created_at: listed.created_at,
            updated_at: listed.updated_at,
            author_association: listed.author_association,
        }
    }
}

/// Every comment in a pull request's conversation, oldest first.
///
/// Oldest first because that is the order GitHub returns and the order the last
/// -reply rule depends on: a decision is read from the comments that came after
/// the question, and "after" is only meaningful in a sequence nothing here
/// reorders.
///
/// Errors rather than truncates at `max_pages`, and errors rather than empties
/// on anything it could not read. Both are the module's fail-closed rule; see
/// its documentation for why an empty list is a claim this function must not
/// make lightly.
pub async fn read_conversation(
    gh: &GhCli,
    repo: &str,
    pr: u64,
    max_pages: u32,
    cancel: &CancellationToken,
) -> Result<Vec<HumanResponse>, GhError> {
    let mut conversation = Vec::new();
    for page in 1..=max_pages {
        let path = format!("/repos/{repo}/issues/{pr}/comments?per_page={PER_PAGE}&page={page}");
        let response = gh.api("GET", &path, None, cancel).await?;
        // Read before the body is consumed, and the only thing that says
        // whether this page was the last one.
        let more = has_a_next_page(response.link.as_deref());
        let listed: Vec<ListedComment> =
            serde_json::from_value(response.body).map_err(|error| {
                // The whole page, not the elements that happened to parse. A
                // conversation this client can only partly read is one whose
                // approval may be the part it could not.
                GhError::Malformed(format!(
                    "{path} answered something that is not a list of comments: {error}"
                ))
            })?;
        conversation.extend(listed.into_iter().map(HumanResponse::from));
        if !more {
            return Ok(conversation);
        }
    }
    // Reached only with a `rel="next"` in hand, so this is a conversation that
    // demonstrably continues past where the read was allowed to go. Reported as
    // a failure because the alternative is to answer a question about the whole
    // conversation having seen part of it.
    Err(GhError::Malformed(format!(
        "the conversation of {repo}#{pr} runs to more than {max_pages} pages and was not read"
    )))
}

/// One comment, by its own id.
///
/// The listing's own answer is a snapshot, and this is how a caller finds out
/// whether it still holds: step 5 of the validation order re-reads each
/// candidate here and refuses one whose `updated_at` has moved since it was
/// listed. The path is `/issues/comments/{id}` — the conversation collection's
/// by-id route, which carries no pull-request number because a comment id is
/// unique within the repository.
pub async fn read_one_comment(
    gh: &GhCli,
    repo: &str,
    comment: u64,
    cancel: &CancellationToken,
) -> Result<HumanResponse, GhError> {
    let path = format!("/repos/{repo}/issues/comments/{comment}");
    let response = gh.api("GET", &path, None, cancel).await?;
    let listed: ListedComment = serde_json::from_value(response.body).map_err(|error| {
        GhError::Malformed(format!("{path} answered no readable comment: {error}"))
    })?;
    Ok(listed.into())
}

/// Whether a `Link` header says there is another page.
///
/// The relation is what is looked for, never the header's presence: GitHub
/// sends a `Link` on the *last* page too, carrying `rel="prev"` and
/// `rel="first"`, so a client that followed pages while a header existed would
/// walk to its own bound on every conversation longer than one page.
///
/// Only the parameters of each link-value are examined, never the URL inside the
/// angle brackets. A page cursor is opaque text this client passes on rather
/// than one it chose, and a URL containing the characters of a relation must
/// not be able to claim to be one. For the same reason the parameter's name must
/// be exactly `rel`, so a relation spelled inside some other parameter's value —
/// `title="rel=next"` — is not a relation either.
///
/// # Every legal spelling of `next`, and which way to be wrong
///
/// RFC 8288 gives a relation more shapes than GitHub happens to send. The value
/// is a space-separated *set* of relation types, so `rel="next last"` is a next;
/// the quotes are optional, so `rel=next` is one; whitespace is legal around the
/// `=`, so `rel = "next"` is one; and a relation need not be the first parameter.
/// Each is read here by parsing the parameter — split at its first `=`, trim both
/// halves, unquote the value, and look for `next` among the value's tokens —
/// rather than by comparing the whole parameter against a single spelling, which
/// is what this function used to do.
///
/// The direction of that old error is the point. Comparing against one spelling
/// answered *"there are no more pages"* for every other legal one, and for this
/// module that is the wrong way to be wrong. A header this parser cannot
/// interpret means **"I could not read this"** and never "the conversation ends
/// here": the first walks the read to [`read_conversation`]'s bound and fails
/// there, which is a refusal an operator investigates, and the second is a silent
/// successful truncation somebody acts on. So a header that is present but holds
/// no link-value this function recognises is reported as a further page. A header
/// GitHub never sent is still the end — an unpaginated response carries no
/// `Link` at all, and that absence is GitHub saying so rather than this function
/// failing to read something.
///
/// What makes the direction load-bearing rather than tidy is §5.6: the decision
/// belongs to the *last* authorized reply, chosen exactly so that an approval
/// followed by "wait, no" does not mutate. Truncate between the two — approval on
/// page one, retraction on page two — and the read returns `Ok` carrying the
/// approval alone and the run proceeds on information already known to be
/// superseded. The one case §5.6 exists to protect is the case a truncation
/// defeats. A module whose whole subject is completeness must not resolve doubt
/// toward incompleteness.
fn has_a_next_page(link: Option<&str>) -> bool {
    let header = link.unwrap_or_default().trim();
    if header.is_empty() {
        // The one unambiguous end. GitHub omits the header entirely on a
        // response it did not paginate, so nothing was misread here.
        return false;
    }
    let mut read_a_link_value = false;
    for segment in header.split(',') {
        // The target ends at the last `>`, and only what follows it is
        // parameters — so a cursor that spells a relation is inside the URL and
        // never inspected. A segment with no target is a continuation of the
        // previous link-value's parameter list, which is how a relation that
        // follows another parameter is reached.
        let parameters = match segment.rsplit_once('>') {
            Some((_target, parameters)) => {
                read_a_link_value = true;
                parameters
            }
            None => segment,
        };
        if parameters.split(';').any(is_a_next_relation) {
            return true;
        }
    }
    // Present, and not one link-value in it. Not an end — unreadable.
    !read_a_link_value
}

/// Whether one `Link` parameter is a relation set holding `next`.
///
/// Split at the *first* `=` so that a value containing one keeps it, and the name
/// is compared whole so that `rel` is the parameter's name rather than a
/// substring of a longer one. Unquoting with [`str::trim_matches`] accepts the
/// unbalanced quote a stricter parser would reject, which is the same direction
/// as everything else here: a relation half-legibly spelled `next` is a page this
/// client should go and read.
fn is_a_next_relation(parameter: &str) -> bool {
    let Some((name, value)) = parameter.split_once('=') else {
        return false;
    };
    if !name.trim().eq_ignore_ascii_case("rel") {
        return false;
    }
    value
        .trim()
        .trim_matches('"')
        .split_whitespace()
        .any(|relation| relation.eq_ignore_ascii_case("next"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The header GitHub sends on every page but the last.
    #[test]
    fn a_next_relation_is_a_further_page() {
        assert!(has_a_next_page(Some(
            "<https://api.github.com/repositories/1/issues/7/comments?page=2>; rel=\"next\", \
             <https://api.github.com/repositories/1/issues/7/comments?page=9>; rel=\"last\""
        )));
    }

    /// And the one it sends on the last page, which is a header with no `next`
    /// in it. A client reading the header's presence rather than its relations
    /// would read this as another page and never stop.
    #[test]
    fn a_last_page_still_carries_a_link_and_is_still_the_end() {
        assert!(!has_a_next_page(Some(
            "<https://api.github.com/repositories/1/issues/7/comments?page=1>; rel=\"first\", \
             <https://api.github.com/repositories/1/issues/7/comments?page=8>; rel=\"prev\""
        )));
        assert!(!has_a_next_page(None));
    }

    /// A relation is a parameter and never an address. The cursor here is a
    /// value GitHub chose and this client passes on, so a URL that spells one
    /// must not be able to claim to be one.
    #[test]
    fn a_url_that_reads_like_a_relation_is_not_one() {
        assert!(!has_a_next_page(Some(
            "<https://api.github.com/x?cursor=rel%3D%22next%22&z=;rel=\"next\">; rel=\"prev\""
        )));
    }

    /// The spellings RFC 8288 allows and GitHub does not happen to use. Each was
    /// read as an end of pages by a parser that compared the whole parameter
    /// against `rel=next`, which is a silent truncation of the conversation.
    #[test]
    fn every_legal_spelling_of_next_is_a_further_page() {
        for header in [
            // A relation list: the value is a set of relation types.
            r#"<https://api.github.com/x?page=2>; rel="next last""#,
            // Whitespace is legal around the '='.
            r#"<https://api.github.com/x?page=2>; rel = "next""#,
            // The quotes are optional on a single token.
            r#"<https://api.github.com/x?page=2>; rel=next"#,
            // Relation types are case-insensitive.
            r#"<https://api.github.com/x?page=2>; rel="NEXT""#,
            // And a relation need not come first.
            r#"<https://api.github.com/x?page=2>; type="application/json"; rel="next""#,
            // Nor need it be in the first link-value.
            r#"<https://api.github.com/x?page=1>; rel="first", <https://api.github.com/x?page=2>; rel="prev next""#,
        ] {
            assert!(has_a_next_page(Some(header)), "{header:?} read as an end");
        }
    }

    /// And widening what counts as a relation must not widen what counts as
    /// `next`. A relation named inside another parameter's value is not one, a
    /// longer token beginning with it is not one, and neither is a relation this
    /// client has no interest in.
    #[test]
    fn a_relation_named_somewhere_else_is_not_a_further_page() {
        for header in [
            r#"<https://api.github.com/x?page=2>; title="rel=next""#,
            r#"<https://api.github.com/x/next?page=2>; rel="prev""#,
            r#"<https://api.github.com/x?page=2>; rel="nextish""#,
            r#"<https://api.github.com/x?page=2>; relation="next""#,
            r#"<https://api.github.com/x?page=2>; rel="prev first""#,
        ] {
            assert!(
                !has_a_next_page(Some(header)),
                "{header:?} read as a further page"
            );
        }
    }

    /// A header holding nothing this function recognises is not an end of pages.
    /// The read walks to its bound and refuses there, because that refusal is a
    /// thing an operator sees and a truncated conversation is not.
    #[test]
    fn a_header_that_cannot_be_read_is_not_an_end() {
        assert!(has_a_next_page(Some("something else entirely")));
        assert!(has_a_next_page(Some("https://api.github.com/x?page=2")));
        // Whereas no header, and a header holding nothing at all, are the end:
        // GitHub sends no `Link` on a response it did not paginate.
        assert!(!has_a_next_page(None));
        assert!(!has_a_next_page(Some("   ")));
    }
}
