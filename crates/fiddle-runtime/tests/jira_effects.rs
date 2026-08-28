mod support;

use serde_json::json;
use support::stub_jira::{StubJira, WriteRoute, SEEDED_PROJECT};

const MARKER: &str = "fx-abc123";
const OTHER_MARKER: &str = "fx-def456";

#[tokio::test]
async fn the_stub_records_a_created_issue_and_answers_a_search_for_its_marker() {
    let server = StubJira::start().await;

    let created = server
        .post_issue(json!({
            "fields": {
                "project": {"key": "IDENT"},
                "summary": "CVE-2025-1",
                "labels": [MARKER],
            }
        }))
        .await;

    assert_eq!(created.status, 201);
    let found = server.search(&format!("labels = {MARKER}")).await;
    assert_eq!(found["issues"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn a_search_for_a_marker_no_issue_carries_finds_nothing() {
    let server = StubJira::start().await;
    server.post_issue(create_labelled(&[MARKER])).await;

    let found = server.search(&format!("labels = {OTHER_MARKER}")).await;

    assert_eq!(
        found["issues"].as_array().unwrap().len(),
        0,
        "a search for a marker that was never written must find nothing, or the search matches \
         on the fact that anything was created rather than on the query"
    );
}

#[tokio::test]
async fn a_search_for_a_marker_two_issues_carry_finds_both() {
    let server = StubJira::start().await;
    server.holds_two_issues_labelled(MARKER).await;

    let found = server.search(&format!("labels = {MARKER}")).await;

    assert_eq!(
        found["issues"].as_array().unwrap().len(),
        2,
        "the ambiguous marker case needs a stub that can answer two, or the arm that refuses two \
         matches is unreachable"
    );
}

#[tokio::test]
async fn a_search_selects_the_one_issue_that_carries_the_marker_and_not_its_neighbour() {
    let server = StubJira::start().await;
    server.post_issue(create_labelled(&[MARKER])).await;
    server.post_issue(create_labelled(&[OTHER_MARKER])).await;

    let found = server.search(&format!("labels = {MARKER}")).await;
    let issues = found["issues"].as_array().unwrap();

    assert_eq!(
        issues.len(),
        1,
        "two issues exist and one carries the marker, so a search that matches everything reds here"
    );
    assert_eq!(issues[0]["fields"]["labels"][0], MARKER);
}

#[tokio::test]
async fn two_clauses_joined_by_and_must_both_hold() {
    let server = StubJira::start().await;
    server.post_issue(create_labelled(&[MARKER])).await;

    let held = server
        .search(&format!("project = IDENT AND labels = {MARKER}"))
        .await;
    let missed = server
        .search(&format!("project = OTHER AND labels = {MARKER}"))
        .await;

    assert_eq!(held["issues"].as_array().unwrap().len(), 1);
    assert_eq!(
        missed["issues"].as_array().unwrap().len(),
        0,
        "a conjunction that drops a clause would answer one here"
    );
}

#[tokio::test]
async fn a_jql_the_stub_cannot_parse_is_refused_and_never_answered_with_every_issue() {
    let server = StubJira::start().await;
    server.post_issue(create_labelled(&[MARKER])).await;

    for unparsed in [
        "labels ~ fx",
        "labels IN (fx-abc123, fx-def456)",
        "summary = CVE-2025-1",
        "labels = fx-abc123 ORDER BY created",
        "",
    ] {
        let answered = server.search_answer(unparsed).await;
        assert_eq!(
            answered.status, 400,
            "the stub must refuse jql it cannot select on rather than answer a count computed \
             from a query it ignored: `{unparsed}` answered {}",
            answered.body
        );
        assert!(
            answered.body["issues"].is_null(),
            "a refused search carries no issues array, or a caller reads a length from it: {}",
            answered.body
        );
    }
}

#[tokio::test]
async fn the_stub_counts_every_write_by_the_route_it_arrived_on() {
    let server = StubJira::start().await;
    let created = server.post_issue(create_labelled(&[MARKER])).await;
    let key = created.body["key"].as_str().unwrap().to_string();
    server.offers_transition(&key, "31", "Done").await;

    server
        .put_issue(&key, json!({"fields": {"summary": "edited"}}))
        .await;
    server
        .post_comment(&key, json!({"body": "a person reads this"}))
        .await;
    server
        .post_transition(&key, json!({"transition": {"id": "31"}}))
        .await;

    assert_eq!(server.creates().await, 1);
    assert_eq!(server.edits().await, 1);
    assert_eq!(server.comments().await, 1);
    assert_eq!(server.transitions().await, 1);
    assert_eq!(server.comments_on(&key).await, 1);
    assert_eq!(
        server.last_comment_on(&key).await["body"],
        "a person reads this"
    );
    assert_eq!(server.last_create().await["fields"]["labels"][0], MARKER);
    assert_eq!(
        server
            .writes()
            .await
            .iter()
            .map(|write| write.route)
            .collect::<Vec<_>>(),
        vec![
            WriteRoute::CreateIssue,
            WriteRoute::EditIssue,
            WriteRoute::AddComment,
            WriteRoute::TransitionIssue,
        ],
        "the count is per route and in arrival order, so a test can say which write repeated"
    );
}

#[tokio::test]
async fn losing_the_answer_to_a_write_leaves_the_write_committed() {
    let server = StubJira::start().await;
    server.loses_the_answer_to_a_committed_write().await;

    let lost = server
        .attempt(
            "POST",
            "/rest/api/3/issue",
            Some(create_labelled(&[MARKER])),
        )
        .await;

    assert!(
        lost.is_err(),
        "a lost answer reaches the client as a transport failure and never as a status: {lost:?}"
    );
    assert_eq!(
        server.creates().await,
        1,
        "the server committed the write, so this is an ambiguous write and not a failed one"
    );
    let keys = server.issue_keys().await;
    assert_eq!(keys.len(), 1, "exactly one issue exists: {keys:?}");
    let read_back = server.get_issue(&keys[0]).await;
    assert_eq!(
        read_back.status, 200,
        "the issue the client never heard about is readable by key, which is how Unknown is \
         resolved by reading the world"
    );
    assert_eq!(read_back.body["fields"]["labels"][0], MARKER);
}

#[tokio::test]
async fn a_write_whose_answer_was_lost_is_still_found_by_a_search_for_its_marker() {
    let server = StubJira::start().await;
    server.loses_the_answer_to_a_committed_write().await;

    let _ = server
        .attempt(
            "POST",
            "/rest/api/3/issue",
            Some(create_labelled(&[MARKER])),
        )
        .await;

    let found = server.search(&format!("labels = {MARKER}")).await;
    assert_eq!(
        found["issues"].as_array().unwrap().len(),
        1,
        "losing the answer is not delaying the search, so the marker is visible at once"
    );
}

#[tokio::test]
async fn losing_the_answer_stops_when_the_stub_is_told_to_answer_again() {
    let server = StubJira::start().await;
    server.loses_the_answer_to_a_committed_write().await;
    let _ = server
        .attempt(
            "POST",
            "/rest/api/3/issue",
            Some(create_labelled(&[MARKER])),
        )
        .await;

    server.answers_every_committed_write().await;
    let answered = server.post_issue(create_labelled(&[OTHER_MARKER])).await;

    assert_eq!(answered.status, 201);
    assert_eq!(server.creates().await, 2);
}

#[tokio::test]
async fn a_read_is_answered_while_the_stub_is_losing_the_answers_to_writes() {
    let server = StubJira::start().await;
    let created = server.post_issue(create_labelled(&[MARKER])).await;
    let key = created.body["key"].as_str().unwrap().to_string();

    server.loses_the_answer_to_a_committed_write().await;

    assert_eq!(
        server.get_issue(&key).await.status,
        200,
        "the control loses the answer to a write, so a run that resolves Unknown by reading can \
         still read"
    );
    assert_eq!(
        server.search(&format!("labels = {MARKER}")).await["issues"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn an_issue_withheld_from_search_is_answered_by_the_create_and_readable_by_key() {
    let server = StubJira::start().await;
    server.withholds_new_issues_from_search().await;

    let created = server.post_issue(create_labelled(&[MARKER])).await;

    assert_eq!(
        created.status, 201,
        "withholding an issue from search does not lose the answer to the create"
    );
    let key = created.body["key"].as_str().unwrap().to_string();
    assert_eq!(server.get_issue(&key).await.status, 200);
    assert_eq!(
        server.search(&format!("labels = {MARKER}")).await["issues"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "jql indexing lags, so a search does not yet reflect an issue that exists"
    );
}

#[tokio::test]
async fn an_issue_withheld_from_search_appears_once_the_index_admits_it() {
    let server = StubJira::start().await;
    server.withholds_new_issues_from_search().await;
    server.post_issue(create_labelled(&[MARKER])).await;

    server.admits_the_withheld_issues_to_search().await;

    assert_eq!(
        server.search(&format!("labels = {MARKER}")).await["issues"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "the test drives the lag rather than waiting on it"
    );
}

#[tokio::test]
async fn the_two_controls_are_independent_of_each_other() {
    let lost = StubJira::start().await;
    lost.loses_the_answer_to_a_committed_write().await;
    let _ = lost
        .attempt(
            "POST",
            "/rest/api/3/issue",
            Some(create_labelled(&[MARKER])),
        )
        .await;

    let withheld = StubJira::start().await;
    withheld.withholds_new_issues_from_search().await;
    let answered = withheld.post_issue(create_labelled(&[MARKER])).await;

    assert_eq!(lost.creates().await, 1);
    assert_eq!(withheld.creates().await, 1);
    assert_eq!(
        lost.search(&format!("labels = {MARKER}")).await["issues"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "a lost answer is a lost answer only; the search still reflects the write"
    );
    assert_eq!(
        answered.status, 201,
        "a withheld issue still answers its create"
    );
    assert_eq!(
        withheld.search(&format!("labels = {MARKER}")).await["issues"]
            .as_array()
            .unwrap()
            .len(),
        0,
        "a withheld issue is withheld from search only; the create was answered"
    );
}

#[tokio::test]
async fn a_create_that_names_no_project_is_refused_and_stores_nothing() {
    let server = StubJira::start().await;

    let refused = server
        .post_issue(json!({"fields": {"summary": "CVE-2025-1"}}))
        .await;

    assert_eq!(refused.status, 400);
    assert!(refused.body["errorMessages"][0]
        .as_str()
        .unwrap()
        .contains("fields.project.key"));
    assert!(
        server.issue_keys().await.is_empty(),
        "a refused create stores nothing"
    );
}

#[tokio::test]
async fn a_transition_the_stub_was_never_told_about_is_refused_rather_than_silently_accepted() {
    let server = StubJira::start().await;
    let created = server.post_issue(create_labelled(&[MARKER])).await;
    let key = created.body["key"].as_str().unwrap().to_string();

    let refused = server
        .post_transition(&key, json!({"transition": {"id": "31"}}))
        .await;

    assert_eq!(
        refused.status, 400,
        "a stub that accepts a transition it cannot model would let a test read an unchanged \
         status and call the transition done"
    );
    assert_eq!(
        server.get_issue(&key).await.body["fields"]["status"],
        serde_json::Value::Null,
        "the refused transition moved nothing"
    );
}

#[tokio::test]
async fn a_transition_the_stub_offers_moves_the_status_it_leads_to() {
    let server = StubJira::start().await;
    let created = server.post_issue(create_labelled(&[MARKER])).await;
    let key = created.body["key"].as_str().unwrap().to_string();
    server.offers_transition(&key, "31", "Done").await;

    let answered = server
        .post_transition(&key, json!({"transition": {"id": "31"}}))
        .await;

    assert_eq!(answered.status, 204);
    let read_back = server.get_issue(&key).await.body;
    assert_eq!(read_back["fields"]["status"]["name"], "Done");
    assert_eq!(
        read_back["fields"]["status"]["statusCategory"]["key"],
        "done"
    );
}

#[tokio::test]
async fn a_write_to_an_issue_the_stub_does_not_hold_commits_nothing() {
    let server = StubJira::start().await;

    let missing = format!("{SEEDED_PROJECT}-404");
    assert_eq!(
        server
            .post_comment(&missing, json!({"body": "into the void"}))
            .await
            .status,
        404
    );
    assert_eq!(
        server
            .post_transition(&missing, json!({"transition": {"id": "31"}}))
            .await
            .status,
        404
    );
    let put = server
        .put_issue(&missing, json!({"fields": {"summary": "into the void"}}))
        .await;
    assert_eq!(
        put.status, 200,
        "a put to an issue the store does not hold falls through to the scripted read answer \
         m5a's tests were written against, so its status says nothing about whether it \
         landed; `edits()` and `committed` are what say that"
    );

    assert!(
        server.writes().await.iter().all(|write| !write.committed),
        "a write against an issue that does not exist commits nothing"
    );
    assert_eq!(server.edits().await, 1, "the attempt was still recorded");
    assert!(server.issue_keys().await.is_empty());
}

#[tokio::test]
async fn every_committed_write_moves_the_updated_field_the_identity_is_built_from() {
    let server = StubJira::start().await;
    let created = server.post_issue(create_labelled(&[MARKER])).await;
    let key = created.body["key"].as_str().unwrap().to_string();
    let first = updated(&server, &key).await;

    server
        .post_comment(&key, json!({"body": "a person reads this"}))
        .await;
    let second = updated(&server, &key).await;

    assert_ne!(
        first, second,
        "a write that left `updated` alone would let a stale identity keep verifying"
    );
    assert!(
        second.ends_with("+0000") && !second.contains("+00:00"),
        "the stub sends the colonless offset jira cloud sends: {second}"
    );
}

#[tokio::test]
async fn an_edit_merges_into_the_stored_issue_rather_than_replacing_it() {
    let server = StubJira::start().await;
    let created = server.post_issue(create_labelled(&[MARKER])).await;
    let key = created.body["key"].as_str().unwrap().to_string();

    let answered = server
        .put_issue(&key, json!({"fields": {"summary": "edited"}}))
        .await;

    assert_eq!(answered.status, 204);
    let read_back = server.get_issue(&key).await.body;
    assert_eq!(read_back["fields"]["summary"], "edited");
    assert_eq!(
        read_back["fields"]["labels"][0], MARKER,
        "an edit that dropped the untouched fields would hide a marker the create wrote"
    );
}

async fn updated(server: &StubJira, key: &str) -> String {
    server.get_issue(key).await.body["fields"]["updated"]
        .as_str()
        .expect("a stored issue carries updated")
        .to_string()
}

fn create_labelled(labels: &[&str]) -> serde_json::Value {
    json!({
        "fields": {
            "project": {"key": SEEDED_PROJECT},
            "summary": "CVE-2025-1",
            "labels": labels,
        }
    })
}
