mod support;

use fiddle_core::{
    combine, decision_request_id, effect_id, payload_hash, DecisionBinding, DeploymentRule,
    EffectKind, HumanDecisionRequirement, InterpretedHumanDecision, PolicyDecision, ProposedEffect,
    FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    EffectContext, EffectError, EffectOutcome, EffectReceipt, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ReadRetry, ResolvedDecision,
};
use fiddle_runtime::github::{EnsurePullRequestReady, GhError, ReadyPullRequest};
use fiddle_runtime::GhCli;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

const REPO: &str = "acme/r";

const PR: u64 = 7;

const NODE_ID: &str = "PR_kwDOabcdef";

const PATIENT: Duration = Duration::from_secs(60);

const HEAD_SHA: &str = "aaaa";

fn op() -> EnsurePullRequestReady {
    op_at_head(HEAD_SHA)
}

fn op_at_head(head_sha: &str) -> EnsurePullRequestReady {
    EnsurePullRequestReady::new(REPO.to_string(), PR, head_sha.to_string())
}

fn identity_of(operation: &EnsurePullRequestReady) -> fiddle_core::EffectId {
    effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsurePullRequestReady,
        &operation.target(),
    )
}

struct World {
    dir: TempDir,
    steps: Mutex<Vec<&'static str>>,
}

impl EffectTrace for World {
    fn step(&self, _kind: EffectKind, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl World {
    fn new() -> Self {
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

    fn script_graphql(&self, n: usize, status: u16, body: serde_json::Value) {
        let dir = self.dir.path().join("graphql");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{n}.json")),
            json!({"status": status, "body": body}).to_string(),
        )
        .unwrap();
    }

    fn script_graphql_ending(&self, n: usize, mode: &str) {
        let dir = self.dir.path().join("graphql");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{n}.json")),
            json!({ "mode": mode }).to_string(),
        )
        .unwrap();
    }

    fn landed_transitions(&self) -> usize {
        self.landed_matching(|_| true)
    }

    fn transitions_whose_answer_was_lost(&self) -> usize {
        self.landed_matching(|line| line.contains(r#""mode":"commit_then_"#))
    }

    fn landed_matching(&self, keep: impl Fn(&str) -> bool) -> usize {
        std::fs::read_to_string(self.dir.path().join("world"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains("markPullRequestReadyForReview") && keep(line))
            .count()
    }

    fn graphql_argv(&self) -> Vec<String> {
        let dir = self.dir.path().join("requests");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_else(|_| Vec::new());
        files.sort();
        let mut calls = files.iter().filter_map(|file| {
            let recorded: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()?;
            let argv: Vec<String> = recorded["argv"]
                .as_array()?
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect();
            argv.iter().any(|a| a == "graphql").then_some(argv)
        });
        let argv = calls.next().expect("no GraphQL call was recorded");
        assert!(
            calls.next().is_none(),
            "more than one GraphQL call was recorded"
        );
        argv
    }

    fn graphql_field(&self, name: &str) -> String {
        let prefix = format!("{name}=");
        self.graphql_argv()
            .into_iter()
            .find_map(|arg| arg.strip_prefix(&prefix).map(str::to_string))
            .unwrap_or_else(|| panic!("no -f {name}=… was passed"))
    }

    fn ctx(&self) -> EffectContext {
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

    fn recorded_paths(&self) -> Vec<String> {
        let dir = self.dir.path().join("requests");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
            .unwrap_or_else(|_| Vec::new());
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
                        .filter_map(serde_json::Value::as_str)
                        .find(|a| a.starts_with('/'))?
                        .to_string(),
                )
            })
            .collect()
    }

    fn graphql_calls(&self) -> usize {
        std::fs::read_to_string(self.dir.path().join("graphql_calls"))
            .ok()
            .and_then(|seen| seen.trim().parse().ok())
            .unwrap_or(0)
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    async fn execute(
        &self,
        operation: EnsurePullRequestReady,
    ) -> Result<EffectReceipt<ReadyPullRequest>, EffectError> {
        self.walk(operation, false).await
    }

    async fn execute_decided(
        &self,
        operation: EnsurePullRequestReady,
    ) -> Result<EffectReceipt<ReadyPullRequest>, EffectError> {
        self.walk(operation, true).await
    }

    async fn walk(
        &self,
        operation: EnsurePullRequestReady,
        decided: bool,
    ) -> Result<EffectReceipt<ReadyPullRequest>, EffectError> {
        let ctx = self.ctx();
        let deployment = Deployment(DeploymentRule::Allow);
        let proposed = ProposedEffect {
            capability: FIXTURE_REPAIR,
            kind: EffectKind::EnsurePullRequestReady,
            target: operation.target(),
            payload: operation.payload(),
        };
        let executor = Executor::new(
            FIXTURE_REPAIR,
            PROJECT.to_string(),
            INVOCATION_REF.to_string(),
            &deployment,
            &ctx,
            self,
            ReadRetry::none(),
        );

        if !decided {
            return executor.execute(proposed, operation).await;
        }

        let effect = identity_of(&operation);
        let decision = ResolvedDecision::approved(
            DecisionBinding {
                request: decision_request_id(PROJECT, INVOCATION_REF, &effect),
                effect,
                payload: payload_hash(&operation.payload()),
                head_sha: HEAD_SHA.to_string(),
            },
            &InterpretedHumanDecision::Approve,
        )
        .expect("an approval is what becomes a ResolvedDecision");

        executor
            .execute_decided(proposed, operation, &decision)
            .await
    }
}

#[test]
fn this_is_the_first_operation_whose_own_minimum_requires_a_person() {
    assert_eq!(
        op().minimum(),
        HumanDecisionRequirement::Human,
        "making a change reviewable is not fiddle's decision to take"
    );
    assert!(matches!(
        combine(op().minimum(), DeploymentRule::Allow),
        PolicyDecision::RequireHumanDecision { .. }
    ));
}

#[tokio::test]
async fn a_run_that_reaches_policy_is_refused_for_want_of_a_person() {
    let world = World::new();
    world.pull(
        PR,
        json!({"number": PR, "draft": true, "node_id": NODE_ID, "state": "open"}),
    );

    let error = world
        .execute(op())
        .await
        .expect_err("no person has agreed to this");

    assert!(
        matches!(
            error,
            EffectError::HumanDecisionRequired {
                kind: EffectKind::EnsurePullRequestReady,
                ..
            }
        ),
        "got {error:?}"
    );
    assert_eq!(
        world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy"
        ],
        "refused at step 4, so nothing was authorized and nothing was applied"
    );
    assert_eq!(
        world.graphql_calls(),
        0,
        "a refused effect dispatches nothing"
    );
    assert_eq!(
        world.recorded_paths(),
        [format!("/repos/{REPO}/pulls/{PR}")],
        "and the one read it made was the pre-mutation look, not a second one"
    );
}

#[test]
fn the_revision_is_part_of_the_identity_and_not_only_of_the_payload() {
    let a = op_at_head("aaaa");
    let b = op_at_head("bbbb");

    assert_ne!(
        identity_of(&a),
        identity_of(&b),
        "two revisions are two effects, and therefore two questions"
    );
    assert_eq!(
        identity_of(&a),
        identity_of(&op_at_head("aaaa")),
        "and the same revision recomputes the same identity, which is what lets \
         a fresh process recognise work it really did"
    );
    assert_ne!(a.target(), b.target(), "which is achieved by the target");
    assert!(a.target().contains("acme/r#7@"), "got {}", a.target());
}

#[tokio::test]
async fn the_postcondition_read_also_yields_the_node_id_the_mutation_needs() {
    let drafting = World::new();
    drafting.pull(
        PR,
        json!({"number": PR, "draft": true, "node_id": NODE_ID, "state": "open"}),
    );

    let observed = op().inspect(&drafting.ctx()).await.unwrap();

    assert!(observed.is_none(), "a draft is not the postcondition");
    assert_eq!(
        drafting.recorded_paths(),
        [format!("/repos/{REPO}/pulls/{PR}")],
        "one read, not two"
    );

    let ready = World::new();
    ready.pull(
        PR,
        json!({"number": PR, "draft": false, "node_id": NODE_ID, "state": "open"}),
    );

    let ctx = ready.ctx();
    let observed = op()
        .inspect(&ctx)
        .await
        .unwrap()
        .expect("a pull request out of draft is the postcondition");

    assert_eq!(
        observed,
        ReadyPullRequest {
            number: PR,
            node_id: NODE_ID.to_string(),
        },
        "the node id comes back with the draft state, from the same read"
    );
    assert_eq!(ready.recorded_paths().len(), 1, "one read, not two");
}

#[tokio::test]
async fn an_already_ready_pull_request_is_the_postcondition() {
    let world = World::new();
    world.pull(
        PR,
        json!({"number": PR, "draft": false, "node_id": NODE_ID, "state": "open"}),
    );

    let receipt = world.execute(op()).await.unwrap();

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("7"),
        "and the receipt names the object a person would look up"
    );
    assert_eq!(world.graphql_calls(), 0, "nothing to do");
    assert_eq!(
        world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition"
        ],
        "settled at step 3, so policy was never asked and nothing was applied"
    );
}

#[tokio::test]
async fn a_refused_mutation_is_not_reported_as_a_lost_write() {
    let world = World::new();
    world.pull(
        PR,
        json!({"number": PR, "draft": true, "node_id": NODE_ID, "state": "open"}),
    );
    world.script_graphql(
        0,
        200,
        json!({"data": null, "errors": [{"type": "FORBIDDEN", "message": "no"}]}),
    );

    let error = world
        .execute_decided(op())
        .await
        .expect_err("a refused mutation did not make the pull request ready");

    let EffectError::Adapter { source, .. } = &error else {
        panic!("expected the adapter's refusal to stand, got {error:?}");
    };
    assert!(
        matches!(source, GhError::GraphQl { kind, .. } if kind == "FORBIDDEN"),
        "and to name what refused it, got {source:?}"
    );
    assert_eq!(
        source.outcome(),
        EffectOutcome::NotCommitted,
        "a refusal in these terms leaves no room for the write having happened"
    );

    assert!(
        !matches!(error, EffectError::Unresolved { .. }),
        "a refusal is settled; calling it unresolved would send somebody to look \
         for a write that was refused"
    );
    assert!(
        !error.to_string().contains("reported success"),
        "and the mutation's own 200 is not a success: {error}"
    );

    assert_eq!(world.graphql_calls(), 1);
    assert_eq!(
        world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
            "resolve_decision",
            "authorize",
            "apply",
            "observe_postcondition"
        ],
        "and the refusal came from the adapter, after the whole order ran"
    );
}

#[tokio::test]
async fn an_approved_ready_transition_commits() {
    let world = World::new();
    world.pull(
        PR,
        json!({"number": PR, "draft": true, "node_id": NODE_ID, "state": "open"}),
    );
    world.script_graphql(
        0,
        200,
        json!({"data": {"markPullRequestReadyForReview": {"pullRequest": {"isDraft": false}}}}),
    );

    let receipt = world
        .execute_decided(op())
        .await
        .expect("an approved transition against a draft commits");

    assert_eq!(
        receipt.outcome,
        EffectOutcome::Committed,
        "and the postcondition was read back rather than taken from the mutation's \
         own 200"
    );
    assert_eq!(
        receipt.value,
        ReadyPullRequest {
            number: PR,
            node_id: NODE_ID.to_string(),
        },
        "carrying what the second read observed"
    );
    assert_eq!(receipt.external_ref.as_deref(), Some("7"));
    assert_eq!(world.graphql_calls(), 1, "one approval, one mutation");
    assert_eq!(world.landed_transitions(), 1, "which really landed");
    assert_eq!(
        world.transitions_whose_answer_was_lost(),
        0,
        "and answered, which is what separates this case from the next one"
    );
    assert_eq!(
        world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
            "resolve_decision",
            "authorize",
            "apply",
            "observe_postcondition"
        ],
        "through the whole authorization order, and the decision resolved once"
    );
}

#[tokio::test]
async fn a_lost_answer_on_the_ready_transition_is_settled_by_reading() {
    const MUTATION: &str = "mutation($id: ID!) { markPullRequestReadyForReview(input: \
                            {pullRequestId: $id}) { pullRequest { isDraft } } }";

    let witness = World::new();
    witness.pull(
        PR,
        json!({"number": PR, "draft": true, "node_id": NODE_ID, "state": "open"}),
    );
    witness.script_graphql_ending(0, "commit_then_die");

    let lost = witness
        .ctx()
        .gh
        .graphql(MUTATION, &[("id", NODE_ID)], &CancellationToken::new())
        .await
        .expect_err("the fixture must really lose the answer, or this proves nothing");
    assert!(
        matches!(lost, GhError::Killed(_)),
        "expected a child that died without answering, got {lost:?}"
    );
    assert_eq!(
        lost.outcome(),
        EffectOutcome::Unknown,
        "and it must classify Unknown, or the executor would never go and look"
    );
    assert_eq!(
        witness.landed_transitions(),
        1,
        "and the mutation must really have landed, or the answer was all that was lost"
    );

    let world = World::new();
    world.pull(
        PR,
        json!({"number": PR, "draft": true, "node_id": NODE_ID, "state": "open"}),
    );
    world.script_graphql_ending(0, "commit_then_die");

    let receipt = world
        .execute_decided(op())
        .await
        .expect("the answer was lost, not the transition");

    assert_eq!(
        receipt.outcome,
        EffectOutcome::Committed,
        "step 8 read the pull request back and found it out of draft"
    );
    assert_eq!(
        receipt.value,
        ReadyPullRequest {
            number: PR,
            node_id: NODE_ID.to_string(),
        },
        "and the receipt names what was observed rather than what was requested"
    );
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("7"),
        "and the object a person would look up"
    );

    assert_eq!(
        world.graphql_calls(),
        1,
        "one approval buys one dispatch, and a lost answer does not buy another"
    );
    assert_eq!(
        world.landed_transitions(),
        1,
        "and it landed exactly once, so `Committed` is not covering for two writes"
    );
    assert_eq!(
        world.transitions_whose_answer_was_lost(),
        1,
        "in this very world: the one that landed is the one whose answer was lost, \
         so this is not a mutation that simply succeeded"
    );
    assert_eq!(
        world.recorded_paths(),
        [
            format!("/repos/{REPO}/pulls/{PR}"),
            format!("/repos/{REPO}/pulls/{PR}")
        ],
        "settled by looking: two reads of the same pull request around one \
         mutation, and the second is what resolved the unknown"
    );
    assert_eq!(
        world.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition",
            "combine_policy",
            "resolve_decision",
            "authorize",
            "apply",
            "observe_postcondition"
        ],
        "and the decision was resolved exactly once, before the single dispatch"
    );
}

#[tokio::test]
async fn the_mutation_the_child_received_binds_the_node_id_from_the_read() {
    let world = World::new();
    world.pull(
        PR,
        json!({"number": PR, "draft": true, "node_id": NODE_ID, "state": "open"}),
    );
    world.script_graphql(
        0,
        200,
        json!({"data": {"markPullRequestReadyForReview": {"pullRequest": {"isDraft": false}}}}),
    );

    let _ = world.execute_decided(op()).await;

    assert_eq!(world.graphql_calls(), 1, "one mutation, dispatched once");

    let query = world.graphql_field("query");
    assert!(
        query.contains("markPullRequestReadyForReview"),
        "the transition exists only as this mutation: {query}"
    );
    assert!(
        query.contains("mutation($id: ID!)"),
        "declared as taking a variable: {query}"
    );
    assert_eq!(
        world.graphql_field("id"),
        NODE_ID,
        "bound to the node id the postcondition read returned"
    );
    assert!(
        !query.contains(NODE_ID),
        "and absent from the query text, so a node id cannot rewrite the query \
         it is passed to: {query}"
    );
}

#[tokio::test]
async fn a_pull_request_this_client_cannot_read_is_not_a_verdict() {
    let world = World::new();
    world.pull(PR, json!({"number": PR, "state": "open"}));

    let error = op()
        .inspect(&world.ctx())
        .await
        .expect_err("a response missing both fields is not an answer");

    assert_eq!(error.outcome(), EffectOutcome::Unknown, "got {error:?}");
}
