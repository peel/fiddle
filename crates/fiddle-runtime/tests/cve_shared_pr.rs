//! `ensure_pull_request_body`: the effect whose object never changes, so its
//! *content* has to enter its identity.
//!
//! Every other effect in this build acts on something a repeat run names
//! identically and correctly so — a branch, a head-and-base pair, a pull request
//! at a revision. Their identities are stable across runs by construction, and
//! the payload is where a changed request becomes visible without becoming a
//! second object.
//!
//! A body update has no such object. The CVE capability keeps **one** shared pull
//! request and rewrites its body as the run learns more, so the pull request it
//! addresses on run two is the pull request it addressed on run one, and `cve` is
//! a stable invocation ref. [`effect_id`] derives from `(project,
//! invocation_ref, kind, target)` and **never** the payload — that is
//! `fiddle-core`'s central rule and it is right — so a target of repository and
//! number alone would give "covers 1 CVE" and "covers 3 CVEs" one identity.
//!
//! # The failure this file exists to prevent is a silence
//!
//! That is what makes it worth a suite of its own. The defect does not raise
//! anything: run two derives the identity run one already spent, the executor's
//! step 3 finds a postcondition it believes satisfied, no mutation is dispatched,
//! no error is returned, and the run reports success against a body that still
//! describes one advisory when three were found. Nobody is told.
//!
//! So the assertions below are stated in a shape a silence cannot satisfy.
//! `a_changed_body_is_a_new_effect_and_applies` compares two identities and then
//! demands the second one *land*, against a world the first one already changed;
//! `an_unchanged_body_is_idempotent` demands the opposite of the same machinery
//! and counts the writes that actually reached the forge. Removing the digest
//! from the target makes the first of them fail on its `assert_ne!`, which is the
//! criterion's requirement: a named test fails, rather than a run quietly doing
//! nothing.
//!
//! # And the constraint that is not about bodies at all
//!
//! `no_comment_edit_path_exists` is here because this is the bean that adds a
//! *content-addressed rewrite* to the build, and a comment is the other thing in
//! this system that has content somebody might want rewritten. M3's
//! `DecisionError::RequestEdited` refuses a request comment whose timestamps
//! disagree, and it is entitled to because nothing in this workspace can edit a
//! comment — the refusal has no other ground to stand on. A bean that added one
//! would have broken M3 to build M4, so the absence is asserted over the whole
//! workspace rather than remembered.
//!
//! Everything runs against `tests/gh_stub/`, whose world is stateful: the body a
//! read answers with is the seed brought up to date with the writes that really
//! landed, so "the second run applied" is a claim about the world rather than
//! about what a fixture was told to say. Offline and credential-free throughout;
//! the `git` in every context is a path that does not exist.

mod support;

use fiddle_core::{
    content_digest, effect_id, EffectId, EffectKind, ProposedEffect, FIXTURE_REPAIR,
};
use fiddle_runtime::effect::{
    EffectContext, EffectOutcome, EffectTrace, ExecutionStep, Executor, IntegrationOperation,
    ReadRetry,
};
use fiddle_runtime::github::{pull_request_body_target, EnsurePullRequestBody};
use fiddle_runtime::GhCli;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// The repository the scripted `gh` answers for.
const REPO: &str = "peel/r";

/// The shared pull request's number. The stub numbers from 7 rather than 1, so an
/// assertion on an external reference cannot pass by accident against an index.
const PR: u64 = 7;

/// The body the world already holds before any run in this file starts.
///
/// Deliberately not a body anything here proposes, so a test that passed by
/// finding the seed would be visible.
const SEEDED_BODY: &str = "opened by fiddle, contents to follow";

/// A generous bound for children that answer immediately. Nothing here is about
/// the deadline; `github_cli` owns the process bounds.
const PATIENT: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// The world one body update runs against
// ---------------------------------------------------------------------------

/// The scripted `gh`'s scratch directory, and everything a test needs to arrange
/// a shared pull request in it or read one back out.
///
/// **This is not Task 17's `forge()`.** The shared fixture's per-task list assigns
/// that name to the task that brings the CVE capability's forge and its
/// `scripted_gh_*` builders, and it has not run. Rather than squat on the name
/// with something narrower than what Task 17 needs, this suite keeps its own
/// world — modelled on `pull_request_effect.rs`'s `Forge`, which is the shape a
/// single-operation suite in this crate already uses. Task 17 inherits nothing
/// from here beyond the stub routes below, which are additive.
struct Forge {
    dir: TempDir,
    steps: Mutex<Vec<&'static str>>,
}

impl EffectTrace for Forge {
    fn step(&self, _kind: EffectKind, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
    }
}

impl Forge {
    /// A world holding one open pull request whose body is [`SEEDED_BODY`].
    ///
    /// Arranged through the stub's own by-number file rather than by driving the
    /// operation under test, so the world these tests make claims about is not
    /// built by the code the claims are about.
    fn holding_the_shared_pull_request() -> Self {
        let dir = TempDir::new().unwrap();
        // Empty, and stays empty: it is what a real `gh` would be pinned to, and
        // it is what makes the operator's keyring unreachable.
        std::fs::create_dir_all(dir.path().join("config")).unwrap();

        let by_number = dir.path().join("pulls_by_number");
        std::fs::create_dir_all(&by_number).unwrap();
        std::fs::write(
            by_number.join(format!("{PR}.json")),
            serde_json::json!({
                "number": PR,
                "state": "open",
                "title": "fiddle: mitigate reported advisories",
                "body": SEEDED_BODY,
                // Carried because the by-number route answers the same object
                // `EnsurePullRequestReady` reads, and a fixture that dropped the
                // fields of a *neighbouring* operation would be a world neither
                // of them could share.
                "draft": false,
                "node_id": "PR_kwDOshared",
            })
            .to_string(),
        )
        .unwrap();

        Self {
            dir,
            steps: Mutex::new(Vec::new()),
        }
    }

    /// A context whose `gh` is the scripted one and whose `git` cannot be run.
    fn context(&self) -> EffectContext {
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

    /// Every request the scripted `gh` recorded, in arrival order.
    fn requests(&self) -> Vec<Vec<String>> {
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
                        .filter_map(|a| a.as_str().map(str::to_string))
                        .collect(),
                )
            })
            .collect()
    }

    /// How many body rewrites were *dispatched* at the shared pull request.
    ///
    /// Counted off the requests the stub recorded — what really left this process
    /// — rather than off the world log, which holds only what landed. The
    /// distinction is the whole of the idempotence claim: a second run that
    /// dispatched a `PATCH` and had it accepted as a no-change would leave the
    /// world identical and this count at two, and "the postcondition was already
    /// satisfied" is a claim that no second request was made at all.
    fn body_writes(&self) -> usize {
        self.requests()
            .iter()
            .filter(|argv| {
                let method = argv
                    .iter()
                    .position(|a| a == "--method")
                    .and_then(|at| argv.get(at + 1));
                method.map(String::as_str) == Some("PATCH")
                    && argv
                        .iter()
                        .any(|a| a == &format!("/repos/{REPO}/pulls/{PR}"))
            })
            .count()
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }
}

/// What one walk of the authorization order over a body update produced.
///
/// `applied` is read off the **step trace** rather than inferred from the
/// outcome. Both a run that rewrote the body and a run that found it already
/// correct answer `Committed` — that is the point of a postcondition — so an
/// assertion over the outcome could not tell the two apart, and the question this
/// suite asks is precisely which of them happened.
struct BodyUpdate {
    effect_id: EffectId,
    applied: bool,
    /// The body the executor's step 8 read back out of the world.
    observed: String,
}

/// Walk the authorization order for one body update.
async fn update_body(forge: &Forge, body: &str) -> BodyUpdate {
    let operation = EnsurePullRequestBody::new(REPO.to_string(), PR, body.to_string());
    let target = operation.target();
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectKind::EnsurePullRequestBody,
        target: target.clone(),
        payload: operation.payload(),
    };

    let before = forge.steps().len();
    let deployment = Deployment(fiddle_core::DeploymentRule::Allow);
    let ctx = forge.context();
    let receipt = Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        forge,
        // One read and no waiting: this suite's subject is the identity and the
        // postcondition, not the read's budget.
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
    .expect("a body update against a pull request the world holds");

    assert_eq!(
        receipt.outcome,
        EffectOutcome::Committed,
        "every walk in this file is expected to conclude; only *how* differs"
    );
    BodyUpdate {
        // Recomputed here from the same four canonical inputs a fresh process
        // would use, rather than taken off the receipt. The receipt carries the
        // identity the executor derived, and asserting against it would compare
        // the executor with itself.
        effect_id: effect_id(
            PROJECT,
            INVOCATION_REF,
            EffectKind::EnsurePullRequestBody,
            &target,
        ),
        applied: forge.steps()[before..].contains(&ExecutionStep::Apply.as_str()),
        observed: receipt.value.body,
    }
}

// ---------------------------------------------------------------------------
// The digest in the target
// ---------------------------------------------------------------------------

/// The defect this bean exists to have prevented, stated against one world.
///
/// **One forge and not two.** The obvious version of this test builds a fresh
/// world for each body, and it would pass with the digest deleted: against a
/// world whose body is still the seed, *any* proposed body applies, so
/// `applied` would be true for a reason that has nothing to do with identity.
/// Run two is the case, so run two is what is run — the second update meets a
/// world the first one already changed, which is exactly the shape the
/// silent no-op hides in.
///
/// Both halves are load-bearing. The `assert_ne!` is what fails when the digest
/// leaves the target, and it fails loudly rather than leaving a run doing nothing.
/// The `applied` half is what says the operation still *works*: an identity that
/// moved with the content but a postcondition that ignored it would be a second
/// effect that immediately declared itself already done.
#[tokio::test]
async fn a_changed_body_is_a_new_effect_and_applies() {
    let forge = Forge::holding_the_shared_pull_request();

    let one = update_body(&forge, "covers 1 CVE").await;
    let three = update_body(&forge, "covers 3 CVEs").await;

    assert_ne!(
        one.effect_id, three.effect_id,
        "a changed body is a different effect, or run two spends run one's identity"
    );
    assert!(
        three.applied,
        "and it applies against a world run one already wrote to; steps were {:?}",
        forge.steps()
    );
    assert_eq!(
        three.observed, "covers 3 CVEs",
        "read back out of the world, so the rewrite is observed rather than reported"
    );
    assert_eq!(forge.body_writes(), 2, "two different bodies, two writes");
}

/// The other direction of the same machinery, and the one a content-addressed
/// identity must not cost.
///
/// An effect that re-derived a new identity for an unchanged body would rewrite
/// the pull request on every run — noise in a reviewer's timeline, and a
/// mutation this system would be making for no reason. So the identity is
/// asserted *equal*, the apply is asserted absent, and the write count is
/// asserted at one.
///
/// The count is the assertion that a satisfied postcondition could not fake. A
/// `PATCH` writing the body it already had would leave the world identical and
/// the observed value identical, and only the request count would say it
/// happened.
#[tokio::test]
async fn an_unchanged_body_is_idempotent() {
    let forge = Forge::holding_the_shared_pull_request();

    let first = update_body(&forge, "covers 1 CVE").await;
    let again = update_body(&forge, "covers 1 CVE").await;

    assert_eq!(
        first.effect_id, again.effect_id,
        "the same body against the same pull request is the same effect"
    );
    assert!(first.applied, "the first run had work to do");
    assert!(
        !again.applied,
        "and the second found the postcondition already satisfied; steps were {:?}",
        forge.steps()
    );
    assert_eq!(again.observed, "covers 1 CVE");
    assert_eq!(forge.body_writes(), 1, "one write, not two");
}

/// The inversion, named so it is reproducible: delete the digest from
/// [`pull_request_body_target`] and this fails, and so does
/// `a_changed_body_is_a_new_effect_and_applies`.
///
/// Three claims rather than one, because the first two alone do not distinguish
/// a digest from the body spliced into the target whole. `format!("{repo}#{pr}@{body}")`
/// also moves with its content and is also recomputable — and it would put
/// unbounded prose somebody wrote into a string that is hashed into an identity
/// and printed in a receipt. The third claim is what rules it out.
#[test]
fn the_inversion_of_removing_the_digest_fails_this_test() {
    assert!(digest_is_part_of_target(EffectKind::EnsurePullRequestBody));
}

/// Whether this kind's canonical target really carries a digest of its content.
///
/// Computed from the target function rather than looked up in a table: a helper
/// that answered from a hand-written list would prove only that somebody
/// remembered to write the kind down.
///
/// The match is exhaustive with no wildcard, for [`EffectKind::as_str`]'s reason.
/// A wildcard would let the next kind whose object is stable across runs — and
/// there will be one — fall through to `false` without anybody being asked.
fn digest_is_part_of_target(kind: EffectKind) -> bool {
    match kind {
        EffectKind::EnsurePullRequestBody => {
            let short = pull_request_body_target(REPO, PR, "covers 1 CVE");
            let other = pull_request_body_target(REPO, PR, "covers 3 CVEs");
            let long = pull_request_body_target(REPO, PR, &"covers 3 CVEs. ".repeat(500));

            // It moves with the content, or two bodies are one effect.
            short != other
                // It is recomputable, or a fresh process derives an identity for
                // work it really did and fails to recognise it.
                && short == pull_request_body_target(REPO, PR, "covers 1 CVE")
                // And it is a *digest*: bounded, and not the prose itself.
                && !short.contains("covers")
                && long.len() == other.len()
        }
        // Every other kind acts on an object a repeat run names identically, so
        // its target is stable by construction and carries no content at all.
        EffectKind::EnsureBranchPublished
        | EffectKind::EnsurePullRequest
        | EffectKind::EnsureCheckRequested
        | EffectKind::PublishDecisionRequest
        | EffectKind::EnsurePullRequestReady => false,
    }
}

/// The digest in the target is the one [`fiddle_core`] publishes, rather than an
/// arithmetic of this module's own.
///
/// It matters because the target is recomputed by a *later build*: a second
/// definition of "the digest of a body" could drift from this one under an edit,
/// and the run that noticed would be the one that opened a second rewrite of a
/// pull request it had already rewritten correctly.
#[test]
fn the_target_names_the_repository_the_number_and_the_published_digest() {
    let target = pull_request_body_target(REPO, PR, "covers 1 CVE");

    assert!(target.contains(REPO), "{target}");
    assert!(target.contains(&PR.to_string()), "{target}");
    assert!(
        target.contains(&content_digest("covers 1 CVE")),
        "the target must carry fiddle_core's digest and not a second one: {target}"
    );
}

// ---------------------------------------------------------------------------
// The constraint M3 depends on: nothing here can edit a comment
// ---------------------------------------------------------------------------

/// **No path in this workspace can change a comment that already exists.**
///
/// The absence is load-bearing rather than incidental, and
/// `DecisionError::RequestEdited` is where that is written down —
/// *"fiddle's own question has been edited, which fiddle has no path that does"*.
/// It refuses a request comment whose `created_at` and `updated_at` disagree, and
/// it is entitled to read that as tampering only because fiddle itself cannot be
/// the editor. A bean that added an edit path would have broken M3 to build M4,
/// silently — the refusal would keep firing and would simply stop meaning what it
/// says.
///
/// The epic's Hard Constraints say `docs/technical/SYSTEM.md` records this. **It
/// does not** — the constraint is stated on the error variant and nowhere in that
/// document, which is why this lane names the variant instead. Recorded here so
/// the next reader does not go looking for a paragraph that was never written.
///
/// Stated over the whole workspace rather than over this milestone's own files,
/// like `cve_protocol::nothing_in_this_workspace_decides_on_claimed_complete`,
/// and carrying premises of its own for the same reason: a walk that found
/// nothing because it was looking in the wrong place, or because its resolution
/// of a path expression had quietly stopped working, would assert nothing at all.
#[test]
fn no_comment_edit_path_exists() {
    // The cheap half, and a real one: the closed set of effect kinds is what a
    // deployment document gates and what an identity is derived over, so a
    // comment-editing effect would have to be spelled here first.
    assert!(
        EffectKind::ALL
            .iter()
            .all(|kind| !kind.as_str().contains("comment")),
        "an effect kind names a comment: {:?}",
        EffectKind::ALL.map(|kind| kind.as_str())
    );

    let scan = scan_for_comment_dispatches();

    // Premise one. A walk that saw no dispatches at all was looking in the wrong
    // place, and every classification below it would be a claim about nothing.
    assert!(
        scan.dispatches > 0,
        "no `.api(` dispatch was found under any crate's src, so this lane is \
         looking in the wrong place"
    );
    // Premise two, and the load-bearing one. The classifier resolves a path
    // expression through the `let` bindings and helper functions it is built
    // from — `ctx.gh.api("POST", &path, …)` where `let path =
    // request.comments_path();`. If that resolution stops working, every
    // dispatch reads as reaching no comment, `edits` is empty, and the assertion
    // below passes while proving nothing.
    assert!(
        !scan.reaching.is_empty(),
        "no dispatch was resolved to a comment path, so the resolution this lane \
         depends on has stopped working"
    );
    // Premise three, and the one this lane learned the hard way. The GraphQL rule
    // matches on the *query text*, which is a module-level `const` at every call
    // site in this build — so it is entirely dependent on the resolver following
    // a `const`, and until it did, a probe that added `updateIssueComment` went
    // undetected while the REST half kept the lane green. This is what says the
    // GraphQL half is running over a query rather than over a bare identifier.
    assert!(
        scan.graphql_mutations > 0,
        "no `.graphql(` call resolved to a query naming a mutation, so the rule \
         that would catch `updateIssueComment` is matching against nothing"
    );
    // Premise four: the allowlist is *exercised* rather than merely unviolated.
    // What it permits is the decision request's create — a `POST` onto the
    // conversation collection, which addresses no comment that already exists —
    // and a lane whose allowlist matched nothing would pass whether or not the
    // rule it encodes was right.
    assert!(
        !scan.allowed.is_empty(),
        "the allowlist matched nothing, so it was never tested: {:#?}",
        scan.reaching
    );

    assert!(
        scan.edits.is_empty(),
        "these reach a comment that already exists with something other than a \
         read, and `DecisionError::RequestEdited` depends on none of them \
         existing:\n{}",
        scan.edits.join("\n")
    );
}

/// What one walk of the workspace's sources found.
struct CommentScan {
    /// Every `.api(` and `.graphql(` call site seen, comment-related or not.
    dispatches: usize,
    /// `.graphql(` call sites whose query resolved to text naming a mutation.
    ///
    /// Separate from [`CommentScan::dispatches`] because it is the GraphQL half's
    /// own non-vacuity witness. A query is a module-level `const`, so the whole
    /// rule depends on the resolver following one — and when it did not, the rule
    /// silently matched nothing while the REST half kept the lane green.
    graphql_mutations: usize,
    /// Those whose resolved path or query names a comment.
    reaching: Vec<String>,
    /// Of those, the ones the rule permits: a read, or the one create.
    allowed: Vec<String>,
    /// Of those, the ones that would change a comment that already exists.
    edits: Vec<String>,
}

/// Walk every crate's `src` and classify every request this build can dispatch.
///
/// `src` only, like the workspace negative in `cve_protocol.rs`: a test may name
/// a comment endpoint freely — `gh_stub` serves two of them — and what the
/// criterion is about is the product.
///
/// Two routes are searched because the build has two. REST goes through
/// [`GhCli::api`](fiddle_runtime::GhCli), whose first argument is the verb and
/// whose second is the path; GraphQL goes through `GhCli::graphql`, whose verdict
/// and whose subject both live in the query text. A lane that searched only the
/// first would miss `updateIssueComment`, which is how GitHub actually spells the
/// thing this constraint forbids.
fn scan_for_comment_dispatches() -> CommentScan {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("this crate lives under the workspace's crates directory");

    let mut scan = CommentScan {
        dispatches: 0,
        graphql_mutations: 0,
        reaching: Vec::new(),
        allowed: Vec::new(),
        edits: Vec::new(),
    };

    for file in rust_sources(crates) {
        let text = std::fs::read_to_string(&file).expect("a source file of this workspace");
        // Collapsed to one line, because an argument list is not. rustfmt breaks
        // `.api(\n  "POST",\n  &format!(…),\n …)` across four lines whenever it
        // is long enough, which is exactly the case a line-at-a-time scan would
        // read as a dispatch with no path.
        let flat = collapse(&text);
        let defined = definitions(&flat);
        let at = file.strip_prefix(crates).unwrap_or(&file).display();

        for (call, args) in calls(&flat, ".api(") {
            scan.dispatches += 1;
            let verb = literal(args.first().map(String::as_str).unwrap_or_default());
            let path = expanded(
                &defined,
                args.get(1).map(String::as_str).unwrap_or_default(),
            );
            if !path.contains("/comments") {
                continue;
            }
            let where_ = format!("{at}: {verb} {call}");
            scan.reaching.push(where_.clone());
            match permitted(&verb, &path) {
                true => scan.allowed.push(where_),
                false => scan.edits.push(where_),
            }
        }

        for (call, args) in calls(&flat, ".graphql(") {
            scan.dispatches += 1;
            let query = expanded(
                &defined,
                args.first().map(String::as_str).unwrap_or_default(),
            );
            if query.contains("mutation") {
                scan.graphql_mutations += 1;
            }
            // A GraphQL field naming a comment type, in camelCase as GraphQL
            // spells one: `updateIssueComment`, `deleteIssueComment`,
            // `minimizeComment`. Every one of them is a mutation — there is no
            // read this build needs from here, because the conversation is read
            // over REST — so any of them reaching this route is a finding rather
            // than something to classify further.
            if query.contains("Comment") {
                let where_ = format!("{at}: graphql {call}");
                scan.reaching.push(where_.clone());
                scan.edits.push(where_);
            }
        }
    }

    scan
}

/// Whether a request that reaches a comment path is one this constraint permits.
///
/// Two arms and no third. A `GET` reads, and reading the conversation is the
/// whole of how a decision arrives — `read_conversation` and `read_one_comment`
/// are both here. A `POST` to the *collection* creates a comment that did not
/// exist, which is how a question gets asked, and it addresses nothing that was
/// already there.
///
/// The collection is told from a member by where the number sits, which is
/// GitHub's own distinction: `/issues/{pr}/comments` ends at the collection, and
/// `/issues/comments/{id}` names one comment. So a literal ending `/comments"` is
/// a collection, and one containing `/comments/` is a member — and a resolved
/// path that reaches both shapes is refused rather than guessed at, because a
/// helper serving two endpoints cannot be judged from one verb.
fn permitted(verb: &str, path: &str) -> bool {
    let collection = path.contains("/comments\"") || path.contains("/comments?");
    let member = path.contains("/comments/");
    match verb {
        "GET" => true,
        "POST" => collection && !member,
        _ => false,
    }
}

/// Every `.rs` file under each crate's `src`, tests excluded.
fn rust_sources(crates: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending: Vec<PathBuf> = std::fs::read_dir(crates)
        .expect("the workspace's crates directory is readable")
        .flatten()
        .map(|entry| entry.path().join("src"))
        .filter(|src| src.is_dir())
        .collect();
    assert!(
        !pending.is_empty(),
        "no crate under {} has a src directory",
        crates.display()
    );
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|why| panic!("{} is readable: {why}", dir.display()))
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }
    found
}

/// The file as one line, with runs of whitespace squeezed to a single space and
/// `//` comment lines dropped.
///
/// Comments go because this file's own module documentation names
/// `updateIssueComment`, and a scan that read prose would report the warning
/// against itself. Only whole comment lines are dropped, which is the shape a
/// doc comment takes; a trailing `// …` after code keeps its code.
fn collapse(text: &str) -> String {
    let mut flat = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            continue;
        }
        flat.push_str(trimmed);
        flat.push(' ');
    }
    flat
}

/// Every call to `marker` in the collapsed text, as the source spells it and
/// split into its top-level arguments.
///
/// Arguments are split on commas at paren depth zero, so
/// `&format!("/repos/{}/pulls", self.repo)` stays one argument rather than
/// becoming two — which is the whole reason this is a scan rather than a
/// `split(',')`.
fn calls(flat: &str, marker: &str) -> Vec<(String, Vec<String>)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = flat[from..].find(marker) {
        let open = from + at + marker.len() - 1;
        from = open + 1;
        let Some(close) = matching_paren(flat, open) else {
            continue;
        };
        let inside = &flat[open + 1..close];
        found.push((
            format!("{marker}{inside})"),
            split_top_level(inside)
                .into_iter()
                .map(str::to_string)
                .collect(),
        ));
    }
    found
}

/// The index of the `)` closing the `(` at `open`, counting nesting and skipping
/// anything inside a string literal.
///
/// The literal skip is not fussiness: an API path is a string, and
/// `"/repos/{}/pulls?head=(x)"` would otherwise close a paren that was never
/// opened and truncate the argument list at the character before the path.
fn matching_paren(flat: &str, open: usize) -> Option<usize> {
    let bytes = flat.as_bytes();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (i, byte) in bytes.iter().enumerate().skip(open) {
        match (in_string, escaped, byte) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') => in_string = false,
            (true, false, _) => {}
            (false, _, b'"') => in_string = true,
            (false, _, b'(') => depth += 1,
            (false, _, b')') => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split an argument list on the commas that are not inside a nested call, a
/// bracket or a string.
fn split_top_level(inside: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start, mut in_string, mut escaped) = (0i32, 0usize, false, false);
    for (i, byte) in inside.bytes().enumerate() {
        match (in_string, escaped, byte) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') => in_string = false,
            (true, false, _) => {}
            (false, _, b'"') => in_string = true,
            (false, _, b'(' | b'[' | b'{') => depth += 1,
            (false, _, b')' | b']' | b'}') => depth -= 1,
            (false, _, b',') if depth == 0 => {
                parts.push(inside[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(inside[start..].trim());
    parts
}

/// The contents of the first string literal in `expr`, or the expression itself.
fn literal(expr: &str) -> String {
    match (expr.find('"'), expr.rfind('"')) {
        (Some(open), Some(close)) if close > open => expr[open + 1..close].to_string(),
        _ => expr.to_string(),
    }
}

/// The expression, plus the text of everything in this file it is built from.
///
/// This is the resolution the lane's second premise is about, and it is why the
/// lane is a search rather than a list. A dispatch almost never carries its path
/// inline: `human/mod.rs` writes `let path = request.comments_path();` and then
/// `ctx.gh.api("POST", &path, …)`, so the argument is the single word `&path` and
/// the endpoint is two hops away. Every identifier in the expression is looked up
/// among this file's `let` bindings and function bodies, and whatever they expand
/// to is looked up in turn, to a fixed point.
///
/// Bounded at four rounds, which is one more than the deepest chain this build
/// has. A bound rather than a true fixed point because the lookup is textual and
/// a self-referential binding — `let path = format!("{path}/comments")` — would
/// otherwise not terminate.
fn expanded(defined: &Definitions, expr: &str) -> String {
    let mut text = expr.to_string();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for _ in 0..4 {
        let mut grew = false;
        for name in identifiers(&text) {
            if !seen.insert(name.clone()) {
                continue;
            }
            if let Some(body) = defined.get(&name) {
                text.push(' ');
                text.push_str(body);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    text
}

/// Every name a file binds, mapped to the text it is bound to.
type Definitions = std::collections::BTreeMap<String, String>;

/// Index one file's bindings, so a dispatch's argument can be resolved to the
/// endpoint it stands for.
///
/// **Four keywords, because the build uses all four.** A REST path is a `let` or
/// a `fn`; a GraphQL query is a module-level `const`, which is how
/// `READY_FOR_REVIEW` is written. A resolver that knew only the first two read
/// every `.graphql(` call as a bare identifier and found no mutation anywhere —
/// the lane stayed green through a probe that added `updateIssueComment`, and the
/// premise on [`CommentScan::graphql_mutations`] is what refuses to let that
/// happen again.
///
/// **A `let` binds a pattern and not a name.** `ready.rs` writes `let (query,
/// variables) = self.mutation()?;`, so a lookup for `let query` finds nothing;
/// every identifier in the pattern is bound to the whole right-hand side here
/// instead. That was the second half of the same defect.
///
/// **A name bound twice keeps both.** `comments.rs` has a `let path` in each of
/// its two readers, one naming the conversation collection and one naming a
/// comment by id, and a map that let the later one win would answer for the
/// wrong endpoint. Concatenating is the conservative direction for a lane about a
/// *forbidden* path: an over-approximation can report something to look at, and
/// only an under-approximation can miss one.
fn definitions(flat: &str) -> Definitions {
    let mut defined = Definitions::new();
    let mut bind = |name: &str, body: &str| {
        defined
            .entry(name.to_string())
            .or_default()
            .push_str(&format!(" {body}"));
    };

    for (at, _) in flat.match_indices("fn ") {
        let rest = &flat[at + 3..];
        let Some(open) = rest.find('(') else { continue };
        let name = rest[..open].trim();
        if !is_identifier(name) {
            continue;
        }
        if let Some(body) = rest
            .find('{')
            .and_then(|brace| matching_brace(rest, brace).map(|close| &rest[brace..=close]))
        {
            bind(name, body);
        }
    }

    for keyword in ["let ", "const ", "static "] {
        for (at, _) in flat.match_indices(keyword) {
            let rest = &flat[at + keyword.len()..];
            let Some(end) = semicolon(rest) else { continue };
            let statement = &rest[..end];
            // `= ` and not `=`, so `==` inside a `let … else` guard is not read
            // as the binding's own equals sign.
            let Some(equals) = statement.find(" = ") else {
                continue;
            };
            let (pattern, body) = statement.split_at(equals);
            for name in identifiers(pattern) {
                bind(&name, &body[3..]);
            }
        }
    }

    defined
}

fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !text.starts_with(|c: char| c.is_numeric())
}

/// The first `;` that is not inside a string literal.
fn semicolon(text: &str) -> Option<usize> {
    let (mut in_string, mut escaped) = (false, false);
    for (i, byte) in text.bytes().enumerate() {
        match (in_string, escaped, byte) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') => in_string = false,
            (true, false, _) => {}
            (false, _, b'"') => in_string = true,
            (false, _, b';') => return Some(i),
            _ => {}
        }
    }
    None
}

/// The index of the `}` closing the `{` at `open`, skipping string literals for
/// [`matching_paren`]'s reason.
fn matching_brace(text: &str, open: usize) -> Option<usize> {
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for (i, byte) in text.bytes().enumerate().skip(open) {
        match (in_string, escaped, byte) {
            (true, true, _) => escaped = false,
            (true, false, b'\\') => escaped = true,
            (true, false, b'"') => in_string = false,
            (true, false, _) => {}
            (false, _, b'"') => in_string = true,
            (false, _, b'{') => depth += 1,
            (false, _, b'}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Every identifier-shaped run in an expression, outside its string literals.
///
/// Outside the literals, because a path is text: `"/repos/{repo}/issues/comments"`
/// carries the words `repos` and `comments`, and looking each of them up as a
/// binding would expand an unrelated `fn comments(` somewhere else in the file.
fn identifiers(expr: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for ch in expr.chars() {
        match (in_string, escaped, ch) {
            (true, true, _) => escaped = false,
            (true, false, '\\') => escaped = true,
            (true, false, '"') => in_string = false,
            (true, false, _) => {}
            (false, _, '"') => {
                in_string = true;
                flush(&mut current, &mut names);
            }
            (false, _, c) if c.is_alphanumeric() || c == '_' => current.push(c),
            _ => flush(&mut current, &mut names),
        }
    }
    flush(&mut current, &mut names);
    names
}

fn flush(current: &mut String, names: &mut Vec<String>) {
    if !current.is_empty() && !current.chars().next().is_some_and(|c| c.is_numeric()) {
        names.push(std::mem::take(current));
    }
    current.clear();
}
