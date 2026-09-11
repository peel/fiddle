use fiddle_runtime::capability::{cited, entitled, steers};
use fiddle_runtime::github::{
    read_conversation, read_reviews, GhCli, HumanResponse, Reviewed, CHANGES_REQUESTED,
};
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const PATIENT: Duration = Duration::from_secs(30);
const TEST_TOKEN: &str = "ghp_replay_sentinel_must_not_appear";

const REPO: &str = "snowplow-incubator/snowplow-identities";
const PR: u64 = 270;
const HEAD: &str = "3acb655c27844efd9bbeb7f91691b2c6b71ce4ea";

const REVIEWS: &str = include_str!("fixtures/pr270/reviews.json");
const ISSUE_COMMENTS: &str = include_str!("fixtures/pr270/issue-comments.json");

struct World {
    dir: TempDir,
}

impl World {
    fn of_270() -> Self {
        let world = Self {
            dir: TempDir::new().unwrap(),
        };
        world.serve("reviews", REVIEWS);
        world.serve("issue-comments", ISSUE_COMMENTS);
        world
    }

    fn serve(&self, collection: &str, page: &str) {
        let dir = self.dir.path().join(collection);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("page-1.json"), page).unwrap();
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
}

fn token() -> CancellationToken {
    CancellationToken::new()
}

async fn reviews_of_270() -> Vec<Reviewed> {
    let world = World::of_270();
    read_reviews(&world.gh(), REPO, PR, 10, &token())
        .await
        .unwrap()
}

async fn conversation_of_270() -> Vec<HumanResponse> {
    let world = World::of_270();
    read_conversation(&world.gh(), REPO, PR, 10, &token())
        .await
        .unwrap()
}

fn the_filter_270_was_opened_under(review: &Reviewed) -> bool {
    review.state.eq_ignore_ascii_case(CHANGES_REQUESTED)
        && entitled(&review.author_association)
        && review.commit_id == HEAD
}

#[tokio::test]
async fn the_one_review_on_270_is_entitled_head_matched_and_not_an_approval() {
    let reviews = reviews_of_270().await;

    assert_eq!(
        reviews.len(),
        1,
        "270 carries one review, and this replay reports against that denominator"
    );

    let review = &reviews[0];
    assert_eq!(review.author.login, "spenes");
    assert_eq!(review.author_association, "MEMBER");
    assert_eq!(review.state, "COMMENTED");
    assert_eq!(
        review.commit_id, HEAD,
        "the review was submitted against the current head, so no staleness rule excuses ignoring it"
    );
    assert!(
        review.body.contains("Should we do them"),
        "the review ends in a question addressed to the run, and a question needs an answer"
    );
}

#[tokio::test]
async fn the_review_270_carries_steers_the_run_although_it_asked_for_no_changes() {
    let reviews = reviews_of_270().await;

    let dropped_before: Vec<&Reviewed> = reviews
        .iter()
        .filter(|it| the_filter_270_was_opened_under(it))
        .collect();
    let steering: Vec<&Reviewed> = reviews.iter().filter(|it| steers(it, HEAD)).collect();

    assert_eq!(
        dropped_before.len(),
        0,
        "the filter this pull request was opened under read the review as chatter"
    );
    assert_eq!(
        steering.len(),
        1,
        "a review that is not an approval, from an entitled author, against the current head, \
         steers the run"
    );
    assert!(
        entitled(&reviews[0].author_association) && reviews[0].commit_id == HEAD,
        "entitlement and head-match are not what the old filter excluded it on"
    );
}

#[tokio::test]
async fn the_bot_carrying_the_findings_is_admitted_because_an_entitled_human_names_it() {
    let conversation = conversation_of_270().await;
    let reviews = reviews_of_270().await;

    assert_eq!(
        conversation.len(),
        2,
        "270 carries two conversation comments"
    );

    let bots: Vec<&HumanResponse> = conversation.iter().filter(|it| it.is_bot).collect();
    assert_eq!(
        bots.len(),
        1,
        "the comment carrying the substance is a bot twice over, by account type and by app"
    );
    assert_eq!(bots[0].author.login, "claude[bot]");
    assert!(
        bots[0]
            .body
            .contains("Merge-free batches record a `0` sample"),
        "finding 1 is in the bot comment"
    );
    assert!(
        bots[0].body.contains("`Sampled` has no constructor"),
        "finding 2 is in the bot comment"
    );

    let citing: Vec<String> = reviews
        .iter()
        .filter(|it| entitled(&it.author_association))
        .map(|it| it.body.clone())
        .chain(
            conversation
                .iter()
                .filter(|it| !it.is_bot && entitled(&it.author_association))
                .map(|it| it.body.clone()),
        )
        .collect();

    assert_eq!(
        citing.len(),
        2,
        "one entitled review and one entitled comment can point at a bot, and this is the \
         denominator the admission is decided against"
    );
    assert!(
        cited("claude[bot]", &citing),
        "spenes names the account in both of them, so the findings are admitted"
    );
    assert!(
        !cited("dependabot[bot]", &citing),
        "and a bot nobody named is not admitted, so the rule is not admitting every bot"
    );
}

#[tokio::test]
async fn the_substance_of_270_needs_both_reads_and_neither_alone_carries_it() {
    let reviews = reviews_of_270().await;
    let conversation = conversation_of_270().await;

    let review = &reviews[0];
    assert!(
        review
            .body
            .contains("Commit message and PR description is missing"),
        "one of the two asks is only in the review"
    );
    assert!(
        !conversation.iter().any(|it| it
            .body
            .contains("Commit message and PR description is missing")),
        "and it is in no conversation comment"
    );

    let findings = conversation
        .iter()
        .find(|it| it.author.login == "claude[bot]")
        .expect("the findings comment is in the conversation read");
    assert!(
        !review.body.contains("Merge-free batches"),
        "the other ask is only in the conversation, named by reference from the review"
    );
    assert!(findings.body.contains("Merge-free batches"));
}
