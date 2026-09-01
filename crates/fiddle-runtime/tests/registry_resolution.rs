mod support;

use fiddle_core::{
    CapabilityId, DecisionBinding, DecisionRequestId, DeploymentRule, EffectId, EffectName,
    HumanDecisionRequest, PayloadHash, ENSURE_PULL_REQUEST, ENSURE_PULL_REQUEST_READY,
    FIXTURE_REPAIR, JIRA_COMMENT_ADDED, JIRA_ISSUE_FILED, JIRA_ISSUE_TRANSITIONED,
    JIRA_PULL_REQUEST_LINKED, PUBLISH_CHANGE, STUB_MARK,
};
use fiddle_runtime::effect::{
    registry, EffectContext, EffectError, EffectOutcome, EffectTrace, ExecutionStep, Executor,
    ReadRetry, StepParams, BUILT_IN,
};
use fiddle_runtime::GhCli;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const REPO: &str = "peel/r";

const OWNER: &str = "peel";

const BASE: &str = "main";

const BRANCH: &str = "fiddle/abc";

const HEAD_SHA: &str = "deadbeef";

const TITLE: &str = "fiddle: repair the fixture";

const BODY: &str = "opened by fiddle";

const PR: u64 = 7;

const NODE_ID: &str = "PR_kwDOabcdef";

const CHECK_WORKFLOW: &str = "ci.yml";

const PATIENT: Duration = Duration::from_secs(60);

struct Forge {
    dir: TempDir,
    steps: Mutex<Vec<&'static str>>,
}

impl EffectTrace for Forge {
    fn step(&self, _kind: &EffectName, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl Forge {
    fn empty() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        Self {
            dir,
            steps: Mutex::new(Vec::new()),
        }
    }

    fn pull(&self, number: u64, body: serde_json::Value) {
        let dir = self.dir.path().join("pulls_by_number");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{number}.json")), body.to_string()).unwrap();
    }

    fn context(&self) -> EffectContext {
        EffectContext::new(
            GhCli::new(
                PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
                vec![
                    "--stub-dir".to_string(),
                    self.dir.path().display().to_string(),
                ],
                "ghp_never_reaches_a_network".to_string(),
                "FIDDLE_GITHUB_TOKEN",
                self.dir.path().join("config"),
                PATIENT,
            ),
            unreachable_git(),
            self.dir.path().to_path_buf(),
            CancellationToken::new(),
        )
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    fn requests(&self) -> Vec<Vec<String>> {
        let dir = self.dir.path().join("requests");
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        files
            .iter()
            .filter_map(|file| {
                let recorded: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
                Some(
                    recorded["argv"]
                        .as_array()?
                        .iter()
                        .filter_map(|a| a.as_str().map(str::to_string))
                        .collect(),
                )
            })
            .collect()
    }

    fn calls(&self) -> usize {
        self.requests().len()
    }

    fn mutations(&self) -> usize {
        self.requests()
            .iter()
            .filter(|argv| {
                argv.iter()
                    .any(|a| ["POST", "PATCH", "PUT", "DELETE"].contains(&a.as_str()))
            })
            .count()
    }
}

fn decision_request() -> HumanDecisionRequest {
    HumanDecisionRequest {
        invocation_ref: INVOCATION_REF.to_string(),
        work_ref: None,
        capability: PUBLISH_CHANGE,
        binding: DecisionBinding {
            request: DecisionRequestId("0000000000000000".to_string()),
            effect: EffectId("0000000000000000".to_string()),
            payload: PayloadHash("0000000000000000".to_string()),
            head_sha: HEAD_SHA.to_string(),
        },
        question: "may this proceed".to_string(),
        rationale: "because".to_string(),
        risks: Vec::new(),
        alternatives: Vec::new(),
        evidence: Vec::new(),
    }
}

fn params_for(capability: CapabilityId) -> StepParams {
    StepParams {
        repo: Some(REPO.to_string()),
        head_owner: Some(OWNER.to_string()),
        branch: Some(BRANCH.to_string()),
        base: Some(BASE.to_string()),
        head_sha: Some(HEAD_SHA.to_string()),
        title: Some(TITLE.to_string()),
        body: Some(BODY.to_string()),
        draft: false,
        pull_request: Some(PR),
        check_workflow: Some(CHECK_WORKFLOW.to_string()),
        decision_request: Some(decision_request()),
        ..StepParams::for_capability(capability)
    }
}

fn params() -> StepParams {
    params_for(FIXTURE_REPAIR)
}

fn executor<'a>(
    forge: &'a Forge,
    ctx: &'a EffectContext,
    deployment: &'a Deployment,
) -> Executor<'a> {
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        deployment,
        ctx,
        forge,
        ReadRetry::none(),
    )
}

fn allowing() -> Deployment {
    Deployment(DeploymentRule::Allow)
}

#[tokio::test]
async fn a_registered_name_resolves_to_the_operation_that_name_means() {
    let forge = Forge::empty();
    let ctx = forge.context();
    let deployment = allowing();
    let executor = executor(&forge, &ctx, &deployment);
    let params = params();

    let construct = registry::resolve(&EffectName::parse(ENSURE_PULL_REQUEST).unwrap())
        .expect("a shipped name is registered");
    let receipt = construct(&executor, &params)
        .expect("the step names everything a pull request needs")
        .run(&executor, &params)
        .await
        .expect("a receipt");

    assert_eq!(receipt.kind.as_str(), ENSURE_PULL_REQUEST);
    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.external_ref.as_deref(), Some("7"));
    assert_eq!(
        forge.mutations(),
        1,
        "exactly one adapter call wrote, and it was the one the executor dispatched"
    );
    assert_eq!(
        forge.calls(),
        3,
        "the look before the write, the write, and the look after it"
    );
    assert_eq!(
        forge.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
            "authorize",
            "apply",
            "observe_postcondition",
        ],
        "a resolved name walks the whole authorization order and skips no step of it"
    );
}

#[tokio::test]
async fn every_registered_descriptor_builds_the_operation_its_name_means_or_refuses_in_its_name() {
    let forge = Forge::empty();
    let ctx = forge.context();
    let deployment = allowing();
    let executor = executor(&forge, &ctx, &deployment);
    let params = params();

    let mut built = Vec::new();
    let mut refused = Vec::new();
    for descriptor in BUILT_IN {
        let name = EffectName::parse(descriptor.name).expect("a registered name parses");
        let construct = registry::resolve(&name).expect("a registered effect has no constructor");
        match construct(&executor, &params) {
            Ok(effect) => {
                assert_eq!(
                    effect.kind(),
                    name,
                    "{} resolves to an operation that means something else",
                    descriptor.name
                );
                built.push(descriptor.name);
            }
            Err(EffectError::Unbuildable { kind, reason }) => {
                assert_eq!(
                    kind, name,
                    "{} refuses in another effect's name, so a reader is told the wrong \
                     operation could not be built",
                    descriptor.name
                );
                assert!(
                    reason.len() > 40,
                    "{} refuses with `{reason}`, which does not say what a step lacks",
                    descriptor.name
                );
                refused.push(descriptor.name);
            }
            Err(other) => panic!("{} answered a step with {other:?}", descriptor.name),
        }
    }

    assert_eq!(
        built,
        vec![
            "ensure_branch_published",
            "ensure_pull_request",
            "ensure_check_requested",
            "publish_decision_request",
            "ensure_pull_request_ready",
            "ensure_pull_request_body",
        ],
        "these are the effects a workflow step names, and the list is measured by building \
         every registered descriptor rather than declared"
    );
    assert_eq!(
        refused,
        vec![
            "jira.issue_filed",
            "jira.comment_added",
            "jira.issue_transitioned",
            "jira.pull_request_linked",
        ],
        "each of these carries an observed issue revision or a scan verdict in its identity, \
         which a synchronous `from_params` cannot read, so it is registered because the \
         executor refuses an unregistered name and not because a step can name it; a jira \
         effect that moved into the list above gained a constructor made of defaults"
    );
    assert_eq!(
        built.len() + refused.len(),
        BUILT_IN.len(),
        "every registered descriptor was asked"
    );
    assert_eq!(
        forge.calls(),
        0,
        "building an operation reaches no adapter; only running it may"
    );
}

#[test]
fn a_name_no_descriptor_holds_resolves_to_no_constructor() {
    assert!(
        registry::resolve(&EffectName::parse("jira.transition").unwrap()).is_none(),
        "a name this build does not register has no operation to mean"
    );
    assert!(
        registry::resolve(&EffectName::parse(ENSURE_PULL_REQUEST).unwrap()).is_some(),
        "and a name it does register has one"
    );
}

#[tokio::test]
async fn a_resolved_effect_proposing_for_another_capability_reaches_no_adapter() {
    let forge = Forge::empty();
    let ctx = forge.context();
    let deployment = allowing();
    let executor = executor(&forge, &ctx, &deployment);
    let params = params_for(STUB_MARK);

    let construct = registry::resolve(&EffectName::parse(ENSURE_PULL_REQUEST).unwrap())
        .expect("a shipped name is registered");
    let refusal = construct(&executor, &params)
        .expect("the step names everything a pull request needs")
        .run(&executor, &params)
        .await
        .expect_err("an executor bound to another capability is not a receipt");

    assert!(
        matches!(refusal, EffectError::PolicyDenied { .. }),
        "got {refusal:?}"
    );
    assert_eq!(
        forge.calls(),
        0,
        "the erasure sits above the executor, so a refused capability reaches no adapter"
    );
    assert_eq!(
        forge.steps(),
        ["validate_capability"],
        "and the walk stopped at the first check"
    );
}

#[tokio::test]
async fn a_denied_rule_stops_a_resolved_effect_before_any_mutation() {
    let forge = Forge::empty();
    let ctx = forge.context();
    let deployment = Deployment(DeploymentRule::Deny);
    let executor = executor(&forge, &ctx, &deployment);
    let params = params();

    let construct = registry::resolve(&EffectName::parse(ENSURE_PULL_REQUEST).unwrap())
        .expect("a shipped name is registered");
    let refusal = construct(&executor, &params)
        .expect("the step names everything a pull request needs")
        .run(&executor, &params)
        .await
        .expect_err("a denied kind is not a receipt");

    assert!(
        matches!(refusal, EffectError::PolicyDenied { .. }),
        "got {refusal:?}"
    );
    assert_eq!(
        forge.mutations(),
        0,
        "a resolved name reaches no write the deployment rule denies"
    );
    assert_eq!(
        forge.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
        ],
        "the walk stopped where the policy denied it"
    );
}

#[tokio::test]
async fn a_resolved_effect_whose_minimum_is_human_is_not_applied() {
    let forge = Forge::empty();
    forge.pull(PR, json!({ "draft": true, "node_id": NODE_ID }));
    let ctx = forge.context();
    let deployment = allowing();
    let executor = executor(&forge, &ctx, &deployment);
    let params = params();

    let construct = registry::resolve(&EffectName::parse(ENSURE_PULL_REQUEST_READY).unwrap())
        .expect("a shipped name is registered");
    let refusal = construct(&executor, &params)
        .expect("the step names everything a ready transition needs")
        .run(&executor, &params)
        .await
        .expect_err("an effect awaiting a human decision is not a receipt");

    assert!(
        matches!(refusal, EffectError::HumanDecisionRequired { .. }),
        "got {refusal:?}"
    );
    assert_eq!(
        forge.mutations(),
        0,
        "a resolved name does not apply what a human has not decided"
    );
    assert_eq!(
        forge.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
        ],
        "the walk stopped at the decision the capability's minimum requires"
    );
}

#[tokio::test]
async fn a_step_that_names_no_repository_builds_nothing_and_reaches_no_adapter() {
    let forge = Forge::empty();
    let ctx = forge.context();
    let deployment = allowing();
    let executor = executor(&forge, &ctx, &deployment);
    let params = StepParams {
        repo: None,
        ..params()
    };

    let construct = registry::resolve(&EffectName::parse(ENSURE_PULL_REQUEST).unwrap())
        .expect("a shipped name is registered");
    let refusal = construct(&executor, &params)
        .err()
        .expect("a step that names no repository builds no operation");

    assert!(
        matches!(refusal, EffectError::Unbuildable { .. }),
        "got {refusal:?}"
    );
    assert_eq!(forge.calls(), 0, "and nothing was asked of the forge");
}

#[test]
fn the_four_jira_names_resolve_and_jira_transition_still_does_not() {
    for registered in [
        JIRA_ISSUE_FILED,
        JIRA_COMMENT_ADDED,
        JIRA_ISSUE_TRANSITIONED,
        JIRA_PULL_REQUEST_LINKED,
    ] {
        assert!(
            registry::resolve(&EffectName::parse(registered).unwrap()).is_some(),
            "{registered} is an effect this build performs, and the executor refuses \
             UnknownEffect for a name no descriptor holds"
        );
    }

    let unperformed = EffectName::parse("jira.transition").unwrap();

    assert!(
        !BUILT_IN
            .iter()
            .any(|descriptor| descriptor.name == unperformed.as_str()),
        "no descriptor may carry the name `jira.transition`. This is the root fact, and \
         `admissible` reads `BUILT_IN` directly rather than through a lookup: \
         `an_admissible_extension_is_answered_beside_the_built_ins` \
         (`src/effect/registry.rs`) installs a test extension that claims this name, and a \
         built-in of the same name makes `admissible` answer `Duplicate` instead"
    );

    assert!(
        registry::describe(&unperformed).is_none(),
        "`describe` is what refuses `jira.transition` for four tests, and registering the \
         name reds every one. In `src/effect/registry.rs`: \
         `lookup_refuses_a_name_no_descriptor_holds`. In `tests/effect_protocol.rs`, through \
         `Executor::walk`: \
         `an_unregistered_proposal_is_refused_before_an_identity_is_derived` and \
         `an_unregistered_name_is_refused_ahead_of_the_capability_it_names`. In \
         `tests/workflow_capability.rs`, through `WorkflowCapability::new`: \
         `an_effect_this_build_does_not_perform_is_refused_when_the_workflow_is_built`"
    );

    assert!(
        registry::resolve(&unperformed).is_none(),
        "and `resolve` is what refuses it for the sixth, \
         `a_name_no_descriptor_holds_resolves_to_no_constructor` in this file. Five further \
         tests spell `jira.transition` and do not depend on it, so none of them is evidence \
         for this invariant: `a_name_no_rule_key_spells_is_left_ungated` \
         (`fiddle-cli/src/config.rs`) passes registered or not, because `rule_for` allows any \
         row a document did not write; `a_name_outside_the_grammar_is_refused` \
         (`fiddle-core/src/effect.rs`) tests the grammar alone; and \
         `every_effect_failure_declares_which_exit_row_it_belongs_in`, \
         `no_other_permanent_refusal_became_a_wait` (`src/effect/receipt.rs`) and \
         `no_effect_failure_a_workflow_can_meet_is_a_wait` \
         (`tests/workflow_capability.rs`) build an `EffectError` value and ask the registry \
         nothing. `fiddle-xc0u` delivers the seventh dependent case: \
         `the_shipped_document_is_admitted_and_a_document_naming_an_unknown_effect_is_not` \
         (`tests/toil_document.rs`) substitutes `jira.transition` into the shipped toil \
         document and asserts `WorkflowRefusal::Unperformable`, so a toil document naming \
         this effect refuses at load. `fiddle-xc0u` adds that file, so a tree without \
         `tests/toil_document.rs` does not hold this case yet"
    );
}

#[test]
fn every_jira_spelling_is_frozen() {
    assert_eq!(JIRA_ISSUE_FILED, "jira.issue_filed");
    assert_eq!(JIRA_COMMENT_ADDED, "jira.comment_added");
    assert_eq!(JIRA_ISSUE_TRANSITIONED, "jira.issue_transitioned");
    assert_eq!(JIRA_PULL_REQUEST_LINKED, "jira.pull_request_linked");
}
