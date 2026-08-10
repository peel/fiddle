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
//! and would fail here.
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
//! Four of this operation's properties are implemented and cannot be gated from
//! this file today, because every one of them needs an `apply` to run and
//! nothing in this build can execute an operation whose `minimum()` is `Human`:
//! `combine(Human, _)` is `RequireHumanDecision` unconditionally, and
//! `Executor::execute`'s step 4 turns that into `EffectError::HumanDecisionRequired`
//! and returns. `AuthorizedEffect` is unforgeable outside `crate::effect`, so
//! `apply` is reachable from `execute` and from nowhere else.
//!
//! That is a gap in the executor rather than in this operation. The RFC's step 4
//! is "combine the capability's minimum effect rule with deployment policy *and,
//! when needed, resolve a matching contextual human decision*", and the third
//! input does not exist: a resolved decision has no way to reach the executor.
//! The bean that adds it owns `crates/fiddle-runtime/src/effect/mod.rs`, and it
//! is a prerequisite for the four tests owed here:
//!
//! - the mutation is GraphQL and carries the node id from the read, with the id
//!   bound as `$id` and never spliced into the query text;
//! - the mutation is dispatched exactly once, including on the path where its
//!   answer was lost and the pull request had to be read back to settle it;
//! - a mutation refused with 200 and a `FORBIDDEN`, against a world that still
//!   shows a draft, reports the adapter's refusal rather than `Unresolved` and
//!   rather than "the adapter reported success";
//! - and, with a decision resolved, the transition commits at all.
//!
//! They are named rather than written because a version of them that passes
//! today would have to avoid the executor, and an assertion that avoids the
//! mandatory authorization boundary is an assertion about something other than
//! what this operation does.

mod support;

use fiddle_core::{
    combine, effect_id, DeploymentRule, EffectKind, HumanDecisionRequirement, PolicyDecision,
    ProposedEffect, FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    EffectContext, EffectError, EffectOutcome, EffectReceipt, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ReadRetry,
};
use fiddle_runtime::github::{EnsurePullRequestReady, ReadyPullRequest};
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

fn op() -> EnsurePullRequestReady {
    op_at_head("aaaa")
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
        let ctx = self.ctx();
        let deployment = Deployment(DeploymentRule::Allow);
        let proposed = ProposedEffect {
            capability: FIXTURE_REPAIR,
            kind: EffectKind::EnsurePullRequestReady,
            target: operation.target(),
            payload: operation.payload(),
        };
        Executor::new(
            FIXTURE_REPAIR,
            PROJECT.to_string(),
            INVOCATION_REF.to_string(),
            &deployment,
            &ctx,
            self,
            // One read and no waiting: this suite's subject is the operation,
            // not the postcondition read's budget.
            ReadRetry::none(),
        )
        .execute(proposed, operation)
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

/// The revision is in the target, so a moved head is a different effect rather
/// than the same effect with a changed request.
///
/// The spelling is asserted too, and that is not tidiness: the target is hashed
/// into the identity, so two ways of writing one target are two effects, and a
/// process recomputing it from the same three facts has to arrive at the same
/// string.
#[test]
fn the_revision_is_part_of_the_identity_and_not_only_of_the_payload() {
    let a = op_at_head("aaaa");
    let b = op_at_head("bbbb");

    assert_ne!(a.target(), b.target());
    assert_ne!(identity_of(&a), identity_of(&b));
    assert!(a.target().contains("acme/r#7@"), "got {}", a.target());
    assert_eq!(
        identity_of(&a),
        identity_of(&op_at_head("aaaa")),
        "and the same revision recomputes the same identity, which is what lets \
         a fresh process recognise work it really did"
    );
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
