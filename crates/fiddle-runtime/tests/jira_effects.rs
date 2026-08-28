mod support;

use fiddle_core::{
    DeploymentRule, EffectName, HumanDecisionRequirement, ProjectedStatus, ProposedEffect,
    WorkState, FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    describe, install, resolve, EffectContext, EffectDescriptor, EffectError, EffectOutcome,
    EffectReceipt, EffectTrace, ExecutionStep, Executor, IntegrationOperation, ReadRetry,
    StepParams,
};
use fiddle_runtime::jira::file_verdict::{FileVerdict, FiledIssue, JIRA_ISSUE_FILED};
use fiddle_runtime::jira::{TransitionIssue, JIRA_ISSUE_TRANSITIONED};
use serde_json::json;
use support::stub_jira::{client_for, StubJira, WriteRoute, SEEDED_PROJECT};
use support::{unreachable_context, Deployment, INVOCATION_REF, PROJECT};

const MARKER: &str = "fx-abc123";
const OTHER_MARKER: &str = "fx-def456";
const CVE: &str = "CVE-2025-1";
const SEVERITY: &str = "high";
const PACKAGE: &str = "acme-parser";
const RATIONALE: &str = "the advisory reaches this build";
const LABEL: &str = "security";

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

#[tokio::test]
async fn the_transitions_the_stub_lists_name_a_destination_status_apart_from_the_transition() {
    let server = StubJira::start().await;
    let key = format!("{SEEDED_PROJECT}-7");
    server
        .holds_issue_in_status(&key, "10000", "To Do", "To Do")
        .await;
    server.offers_transition(&key, "31", "In Review").await;

    let listed = server.get_transitions(&key).await;

    assert_eq!(listed.status, 200);
    let only = &listed.body["transitions"][0];
    assert_eq!(only["id"], "31", "the transition is sent by this id");
    assert_eq!(
        only["name"], "Move to In Review",
        "a transition carries a name of its own, so a resolver that read this one would resolve \
         nothing when it looked for a status"
    );
    assert_eq!(
        only["to"]["name"], "In Review",
        "the destination status is what a caller asks for"
    );
    assert_eq!(
        only["to"]["id"], "931",
        "and the destination status carries an id that is not the transition id, so `resolved to \
         an id` names one of the two and not the other"
    );
}

#[tokio::test]
async fn a_transitions_listing_for_an_issue_the_stub_does_not_hold_is_refused_and_lists_nothing() {
    let server = StubJira::start().await;

    let listed = server
        .get_transitions(&format!("{SEEDED_PROJECT}-404"))
        .await;

    assert_eq!(
        listed.status, 404,
        "a listing that answered for an absent issue would let a resolution succeed against \
         nothing"
    );
}

#[tokio::test]
async fn the_transition_that_is_sent_is_the_id_the_site_offered_and_never_a_name() {
    let server = StubJira::start().await;
    let key = format!("{SEEDED_PROJECT}-7");
    server
        .holds_issue_in_status(&key, "10000", "To Do", "To Do")
        .await;
    server.offers_transition(&key, "31", "In Review").await;
    let read_in = updated(&server, &key).await;

    let receipt = transition_to(&server, &key, &read_in, "In Review")
        .await
        .expect("the offered transition is performed");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.value.jira_status_name, "In Review");
    assert_eq!(
        server.transition_requests().await,
        1,
        "one transition was sent, so no read was mistaken for a write"
    );
    assert_eq!(
        last_transition(&server, &key).await,
        json!({"transition": {"id": "31"}}),
        "the id the listing offered is what reaches the site; a name would have reached it \
         verbatim and the stub refuses a transition it never offered"
    );
    assert!(
        server
            .request_lines()
            .await
            .iter()
            .any(|line| line == &format!("GET /rest/api/3/issue/{key}/transitions HTTP/1.1")),
        "the id was read from the site rather than assumed: {:?}",
        server.request_lines().await
    );
}

#[tokio::test]
async fn a_state_two_transitions_reach_is_refused_rather_than_resolved_to_the_first() {
    let server = StubJira::start().await;
    let key = format!("{SEEDED_PROJECT}-7");
    server
        .holds_issue_in_status(&key, "10000", "To Do", "To Do")
        .await;
    server.offers_transition(&key, "31", "Done").await;
    server.offers_transition(&key, "41", "Done").await;
    let read_in = updated(&server, &key).await;

    let refused = transition_to(&server, &key, &read_in, "Done")
        .await
        .expect_err("two transitions to one status name cannot be told apart by that name");

    let said = format!("{refused}");
    assert!(
        said.contains("31") && said.contains("41"),
        "the refusal must name both transitions it could not choose between: {said}"
    );
    assert!(
        matches!(refused, EffectError::Adapter { .. }),
        "the lookup refused before the write, so nothing was sent and the run ends at a \
         definite adapter failure; an Unresolved here would tell a reader the write may have \
         landed and its answer was lost: {refused}"
    );
    assert_eq!(
        server.transition_requests().await,
        0,
        "a lookup by name returns the first match only, so a build that took it would have \
         written one of two states here"
    );
}

#[tokio::test]
async fn a_state_the_workflow_does_not_offer_is_refused_and_writes_nothing() {
    let server = StubJira::start().await;
    let key = format!("{SEEDED_PROJECT}-7");
    server
        .holds_issue_in_status(&key, "10000", "To Do", "To Do")
        .await;
    server.offers_transition(&key, "31", "Done").await;
    let read_in = updated(&server, &key).await;

    let refused = transition_to(&server, &key, &read_in, "In Review")
        .await
        .expect_err("a workflow that offers no route to a state cannot reach it");

    let said = format!("{refused}");
    assert!(
        said.contains("In Review") && said.contains("31 to `Done`"),
        "the refusal must name the state asked for and what the workflow does offer: {said}"
    );
    assert!(
        matches!(refused, EffectError::Adapter { .. }),
        "a workflow that offers no route sends nothing, so the run ends at a definite adapter \
         failure and never at an ambiguous write: {refused}"
    );
    assert_eq!(server.transition_requests().await, 0);
    assert_eq!(
        server.get_issue(&key).await.body["fields"]["status"]["name"],
        "To Do",
        "the refused transition moved nothing"
    );
}

#[tokio::test]
async fn an_issue_already_in_the_state_is_committed_and_no_transition_is_sent() {
    let server = StubJira::start().await;
    let key = format!("{SEEDED_PROJECT}-7");
    server
        .holds_issue_in_status(&key, "10001", "In Review", "In Progress")
        .await;
    let read_in = updated(&server, &key).await;

    let receipt = transition_to(&server, &key, &read_in, "In Review")
        .await
        .expect("an issue already in the state needs no write");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        server.transition_requests().await,
        0,
        "the postcondition was already held, so nothing was sent and no workflow was consulted"
    );
    assert!(
        !server
            .request_lines()
            .await
            .iter()
            .any(|line| line.contains("/transitions")),
        "and the transitions listing costs nothing on the path that writes nothing: {:?}",
        server.request_lines().await
    );
}

#[tokio::test]
async fn the_receipt_names_the_state_the_issue_was_read_in_and_not_the_one_it_reached() {
    let server = StubJira::start().await;
    let key = format!("{SEEDED_PROJECT}-7");
    server
        .holds_issue_in_status(&key, "10000", "To Do", "To Do")
        .await;
    server.offers_transition(&key, "31", "In Review").await;
    let read_in = updated(&server, &key).await;

    let receipt = transition_to(&server, &key, &read_in, "In Review")
        .await
        .expect("the offered transition is performed");
    let after = updated(&server, &key).await;

    assert_ne!(
        read_in, after,
        "a committed write moves `fields.updated`, so the two revisions differ"
    );
    assert_eq!(
        receipt.target,
        format!("{key}@2026-08-26T09:00:01Z"),
        "the identity names the state a human approved, canonicalised, and never the state the \
         write produced"
    );
    assert!(
        !receipt.target.contains("+0000"),
        "and the colonless offset jira sent never reaches it: {}",
        receipt.target
    );
}

#[tokio::test]
async fn a_typed_state_is_reported_beside_the_status_the_site_named() {
    let server = StubJira::start().await;
    let key = format!("{SEEDED_PROJECT}-7");
    server
        .holds_issue_in_status(&key, "10002", "Awaiting QA", "In Progress")
        .await;
    let read_in = updated(&server, &key).await;

    let receipt = transition_to(&server, &key, &read_in, "Awaiting QA")
        .await
        .expect("an issue already in the state needs no write");

    assert_eq!(
        receipt.value,
        ProjectedStatus {
            state: WorkState::InProgress,
            jira_status_id: "10002".to_string(),
            jira_status_name: "Awaiting QA".to_string(),
            jira_status_category: "In Progress".to_string(),
        },
        "no `[jira.workflow]` table reaches an effect context, so the category is what supplies \
         the typed state and every jira fact must survive beside it"
    );
    assert!(
        receipt.postcondition.contains("Awaiting QA")
            && receipt.postcondition.contains("InProgress"),
        "the line a person reads names the site's word and this build's reading of it: {}",
        receipt.postcondition
    );
}

#[tokio::test]
async fn an_issue_the_site_does_not_hold_is_refused_before_any_workflow_is_read() {
    let server = StubJira::start().await;
    server.holds_nothing().await;
    let missing = format!("{SEEDED_PROJECT}-404");

    let refused = transition_to(
        &server,
        &missing,
        "2026-08-26T09:00:01.000+0000",
        "In Review",
    )
    .await
    .expect_err("an issue that is not there has no state to move");

    assert!(
        format!("{refused}").contains(&missing),
        "the refusal names the issue it could not read: {refused}"
    );
    assert_eq!(server.transition_requests().await, 0);
}

#[tokio::test]
async fn an_issue_whose_updated_field_is_not_a_time_is_refused_rather_than_read_as_a_state() {
    let server = StubJira::start().await;
    let key = format!("{SEEDED_PROJECT}-7");
    server
        .holds_issue_updated_at(&key, "10000", "To Do", "To Do", "yesterday")
        .await;

    let refused = transition_to(&server, &key, "2026-08-26T09:00:01.000+0000", "In Review")
        .await
        .expect_err("a state no identity can name must not be reported as observed");

    assert!(
        format!("{refused}").contains("yesterday"),
        "the refusal quotes the field it could not read, so a reader is not left guessing: \
         {refused}"
    );
    assert_eq!(
        server.transition_requests().await,
        0,
        "and nothing was written on the strength of a state that could not be named"
    );
}

struct Silent;

impl EffectTrace for Silent {
    fn step(&self, _kind: &EffectName, _step: ExecutionStep) {}
}

const JIRA: &[EffectDescriptor] = &[TransitionIssue::descriptor(), FileVerdict::descriptor()];

fn registered() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if describe(&EffectName::shipped(JIRA_ISSUE_TRANSITIONED)).is_none() {
            install(JIRA).expect("one extension holds every jira effect this binary drives");
        }
    });
}

async fn transition_to(
    server: &StubJira,
    key: &str,
    read_in: &str,
    to: &str,
) -> Result<EffectReceipt<ProjectedStatus>, EffectError> {
    registered();
    let operation = TransitionIssue::new(key, read_in, to)
        .expect("the stub sends a `fields.updated` this build can read");
    let ctx: EffectContext = unreachable_context().with_jira(client_for(server));
    let deployment = Deployment(DeploymentRule::Allow);
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectName::shipped(JIRA_ISSUE_TRANSITIONED),
        target: operation.target(),
        payload: IntegrationOperation::payload(&operation),
    };
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        &Silent,
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
}

async fn last_transition(server: &StubJira, key: &str) -> serde_json::Value {
    server
        .writes()
        .await
        .iter()
        .rfind(|write| write.route == WriteRoute::TransitionIssue && write.issue == key)
        .map(|write| write.body.clone())
        .unwrap_or_else(|| panic!("the stub was asked to transition {key}"))
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

fn the_registry_answers_the_name() {
    registered();
    assert!(
        describe(&EffectName::shipped(JIRA_ISSUE_FILED)).is_some(),
        "the executor stops at UnknownEffect for a name no descriptor holds, so every run below \
         would refuse before it reached the operation"
    );
}

fn verdict() -> FileVerdict {
    FileVerdict::new(
        CVE.to_string(),
        SEVERITY.to_string(),
        PACKAGE.to_string(),
        RATIONALE.to_string(),
        LABEL.to_string(),
        SEEDED_PROJECT.to_string(),
        MARKER.to_string(),
    )
}

async fn filed_through_the_executor(
    ctx: &EffectContext,
    operation: FileVerdict,
) -> Result<EffectReceipt<FiledIssue>, EffectError> {
    the_registry_answers_the_name();
    let deployment = Deployment(DeploymentRule::Allow);
    let trace = Silent;
    let executor = Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        ctx,
        &trace,
        ReadRetry::none(),
    );
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectName::shipped(JIRA_ISSUE_FILED),
        target: IntegrationOperation::target(&operation),
        payload: IntegrationOperation::payload(&operation),
    };
    executor.execute(proposed, operation).await
}

async fn file_verdict(server: &StubJira) -> Result<EffectReceipt<FiledIssue>, EffectError> {
    let ctx = unreachable_context().with_jira(client_for(server));
    filed_through_the_executor(&ctx, verdict()).await
}

#[test]
fn the_descriptor_the_derive_wrote_names_this_effect_and_the_judgment_it_needs() {
    assert_eq!(FileVerdict::descriptor().name, JIRA_ISSUE_FILED);
    assert_eq!(
        FileVerdict::descriptor().minimum,
        HumanDecisionRequirement::Automatic
    );
    assert_eq!(
        IntegrationOperation::kind(&verdict()).as_str(),
        JIRA_ISSUE_FILED,
        "the kind the executor compares against comes from the derive"
    );
    assert_eq!(
        IntegrationOperation::minimum(&verdict()),
        HumanDecisionRequirement::Automatic,
        "one derive writes both the descriptor and the operation, so this pins the requirement \
         itself; a hand-written `minimum` that drifted from the descriptor is what it guards"
    );
    assert_eq!(
        IntegrationOperation::target(&verdict()),
        format!("{SEEDED_PROJECT}/{MARKER}"),
        "the identity is the project and the marker, so the same verdict re-derives the same \
         effect id in a later process"
    );
}

#[tokio::test]
async fn a_step_names_no_verdict_so_the_registered_constructor_refuses_rather_than_defaults() {
    let server = StubJira::start().await;
    let ctx = unreachable_context().with_jira(client_for(&server));
    the_registry_answers_the_name();
    let deployment = Deployment(DeploymentRule::Allow);
    let trace = Silent;
    let executor = Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        &trace,
        ReadRetry::none(),
    );

    let construct =
        resolve(&EffectName::shipped(JIRA_ISSUE_FILED)).expect("the derived descriptor is held");
    let error = construct(&executor, &StepParams::for_capability(FIXTURE_REPAIR))
        .err()
        .expect("a step carries no advisory and no rationale");

    assert!(
        matches!(error, EffectError::Unbuildable { .. }),
        "the operation refuses a step it cannot be built from rather than filing a ticket made \
         of defaults: {error:?}"
    );
    assert_eq!(
        server.create_requests().await,
        0,
        "and a refused construction reaches no site"
    );
}

#[tokio::test]
async fn two_marker_matches_refuse_the_write_and_create_nothing() {
    let server = StubJira::start().await;
    server.holds_two_issues_labelled(MARKER).await;

    let error = file_verdict(&server)
        .await
        .expect_err("an ambiguous marker refuses");

    assert!(
        matches!(error, EffectError::DuplicateState { count: 2, .. }),
        "two issues carry the marker, so the run must end at the ambiguity exit and nothing \
         weaker: {error:?}"
    );
    assert!(
        format!("{error}").contains('2'),
        "the refusal must say how many it found: {error}"
    );
    assert_eq!(
        server.create_requests().await,
        0,
        "an ambiguous marker sends no create"
    );
    assert_eq!(
        server.issues_that_exist().await,
        2,
        "and files no third issue beside the two it refused to choose between"
    );
}

#[tokio::test]
async fn a_marker_matching_across_more_than_one_search_page_is_still_ambiguous() {
    let server = StubJira::start().await;
    server.caps_search_pages_at(1).await;
    for key in ["901", "902", "903"] {
        server
            .holds_issue_labelled(&format!("{SEEDED_PROJECT}-{key}"), &[MARKER])
            .await;
    }

    let error = file_verdict(&server)
        .await
        .expect_err("three matches are not one");

    assert!(
        matches!(error, EffectError::DuplicateState { count: 3, .. }),
        "the site serves one issue per page here, so an inspect that counted a single page \
         would read one match, answer a receipt naming it and never refuse; the count has to \
         come from following the page token: {error:?}"
    );
    assert_eq!(server.create_requests().await, 0);
}

#[tokio::test]
async fn the_marker_is_written_in_the_create_and_never_in_a_second_edit() {
    let server = StubJira::start().await;

    let receipt = file_verdict(&server).await.expect("it files");

    let created = server.last_create().await;
    let labels = created["fields"]["labels"]
        .as_array()
        .unwrap_or_else(|| panic!("a create carries a labels array: {created}"));
    assert!(
        labels.iter().any(|label| label == &json!(MARKER)),
        "the marker rides the create, or an interruption between create and edit orphans an \
         unmarked issue: {created}"
    );
    assert_eq!(
        server.edit_requests().await,
        0,
        "and no second write carries it"
    );
    assert_eq!(server.create_requests().await, 1);
    assert_eq!(server.issues_that_exist().await, 1);
    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some(receipt.value.key.as_str()),
        "the receipt names the issue a person opens"
    );
    assert_eq!(
        found(&server, &by_marker(MARKER)).await,
        1,
        "and the marker search finds exactly the issue the receipt names"
    );
}

#[tokio::test]
async fn an_issue_that_already_carries_the_marker_is_answered_by_a_read_and_no_write() {
    let server = StubJira::start().await;
    server
        .holds_issue_labelled(&format!("{SEEDED_PROJECT}-901"), &[MARKER])
        .await;

    let receipt = file_verdict(&server)
        .await
        .expect("the world already satisfies this");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.value.key, format!("{SEEDED_PROJECT}-901"));
    assert!(
        server.writes().await.is_empty(),
        "an effect the world already satisfies sends no write at all: {:?}",
        server.writes().await
    );
}

#[tokio::test]
async fn an_interrupted_create_and_a_fresh_process_after_the_lag_leave_exactly_one_issue() {
    let server = StubJira::start().await;
    server.withholds_new_issues_from_search().await;
    server.loses_the_answer_to_a_committed_write().await;

    let interrupted = file_verdict(&server)
        .await
        .expect_err("a lost answer is not a receipt");

    assert!(
        matches!(interrupted, EffectError::Unresolved { .. }),
        "the client heard no key and the index cannot yet see the issue, so the run ends \
         unresolved rather than refused: {interrupted:?}"
    );
    assert_eq!(
        server.issues_that_exist().await,
        1,
        "the write committed all the same"
    );

    server.answers_every_committed_write().await;
    server.admits_the_withheld_issues_to_search().await;

    let resolved = file_verdict(&server)
        .await
        .expect("a fresh process resolves it by reading");

    assert_eq!(resolved.outcome, EffectOutcome::Committed);
    assert_eq!(
        resolved.value.key,
        format!("{SEEDED_PROJECT}-1"),
        "and the receipt names the issue the interrupted run created"
    );
    assert_eq!(
        server.create_requests().await,
        1,
        "the second run resolved the ambiguity by reading the world, so it sent no second create"
    );
    assert_eq!(
        server.issues_that_exist().await,
        1,
        "exactly one issue exists; this counts the issues the store holds and never the create \
         requests, which a refused create would inflate"
    );
}

#[tokio::test]
async fn a_fresh_process_inside_the_lag_window_files_a_second_issue_and_the_next_read_refuses_both()
{
    let server = StubJira::start().await;
    server.withholds_new_issues_from_search().await;
    server.loses_the_answer_to_a_committed_write().await;

    let interrupted = file_verdict(&server)
        .await
        .expect_err("a lost answer is not a receipt");
    assert!(
        matches!(interrupted, EffectError::Unresolved { .. }),
        "{interrupted:?}"
    );

    server.answers_every_committed_write().await;

    let inside = file_verdict(&server)
        .await
        .expect_err("the index still hides the first issue from its own marker");

    assert!(
        matches!(inside, EffectError::Unresolved { .. }),
        "{inside:?}"
    );
    assert_eq!(
        server.issues_that_exist().await,
        2,
        "a retry arriving inside the indexing lag searches by the marker, sees nothing for an \
         issue that exists and files a second ticket; the exactly-once claim holds across an \
         interruption longer than that lag and not inside it"
    );

    server.admits_the_withheld_issues_to_search().await;

    let refused = file_verdict(&server)
        .await
        .expect_err("two markers are not one");

    assert!(
        matches!(refused, EffectError::DuplicateState { count: 2, .. }),
        "once the index catches up the pair is named for a person rather than added to: \
         {refused:?}"
    );
    assert_eq!(
        server.create_requests().await,
        2,
        "and the run that found the pair sent no third create"
    );
}
