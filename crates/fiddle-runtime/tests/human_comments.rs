//! The conversation adapter: what it reads, how far it reads, and what it
//! refuses.
//!
//! Two properties carry the weight here and both are about failing closed. The
//! first is that the read is *complete* — every page, or an error naming the
//! bound, because a truncated read that found no approval and a whole read that
//! found none are different facts and only one of them is a decision. The
//! second is that an unreadable conversation is never an empty one, which is
//! the rule `observe_checks` already applies at the checks boundary.
//!
//! A third is about which collection is the conversation. Inline review
//! comments live at `/pulls/{n}/comments` and are not consulted for a
//! work-level decision, so the case below puts an approval there and asserts
//! the endpoint was never requested at all — not that its content was ignored.
//!
//! Driven through the product's `cli.program` seam against the scripted `gh` in
//! `tests/gh_stub/`, like `github_cli` and `github_graphql`. Nothing here
//! reaches GitHub.

use fiddle_runtime::github::{read_conversation, read_one_comment, GhCli};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// A generous bound for a stub that answers immediately. No case here is about
/// the deadline; `github_cli` owns that one, and it is the same bound.
const PATIENT: Duration = Duration::from_secs(30);

/// The credential the client is built with. Sentinel-shaped, like every other
/// suite's, so nothing here can pass against an empty string.
const TEST_TOKEN: &str = "ghp_conversation_sentinel_must_not_appear";

/// A run nobody interrupted.
fn token() -> CancellationToken {
    CancellationToken::new()
}

/// A scratch world: a scripted `gh`, the two comment collections it answers
/// from, and the requests it recorded.
struct World {
    dir: TempDir,
}

impl World {
    fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
        }
    }

    /// A `GhCli` pointed at the scripted `gh`.
    ///
    /// The stub's scratch directory arrives through `cli.args`, not through the
    /// environment, for the reason the fixture's own header gives: the adapter
    /// clears the environment and sets exactly five names, so a sixth could not
    /// reach the child even if the test wanted one.
    fn gh(&self) -> GhCli {
        let config = self.dir.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        GhCli::new(
            PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
            vec![
                "--stub-dir".to_string(),
                self.dir.path().display().to_string(),
            ],
            TEST_TOKEN.to_string(),
            "FIDDLE_GITHUB_TOKEN",
            config,
            PATIENT,
        )
    }

    /// One page of one collection. The stub emits a `rel="next"` while a
    /// further page exists, so scripting page `k + 1` is what makes page `k`
    /// carry one.
    fn page(&self, collection: &str, page: u64, comments: &[serde_json::Value]) {
        let dir = self.dir.path().join(collection);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("page-{page}.json")),
            serde_json::Value::Array(comments.to_vec()).to_string(),
        )
        .unwrap();
    }

    /// One comment answered by its own id, which is a different route from the
    /// listing and is scripted separately.
    fn by_id(&self, collection: &str, id: u64, comment: &serde_json::Value) {
        let dir = self.dir.path().join(collection).join("by-id");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.json")), comment.to_string()).unwrap();
    }

    /// Make one collection unreadable, with the status GitHub answered.
    fn script_status(&self, collection: &str, status: u16) {
        std::fs::write(
            self.dir.path().join(format!("{collection}-unreadable")),
            status.to_string(),
        )
        .unwrap();
    }

    /// Every API path the stub was asked for, in arrival order.
    ///
    /// Read out of the recorded `argv` rather than out of anything the adapter
    /// reports about itself, so "the endpoint was never asked for" is a claim
    /// about the child that ran.
    fn recorded_paths(&self) -> Vec<String> {
        let requests = self.dir.path().join("requests");
        let Ok(entries) = std::fs::read_dir(&requests) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries.map(|entry| entry.unwrap().path()).collect();
        // Named `0000.json` upward by arrival, so the order is the filename's.
        files.sort();
        files
            .iter()
            .filter_map(|file| {
                let recorded: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
                recorded["argv"]
                    .as_array()?
                    .iter()
                    .filter_map(|arg| arg.as_str())
                    .find(|arg| arg.starts_with('/'))
                    .map(str::to_string)
            })
            .collect()
    }
}

/// An ordinary comment by a person, in the shape the listing returns.
fn comment(id: u64, login: &str, user_id: u64, body: &str) -> serde_json::Value {
    json!({
        "id": id,
        "body": body,
        "created_at": "2026-08-10T00:00:00Z",
        "updated_at": "2026-08-10T00:00:00Z",
        "author_association": "OWNER",
        "user": { "login": login, "id": user_id, "type": "User" },
        "performed_via_github_app": null,
    })
}

/// A comment exactly as written, for the cases whose subject is the payload
/// itself — a field missing, an app that authored it, a timestamp that moved.
fn raw_comment(value: serde_json::Value) -> serde_json::Value {
    value
}

/// Every page, and not just the first. A conversation long enough to paginate
/// is exactly where an approval hides.
#[tokio::test]
async fn every_page_of_the_conversation_is_read() {
    let world = World::new();
    world.page("issue-comments", 1, &[comment(1, "peel", 505401, "first")]);
    world.page("issue-comments", 2, &[comment(2, "peel", 505401, "second")]);
    world.page("issue-comments", 3, &[comment(3, "peel", 505401, "third")]);
    let all = read_conversation(&world.gh(), "acme/r", 7, 10, &token())
        .await
        .unwrap();
    assert_eq!(all.iter().map(|c| c.comment).collect::<Vec<_>>(), [1, 2, 3]);
}

/// The bound is a bound. A conversation longer than max_pages is refused rather
/// than silently truncated, because a truncated read that found no approval and
/// one that found none are different facts.
#[tokio::test]
async fn a_conversation_longer_than_the_bound_is_refused_not_truncated() {
    let world = World::new();
    for k in 1..=4 {
        world.page("issue-comments", k, &[comment(k, "peel", 505401, "x")]);
    }
    let err = read_conversation(&world.gh(), "acme/r", 7, 2, &token())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("more than 2 pages"), "got {err}");
}

/// Inline review comments are a different collection and are never consulted for
/// a work-level decision. A decoy approval there must not be seen.
#[tokio::test]
async fn inline_review_comments_are_never_read() {
    let world = World::new();
    world.page(
        "issue-comments",
        1,
        &[comment(1, "peel", 505401, "thinking about it")],
    );
    world.page(
        "review-comments",
        1,
        &[comment(99, "peel", 505401, "approve")],
    );
    let all = read_conversation(&world.gh(), "acme/r", 7, 10, &token())
        .await
        .unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].comment, 1);
    // And the endpoint was never even asked for.
    assert!(!world
        .recorded_paths()
        .iter()
        .any(|p| p.contains("/pulls/7/comments")));
}

/// The listing carries everything an actor check needs, so nothing here costs a
/// second call per comment.
#[tokio::test]
async fn a_response_carries_actor_identity_and_edit_state_from_the_listing_alone() {
    let world = World::new();
    world.page(
        "issue-comments",
        1,
        &[raw_comment(json!({
            "id": 42, "body": "approve", "created_at": "2026-08-10T00:00:00Z",
            "updated_at": "2026-08-10T00:00:01Z", "author_association": "OWNER",
            "user": {"login": "peel", "id": 505401, "type": "User"},
            "performed_via_github_app": null
        }))],
    );
    let all = read_conversation(&world.gh(), "acme/r", 7, 10, &token())
        .await
        .unwrap();
    let r = &all[0];
    assert_eq!(
        (r.comment, r.author.id, r.author.login.as_str()),
        (42, 505401, "peel")
    );
    assert_eq!(r.created_at, "2026-08-10T00:00:00Z");
    assert_eq!(r.updated_at, "2026-08-10T00:00:01Z");
    assert_eq!(r.body, "approve");
    assert!(!r.is_bot);
    assert_eq!(r.author_association, "OWNER");
    assert_eq!(world.recorded_paths().len(), 1);
}

/// Two ways of being not a person, and both are recognised. A bot's comment is
/// read and recorded; what it is not is a human decision.
#[tokio::test]
async fn a_bot_and_an_app_are_both_marked_not_a_person() {
    let world = World::new();
    world.page(
        "issue-comments",
        1,
        &[
            raw_comment(json!({"id": 1, "body": "approve", "created_at": "t", "updated_at": "t",
            "author_association": "NONE", "user": {"login": "dependabot[bot]", "id": 1, "type": "Bot"},
            "performed_via_github_app": null})),
            raw_comment(json!({"id": 2, "body": "approve", "created_at": "t", "updated_at": "t",
            "author_association": "NONE", "user": {"login": "svc", "id": 2, "type": "User"},
            "performed_via_github_app": {"id": 9, "slug": "some-app"}})),
        ],
    );
    let all = read_conversation(&world.gh(), "acme/r", 7, 10, &token())
        .await
        .unwrap();
    assert!(
        all.iter().all(|c| c.is_bot),
        "both must be marked, got {all:?}"
    );
}

/// One comment by its own id, which is what step 5 of the validation order uses
/// to find out whether it changed since it was listed.
#[tokio::test]
async fn one_comment_can_be_re_read_by_its_own_id() {
    let world = World::new();
    world.by_id(
        "issue-comments",
        42,
        &raw_comment(json!({"id": 42, "body": "approve",
        "created_at": "t0", "updated_at": "t9", "author_association": "OWNER",
        "user": {"login": "peel", "id": 505401, "type": "User"},
        "performed_via_github_app": null})),
    );
    let one = read_one_comment(&world.gh(), "acme/r", 42, &token())
        .await
        .unwrap();
    assert_eq!(one.updated_at, "t9");
}

/// Fails closed, the way `observe_checks` does. An unreadable conversation is an
/// error, never an empty list — because empty means "nobody has answered", and
/// that is a decision.
#[tokio::test]
async fn an_unreadable_conversation_is_an_error_and_never_an_empty_list() {
    let world = World::new();
    world.script_status("issue-comments", 500);
    let err = read_conversation(&world.gh(), "acme/r", 7, 10, &token()).await;
    assert!(err.is_err(), "a 500 must not read as an empty conversation");
}

/// A malformed comment is not skipped. A conversation the adapter can only
/// partly parse is one whose approval it may have been the part it could not.
#[tokio::test]
async fn a_comment_missing_a_field_refuses_the_whole_read() {
    let world = World::new();
    world.page(
        "issue-comments",
        1,
        &[raw_comment(json!({"id": 1, "body": "approve"}))],
    );
    assert!(read_conversation(&world.gh(), "acme/r", 7, 10, &token())
        .await
        .is_err());
}
