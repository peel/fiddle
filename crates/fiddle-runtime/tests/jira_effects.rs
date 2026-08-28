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
    assert_eq!(found(&server, &by_marker(MARKER)).await, 1);
}

#[tokio::test]
async fn a_search_for_a_marker_no_issue_carries_finds_nothing() {
    let server = StubJira::start().await;
    server.post_issue(create_labelled(&[MARKER])).await;

    assert_eq!(
        found(&server, &by_marker(OTHER_MARKER)).await,
        0,
        "a search for a marker that was never written must find nothing, or the search matches \
         on the fact that anything was created rather than on the query"
    );
}

#[tokio::test]
async fn a_search_for_a_marker_two_issues_carry_finds_both() {
    let server = StubJira::start().await;
    server.holds_two_issues_labelled(MARKER).await;

    assert_eq!(
        found(&server, &by_marker(MARKER)).await,
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

    let issues = server.all_search_matches(&by_marker(MARKER)).await;

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

    let held = found(&server, &format!("project = IDENT AND labels = {MARKER}")).await;
    let missed = found(&server, &format!("project = OTHER AND labels = {MARKER}")).await;

    assert_eq!(held, 1);
    assert_eq!(
        missed, 0,
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
async fn a_count_taken_from_one_search_page_is_a_floor_and_never_a_total() {
    let server = StubJira::start().await;
    server.caps_search_pages_at(1).await;
    for key in ["IDENT-11", "IDENT-12", "IDENT-13"] {
        server.holds_issue_labelled(key, &[MARKER]).await;
    }

    let jql = by_marker(MARKER);
    let first_page = server.search_page(&jql).await;
    let by_one_page = first_page["issues"].as_array().unwrap().len();
    let by_following_pages = server.all_search_matches(&jql).await.len();

    assert_eq!(
        by_one_page, 1,
        "the stub serves a bounded page, as the jql search endpoint does"
    );
    assert_eq!(
        by_following_pages, 3,
        "following the page token reaches every match: {first_page}"
    );
    assert_ne!(
        by_one_page, by_following_pages,
        "a reader that counts one page reports a floor as a total; an exactly-once inspect that \
         did so would answer Some(one) where three issues carry the marker, and file a duplicate \
         instead of refusing an ambiguous marker"
    );
    assert_eq!(
        first_page["isLast"], false,
        "a page that is not the last says so: {first_page}"
    );
    assert!(
        first_page["nextPageToken"].is_string(),
        "there is more, and the answer says how to reach it: {first_page}"
    );
}

#[tokio::test]
async fn the_last_search_page_reports_that_it_is_last_and_offers_no_further_token() {
    let server = StubJira::start().await;
    server.caps_search_pages_at(2).await;
    for key in ["IDENT-11", "IDENT-12", "IDENT-13"] {
        server.holds_issue_labelled(key, &[MARKER]).await;
    }

    let jql = by_marker(MARKER);
    let first = server.search_page(&jql).await;
    let token = first["nextPageToken"].as_str().expect("more pages remain");
    let last = server.search_page_after(&jql, token).await;

    assert_eq!(first["issues"].as_array().unwrap().len(), 2);
    assert_eq!(last["issues"].as_array().unwrap().len(), 1);
    assert_eq!(last["isLast"], true);
    assert!(
        last["nextPageToken"].is_null(),
        "the absence of a token is what ends the walk, so the last page must carry none: {last}"
    );
}

#[tokio::test]
async fn a_search_answer_carries_no_total_so_a_count_must_come_from_following_the_pages() {
    let server = StubJira::start().await;
    server.caps_search_pages_at(1).await;
    server.holds_two_issues_labelled(MARKER).await;

    let page = server.search_page(&by_marker(MARKER)).await;

    assert!(
        page["total"].is_null() && page["startAt"].is_null(),
        "this endpoint reports no total and no offset, so a caller that wants a count has to \
         follow the pages rather than read one number off one answer: {page}"
    );
}

#[tokio::test]
async fn a_page_token_the_stub_never_issued_is_refused_rather_than_read_as_an_offset() {
    let server = StubJira::start().await;
    server.caps_search_pages_at(1).await;
    server.holds_two_issues_labelled(MARKER).await;

    let refused = server
        .search_page_answer_after(&by_marker(MARKER), "0")
        .await;

    assert_eq!(
        refused.status, 400,
        "a page token is opaque and comes from the server; a stub that read it as an offset \
         would let a caller page by arithmetic and disagree with the real site: {}",
        refused.body
    );
    assert!(refused.body["issues"].is_null());
}

#[tokio::test]
async fn a_page_token_issued_for_one_query_is_refused_for_another() {
    let server = StubJira::start().await;
    server.caps_search_pages_at(1).await;
    server.holds_two_issues_labelled(MARKER).await;
    server.holds_two_issues_labelled(OTHER_MARKER).await;

    let token = server.search_page(&by_marker(MARKER)).await["nextPageToken"]
        .as_str()
        .expect("more pages remain")
        .to_string();
    let refused = server
        .search_page_answer_after(&by_marker(OTHER_MARKER), &token)
        .await;

    assert_eq!(
        refused.status, 400,
        "a token names a position in one query's result, so answering a different query from it \
         would hand back matches the caller never asked for: {}",
        refused.body
    );
}

#[tokio::test]
async fn a_page_larger_than_the_cap_is_still_capped_and_still_says_there_is_more() {
    let server = StubJira::start().await;
    server.caps_search_pages_at(1).await;
    server.holds_two_issues_labelled(MARKER).await;

    let answered = server
        .search_answer_with(&[("jql", &by_marker(MARKER)), ("maxResults", "500")])
        .await;

    assert_eq!(answered.status, 200);
    assert_eq!(
        answered.body["issues"].as_array().unwrap().len(),
        1,
        "the site caps the page whatever the caller asks for, so asking for everything is not a \
         way to avoid paging: {}",
        answered.body
    );
    assert_eq!(answered.body["isLast"], false);
}

#[tokio::test]
async fn a_start_at_offset_is_refused_because_this_endpoint_pages_by_token() {
    let server = StubJira::start().await;
    server.holds_two_issues_labelled(MARKER).await;

    let refused = server
        .search_answer_with(&[("jql", &by_marker(MARKER)), ("startAt", "0")])
        .await;

    assert_eq!(
        refused.status, 400,
        "the withdrawn search endpoint paged by offset and this one does not; accepting startAt \
         and ignoring it would answer page one to a caller that asked for page two: {}",
        refused.body
    );
}

#[tokio::test]
async fn a_page_size_the_stub_cannot_read_is_refused_rather_than_defaulted() {
    let server = StubJira::start().await;
    server.holds_two_issues_labelled(MARKER).await;

    for unreadable in ["", "0", "many", "-1"] {
        let refused = server
            .search_answer_with(&[("jql", &by_marker(MARKER)), ("maxResults", unreadable)])
            .await;
        assert_eq!(
            refused.status, 400,
            "a page size the stub silently replaced with its default would answer a page the \
             caller never asked for: `{unreadable}` answered {}",
            refused.body
        );
    }
}

#[tokio::test]
async fn a_retry_that_searches_inside_the_indexing_lag_window_finds_nothing_while_the_issue_exists()
{
    let server = StubJira::start().await;
    server.withholds_new_issues_from_search().await;

    let created = server.post_issue(create_labelled(&[MARKER])).await;
    assert_eq!(
        created.status, 201,
        "the create was answered; only the index lags"
    );
    let key = created.body["key"].as_str().unwrap().to_string();
    let jql = by_marker(MARKER);

    assert_eq!(
        found(&server, &jql).await,
        0,
        "this is a retry's inspect inside the lag window: it searches by the marker and sees \
         nothing while the issue exists, which is the window in which a second create is filed"
    );
    assert_eq!(server.issues_that_exist().await, 1);
    assert_eq!(
        server.get_issue(&key).await.status,
        200,
        "the issue the search cannot see still reads back by key"
    );

    server.admits_the_withheld_issues_to_search().await;

    assert_eq!(
        found(&server, &jql).await,
        1,
        "the window closes when the index catches up, and the same search then answers one"
    );
    assert_eq!(
        server.create_requests().await,
        1,
        "the stub was asked to create once, so the second answer came from the index and not \
         from a repeated write"
    );
}

#[tokio::test]
async fn a_lost_answer_inside_the_lag_window_hides_the_write_from_the_client_and_from_the_search() {
    let server = StubJira::start().await;
    server.withholds_new_issues_from_search().await;
    server.loses_the_answer_to_a_committed_write().await;

    let lost = server
        .attempt(
            "POST",
            "/rest/api/3/issue",
            Some(create_labelled(&[MARKER])),
        )
        .await;

    assert!(lost.is_err(), "the client learns no key: {lost:?}");
    assert_eq!(
        server.issues_that_exist().await,
        1,
        "the write committed all the same"
    );
    let jql = by_marker(MARKER);
    assert_eq!(
        found(&server, &jql).await,
        0,
        "both controls hold at once, so a retry has no key to read and no marker to find; this \
         is the sequence the exactly-once claim is bounded by, and it is why the claim holds \
         only across an interruption longer than the indexing lag"
    );

    server.admits_the_withheld_issues_to_search().await;

    assert_eq!(
        found(&server, &jql).await,
        1,
        "outside the window the marker resolves the ambiguous write by reading the world"
    );
    assert_eq!(server.create_requests().await, 1);
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

    assert_eq!(server.create_requests().await, 1);
    assert_eq!(server.edit_requests().await, 1);
    assert_eq!(server.comment_requests().await, 1);
    assert_eq!(server.transition_requests().await, 1);
    assert_eq!(server.comment_requests_on(&key).await, 1);
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
async fn a_request_count_and_an_issue_count_are_different_numbers() {
    let server = StubJira::start().await;

    server.post_issue(create_labelled(&[MARKER])).await;
    let refused = server
        .post_issue(json!({"fields": {"summary": "no project"}}))
        .await;

    assert_eq!(refused.status, 400);
    assert_eq!(
        server.create_requests().await,
        2,
        "two creates were sent, and a refused one is still a create the stub was asked for"
    );
    assert_eq!(
        server.issues_that_exist().await,
        1,
        "one issue exists; a test that means `exactly one issue exists` must ask this and never \
         the request count, or a refused create reads as a duplicate"
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
        server.create_requests().await,
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

    assert_eq!(
        found(&server, &by_marker(MARKER)).await,
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
    assert_eq!(server.create_requests().await, 2);
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
    assert_eq!(found(&server, &by_marker(MARKER)).await, 1);
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
        found(&server, &by_marker(MARKER)).await,
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
        found(&server, &by_marker(MARKER)).await,
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

    assert_eq!(lost.create_requests().await, 1);
    assert_eq!(withheld.create_requests().await, 1);
    assert_eq!(
        found(&lost, &by_marker(MARKER)).await,
        1,
        "a lost answer is a lost answer only; the search still reflects the write"
    );
    assert_eq!(
        answered.status, 201,
        "a withheld issue still answers its create"
    );
    assert_eq!(
        found(&withheld, &by_marker(MARKER)).await,
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
         m5a's tests were written against, so its status says nothing about whether it landed; \
         `edit_requests()` and `committed` are what say that"
    );

    assert!(
        server.writes().await.iter().all(|write| !write.committed),
        "a write against an issue that does not exist commits nothing"
    );
    assert_eq!(
        server.edit_requests().await,
        1,
        "the attempt was still recorded"
    );
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

async fn found(server: &StubJira, jql: &str) -> usize {
    server.all_search_matches(jql).await.len()
}

fn by_marker(marker: &str) -> String {
    format!("labels = {marker}")
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
