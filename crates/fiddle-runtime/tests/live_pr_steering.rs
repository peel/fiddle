use fiddle_runtime::capability::Direction;
use fiddle_runtime::github::{open_pull_request_on, read_conversation, read_reviews, GhCli};
use std::path::PathBuf;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const PATIENT: Duration = Duration::from_secs(60);
const PAGES: u32 = 10;

fn required(name: &str) -> String {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!(
            "{name} is unset, and this lane refuses rather than skips: a lane that passed by \
             reading nothing cannot be told from one that passed by reading a real pull request"
        ),
    }
}

fn gh() -> GhCli {
    let config = std::env::temp_dir().join("fiddle-live-pr-steering-gh");
    std::fs::create_dir_all(&config).unwrap();
    GhCli::new(
        PathBuf::from("gh"),
        Vec::new(),
        required("FIDDLE_LIVE_TOKEN"),
        "FIDDLE_LIVE_TOKEN",
        config,
        PATIENT,
    )
}

#[tokio::test]
#[ignore = "reaches github; run through scripts/live-pr-steering.sh"]
async fn the_direction_on_a_real_pull_request_is_read_through_fiddles_own_reader() {
    let repo = required("FIDDLE_LIVE_REPO");
    let pr: u64 = required("FIDDLE_LIVE_PR")
        .parse()
        .expect("FIDDLE_LIVE_PR is a pull request number");
    let expect_direction = std::env::var("FIDDLE_LIVE_EXPECT_DIRECTION").is_ok();

    let cancel = CancellationToken::new();
    let gh = gh();

    let reviews = read_reviews(&gh, &repo, pr, PAGES, &cancel)
        .await
        .expect("the reviews of the pull request are readable");
    let conversation = read_conversation(&gh, &repo, pr, PAGES, &cancel)
        .await
        .expect("the conversation of the pull request is readable");

    println!(
        "live: {repo}#{pr} answered {} review(s) and {} conversation comment(s)",
        reviews.len(),
        conversation.len()
    );
    for review in &reviews {
        println!(
            "live: review by {} ({}) state={} head={}",
            review.author.login, review.author_association, review.state, review.commit_id
        );
    }
    for comment in &conversation {
        println!(
            "live: comment by {} ({}) bot={}",
            comment.author.login, comment.author_association, comment.is_bot
        );
    }

    let head = match std::env::var("FIDDLE_LIVE_HEAD") {
        Ok(head) if !head.trim().is_empty() => head,
        _ => reviews
            .iter()
            .map(|review| review.commit_id.clone())
            .find(|sha| !sha.is_empty())
            .expect("a review names the head it was written against"),
    };

    let direction = Direction::read_from(reviews, conversation, &head);
    println!(
        "live: {} review(s) steer and {} voice(s) are context",
        direction.asked.len(),
        direction.said.len()
    );
    match direction.rendered() {
        Some(task) => println!(
            "live: the agent would be briefed with {} bytes\n{task}",
            task.len()
        ),
        None => println!("live: nothing on this pull request steers the run"),
    }

    if expect_direction {
        assert!(
            !direction.asked.is_empty(),
            "FIDDLE_LIVE_EXPECT_DIRECTION is set, so this pull request is expected to steer \
             the run, and nothing on it did"
        );
        assert!(
            direction.rendered().is_some(),
            "and the direction renders a task the agent can be briefed with"
        );
    }
}

#[tokio::test]
#[ignore = "reaches github; run through scripts/live-pr-steering.sh"]
async fn the_open_pull_request_for_a_branch_is_found_through_fiddles_own_lookup() {
    let repo = required("FIDDLE_LIVE_REPO");
    let branch = required("FIDDLE_LIVE_BRANCH");
    let base = std::env::var("FIDDLE_LIVE_BASE").unwrap_or_else(|_| "main".to_string());
    let owner = repo
        .split_once('/')
        .map(|(owner, _)| owner.to_string())
        .expect("FIDDLE_LIVE_REPO is owner/name");

    let cancel = CancellationToken::new();
    let found = open_pull_request_on(&gh(), &repo, &owner, &branch, &base, &cancel)
        .await
        .expect("the pull request listing is readable");

    match &found {
        Some(open) => println!(
            "live: {repo} branch {branch} has open pull request #{} at {}",
            open.number, open.head_sha
        ),
        None => println!("live: {repo} branch {branch} has no open pull request into {base}"),
    }

    let expected = required("FIDDLE_LIVE_PR")
        .parse::<u64>()
        .expect("FIDDLE_LIVE_PR is a pull request number");
    let open = found.expect("the branch this lane names carries an open pull request");
    assert_eq!(
        open.number, expected,
        "the lookup found the pull request this lane names, so the steering step reads the \
         one a person reviewed"
    );
    assert!(
        !open.head_sha.is_empty(),
        "and it carries a head, without which no review could be shown current"
    );
}
