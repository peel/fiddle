//! `ensure_pull_request`, end to end against the scripted `gh`.
//!
//! The executor's protocol is not re-argued here — `effect_protocol.rs` owns it,
//! against a scripted operation that never reaches a process. What this file
//! asks is the question that is specific to a pull request, and it is a question
//! about *identity* and about *what a refusal means*.
//!
//! **Identity is head and base.** release-please's own documentation records
//! what a title-parsing identity costs: change the title format and a second
//! pull request opens, because the first stopped being recognised. So the title
//! is asserted here to be payload — hashed, so a change is detectable, and
//! absent from every lookup.
//!
//! **A 422 is GitHub preventing the duplicate.** Creating a pull request for a
//! head and base that already has an open one is refused with that status, and a
//! client that read the refusal as an error would report a failure that did not
//! happen. The interesting direction is therefore not "the create failed" but
//! "the create was refused, the read found the pull request, and the effect
//! committed with its number".
//!
//! Everything runs against `tests/gh_stub/`, whose world is a *stateful* one:
//! the pull requests it lists are the ones that were really created or really
//! seeded, so an assertion about how many exist is an assertion about the world
//! rather than about what a fixture was told to say. Offline and credential-free
//! throughout; the `git` in every context is a path that does not exist, so an
//! operation that grew a second mutation channel would fail loudly.

mod support;

use fiddle_core::{effect_id, payload_hash, EffectKind, ProposedEffect, FIXTURE_REPAIR};
use fiddle_runtime::effect::{
    EffectContext, EffectError, EffectOutcome, EffectReceipt, EffectTrace, ExecutionStep, Executor,
    ReadRetry,
};
use fiddle_runtime::github::{branch_name, EnsurePullRequest, PullRequest};
use fiddle_runtime::{GhCli, GhError};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// The repository the scripted `gh` answers for, and the one the API paths name.
const REPO: &str = "peel/r";

/// The owner the head branch lives under. The lookup must qualify the head with
/// it, or the query matches a branch of that name in any repository.
const OWNER: &str = "peel";

const BASE: &str = "main";

/// The title this run proposes. Deliberately not the title of anything already
/// in the world, so a test that passed by matching on one would be visible.
const TITLE: &str = "fiddle: repair the fixture";

/// The script key every create in this file is scripted under.
const CREATE: &str = "POST_repos_peel_r_pulls";

/// A generous bound for children that answer immediately. Nothing here is about
/// the deadline; `github_cli` owns the process bounds.
const PATIENT: Duration = Duration::from_secs(60);

/// The head branch, recomputed the way a fresh process would.
fn head() -> String {
    branch_name(PROJECT, INVOCATION_REF)
}

/// The owner-qualified head, as the world spells it.
fn head_label() -> String {
    format!("{OWNER}:{}", head())
}

// ---------------------------------------------------------------------------
// The world one pull request effect runs against
// ---------------------------------------------------------------------------

/// The scripted `gh`'s scratch directory, and everything a test needs to arrange
/// a world in it or read one back out.
///
/// The three counts below are deliberately separate, because a duplicate hides
/// between them: [`Forge::creation_requests`] is what was *asked for*, counted
/// from the requests the stub recorded; [`Forge::landed_creations`] is what
/// actually changed the world; and [`Forge::open_pull_requests`] is the object
/// count, which is the property the milestone is stated in.
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
    /// A repository with no pull requests.
    fn empty() -> Self {
        let dir = TempDir::new().unwrap();
        // Empty, and stays empty: it is what a real `gh` would be pinned to, and
        // it is what makes the operator's keyring unreachable.
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        Self {
            dir,
            steps: Mutex::new(Vec::new()),
        }
    }

    /// How one scripted request ends. `<status> <exit> <mode>`.
    fn script(&self, key: &str, spec: &str) {
        let script = self.dir.path().join("script");
        std::fs::create_dir_all(&script).unwrap();
        std::fs::write(script.join(key), spec).unwrap();
    }

    /// Put an open pull request in the world before the effect runs.
    ///
    /// Arranged through the stub's own seed rather than by driving the operation
    /// under test, so a world this file claims to have built is not built by the
    /// code the assertions are about.
    fn open_pull_request(&self, head: &str, base: &str, title: &str) {
        let path = self.dir.path().join("pulls_seed");
        let mut seed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_default();
        seed.push(serde_json::json!({ "head": head, "base": base, "title": title }));
        std::fs::write(&path, serde_json::Value::Array(seed).to_string()).unwrap();
    }

    /// Make the list endpoint ignore the query.
    ///
    /// A stand-in for anything between this client and GitHub that answers a
    /// filtered read with something wider — a proxy, a cached page, a future
    /// parameter GitHub stops honouring. The client's own check is what has to
    /// hold then, and this is how it is asked.
    fn answer_without_filtering(&self) {
        std::fs::write(self.dir.path().join("pulls_unfiltered"), "yes").unwrap();
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

    /// The API path of the *n*th request made with `method`.
    fn path_of(&self, method: &str, nth: usize) -> String {
        self.requests()
            .into_iter()
            .filter(|argv| {
                argv.iter()
                    .position(|a| a == "--method")
                    .and_then(|at| argv.get(at + 1))
                    .map(String::as_str)
                    == Some(method)
            })
            .nth(nth)
            .and_then(|argv| argv.iter().find(|a| a.starts_with('/')).cloned())
            .unwrap_or_else(|| panic!("no {method} request number {nth} was recorded"))
    }

    /// How many times a create was *dispatched*, landed or not. The number a
    /// retry would move and a postcondition read would not.
    fn creation_requests(&self) -> usize {
        self.requests()
            .iter()
            .filter(|argv| {
                argv.iter().any(|a| a == "POST") && argv.iter().any(|a| a.starts_with("/repos"))
            })
            .count()
    }

    /// How many creates actually changed the world, read out of the log the stub
    /// writes when a mutation lands.
    fn landed_creations(&self) -> usize {
        self.creations_matching(|_| true)
    }

    /// How many creates landed and then took their answer with them.
    ///
    /// Asserted of the world the test is making its claims about, rather than
    /// demonstrated on some other directory and assumed to have happened here
    /// too. Without it, an exactly-once assertion would pass on a create that
    /// simply succeeded — which is a different property, and a much easier one.
    fn creations_whose_answer_was_lost(&self) -> usize {
        self.creations_matching(|line| line.contains(r#""mode":"commit_then_"#))
    }

    fn creations_matching(&self, keep: impl Fn(&str) -> bool) -> usize {
        std::fs::read_to_string(self.dir.path().join("world"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains("POST_repos_peel_r_pulls") && keep(line))
            .count()
    }

    /// The object count: every open pull request this world holds, however it
    /// came to exist.
    fn open_pull_requests(&self) -> usize {
        let seeded: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join("pulls_seed")).unwrap_or_default(),
        )
        .unwrap_or_default();
        seeded.len() + self.landed_creations()
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }
}

/// The operation under test, proposing `title`.
fn ensure_titled(title: &str) -> EnsurePullRequest {
    EnsurePullRequest::new(
        REPO.to_string(),
        OWNER.to_string(),
        head(),
        BASE.to_string(),
        title.to_string(),
        "opened by fiddle".to_string(),
    )
}

fn ensure() -> EnsurePullRequest {
    ensure_titled(TITLE)
}

/// Walk the authorization order for one pull request effect.
async fn open_the_pull_request(
    forge: &Forge,
    ctx: &EffectContext,
    operation: EnsurePullRequest,
) -> Result<EffectReceipt<PullRequest>, EffectError> {
    let deployment = Deployment(fiddle_core::DeploymentRule::Allow);
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectKind::EnsurePullRequest,
        target: operation.target(),
        payload: operation.payload(),
    };
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        ctx,
        forge,
        // One read and no waiting: this suite's subject is the pull-request
        // operation, not the postcondition read's budget.
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
}

// ---------------------------------------------------------------------------
// Identity is head and base
// ---------------------------------------------------------------------------

/// The rule this operation exists to get right, stated where nothing else can
/// interfere: the identity is derived from head and base, and the title moves
/// the payload hash instead.
///
/// Both halves are the point. If the title were in the target, two runs that
/// worded it differently would derive two effect identities and open two pull
/// requests. If it were in *neither*, a widened request would be
/// indistinguishable from the one that was approved.
#[test]
fn a_title_moves_the_payload_hash_and_never_the_identity() {
    let ours = ensure_titled("fiddle: repair the fixture");
    let reworded = ensure_titled("chore(deps): repair the fixture");

    assert_eq!(
        ours.target(),
        reworded.target(),
        "a reworded title is the same pull request"
    );
    assert_eq!(
        effect_id(
            PROJECT,
            INVOCATION_REF,
            EffectKind::EnsurePullRequest,
            &ours.target()
        ),
        effect_id(
            PROJECT,
            INVOCATION_REF,
            EffectKind::EnsurePullRequest,
            &reworded.target()
        ),
        "so a fresh process recomputes the same identity for it"
    );
    assert!(
        !ours.target().contains("repair the fixture"),
        "and no part of the title reaches the identity: {}",
        ours.target()
    );

    assert_ne!(
        payload_hash(&ours.payload()),
        payload_hash(&reworded.payload()),
        "but the change is still detectable, which is what the payload hash is for"
    );
    assert_eq!(
        payload_hash(&ours.payload()),
        payload_hash(&ensure_titled("fiddle: repair the fixture").payload()),
        "and the payload is canonical: the same request hashes the same"
    );
}

/// The same rule against the world. An open pull request with an entirely
/// different title is *this* pull request, and finding it is what stops a second
/// one being opened.
#[tokio::test]
async fn a_pull_request_is_located_by_head_and_base_not_by_title() {
    let forge = Forge::empty();
    forge.open_pull_request(&head_label(), BASE, "something else entirely");
    let ctx = forge.context();

    let receipt = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect("the pull request is already open");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(receipt.external_ref.as_deref(), Some("7"));
    // The witness that this is the *existing* one rather than a fresh one that
    // happened to get the same number: the title it came back with is the title
    // nobody in this run proposed.
    assert_eq!(
        receipt.value.title, "something else entirely",
        "the pull request that was found is the one the world already held"
    );
    assert_eq!(
        forge.creation_requests(),
        0,
        "a differing title is not a reason to create a second"
    );
    assert_eq!(forge.open_pull_requests(), 1);
    assert_eq!(
        forge.steps(),
        [
            "validate_capability",
            "derive_identity",
            "inspect_postcondition"
        ],
        "the walk stops at the inspection; nothing is even put to policy"
    );
}

/// The exact query matters, and this is the assertion that pins it.
///
/// An unqualified head matches a branch of that name in *any* repository, which
/// is how a lookup finds somebody else's pull request and reports it as this
/// run's work. `state=open` is the second half: a closed pull request is not the
/// postcondition, because the work is no longer proposed.
#[tokio::test]
async fn the_lookup_qualifies_the_head_with_its_owner() {
    let forge = Forge::empty();
    let ctx = forge.context();

    let _ = open_the_pull_request(&forge, &ctx, ensure()).await;

    let lookup = forge.path_of("GET", 0);
    assert!(
        lookup.contains(&format!("head={OWNER}%3Afiddle%2F")),
        "the head must be owner-qualified: {lookup}"
    );
    assert!(lookup.contains(&format!("base={BASE}")), "{lookup}");
    assert!(lookup.contains("state=open"), "{lookup}");
    assert!(
        !lookup.contains("title") && !lookup.contains("repair"),
        "no part of the payload belongs in the lookup: {lookup}"
    );
}

/// And the qualification does real work: the same branch name under another
/// owner is a different pull request, so it is not found and this run opens its
/// own.
///
/// Without this, the previous test could pass against a fixture that ignored the
/// query entirely — the assertion would be about a string nobody acted on.
#[tokio::test]
async fn a_head_under_another_owner_is_not_this_pull_request() {
    let forge = Forge::empty();
    forge.open_pull_request(&format!("someone-else:{}", head()), BASE, "theirs");
    let ctx = forge.context();

    let receipt = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect("nothing matched, so the pull request is opened");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("8"),
        "the effect settled on the one it opened, not on the other owner's"
    );
    assert_eq!(receipt.value.head, head_label());
    assert_eq!(receipt.value.title, TITLE);
    assert_eq!(forge.creation_requests(), 1);
}

/// What comes back is checked against what was asked for, rather than trusted
/// because it arrived.
///
/// The read is the entire basis for calling a refused create committed, so an
/// object that is not the one this operation is about must never become the
/// receipt's `external_ref`. Here the list endpoint answers without honouring
/// the query — a proxy, a cache, a parameter GitHub stopped supporting — and the
/// client refuses it instead of reporting somebody else's pull request as this
/// run's.
#[tokio::test]
async fn a_pull_request_that_is_not_the_one_asked_for_is_never_settled_on() {
    let forge = Forge::empty();
    forge.open_pull_request(&format!("someone-else:{}", head()), BASE, "theirs");
    forge.answer_without_filtering();
    let ctx = forge.context();

    let error = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect_err("an object that was not asked for is not an answer");

    assert!(
        matches!(
            error,
            EffectError::Adapter {
                source: GhError::Malformed(_),
                ..
            }
        ),
        "expected the read to be refused, got {error:?}"
    );
    assert_eq!(
        forge.creation_requests(),
        0,
        "and an unreadable answer is never written over"
    );
}

/// Two open pull requests for one head and base is a state to report, not a set
/// to pick from. GitHub will not create the second, so arriving here means
/// something outside this system did.
#[tokio::test]
async fn two_open_pull_requests_are_reported_rather_than_chosen_between() {
    let forge = Forge::empty();
    forge.open_pull_request(&head_label(), BASE, "one");
    forge.open_pull_request(&head_label(), BASE, "two");
    let ctx = forge.context();

    let error = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect_err("two matching objects is not a postcondition");

    assert!(
        matches!(error, EffectError::DuplicateState { count: 2, .. }),
        "expected DuplicateState with the count, got {error:?}"
    );
    assert_eq!(forge.creation_requests(), 0);
}

// ---------------------------------------------------------------------------
// What a 422 means
// ---------------------------------------------------------------------------

/// The failure mode this operation exists to prevent, and it is a subtle one:
/// GitHub already refuses the duplicate — that is what the 422 *is* — and a
/// client that reads the refusal as an error reports a failure that did not
/// happen.
///
/// The witness at the top is what makes this a test rather than a coincidence.
/// A create that simply succeeded would also reach `Committed` with one open
/// pull request, so the refusal has to be shown to be real, and shown to carry
/// no usable prose: the message the client is handed is `scripted 422`, which
/// says nothing about duplication in any language. Nothing could have been
/// resolved by matching on it.
#[tokio::test]
async fn a_422_for_a_pull_request_that_already_exists_is_not_a_false_failure() {
    let witness = Forge::empty();
    witness.script(CREATE, "422 1 conflict");
    let refusal = witness
        .context()
        .gh
        .api(
            "POST",
            &format!("/repos/{REPO}/pulls"),
            Some(&serde_json::json!({ "head": head_label(), "base": BASE })),
            &CancellationToken::new(),
        )
        .await
        .expect_err("the create must really be refused, or this proves nothing");
    assert!(
        matches!(
            refusal,
            GhError::Http {
                status: 422,
                ref message,
                ..
            } if message == "scripted 422"
        ),
        "expected a 422 whose message says nothing, got {refusal:?}"
    );
    assert_eq!(
        refusal.outcome(),
        EffectOutcome::Unknown,
        "a 422 is never classified on its face; being Unknown is what forces the read"
    );
    assert_eq!(
        witness.landed_creations(),
        0,
        "and the refused create must really have changed nothing"
    );

    let forge = Forge::empty();
    forge.script(CREATE, "422 1 conflict");
    let ctx = forge.context();

    let receipt = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect("the duplicate was prevented, which is not a failure");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("7"),
        "and the receipt carries the existing pull request's number"
    );
    assert_eq!(
        receipt.value.title, "opened by another run",
        "the object settled on is the one that made the create a duplicate"
    );
    assert_eq!(
        forge.open_pull_requests(),
        1,
        "exactly one pull request — the property, stated as an object count"
    );
    assert_eq!(forge.creation_requests(), 1, "dispatched exactly once");
    assert_eq!(
        forge.landed_creations(),
        0,
        "and this run created nothing: the 422 was a real refusal, resolved by \
         the read rather than by the response"
    );
}

/// A 422 with nothing matching is a real validation failure and stays one.
///
/// The other side of the same rule, and the one that keeps the first from being
/// a client that calls every 422 a success. `Unresolved` rather than a confident
/// failure is deliberate: a 422 covers malformed input, invalid ref syntax and
/// spam protection alike, and the read found nothing to settle which — so nobody
/// knows, and saying so is the honest answer.
#[tokio::test]
async fn a_422_with_no_matching_pull_request_stays_a_failure() {
    let forge = Forge::empty();
    forge.script(CREATE, "422 1 normal");
    let ctx = forge.context();

    let error = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect_err("nothing was observed, so nothing is confirmed");

    assert!(
        matches!(error, EffectError::Unresolved { .. }),
        "expected Unresolved rather than a receipt, got {error:?}"
    );
    assert_eq!(
        forge.open_pull_requests(),
        0,
        "no pull request was created, and none was invented by reading"
    );
    assert_eq!(
        forge.creation_requests(),
        1,
        "an unresolved outcome is never resolved by dispatching again"
    );
}

/// An interrupted create followed by the postcondition read yields one pull
/// request.
///
/// The hazard is demonstrated before it is relied on, because both halves have
/// to be real: the create really lands, and the answer really is lost. A create
/// that simply succeeded would also reach `Committed`, and only the witness says
/// which route was taken.
#[tokio::test]
async fn a_lost_create_response_does_not_produce_a_second_pull_request() {
    let witness = Forge::empty();
    witness.script(CREATE, "201 0 commit_then_die");
    let lost = witness
        .context()
        .gh
        .api(
            "POST",
            &format!("/repos/{REPO}/pulls"),
            Some(&serde_json::json!({ "head": head_label(), "base": BASE })),
            &CancellationToken::new(),
        )
        .await
        .expect_err("the fixture must really lose the answer, or it proves nothing");
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
        witness.landed_creations(),
        1,
        "and the write must really have landed, or the answer was all that was lost"
    );

    let forge = Forge::empty();
    forge.script(CREATE, "201 0 commit_then_die");
    let ctx = forge.context();

    let receipt = open_the_pull_request(&forge, &ctx, ensure())
        .await
        .expect("the answer was lost, not the write");

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        receipt.external_ref.as_deref(),
        Some("7"),
        "the number comes from the read, since the create never reported one"
    );
    assert_eq!(receipt.value.head, head_label());
    assert_eq!(receipt.value.base, BASE);
    assert_eq!(
        forge.creations_whose_answer_was_lost(),
        1,
        "the ambiguity is real in *this* world: the create landed and the child \
         died before it could say so, which is the only route by which reaching \
         Committed means anything here"
    );
    assert_eq!(
        forge.open_pull_requests(),
        1,
        "exactly one pull request, which is the property"
    );
    assert_eq!(
        forge.creation_requests(),
        1,
        "and it was dispatched exactly once; an Unknown settled by retrying \
         instead of by reading would show up here as two"
    );
    assert_eq!(
        forge.steps().iter().filter(|s| **s == "apply").count(),
        1,
        "the executor agrees it dispatched once"
    );
}

// ---------------------------------------------------------------------------
// What the classification is allowed to depend on
// ---------------------------------------------------------------------------

/// No branch may depend on GitHub's prose.
///
/// The behavioural half of this is already above — the 422 that resolves carries
/// the message `scripted 422`, so nothing could have been matched on it. This is
/// the source-level half, and it is here because the failure it guards against
/// is one somebody adds later in good faith: a `contains("already exists")` is
/// an easy line to write and it is a client whose correctness depends on English
/// that GitHub never promised to keep.
///
/// Comment lines are excluded on purpose. The module documentation has to be
/// able to *name* the phrase it refuses to match on, or the rule cannot be
/// explained to the next reader.
#[test]
fn no_branch_depends_on_githubs_error_prose() {
    let source = include_str!("../src/github/pulls.rs");
    let code: String = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    for phrase in [
        "already exists",
        "A pull request already",
        "No commits between",
        "Validation Failed",
    ] {
        assert!(
            !code.contains(phrase),
            "classification must not depend on GitHub's wording: {phrase:?}"
        );
    }
    assert!(
        !code.contains("message"),
        "the error envelope's message is prose; the status and the postcondition \
         read are the contract"
    );
}
