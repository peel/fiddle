mod fixture;
mod support;

use fiddle_core::{
    AttemptId, DeploymentRule, EffectId, EffectName, EvidenceRef, HumanDecisionRequirement,
    NextAction, PayloadHash, Published, WorkItemState, ENSURE_PULL_REQUEST,
    ENSURE_PULL_REQUEST_READY, JIRA_PULL_REQUEST_LINKED, STUB_MARK,
};
use fiddle_runtime::agent::{AgentBudget, ToolHost, ToolReceipts, Verdict};
use fiddle_runtime::capability::workflow::{
    without_waiting, Step, Workflow, WorkflowCapability, WorkflowPorts, WorkflowRefusal, WORKFLOW,
};
use fiddle_runtime::capability::{
    Capability, CapabilityError, Executed, ExecutionGrant, ExecutionInput,
};
use fiddle_runtime::effect::{
    registry, EffectContext, EffectError, EffectOutcome, EffectTrace, ErasedReceipt, ExecutionStep,
    Executor, OutputRefusal, ReadRetry, Recurrence, StepOutputs, StepParams,
};
use fiddle_runtime::workspace::{Workspace, WorkspaceCommand};
use fiddle_runtime::{GhCli, GhError, Redaction};
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::json;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::stub_jira::{client_for, StubJira, WriteRoute};
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const ATTEMPT: &str = "01JQZX0000000000000000000";

const STAGE: &str = "triage";

const REPO: &str = "peel/r";

const OWNER: &str = "peel";

const BASE: &str = "main";

const BRANCH: &str = "fiddle/abc";

const HEAD_SHA: &str = "deadbeef";

const PR: u64 = 7;

const STALE_PULL_REQUEST: u64 = 9001;

const ISSUE: &str = "IDENT-1";

const AT_SEVEN: &str = "2026-08-26T07:00:00.000+0000";

const AT_EIGHT: &str = "2026-08-26T08:00:00.000+0000";

const PATIENT: Duration = Duration::from_secs(60);

const TRACE: &str = "trace";

const CHANGE_EVALUATE: &str = "change_evaluate.md";

const A_SIGNATURE: &str = "crates/fiddle-runtime/src/effect/mod.rs changes a public signature \
                           the ticket did not name";

const A_SECOND_FAULT: &str = "crates/fiddle-runtime/src/jira/link.rs fixes a second fault, and \
                              the ticket asked only for the first";

struct World {
    dir: TempDir,
    workspace: Arc<Workspace>,
    steps: Mutex<Vec<&'static str>>,
    jira: Option<StubJira>,
}

impl EffectTrace for World {
    fn step(&self, _kind: &EffectName, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

fn world() -> World {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("config")).unwrap();
    std::fs::create_dir_all(dir.path().join("prompts")).unwrap();
    std::fs::write(
        dir.path().join("prompts/triage.md"),
        "Triage this project.\n",
    )
    .unwrap();
    std::fs::copy(
        shipped_prompts().join(CHANGE_EVALUATE),
        dir.path().join("prompts").join(CHANGE_EVALUATE),
    )
    .expect("this repository ships the evaluation prompt as a file");
    let repo = fixture::trivial_repo(dir.path());
    let workspace = Workspace::create(
        &repo,
        &dir.path().join("ws"),
        &AttemptId(ATTEMPT.to_string()),
        CancellationToken::new(),
    )
    .expect("a workspace");
    World {
        dir,
        workspace: Arc::new(workspace),
        steps: Mutex::new(Vec::new()),
        jira: None,
    }
}

async fn world_holding(issue: &str) -> World {
    let server = StubJira::start().await;
    server.holds_issue_labelled(issue, &[]).await;
    World {
        jira: Some(server),
        ..world()
    }
}

impl World {
    fn context(&self) -> EffectContext {
        let ctx = self.plain_context();
        match &self.jira {
            Some(server) => ctx.with_jira(client_for(server)),
            None => ctx,
        }
    }

    fn jira(&self) -> &StubJira {
        self.jira
            .as_ref()
            .expect("this world was built with a Jira the run can reach")
    }

    async fn opened_pull_request_number(&self) -> u64 {
        let ctx = self.plain_context();
        let listed = ctx
            .gh
            .api(
                "GET",
                &format!("/repos/{REPO}/pulls?head={OWNER}%3A{BRANCH}&base={BASE}&state=open"),
                None,
                &CancellationToken::new(),
            )
            .await
            .expect("the forge answers the pull request lookup")
            .body;
        let listed = listed.as_array().expect("a list of pull requests").clone();
        assert_eq!(
            listed.len(),
            1,
            "one pull request is open on this branch, so the number below is not a choice"
        );
        listed[0]["number"]
            .as_u64()
            .expect("the forge names the pull request it holds")
    }

    fn the_forge_already_numbered(&self, number: u64) {
        std::fs::write(
            self.dir.path().join("pulls_seed"),
            json!([{
                "number": number,
                "head": format!("{OWNER}:another"),
                "base": BASE,
                "state": "open",
                "title": "a pull request on another branch",
            }])
            .to_string(),
        )
        .unwrap();
    }

    fn the_forge_now_holds_only(&self, number: u64) {
        std::fs::write(self.dir.path().join("world"), "").unwrap();
        std::fs::write(
            self.dir.path().join("pulls_seed"),
            json!([{
                "number": number,
                "head": format!("{OWNER}:{BRANCH}"),
                "base": BASE,
                "state": "open",
                "title": "the same branch under another number",
            }])
            .to_string(),
        )
        .unwrap();
    }

    async fn linked_pull_requests(&self) -> Vec<u64> {
        self.jira()
            .writes()
            .await
            .iter()
            .filter(|write| write.route == WriteRoute::AddComment)
            .filter_map(|write| {
                write.body["body"]["content"][0]["content"][0]["text"]
                    .as_str()
                    .and_then(|text| text.rsplit_once('/'))
                    .and_then(|(_, number)| number.parse::<u64>().ok())
            })
            .collect()
    }

    fn plain_context(&self) -> EffectContext {
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

    fn pull(&self, number: u64, body: serde_json::Value) {
        let dir = self.dir.path().join("pulls_by_number");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{number}.json")), body.to_string()).unwrap();
    }

    fn ports(&self, model: MockCompletionModel) -> WorkflowPorts<MockCompletionModel> {
        WorkflowPorts {
            model,
            host: ToolHost {
                workspace: Arc::clone(&self.workspace),
                cancel: CancellationToken::new(),
                check: appending("agent"),
                commands: Arc::new(Vec::new()),
                command_timeout: PATIENT,
                receipts: Arc::new(Mutex::new(ToolReceipts::default())),
            },
            budget: AgentBudget {
                max_turns: 8,
                max_tokens: 4096,
                deadline: PATIENT,
                max_changed_files: 16,
                tool_timeout: PATIENT,
            },
            redaction: Redaction::of("sk-mock-must-not-appear-0d1e"),
            transcripts: None,
            prompts: self.dir.path().join("prompts"),
        }
    }

    fn ran(&self) -> Vec<String> {
        std::fs::read_to_string(self.workspace.root().join(TRACE))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
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

fn appending(line: &str) -> WorkspaceCommand {
    WorkspaceCommand {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), format!("echo {line} >> {TRACE}")],
        timeout: PATIENT,
    }
}

fn check_step(line: &str) -> Step {
    let command = appending(line);
    Step::Check {
        program: command.program,
        args: command.args,
        timeout_secs: 60,
    }
}

fn failing_step() -> Step {
    Step::Check {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "exit 3".to_string()],
        timeout_secs: 60,
    }
}

fn agent_step() -> Step {
    Step::Agent {
        prompt: PathBuf::from("triage.md"),
        max_turns: 4,
    }
}

fn shipped_prompts() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../workflows/prompts")
}

fn evaluate_step() -> Step {
    Step::Evaluate {
        prompt: PathBuf::from(CHANGE_EVALUATE),
        max_turns: 8,
    }
}

fn effect_step(name: &str) -> Step {
    Step::Effect {
        name: EffectName::parse(name).unwrap(),
    }
}

fn workflow(steps: Vec<Step>) -> Workflow {
    Workflow::new(STAGE.to_string(), STAGE.to_string(), steps).expect("a workflow with steps")
}

fn evaluate_then_open() -> Vec<Step> {
    vec![
        evaluate_step(),
        check_step("after"),
        effect_step(ENSURE_PULL_REQUEST),
    ]
}

fn canonical() -> Workflow {
    workflow(vec![
        agent_step(),
        check_step("check"),
        effect_step(ENSURE_PULL_REQUEST),
    ])
}

fn observed_issue() -> WorkItemState {
    observed_issue_at(AT_SEVEN)
}

fn observed_issue_at(updated: &str) -> WorkItemState {
    WorkItemState {
        id: ISSUE.to_string(),
        status: "In Progress".to_string(),
        projected_status: None,
        revision: Some(updated.to_string()),
    }
}

fn params_naming_a_stale_pull_request() -> StepParams {
    StepParams {
        pull_request: Some(STALE_PULL_REQUEST),
        ..params()
    }
}

fn params() -> StepParams {
    StepParams {
        repo: Some(REPO.to_string()),
        head_owner: Some(OWNER.to_string()),
        branch: Some(BRANCH.to_string()),
        base: Some(BASE.to_string()),
        head_sha: Some(HEAD_SHA.to_string()),
        title: Some("fiddle: triage".to_string()),
        body: Some("opened by fiddle".to_string()),
        draft: false,
        pull_request: Some(PR),
        check_workflow: Some("ci.yml".to_string()),
        decision_request: None,
        ..StepParams::for_capability(WORKFLOW)
    }
}

fn executor<'a>(
    world: &'a World,
    ctx: &'a EffectContext,
    deployment: &'a Deployment,
) -> Executor<'a> {
    Executor::new(
        WORKFLOW,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        deployment,
        ctx,
        world,
        ReadRetry::none(),
    )
}

fn allowing() -> Deployment {
    Deployment(DeploymentRule::Allow)
}

fn grant_for(capability: fiddle_core::CapabilityId) -> ExecutionGrant {
    ExecutionGrant::authorise(
        &NextAction::Execute {
            capability_id: capability,
        },
        &AttemptId(ATTEMPT.to_string()),
    )
    .expect("an Execute derivation authorises")
}

fn grant() -> ExecutionGrant {
    grant_for(WORKFLOW)
}

fn reporting() -> MockCompletionModel {
    MockCompletionModel::new([
        MockTurn::tool_call("c1", "run_check", json!({})),
        MockTurn::text(
            json!({"changed_files": [], "summary": "triaged", "claimed_complete": true})
                .to_string(),
        ),
    ])
}

fn silent() -> MockCompletionModel {
    MockCompletionModel::new([])
}

fn answering(verdict: serde_json::Value) -> MockTurn {
    MockTurn::text(verdict.to_string())
}

fn rejecting() -> MockCompletionModel {
    MockCompletionModel::new([answering(
        json!({"verdict": "rejected", "findings": [A_SIGNATURE, A_SECOND_FAULT]}),
    )])
}

fn accepting() -> MockCompletionModel {
    MockCompletionModel::new([answering(json!({"verdict": "accepted"}))])
}

fn offered_to(model: &MockCompletionModel) -> Vec<String> {
    let requests = model.requests();
    assert!(
        !requests.is_empty(),
        "no request reached the model, so the tool names below are the names of nothing"
    );
    let mut names: Vec<String> = requests
        .iter()
        .flat_map(|request| request.tools.iter().map(|tool| tool.name.clone()))
        .collect();
    names.sort();
    names.dedup();
    names
}

async fn entered_and_concluded(
    world: &World,
    steps: Vec<Step>,
    model: MockCompletionModel,
) -> (Vec<StepOutputs>, Executed) {
    let ctx = world.context();
    let deployment = allowing();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(steps),
        executor(world, &ctx, &deployment),
        params(),
        world.ports(model),
    )
    .expect("a workflow this build can run");
    let concluded = capability
        .execute(ExecutionInput::unobserved(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
        ))
        .await
        .expect("a workflow that ran to an end");
    (capability.earned_on_entering_each_step(), concluded)
}

async fn ran_with(world: &World, steps: Vec<Step>, model: MockCompletionModel) -> Vec<StepOutputs> {
    entered_and_concluded(world, steps, model).await.0
}

async fn concluded_by(world: &World, steps: Vec<Step>, model: MockCompletionModel) -> Executed {
    entered_and_concluded(world, steps, model).await.1
}

async fn refused_by(
    world: &World,
    steps: Vec<Step>,
    model: MockCompletionModel,
) -> CapabilityError {
    let ctx = world.context();
    let deployment = allowing();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(steps),
        executor(world, &ctx, &deployment),
        params(),
        world.ports(model),
    )
    .expect("a workflow this build can run");
    capability
        .execute(ExecutionInput::unobserved(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
        ))
        .await
        .expect_err("this run was not expected to complete")
}

#[tokio::test]
async fn a_workflow_runs_its_steps_in_the_order_the_document_names_them() {
    let world = world();
    let ctx = world.context();
    let deployment = allowing();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        canonical(),
        executor(&world, &ctx, &deployment),
        params(),
        world.ports(reporting()),
    )
    .expect("a workflow this build can run");

    let evidence = capability
        .execute(ExecutionInput::unobserved(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
        ))
        .await
        .expect("a completed workflow");

    assert_eq!(
        world.ran(),
        ["agent", "check"],
        "the agent step ran before the check step"
    );
    assert_eq!(
        world.mutations(),
        1,
        "the effect step ran, and it ran exactly once"
    );
    assert_eq!(
        evidence,
        Executed::Earned(EvidenceRef(format!("workflow:{STAGE}:{ATTEMPT}"))),
        "the evidence names the workflow and the attempt it ran under"
    );
    assert_eq!(
        capability.receipts().len(),
        1,
        "an effect a workflow performed leaves a receipt"
    );
}

#[tokio::test]
async fn a_step_that_fails_stops_every_step_after_it() {
    let world = world();
    let ctx = world.context();
    let deployment = allowing();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(vec![
            check_step("one"),
            failing_step(),
            check_step("three"),
            effect_step(ENSURE_PULL_REQUEST),
        ]),
        executor(&world, &ctx, &deployment),
        params(),
        world.ports(silent()),
    )
    .expect("a workflow this build can run");

    let refusal = capability
        .execute(ExecutionInput::unobserved(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
        ))
        .await
        .expect_err("a workflow whose check exits 3 did not complete");

    assert!(
        matches!(refusal, CapabilityError::CheckFailed { exit_code: 3, .. }),
        "got {refusal:?}"
    );
    assert_eq!(
        world.ran(),
        ["one"],
        "the steps after the failure did not run"
    );
    assert_eq!(world.calls(), 0, "and the effect step reached no adapter");
    assert_ne!(
        refusal.recurrence(),
        Recurrence::Awaiting,
        "a failed check is not a wait"
    );
}

#[tokio::test]
async fn a_grant_for_another_capability_runs_no_step() {
    let world = world();
    let ctx = world.context();
    let deployment = allowing();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        canonical(),
        executor(&world, &ctx, &deployment),
        params(),
        world.ports(reporting()),
    )
    .expect("a workflow this build can run");

    let refusal = capability
        .execute(ExecutionInput::unobserved(
            grant_for(STUB_MARK),
            "fiddle-demo",
            INVOCATION_REF,
        ))
        .await
        .expect_err("a grant for another capability is not evidence");

    assert!(
        matches!(refusal, CapabilityError::NotAuthorised { .. }),
        "got {refusal:?}"
    );
    assert_eq!(world.ran(), Vec::<String>::new(), "no step ran");
    assert_eq!(world.calls(), 0, "and no adapter was reached");
}

#[tokio::test]
async fn a_run_asked_for_another_invocation_runs_no_step() {
    let world = world();
    let ctx = world.context();
    let deployment = allowing();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        canonical(),
        executor(&world, &ctx, &deployment),
        params(),
        world.ports(reporting()),
    )
    .expect("a workflow this build can run");

    let refusal = capability
        .execute(ExecutionInput::unobserved(
            grant(),
            "fiddle-demo",
            "beans:somebody-else",
        ))
        .await
        .expect_err("an executor bound elsewhere is not evidence");

    assert!(
        matches!(refusal, CapabilityError::Misbound { .. }),
        "got {refusal:?}"
    );
    assert_eq!(world.ran(), Vec::<String>::new(), "no step ran");
}

#[test]
fn the_stage_a_bundle_files_this_under_is_the_static_one_not_the_documents() {
    let world = world();
    let ctx = world.context();
    let deployment = allowing();
    let document = Workflow::new(
        STAGE.to_string(),
        "the document says review".to_string(),
        vec![check_step("check")],
    )
    .unwrap();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        document,
        executor(&world, &ctx, &deployment),
        params(),
        world.ports(silent()),
    )
    .expect("a workflow this build can run");

    assert_eq!(capability.stage(), STAGE);
    assert_ne!(
        capability.stage(),
        capability.workflow().stage(),
        "the static stage is what a bundle is filed under, and it was not read from the document"
    );
    assert_eq!(
        canonical().stage(),
        STAGE,
        "and the canonical document and its static stage agree"
    );
}

#[tokio::test]
async fn a_workflow_naming_an_effect_this_build_gates_is_refused_when_it_is_built() {
    let world = world();
    world.pull(PR, json!({ "draft": true, "node_id": "PR_kwDOabcdef" }));
    let ctx = world.context();
    let deployment = allowing();

    let refusal = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(vec![
            check_step("one"),
            effect_step(ENSURE_PULL_REQUEST_READY),
        ]),
        executor(&world, &ctx, &deployment),
        params(),
        world.ports(silent()),
    )
    .err()
    .expect("a step that can suspend is not a workflow this build runs");

    assert_eq!(
        refusal,
        WorkflowRefusal::Gated {
            name: EffectName::parse(ENSURE_PULL_REQUEST_READY).unwrap()
        }
    );
    assert_eq!(world.ran(), Vec::<String>::new(), "no step ran");

    let executor = executor(&world, &ctx, &deployment);
    let params = params();
    let direct = registry::resolve(&EffectName::parse(ENSURE_PULL_REQUEST_READY).unwrap())
        .expect("a shipped name is registered")(&executor, &params)
    .expect("the step names everything a ready transition needs")
    .run(&executor, &params)
    .await
    .expect_err("an effect awaiting a human decision is not a receipt");

    assert_eq!(
        direct.recurrence(),
        Recurrence::Awaiting,
        "the same effect run outside a workflow does wait, so the refusal above is not vacuous"
    );
}

#[tokio::test]
async fn an_effect_a_deployment_rule_gates_fails_the_run_rather_than_waiting_for_a_person() {
    let deployment = Deployment(DeploymentRule::RequireHuman);
    {
        let alone = world();
        let ctx = alone.context();
        let executor = executor(&alone, &ctx, &deployment);
        let params = params();
        let direct = registry::resolve(&EffectName::parse(ENSURE_PULL_REQUEST).unwrap())
            .expect("a shipped name is registered")(&executor, &params)
        .expect("the step names everything a pull request needs")
        .run(&executor, &params)
        .await
        .expect_err("a rule requiring a person is not a receipt");
        assert_eq!(
            direct.recurrence(),
            Recurrence::Awaiting,
            "this world does gate the effect on a person"
        );
        assert_eq!(alone.mutations(), 0);
    }

    let world = world();
    let ctx = world.context();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(vec![check_step("one"), effect_step(ENSURE_PULL_REQUEST)]),
        executor(&world, &ctx, &deployment),
        params(),
        world.ports(silent()),
    )
    .expect("no minimum in this build gates this effect, so the workflow is built");

    let refusal = capability
        .execute(ExecutionInput::unobserved(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
        ))
        .await
        .expect_err("a workflow that cannot finish is not evidence");

    assert!(
        matches!(refusal, CapabilityError::WouldWait { .. }),
        "got {refusal:?}"
    );
    assert_eq!(
        refusal.recurrence(),
        Recurrence::Permanent,
        "a workflow concludes; it does not wait for an answer it cannot ask for"
    );
    assert_ne!(refusal.recurrence(), Recurrence::Awaiting);
    assert_eq!(world.ran(), ["one"], "the steps before the effect ran");
    assert_eq!(world.mutations(), 0, "and the gated effect wrote nothing");
}

#[test]
fn no_effect_failure_a_workflow_can_meet_is_a_wait() {
    let kind = EffectName::parse(ENSURE_PULL_REQUEST).unwrap();
    let reason = || "because".to_string();
    let failures = || {
        vec![
            EffectError::UnknownEffect {
                kind: EffectName::parse("jira.transition").unwrap(),
            },
            EffectError::Unbuildable {
                kind: kind.clone(),
                reason: reason(),
            },
            EffectError::PolicyDenied {
                kind: kind.clone(),
                reason: reason(),
            },
            EffectError::HumanDecisionRequired {
                kind: kind.clone(),
                reason: reason(),
            },
            EffectError::Unresolved {
                kind: kind.clone(),
                reason: reason(),
            },
            EffectError::PayloadDiverged {
                kind: kind.clone(),
                approved: PayloadHash("a".to_string()),
                applying: PayloadHash("b".to_string()),
            },
            EffectError::DuplicateState {
                kind: kind.clone(),
                count: 2,
            },
            EffectError::IdentityDiverged {
                kind: kind.clone(),
                part: "target",
                proposed: "a".to_string(),
                performing: "b".to_string(),
            },
            EffectError::Adapter {
                kind: kind.clone(),
                source: Box::new(GhError::Auth),
            },
        ]
    };

    assert_eq!(
        failures().len(),
        EffectError::VARIANT_COUNT,
        "an effect failure was added without a case here: the list above is written \
         by hand and the name of this test claims it holds every failure a workflow \
         can meet, so a variant missing from it is a claim this test never checked"
    );
    assert_eq!(
        failures()
            .iter()
            .filter(|error| error.recurrence() == Recurrence::Awaiting)
            .count(),
        1,
        "one of these failures does wait, so the mapping below has something to change"
    );

    let mut changed = 0;
    for error in failures() {
        let waited = error.recurrence() == Recurrence::Awaiting;
        let expected = error.recurrence();
        let mapped = without_waiting(error);
        assert_ne!(
            mapped.recurrence(),
            Recurrence::Awaiting,
            "a workflow does not wait: {mapped}"
        );
        match waited {
            true => {
                assert!(matches!(mapped, CapabilityError::WouldWait { .. }));
                changed += 1;
            }
            false => {
                assert!(
                    matches!(mapped, CapabilityError::Effect(_)),
                    "a failure that was not a wait must be carried through unchanged: {mapped}"
                );
                assert_eq!(
                    mapped.recurrence(),
                    expected,
                    "and it must keep the exit row the effect gave it"
                );
            }
        }
    }
    assert_eq!(changed, 1, "exactly the waiting failure was rewritten");
}

#[test]
fn only_the_effects_this_build_performs_without_a_person_are_admitted() {
    let world = world();
    let ctx = world.context();
    let deployment = allowing();
    let mut admitted = 0;
    let mut gated = 0;

    for descriptor in registry::BUILT_IN {
        let name = EffectName::parse(descriptor.name).expect("a registered name parses");
        let built = WorkflowCapability::new(
            WORKFLOW,
            STAGE,
            workflow(vec![Step::Effect { name: name.clone() }]),
            executor(&world, &ctx, &deployment),
            params(),
            world.ports(silent()),
        );
        match descriptor.minimum {
            HumanDecisionRequirement::Human => {
                assert_eq!(
                    built.err(),
                    Some(WorkflowRefusal::Gated { name }),
                    "{} gates on a person and was admitted",
                    descriptor.name
                );
                gated += 1;
            }
            HumanDecisionRequirement::Automatic => {
                assert!(
                    built.is_ok(),
                    "{} needs no person and was refused",
                    descriptor.name
                );
                admitted += 1;
            }
        }
    }

    assert_eq!(
        admitted + gated,
        registry::BUILT_IN.len(),
        "every registered effect was put to this workflow"
    );
    assert!(
        gated > 0,
        "no effect was gated, so nothing was refused here"
    );
    assert!(admitted > 0, "no effect was admitted, so nothing was run");
}

#[test]
fn a_prompt_this_run_cannot_read_is_refused_before_any_step_runs() {
    let world = world();
    let ctx = world.context();
    let deployment = allowing();

    let refusal = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(vec![
            check_step("one"),
            Step::Agent {
                prompt: PathBuf::from("absent.md"),
                max_turns: 4,
            },
        ]),
        executor(&world, &ctx, &deployment),
        params(),
        world.ports(silent()),
    )
    .err()
    .expect("a step whose prompt is not there has no task");

    assert!(
        matches!(refusal, WorkflowRefusal::Unreadable { .. }),
        "got {refusal:?}"
    );
    assert_eq!(world.ran(), Vec::<String>::new(), "no step ran");
}

#[test]
fn a_prompt_that_says_nothing_is_refused_when_the_workflow_is_built() {
    let world = world();
    let ctx = world.context();
    let deployment = allowing();
    std::fs::write(world.dir.path().join("prompts/empty.md"), "  \n").unwrap();

    let refusal = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(vec![Step::Agent {
            prompt: PathBuf::from("empty.md"),
            max_turns: 4,
        }]),
        executor(&world, &ctx, &deployment),
        params(),
        world.ports(silent()),
    )
    .err()
    .expect("a prompt that says nothing gives the step no task");

    assert!(
        matches!(refusal, WorkflowRefusal::Taskless { .. }),
        "got {refusal:?}"
    );
}

#[test]
fn an_effect_this_build_does_not_perform_is_refused_when_the_workflow_is_built() {
    let world = world();
    let ctx = world.context();
    let deployment = allowing();
    let name = EffectName::parse("jira.transition").unwrap();

    let refusal = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(vec![Step::Effect { name: name.clone() }]),
        executor(&world, &ctx, &deployment),
        params(),
        world.ports(silent()),
    )
    .err()
    .expect("a name this build cannot perform is not a step it can run");

    assert_eq!(refusal, WorkflowRefusal::Unperformable { name });
}

#[test]
fn a_workflow_proposing_under_another_capability_is_refused_when_it_is_built() {
    let world = world();
    let ctx = world.context();
    let deployment = allowing();

    let refusal = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        canonical(),
        executor(&world, &ctx, &deployment),
        StepParams {
            capability: STUB_MARK,
            ..params()
        },
        world.ports(silent()),
    )
    .err()
    .expect("a workflow may not propose effects under a capability it is not filed under");

    assert_eq!(
        refusal,
        WorkflowRefusal::Misbound {
            filed: WORKFLOW,
            proposing: STUB_MARK
        }
    );
}

#[test]
fn the_identity_a_workflow_is_filed_under_is_not_one_of_the_five_the_cli_selects() {
    let selectable: Vec<&str> = fiddle_runtime::CAPABILITIES
        .iter()
        .map(|capability| capability.0)
        .collect();
    assert!(
        !selectable.contains(&WORKFLOW.0),
        "a workflow needs a document, so naming it on the command line would select nothing"
    );
    assert_eq!(
        selectable,
        [
            "stub_mark",
            "fixture_repair",
            "publish_change",
            "propose_change",
            "cve_mitigate"
        ]
    );
}

#[tokio::test]
async fn a_pull_request_one_step_opens_reaches_the_step_that_links_it() {
    let world = world_holding(ISSUE).await;
    world.the_forge_already_numbered(PR);
    let ctx = world.context();
    let deployment = allowing();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(vec![
            effect_step(ENSURE_PULL_REQUEST),
            effect_step(JIRA_PULL_REQUEST_LINKED),
        ]),
        executor(&world, &ctx, &deployment),
        params_naming_a_stale_pull_request(),
        world.ports(silent()),
    )
    .expect("a workflow this build can run");
    let observed = observed_issue();

    capability
        .execute(ExecutionInput::observed(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
            Some(&observed),
        ))
        .await
        .expect("both steps run");

    assert_eq!(
        world.mutations(),
        1,
        "the earlier step opened the pull request rather than finding one already there"
    );
    let opened = world.opened_pull_request_number().await;
    assert_eq!(
        world.linked_pull_requests().await,
        vec![opened],
        "the link names the pull request the earlier step opened"
    );
    assert_ne!(
        opened, STALE_PULL_REQUEST,
        "and the step parameters name another number, so the comparison above could not \
         have been satisfied by configuration"
    );
    assert_ne!(
        opened, PR,
        "the forge already held {PR} on another branch, so the number this run earned is \
         written in no line of this test"
    );
    assert_eq!(
        capability.receipts().len(),
        2,
        "both effect steps left a receipt"
    );
}

fn receipt_naming(kind: &str, external_ref: Option<&str>) -> ErasedReceipt {
    ErasedReceipt {
        kind: EffectName::parse(kind).unwrap(),
        effect_id: EffectId("0000000000000000".to_string()),
        payload_hash: PayloadHash("0000000000000000".to_string()),
        target: pull_request_target(),
        outcome: EffectOutcome::Committed,
        postcondition: "a pull request".to_string(),
        external_ref: external_ref.map(str::to_string),
    }
}

fn pull_request_target() -> String {
    format!("{REPO}/pulls/{BASE}...{OWNER}:{BRANCH}")
}

fn opening_then_linking() -> Workflow {
    workflow(vec![
        effect_step(ENSURE_PULL_REQUEST),
        effect_step(JIRA_PULL_REQUEST_LINKED),
    ])
}

fn opened() -> EffectName {
    EffectName::parse(ENSURE_PULL_REQUEST).unwrap()
}

#[tokio::test]
async fn a_link_step_reached_before_any_pull_request_is_opened_refuses_rather_than_reading_the_parameters(
) {
    let world = world_holding(ISSUE).await;
    let ctx = world.context();
    let deployment = allowing();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(vec![effect_step(JIRA_PULL_REQUEST_LINKED)]),
        executor(&world, &ctx, &deployment),
        params_naming_a_stale_pull_request(),
        world.ports(silent()),
    )
    .expect("a workflow this build can run");
    let observed = observed_issue();

    let outcome = capability
        .execute(ExecutionInput::observed(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
            Some(&observed),
        ))
        .await;

    assert!(
        world.jira().request_lines().await.is_empty(),
        "the step reached no Jira adapter, so it read the parameters for no number"
    );
    let refusal =
        outcome.expect_err("a link step earns its number from a step and not from configuration");
    assert!(
        format!("{refusal}").contains("no step before this one in this run opened a pull request"),
        "got {refusal}"
    );
    assert!(
        !format!("{refusal}").contains("fields.updated"),
        "the observed issue did reach the step, so this refusal is about the pull request \
         and not about the issue: {refusal}"
    );

    let unobserved = capability
        .execute(ExecutionInput::unobserved(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
        ))
        .await
        .expect_err("a run that observed no issue builds no issue-acting effect");

    assert!(
        format!("{unobserved}").contains("fields.updated"),
        "a run that observed no work item refuses for the issue and not for the pull \
         request, so the two refusals above are told apart: {unobserved}"
    );
}

#[test]
fn a_receipt_whose_external_reference_is_not_a_number_refuses_and_names_the_step() {
    let mut outputs = StepOutputs::default();

    let refusal = outputs
        .record(&receipt_naming(ENSURE_PULL_REQUEST, Some("PR_kwDOabcdef")))
        .expect_err("an external reference that is not a number names no pull request");

    assert_eq!(
        refusal,
        OutputRefusal::Unreadable {
            step: opened(),
            answered: "PR_kwDOabcdef".to_string()
        }
    );
    assert!(
        format!("{refusal}").contains(ENSURE_PULL_REQUEST),
        "the reason names the step that answered: {refusal}"
    );
    assert_eq!(
        outputs.pull_request(),
        None,
        "and no number was put in its place"
    );

    let carried = CapabilityError::from(refusal.clone());
    assert_eq!(
        format!("{carried}"),
        format!("{refusal}"),
        "a workflow carries the reason to its caller unchanged"
    );
    assert_eq!(
        carried.recurrence(),
        Recurrence::Permanent,
        "and a receipt this build cannot read is not a wait"
    );

    let mut corrected = StepOutputs::default();
    corrected
        .record(&receipt_naming(ENSURE_PULL_REQUEST, Some("4242")))
        .expect("a receipt that differs only in a readable reference is recorded");
    assert_eq!(
        corrected.pull_request(),
        Some(4242),
        "so the refusal above answers the reference and not the receipt around it"
    );
}

#[test]
fn a_receipt_that_names_no_pull_request_is_refused_for_another_reason_than_one_that_cannot_be_read()
{
    let mut absent = StepOutputs::default();
    let unnamed = absent
        .record(&receipt_naming(ENSURE_PULL_REQUEST, None))
        .expect_err("a receipt with no external reference names no pull request");

    let mut empty = StepOutputs::default();
    let unreadable = empty
        .record(&receipt_naming(ENSURE_PULL_REQUEST, Some("")))
        .expect_err("an empty external reference is not a pull request number");

    assert_eq!(unnamed, OutputRefusal::Unnamed { step: opened() });
    assert_eq!(
        unreadable,
        OutputRefusal::Unreadable {
            step: opened(),
            answered: String::new()
        }
    );
    assert_ne!(
        format!("{unnamed}"),
        format!("{unreadable}"),
        "an absent reference and one this build cannot read are two faults, and a reader \
         is told which one happened"
    );
    assert_eq!(absent.pull_request(), None);
    assert_eq!(empty.pull_request(), None);
}

#[test]
fn a_receipt_from_a_step_that_opens_no_pull_request_earns_nothing_and_refuses_nothing() {
    let mut outputs = StepOutputs::default();

    outputs
        .record(&receipt_naming(JIRA_PULL_REQUEST_LINKED, Some("10001")))
        .expect("a comment receipt carries no pull request output");
    assert_eq!(
        outputs.pull_request(),
        None,
        "a comment id is not a pull request number"
    );

    outputs
        .record(&receipt_naming(
            JIRA_PULL_REQUEST_LINKED,
            Some("not a number"),
        ))
        .expect("and a reference this build reads no output from is not refused here");
    assert_eq!(outputs.pull_request(), None);
}

#[test]
fn one_run_that_earns_two_different_pull_requests_refuses_and_earning_one_twice_does_not() {
    let mut outputs = StepOutputs::default();

    outputs
        .record(&receipt_naming(ENSURE_PULL_REQUEST, Some("7")))
        .expect("the first pull request is recorded");
    outputs
        .record(&receipt_naming(ENSURE_PULL_REQUEST, Some("7")))
        .expect("the same pull request twice is one pull request");
    assert_eq!(outputs.pull_request(), Some(7));

    let refusal = outputs
        .record(&receipt_naming(ENSURE_PULL_REQUEST, Some("4242")))
        .expect_err("two different pull requests in one run leave a later step no answer");

    assert_eq!(
        refusal,
        OutputRefusal::Diverged {
            step: opened(),
            held: 7,
            answered: 4242
        }
    );
    assert_eq!(
        outputs.pull_request(),
        Some(7),
        "and the number the run earned first is unchanged"
    );
}

#[tokio::test]
async fn a_second_run_does_not_inherit_the_pull_request_the_first_run_earned() {
    let world = world_holding(ISSUE).await;
    let ctx = world.context();
    let deployment = allowing();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        opening_then_linking(),
        executor(&world, &ctx, &deployment),
        params_naming_a_stale_pull_request(),
        world.ports(silent()),
    )
    .expect("a workflow this build can run");
    let earlier = observed_issue_at(AT_SEVEN);
    let later = observed_issue_at(AT_EIGHT);

    capability
        .execute(ExecutionInput::observed(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
            Some(&earlier),
        ))
        .await
        .expect("the first run opens and links");
    let first = world.opened_pull_request_number().await;
    world.the_forge_now_holds_only(4242);
    capability
        .execute(ExecutionInput::observed(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
            Some(&later),
        ))
        .await
        .expect("the second run links what it finds");
    let second = world.opened_pull_request_number().await;

    assert_ne!(
        first, second,
        "the forge answers a different pull request to the second run"
    );
    assert_eq!(
        world.linked_pull_requests().await,
        vec![first, second],
        "each run linked the pull request its own steps earned"
    );

    assert_eq!(
        capability.earned_on_entering_each_step(),
        vec![
            StepOutputs::default(),
            outputs_holding(first),
            StepOutputs::default(),
            outputs_holding(second),
        ],
        "each run entered its first step holding no pull request, so the second run read \
         nothing the first run earned"
    );
}

fn outputs_holding(number: u64) -> StepOutputs {
    let mut held = StepOutputs::default();
    held.record(&receipt_naming(
        ENSURE_PULL_REQUEST,
        Some(&number.to_string()),
    ))
    .expect("a readable pull request number records");
    held
}

#[tokio::test]
async fn a_run_starts_holding_no_pull_request_when_the_step_parameters_carry_one() {
    let world = world_holding(ISSUE).await;
    let ctx = world.context();
    let deployment = allowing();
    let capability = WorkflowCapability::new(
        WORKFLOW,
        STAGE,
        workflow(vec![effect_step(JIRA_PULL_REQUEST_LINKED)]),
        executor(&world, &ctx, &deployment),
        StepParams {
            earned: outputs_holding(STALE_PULL_REQUEST),
            ..params_naming_a_stale_pull_request()
        },
        world.ports(silent()),
    )
    .expect("a workflow this build can run");
    let observed = observed_issue();

    let outcome = capability
        .execute(ExecutionInput::observed(
            grant(),
            "fiddle-demo",
            INVOCATION_REF,
            Some(&observed),
        ))
        .await;

    assert_eq!(
        capability.earned_on_entering_each_step(),
        vec![StepOutputs::default()],
        "the run entered its only step holding no pull request, though the parameters \
         handed it one"
    );
    assert!(
        world.jira().request_lines().await.is_empty(),
        "and the step reached no Jira adapter with the number the parameters carried"
    );
    let refusal =
        outcome.expect_err("a number placed in the step parameters is not a number a step earned");
    assert!(
        format!("{refusal}").contains("no step before this one in this run opened a pull request"),
        "got {refusal}"
    );
}

#[tokio::test]
async fn a_run_that_stops_on_a_rejection_reads_back_the_sentences_the_model_wrote() {
    let world = world();
    let (entered, concluded) = entered_and_concluded(
        &world,
        vec![evaluate_step(), check_step("after")],
        rejecting(),
    )
    .await;

    assert_eq!(
        entered.len(),
        1,
        "the run entered the evaluation step and no step after it"
    );
    assert_eq!(
        entered[0].verdict(),
        None,
        "the run entered the evaluation step holding no verdict"
    );
    assert_eq!(
        concluded,
        Executed::Rejected {
            findings: vec![Published::of(A_SIGNATURE), Published::of(A_SECOND_FAULT)],
        },
        "the run reads back the sentences the model wrote, in the order it wrote them"
    );
    assert_ne!(
        concluded,
        Executed::Earned(EvidenceRef(format!("workflow:{STAGE}:{ATTEMPT}"))),
        "the model rejected the change and the run read it as evidence it earned"
    );
    assert_eq!(
        world.ran(),
        Vec::<String>::new(),
        "the evaluation step ran no command of its own, and the step after it never ran"
    );
}

#[tokio::test]
async fn the_step_after_an_evaluation_reads_the_acceptance_the_model_answered() {
    let world = world();
    let entered = ran_with(
        &world,
        vec![evaluate_step(), check_step("after")],
        accepting(),
    )
    .await;

    let read = entered[1]
        .verdict()
        .expect("the step after the evaluation is given the verdict the evaluation earned");
    assert_eq!(
        read,
        &Verdict::Accepted {},
        "the model accepted the change, so the same reading that answered a rejection above \
         answers an acceptance here"
    );
}

#[tokio::test]
async fn an_evaluation_step_is_offered_no_tool_that_changes_the_project() {
    let judged = world();
    let judge = accepting();
    ran_with(&judged, vec![evaluate_step()], judge.clone()).await;

    let repaired = world();
    let repairer = reporting();
    ran_with(&repaired, vec![agent_step()], repairer.clone()).await;

    assert_eq!(
        offered_to(&judge),
        ["list_files", "read_file", "search_files"],
        "an evaluation step is offered the three tools that read the project and no other"
    );
    assert_eq!(
        offered_to(&repairer),
        [
            "edit_file",
            "list_files",
            "read_file",
            "run_check",
            "search_files",
            "write_file"
        ],
        "the same reading shows a repair step being offered the tools that change the \
         project, so the reading above is not the reading of a list nothing fills"
    );
    assert_eq!(
        judged.ran(),
        Vec::<String>::new(),
        "the evaluation ran no check, and this world's check writes a file when it runs"
    );
    assert_eq!(
        repaired.ran(),
        ["agent"],
        "the repair step did run this world's check, so the line above is not vacuous"
    );
}

#[tokio::test]
async fn a_second_evaluation_that_answers_otherwise_refuses_rather_than_replacing_the_verdict() {
    let disagreed = world();
    let refusal = refused_by(
        &disagreed,
        vec![evaluate_step(), evaluate_step()],
        MockCompletionModel::new([
            answering(json!({"verdict": "accepted"})),
            answering(json!({"verdict": "rejected", "findings": [A_SIGNATURE]})),
        ]),
    )
    .await;

    assert_eq!(
        refusal.to_string(),
        OutputRefusal::Reconsidered {
            held: "accepted",
            answered: "rejected",
        }
        .to_string(),
        "got {refusal:?}"
    );
    assert_eq!(
        refusal.recurrence(),
        Recurrence::Permanent,
        "a run that cannot say which verdict it holds does not improve by being run again"
    );

    let agreed = world();
    let entered = ran_with(
        &agreed,
        vec![evaluate_step(), evaluate_step(), check_step("after")],
        MockCompletionModel::new([
            answering(json!({"verdict": "accepted"})),
            answering(json!({"verdict": "accepted"})),
        ]),
    )
    .await;
    assert_eq!(
        entered[2].verdict(),
        Some(&Verdict::Accepted {}),
        "two evaluations that answer the same thing are one verdict, so the refusal above \
         names the disagreement and not the second step"
    );
}

#[test]
fn two_rejections_are_one_verdict_only_when_they_name_the_same_findings() {
    let rejection = Verdict::Rejected {
        findings: vec![A_SIGNATURE.to_string()],
    };

    let mut repeated = StepOutputs::default();
    repeated
        .record_verdict(rejection.clone())
        .expect("a first rejection is recorded");
    repeated
        .record_verdict(rejection.clone())
        .expect("a second rejection that names the same findings is the verdict already held");
    assert_eq!(repeated.verdict(), Some(&rejection));

    let mut widened = StepOutputs::default();
    widened
        .record_verdict(rejection)
        .expect("a first rejection is recorded");
    let refusal = widened
        .record_verdict(Verdict::Rejected {
            findings: vec![A_SIGNATURE.to_string(), A_SECOND_FAULT.to_string()],
        })
        .expect_err("a rejection that names other findings is another verdict");
    assert_eq!(
        refusal,
        OutputRefusal::Reconsidered {
            held: "rejected",
            answered: "rejected",
        },
        "the refusal names both spellings, and the findings are what differ"
    );
}

#[tokio::test]
async fn a_rejection_that_names_nothing_it_read_is_refused_rather_than_carried() {
    let world = world();
    let refusal = refused_by(
        &world,
        vec![evaluate_step(), check_step("after")],
        MockCompletionModel::new([answering(json!({"verdict": "rejected", "findings": []}))]),
    )
    .await;

    assert!(
        refusal.to_string().contains("named nothing it read"),
        "got {refusal:?}"
    );
    assert_eq!(
        world.ran(),
        Vec::<String>::new(),
        "the step after the evaluation did not run"
    );
}

#[test]
fn an_answer_that_does_not_say_which_verdict_it_is_is_not_read_as_an_acceptance() {
    for text in [
        "{}",
        r#"{"findings": ["a finding"]}"#,
        r#"{"verdict": "unclear"}"#,
        r#"{"verdict": "rejected"}"#,
        r#"{"verdict": "accepted", "findings": ["a finding"]}"#,
    ] {
        let read = serde_json::from_str::<Verdict>(text);
        assert!(
            read.is_err(),
            "`{text}` was read as {read:?} rather than refused"
        );
    }
    assert_eq!(
        serde_json::from_str::<Verdict>(r#"{"verdict": "accepted"}"#).unwrap(),
        Verdict::Accepted {},
        "the one shape that does name an acceptance is read as one, so the refusals above \
         are not the refusals of a type nothing deserializes"
    );
    assert_eq!(
        serde_json::from_str::<Verdict>(r#"{"verdict": "rejected", "findings": ["a finding"]}"#)
            .unwrap(),
        Verdict::Rejected {
            findings: vec!["a finding".to_string()],
        },
        "and so is the one shape that names a rejection"
    );
}

#[tokio::test]
async fn the_evaluation_step_sends_the_prompt_this_repository_ships() {
    let world = world();
    let model = accepting();
    ran_with(&world, vec![evaluate_step()], model.clone()).await;

    let shipped = std::fs::read_to_string(shipped_prompts().join(CHANGE_EVALUATE))
        .expect("this repository ships the evaluation prompt as a file");
    let sent = serde_json::to_string(&model.requests()[0].chat_history)
        .expect("the messages the model received serialize");
    let mut compared = 0;
    for line in shipped.lines().filter(|line| line.len() > 40) {
        assert!(
            sent.contains(line),
            "the shipped prompt says `{line}` and the model was not told it"
        );
        compared += 1;
    }
    assert!(
        compared > 8,
        "only {compared} lines of the shipped prompt were long enough to compare, so this \
         test compared almost nothing"
    );
}

#[tokio::test]
async fn a_rejected_evaluation_opens_no_pull_request_and_stops_the_steps_after_it() {
    let refused = world();
    ran_with(&refused, evaluate_then_open(), rejecting()).await;

    let accepted = world();
    ran_with(&accepted, evaluate_then_open(), accepting()).await;

    assert_eq!(
        refused.mutations(),
        0,
        "a rejected evaluation opened a pull request"
    );
    assert_eq!(
        accepted.mutations(),
        1,
        "an accepted evaluation opens one, so the count above counts something that moves"
    );
    assert_eq!(
        refused.ran(),
        Vec::<String>::new(),
        "the check step after the rejected evaluation ran"
    );
    assert_eq!(
        accepted.ran(),
        ["after"],
        "the same check step runs after an acceptance, so the reading above is not vacuous"
    );
}

#[tokio::test]
async fn a_rejected_run_and_a_failed_run_conclude_as_different_values() {
    let refused = world();
    let rejection = concluded_by(&refused, evaluate_then_open(), rejecting()).await;
    assert_eq!(
        rejection,
        Executed::Rejected {
            findings: vec![Published::of(A_SIGNATURE), Published::of(A_SECOND_FAULT)],
        },
        "the run concludes as the rejection the model answered, and names what it read"
    );

    let broken = world();
    let failure = refused_by(
        &broken,
        vec![failing_step(), effect_step(ENSURE_PULL_REQUEST)],
        silent(),
    )
    .await;
    assert!(
        matches!(
            failure,
            CapabilityError::CheckFailed {
                exit_code: 3,
                claimed: false,
                ..
            }
        ),
        "a step that failed is read as the failure it is, and not as a rejection: got {failure:?}"
    );

    let accepted = world();
    let earned = concluded_by(&accepted, evaluate_then_open(), accepting()).await;
    assert_eq!(
        earned,
        Executed::Earned(EvidenceRef(format!("workflow:{STAGE}:{ATTEMPT}"))),
        "an accepted run concludes as the evidence it earned, so the rejection above is one \
         of two values this run can conclude as, and not the only one"
    );
}

#[tokio::test]
async fn a_rejection_with_no_step_after_it_is_still_a_rejection() {
    let refused = world();
    let rejection = concluded_by(&refused, vec![evaluate_step()], rejecting()).await;

    assert_eq!(
        rejection,
        Executed::Rejected {
            findings: vec![Published::of(A_SIGNATURE), Published::of(A_SECOND_FAULT)],
        },
        "a rejection is what the verdict says, and not what the steps after it did"
    );

    let accepted = world();
    let earned = concluded_by(&accepted, vec![evaluate_step()], accepting()).await;
    assert_eq!(
        earned,
        Executed::Earned(EvidenceRef(format!("workflow:{STAGE}:{ATTEMPT}"))),
        "the same one-step workflow earns its evidence when the model accepts"
    );
}
