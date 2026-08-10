//! M2's mandatory automated proof: **exactly one branch, one pull request and
//! one requested check across a lost answer**, offline, credential-free, and
//! gating.
//!
//! This is the RFC's own sentence, asserted against the compiled binary:
//!
//! > *A disposable GitHub repository receives exactly one branch and pull
//! > request. Failure injection after an ambiguous write followed by a
//! > fresh-process retry proves that no branch, PR, or check request is
//! > duplicated.*
//!
//! Task 12's live lane repeats it against real GitHub and never gates, so this
//! is the lane that has to be right.
//!
//! # The fault is real, and it is real in the only way that proves anything
//!
//! Every ambiguity here is produced by a fixture that **applies the mutation and
//! then dies**, so the process that comes after meets a world that genuinely
//! changed through a request whose answer was genuinely lost. Calling the same
//! function twice would exercise read-before-write and would say nothing at all
//! about a dropped response.
//!
//! Both halves of that are *observed* rather than arranged and hoped for:
//!
//! - the scripted `gh` records the mode beside every mutation it applies, so a
//!   scenario asserts, of the world it makes its claims about, that the write
//!   landed under a `gh` that then died before it could say so;
//! - the recording `git` writes `pushed_then_died` between the push and the
//!   abort, so a scenario asserts the ref landed *before* the answer was lost
//!   rather than instead of it.
//!
//! Without those two, every assertion below would still pass on a world where no
//! fault had ever fired — which is the difference between proving exactly-once
//! and proving nothing.
//!
//! # Why all three objects, and why the check matters most
//!
//! `git push` to a named ref is already idempotent, and GitHub already refuses a
//! second pull request for the same head and base. A proof that demonstrated
//! exactly-once only where GitHub was going to provide it anyway would have
//! demonstrated GitHub's property rather than fiddle's. The **check request** is
//! the object nothing out there protects: a workflow dispatch answers `204` with
//! no run id, and the runs listing does not expose dispatch inputs, so the only
//! thing standing between an interrupted attempt and an unbounded supply of
//! workflow runs is that fiddle recomputes the run's name from canonical inputs
//! and looks for it. Each of the three is injected at, in turn.
//!
//! # Offline and credential-free, which is the point rather than a constraint
//!
//! `[github] cli = { program, args }` and `[github] git` are the product seams an
//! operator uses to pin or wrap the two programs; these scenarios point them at
//! `fiddle-runtime`'s own scripted fixtures, reached through
//! [`support::gh_stub_binary`] and [`support::git_stub_binary`]. The "remote" is
//! a bare git repository in a temporary directory. Nothing here reaches a
//! network, and the token these runs export authenticates nothing — it exists so
//! that the *resolution* of the credential is exercised, and it is asserted to
//! reach no observable surface.
//!
//! # What is deliberately not here
//!
//! The plan's Task 11 also asked for a `publish_selection.rs` carrying
//! `inspect_names_the_capability_without_building_it` and
//! `run_refuses_the_capability_when_its_credential_is_absent`. Both already exist,
//! written by Task 10 as
//! `github_deployment::inspect_names_the_publishing_capability_without_building_it`
//! and `github_deployment::a_publication_without_its_credential_names_the_variable`,
//! and both are stronger there than the sketch: the first asserts the report
//! directory stays absent, and the second pins exit 2 as well as the variable
//! name. They are folded rather than duplicated — two files asserting one
//! property drift, and the weaker copy is the one that goes stale.

mod support;

use std::path::{Path, PathBuf};
use support::Scenario;

/// The work this milestone's scenarios are about, and the reference that
/// addresses it. Every effect identity below is derived from this pair and the
/// project name, and from nothing else — which is what a fresh process
/// recomputes.
const WORK_ID: &str = "fiddle-m0-demo";
const INVOCATION_REF: &str = "beans:fiddle-m0-demo";

/// The variable every document below names. Never a value.
const CREDENTIAL: &str = "FIDDLE_GITHUB_TOKEN";

/// What is exported as that credential: a string that authenticates nothing, and
/// that must appear on no surface. The second sentinel of the suite, beside
/// `capability_selection.rs`'s `LITELLM_API_KEY` one.
const SENTINEL: &str = "ghp_m2_sentinel_must_never_be_printed_4b71";

/// The repository the scripted `gh` answers for, and the workflow a check is
/// requested from.
const REPO: &str = "peel/r";
const WORKFLOW: &str = "verify.yml";
const BASE: &str = "main";

/// Which of the three objects a scenario makes ambiguous.
///
/// A closed enum rather than the sketch's `&str`, so a scenario cannot ask for an
/// object this file does not arrange and silently get the well-behaved world
/// instead — which would be a passing test of nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Object {
    Branch,
    PullRequest,
    Check,
}

impl Object {
    const ALL: [Object; 3] = [Object::Branch, Object::PullRequest, Object::Check];

    fn as_str(self) -> &'static str {
        match self {
            Object::Branch => "branch",
            Object::PullRequest => "pull_request",
            Object::Check => "check",
        }
    }
}

// ---------------------------------------------------------------------------
// The scripted world
// ---------------------------------------------------------------------------

/// One disposable project, one bare "GitHub", and the two scripted programs the
/// `[github]` table points at.
///
/// Every count it answers is read out of the world rather than out of a report:
/// the refs a real bare repository holds, the mutations the scripted `gh` wrote
/// down, the pushes the recording `git` logged. A bundle field saying "one pull
/// request" is fiddle's opinion; these are the world's.
struct ScriptedWorld {
    scenario: Scenario,
    /// The scripted `gh`'s scratch directory: its script, its request log, its
    /// world log, and the bare repository it answers ref reads out of.
    stub: PathBuf,
    /// The bare repository that stands in for the remote. A ref can only be
    /// created pointing at an object the remote already holds, so this is a real
    /// repository reached by a real `git push` — the scripted `gh` mirrors it
    /// rather than modelling it.
    remote: PathBuf,
    /// The worktree whose `HEAD` is published, and the only channel the
    /// recording `git` has for its mode and its records.
    work: PathBuf,
}

impl ScriptedWorld {
    /// A project with one open work item, one commit to publish, and an empty
    /// remote.
    fn new() -> Self {
        let scenario = Scenario::new();
        scenario.write_work_item(WORK_ID, "open");
        let work = scenario.write_fixture_repo();

        let stub = scenario.dir().join("gh-stub");
        std::fs::create_dir_all(stub.join("script")).unwrap();
        // Empty, and it stays empty: it is what a real `gh` would be pinned to,
        // and it is what makes the operator's keyring unreachable.
        std::fs::create_dir_all(stub.join("config")).unwrap();

        // `remote.git` beside the scratch directory is the name the scripted `gh`
        // looks for; see `fiddle-runtime/tests/gh_stub/gh_stub.rs`.
        let remote = stub.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare", "."]);
        git(
            &work,
            &["remote", "add", "origin", &remote.display().to_string()],
        );

        let world = ScriptedWorld {
            scenario,
            stub,
            remote,
            work,
        };
        world.scenario.append_config(&world.forge_table());
        world
    }

    /// The `[github]` table these scenarios run against.
    ///
    /// `config_dir` is written down rather than left to its default, because the
    /// default is relative to the working directory and a scenario that took it
    /// would create a scratch directory inside the package it is run from. Every
    /// path here lives inside the scenario, so a scenario leaves nothing behind.
    fn forge_table(&self) -> String {
        format!(
            "[github]\n\
             repo = \"{REPO}\"\n\
             base = \"{BASE}\"\n\
             token = {{ env = \"{CREDENTIAL}\" }}\n\
             cli = {{ program = {gh}, args = [\"--stub-dir\", {stub}] }}\n\
             git = {git}\n\
             work = {work}\n\
             workflow = \"{WORKFLOW}\"\n\
             config_dir = {config_dir}\n\
             timeout = \"120s\"\n",
            gh = toml_path(support::gh_stub_binary()),
            stub = toml_path(&self.stub),
            git = toml_path(support::git_stub_binary()),
            work = toml_path(&self.work),
            config_dir = toml_path(&self.stub.join("config")),
        )
    }

    /// Make the mutation at `object` land and then lose its answer **to a
    /// cancellation**, which is the other provenance one ambiguous write has.
    ///
    /// The two are not interchangeable, and the milestone's holistic review found
    /// out how by finding the only one of them the code got right. Both arrive at
    /// the same place — [`crate::process::run_bounded`] killing the child's process
    /// group — but the runtime *chose* the death in one case and had it forced on
    /// it in the other, and only the cancellation is reachable from a `^C`: every
    /// bounded child is the leader of its own process group, so a terminal
    /// interrupt reaches it solely through the token.
    ///
    /// The fixtures here do not end themselves. They apply the mutation, record
    /// that it landed, and then wait to be killed:
    ///
    /// - the **branch** goes through the recording `git`'s `push_then_waits`;
    /// - the **pull request** and the **check** go through the scripted `gh`'s
    ///   `commit_then_wait`.
    ///
    /// So what ends the request is the interrupt this test delivers to the real
    /// binary, through the real handler, on the real token — the production path
    /// the finding named.
    fn make_ambiguous_by_cancellation(&self, object: Object) {
        match object {
            Object::Branch => self.push_mode("push_then_waits"),
            Object::PullRequest => {
                self.push_mode("delegated");
                self.script(&pulls_key(), "201 0 commit_then_wait");
            }
            Object::Check => {
                self.push_mode("delegated");
                self.script(&dispatch_key(), "204 0 commit_then_wait");
            }
        }
    }

    /// The marker a waiting fixture writes *between* the mutation and the wait.
    ///
    /// Waited on rather than slept past: it is the fixture's own record that the
    /// world has already changed, so the interrupt cannot arrive before the write
    /// it is supposed to make ambiguous.
    fn landing_marker(&self, object: Object) -> PathBuf {
        match object {
            Object::Branch => self.work.join("pushed_then_waited"),
            Object::PullRequest | Object::Check => self.stub.join("landed_and_waiting"),
        }
    }

    /// Undo what [`ScriptedWorld::make_ambiguous_by_cancellation`] arranged, so the
    /// retry meets a world that answers.
    ///
    /// The waiting fixtures cannot be left in place the way `recover_from` leaves
    /// the dying ones: a second create against `commit_then_wait` would hang rather
    /// than be seen. The counts are what catches a wrong retry instead — every mode
    /// here still records the mutation it applies, and `pushes` still counts every
    /// push dispatched, so a duplicate shows up as two.
    fn recover_from_cancellation(&self, object: Object) {
        let marker = self.landing_marker(object);
        if marker.exists() {
            std::fs::remove_file(&marker).unwrap();
        }
        match object {
            Object::Branch => self.push_mode("delegated"),
            Object::PullRequest => self.script(&pulls_key(), "201 0 normal"),
            Object::Check => self.script(&dispatch_key(), "204 0 normal"),
        }
    }

    /// Run the publication, wait for the mutation at `object` to land, and then
    /// interrupt the **binary** with a real `SIGINT`.
    ///
    /// This is the one scenario in the suite that drives the signal rather than the
    /// token, and it has to: `main::cancel_on_interrupt` is the production path the
    /// finding named, and a test that cancelled a token directly would prove the
    /// classification without proving anything reaches it. The child is signalled by
    /// pid, so nothing else in this process's group is disturbed — and `gh` and
    /// `git` are in process groups of their own, so the signal reaches them *only*
    /// if the handler cancels the token.
    fn publish_then_interrupt(&self, object: Object) -> std::process::Output {
        let marker = self.landing_marker(object);
        let child = self
            .scenario
            .spawnable_run_command(INVOCATION_REF)
            .args(["--capability", "publish_change", "--json"])
            .env(CREDENTIAL, SENTINEL)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "{}: the fixture never recorded a landed mutation, so there was \
                 nothing to make ambiguous",
                object.as_str()
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        interrupt(child.id());
        child.wait_with_output().unwrap()
    }

    /// Make the mutation at `object` land and then lose its answer.
    ///
    /// One call per object, and each arrangement is the fixture's own
    /// mutate-then-die path rather than a second call to the same code:
    ///
    /// - the **branch** goes through the recording `git`'s `push_then_killed`,
    ///   which delegates the push to a real `git` against the real bare
    ///   repository and *then* aborts, so the process leaves no exit code behind
    ///   at all;
    /// - the **pull request** and the **check** go through the scripted `gh`'s
    ///   `commit_then_die`, which applies the mutation to its world log and
    ///   *then* exits 137.
    ///
    /// The scripts are deliberately left in place for the retry. A second create
    /// or a second dispatch would therefore die the same way and be seen, rather
    /// than quietly succeeding against a world the harness had made safe.
    fn make_ambiguous(&self, object: Object) {
        match object {
            Object::Branch => self.push_mode("push_then_killed"),
            Object::PullRequest => {
                self.push_mode("delegated");
                self.script(&pulls_key(), "201 0 commit_then_die");
            }
            Object::Check => {
                self.push_mode("delegated");
                self.script(&dispatch_key(), "204 0 commit_then_die");
            }
        }
    }

    /// What has to fail for the *run* to end without recording anything, given
    /// that the ambiguity at `object` was correctly resolved by reading the world.
    ///
    /// This is the second half of the RFC's "failure injection", and it is what
    /// makes the retry a real retry: an attempt that resolved its own ambiguity
    /// and then published a bundle has nothing left to prove. What is wanted is
    /// an attempt that **changed the world and recorded nothing about it**, which
    /// is precisely the window `fiddle-runtime`'s journal exists for.
    ///
    /// The injection is placed *after* the ambiguous object in the capability's
    /// own order, so the object under test is always reached:
    ///
    /// - after the **branch**, the pull request create is refused `500`;
    /// - after the **pull request**, the dispatch is refused `500`;
    /// - after the **check** — which is the last effect there is — the
    ///   correlation marker cannot be written, so the capability fails having
    ///   committed all three.
    fn interrupt_after(&self, object: Object) {
        match object {
            Object::Branch => self.script(&pulls_key(), "500 1 normal"),
            Object::PullRequest => self.script(&dispatch_key(), "500 1 normal"),
            Object::Check => self.scenario.make_changes_dir_unwritable(),
        }
    }

    /// Undo exactly what [`ScriptedWorld::interrupt_after`] arranged, and nothing
    /// else: the transient failure is over, and the retry meets a readable world.
    ///
    /// The *ambiguity* is deliberately not undone. So if the retry wrongly
    /// dispatched a second create or a second dispatch, it would meet the same
    /// mutate-then-die fixture and be seen — rather than quietly succeeding
    /// against a world the harness had made safe for it.
    fn recover_from(&self, object: Object) {
        match object {
            Object::Branch => self.script(&pulls_key(), "201 0 normal"),
            Object::PullRequest => self.script(&dispatch_key(), "204 0 normal"),
            Object::Check => self.scenario.make_changes_dir_writable(),
        }
    }

    /// Make the runs listing answer `status` — but only once a dispatch has
    /// landed, so step 3's read still succeeds and the effect is still dispatched.
    ///
    /// This is what makes a lost answer's *classification* observable at all. When
    /// the postcondition read succeeds, step 8 finds the object and reports
    /// `Committed` whatever the dispatch failure was taken to mean, so every count
    /// in this file holds equally of a runtime that classified the lost answer
    /// correctly and one that did not. Only a read that cannot be made forces the
    /// executor to fall back on what the dispatch said.
    fn make_the_settling_read_fail(&self, status: u16) {
        std::fs::write(
            self.stub.join("runs_unreadable_after_a_dispatch"),
            status.to_string(),
        )
        .unwrap();
    }

    fn let_the_settling_read_succeed(&self) {
        let marker = self.stub.join("runs_unreadable_after_a_dispatch");
        if marker.exists() {
            std::fs::remove_file(marker).unwrap();
        }
    }

    /// How one scripted write ends. `<status> <exit> <mode>`.
    fn script(&self, key: &str, spec: &str) {
        std::fs::write(self.stub.join("script").join(key), spec).unwrap();
    }

    /// Which mode the recording `git` runs in. Written into the worktree because
    /// that is the only channel it has: the push environment is pinned to seven
    /// names and its argument vector is asserted exactly elsewhere.
    fn push_mode(&self, mode: &str) {
        std::fs::write(self.work.join("mode"), mode).unwrap();
    }

    /// Point `[github] cli.program` somewhere else, by rewriting the document
    /// rather than appending a second `[github]` table — which would not parse.
    ///
    /// The substitution is checked, so a scenario cannot silently keep the
    /// scripted `gh` and pass against a world it never meant to arrange.
    fn use_gh(&self, gh: &Path) {
        let before = self.scenario.config_text();
        let after = before.replace(
            &format!("program = {}", toml_path(support::gh_stub_binary())),
            &format!("program = {}", toml_path(gh)),
        );
        assert_ne!(
            before, after,
            "the document must name the scripted `gh` for it to be replaced"
        );
        std::fs::write(self.scenario.config_path(), after).unwrap();
    }

    /// Every request the scripted `gh` recorded, in arrival order — argv, request
    /// body and the whole environment the child received.
    fn gh_requests(&self) -> Vec<serde_json::Value> {
        let mut paths = support::walkdir_files(self.stub.join("requests"));
        paths.sort();
        paths
            .iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .filter_map(|text| serde_json::from_str(&text).ok())
            .collect()
    }

    /// `fiddle run <ref> --capability publish_change --json`, with the credential
    /// exported, unjudged.
    fn publish(&self) -> std::process::Output {
        self.publish_with_token(SENTINEL)
    }

    fn publish_with_token(&self, token: &str) -> std::process::Output {
        self.scenario
            .run_command(INVOCATION_REF)
            .args(["--capability", "publish_change", "--json"])
            .env(CREDENTIAL, token)
            .output()
            .unwrap()
    }

    // -- what the world holds ------------------------------------------------

    /// Every branch the bare repository holds. The object count for the first of
    /// the three, read out of a real repository's refs.
    fn branches(&self) -> Vec<String> {
        git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        )
        .lines()
        .map(str::to_string)
        .collect()
    }

    /// Every mutation the scripted `gh` actually applied, in the order it applied
    /// them, whose request key contains `needle`.
    ///
    /// The world log and not the request log: a mutation that was *asked for* and
    /// a mutation that *changed something* are the two numbers a duplicate hides
    /// between, and this is the second.
    fn landed(&self, needle: &str) -> Vec<serde_json::Value> {
        std::fs::read_to_string(self.stub.join("world"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|entry| {
                entry["key"]
                    .as_str()
                    .is_some_and(|key| key.starts_with("POST") && key.contains(needle))
            })
            .collect()
    }

    /// The pull requests that came to exist. The object count for the second of
    /// the three: the scripted `gh` answers every pull-request read out of this
    /// same log, so it *is* the world rather than a description of it.
    fn pull_requests(&self) -> Vec<serde_json::Value> {
        self.landed("pulls")
    }

    /// The workflow runs that came to exist, for the same reason.
    fn workflow_runs(&self) -> Vec<serde_json::Value> {
        self.landed("dispatches")
    }

    /// How many pushes were dispatched, counted from what the pushing `git` wrote
    /// down rather than inferred from the remote.
    ///
    /// The number the branch's duplicate would hide behind: a ref pushed twice
    /// with the same commit leaves a remote that looks identical either way, so
    /// "one branch" alone would not distinguish read-before-write from a retried
    /// mutation. This does.
    fn pushes(&self) -> usize {
        std::fs::read_to_string(self.work.join("pushes"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    /// Did the recording `git` push a ref and *then* die?
    fn push_died_after_landing(&self) -> bool {
        self.work.join("pushed_then_died").exists()
    }

    /// Forget that it did, so the next run's own behaviour is what the marker
    /// reports.
    fn forget_the_push_death(&self) {
        let marker = self.work.join("pushed_then_died");
        if marker.exists() {
            std::fs::remove_file(marker).unwrap();
        }
    }

    /// Every project-relative path whose bytes contain `needle`, anywhere under
    /// the project — published bundles, attempt journals, the fixture root, and
    /// anything a run might have written that nobody thought to look at.
    ///
    /// Paths rather than a concatenation, so a failing assertion names the file
    /// to go and open, and so the two kinds of hit can be told apart: see
    /// [`is_fixture_recording`].
    fn files_holding(&self, needle: &str) -> Vec<String> {
        self.scenario
            .project_tree()
            .into_iter()
            .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(needle))
            .map(|(path, _)| path)
            .collect()
    }
}

/// Whether `path` is one of the two fixtures' recordings of **their own
/// environment**, rather than something fiddle published.
///
/// The scripted `gh` and the recording `git` each write down every variable they
/// were handed, and the credential is one of them by design: that recording is
/// how `github_cli` and `git_publish` assert the environment is exactly five and
/// exactly seven names. So both files necessarily hold the token, and neither is
/// a surface — the child is where the credential is *supposed* to arrive.
///
/// Named as a predicate rather than filtered out inside the search, so the
/// sentinel scan reports every other hit by name and cannot quietly grow an
/// exemption: a leak into a new file fails, loudly, with the path.
fn is_fixture_recording(path: &str) -> bool {
    path.starts_with("gh-stub/requests/")
        || path == "fixture/push.json"
        || path == "fixture/rev-parse.json"
}

impl Drop for ScriptedWorld {
    /// Put back anything a scenario sealed, so the temporary directory can be
    /// removed however the test ended — including on the panic of a failing
    /// assertion, which is when the leak would otherwise happen.
    fn drop(&mut self) {
        if self.scenario.stub_root().join("changes").exists() {
            self.scenario.make_changes_dir_writable();
        }
        if self.scenario.report_dir().exists() {
            self.scenario.make_report_dir_writable();
        }
    }
}

// ---------------------------------------------------------------------------
// Reading a run
// ---------------------------------------------------------------------------

/// The `--json` run payload, with the stderr beside it for a failing assertion
/// to quote.
fn payload_of(out: &std::process::Output) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    serde_json::from_str(&stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {stdout}\nstderr = {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// The evidence the executed capability published, as strings.
fn evidence_of(payload: &serde_json::Value) -> Vec<String> {
    payload["progress"][0]["evidence"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The `effect:<kind>:<effect id>` prefix of the receipt for `kind`, or `None`
/// when this run produced no receipt for it.
///
/// The identity and nothing after it: the outcome and the external reference
/// legitimately differ between an attempt that created an object and one that
/// recognised it, while the identity must not — it is derived from canonical
/// inputs alone, so two processes over the same work compute the same one or the
/// exactly-once property has no mechanism at all.
fn effect_identity(payload: &serde_json::Value, kind: &str) -> Option<String> {
    evidence_of(payload)
        .into_iter()
        .find(|reference| reference.starts_with(&format!("effect:{kind}:")))
        .map(|reference| {
            reference
                .splitn(4, ':')
                .take(3)
                .collect::<Vec<_>>()
                .join(":")
        })
}

/// The summary the executed stage reported, or the empty string when the run
/// executed nothing.
fn summary_of(payload: &serde_json::Value) -> String {
    payload["progress"][0]["summary"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

// ---------------------------------------------------------------------------
// The milestone's headline assertion
// ---------------------------------------------------------------------------

/// **The milestone's mandatory proof**, at each of the three objects in turn.
///
/// One ordered walk per object, each step observing the world its predecessor
/// left — the shape `m0_skeleton.rs` established. The walk is:
///
/// 1. the mutation at this object lands and its answer is lost;
/// 2. the run is then interrupted, so it ends having changed the world and
///    recorded nothing about it;
/// 3. every local record is deleted — bundle and journal alike;
/// 4. a genuinely fresh process runs the same invocation and completes;
/// 5. the world holds exactly one of each object, and exactly one mutation was
///    ever dispatched at each of them.
#[test]
fn an_ambiguous_write_then_a_fresh_process_leaves_exactly_one_of_each() {
    for object in Object::ALL {
        let at = object.as_str();
        let world = ScriptedWorld::new();
        world.make_ambiguous(object);
        world.interrupt_after(object);

        // -- the interrupted attempt ----------------------------------------
        let first = world.publish();
        let first_payload = payload_of(&first);
        assert_ne!(
            first.status.code(),
            Some(0),
            "{at}: the interrupted attempt must not claim success: {first_payload}"
        );
        assert!(
            world.scenario.read_change_marker(WORK_ID).is_none(),
            "{at}: the interrupted attempt must not have accounted for the work"
        );

        // The fault fired, and it fired in the order that makes it an ambiguous
        // write rather than a failed one. Asserted before anything else, because
        // every count below would hold just as well of a world where nothing had
        // ever died — and a test that passes when the thing it tests never
        // happened is the one antipattern this bean cannot afford.
        assert_the_answer_was_lost(&world, object);

        // What the attempt did reach is what it published, so an operator is
        // told about a branch that really is out there.
        let first_branch = effect_identity(&first_payload, "ensure_branch_published")
            .unwrap_or_else(|| {
                panic!("{at}: the branch receipt must reach the bundle: {first_payload}")
            });

        // -- nothing local survives -----------------------------------------
        world.scenario.remove_local_records();
        world.forget_the_push_death();
        world.recover_from(object);

        // -- the fresh process ----------------------------------------------
        let second = world.publish();
        let second_payload = payload_of(&second);
        assert_eq!(
            second.status.code(),
            Some(0),
            "{at}: the retry must complete: {second_payload}\nstderr = {}",
            String::from_utf8_lossy(&second.stderr)
        );

        // -- exactly one of each, against the world -------------------------
        let branches = world.branches();
        assert_eq!(
            branches.len(),
            1,
            "{at}: exactly one branch, got {branches:?}"
        );
        assert!(
            branches[0].starts_with("fiddle/"),
            "{at}: the branch is the one fiddle names, got {branches:?}"
        );
        assert_eq!(
            world.pull_requests().len(),
            1,
            "{at}: exactly one pull request, got {:?}",
            world.pull_requests()
        );
        assert_eq!(
            world.workflow_runs().len(),
            1,
            "{at}: exactly one requested check, got {:?}",
            world.workflow_runs()
        );

        // And exactly one mutation was *dispatched* at each of them, which is
        // the stronger claim: `git push` of the same commit twice and GitHub's
        // own refusal of a second pull request would both leave the counts above
        // at one whatever fiddle did.
        assert_eq!(
            world.pushes(),
            1,
            "{at}: exactly one push was ever dispatched"
        );
        assert!(
            !world.push_died_after_landing(),
            "{at}: the retry must not have pushed at all — the branch was already there"
        );

        // -- the identity was recomputed, not remembered ---------------------
        assert_eq!(
            effect_identity(&second_payload, "ensure_branch_published").as_deref(),
            Some(first_branch.as_str()),
            "{at}: the retry must derive the identity the first attempt derived, \
             with nothing local left to read it from: {second_payload}"
        );
    }
}

/// The fixture's own record that the mutation landed and the answer was lost
/// afterwards.
///
/// Separate from the walk because it is the walk's precondition rather than one
/// of its steps: if this does not hold, every count the walk goes on to make is
/// a count of a world no fault ever touched.
fn assert_the_answer_was_lost(world: &ScriptedWorld, object: Object) {
    let at = object.as_str();
    match object {
        Object::Branch => {
            assert!(
                world.push_died_after_landing(),
                "{at}: the recording `git` must have pushed the ref and *then* died"
            );
            assert_eq!(
                world.branches().len(),
                1,
                "{at}: and the ref must really be on the remote, or nothing was lost"
            );
            // Within the interrupted run itself, and before any retry: the
            // `Unknown` the abort produced was settled by *reading the remote*
            // rather than by pushing again. A runtime that resolved it the other
            // way would show two here and one branch, which is the duplicate that
            // hides behind an idempotent ref.
            assert_eq!(
                world.pushes(),
                1,
                "{at}: the lost answer must be resolved by looking, never by \
                 re-dispatching the push"
            );
        }
        Object::PullRequest => assert_landed_under(world, "pulls", "commit_then_die"),
        Object::Check => assert_landed_under(world, "dispatches", "commit_then_die"),
    }
}

/// **The same mandatory proof, through the other provenance of one ambiguous
/// write:** the answer is lost to a `^C` rather than to a killed child.
///
/// Until M2's holistic review this lane could not reach this case at all. Every
/// ambiguity it injected was a fixture that exited 137 or aborted, which reaches
/// `GhError::Killed` — the provenance the adapter classified *correctly* — while a
/// cancellation reached a variant classified `NotCommitted` beside a pre-spawn
/// refusal. So the one test carrying the milestone's mandatory proof was
/// structurally blind to the one case the code got wrong, and both existing
/// cancellation tests were pre-spawn.
///
/// The walk is the headline test's, with the interrupt in place of the injected
/// failure — and the interrupt does both jobs at once, because a cancelled run
/// also ends without accounting for the work:
///
/// 1. the mutation at this object lands, and the fixture records that it landed
///    and then waits;
/// 2. a real `SIGINT` reaches the real binary, whose real handler cancels the
///    token — the only channel that reaches a child in its own process group;
/// 3. the run must report the outcome as **unresolved** rather than as a settled
///    adapter failure, which is the whole of the classification, observed through
///    a published document;
/// 4. every local record is deleted;
/// 5. a fresh process runs the same invocation and completes, and the world holds
///    exactly one of each object with exactly one mutation ever dispatched at it.
///
/// Step 3 is the assertion an inversion breaks. Steps 1 and 5 hold just as well of
/// a misclassifying runtime here — the scripted world has no listing lag, so its
/// step-3 read finds the object either way — and a lane that asserted only those
/// would pass on the defect. What `Unknown` buys is that the interrupted run tells
/// an operator *nobody knows* instead of *it did not happen*, and only the second
/// of those licenses the retry that duplicates.
#[test]
fn a_cancellation_after_the_write_lands_is_unresolved_and_never_duplicated() {
    for object in Object::ALL {
        let at = object.as_str();
        let world = ScriptedWorld::new();
        world.make_ambiguous_by_cancellation(object);

        // -- the interrupted attempt ----------------------------------------
        let first = world.publish_then_interrupt(object);
        let first_payload = payload_of(&first);
        let first_stderr = String::from_utf8_lossy(&first.stderr).to_string();
        assert_ne!(
            first.status.code(),
            Some(0),
            "{at}: an interrupted attempt must not claim success: {first_payload}"
        );

        // The fault fired, it fired in the order that makes it an ambiguous write,
        // and it fired for the *reason* this test is about. The last of those three
        // is the one that needs its own assertion: the fixtures wait rather than
        // die, so a signal that never arrived would leave the runtime's own deadline
        // to end the request instead — which is `Timeout`, also `Unknown`, and would
        // let every assertion below pass without a cancellation ever being
        // classified.
        assert!(
            first_stderr.contains("interrupted; stopping the attempt"),
            "{at}: the interrupt must have reached the binary's own handler, or the \
             deadline ended this request and no cancellation was classified: \
             {first_stderr}"
        );
        assert_the_answer_was_lost_to_a_cancellation(&world, object);

        // -- what the run said about it -------------------------------------
        //
        // The classification, read off a published field. An `EffectOutcome::Unknown`
        // reaches `EffectError::Unresolved`, whose rendering is "left an unresolved
        // outcome"; a `NotCommitted` reaches `EffectError::Adapter`, whose rendering
        // is "adapter failure for". The two are the fix and the defect, and this is
        // where they are told apart.
        let summary = summary_of(&first_payload);
        assert!(
            summary.contains("unresolved outcome"),
            "{at}: a write whose answer was lost to a cancellation must be reported \
             unresolved, never as a settled failure: {summary:?} in {first_payload}"
        );
        // And the ambiguity named is the *dispatch's*, not the read's. Both
        // failures are cancellations here — the read that follows is refused
        // before spawning, because the token it is handed is the cancelled one —
        // so "cancelled" alone would match the wrong half and would hold even if
        // the write had been classified as never having happened. The
        // after-spawn provenance is the one that has to appear.
        assert!(
            summary.contains("cancelled after"),
            "{at}: the unresolved write must be the one cancelled *after* it was \
             started, not a deadline and not the pre-spawn refusal that followed \
             it: {summary:?}"
        );

        // -- nothing local survives -----------------------------------------
        world.scenario.remove_local_records();
        world.recover_from_cancellation(object);

        // -- the fresh process ----------------------------------------------
        let second = world.publish();
        let second_payload = payload_of(&second);
        assert_eq!(
            second.status.code(),
            Some(0),
            "{at}: the retry must complete: {second_payload}\nstderr = {}",
            String::from_utf8_lossy(&second.stderr)
        );

        // -- exactly one of each, against the world -------------------------
        let branches = world.branches();
        assert_eq!(
            branches.len(),
            1,
            "{at}: exactly one branch, got {branches:?}"
        );
        assert_eq!(
            world.pull_requests().len(),
            1,
            "{at}: exactly one pull request, got {:?}",
            world.pull_requests()
        );
        assert_eq!(
            world.workflow_runs().len(),
            1,
            "{at}: exactly one requested check, got {:?}",
            world.workflow_runs()
        );
        assert_eq!(
            world.pushes(),
            1,
            "{at}: exactly one push was ever dispatched"
        );
    }
}

/// The **killed-child** provenance, asserted where its classification is actually
/// observable: a dispatch that landed, an answer that was lost, and a settling read
/// that cannot be made.
///
/// This test exists because inverting the killed-child rule deliberately — the
/// technique Task 15 established — left the rest of this lane green. Every other
/// scenario here arranges a postcondition read that *succeeds*, and step 8 reports
/// `Committed` off a successful read whatever it thought the dispatch meant. So the
/// lane injected the killed child four times over and could not have noticed it
/// being classified as a refusal.
///
/// The gap is closed by taking away the read rather than by adding another count.
/// With the listing unreadable *after* the dispatch has landed — step 3's read
/// still works, so the effect really is dispatched — the executor has nothing left
/// but the dispatch's own classification, and the two possible answers are the fix
/// and the defect:
///
/// - `Unknown` → `EffectError::Unresolved`, "nobody knows, go and look";
/// - `NotCommitted` → `EffectError::Adapter`, "it did not happen", which is the
///   sentence that licenses the retry that dispatches a second workflow run.
///
/// The check is the object this is worst for, and the reason is
/// `docs/specs`' own: a workflow dispatch answers 204 with no run id and the runs
/// listing does not expose dispatch inputs, so nothing outside fiddle prevents a
/// duplicate.
#[test]
fn a_killed_child_whose_settling_read_fails_is_unresolved_and_never_duplicated() {
    let world = ScriptedWorld::new();
    world.push_mode("delegated");
    world.script(&dispatch_key(), "204 0 commit_then_die");
    world.make_the_settling_read_fail(500);

    // -- the attempt that changed the world and could not confirm it ----------
    let first = world.publish();
    let first_payload = payload_of(&first);
    assert_ne!(
        first.status.code(),
        Some(0),
        "an unsettled write must not be reported as success: {first_payload}"
    );

    // The premise, before any claim about what was *said* about it: the dispatch
    // really landed, and it landed under a `gh` that then died without answering.
    assert_landed_under(&world, "dispatches", "commit_then_die");

    let summary = summary_of(&first_payload);
    assert!(
        summary.contains("unresolved outcome"),
        "a write whose answer was lost and whose postcondition could not be read \
         must be reported unresolved, never as a settled failure: {summary:?} in \
         {first_payload}"
    );
    // And the ambiguity named is the *dispatch's* rather than the read's, which
    // reported an HTTP 500. Without this the assertion above would hold of any
    // unsettled effect at all.
    assert!(
        summary.contains("(gh was killed"),
        "the unresolved write must be named as the killed child it was: {summary:?}"
    );

    // -- nothing local survives, and the world becomes readable again --------
    world.scenario.remove_local_records();
    world.script(&dispatch_key(), "204 0 normal");
    world.let_the_settling_read_succeed();

    // -- the fresh process ---------------------------------------------------
    let second = world.publish();
    let second_payload = payload_of(&second);
    assert_eq!(
        second.status.code(),
        Some(0),
        "the retry must complete: {second_payload}\nstderr = {}",
        String::from_utf8_lossy(&second.stderr)
    );

    assert_eq!(world.branches().len(), 1, "exactly one branch");
    assert_eq!(
        world.pull_requests().len(),
        1,
        "exactly one pull request, got {:?}",
        world.pull_requests()
    );
    assert_eq!(
        world.workflow_runs().len(),
        1,
        "exactly one requested check, got {:?}",
        world.workflow_runs()
    );
    assert_eq!(world.pushes(), 1, "exactly one push was ever dispatched");
}

/// The fixture's own record that the mutation landed and the answer was then lost
/// to a cancellation.
///
/// The counterpart of [`assert_the_answer_was_lost`], and separate from the walk
/// for the same reason: without it every count that follows is a count of a world
/// no fault ever touched.
fn assert_the_answer_was_lost_to_a_cancellation(world: &ScriptedWorld, object: Object) {
    let at = object.as_str();
    match object {
        Object::Branch => {
            assert_eq!(
                world.branches().len(),
                1,
                "{at}: the ref must really be on the remote, or nothing was lost"
            );
            assert_eq!(
                world.pushes(),
                1,
                "{at}: and the lost answer must not have been resolved by pushing \
                 again"
            );
        }
        Object::PullRequest => assert_landed_under(world, "pulls", "commit_then_wait"),
        Object::Check => assert_landed_under(world, "dispatches", "commit_then_wait"),
    }
}

/// Send one `SIGINT` to a pid.
///
/// Through `kill` rather than through a signalling crate, because this suite
/// deliberately has no dependency that could reach the runtime: `fiddle-acceptance`
/// drives the compiled binary and links neither `fiddle-core` nor `fiddle-runtime`,
/// and adding a libc dependency to deliver one signal would be the first crack in
/// that.
fn interrupt(pid: u32) {
    let status = std::process::Command::new("kill")
        .args(["-INT", &pid.to_string()])
        .status()
        .expect("kill is on the PATH");
    assert!(status.success(), "could not interrupt {pid}");
}

/// Exactly one mutation matching `needle` landed, and the scripted `gh` recorded
/// that it landed under `mode` — which for `commit_then_die` means the world
/// changed and the process then exited 137 without answering.
fn assert_landed_under(world: &ScriptedWorld, needle: &str, mode: &str) {
    let landed = world.landed(needle);
    assert_eq!(
        landed.len(),
        1,
        "the interrupted attempt must have landed exactly one {needle}: {landed:?}"
    );
    assert_eq!(
        landed[0]["mode"], mode,
        "the mutation must have landed under a `gh` that then died: {landed:?}"
    );
}

// ---------------------------------------------------------------------------
// The retry is a second attempt, not a replay
// ---------------------------------------------------------------------------

/// The two runs are two attempts at one piece of work: different attempt
/// identities, the same work reference.
///
/// The distinction `m0_skeleton.rs` draws, made here across an ambiguous write:
/// a retry that reused the attempt id would be a replay, and one that changed the
/// work reference would be a different piece of work — and neither could support
/// a claim about *the same* branch, pull request and check.
#[test]
fn the_retry_carries_a_distinct_attempt_id_and_the_same_work_ref() {
    let world = ScriptedWorld::new();
    world.make_ambiguous(Object::PullRequest);
    world.interrupt_after(Object::PullRequest);

    let first = payload_of(&world.publish());
    // Read before the records are removed: `attempt_id` and `work_ref` are
    // bundle fields, and the `run --json` payload deliberately does not carry
    // them — so this is read out of the published document a consumer would read.
    let first_bundle = world.scenario.read_bundle(&first);

    world.scenario.remove_local_records();
    world.recover_from(Object::PullRequest);

    let second = payload_of(&world.publish());
    let second_bundle = world.scenario.read_bundle(&second);

    assert_ne!(
        first_bundle["attempt_id"], second_bundle["attempt_id"],
        "two attempts, two identities: {first_bundle} / {second_bundle}"
    );
    assert_eq!(
        first_bundle["work_ref"], second_bundle["work_ref"],
        "one piece of work: {first_bundle} / {second_bundle}"
    );
    assert_eq!(
        first_bundle["invocation_ref"], second_bundle["invocation_ref"],
        "addressed the same way both times"
    );
}

// ---------------------------------------------------------------------------
// The second sentinel
// ---------------------------------------------------------------------------

/// The forge credential reaches no observable surface, against a `gh` that hands
/// it straight back.
///
/// Adversarial rather than incidental, and that is the whole value: the scripted
/// `gh`'s `echo_token` mode puts the credential in its **response body**, which
/// is exactly the shape of the defect M1 shipped and had to repair. The status is
/// a `422`, so the body is quoted into an `EffectError` — which is rendered into
/// `RunOutcome`'s reason and into `progress[0].summary`, both of them published
/// document fields. A client that carried a response body into a diagnostic
/// therefore leaks here, rather than this test passing because nothing happened
/// to be echoed.
#[test]
fn the_github_token_appears_in_no_bundle_no_stdout_and_no_diagnostic() {
    let world = ScriptedWorld::new();
    world.push_mode("delegated");
    world.script(&pulls_key(), "422 1 echo_token");

    let out = world.publish();
    let payload = payload_of(&out);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    // Asserted first, because every assertion below holds trivially of a run
    // that never resolved a credential or never reached the response carrying
    // it. This run pushed a branch with the credential and then read a body that
    // contained it.
    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "publish_change",
        "the credential was resolved and the capability ran: {payload}"
    );
    assert_eq!(
        world.branches().len(),
        1,
        "the credential-carrying `git push` really ran: {payload}"
    );
    assert!(
        summary_of(&payload).contains("422"),
        "the token-bearing response must have been read and reported: {payload}"
    );
    // And the credential really reached the child that echoed it back, observed
    // in what the fixture recorded of its own environment. Without this the
    // search below could pass against a run whose `gh` never held the token at
    // all, which is not a test of anything.
    assert!(
        world.gh_requests().iter().any(|request| {
            request["env"].as_array().is_some_and(|env| {
                env.iter()
                    .any(|name| name == &format!("GH_TOKEN={SENTINEL}"))
            })
        }),
        "the scripted `gh` must have received the credential it echoed back"
    );

    assert!(
        !stdout.contains(SENTINEL),
        "the token reached stdout: {stdout}"
    );
    assert!(
        !stderr.contains(SENTINEL),
        "the token reached a diagnostic: {stderr}"
    );
    // The bundle this run published is what a stranger reads, and it is inside
    // the tree searched below. Named first so the search is provably about
    // something.
    assert!(
        payload["report"].is_string(),
        "this run must have published a bundle for the search to be about: {payload}"
    );
    let holding = world.files_holding(SENTINEL);
    assert!(
        !holding.is_empty(),
        "the scan found the token nowhere at all, not even in the fixtures' own \
         recordings of the environment they were handed — so it is looking at \
         nothing and would pass on a real leak"
    );
    let leaked: Vec<&String> = holding
        .iter()
        .filter(|path| !is_fixture_recording(path))
        .collect();
    assert!(leaked.is_empty(), "the token was written to {leaked:?}");
}

// ---------------------------------------------------------------------------
// Fail closed through the adapter
// ---------------------------------------------------------------------------

/// An unreadable GitHub is not an empty GitHub: nothing is published, and the
/// forge is reported as unread rather than as holding nothing.
///
/// # SPEC_DEFECT: the bean's own criterion cannot be satisfied as written
///
/// `m2-fail-closed-through-the-adapter` asks for *"exit 20, an Unavailable review
/// observation, and zero capability executions"*. Its three halves are mutually
/// exclusive in this design, and the evidence is in the tree rather than in an
/// opinion:
///
/// 1. **An `Unavailable` review requires the capability to have executed.**
///    `orchestration::with_publication` folds a review into the view from
///    `Capability::publication()`, and `PublishChange` fills that in inside
///    `execute`. A run that executed nothing leaves the pair
///    `WorkStateView::without_publication` gives it, which is `NotApplicable` —
///    not `Unavailable`. So "an Unavailable review" and "zero capability
///    executions" cannot both hold of one bundle.
///
/// 2. **An unreachable GitHub cannot be known before executing.**
///    `derive_next` is a function of the work item and the change set alone, and
///    its pre-execution input comes from `orchestration::observe`, which is local
///    and credential-free *by hard constraint*: `inspect` shares that call and
///    must stay read-only and credential-free for every value of `--capability`
///    (`github_deployment::inspect_names_the_publishing_capability_without_building_it`).
///    Making the derivation depend on a forge read would put a credentialled call
///    inside `inspect`, which is the property M1 had to repair once.
///
/// 3. **Exit 20 is the wrong row.** `RunOutcome::Failed` is documented as the
///    outcome that "will not succeed by being repeated as invoked". An unreachable
///    host succeeds when the network returns, which is exactly
///    `RunOutcome::Retryable` — exit 11, the row Task 10 established and
///    `github_deployment::run_constructs_and_executes_the_publishing_capability`
///    pins. Nothing was changed here to move it: `assess` was deliberately left
///    alone, because a branch that no reachable world can take is inert code, and
///    ADR 013 already priced one of those.
///
/// What is asserted instead is the property the criterion's own comment names —
/// *fail-closed survives contact with the network layer* — in its strongest
/// available form: **no object was created anywhere**, no push was even
/// dispatched, and the review and verification are both `Unavailable` with a
/// reason rather than an `Available` view whose fields are all empty.
#[test]
fn an_unreachable_github_publishes_nothing_and_reports_an_unread_forge() {
    let world = ScriptedWorld::new();
    // Not a `gh` at all, and not a scripted status either: a connection that was
    // refused. `gh` answers that with a message on stderr and no HTTP response,
    // which the adapter classifies `Malformed`. That is `Unknown` rather than a
    // refusal — a wrapper is free to deliver a request and then print something
    // unreadable — and it changes nothing here, because the failure lands on the
    // *first* read of the first effect, which is step 3. Nothing was dispatched to
    // be ambiguous about, so no postcondition is owed and nothing may be guessed
    // about a world nobody saw.
    world.use_gh(&unreachable_gh(world.scenario.dir()));

    let out = world.publish();
    let payload = payload_of(&out);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    // The row a capability failure has had since M1, and the one Task 10 pinned.
    // See this test's documentation for why it is not 20.
    assert_eq!(
        out.status.code(),
        Some(11),
        "an unreachable forge fails the run, retryably: {payload}\nstderr = {stderr}"
    );

    // The load-bearing half: unread, not empty. An `Available` review with every
    // field `None` would be the positive claim *the forge was read and holds
    // nothing*, which is the reading that puts a second branch on a remote.
    assert!(
        payload["observations"]["review"]["unavailable"].is_object(),
        "an unreadable forge must be Unavailable and never an empty Available: {payload}"
    );
    assert!(
        payload["observations"]["verification"]["unavailable"].is_object(),
        "and so must the verification: {payload}"
    );
    assert!(
        payload["observations"]["review"]["unavailable"]["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty()),
        "an unread forge must say why it was not read: {payload}"
    );

    // Fail *closed*: the world is untouched, and the push was not even
    // dispatched. This is what "zero executions" was reaching for, asserted
    // where it is true — of the effects, which are the things that could have
    // duplicated, rather than of the capability, which necessarily ran in order
    // to discover that the forge was unreadable.
    assert!(world.branches().is_empty(), "no branch was created");
    assert!(
        world.pull_requests().is_empty(),
        "no pull request was created"
    );
    assert!(world.workflow_runs().is_empty(), "no check was requested");
    assert_eq!(
        world.pushes(),
        0,
        "the push must not even be dispatched: the branch's postcondition was \
         never read, so nothing licensed one"
    );
    assert_eq!(
        payload["capability_executions"][0]["status"], "failed",
        "the capability ran and failed, which is how the unreadable forge was \
         discovered at all: {payload}"
    );
}

/// A `gh` that cannot reach GitHub: a message on stderr, no response, exit 1.
///
/// The shape of a refused connection, taken from what `gh` itself does — it is
/// not an HTTP status, which is the point. A status could be classified; this
/// cannot, and the adapter must therefore refuse to conclude anything about the
/// world.
#[cfg(unix)]
fn unreachable_gh(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("unreachable-gh");
    std::fs::write(
        &path,
        "#!/bin/sh\n\
         echo 'dial tcp: connect: connection refused' >&2\n\
         exit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

// ---------------------------------------------------------------------------
// The executor's step trace, in a real run
// ---------------------------------------------------------------------------

/// The seven steps of every effect a real run walked are in that attempt's
/// journal, on disk, where a recovery would find them.
///
/// This is the black-box half of the trace's production sink. The runtime-level
/// half — that the journal file really receives each step, asserted against the
/// file rather than against the trait — is `attempt::the_executors_steps_reach_the_attempt_journal`.
///
/// The journal is superseded the moment a bundle lands, so a successful run
/// leaves nothing to read: the record exists precisely for the attempts that did
/// *not* finish recording themselves. So this scenario reaches that state the way
/// `binary_repair.rs` already does — the attempt directory is writable and
/// `<report.dir>` is not — which is the one arrangement where a capability runs
/// to completion and its bundle cannot be published.
#[test]
fn the_effect_steps_of_a_real_run_reach_the_attempt_journal() {
    let world = ScriptedWorld::new();
    world.push_mode("delegated");
    world.scenario.prepare_journal_dir();
    world.scenario.make_report_dir_unwritable();

    let out = world.publish();
    world.scenario.make_report_dir_writable();
    let payload = payload_of(&out);

    // The run really executed all three effects: the world holds all three
    // objects. Without this the journal assertions below could be satisfied by a
    // run that walked one effect and stopped.
    assert_eq!(world.branches().len(), 1, "{payload}");
    assert_eq!(world.pull_requests().len(), 1, "{payload}");
    assert_eq!(world.workflow_runs().len(), 1, "{payload}");

    let records = world.scenario.journal_records();
    assert_eq!(
        records.len(),
        1,
        "one attempt, one journal, and it must have survived an unpublished \
         bundle: {records:?}"
    );
    let recorded: Vec<serde_json::Value> = std::fs::read_to_string(&records[0])
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();

    // The order of the whole walk, per effect kind, read off the file. Asserted
    // as a sequence rather than as a set: a trace that recorded `authorize`
    // before `inspect_postcondition` would be describing an executor that
    // authorized a mutation before asking whether it was needed, and a set could
    // not tell the two apart.
    const ORDER: [&str; 7] = [
        "validate_capability",
        "derive_identity",
        "inspect_postcondition",
        "combine_policy",
        "authorize",
        "apply",
        "observe_postcondition",
    ];
    for kind in [
        "ensure_branch_published",
        "ensure_pull_request",
        "ensure_check_requested",
    ] {
        let steps: Vec<&str> = recorded
            .iter()
            .filter(|record| record["record"] == "effect_step" && record["kind"] == kind)
            .filter_map(|record| record["step"].as_str())
            .collect();
        assert_eq!(
            steps, ORDER,
            "the journal must hold {kind}'s whole walk, in order"
        );
    }

    // The records the journal already carried are untouched: the step trace is
    // an addition to what an interrupted attempt says about itself, never a
    // replacement for it.
    assert!(
        recorded.iter().any(|record| record["record"] == "intent"),
        "the intent record must still be the first thing written: {recorded:?}"
    );
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// The scripted `gh`'s key for the pull-request create.
///
/// Derived from the same two constants the document names rather than pasted, so
/// a scenario that changed the repository cannot end up scripting a request
/// nothing makes — which would leave the well-behaved default in place and pass.
fn pulls_key() -> String {
    script_key("POST", &format!("/repos/{REPO}/pulls"))
}

/// And for the workflow dispatch.
fn dispatch_key() -> String {
    script_key(
        "POST",
        &format!("/repos/{REPO}/actions/workflows/{WORKFLOW}/dispatches"),
    )
}

/// One API request as the scripted `gh` names its script files: the method, then
/// the path with every separator mangled into an underscore.
///
/// Spelled here from the fixture's documented rule rather than imported, for the
/// reason `Scenario::expected_marker` re-derives the correlation key: this lane
/// checks the binary against a stated contract instead of against another copy of
/// the implementation.
fn script_key(method: &str, path: &str) -> String {
    format!(
        "{method}_{}",
        path.trim_start_matches('/')
            .replace(['/', '?', '&', '=', '%'], "_")
    )
}

/// A path as a TOML string, escaped rather than pasted.
fn toml_path(path: &Path) -> String {
    format!("{:?}", path.display().to_string())
}

/// Run git in `dir`, panicking with its stderr if it fails.
fn git(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("could not run git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The same, returning what git said on stdout, trimmed.
fn git_says(dir: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("could not run git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
