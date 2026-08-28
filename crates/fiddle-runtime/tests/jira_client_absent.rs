mod support;

use fiddle_core::{
    DeploymentRule, EffectName, HumanDecisionRequirement, ProposedEffect, FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    install, AdapterError, AuthorizedEffect, DynEffect, EffectContext, EffectDescriptor,
    EffectError, EffectOutcome, EffectPhase, EffectReceipt, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ObservedState, ReadRetry, StepParams,
};
use fiddle_runtime::jira::{JiraError, JiraHttp};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use support::{unreachable_context, Deployment, INVOCATION_REF, PAYLOAD, PROJECT, TARGET};

const REACHED: &str = "jira.client_reached";

const THROUGH_THE_EXECUTOR: &str = "adapter failure for jira.client_reached: \
                                    this deployment holds no `[jira]` configuration, \
                                    so no request was sent";

const INSTALLED: &[EffectDescriptor] = &[EffectDescriptor {
    name: REACHED,
    minimum: HumanDecisionRequirement::Automatic,
    construct: unshipped,
}];

fn unshipped(
    _executor: &Executor<'_>,
    _params: &StepParams,
) -> Result<Box<dyn DynEffect>, EffectError> {
    Err(EffectError::Unbuildable {
        kind: EffectName::shipped(REACHED),
        reason: "this operation is executed directly and never resolved from a name".to_string(),
    })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Asks {
    WhenItInspects,
    WhenItApplies,
}

#[derive(Debug, Default)]
struct Probe {
    reached: AtomicUsize,
    written: AtomicBool,
}

struct ClientReached;

impl ObservedState for ClientReached {
    type Value = String;

    fn describe(&self) -> String {
        "the jira client answered".to_string()
    }

    fn reference(&self) -> Option<String> {
        None
    }

    fn into_value(self) -> String {
        TARGET.to_string()
    }
}

struct ReadTheClient<'p> {
    probe: &'p Probe,
    asks: Asks,
}

#[async_trait::async_trait]
impl IntegrationOperation for ReadTheClient<'_> {
    type State = ClientReached;

    type Error = JiraError;

    fn kind(&self) -> EffectName {
        EffectName::shipped(REACHED)
    }

    fn target(&self) -> String {
        TARGET.to_string()
    }

    fn minimum(&self) -> HumanDecisionRequirement {
        HumanDecisionRequirement::Automatic
    }

    fn payload(&self) -> String {
        PAYLOAD.to_string()
    }

    async fn inspect(&self, ctx: &EffectContext) -> Result<Option<ClientReached>, JiraError> {
        if self.asks == Asks::WhenItInspects {
            let _ = ctx.jira_client()?;
            self.probe.reached.fetch_add(1, Ordering::SeqCst);
        }
        match self.probe.written.load(Ordering::SeqCst) {
            false => Ok(None),
            true => Ok(Some(ClientReached)),
        }
    }

    async fn apply(
        &self,
        ctx: &EffectContext,
        _authorized: &AuthorizedEffect<Self>,
    ) -> Result<(), JiraError> {
        if self.asks == Asks::WhenItApplies {
            let _ = ctx.jira_client()?;
            self.probe.reached.fetch_add(1, Ordering::SeqCst);
        }
        self.probe.written.store(true, Ordering::SeqCst);
        Ok(())
    }
}

struct Silent;

impl EffectTrace for Silent {
    fn step(&self, _kind: &EffectName, _step: ExecutionStep) {}
}

fn registered() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        install(INSTALLED).expect("this operation's effect is installed once for this binary");
    });
}

fn a_client_for_a_site_no_test_reaches() -> JiraHttp {
    JiraHttp::new(
        "http://127.0.0.1:1",
        "bot@example.com",
        "s3cr3t",
        Duration::from_secs(1),
    )
    .expect("a client is built without reaching the site")
}

fn proposed() -> ProposedEffect {
    ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectName::shipped(REACHED),
        target: TARGET.to_string(),
        payload: PAYLOAD.to_string(),
    }
}

async fn run(
    ctx: &EffectContext,
    probe: &Probe,
    asks: Asks,
) -> Result<EffectReceipt<String>, EffectError> {
    registered();
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
    executor
        .execute(proposed(), ReadTheClient { probe, asks })
        .await
}

fn refused_by(error: &EffectError) -> &JiraError {
    error
        .adapter_source::<JiraError>()
        .unwrap_or_else(|| panic!("the client's own refusal must survive the boundary: {error:?}"))
}

#[tokio::test]
async fn an_operation_reading_the_client_during_inspect_refuses_and_names_the_missing_table() {
    let ctx = unreachable_context();
    let probe = Probe::default();

    let error = run(&ctx, &probe, Asks::WhenItInspects)
        .await
        .expect_err("an operation with no client produces no receipt");

    assert_eq!(
        format!("{error}"),
        THROUGH_THE_EXECUTOR,
        "the reader learns why the run stopped from the executor's own words, and never \
         has to reach into the variant to find out"
    );
    assert!(
        matches!(refused_by(&error), JiraError::Unconfigured),
        "the operation refused, and no unwrap of a None ended the run: {error:?}"
    );
    assert_eq!(
        refused_by(&error).outcome(EffectPhase::Inspect),
        EffectOutcome::NotCommitted,
        "a read that never left the process changed nothing"
    );
    assert_eq!(
        probe.reached.load(Ordering::SeqCst),
        0,
        "no client was handed out, so the operation never got past the refusal"
    );
    assert!(
        !probe.written.load(Ordering::SeqCst),
        "and nothing was written"
    );
}

#[tokio::test]
async fn an_operation_reading_the_client_during_apply_refuses_as_a_write_that_committed_nothing() {
    let ctx = unreachable_context();
    let probe = Probe::default();

    let error = run(&ctx, &probe, Asks::WhenItApplies)
        .await
        .expect_err("an operation with no client produces no receipt");

    assert_eq!(
        format!("{error}"),
        THROUGH_THE_EXECUTOR,
        "the write refused in the same words the read did"
    );
    assert!(
        matches!(error, EffectError::Adapter { .. }),
        "a refusal the client classifies NotCommitted is a definite adapter failure, and an \
         Unresolved here would record an ambiguous write for a request never sent: {error:?}"
    );
    assert_eq!(
        refused_by(&error).outcome(EffectPhase::Apply),
        EffectOutcome::NotCommitted,
        "no request left the process, so the write committed nothing"
    );
    assert_eq!(
        probe.reached.load(Ordering::SeqCst),
        0,
        "no client was handed out during the write"
    );
    assert!(
        !probe.written.load(Ordering::SeqCst),
        "and the operation stopped before it recorded one"
    );
}

#[tokio::test]
async fn the_same_operation_given_a_client_reaches_it_in_both_phases_and_refuses_nothing() {
    for (asks, phase, reads) in [
        (Asks::WhenItInspects, "inspect", 2),
        (Asks::WhenItApplies, "apply", 1),
    ] {
        let ctx = unreachable_context().with_jira(a_client_for_a_site_no_test_reaches());
        let probe = Probe::default();

        let receipt = run(&ctx, &probe, asks).await.unwrap_or_else(|error| {
            panic!("an operation reading the client during {phase} was given one: {error}")
        });

        assert_eq!(
            receipt.outcome,
            EffectOutcome::Committed,
            "an operation reading the client during {phase} ran to a receipt"
        );
        assert_eq!(
            probe.reached.load(Ordering::SeqCst),
            reads,
            "an operation reading the client during {phase} was handed one every time it \
             asked, so this test cannot pass for an operation that always fails"
        );
        assert!(
            probe.written.load(Ordering::SeqCst),
            "and the write it guards happened"
        );
    }
}
