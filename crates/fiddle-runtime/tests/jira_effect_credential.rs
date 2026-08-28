mod support;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use fiddle_core::{
    CapabilityId, DeploymentRule, EffectName, EvidenceRef, FiddleBuild, InvocationRef, Mode,
    ProposedEffect, FIXTURE_REPAIR, STUB_MARK, UNKNOWN_REVISION,
};
use fiddle_runtime::effect::{
    EffectContext, EffectError, EffectReceipt, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ObservedState, ReadRetry,
};
use fiddle_runtime::jira::file_verdict::FileVerdict;
use fiddle_runtime::jira::{AddComment, JiraHttp, LinkPullRequest, TransitionIssue};
use fiddle_runtime::{
    attempt, AttemptContext, Capability, CapabilityError, ExecutionGrant, StubChangePort,
    StubWorkItemPort, BUNDLE_FILE,
};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use support::stub_jira::{StubJira, PATIENT, SEEDED_PROJECT, USER};
use support::{unreachable_context, Deployment, INVOCATION_REF, PROJECT};

const SENTINEL: &str = "jira-write-sentinel-Nc7qv2";

const ECHOED: &str = "the-site-spoke-these-words";

const MARKER: &str = "fx-abc123";

const WORK_ID: &str = "fiddle-m0-demo";
const ATTEMPT_REF: &str = "beans:fiddle-m0-demo";

fn encoded_sentinel() -> String {
    BASE64.encode(format!("{USER}:{SENTINEL}"))
}

fn credentialled(server: &StubJira) -> EffectContext {
    unreachable_context()
        .with_jira(JiraHttp::new(server.base_url(), USER, SENTINEL, PATIENT).expect("a client"))
}

#[derive(Default)]
struct Recorded {
    steps: Mutex<Vec<String>>,
}

impl EffectTrace for Recorded {
    fn step(&self, kind: &EffectName, step: ExecutionStep) {
        self.steps
            .lock()
            .expect("no step panicked while holding the trace")
            .push(format!("{kind} {}", step.as_str()));
    }
}

struct Surfaces {
    seen: Vec<(String, String)>,
}

impl Surfaces {
    fn new() -> Self {
        Surfaces { seen: Vec::new() }
    }

    fn record(&mut self, surface: &str, text: impl Into<String>) {
        self.seen.push((surface.to_string(), text.into()));
    }

    fn record_outcome<T>(
        &mut self,
        surface: &str,
        outcome: &Result<EffectReceipt<T>, EffectError>,
    ) {
        match outcome {
            Ok(receipt) => {
                self.record(
                    &format!("{surface}: the receipt's target"),
                    receipt.target.clone(),
                );
                self.record(
                    &format!("{surface}: the receipt's postcondition"),
                    receipt.postcondition.clone(),
                );
                self.record(
                    &format!("{surface}: the receipt's external reference"),
                    receipt.external_ref.clone().unwrap_or_default(),
                );
            }
            Err(failure) => {
                self.record(
                    &format!("{surface}: the diagnostic a reader is shown"),
                    format!("{failure}"),
                );
                self.record(
                    &format!("{surface}: the diagnostic a log carries"),
                    format!("{failure:?}"),
                );
            }
        }
    }

    fn record_trace(&mut self, surface: &str, trace: &Recorded) {
        let steps = trace.steps.lock().expect("no step panicked").join(", ");
        assert!(
            !steps.is_empty(),
            "{surface} recorded no execution step, so searching its trace would search nothing"
        );
        self.record(&format!("{surface}: the execution trace"), steps);
    }

    fn carry_no_credential(&self) {
        let encoded = encoded_sentinel();
        assert!(
            !self.seen.is_empty(),
            "no surface was collected, so this check would hold without reading anything"
        );
        for (surface, text) in &self.seen {
            assert!(
                !text.contains(SENTINEL),
                "the credential reached {surface}: {text}"
            );
            assert!(
                !text.contains(&encoded),
                "the encoded credential reached {surface}: {text}"
            );
        }
    }

    fn are_shown_to_bite_for_each_of(&self, operations: &[&str]) {
        for operation in operations {
            let carrying = self
                .seen
                .iter()
                .filter(|(surface, text)| surface.starts_with(operation) && text.contains(ECHOED))
                .count();
            assert!(
                carrying > 0,
                "no surface of `{operation}` carries `{ECHOED}`, so this operation contributed \
                 nothing the credential search could have found and one biting operation would \
                 pass for all four: {:?}",
                self.seen
            );
        }
        self.are_shown_to_bite();
    }

    fn are_shown_to_bite(&self) {
        let carrying: Vec<&str> = self
            .seen
            .iter()
            .filter(|(_, text)| text.contains(ECHOED))
            .map(|(surface, _)| surface.as_str())
            .collect();
        assert!(
            !carrying.is_empty(),
            "no collected surface carries `{ECHOED}`, which the site said in the same breath as \
             the credential; a search that finds neither word proves nothing about the one that \
             matters: {:?}",
            self.seen
        );
    }
}

fn spoken_by_the_site() -> String {
    json!({"errorMessages": [format!("{ECHOED}, and {SENTINEL} was refused")]}).to_string()
}

fn verdict() -> FileVerdict {
    FileVerdict::new(
        "CVE-2025-1".to_string(),
        "high".to_string(),
        "acme-parser".to_string(),
        "the advisory reaches this build".to_string(),
        "security".to_string(),
        SEEDED_PROJECT.to_string(),
        MARKER.to_string(),
    )
}

async fn run<O>(
    ctx: &EffectContext,
    trace: &Recorded,
    operation: O,
) -> Result<EffectReceipt<<O::State as ObservedState>::Value>, EffectError>
where
    O: IntegrationOperation,
{
    let deployment = Deployment(DeploymentRule::Allow);
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: operation.kind(),
        target: operation.target(),
        payload: operation.payload(),
    };
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        ctx,
        trace,
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
}

async fn seeded_revision(server: &StubJira, key: &str) -> String {
    server.get_issue(key).await.body["fields"]["updated"]
        .as_str()
        .expect("a seeded issue carries fields.updated")
        .to_string()
}

#[tokio::test]
async fn a_site_that_echoes_the_credential_when_it_refuses_a_write_reaches_no_reader_surface() {
    let server = StubJira::start().await;
    server.refuses_with_body(400, &spoken_by_the_site()).await;
    let ctx = credentialled(&server);
    let mut surfaces = Surfaces::new();

    let trace = Recorded::default();
    surfaces.record_outcome("filing a verdict", &run(&ctx, &trace, verdict()).await);
    surfaces.record_trace("filing a verdict", &trace);

    let trace = Recorded::default();
    let commenting = AddComment::new(
        format!("{SEEDED_PROJECT}-1"),
        "2026-08-26T09:00:00.000+0000",
        "the check is green".to_string(),
        PROJECT,
        INVOCATION_REF,
    )
    .expect("an operation builds from a revision this build can read");
    surfaces.record_outcome("adding a comment", &run(&ctx, &trace, commenting).await);
    surfaces.record_trace("adding a comment", &trace);

    let trace = Recorded::default();
    let linking = LinkPullRequest::new(
        format!("{SEEDED_PROJECT}-1"),
        "2026-08-26T09:00:00.000+0000",
        "peel/fiddle-test".to_string(),
        42,
        PROJECT,
        INVOCATION_REF,
    )
    .expect("an operation builds from a revision this build can read");
    surfaces.record_outcome("linking a pull request", &run(&ctx, &trace, linking).await);
    surfaces.record_trace("linking a pull request", &trace);

    let trace = Recorded::default();
    let transitioning = TransitionIssue::new(
        &format!("{SEEDED_PROJECT}-1"),
        "2026-08-26T09:00:00.000+0000",
        "In Review",
    )
    .expect("an operation builds from a revision this build can read");
    surfaces.record_outcome(
        "transitioning an issue",
        &run(&ctx, &trace, transitioning).await,
    );
    surfaces.record_trace("transitioning an issue", &trace);

    surfaces.are_shown_to_bite_for_each_of(&[
        "filing a verdict",
        "adding a comment",
        "linking a pull request",
        "transitioning an issue",
    ]);
    surfaces.carry_no_credential();
}

#[tokio::test]
async fn a_workflow_name_that_carries_the_credential_reaches_no_diagnostic() {
    let server = StubJira::start().await;
    let key = format!("{SEEDED_PROJECT}-31");
    server
        .holds_issue_in_status(&key, "10001", "To Do", "To Do")
        .await;
    server
        .offers_transition(&key, "31", &format!("{ECHOED}-{SENTINEL}"))
        .await;
    let revision = seeded_revision(&server, &key).await;

    let ctx = credentialled(&server);
    let trace = Recorded::default();
    let operation = TransitionIssue::new(&key, &revision, "In Review")
        .expect("an operation builds from a revision this build can read");
    let outcome = run(&ctx, &trace, operation).await;

    let mut surfaces = Surfaces::new();
    surfaces.record_outcome(
        "transitioning to a state the workflow does not offer",
        &outcome,
    );
    surfaces.are_shown_to_bite();
    surfaces.carry_no_credential();
}

#[tokio::test]
async fn an_issue_key_that_carries_the_credential_reaches_no_receipt() {
    let server = StubJira::start().await;
    server
        .holds_issue_labelled(&format!("{SEEDED_PROJECT}-{ECHOED}-{SENTINEL}"), &[MARKER])
        .await;

    let ctx = credentialled(&server);
    let trace = Recorded::default();
    let outcome = run(&ctx, &trace, verdict()).await;

    assert!(
        outcome.is_ok(),
        "one issue carries the marker, so the exactly-once inspect settles without writing: {:?}",
        outcome.as_ref().err().map(|e| format!("{e}"))
    );
    let mut surfaces = Surfaces::new();
    surfaces.record_outcome(
        "a verdict already filed under a key the site chose",
        &outcome,
    );
    surfaces.are_shown_to_bite();
    surfaces.carry_no_credential();
}

#[tokio::test]
async fn a_revision_field_that_carries_the_credential_reaches_no_diagnostic() {
    let server = StubJira::start().await;
    let key = format!("{SEEDED_PROJECT}-77");
    server
        .holds_issue_updated_at(
            &key,
            "10002",
            "To Do",
            "To Do",
            &format!("{ECHOED}-{SENTINEL}"),
        )
        .await;

    let ctx = credentialled(&server);
    let trace = Recorded::default();
    let operation = TransitionIssue::new(&key, "2026-08-26T09:00:00.000+0000", "In Review")
        .expect("an operation builds from a revision this build can read");
    let outcome = run(&ctx, &trace, operation).await;

    assert!(
        outcome.is_err(),
        "a `fields.updated` no identity can name must refuse rather than be reported as observed"
    );
    let mut surfaces = Surfaces::new();
    surfaces.record_outcome("reading a revision the site spelled unreadably", &outcome);
    surfaces.are_shown_to_bite();
    surfaces.carry_no_credential();
}

#[tokio::test]
async fn a_write_carries_the_credential_in_its_header_and_in_no_payload_it_sends() {
    let server = StubJira::start().await;
    let ctx = credentialled(&server);
    let trace = Recorded::default();

    let _ = run(&ctx, &trace, verdict()).await;

    assert_eq!(
        server.create_requests().await,
        1,
        "the run must really have written, or the payloads searched below are the payloads of \
         nothing"
    );

    let authorization = server.last_authorization().await;
    assert_eq!(
        authorization,
        format!("Basic {}", encoded_sentinel()),
        "the credential really did reach this site on this run, so the absences below are \
         redaction and not an unsent request"
    );

    let mut surfaces = Surfaces::new();
    for (at, write) in server.writes().await.iter().enumerate() {
        surfaces.record(&format!("the body of write {at}"), write.body.to_string());
    }
    for line in server.request_lines().await {
        surfaces.record("a request line the site read", line);
    }
    assert!(
        !surfaces.seen.is_empty(),
        "no request reached the site, so no payload was examined"
    );
    surfaces.carry_no_credential();
}

struct WritesToJira {
    base_url: String,
}

#[async_trait::async_trait]
impl Capability for WritesToJira {
    fn id(&self) -> CapabilityId {
        STUB_MARK
    }

    fn stage(&self) -> &'static str {
        "filed"
    }

    async fn execute(
        &self,
        _grant: ExecutionGrant,
        _work_id: &str,
        _invocation_ref: &str,
    ) -> Result<EvidenceRef, CapabilityError> {
        let ctx = unreachable_context()
            .with_jira(JiraHttp::new(&self.base_url, USER, SENTINEL, PATIENT).expect("a client"));
        let receipt = run(&ctx, &Recorded::default(), verdict()).await?;
        Ok(EvidenceRef(format!("jira:{}", receipt.target)))
    }
}

fn work_item_root(dir: &Path) -> PathBuf {
    let root = dir.join("stub-state");
    std::fs::create_dir_all(root.join("work")).expect("a work directory");
    std::fs::create_dir_all(root.join("changes")).expect("a changes directory");
    std::fs::write(
        root.join(format!("work/{WORK_ID}.json")),
        format!(r#"{{"id":"{WORK_ID}","status":"open"}}"#),
    )
    .expect("an open work item");
    root
}

fn published_bundles(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match path.is_dir() {
                true => stack.push(path),
                false if path.file_name().is_some_and(|name| name == BUNDLE_FILE) => {
                    found.push(path)
                }
                false => {}
            }
        }
    }
    found.sort();
    found
}

#[tokio::test]
async fn a_credential_the_site_echoes_reaches_no_published_report_bundle() {
    let server = StubJira::start().await;
    server.refuses_with_body(400, &spoken_by_the_site()).await;

    let dir = tempfile::tempdir().expect("a temporary project");
    let stub_root = work_item_root(dir.path());
    let reports = dir.path().join("reports");
    let reference: InvocationRef = ATTEMPT_REF.parse().expect("a reference this build reads");
    let work_items = StubWorkItemPort::new(stub_root.clone());
    let changes = StubChangePort::new(stub_root);
    let capability = WritesToJira {
        base_url: server.base_url().to_string(),
    };

    let record = attempt(&AttemptContext {
        project: PROJECT,
        reference: &reference,
        mode: Mode::Unattended,
        build: FiddleBuild::new("0.1.0", UNKNOWN_REVISION),
        report_dir: &reports,
        work_items: &work_items,
        changes: &changes,
        capability: &capability,
        trace: None,
        cancel: &tokio_util::sync::CancellationToken::new(),
    })
    .await;

    assert!(
        record.published.is_some(),
        "a bundle that was never published cannot be searched for a credential"
    );
    let bundles = published_bundles(&reports);
    assert_eq!(
        bundles.len(),
        1,
        "one attempt publishes one bundle, and a count of {} means this test read the wrong \
         files: {bundles:?}",
        bundles.len()
    );

    let mut surfaces = Surfaces::new();
    for path in &bundles {
        surfaces.record(
            &format!("the published bundle at {}", path.display()),
            std::fs::read_to_string(path).expect("a published bundle is readable"),
        );
    }
    surfaces.are_shown_to_bite();
    surfaces.carry_no_credential();
}
