use fiddle_runtime::github::{read_conversation, read_one_comment, GhCli};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const PATIENT: Duration = Duration::from_secs(30);

const TEST_TOKEN: &str = "ghp_conversation_sentinel_must_not_appear";

fn token() -> CancellationToken {
    CancellationToken::new()
}

struct World {
    dir: TempDir,
}

impl World {
    fn new() -> Self {
        Self {
            dir: TempDir::new().unwrap(),
        }
    }

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

    fn page(&self, collection: &str, page: u64, comments: &[serde_json::Value]) {
        let dir = self.dir.path().join(collection);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("page-{page}.json")),
            serde_json::Value::Array(comments.to_vec()).to_string(),
        )
        .unwrap();
    }

    fn link(&self, collection: &str, page: u64, header: &str) {
        let dir = self.dir.path().join(collection);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("page-{page}.link")), header).unwrap();
    }

    fn by_id(&self, collection: &str, id: u64, comment: &serde_json::Value) {
        let dir = self.dir.path().join(collection).join("by-id");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{id}.json")), comment.to_string()).unwrap();
    }

    fn script_status(&self, collection: &str, status: u16) {
        std::fs::write(
            self.dir.path().join(format!("{collection}-unreadable")),
            status.to_string(),
        )
        .unwrap();
    }

    fn recorded_paths(&self) -> Vec<String> {
        let requests = self.dir.path().join("requests");
        let Ok(entries) = std::fs::read_dir(&requests) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries.map(|entry| entry.unwrap().path()).collect();
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

fn raw_comment(value: serde_json::Value) -> serde_json::Value {
    value
}

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

#[tokio::test]
async fn exactly_max_pages_is_read_and_not_refused() {
    let world = World::new();
    for k in 1..=3 {
        world.page("issue-comments", k, &[comment(k, "peel", 505401, "x")]);
    }
    let all = read_conversation(&world.gh(), "acme/r", 7, 3, &token())
        .await
        .unwrap();
    assert_eq!(all.iter().map(|c| c.comment).collect::<Vec<_>>(), [1, 2, 3]);
}

#[tokio::test]
async fn a_relation_list_and_sloppy_whitespace_are_still_a_next_page() {
    const PAGE_2: &str = "https://api.github.com/repositories/1/issues/7/comments?page=2";
    for header in [
        format!(r#"<{PAGE_2}>; rel="next last""#),
        format!(r#"<{PAGE_2}>; rel = "next""#),
        format!(r#"<{PAGE_2}>; rel=next"#),
        format!(r#"<{PAGE_2}>; type="application/json", rel="next""#),
        format!(r#"<{PAGE_2}>; rel="NEXT""#),
    ] {
        let world = World::new();
        world.page(
            "issue-comments",
            1,
            &[comment(1, "peel", 505401, "approve")],
        );
        world.page(
            "issue-comments",
            2,
            &[comment(2, "peel", 505401, "wait, no")],
        );
        world.link("issue-comments", 1, &header);
        let all = read_conversation(&world.gh(), "acme/r", 7, 10, &token())
            .await
            .unwrap_or_else(|error| panic!("{header:?} was not read at all: {error}"));
        assert_eq!(all.len(), 2, "{header:?} was read as an end of pages");
        assert_eq!(all[1].body, "wait, no");
    }
}

#[tokio::test]
async fn a_relation_named_somewhere_else_is_not_a_next_page() {
    const PAGE_2: &str = "https://api.github.com/repositories/1/issues/7/comments?page=2";
    const SPELLS_IT_IN_THE_PATH: &str =
        "https://api.github.com/repositories/1/issues/7/next?page=2";
    for header in [
        format!(r#"<{PAGE_2}>; title="rel=next""#),
        format!(r#"<{SPELLS_IT_IN_THE_PATH}>; rel="prev""#),
        format!(r#"<{PAGE_2}>; rel="nextish""#),
        format!(r#"<{PAGE_2}>; rel="prev first""#),
    ] {
        let world = World::new();
        world.page("issue-comments", 1, &[comment(1, "peel", 505401, "only")]);
        world.page(
            "issue-comments",
            2,
            &[comment(2, "peel", 505401, "unreachable")],
        );
        world.link("issue-comments", 1, &header);
        let all = read_conversation(&world.gh(), "acme/r", 7, 10, &token())
            .await
            .unwrap();
        assert_eq!(all.len(), 1, "{header:?} was read as a next page");
        assert_eq!(all[0].body, "only");
    }
}

#[tokio::test]
async fn a_link_header_that_cannot_be_read_is_refused_not_treated_as_the_end() {
    let world = World::new();
    world.page(
        "issue-comments",
        1,
        &[comment(1, "peel", 505401, "approve")],
    );
    world.page(
        "issue-comments",
        2,
        &[comment(2, "peel", 505401, "wait, no")],
    );
    for page in 1..=2 {
        world.link(
            "issue-comments",
            page,
            "something that is not a link at all",
        );
    }
    let err = read_conversation(&world.gh(), "acme/r", 7, 2, &token())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("more than 2 pages"), "got {err}");
    assert_eq!(
        world.recorded_paths().len(),
        2,
        "the read stopped instead of walking: {:?}",
        world.recorded_paths()
    );
}

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
    assert!(!world
        .recorded_paths()
        .iter()
        .any(|p| p.contains("/pulls/7/comments")));
}

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

#[tokio::test]
async fn an_unreadable_conversation_is_an_error_and_never_an_empty_list() {
    let world = World::new();
    world.script_status("issue-comments", 500);
    let err = read_conversation(&world.gh(), "acme/r", 7, 10, &token()).await;
    assert!(err.is_err(), "a 500 must not read as an empty conversation");
}

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
