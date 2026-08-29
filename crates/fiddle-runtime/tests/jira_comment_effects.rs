mod support;

use fiddle_core::{
    DeploymentRule, EffectName, Observation, ProposedEffect, FIXTURE_REPAIR, JIRA_COMMENT_ADDED,
    JIRA_PULL_REQUEST_LINKED,
};
use fiddle_runtime::effect::{
    describe, EffectError, EffectOutcome, EffectReceipt, EffectTrace, ExecutionStep, Executor,
    FromStepParams, IntegrationOperation, ObservedState, ReadRetry, StepParams,
};
use fiddle_runtime::jira::comment::{AddComment, MarkedComment};
use fiddle_runtime::jira::link::LinkPullRequest;
use fiddle_runtime::jira::{ConfiguredNames, JiraWorkItemPort};
use fiddle_runtime::ports::WorkItemPort;
use support::stub_jira::{client_for, StubJira};
use support::{unreachable_context, Deployment, INVOCATION_REF, PROJECT};
use tokio_util::sync::CancellationToken;

const ISSUE: &str = "IDENT-1";

const AT_SEVEN: &str = "2026-08-26T07:00:00.000+0000";

const AT_EIGHT: &str = "2026-08-26T08:00:00.000+0000";

const REPO: &str = "peel/fiddle-test";

const PULL: u64 = 42;

struct Silent;

impl EffectTrace for Silent {
    fn step(&self, _kind: &EffectName, _step: ExecutionStep) {}
}

fn registered() {
    for name in [JIRA_COMMENT_ADDED, JIRA_PULL_REQUEST_LINKED] {
        assert!(
            describe(&EffectName::shipped(name)).is_some(),
            "`walk` refuses an unregistered name before its first traced step, so every run \
             below would stop at UnknownEffect; {name} is a built-in of this build and this \
             binary installs nothing"
        );
    }
}

async fn holding_the_issue() -> StubJira {
    let server = StubJira::start().await;
    server.holds_issue_labelled(ISSUE, &[]).await;
    server
}

async fn ran<O>(
    server: &StubJira,
    operation: O,
    project: &str,
) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, EffectError>
where
    O: IntegrationOperation,
{
    registered();
    let ctx = unreachable_context().with_jira(client_for(server));
    let deployment = Deployment(DeploymentRule::Allow);
    let trace = Silent;
    let executor = Executor::new(
        FIXTURE_REPAIR,
        project.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        &trace,
        ReadRetry::none(),
    );
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: operation.kind(),
        target: operation.target(),
        payload: operation.payload(),
    };
    executor.execute(proposed, operation).await
}

fn a_comment(updated: &str, text: &str) -> AddComment {
    AddComment::new(
        ISSUE.to_string(),
        updated,
        text.to_string(),
        PROJECT,
        INVOCATION_REF,
    )
    .expect("the stamp is a `fields.updated` the effect can read")
}

fn a_link(updated: &str) -> LinkPullRequest {
    LinkPullRequest::new(
        ISSUE.to_string(),
        updated,
        REPO.to_string(),
        PULL,
        PROJECT,
        INVOCATION_REF,
    )
    .expect("the stamp is a `fields.updated` the effect can read")
}

async fn add_comment(
    server: &StubJira,
    updated: &str,
    text: &str,
) -> Result<EffectReceipt<MarkedComment>, EffectError> {
    ran(server, a_comment(updated, text), PROJECT).await
}

async fn link_pull_request(
    server: &StubJira,
    updated: &str,
) -> Result<EffectReceipt<MarkedComment>, EffectError> {
    ran(server, a_link(updated), PROJECT).await
}

async fn last_comment_text(server: &StubJira) -> String {
    server.last_comment_on(ISSUE).await.to_string()
}

#[tokio::test]
async fn a_comment_is_posted_once_and_recognised_again_from_a_fresh_process() {
    let server = holding_the_issue().await;

    let first = add_comment(&server, AT_SEVEN, "the fixture is repaired")
        .await
        .expect("it posts");
    let second = add_comment(&server, AT_SEVEN, "the fixture is repaired")
        .await
        .expect("a second run recognises it");

    assert_eq!(
        server.comment_requests_on(ISSUE).await,
        1,
        "the second run read the issue's comments, found its own marker and wrote nothing"
    );
    assert_eq!(first.outcome, EffectOutcome::Committed);
    assert_eq!(second.outcome, EffectOutcome::Committed);
    assert_eq!(
        first.value, second.value,
        "both runs name the one comment that exists"
    );
    assert_eq!(
        first.effect_id, second.effect_id,
        "one identity, so one marker was looked for both times"
    );
}

#[tokio::test]
async fn a_pull_request_link_names_the_run_that_made_it() {
    let server = holding_the_issue().await;

    link_pull_request(&server, AT_SEVEN)
        .await
        .expect("it links");

    let comment = last_comment_text(&server).await;
    assert!(
        comment.contains(REPO),
        "the link names the repository: {comment}"
    );
    assert!(
        comment.contains(&PULL.to_string()),
        "and the pull request: {comment}"
    );
    assert!(
        comment.contains(&a_link(AT_SEVEN).marker()),
        "and it carries the marker the next run looks for: {comment}"
    );
}

#[tokio::test]
async fn the_revision_a_run_observes_is_the_revision_the_link_builds_its_identity_from() {
    let server = StubJira::start().await;
    server
        .holds_issue_in_status(ISSUE, "10001", "In Review", "In Progress")
        .await;
    let raw = server.get_issue(ISSUE).await.body["fields"]["updated"]
        .as_str()
        .expect("the stub answers a `fields.updated`")
        .to_string();

    let observed = JiraWorkItemPort::new(
        client_for(&server),
        ConfiguredNames::new(None, None, None, None, None),
        server.site(),
    )
    .observe(ISSUE, &CancellationToken::new())
    .await;
    let Observation::Available { revision, .. } = observed else {
        panic!("the stub holds the issue the port was asked for");
    };
    let revision = revision.expect("the port reads the `fields.updated` it was answered");
    assert_ne!(
        revision, raw,
        "the port canonicalises, so the two runs below are given two spellings and not one \
         string compared with itself"
    );

    link_pull_request(&server, &raw).await.expect("it links");
    link_pull_request(&server, &revision)
        .await
        .expect("a run given the observed revision links again");

    assert_eq!(
        server.comment_requests_on(ISSUE).await,
        1,
        "the revision a work item observation carries is the input this effect derives its \
         identity from, so the second run found its own marker and sent no write"
    );

    link_pull_request(&server, AT_EIGHT)
        .await
        .expect("a revision naming another state links again");

    assert_eq!(
        server.comment_requests_on(ISSUE).await,
        2,
        "a revision the run did not observe builds a second identity and writes a second time, \
         so the count above is not one for every input this test could have given"
    );
}

#[tokio::test]
async fn a_pull_request_link_is_posted_once_and_recognised_again() {
    let server = holding_the_issue().await;

    link_pull_request(&server, AT_SEVEN)
        .await
        .expect("it links");
    link_pull_request(&server, AT_SEVEN)
        .await
        .expect("a second run recognises it");

    assert_eq!(
        server.comment_requests_on(ISSUE).await,
        1,
        "the link is a comment, and the second run recognised its own marker"
    );
}

#[tokio::test]
async fn a_comment_and_a_link_on_one_issue_do_not_recognise_each_other() {
    let server = holding_the_issue().await;

    add_comment(&server, AT_SEVEN, "the fixture is repaired")
        .await
        .expect("the comment posts");
    link_pull_request(&server, AT_SEVEN)
        .await
        .expect("the link posts beside it");

    assert_eq!(
        server.comment_requests_on(ISSUE).await,
        2,
        "two effects share a target and differ in kind, so they carry two markers; one marker \
         for both would leave the second effect believing the first had already run"
    );
    assert_ne!(
        a_comment(AT_SEVEN, "the fixture is repaired").marker(),
        a_link(AT_SEVEN).marker(),
        "the kind is an input to the identity, so the markers differ"
    );
}

#[tokio::test]
async fn a_run_that_re_reads_the_moved_updated_field_builds_a_second_identity_and_writes_again() {
    let server = holding_the_issue().await;

    add_comment(&server, AT_SEVEN, "the fixture is repaired")
        .await
        .expect("the first run posts");
    let moved = add_comment(&server, AT_EIGHT, "the fixture is repaired")
        .await
        .expect("a run holding a later `fields.updated` posts under its own identity");

    assert_eq!(
        server.comment_requests_on(ISSUE).await,
        2,
        "`fields.updated` is inside the target, so a write moves the identity; a retry that \
         re-reads the issue rather than carrying the identity it started with writes again, \
         and this is the bound on the exactly-once claim"
    );
    assert_ne!(
        moved.target,
        a_comment(AT_SEVEN, "the fixture is repaired").target(),
        "the two runs name two targets, and so two identities"
    );
}

#[tokio::test]
async fn the_identity_is_built_from_the_canonical_updated_and_never_from_the_raw_field() {
    let colonless = a_comment(AT_SEVEN, "x");
    let rfc_3339 = a_comment("2026-08-26T07:00:00Z", "x");
    let offset = a_comment("2026-08-26T09:00:00.000+0200", "x");

    assert_eq!(
        colonless.target(),
        "IDENT-1@2026-08-26T07:00:00Z",
        "jira cloud sends a colonless offset, and the target carries the canonical form"
    );
    assert_eq!(
        colonless.target(),
        rfc_3339.target(),
        "one instant spelled two ways is one identity"
    );
    assert_eq!(
        colonless.target(),
        offset.target(),
        "and an instant sent at another offset is the same instant"
    );
    assert_eq!(
        colonless.marker(),
        offset.marker(),
        "so both spellings look for one marker rather than writing two comments"
    );
}

#[tokio::test]
async fn an_updated_field_the_effect_cannot_read_refuses_rather_than_naming_an_identity() {
    let Err(refused) = AddComment::new(
        ISSUE.to_string(),
        "yesterday",
        "x".to_string(),
        PROJECT,
        INVOCATION_REF,
    ) else {
        panic!("a time the effect cannot read must build no identity");
    };

    assert!(
        format!("{refused}").contains("yesterday"),
        "the refusal quotes what it could not read: {refused}"
    );
}

#[tokio::test]
async fn an_operation_whose_identity_the_executor_does_not_share_posts_nothing() {
    let server = holding_the_issue().await;

    let error = ran(
        &server,
        a_comment(AT_SEVEN, "the fixture is repaired"),
        "other/project",
    )
    .await
    .expect_err("an operation looking up a marker the executor would not authorize refuses");

    assert_eq!(
        server.comment_requests_on(ISSUE).await,
        0,
        "nothing was posted, so no comment carries a marker no later run will look for"
    );
    assert!(
        format!("{error}").contains(&a_comment(AT_SEVEN, "the fixture is repaired").marker()),
        "the refusal names the marker the operation would have looked up: {error}"
    );
    assert!(
        matches!(error, EffectError::Adapter { .. }),
        "the identities were compared before the post, so nothing left the process and the run \
         ends at a definite adapter failure; an Unresolved here would record an ambiguous write \
         for a comment never sent: {error}"
    );
}

#[tokio::test]
async fn a_comment_on_an_issue_the_site_does_not_hold_refuses_and_names_both_causes() {
    let server = StubJira::start().await;
    server.holds_nothing().await;

    let error = add_comment(&server, AT_SEVEN, "the fixture is repaired")
        .await
        .expect_err("an issue the site does not answer produces no receipt");

    assert!(
        format!("{error}").contains("/rest/api/3/myself"),
        "a 404 on an issue read is an absence or a refused credential, and this effect says it \
         did not settle which: {error}"
    );
    assert_eq!(server.comment_requests_on(ISSUE).await, 0);
}

#[test]
fn neither_effect_is_buildable_from_a_step_alone() {
    let ctx = unreachable_context();
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
    let mut params = StepParams::for_capability(FIXTURE_REPAIR);
    params.repo = Some(REPO.to_string());
    params.body = Some("the fixture is repaired".to_string());
    params.pull_request = Some(PULL);

    for refused in [
        AddComment::from_params(&executor, &params).err(),
        LinkPullRequest::from_params(&executor, &params).err(),
    ] {
        let refused = refused.expect("a step alone carries no issue key and no `fields.updated`");
        assert!(
            format!("{refused}").contains("fields.updated"),
            "the refusal names the fact only a read of the issue supplies, so a caller cannot \
             read it as a missing optional parameter: {refused}"
        );
    }
}

#[tokio::test]
async fn a_comment_the_stub_took_reads_back_from_the_issue_it_was_posted_to_and_from_no_other() {
    let server = holding_the_issue().await;
    let neighbour = "IDENT-2";
    server.holds_issue_labelled(neighbour, &[]).await;

    server
        .post_comment(ISSUE, serde_json::json!({"body": "a person reads this"}))
        .await;

    let held = server.get_issue(ISSUE).await.body;
    let other = server.get_issue(neighbour).await.body;

    assert_eq!(held["fields"]["comment"]["total"], 1);
    assert_eq!(
        held["fields"]["comment"]["comments"][0]["body"], "a person reads this",
        "an effect that recognises its own comment has to be able to read one back: {held}"
    );
    assert_eq!(
        other["fields"]["comment"]["total"], 0,
        "and a comment belongs to the issue it was posted to; a stub that showed it on every \
         issue would let one marker answer for every issue: {other}"
    );
    assert!(
        other["fields"]["comment"]["comments"].is_array(),
        "an issue with no comment still answers a comment container, as the site does, so a \
         first inspect reads an empty list rather than refusing: {other}"
    );
}
