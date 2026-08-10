//! `ensure_pull_request_ready`: the first operation in this build that a person
//! has to have agreed to, and the first whose identity carries a revision.
//!
//! Three questions are specific to this operation, and none of them is the
//! executor's protocol — `effect_protocol.rs` owns that against a scripted
//! operation that never reaches a process.
//!
//! **Its minimum requires a person.** M2's three operations all declare
//! `Automatic`, so `combine`'s `(Human, Allow)` cell had been asserted in
//! `policy.rs`'s own table and produced by nothing that runs. Asserting it here,
//! through the operation, makes it a fact about the build rather than about a
//! table: an edit that quietly relaxed this minimum would still pass `policy.rs`
//! and would fail here. That is a demonstration rather than a reading —
//! `minimum()` relaxed to `Automatic` fails
//! `this_is_the_first_operation_whose_own_minimum_requires_a_person` and
//! `a_run_that_reaches_policy_is_refused_for_want_of_a_person`, and nothing
//! else in the workspace notices.
//!
//! It is asserted twice, and the second is not a duplicate of the first. One
//! calls `combine` directly, which pins the declaration; the other proposes the
//! effect and lets the executor walk, which pins *where* the declaration is
//! consulted — after step 3 and before anything is authorized — and that a
//! refused walk reaches no adapter at all.
//!
//! **Its identity carries the head sha.** The target is `{repo}#{pr}@{head_sha}`,
//! so a pull request whose head has moved is a different effect with a different
//! `EffectId` — and therefore a different `DecisionRequestId`, and therefore a
//! different question. That is what makes an approval given for an earlier
//! revision unrecognisable rather than merely rejected. Put the revision in the
//! payload alone and the identity would be unchanged, so the stale approval
//! would arrive looking like an answer to the current question and be refused as
//! a payload divergence — which reads as a caller misbehaving rather than as a
//! change having moved on since somebody looked at it.
//!
//! **One read answers both of its questions.** `GET /repos/{repo}/pulls/{pr}`
//! carries `draft` and `node_id` together, which is why this operation addresses
//! that endpoint rather than the listing `EnsurePullRequest` uses.
//!
//! Everything that reaches a process runs against `tests/gh_stub/`, offline and
//! credential-free; the `git` in every context is a path that does not exist, so
//! an operation that grew a second mutation channel would fail loudly.
//!
//! # What is not asserted here yet, and why
//!
//! Two of this operation's properties are implemented and asserted nowhere, and
//! the thing they wait on is the **fixture**, not the executor.
//!
//! It was the executor until bean `fiddle-rvcu` landed
//! `Executor::execute_decided`, and four properties waited on it. An operation
//! whose `minimum()` is `Human` could not commit anything: `combine(Human, _)` is
//! `RequireHumanDecision` unconditionally, step 4 turned that into
//! `EffectError::HumanDecisionRequired` and returned, and `AuthorizedEffect` is
//! unforgeable outside `crate::effect`, so `apply` had no second route to reach.
//! Step 4 now takes the RFC's third input — *"and, when needed, resolve a
//! matching contextual human decision"* — so a walk carrying a resolved approval
//! reaches `apply`, and two of the four were written against it:
//! `a_refused_mutation_is_not_reported_as_a_lost_write` and
//! `the_mutation_the_child_received_binds_the_node_id_from_the_read`.
//!
//! What is left needs a `gh` that can **mutate and then fail**, and the scripted
//! one cannot. Its GraphQL route answers a scripted status and body and does
//! nothing else: it short-circuits ahead of the `script`/`commit_then_*`
//! machinery, which is keyed on the REST write path. So a scripted mutation can
//! neither change the world it was sent to nor die after sending. Bean
//! `fiddle-8vpm` is that route's fault injection, and it is the prerequisite for
//! the two owed here:
//!
//! - the mutation is dispatched exactly once **on the path where its answer was
//!   lost** and the pull request had to be read back to settle it, which needs a
//!   `gh` that mutates and *then* dies. The two halves of exactly-once that do
//!   not need one are asserted:
//!   `a_run_that_reaches_policy_is_refused_for_want_of_a_person` (a refused walk
//!   dispatches nothing), `an_already_ready_pull_request_is_the_postcondition`
//!   (an effect the world satisfies dispatches nothing), and one call on both of
//!   the decided walks above;
//! - and, with a decision resolved, the transition **commits at all** — a
//!   receipt whose outcome is `Committed`. That needs the post-mutation read to
//!   answer `draft: false`, where today both of a walk's reads are served from the
//!   same `pulls_by_number/{n}.json`, so the mutation cannot be made to have
//!   happened.
//!
//! Together they are what is left of `m3-ready-mutation-is-graphql-and-once`'s
//! dispatch half; `m3-refusal-is-not-a-lost-write` is asserted in full.
//!
//! **Nothing is approximated in their absence.** No weaker version that avoids
//! the executor was written, because an assertion that avoids the mandatory
//! authorization boundary is an assertion about something other than what this
//! operation does — which reads as coverage while gating nothing. The one
//! property asserted below the executor rather than through it is
//! `github::ready`'s `a_mutation_with_no_node_id_in_hand_is_not_sent`, which pins
//! the read-once handoff's own refusal — `NotSent`, so `NotCommitted` — and is
//! there because that case is unreachable through the executor *by construction*:
//! step 3 fills the cell before step 7 on every path that gets there.

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

/// The repository these cases are about, and the one the target names.
const REPO: &str = "acme/r";

/// The pull request's number. Seven rather than one, so an assertion on an
/// external reference cannot pass by accident against an index or a count.
const PR: u64 = 7;

/// The node id the scripted world holds. Shaped like GitHub's, and deliberately
/// not derivable from the number, so a client that fabricated one rather than
/// reading it would be visible.
const NODE_ID: &str = "PR_kwDOabcdef";

/// A generous bound for a stub that answers immediately. Nothing here is about
/// the deadline; `github_cli` owns the process bounds.
const PATIENT: Duration = Duration::from_secs(60);

/// The revision every case that is not about staleness runs at.
///
/// Named because the decided walk has to state it twice — once in the operation
/// and once in the binding the approval carries — and two spellings of one
/// revision would make an approval fail to match for a reason no test intended.
const HEAD_SHA: &str = "aaaa";

fn op() -> EnsurePullRequestReady {
    op_at_head(HEAD_SHA)
}

fn op_at_head(head_sha: &str) -> EnsurePullRequestReady {
    EnsurePullRequestReady::new(REPO.to_string(), PR, head_sha.to_string())
}

/// The identity a fresh process would recompute for this operation.
fn identity_of(operation: &EnsurePullRequestReady) -> fiddle_core::EffectId {
    effect_id(
        PROJECT,
        INVOCATION_REF,
        EffectKind::EnsurePullRequestReady,
        &operation.target(),
    )
}

// ---------------------------------------------------------------------------
// The world one ready transition runs against
// ---------------------------------------------------------------------------

/// The scripted `gh`'s scratch directory, and what a test needs to arrange a
/// pull request in it or read the requests back out.
///
/// The reads and the mutations are counted separately, because the two claims
/// this operation makes are about different numbers: how many times it *looked*
/// is what "one read answers both questions" is stated in, and how many times it
/// *mutated* is what "nothing to do" is stated in.
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
    /// A repository holding nothing. A read of a pull request nobody scripted is
    /// a panic in the stub naming the file, rather than a 404 answering a
    /// question the test did not ask.
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        // Empty, and stays empty: it is what a real `gh` would be pinned to, and
        // beside an absent `HOME` it is what makes an operator's keyring
        // unreachable.
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        Self {
            dir,
            steps: Mutex::new(Vec::new()),
        }
    }

    /// Put one pull request in the world, as `GET /repos/{repo}/pulls/{n}`
    /// answers it.
    ///
    /// Arranged through the stub's own seed rather than by driving the operation
    /// under test, so a world this file claims to have built is not built by the
    /// code the assertions are about.
    fn pull(&self, number: u64, body: serde_json::Value) {
        let dir = self.dir.path().join("pulls_by_number");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{number}.json")), body.to_string()).unwrap();
    }

    /// Script the answer to GraphQL call `n`, status and body separately.
    ///
    /// The two are separate arguments because for GraphQL they are separate
    /// facts: a refusal arrives as **200** with an `errors[]`, so a fixture that
    /// derived one from the other could not express the case
    /// `a_refused_mutation_is_not_reported_as_a_lost_write` is about.
    fn script_graphql(&self, n: usize, status: u16, body: serde_json::Value) {
        let dir = self.dir.path().join("graphql");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("{n}.json")),
            json!({"status": status, "body": body}).to_string(),
        )
        .unwrap();
    }

    /// The `argv` of the one GraphQL call the world received.
    ///
    /// Read out of the request record the stub writes for *every* invocation
    /// before it routes, so this is the command line the child really got rather
    /// than anything this process believes it sent. That is the whole point of
    /// asserting here as well as over
    /// [`EnsurePullRequestReady`]'s own `mutation`: one is what the operation
    /// would hand the adapter, this is what the adapter then spawned.
    ///
    /// [`World::recorded_paths`] cannot answer this, and deliberately is not
    /// changed to: it looks for an argument beginning with `/`, which a REST call
    /// has and a GraphQL call does not.
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

    /// The one `-f name=value` the mutation binds, by name.
    ///
    /// `gh` spells a GraphQL variable and a form field the same way, which is why
    /// the query itself arrives as `query=…` in this same list; ADR 018 records
    /// that `-f id=…` really does reach GitHub as a variable.
    fn graphql_field(&self, name: &str) -> String {
        let prefix = format!("{name}=");
        self.graphql_argv()
            .into_iter()
            .find_map(|arg| arg.strip_prefix(&prefix).map(str::to_string))
            .unwrap_or_else(|| panic!("no -f {name}=… was passed"))
    }

    /// A context whose `gh` is the scripted one and whose `git` cannot be run.
    fn ctx(&self) -> EffectContext {
        EffectContext::new(
            GhCli::new(
                PathBuf::from(env!("CARGO_BIN_EXE_gh_stub")),
                // The scratch directory arrives in `argv` because the adapter's
                // environment has room for exactly five names.
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

    /// Every API path the scripted `gh` was pointed at, in arrival order.
    ///
    /// Read out of the requests the stub recorded rather than counted by the
    /// operation, so this is what was really asked of the world.
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

    /// How many GraphQL calls the world received, from the counter the stub
    /// keeps — which is where it has to live, since each call is its own
    /// process.
    fn graphql_calls(&self) -> usize {
        std::fs::read_to_string(self.dir.path().join("graphql_calls"))
            .ok()
            .and_then(|seen| seen.trim().parse().ok())
            .unwrap_or(0)
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    /// Walk the authorization order for one ready transition.
    async fn execute(
        &self,
        operation: EnsurePullRequestReady,
    ) -> Result<EffectReceipt<ReadyPullRequest>, EffectError> {
        self.walk(operation, false).await
    }

    /// The same walk, with an approval in hand.
    ///
    /// This is the only way an operation whose `minimum()` is `Human` commits
    /// anything, and it is the executor's own entry point rather than a way
    /// around it: step 4 still calls `combine`, still answers
    /// `RequireHumanDecision`, and still refuses unless the decision it is handed
    /// names *this* effect and carries *this* payload digest.
    async fn execute_decided(
        &self,
        operation: EnsurePullRequestReady,
    ) -> Result<EffectReceipt<ReadyPullRequest>, EffectError> {
        self.walk(operation, true).await
    }

    /// Both entry points, so the two differ in the decision alone.
    ///
    /// Written once rather than twice because a second copy could drift into
    /// proposing a different effect, and then a test about the decided path
    /// would be about two changes rather than one.
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
            // One read and no waiting: this suite's subject is the operation,
            // not the postcondition read's budget.
            ReadRetry::none(),
        );

        if !decided {
            return executor.execute(proposed, operation).await;
        }

        // The approval a person gave, recomputed from canonical inputs the way
        // the continuation that read it back out of a comment would have to.
        // Nothing here is a shortcut past step 4's comparisons — the identity and
        // the digest are derived from the same three facts the executor derives
        // them from, so an operation built at another revision would not match.
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

// ---------------------------------------------------------------------------
// A person has to have agreed, and to what revision
// ---------------------------------------------------------------------------

/// The whole reason `combine` was written, reached by something that runs.
///
/// Both halves are asserted rather than only the declaration. The minimum on its
/// own is a value; what makes it a gate is that a deployment saying `Allow` —
/// the most permissive thing a document can say — still produces a decision that
/// has to go to a person.
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

/// The same gate, on the path a run actually takes.
///
/// The case above asserts `combine` directly, which is one function call away
/// from being another unit test of the policy table. This one proposes the
/// effect and lets the executor walk, so what is asserted is that the minimum is
/// consulted *where a run consults it* — after the postcondition inspection,
/// before anything is authorized — and that the refusal a run gets back is the
/// one that says a person is owed a question rather than one that says a rule
/// forbade it.
///
/// The step trace is what pins the position, and the trace stopping at
/// `combine_policy` is what makes `graphql_calls() == 0` mean something: not
/// "nothing happened to mutate" but "the walk was refused, and a refused walk
/// dispatches nothing". That is the half of exactly-once that is reachable
/// today — the other half, one dispatch on the paths that do commit, is owed
/// and named in this file's header.
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

/// The revision is in the target, so a moved head is a different effect rather
/// than the same effect with a changed request.
///
/// The spelling is asserted too, and that is not tidiness: the target is hashed
/// into the identity, so two ways of writing one target are two effects, and a
/// process recomputing it from the same three facts has to arrive at the same
/// string.
///
/// **The identity is asserted before the target, and the order is deliberate.**
/// Two `EffectId`s is what the property *is*; a target carrying `@{head_sha}` is
/// only how it is achieved. Asserted the other way round — as this was — dropping
/// `@{head_sha}` from `pull_request_ready_target` failed on the target strings
/// and the identity comparison below was never evaluated, so the diagnostic
/// named the mechanism and the criterion's own claim went untested at the moment
/// it broke. Measured: under that inversion the two identities collapse to one
/// digest, and this line is what says so.
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

// ---------------------------------------------------------------------------
// One read, and what it settles
// ---------------------------------------------------------------------------

/// One read answers both questions the operation has: is it still a draft, and
/// what is the node id the mutation needs.
///
/// Stated over both of the read's outcomes, because the claim is about the
/// *read* rather than about one of its answers. A draft is not the
/// postcondition; a pull request that is out of draft is, and the state it
/// returns carries the node id — so `apply` has its input from the read the
/// executor was going to make anyway, and never from a call of its own.
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

/// Already ready is the postcondition, so step 3 returns it and nothing mutates.
///
/// The step trace is the load-bearing assertion, and it is also why this case is
/// reachable while the four in the module header are not: step 3 runs *before*
/// step 4, so an effect the world already satisfies is never put to policy. The
/// `Human` minimum therefore cannot refuse a transition that has already
/// happened — which is the correct behaviour and not an accident of ordering.
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

// ---------------------------------------------------------------------------
// What a person's approval actually spends
// ---------------------------------------------------------------------------

/// A mutation GitHub refused is a refusal, and never a write whose answer was
/// lost.
///
/// The distinction is the whole of M2 and this is the one operation that can get
/// it wrong in a new way, because a refused GraphQL mutation arrives as **200
/// with an `errors[]`** rather than as a status a transport check would notice.
/// A client that read the status line would see success; the world still shows a
/// draft, so step 8's `Ok(None)` would then be reported either as "the adapter
/// reported success and the postcondition was not observed" — which sends
/// somebody to investigate a settled failure — or as `Unresolved`, which is worse
/// still: it says the write *may* have landed, and the next run resolves that by
/// looking rather than by asking why it was refused.
///
/// So the absences are asserted, not only the presence. `Adapter` is the right
/// answer, but a test that checked only for it would pass on a regression into a
/// neighbouring variant carrying the same word, and the two things this must
/// never be are the two the criterion names.
#[tokio::test]
async fn a_refused_mutation_is_not_reported_as_a_lost_write() {
    let world = World::new();
    world.pull(
        PR,
        json!({"number": PR, "draft": true, "node_id": NODE_ID, "state": "open"}),
    );
    // 200, because that is what GitHub answers. The refusal is in the body.
    world.script_graphql(
        0,
        200,
        json!({"data": null, "errors": [{"type": "FORBIDDEN", "message": "no"}]}),
    );

    let error = world
        .execute_decided(op())
        .await
        .expect_err("a refused mutation did not make the pull request ready");

    // What it is: the adapter's own refusal, carried out of the body.
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

    // What it is not, which is the half the criterion exists for.
    assert!(
        !matches!(error, EffectError::Unresolved { .. }),
        "a refusal is settled; calling it unresolved would send somebody to look \
         for a write that was refused"
    );
    assert!(
        !error.to_string().contains("reported success"),
        "and the mutation's own 200 is not a success: {error}"
    );

    // Once, on the refused path too. The executor retries the read and never the
    // write, and a refusal is not the exception to that.
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

/// The mutation the child was really spawned with: GraphQL, the node id from the
/// read, bound as a variable.
///
/// `github::ready`'s `the_mutation_binds_its_input_rather_than_spelling_it` makes
/// the same claim about the pair `apply` hands the adapter. This one makes it
/// about the `argv` the process received, which is a different claim: everything
/// between the two — `GhCli::graphql`'s `-f query=…`, `-f id=…` — is code that
/// could splice, and the whole reason the id travels as a variable is that it is
/// a value GitHub chose and interpolation would let it rewrite the query it
/// appears in.
///
/// **The walk's ending is not this test's subject.** The world still shows a
/// draft after the mutation, so it ends `Unresolved` — correctly, because a
/// scripted `gh` that says `data` and changes nothing is exactly the case step 8
/// exists to disbelieve. The request is recorded before any of that is decided,
/// which is what makes the assertion available without a world the fixture cannot
/// yet build; see this file's header on `fiddle-8vpm`.
#[tokio::test]
async fn the_mutation_the_child_received_binds_the_node_id_from_the_read() {
    let world = World::new();
    world.pull(
        PR,
        json!({"number": PR, "draft": true, "node_id": NODE_ID, "state": "open"}),
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

/// A read that came back without the fields this operation needs is not an
/// absence of a draft.
///
/// The direction matters. `Ok(None)` here would mean "still a draft", which
/// mutates; `Ok(Some(_))` would mean "already ready", which reports work nobody
/// did. Neither is a thing to conclude from an answer that could not be read, so
/// both are refused — and the refusal is `Unknown`, which costs a second look
/// rather than a second write.
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
