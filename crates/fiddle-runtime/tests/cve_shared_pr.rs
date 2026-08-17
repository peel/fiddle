//! The one shared pull request: how a run finds it, what it may do to it, and
//! why its body update needs a content-addressed identity.
//!
//! Two subjects, and they are the same object seen from two ends. Task 17.a
//! asks *which pull request is this run's, and may it work there at all* — a
//! read and a refusal, before a tree exists. Task 18 asks *how does a rewrite of
//! that pull request's body stay honest across runs* — an identity question,
//! after everything else has happened. They share a file because they share the
//! object, and because a reader who wonders why a body update needs a digest has
//! to know first that there is only ever one pull request for it to be about.
//!
//! # Discovery: the label is the only thing that identifies it
//!
//! Nothing else can. The branch name is dated, so it changes; the title names no
//! advisory, because the pull request outlives any one run's findings; the body
//! is prose a rescan rewrites. Design §4's model is one pull request per
//! repository, and `security/cve` is what makes that one findable — so a pull
//! request created without it is invisible to the next run, which opens a
//! second, which is the state the whole model exists to prevent.
//!
//! `a_created_pull_request_carries_the_label_that_finds_it` and
//! `a_pull_request_created_without_the_label_is_invisible_to_the_next_run` are a
//! pair and neither means much alone: the first would pass against a discovery
//! read that ignored labels entirely, and the second is what rules that out.
//! The same shape recurs throughout this half — the anomaly note has a
//! *no-anomaly* negative, the postcondition's label gate has a *labelled* half
//! beside its unlabelled one, and the ordering test runs one driver over two
//! worlds so that "nothing was committed" is not "this driver never commits".
//!
//! **The label reaches the world only by being sent.** `gh_stub` derives a pull
//! request's labels from the seed a test wrote plus the `POST
//! /issues/{n}/labels` calls that really landed, so a create that skipped its
//! second request produces an object the discovery read genuinely cannot find.
//! Nothing here asserts that a request was *made*; every claim is about what the
//! world holds afterwards, read back through the client.
//!
//! # The refusal, and why it is a value rather than a check
//!
//! A pull request carrying the label whose head is outside `security/` is an
//! ordinary mistake — a person labels their own branch, meaning *this is about
//! the CVEs* — and it is one a run must not work around. Committing onto that
//! branch writes to somebody else's work; opening a second pull request is the
//! duplicate. So it stops, and it has to stop *before* the commit, because a run
//! that commits and then fails to push has already done the damage.
//!
//! `plan` answers `Result<Approved, Refusal>` and only `Approved` names a
//! branch, so after a refusal there is nothing for a checkout or a commit to be
//! addressed at. That is the structural half.
//! `the_prefix_refusal_reaches_the_run_before_any_commit` is the driven half,
//! over a real git tree whose history is asserted unchanged.
//!
//! # Bodies: the effect whose object never changes, so its *content* has to
//! enter its identity
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
    content_digest, effect_id, AttemptId, EffectId, EffectKind, ProposedEffect, FIXTURE_REPAIR,
};
use fiddle_runtime::capability::cve::{
    check_out, plan, plan_shared_pull_request, publish_shared_work, Approved, Checkout, PlanError,
    Refusal, SharedPublication, SharedWork, BRANCH_STEM, CVE_LABEL, PUSHABLE_PREFIX,
};
use fiddle_runtime::capability::{land, GroupStatus, InWorktree};
use fiddle_runtime::effect::{
    EffectContext, EffectOutcome, EffectReceipt, EffectTrace, ExecutionStep, Executor,
    IntegrationOperation, ReadRetry,
};
use fiddle_runtime::github::{
    find_labelled_pull_request, pull_request_body_target, EnsurePullRequest, EnsurePullRequestBody,
    PullRequest,
};
use fiddle_runtime::journal::{AttemptTrace, FileJournal, JOURNAL_DIR};
use fiddle_runtime::workspace::Workspace;
use fiddle_runtime::{GhCli, GitCli};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use support::cve::{
    ask_git, landing_world, remote_world, try_ask_git, LandingWorld, RemoteWorld,
    ONLY_ON_THE_REMOTE_BASE, ON_THE_SHARED_BRANCH,
};
use support::{unreachable_git, Deployment, INVOCATION_REF, PROJECT};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// The repository the scripted `gh` answers for.
const REPO: &str = "peel/r";

/// The owner the head branch lives under, and the one the lookup qualifies it
/// with. A head that is not owner-qualified matches a branch of that name in any
/// repository, which `pull_request_effect.rs` states in full.
const OWNER: &str = "peel";

/// The branch every pull request in this file is proposed into.
const BASE: &str = "main";

/// The head branch of the shared pull request these lanes arrange.
///
/// A real one of this capability's own making: `security/cve-remediation-` plus
/// a date, which is what [`BRANCH_STEM`] renders and what the pushable prefix
/// admits. Dated to a *different* day from [`TODAY`] on purpose — a reused
/// branch is whichever day's branch is still open, and a fixture that dated it
/// today could not tell reuse from a fresh cut.
const SHARED_HEAD: &str = "security/cve-remediation-20260813";

/// The day a fresh branch in this file would be cut on.
const TODAY: &str = "20260817";

/// The title the shared pull request carries.
///
/// Deliberately naming no advisory, for the reason `capability::cve`'s commit
/// subject names none: the shared pull request outlives any one run's findings.
const SHARED_TITLE: &str = "fiddle: mitigate reported advisories";

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

/// Git's empty tree, which every repository can name whether or not it has ever
/// stored one. See [`Forge::seed_branch`].
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// The invocation the driven lanes run under, as a report directory names it.
///
/// Any stable string would do; it is the directory `<report.dir>/.attempts/`
/// holds this attempt's journal under, and nothing reads it back but this file.
const SLUG: &str = "beans-w-1";

/// The advisory the driven lanes' one clean group is about.
///
/// A different year and number from every other id in this crate's CVE suites, so
/// a commit body found on the branch cannot be some other fixture's.
const LANDED_CVE: &str = "CVE-2026-4242";

/// A commit the pure decision is handed and does nothing with.
///
/// The unit lane below takes no forge and no git, so nothing can resolve this;
/// it is a full object name because that is what the field carries, and a value
/// no world in this file produces so that a lane finding it somewhere real would
/// be visible.
const A_TIP: &str = "aaaaaaaabbbbbbbbccccccccddddddddeeeeeeee";

/// What a driven run has to say for itself, before the anomaly note.
///
/// Deliberately not prose any assertion below matches on: what this file is about
/// is the shape of the body, not its wording, and a lane that pinned the wording
/// would fail the day the disposition table lands in it.
const RUN_SUMMARY: &str = "fiddle mitigated the advisories listed below.";

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
    /// One entry per traced step, carrying what the outside world held at the
    /// moment the executor announced it. See [`Watched`] and
    /// [`Forge::mutations_outside_an_effect`].
    watched: Mutex<Vec<Watched>>,
    /// The production fan-out to an attempt's journal, attached by the driver
    /// that has a `<report.dir>` to write one into and left empty by every lane
    /// that does not — which is [`AttemptTrace`]'s own arrangement, silently
    /// discarding while no attempt owns it.
    trace: AttemptTrace,
}

impl EffectTrace for Forge {
    fn step(&self, kind: EffectKind, step: ExecutionStep) {
        self.steps.lock().unwrap().push(step.as_str());
        // Read *before* the work behind the step, which is the same moment the
        // journal's record is written and the reason the record is worth
        // writing: a window opened at `apply` and closed at
        // `observe_postcondition` therefore brackets exactly the mutations that
        // step dispatched.
        self.watched.lock().unwrap().push(Watched {
            kind,
            step,
            mutations: self.mutations().len(),
            remote_branches: self.remote_branches(),
        });
        self.trace.step(kind, step);
    }
}

/// What the world held at one announced step of the authorization order.
///
/// The two counts are the two channels a mutation can reach the outside world
/// through, and both are needed: a pull request create is an HTTP request the
/// scripted `gh` records, and a branch publication is a `git push` that records
/// nothing anywhere — it is only visible as a ref that was not on the remote
/// before and is after.
struct Watched {
    kind: EffectKind,
    step: ExecutionStep,
    /// How many mutations the forge had recorded by this point.
    mutations: usize,
    /// Which branches the remote held by this point.
    remote_branches: Vec<String>,
}

impl Forge {
    /// A world holding one open pull request whose body is [`SEEDED_BODY`].
    ///
    /// Arranged through the stub's own by-number file rather than by driving the
    /// operation under test, so the world these tests make claims about is not
    /// built by the code the claims are about.
    fn holding_the_shared_pull_request() -> Self {
        let world = Self::empty();
        let dir = &world.dir;

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

        world
    }

    /// A world holding nothing at all: no pull request, no issue, no label.
    ///
    /// The starting point for the discovery lanes, which arrange what they need
    /// through [`Forge::seed_pull_request`] and [`Forge::seed_issue`] — or, in
    /// the one lane whose subject is a *create*, by driving the operation and
    /// then reading the world back through the client.
    fn empty() -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        let forge = Self {
            dir,
            steps: Mutex::new(Vec::new()),
            watched: Mutex::new(Vec::new()),
            trace: AttemptTrace::new(),
        };
        // **The remote is part of an empty world, not an extra a lane opts into.**
        // The scripted `gh` reads a pull request's head sha and a branch ref out
        // of `remote.git` beside its own scratch directory — see
        // `tests/gh_stub/gh_stub.rs` — so a forge without one answers `null` for
        // a head it visibly holds, and the discovery read then reports a
        // malformed pull request. Building it here means every lane's world can
        // be asked *what is the remote actually at*, which is the question this
        // half of the task is entirely about.
        ask_git(
            forge.dir.path(),
            &[
                "-c",
                "init.defaultBranch=main",
                "init",
                "--quiet",
                "--bare",
                "remote.git",
            ],
        );
        forge
    }

    /// The bare repository both adapters see: `git` over a path, the scripted
    /// `gh` over its ref files.
    fn remote(&self) -> PathBuf {
        self.dir.path().join("remote.git")
    }

    /// Put `branch` on the remote, at a commit of its own, and answer the sha.
    ///
    /// Plumbing rather than a working tree, because what a lane arranging a
    /// *pull request* needs from the remote is a ref that resolves — the head sha
    /// the forge then reports. The commit carries the empty tree and the branch's
    /// own name as its message, which is enough to make two branches two commits;
    /// a lane that needs real files on the branch builds a
    /// [`remote_world`](support::cve::remote_world) instead, and that one pushes.
    ///
    /// **A branch the remote already holds is left exactly where it is**, and
    /// that is not tidiness. A lane that builds a
    /// [`remote_world`](support::cve::remote_world) has already pushed real
    /// history onto the shared branch, and then seeds the pull request that is
    /// open on it; a `seed_branch` that overwrote would replace the tip the
    /// checkout is about with a commit carrying an empty tree, and every
    /// assertion downstream would be about the fixture's second thoughts.
    fn seed_branch(&self, branch: &str) -> String {
        let remote = self.remote();
        let named = format!("refs/heads/{branch}");
        if let Ok(existing) = try_ask_git(&remote, &["rev-parse", "--verify", "--quiet", &named]) {
            return existing;
        }
        let commit = ask_git(
            &remote,
            &[
                // Per invocation, for `support::cve`'s reason: a CI runner has no
                // `user.email` and `commit-tree` refuses outright without one.
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit-tree",
                // The empty tree, which every git can name whether or not the
                // repository has stored it.
                EMPTY_TREE,
                "-m",
                branch,
            ],
        );
        ask_git(&remote, &["update-ref", &named, &commit]);
        commit
    }

    /// What the remote holds for the head of pull request `number`.
    ///
    /// Read out of the **remote** rather than out of the seed a test wrote, so
    /// that "the worktree is at the pull request's tip" is an agreement between
    /// two independent readings of one repository — git's, through the checkout,
    /// and this one — rather than one fixture value compared with itself.
    fn pr(&self, number: u64) -> SeededPullRequest {
        let seed: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join("pulls_seed")).unwrap_or_default(),
        )
        .unwrap_or_default();
        let head = seed
            .iter()
            .find(|pr| pr["number"].as_u64() == Some(number))
            .and_then(|pr| pr["head"].as_str())
            .unwrap_or_else(|| panic!("this world holds no pull request numbered {number}"))
            .to_string();
        let branch = head.split_once(':').map(|(_, r)| r).unwrap_or(&head);
        SeededPullRequest {
            head_sha: ask_git(
                &self.remote(),
                &["rev-parse", &format!("refs/heads/{branch}")],
            ),
        }
    }

    /// Every branch the remote holds, in ref order.
    fn remote_branches(&self) -> Vec<String> {
        ask_git(
            &self.remote(),
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    /// Put an open pull request in the world, at a number of the test's choosing.
    ///
    /// **The number is named rather than positional**, and that is what makes
    /// `several_open_pull_requests_take_the_lowest_and_note_the_rest` a real
    /// test. The stub's default numbering is the seed's own order, so a world
    /// seeded 57, 41, 63 would come out 7, 8, 9 and "the lowest" would be
    /// indistinguishable from "the first". Naming the numbers lets the arrival
    /// order and the numeric order disagree, which is the only arrangement in
    /// which the three candidate rules — lowest, first, last — give three
    /// different answers.
    ///
    /// Arranged through the stub's own seed rather than by driving the code
    /// under test, so a world this file makes claims about is not built by the
    /// thing the claims are about.
    fn seed_pull_request(&self, number: u64, head: &str, labels: &[&str]) {
        self.seed_pull_request_in_state(number, head, labels, "open");
    }

    /// The same, for a pull request that is not open.
    ///
    /// Only the label search needs one, and only to be shown ignoring it: a
    /// closed pull request is a branch that has been merged or abandoned, and a
    /// run that settled on one would commit onto history its base already
    /// carries.
    fn seed_pull_request_in_state(&self, number: u64, head: &str, labels: &[&str], state: &str) {
        // The branch goes on the remote first, because a pull request whose head
        // is not a ref is not a state GitHub will produce: `POST /pulls` refuses
        // a head that does not exist. Until 17.b the fixture could get away with
        // it, because nothing read a head sha; now the discovery read does, and a
        // world seeded without one would be asking the client to survive an
        // answer the real forge cannot give.
        self.seed_branch(head);

        let path = self.dir.path().join("pulls_seed");
        let mut seed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_default();
        seed.push(serde_json::json!({
            "number": number,
            "state": state,
            "head": format!("{OWNER}:{head}"),
            "base": BASE,
            "title": SHARED_TITLE,
            "body": SEEDED_BODY,
            "labels": labels,
        }));
        std::fs::write(&path, serde_json::Value::Array(seed).to_string()).unwrap();
    }

    /// Put a plain issue — not a pull request — in the world, carrying `labels`.
    ///
    /// The issues listing is where a label search is answered, and GitHub's own
    /// answer mixes issues and pull requests: a pull request *is* an issue there,
    /// and the only thing that says which is the `pull_request` key. A label
    /// somebody put on an ordinary issue is therefore in front of the discovery
    /// read, and a reader that took the lowest number of whatever came back
    /// would settle on an object with no head branch at all.
    fn seed_issue(&self, number: u64, labels: &[&str]) {
        let path = self.dir.path().join("issues_seed");
        let mut seed: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
                .unwrap_or_default();
        seed.push(serde_json::json!({
            "number": number,
            "title": "a person's own note about advisories",
            "labels": labels,
        }));
        std::fs::write(&path, serde_json::Value::Array(seed).to_string()).unwrap();
    }

    /// Make the issues listing ignore the `labels` and `state` parameters.
    ///
    /// The counterpart of `pull_request_effect.rs`'s `answer_without_filtering`,
    /// and it stands for the same thing: anything between this client and GitHub
    /// that answers a filtered read with something wider — a proxy, a cached
    /// page, a parameter GitHub stops honouring. The discovery read's own check
    /// is what has to hold then, and this is how it is asked.
    fn answer_the_label_search_without_filtering(&self) {
        std::fs::write(self.dir.path().join("issues_unfiltered"), "yes").unwrap();
    }

    /// The scripted `gh`, on its own rather than inside an [`EffectContext`].
    ///
    /// The discovery read is a plain read and takes a client, not a context —
    /// it proposes no effect, because it changes nothing. Built from the same
    /// four arguments the context's is, so the two cannot address different
    /// worlds.
    fn gh(&self) -> GhCli {
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
        )
    }

    /// A context whose `gh` is the scripted one and whose `git` cannot be run.
    fn context(&self) -> EffectContext {
        EffectContext::new(
            self.gh(),
            unreachable_git(),
            self.dir.path().to_path_buf(),
            CancellationToken::new(),
        )
    }

    /// The same, with a `git` that really runs, pushing out of `worktree`.
    ///
    /// The **real** `git` and not `git_stub`: what the driven lanes are about is a
    /// ref that appears on a remote the scripted `gh` then reads back through its
    /// own door, and a fixture that only claimed to have pushed would leave the
    /// postcondition read with nothing to find. `effect_protocol.rs` makes the same
    /// choice for the same reason.
    ///
    /// Its credential is a sentinel that reaches no network: the remote is a path
    /// on this filesystem, so the five names `GitCli` builds for
    /// `credential.https://github.com` are set and never consulted.
    fn context_pushing_from(&self, worktree: &Path) -> EffectContext {
        EffectContext::new(
            self.gh(),
            GitCli::new(
                PathBuf::from("git"),
                "ghp_never_reaches_a_network".to_string(),
                "FIDDLE_GITHUB_TOKEN",
                PATIENT,
            ),
            worktree.to_path_buf(),
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

    /// How many pull-request creates were *dispatched*, landed or not.
    ///
    /// Narrowed to the pulls collection rather than counting every `POST` under
    /// `/repos`, because a create is no longer this operation's only write: the
    /// label goes on through `POST /repos/{repo}/issues/{n}/labels`, and a
    /// counter that swept both would report two creates for one pull request and
    /// make `never a second` unfalsifiable in the wrong direction.
    fn creation_requests(&self) -> usize {
        self.requests()
            .iter()
            .filter(|argv| {
                let method = argv
                    .iter()
                    .position(|a| a == "--method")
                    .and_then(|at| argv.get(at + 1));
                method.map(String::as_str) == Some("POST")
                    && argv.iter().any(|a| a == &format!("/repos/{REPO}/pulls"))
            })
            .count()
    }

    /// The object count: every open pull request this world holds, however it
    /// came to exist.
    ///
    /// The seed plus the creates that really landed, which is the same pair the
    /// stub's own listing is built from — so this counts objects rather than
    /// requests, and a create whose answer was lost still counts once.
    fn open_pull_requests(&self) -> usize {
        let seeded: Vec<serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(self.dir.path().join("pulls_seed")).unwrap_or_default(),
        )
        .unwrap_or_default();
        let landed = std::fs::read_to_string(self.dir.path().join("world"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.contains(&format!("POST_repos_{}_pulls", REPO.replace('/', "_"))))
            .count();
        seeded.len() + landed
    }

    fn steps(&self) -> Vec<&'static str> {
        self.steps.lock().unwrap().clone()
    }

    /// Every request that could have changed the forge, in arrival order.
    ///
    /// Read off the **requests** rather than off the world log, and the
    /// distinction is the whole routing claim: the world log holds what landed,
    /// and what this lane excludes is a mutation *dispatched* outside the
    /// executor — one the forge happened to refuse is still one that reached it.
    ///
    /// Anything that is not a `GET`. Not a list of the verbs this build happens
    /// to use today: a `PUT` or a `DELETE` added by a later change is exactly the
    /// mutation nobody would remember to widen a list for.
    fn mutations(&self) -> Vec<String> {
        self.requests()
            .iter()
            .filter(|argv| method_of(argv).as_deref() != Some("GET"))
            .map(|argv| argv.join(" "))
            .collect()
    }

    /// Every mutation that did **not** happen inside an effect's apply window.
    ///
    /// A window opens where the executor announced [`ExecutionStep::Apply`] for a
    /// kind and closes where it announced [`ExecutionStep::ObservePostcondition`]
    /// for the same kind, and the bounds are the mutation counts read at those two
    /// moments. Every mutation this process dispatched has an index; one that lies
    /// in no window is one that reached the forge without a recorded effect step,
    /// which is exactly what this file's Hard Constraint excludes.
    ///
    /// **Not a count comparison.** Two mutations for one effect is the ordinary
    /// case here — a create is a `POST /pulls` and then a `POST
    /// /issues/{n}/labels`, inside one apply — so `mutations().len() ==
    /// applies().len()` would be false for a correct run and true for several
    /// wrong ones. What the routing claim is actually about is *which* step each
    /// mutation happened under, and an index in a window is that.
    fn mutations_outside_an_effect(&self) -> Vec<String> {
        let mutations = self.mutations();
        let covered = self.apply_windows();
        mutations
            .iter()
            .enumerate()
            .filter(|(at, _)| !covered.iter().any(|(from, to)| (from..to).contains(&at)))
            .map(|(_, what)| what.clone())
            .collect()
    }

    /// `[apply, observe)` in mutation indices, one per effect that applied.
    fn apply_windows(&self) -> Vec<(usize, usize)> {
        let watched = self.watched.lock().unwrap();
        let mut windows = Vec::new();
        for (at, opened) in watched.iter().enumerate() {
            if opened.step != ExecutionStep::Apply {
                continue;
            }
            // The matching close is the next `observe_postcondition` announced
            // for the same kind. Matched on the kind rather than taken as the
            // next step of any kind, so two effects whose walks somehow
            // interleaved could not have one's window swallow the other's.
            let closed = watched[at + 1..]
                .iter()
                .find(|later| {
                    later.kind == opened.kind && later.step == ExecutionStep::ObservePostcondition
                })
                // An apply the executor never came back from — a lost answer —
                // leaves the window open to the end, which is the honest bound:
                // whatever it dispatched is still attributable to it.
                .map(|later| later.mutations)
                .unwrap_or(usize::MAX);
            windows.push((opened.mutations, closed));
        }
        windows
    }

    /// Which branches the remote gained during `kind`'s apply window.
    ///
    /// The other channel a mutation travels on, and the one no request log can
    /// see: `EnsureBranchPublished` changes the world with a `git push`, which
    /// leaves nothing behind at the forge's API at all. A ref that appeared
    /// between the apply and the observe appeared because of that push.
    fn branches_gained_during(&self, kind: EffectKind) -> Vec<String> {
        let watched = self.watched.lock().unwrap();
        let before = watched
            .iter()
            .find(|it| it.kind == kind && it.step == ExecutionStep::Apply)
            .map(|it| it.remote_branches.clone())
            .unwrap_or_default();
        let after = watched
            .iter()
            .find(|it| it.kind == kind && it.step == ExecutionStep::ObservePostcondition)
            .map(|it| it.remote_branches.clone())
            .unwrap_or_default();
        after
            .into_iter()
            .filter(|branch| !before.contains(branch))
            .collect()
    }

    /// Attach the journal of the attempt the driver is running, so the executor's
    /// steps are recorded where a recovery would look for them.
    ///
    /// The production [`FileJournal`], writing the production `.jsonl`, rather
    /// than a recorder of this file's own: the claim is about *the journal's*
    /// effect steps, and a bespoke sink would prove only that this suite can count
    /// its own callbacks.
    fn journalling(&self, report_dir: &Path, attempt: &AttemptId) -> Journal {
        self.trace.attach(Arc::new(FileJournal::new(
            report_dir,
            SLUG,
            attempt,
            INVOCATION_REF,
        )));
        Journal {
            path: report_dir
                .join(JOURNAL_DIR)
                .join(SLUG)
                .join(format!("{}.jsonl", attempt.0)),
        }
    }
}

/// What the forge holds for one seeded pull request, read back out of the
/// remote.
struct SeededPullRequest {
    head_sha: String,
}

/// The `--method` of one recorded `gh` invocation.
///
/// Every request this client makes carries one — `GhCli::api` writes
/// `--method <verb> <path>` — so a recorded invocation without it is a `gh`
/// spawned by something other than that client, which is a finding rather than a
/// `GET` to be assumed.
fn method_of(argv: &[String]) -> Option<String> {
    argv.iter()
        .position(|a| a == "--method")
        .and_then(|at| argv.get(at + 1))
        .cloned()
}

/// One attempt's journal, read back off disk.
///
/// A reader rather than a recorder. What it answers is what a *fresh process*
/// picking through `<report.dir>/.attempts/` would find, which is the only reason
/// the journal is written at all.
struct Journal {
    path: PathBuf,
}

impl Journal {
    fn records(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// Every effect kind the journal's `effect_step` records name.
    ///
    /// Resolved back through [`EffectKind::ALL`] rather than compared as strings,
    /// so a kind the journal spells in a way no [`EffectKind`] does is a `None`
    /// this drops rather than a name that silently matches nothing.
    fn effect_steps_kinds(&self) -> Vec<EffectKind> {
        self.kinds_where(|_| true)
    }

    /// The subset that reached [`ExecutionStep::Apply`] — the ones that were
    /// allowed to change something.
    fn kinds_that_applied(&self) -> Vec<EffectKind> {
        self.kinds_where(|step| step == ExecutionStep::Apply.as_str())
    }

    fn kinds_where(&self, wanted: impl Fn(&str) -> bool) -> Vec<EffectKind> {
        let mut kinds: Vec<EffectKind> = self
            .records()
            .iter()
            .filter(|record| record["record"] == "effect_step")
            .filter(|record| wanted(record["step"].as_str().unwrap_or_default()))
            .filter_map(|record| {
                let named = record["kind"].as_str()?;
                EffectKind::ALL
                    .iter()
                    .copied()
                    .find(|k| k.as_str() == named)
            })
            .collect();
        kinds.dedup();
        kinds
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
// Discovery: the label is what finds the one shared pull request
// ---------------------------------------------------------------------------

/// Walk the authorization order for one pull request create, carrying `labels`.
///
/// Through the executor rather than by calling the operation, because a label
/// applied outside it would be a mutation that reached the forge without an
/// effect step — the epic's Hard Constraint — and because the postcondition read
/// is the whole of what says the label really landed.
async fn open_the_shared_pull_request(
    forge: &Forge,
    head: &str,
    labels: &[&str],
) -> EffectReceipt<PullRequest> {
    let operation = EnsurePullRequest::new(
        REPO.to_string(),
        OWNER.to_string(),
        head.to_string(),
        BASE.to_string(),
        SHARED_TITLE.to_string(),
        "opened by fiddle, contents to follow".to_string(),
        false,
    )
    .labelled(labels.iter().map(|it| it.to_string()).collect());
    let proposed = ProposedEffect {
        capability: FIXTURE_REPAIR,
        kind: EffectKind::EnsurePullRequest,
        target: operation.target(),
        payload: operation.payload(),
    };
    let deployment = Deployment(fiddle_core::DeploymentRule::Allow);
    let ctx = forge.context();
    Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        forge,
        ReadRetry::none(),
    )
    .execute(proposed, operation)
    .await
    .expect("a pull request create against a world that holds none")
}

/// The discovery read, exactly as the next run would make it.
async fn discover(forge: &Forge) -> Option<fiddle_runtime::github::SharedPullRequest> {
    find_labelled_pull_request(&forge.gh(), REPO, CVE_LABEL, &CancellationToken::new())
        .await
        .expect("the label search is readable")
}

/// Discover and decide, which is the whole of what this lane does before a run
/// may touch a tree.
async fn decide(forge: &Forge) -> Result<Approved, PlanError> {
    plan_shared_pull_request(&forge.gh(), REPO, BASE, TODAY, &CancellationToken::new()).await
}

/// **The label is what makes the created pull request findable next time.**
///
/// The criterion's own words: *a PR without it is invisible to the next run,
/// which then opens a second*. So the claim is not that a label was sent — a
/// request is a claim about a request — but that the object the create left
/// behind is found **by the same observation the next run makes**, which is the
/// only thing that decides whether a second gets opened.
///
/// Both halves are read out of the world rather than out of the fixture. The
/// receipt's value is the executor's step 8 observation, and
/// [`discover`] is a second, independent read through a *different endpoint*:
/// the create went to `/pulls`, the label to `/issues/{n}/labels`, and the
/// search reads `/issues?labels=`. Three endpoints and one object, which is what
/// makes the agreement between them evidence.
///
/// Its inversion is the test below, and the two are a pair: this one would pass
/// against a discovery read that answered every open pull request whatever it
/// was labelled, and that one is what rules it out.
#[tokio::test]
async fn a_created_pull_request_carries_the_label_that_finds_it() {
    let forge = Forge::empty();
    // The head has to be on the remote before a pull request can be opened from
    // it — GitHub refuses a create whose head is not a ref, and since 17.b the
    // discovery read asks what that ref is at. In a real run the branch is there
    // because `EnsureBranchPublished` put it there one effect earlier.
    forge.seed_branch(SHARED_HEAD);

    let receipt = open_the_shared_pull_request(&forge, SHARED_HEAD, &[CVE_LABEL]).await;

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert!(
        receipt.value.labels.contains(&CVE_LABEL.to_string()),
        "the postcondition read found the pull request carrying the label: {:?}",
        receipt.value
    );

    let found = discover(&forge)
        .await
        .expect("the run that created it must be able to find it again");
    assert_eq!(found.number, receipt.value.number);
    assert_eq!(
        found.head, SHARED_HEAD,
        "and at the branch it was opened on"
    );
    assert!(
        found.duplicates.is_empty(),
        "one pull request, so nothing to note: {:?}",
        found.duplicates
    );

    // **One effect, two requests** — which is what *as part of creating* can
    // mean, given that the create endpoint has no `labels` parameter and the
    // label lives on the issues collection. The two requests are inside one
    // authorization walk, so there is exactly one apply step, and a label call
    // that failed would fail the effect rather than leaving a create reported as
    // a success.
    assert_eq!(
        forge
            .steps()
            .iter()
            .filter(|step| **step == ExecutionStep::Apply.as_str())
            .count(),
        1,
        "the label is not a second effect: {:?}",
        forge.steps()
    );
    let posts: Vec<String> = forge
        .requests()
        .iter()
        .filter(|argv| argv.iter().any(|a| a == "POST"))
        .filter_map(|argv| argv.iter().find(|a| a.starts_with('/')).cloned())
        .collect();
    assert_eq!(
        posts,
        [
            format!("/repos/{REPO}/pulls"),
            format!("/repos/{REPO}/issues/{}/labels", receipt.value.number),
        ],
        "the create, and then the label on the object it created — in that order, \
         because the label is addressed by a number that does not exist until the \
         create has run"
    );
}

/// **An unlabelled pull request for this head and base is not this effect having
/// happened.**
///
/// The case is the one the two-request shape cannot make impossible: a process
/// dies between the create and the label, and leaves a pull request the next run
/// cannot find. What the postcondition can do is refuse to call that done — and
/// that is what this asserts, directly against
/// [`IntegrationOperation::inspect`], because it is a claim about the *read* and
/// no walk of the executor would separate it from what the create did.
///
/// Both directions, and the pair is the test. Against the unlabelled world the
/// answer is `None`, which is what makes the executor dispatch and what makes
/// step 8 report a postcondition that does not hold; against the labelled one it
/// is `Some`, which is what stops an already-correct pull request being touched
/// again. A gate that ignored labels would answer `Some` to both.
#[tokio::test]
async fn an_unlabelled_pull_request_is_not_the_labelled_effect_having_happened() {
    let operation = EnsurePullRequest::new(
        REPO.to_string(),
        OWNER.to_string(),
        SHARED_HEAD.to_string(),
        BASE.to_string(),
        SHARED_TITLE.to_string(),
        "opened by fiddle, contents to follow".to_string(),
        false,
    )
    .labelled(vec![CVE_LABEL.to_string()]);

    let unlabelled = Forge::empty();
    unlabelled.seed_pull_request(41, SHARED_HEAD, &[]);
    assert_eq!(
        operation.inspect(&unlabelled.context()).await.unwrap(),
        None,
        "the pull request exists and the postcondition does not hold"
    );

    let labelled = Forge::empty();
    labelled.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);
    let observed = operation
        .inspect(&labelled.context())
        .await
        .unwrap()
        .expect("the same head and base, and this time it carries the label");
    assert_eq!(observed.number, 41);
    assert_eq!(observed.labels, [CVE_LABEL]);
}

/// A label somebody else added does not make the postcondition fail.
///
/// The check is a superset and not an equality, and this is why: a person is
/// entitled to put `needs-triage` on a pull request fiddle opened. An operation
/// that demanded its own labels exactly would find a postcondition it could never
/// satisfy — it would re-apply its labels on every run and still not match — and
/// would report a mismatch that is not one.
#[tokio::test]
async fn a_label_a_person_added_does_not_unsatisfy_the_postcondition() {
    let operation = EnsurePullRequest::new(
        REPO.to_string(),
        OWNER.to_string(),
        SHARED_HEAD.to_string(),
        BASE.to_string(),
        SHARED_TITLE.to_string(),
        "opened by fiddle, contents to follow".to_string(),
        false,
    )
    .labelled(vec![CVE_LABEL.to_string()]);

    let forge = Forge::empty();
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL, "needs-triage"]);

    let observed = operation
        .inspect(&forge.context())
        .await
        .unwrap()
        .expect("ours is there, beside somebody else's");
    assert_eq!(observed.labels, [CVE_LABEL, "needs-triage"]);
}

/// The inversion, and the reason the label is not decoration.
///
/// A create that carried no label really happens — the pull request is in the
/// world and the effect committed — and the discovery read finds **nothing**.
/// The next run therefore proposes a create of its own, which is the second pull
/// request this whole model exists to prevent.
///
/// This is what stops its neighbour above passing for the wrong reason. Without
/// it, a discovery read that ignored the label and answered every open pull
/// request would satisfy that test exactly as well.
#[tokio::test]
async fn a_pull_request_created_without_the_label_is_invisible_to_the_next_run() {
    let forge = Forge::empty();

    let receipt = open_the_shared_pull_request(&forge, SHARED_HEAD, &[]).await;

    assert_eq!(receipt.outcome, EffectOutcome::Committed);
    assert_eq!(
        forge.open_pull_requests(),
        1,
        "the pull request really exists; it is only the label that is missing"
    );
    assert!(
        receipt.value.labels.is_empty(),
        "and it carries none: {:?}",
        receipt.value
    );

    assert!(
        discover(&forge).await.is_none(),
        "an unlabelled pull request is invisible to the discovery read, so the \
         next run would open a second one"
    );
}

/// A pull request already open is worked on, and no second is created.
///
/// The count is over *creates dispatched*, not over the objects the world holds:
/// a run that dispatched a create and had it refused as a duplicate would leave
/// the object count at one and would still have got the model wrong, because the
/// only thing that stopped it was GitHub.
#[tokio::test]
async fn an_existing_open_pull_request_is_reused_and_never_duplicated() {
    let forge = Forge::empty();
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);

    let approved = decide(&forge)
        .await
        .expect("a head under the pushable prefix");

    assert_eq!(approved.reused(), Some(41));
    assert_eq!(
        approved.branch(),
        SHARED_HEAD,
        "the run adds to the pull request's own branch rather than cutting one"
    );
    assert!(
        !approved.branch().contains(TODAY),
        "and it is emphatically not today's fresh branch: {}",
        approved.branch()
    );
    assert!(approved.duplicates().is_empty());
    assert_eq!(forge.creation_requests(), 0, "never a second");
    assert_eq!(forge.open_pull_requests(), 1);
}

/// With nothing open, a dated branch is cut **from an origin ref**.
///
/// The origin half is the one that is easy to lose and the one that has already
/// cost a run: a branch cut from local `HEAD` carries whatever the previous run
/// left in the worktree, and a branch cut from local `main` carries whatever that
/// happened to be fetched to. Design §4 says *never branch from local `HEAD` or
/// local `main`*, and this is where that is held.
#[tokio::test]
async fn with_nothing_open_a_dated_branch_is_cut_from_an_origin_ref() {
    let forge = Forge::empty();

    let approved = decide(&forge)
        .await
        .expect("an empty world plans a fresh cut");

    assert_eq!(approved.reused(), None);
    assert_eq!(approved.branch(), format!("{BRANCH_STEM}{TODAY}"));
    assert!(
        approved.branch().starts_with(PUSHABLE_PREFIX),
        "a branch this capability cuts must satisfy its own push guard: {}",
        approved.branch()
    );
    assert_eq!(
        approved.from(),
        format!("origin/{BASE}"),
        "the remote's base, and never local HEAD or local main"
    );
    assert_eq!(
        forge.creation_requests(),
        0,
        "the create is not this lane's"
    );
}

/// Reuse checks out the **remote** tip of the pull request's branch.
///
/// The same rule as its neighbour above, stated for the other arm, and it is a
/// separate claim rather than a restatement: a fresh cut has no local branch to
/// be tempted by, and a reused one does — a `security/cve-remediation-…` left
/// behind by yesterday's run in the same clone is exactly the stale thing
/// `origin/` excludes.
#[tokio::test]
async fn reuse_names_the_remote_tip_of_the_pull_requests_branch() {
    let forge = Forge::empty();
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);

    let approved = decide(&forge)
        .await
        .expect("a head under the pushable prefix");

    assert_eq!(approved.from(), format!("origin/{SHARED_HEAD}"));
    assert!(!approved.from().contains("HEAD"), "{}", approved.from());
}

/// **Several open pull requests: the lowest, and the rest noted.**
///
/// Seeded 57, 41, 63 — arrival order and numeric order deliberately disagreeing,
/// because that is the only arrangement in which *lowest*, *first* and *last*
/// give three different answers. Taking the first would answer 57 and taking the
/// last would answer 63.
///
/// The note is the second half of the criterion and is not decoration: GitHub
/// will not create a second pull request for one head and base, so more than one
/// open labelled pull request is something a person did, and a person is who
/// closes the extras. A run that quietly picked one and said nothing would leave
/// the anomaly to be discovered by whoever eventually merged the wrong one.
#[tokio::test]
async fn several_open_pull_requests_take_the_lowest_and_note_the_rest() {
    let forge = Forge::empty();
    for number in [57u64, 41, 63] {
        forge.seed_pull_request(
            number,
            &format!("{BRANCH_STEM}2026080{number}"),
            &[CVE_LABEL],
        );
    }

    let approved = decide(&forge).await.expect("every head is pushable here");

    assert_eq!(approved.reused(), Some(41), "the lowest, not the first");
    assert_eq!(
        approved.duplicates(),
        [57, 63],
        "ascending, and both of them"
    );

    let note = approved
        .note()
        .expect("an anomaly a person made is an anomaly a person is told about");
    assert!(
        note.contains("57") && note.contains("63"),
        "the note must name the ones a person has to close: {note}"
    );
    assert!(
        !note.contains("41"),
        "and must not name the one being reused, which is not a duplicate: {note}"
    );
    assert_eq!(forge.creation_requests(), 0, "and never another");
}

/// One pull request is not an anomaly, so there is nothing to note.
///
/// The negative half of the test above. Without it, a `note()` that returned the
/// same paragraph however many pull requests were open would satisfy that one and
/// would put an anomaly warning on every ordinary run's body.
#[tokio::test]
async fn one_open_pull_request_is_not_an_anomaly_and_is_not_noted() {
    let forge = Forge::empty();
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);

    assert_eq!(decide(&forge).await.unwrap().note(), None);
}

/// A label search answered wider than it was asked is narrowed by this client.
///
/// The same rule `EnsurePullRequest::read` applies to the pull-request listing,
/// and it is here for the same reason: the filtering is GitHub's and confirming
/// that what came back is what was asked for is this client's. What stands in for
/// a widened answer is anything between here and GitHub — a proxy, a cached page,
/// a parameter that stops being honoured.
///
/// The unlabelled pull request is the *lower* number, so a client that trusted
/// the server's filter would settle on it and not on the one carrying the label.
#[tokio::test]
async fn a_label_search_answered_without_its_filter_is_narrowed_here() {
    let forge = Forge::empty();
    forge.seed_pull_request(12, "feature/somebody-elses-work", &[]);
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);
    forge.answer_the_label_search_without_filtering();

    let found = discover(&forge)
        .await
        .expect("one pull request carries the label");

    assert_eq!(
        found.number, 41,
        "the unlabelled #12 is lower and must still not be settled on"
    );
    assert!(found.duplicates.is_empty());
}

/// A **closed** pull request carrying the label is not the one to work in.
///
/// The other half of what the query asks for, re-checked here for the reason the
/// label half is. A closed pull request is a branch that has been merged or
/// abandoned: committing onto it puts a bump on history the base already
/// carries, or on a branch the remote has deleted — and the merged case is the
/// worse one, because the push succeeds.
///
/// #12 is again the *lower* number, so a client that took the lowest of whatever
/// came back would settle on it.
#[tokio::test]
async fn a_closed_pull_request_carrying_the_label_is_not_settled_on() {
    let forge = Forge::empty();
    forge.seed_pull_request_in_state(
        12,
        "security/cve-remediation-20260701",
        &[CVE_LABEL],
        "closed",
    );
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);
    forge.answer_the_label_search_without_filtering();

    let found = discover(&forge).await.expect("the open one is still found");

    assert_eq!(found.number, 41, "#12 is closed and merged or abandoned");
    assert!(
        found.duplicates.is_empty(),
        "and a closed pull request is not an anomaly to report: {:?}",
        found.duplicates
    );
}

/// A plain issue carrying the label is not a pull request and is not taken.
///
/// GitHub's label search is over *issues*, and a pull request is an issue there —
/// which is why the search finds one at all, and why it also finds things that
/// are not. `security/cve` on an ordinary issue is entirely plausible; a reader
/// that took the lowest number of whatever came back would settle on an object
/// with no head branch, no base and nothing to commit onto.
#[tokio::test]
async fn a_plain_issue_carrying_the_label_is_not_the_shared_pull_request() {
    let forge = Forge::empty();
    forge.seed_issue(9, &[CVE_LABEL]);
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);

    let found = discover(&forge)
        .await
        .expect("the pull request is still found");

    assert_eq!(found.number, 41, "#9 is an issue, not a pull request");
    assert!(
        found.duplicates.is_empty(),
        "and it is not an anomaly either: {:?}",
        found.duplicates
    );
}

// ---------------------------------------------------------------------------
// The tree the attempt runs in, and every mutation that leaves this process
// ---------------------------------------------------------------------------

/// Everything one whole run of this half left behind.
///
/// Assembled by [`publish`] and read by the three lanes below. Every field is
/// something *observed* after the fact — the worktree's own `HEAD`, the journal
/// on disk, the receipts' values — rather than something the driver was told.
struct Published {
    /// Which branch the run settled on, and how.
    approved: Approved,
    /// The two revisions it saw and which it used.
    checkout: Checkout,
    /// What the worktree the attempt ran in was actually sitting on, asked of
    /// git after the checkout and before anything was committed.
    worktree_head: String,
    /// The branch, its observed head and the one pull request.
    work: SharedWork,
    /// Every commit body the worktree held before the landing, and after it.
    /// The pair is what says the landing really committed.
    history_before_landing: String,
    history_after_landing: String,
    /// How many records the attempt's journal gained while the landing ran.
    journal_grew_across_the_landing: usize,
    /// The journal itself, read back off disk.
    journal: Journal,
    /// Held so that the journal's file outlives this value.
    _reports: TempDir,
}

impl Published {
    /// The bundle this run would publish, with the checkout's three keys under
    /// `observations`.
    ///
    /// **Assembled here rather than read off a `ReportBundle`, and that is a
    /// statement about scope rather than a shortcut.** `observations` in a
    /// published bundle is `fiddle_core::WorkStateView` — a closed set of four
    /// named ports belonging to M0's assessment — and there is no slot in it for a
    /// capability's own facts. Widening it, and the `fiddle-cli` rendering that
    /// serializes it, is the wiring task's; what a run *produces* for those three
    /// keys is this one's, and [`Checkout::observed`] is the whole of it. A lane
    /// that asserted against a bundle this milestone cannot yet publish would be
    /// asserting against a shim.
    fn bundle(&self) -> serde_json::Value {
        serde_json::json!({ "observations": self.checkout.observed() })
    }

    /// Did the landing's commit stay out of the effect journal?
    ///
    /// Two halves and neither is enough. The commit must have *happened* — a run
    /// that committed nothing would satisfy "no effect was recorded for it" for
    /// the wrong reason — and the journal must not have grown while it did.
    fn local_commits_are_not_effects(&self) -> bool {
        self.history_after_landing != self.history_before_landing
            && self.journal_grew_across_the_landing == 0
    }
}

/// The whole of what this half does, in the order it does it.
///
/// Written out here rather than hidden behind a helper per step, because the
/// order *is* part of what the lanes below are about: discover, decide, fetch and
/// check out, land, and only then publish. `?`-free and panicking on the way,
/// because every failure here is a fixture failure — the refusal arm has its own
/// driver, [`discover_then_land`], one section down.
///
/// **One driver and not two.** The plan's sketch had `publish` and
/// `publish_fresh`; which arm is taken is a property of the world the forge holds
/// rather than of the driver, so two would be one function called twice with the
/// difference written in the wrong place.
async fn publish(forge: &Forge, world: &RemoteWorld) -> Published {
    let cancel = CancellationToken::new();
    let attempt = AttemptId("01JCVEPUBLISH0000000000000".to_string());
    let reports = TempDir::new().expect("a temporary directory for the attempt journal");
    let journal = forge.journalling(reports.path(), &attempt);

    let approved = plan_shared_pull_request(&forge.gh(), REPO, BASE, TODAY, &cancel)
        .await
        .expect("a head under the pushable prefix");

    // The fetch and the two revisions, run in the clone the worktrees are branched
    // from — there is no worktree yet, and which revision it is made at is what
    // this answers.
    let checkout = check_out(&world.tree, &approved)
        .await
        .expect("the remote holds the refs this run named");

    let root = TempDir::new().expect("a temporary directory for the worktree");
    let workspace = Workspace::create_at(
        world.tree.path(),
        root.path(),
        &attempt,
        checkout.revision(),
        cancel.clone(),
    )
    .expect("a worktree at the revision the checkout named");
    let worktree_head = ask_git(workspace.root(), &["rev-parse", "HEAD"]);
    let history_before_landing = ask_git(workspace.root(), &["log", "--format=%B"]);

    let changed = world.bump_into(workspace.root());
    let before = journal.records().len();
    land(
        &InWorktree::new(&workspace, PATIENT),
        &world.group,
        &GroupStatus::Clean,
        &changed,
    )
    .await
    .expect("a clean group over a tree that really changed");
    let journal_grew_across_the_landing = journal.records().len() - before;
    let history_after_landing = ask_git(workspace.root(), &["log", "--format=%B"]);
    let landed = ask_git(workspace.root(), &["rev-parse", "HEAD"]);

    let deployment = Deployment(fiddle_core::DeploymentRule::Allow);
    let ctx = forge.context_pushing_from(workspace.root());
    let executor = Executor::new(
        FIXTURE_REPAIR,
        PROJECT.to_string(),
        INVOCATION_REF.to_string(),
        &deployment,
        &ctx,
        forge,
        ReadRetry::none(),
    );
    let work = publish_shared_work(
        &executor,
        FIXTURE_REPAIR,
        &approved,
        &SharedPublication {
            repo: REPO.to_string(),
            head_owner: OWNER.to_string(),
            title: SHARED_TITLE.to_string(),
            summary: RUN_SUMMARY.to_string(),
            head_sha: landed,
        },
    )
    .await
    .expect("a branch this capability may push to, and one pull request");

    Published {
        approved,
        checkout,
        worktree_head,
        work,
        history_before_landing,
        history_after_landing,
        journal_grew_across_the_landing,
        journal,
        _reports: reports,
    }
}

/// **Reuse runs in the pull request's remote tip, and the bundle says so.**
///
/// Three claims, and the world is built so that each of them has a wrong answer
/// available to be caught. [`remote_world`] leaves a *stale local branch of the
/// same name* in the clone, pointing at a different commit, and a local `main`
/// the remote has never seen — so a checkout by branch name and a checkout from
/// local `HEAD` both land somewhere this lane can name.
///
/// The head sha is compared against what the **remote** holds rather than against
/// a fixture constant: the forge reports it out of `remote.git` and the worktree
/// resolves it through the fetch, which makes the agreement evidence rather than
/// one value compared with itself.
///
/// The observations are the second half and they are a separate claim. Design §4:
/// *the observation carries the base revision **and** the open PR's head, and the
/// bundle says which of the two the attempt actually ran against.* A run that
/// recorded only the one it used would satisfy the first assertion here and fail
/// the second, which is the point of there being two.
#[tokio::test]
async fn reusing_a_pull_request_checks_out_its_remote_tip_and_records_both_revisions() {
    let forge = Forge::empty();
    let world = remote_world(&forge.remote(), Some(SHARED_HEAD), &[LANDED_CVE]);
    forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);
    // Read **before** the run, because the run pushes onto this very branch: the
    // tip afterwards is the commit the landing added, and a lane that read it
    // then would be comparing the worktree against its own output.
    let tip = forge.pr(41).head_sha;

    let out = publish(&forge, &world).await;

    assert_eq!(out.approved.reused(), Some(41));
    assert_eq!(
        out.worktree_head, tip,
        "the remote tip, never a local branch left by an earlier run"
    );
    // The two wrong answers, named. Without these the assertion above would hold
    // in a clone whose local refs happened to agree with the remote's, which is
    // every clone until the day it is not.
    assert_ne!(
        out.worktree_head,
        world
            .stale_head
            .clone()
            .expect("this world has a stale local branch of the same name"),
        "checking the branch out by name would land here"
    );
    assert_ne!(
        out.worktree_head, world.stale_main,
        "and branching from local HEAD would land here"
    );
    // A second, independent witness that is not a sha at all: the file only the
    // shared branch carries.
    assert!(
        workspace_holds(&world, &out.worktree_head, ON_THE_SHARED_BRANCH),
        "the tree the attempt ran in has to be the shared branch's tree"
    );

    let obs = out.bundle()["observations"].clone();
    assert!(
        obs["base_revision"].is_string() && obs["pr_head"].is_string(),
        "both are observed; the bundle says which the attempt ran against: {obs}"
    );
    assert_eq!(obs["attempt_tree"], "pr_head");
    assert_eq!(obs["pr_head"], out.worktree_head, "{obs}");
    assert_eq!(
        obs["base_revision"], world.base_revision,
        "the base is observed on this arm too, and it is the remote's: {obs}"
    );
    assert_ne!(
        obs["base_revision"], obs["pr_head"],
        "a world in which the two coincided could not tell them apart: {obs}"
    );

    // And the publication that followed added to the pull request it reused.
    // Stated here as well as in the read-only lane because this is the arm where
    // a create really could have been dispatched and was not: the executor's own
    // step 3 found the postcondition holding, which is what makes *never a
    // second* idempotence rather than a branch somebody has to keep correct.
    assert_eq!(out.work.pull_request, 41);
    assert_eq!(forge.creation_requests(), 0, "never a second");
    assert_eq!(forge.open_pull_requests(), 1);
    assert_eq!(
        out.work.head_sha,
        ask_git(
            &forge.remote(),
            &["rev-parse", &format!("refs/heads/{SHARED_HEAD}")]
        ),
        "the receipt carries what the remote was observed to hold, and the branch \
         has moved on from {tip} to the commit the landing added"
    );
}

/// **Every external mutation passes the effect executor.**
///
/// M2's invariant: [`AuthorizedEffect`] has no constructor outside
/// `effect/mod.rs`, so this asserts *routing* rather than merely behaviour — what
/// it excludes is a mutation reaching the forge without a recorded effect step.
///
/// [`AuthorizedEffect`]: fiddle_runtime::effect::AuthorizedEffect
///
/// # Why it is a window and not a count
///
/// The obvious form is `mutations().len() == applies().len()`, and it is wrong in
/// both directions. One effect here dispatches *two* requests — the create and
/// then the label on the object it created — so the equality is false for a
/// correct run; and two zeros are equal, so it is true for a run that did
/// nothing. What the constraint is actually about is *which step each mutation
/// happened under*, so each is attributed to the apply window it fell inside, and
/// what the lane reports is the ones that fell in none.
///
/// # Both channels, because there are two
///
/// A pull request create is an HTTP request the scripted `gh` writes down. A
/// branch publication is a `git push`, which leaves nothing at the forge's API at
/// all — it is visible only as a ref the remote did not have before. A lane that
/// checked the request log alone would report a clean routing for a build that
/// pushed from anywhere it liked.
///
/// # And the negative half
///
/// A local commit is not an external mutation and is deliberately not journaled
/// as one. That is asserted with its premise attached: the landing must really
/// have committed, or "no effect was recorded for it" holds for the wrong reason.
#[tokio::test]
async fn every_external_mutation_passes_the_effect_executor() {
    let forge = Forge::empty();
    let world = remote_world(&forge.remote(), None, &[LANDED_CVE]);

    let out = publish(&forge, &world).await;

    let kinds = out.journal.effect_steps_kinds();
    assert!(
        kinds.contains(&EffectKind::EnsureBranchPublished),
        "the journal must name the branch effect: {kinds:?}"
    );
    assert!(
        kinds.contains(&EffectKind::EnsurePullRequest),
        "and the pull request effect: {kinds:?}"
    );
    // The premise for everything below: both really *applied*, so there were
    // windows to attribute mutations to. A walk that found every postcondition
    // already satisfied would leave `kinds` above populated and every window
    // absent, and the routing claim would then be a claim about nothing.
    let applied = out.journal.kinds_that_applied();
    assert!(
        applied.contains(&EffectKind::EnsureBranchPublished)
            && applied.contains(&EffectKind::EnsurePullRequest),
        "both effects had work to do in an empty world: {applied:?}"
    );

    // Premise two: mutations really reached the forge. Without it the assertion
    // that follows is satisfied by a run that dispatched nothing.
    let mutations = forge.mutations();
    assert!(
        !mutations.is_empty(),
        "no request that could change the forge was dispatched at all, so this \
         lane is measuring nothing"
    );
    assert_eq!(
        forge.mutations_outside_an_effect(),
        Vec::<String>::new(),
        "these reached the forge outside any effect's apply window; every one of \
         the {} dispatched must fall inside one",
        mutations.len()
    );

    // The other channel. The branch is on the remote, and it got there during the
    // branch effect's apply — not before it, and not after the walk had finished.
    assert_eq!(
        forge.branches_gained_during(EffectKind::EnsureBranchPublished),
        [out.approved.branch().to_string()],
        "the push must have happened inside the branch effect's apply window; the \
         remote's branches are now {:?}",
        forge.remote_branches()
    );

    assert!(
        out.local_commits_are_not_effects(),
        "a local commit is not an external mutation and is deliberately not \
         journaled as one — the landing committed ({} record(s) added)",
        out.journal_grew_across_the_landing
    );
    assert!(
        out.history_after_landing.contains(LANDED_CVE),
        "and it is *this group's* commit rather than any commit at all: {}",
        out.history_after_landing
    );
    // And the whole of what was published is the one branch and the one pull
    // request, so "every mutation is accounted for" is a claim about a run that
    // really did something.
    assert_eq!(out.work.branch, out.approved.branch());
    assert_eq!(forge.creation_requests(), 1, "one create, and only one");
    assert_eq!(forge.open_pull_requests(), 1);
}

/// **With nothing open, the branch is dated and cut from the remote.**
///
/// The fresh arm of the checkout, driven rather than planned:
/// `with_nothing_open_a_dated_branch_is_cut_from_an_origin_ref` asserts what
/// [`Approved::from`] *names*, and this asserts what the run then *ran* — the
/// fetch it made and the tree it ended up in.
///
/// The recorded git calls are the subject's own, because [`remote_world`]
/// deliberately does not record its construction: a list holding the fixture's
/// `clone` and `commit` would make "the subject named origin/main" an assertion
/// about what this file did.
#[tokio::test]
async fn a_fresh_branch_is_cut_from_the_remote_and_is_dated() {
    let forge = Forge::empty();
    let world = remote_world(&forge.remote(), None, &[LANDED_CVE]);

    let out = publish(&forge, &world).await;

    assert!(
        out.approved.branch().starts_with(BRANCH_STEM),
        "{}",
        out.approved.branch()
    );
    assert!(out.approved.branch().ends_with(TODAY), "and dated");

    let calls = world.tree.git_calls();
    assert!(
        calls.iter().any(|call| call.contains("origin/main")),
        "never local HEAD or local main; a stale local main contaminated a prior \
         run. The subject ran: {calls:?}"
    );
    // And it is not merely *mentioned*: the tree the attempt ran in is the
    // remote's base and neither of the clone's own commits.
    assert_eq!(out.worktree_head, world.base_revision);
    assert_ne!(
        out.worktree_head, world.stale_main,
        "branching from local main would land here"
    );
    assert!(
        workspace_holds(&world, &out.worktree_head, ONLY_ON_THE_REMOTE_BASE),
        "the base moved on after the clone was taken, and the attempt has to be \
         standing on the commit that moved it"
    );

    // The fresh arm's observations, which are the other half of Design §4's
    // sentence: there is no pull request, and the bundle says so rather than
    // leaving a reader to guess from a missing key.
    let obs = out.bundle()["observations"].clone();
    assert_eq!(obs["attempt_tree"], "base_revision");
    assert_eq!(obs["base_revision"], world.base_revision);
    // `get` and not `obs["pr_head"]`, because indexing a JSON object with a key
    // it does not hold answers `Null` — so the obvious spelling would pass for a
    // run that wrote no such key at all, which is precisely the *absent versus
    // null* distinction [`Checkout::observed`] is written to keep. Measured: a
    // probe that recorded only the revision it used left this lane green.
    assert!(
        obs.get("pr_head").is_some_and(|it| it.is_null()),
        "no pull request was open, and the bundle has to say so rather than \
         leaving a reader to read a missing key as an old build: {obs}"
    );
}

/// Whether the commit `revision` carries `path`.
///
/// Asked of the clone rather than of the worktree, which no longer exists by the
/// time a lane reads a [`Published`] — the workspace's `Drop` removed it, which is
/// the invariant `no_worktree_survives_the_attempt` is about. The commit is still
/// in the store, because a worktree shares the object store it was branched from.
fn workspace_holds(world: &RemoteWorld, revision: &str, path: &str) -> bool {
    ask_git(
        world.tree.path(),
        &["ls-tree", "--name-only", revision, path],
    )
    .lines()
    .any(|line| line == path)
}

// ---------------------------------------------------------------------------
// The push guard, and why it comes first
// ---------------------------------------------------------------------------

/// **A head outside the pushable prefix is refused, and refused as a value.**
///
/// The failure being prevented is an ordering failure: a run that checked out
/// `feature/not-security`, committed a bump onto it and *then* discovered it may
/// not push there has already written to a branch somebody else owns, and has to
/// be unwound by hand.
///
/// It is refused rather than worked around, because there is no safe workaround.
/// Opening a second pull request is the thing this model exists to prevent, and
/// pushing anyway is the thing the prefix exists to prevent.
#[tokio::test]
async fn a_head_branch_outside_the_pushable_prefix_stops_before_committing() {
    let forge = Forge::empty();
    forge.seed_pull_request(41, "feature/not-security", &[CVE_LABEL]);

    let refused = decide(&forge)
        .await
        .expect_err("a head outside the pushable prefix is not something to work around");

    assert!(
        matches!(
            &refused,
            PlanError::Refused(Refusal::HeadOutsideThePushablePrefix { number: 41, .. })
        ),
        "{refused}"
    );
    // Both halves in the message, because an operator reading it has to know
    // which branch was refused *and* what would have been acceptable. A refusal
    // naming only one of them sends them to the source.
    let said = refused.to_string();
    assert!(said.contains("feature/not-security"), "{said}");
    assert!(said.contains(PUSHABLE_PREFIX), "{said}");
    assert!(said.contains("41"), "{said}");
}

/// The refusal reaches the run **before** anything is committed.
///
/// Two worlds and one driver. The driver is the order a run works in — discover
/// the shared pull request, decide which branch to work on, and only then land a
/// group onto a tree — and the only difference between the two worlds is the head
/// branch of the pull request that is already open.
///
/// Both assertions are needed and neither is enough. The refused world's tree
/// must be untouched, which is the criterion; the pushable world's tree must be
/// *committed to* by the same driver, which is what says the untouched tree is
/// untouched because of the guard rather than because the driver never commits
/// anything. That second half is the one an inversion moves: put the guard after
/// the landing and the refused world grows a commit.
#[tokio::test]
async fn the_prefix_refusal_reaches_the_run_before_any_commit() {
    let refused_world = landing_world(&["CVE-2026-4242"]);
    let refused_forge = Forge::empty();
    refused_forge.seed_pull_request(41, "feature/not-security", &[CVE_LABEL]);
    let before = refused_world.tree.all_commit_bodies();

    let outcome = discover_then_land(&refused_forge, &refused_world).await;

    assert!(matches!(outcome, Err(PlanError::Refused(_))), "{outcome:?}");
    assert_eq!(
        refused_world.tree.all_commit_bodies(),
        before,
        "the history must be exactly what it was, so nothing was committed"
    );
    assert!(
        refused_world.tree.git_calls().is_empty(),
        "and no git command ran at all: {:?}",
        refused_world.tree.git_calls()
    );

    // The same driver, the same group, the same tree shape — and a head branch
    // this capability may push to.
    let allowed_world = landing_world(&["CVE-2026-4242"]);
    let allowed_forge = Forge::empty();
    allowed_forge.seed_pull_request(41, SHARED_HEAD, &[CVE_LABEL]);
    let before = allowed_world.tree.all_commit_bodies();

    discover_then_land(&allowed_forge, &allowed_world)
        .await
        .expect("a pushable head lets the landing run");

    assert_ne!(
        allowed_world.tree.all_commit_bodies(),
        before,
        "so the driver really does commit when the guard lets it"
    );
    assert!(
        allowed_world
            .tree
            .head_commit_body()
            .contains("CVE-2026-4242"),
        "and it is this group's commit: {}",
        allowed_world.tree.head_commit_body()
    );
}

/// The order a run works in: decide first, commit second.
///
/// Written out here rather than hidden inside a helper, because the order *is*
/// the subject. `?` on the plan is what makes the landing below it unreachable
/// after a refusal — there is no branch to commit onto, because [`Approved`] is
/// the only value that names one.
async fn discover_then_land(forge: &Forge, world: &LandingWorld) -> Result<Approved, PlanError> {
    let approved =
        plan_shared_pull_request(&forge.gh(), REPO, BASE, TODAY, &CancellationToken::new()).await?;

    land(
        &world.tree,
        &world.group,
        &fiddle_runtime::capability::GroupStatus::Clean,
        &world.changed,
    )
    .await
    .expect("a clean group over a tree that really changed");

    Ok(approved)
}

/// The prefix and the branch this capability cuts cannot drift apart.
///
/// Two constants and one convention. A pushable prefix that stopped matching the
/// stem would refuse every branch this capability cut for itself, which is a
/// failure the discovery lanes above could not see: they arrange a *reused*
/// branch, and a fresh cut is the arm they never take.
#[test]
fn the_branch_this_capability_cuts_satisfies_its_own_push_guard() {
    assert!(
        BRANCH_STEM.starts_with(PUSHABLE_PREFIX),
        "{BRANCH_STEM} is not under {PUSHABLE_PREFIX}"
    );
    // And the values themselves, once, so neither can be quietly changed to
    // something that still satisfies the relation above and means nothing:
    // `x/` and `x/y-` would pass it.
    assert_eq!(PUSHABLE_PREFIX, "security/");
    assert_eq!(BRANCH_STEM, "security/cve-remediation-");
    assert_eq!(CVE_LABEL, "security/cve");
}

/// The pure decision, stated without a forge.
///
/// `plan` is the half that refuses, and it takes no client, no context and no
/// `Git`. That is the structural half of *before any commit*: the value it
/// answers on a refusal names no branch, so there is nothing for a checkout or a
/// commit to be addressed at.
#[test]
fn the_decision_is_taken_over_the_observation_alone() {
    use fiddle_runtime::github::SharedPullRequest;

    let outside = SharedPullRequest {
        number: 41,
        head: "feature/not-security".to_string(),
        head_sha: A_TIP.to_string(),
        base: BASE.to_string(),
        title: SHARED_TITLE.to_string(),
        duplicates: Vec::new(),
    };
    assert!(matches!(
        plan(Some(outside), BASE, TODAY),
        Err(Refusal::HeadOutsideThePushablePrefix { .. })
    ));

    let inside = SharedPullRequest {
        number: 41,
        head: SHARED_HEAD.to_string(),
        head_sha: A_TIP.to_string(),
        base: BASE.to_string(),
        title: SHARED_TITLE.to_string(),
        duplicates: vec![57, 63],
    };
    let approved = plan(Some(inside), BASE, TODAY).expect("a pushable head");
    assert_eq!(approved.reused(), Some(41));
    assert_eq!(approved.duplicates(), [57, 63]);
    assert_eq!(
        approved.pr_head(),
        Some(A_TIP),
        "the tip the observation named is carried through, because it is what the \
         attempt's tree is made at"
    );

    let fresh = plan(None, BASE, TODAY).expect("nothing open is not a refusal");
    assert_eq!(fresh.reused(), None);
    assert_eq!(fresh.branch(), format!("{BRANCH_STEM}{TODAY}"));
    assert_eq!(
        fresh.pr_head(),
        None,
        "there is no pull request, so there is no head for one to have"
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
