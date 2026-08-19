//! What reaches the model, and what a milestone of arithmetic keeps back from it.
//!
//! The subject is [`fiddle_runtime::capability::cve`], and it is the **only**
//! agentic step in M4a. Everything else this milestone does is decided in Rust:
//! the version comparison, attribution's four rules and the resolver commands
//! behind them, grouping, the version each group moves to, the already-fixed
//! set, the fold rule, the five checks and both rescan conditions. This suite is
//! about the one place a model is consulted, and about how little it is told.
//!
//! # The claim, and why it is asserted where it is
//!
//! *The model receives the projection and the scope rules only* — asserted
//! against the **serialized outbound request** rather than against the builders
//! that produced it. The two are not the same assertion. A capability can
//! compose a prompt it is proud of and then hand the provider something else: a
//! preamble set on the agent, a document attached beside the message, a tool
//! description that names a path. What a provider integration renders is
//! [`CompletionRequest`]'s serialization, so that is what [`sent`] reads, exactly
//! as `interpretation.rs` does — an arrangement `binary_repair`'s
//! `the_serialized_request_offers_four_tools_and_carries_no_host_fact`
//! established against bodies a compiled binary put on a socket.
//!
//! # A sentinel is only evidence if something planted it
//!
//! Three of the four assertions below are absences, and an absence proves
//! nothing about a world that never held the thing. So the world is built to
//! hold all three — see [`migration_world`], whose document really carries
//! advisory prose, whose group's targets really came from `go list -m -json`,
//! and whose worktree really lives under a directory named for [`HOST_ROOT`].
//! [`the_world_holds_everything_the_prompt_must_not`] asserts each of those
//! premises on its own, as a lane rather than as a comment, so that a fixture
//! which stopped planting one of them fails *there* instead of quietly making
//! this suite vacuous.
//!
//! Nothing here reaches a credential, a socket, a real `go` or a module proxy.
//! The model is `MockCompletionModel`, an ordinary test dependency:
//! `GroupMigration` is generic over Rig's own `CompletionModel`, so a script
//! substitutes where a gateway would and nothing in `src/` knows a test is
//! happening.
//!
//! [`CompletionRequest`]: rig_core::completion::CompletionRequest

mod support;

use fiddle_runtime::capability::{
    land, record_fold, undeclared, CapabilityError, ForbiddenShape, GroupMigration, GroupStatus,
    InWorktree, MigrationAttempt, NeedsWork,
};
use fiddle_runtime::cve::dedup::FixedInCommits;
use fiddle_runtime::cve::fold::Landed;
use fiddle_runtime::evaluate::{evaluate, Evaluation, RescanVerdict};
use fiddle_runtime::workspace::{Content, FileEdit, WorkspacePath};
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::json;
use std::time::Duration;
use support::cve::{
    ask_git, contract, contract_scanned_by, exit, green_tree, landing_worktree, landing_world,
    migration_world, stdout, tree_rescanned_by, tree_where, LandingWorld, MigrationWorld, GO_BUILD,
    HOST_ROOT, LANDING_CREATED, LANDING_UNRELATED, MIGRATION_SOURCE as SOURCE,
    MIGRATION_TEST_BEFORE, MIGRATION_TEST_SOURCE as TEST_SOURCE, SENTINEL_PROSE,
};

// ---------------------------------------------------------------------------
// What the model is scripted to do
// ---------------------------------------------------------------------------

/// The migration this suite's model performs: read the source, rewrite it, run
/// the check, report.
///
/// A real edit rather than a no-op, and it calls three of the four tools, so the
/// request stream this suite reads is the one a working attempt produces and not
/// the single turn of a model that answered immediately. That matters: with one
/// turn there would be one request, and *what reaches the model over an attempt*
/// would have been asserted over its opening move alone.
fn migrates() -> Vec<MockTurn> {
    vec![
        MockTurn::tool_call("c1", "read_file", json!({ "path": SOURCE })),
        MockTurn::tool_call(
            "c2",
            "write_file",
            json!({
                "path": SOURCE,
                "contents": "package main\n\nfunc main() {\n\trenamedName()\n}\n\nfunc renamedName() {}\n",
            }),
        ),
        MockTurn::tool_call("c3", "run_check", json!({})),
        MockTurn::text(
            r#"{"changed_files":["main.go"],"summary":"applied the rename","claimed_complete":true}"#,
        ),
    ]
}

// ---------------------------------------------------------------------------
// What a migration leaves behind (Task 14.b)
// ---------------------------------------------------------------------------

/// The report a scripted model signs off with.
///
/// `claimed_complete: true` on every script below, forbidden ones included, and
/// that is the point rather than laziness: a model that had *said* it went out
/// of scope would be an easy case, and the lanes here are about the one where
/// the claim and the diff disagree.
const CLAIMS_DONE: &str = r#"{"changed_files":["main.go","main_test.go"],"summary":"applied the rename","claimed_complete":true}"#;

/// [`SOURCE`] after a rename that reached every call site.
const RENAMED_SOURCE: &str = "\
package main

func main() {
\trenamedName()
}

func renamedName() {}
";

/// [`TEST_SOURCE`] after the same rename.
///
/// The assertion line is **byte-identical** to the one in
/// [`MIGRATION_TEST_BEFORE`], which is what makes this a clean edit: the message
/// names no function, so a uniform rename has no reason to touch it. Every
/// forbidden script below is this string with exactly one further thing done to
/// it, so a lane that fires is firing on that one thing.
const RENAMED_TEST: &str = "\
package main

import \"testing\"

func TestRenamedName(t *testing.T) {
\trenamedName()
\tif testing.Short() {
\t\tt.Errorf(\"this test must run even in short mode\")
\t}
}
";

/// The uniform migration: the same rename at every call site, `_test.go`
/// included.
///
/// **The classifier's positive control.** Without it, every `NeedsWork`
/// assertion below is satisfied by a classifier that answers `NeedsWork` for
/// any diff at all, and none of the four shape lanes measures anything.
fn migrates_uniformly() -> Vec<MockTurn> {
    edits(&[
        (SOURCE, RENAMED_SOURCE.to_string()),
        (TEST_SOURCE, RENAMED_TEST.to_string()),
    ])
}

/// The uniform migration, plus a `t.Skip` in front of the renamed call.
fn adds_a_skip() -> Vec<MockTurn> {
    edits(&[
        (SOURCE, RENAMED_SOURCE.to_string()),
        (
            TEST_SOURCE,
            RENAMED_TEST.replace(
                "\trenamedName()\n",
                "\tt.Skip(\"the bump changed this API\")\n\trenamedName()\n",
            ),
        ),
    ])
}

/// The uniform migration, plus the one assertion downgraded to a log line.
///
/// The shape a model reaches for when the migration broke a test and it would
/// rather the suite went green: the assertion is not deleted, so a rule looking
/// for a *shorter* file would miss it.
fn weakens_an_assertion() -> Vec<MockTurn> {
    edits(&[
        (SOURCE, RENAMED_SOURCE.to_string()),
        (TEST_SOURCE, RENAMED_TEST.replace("t.Errorf(", "t.Logf(")),
    ])
}

/// The uniform migration, plus a branch that was not there.
fn adds_control_flow() -> Vec<MockTurn> {
    edits(&[
        (
            SOURCE,
            RENAMED_SOURCE.replace(
                "\trenamedName()\n",
                "\tif len(\"x\") > 0 {\n\t\trenamedName()\n\t}\n",
            ),
        ),
        (TEST_SOURCE, RENAMED_TEST.to_string()),
    ])
}

/// The uniform migration, plus a `replace` directive pointing the module
/// somewhere else.
///
/// Built from the world's own `go.mod` rather than from a fixture copy of one,
/// because a script that wrote a whole invented `go.mod` would be *removing*
/// every line the real one had and the lane would no longer be about the one
/// line it added.
fn adds_a_replace_directive(world: &MigrationWorld) -> Vec<MockTurn> {
    let module = world.target_module();
    let go_mod = std::fs::read_to_string(world.tree.path().join("go.mod"))
        .expect("the fixture tree has a go.mod");
    edits(&[
        (SOURCE, RENAMED_SOURCE.to_string()),
        (TEST_SOURCE, RENAMED_TEST.to_string()),
        (
            "go.mod",
            format!("{go_mod}\nreplace {module} => ../vendored/{module}\n"),
        ),
    ])
}

/// A model that changes nothing and says it is finished.
///
/// The world in which nothing but the checks can be deciding: no file moved, so
/// there is no diff to classify and no shape to find.
fn claims_success_without_editing() -> Vec<MockTurn> {
    vec![MockTurn::text(
        r#"{"changed_files":[],"summary":"nothing needed doing","claimed_complete":true}"#,
    )]
}

/// The uniform migration, reported as a failure.
///
/// The mirror of [`claims_success_without_editing`]: the claim points the other
/// way from what the tree can be shown to be.
///
/// The **declaration is honest** and only `claimed_complete` is not, which is
/// what keeps this lane about the claim. A script that also understated its diff
/// would be refused by the declaration rule before `claimed_complete` came up,
/// and the lane would pass while measuring the wrong thing — see
/// [`migrates_and_understates_it`], which is that script on purpose.
fn migrates_and_disowns_it() -> Vec<MockTurn> {
    let mut script = migrates_uniformly();
    script.pop();
    script.push(MockTurn::text(
        r#"{"changed_files":["main.go","main_test.go"],"summary":"I do not think this is right","claimed_complete":false}"#,
    ));
    script
}

/// The uniform migration, declared as if it had only touched one of the two
/// files.
///
/// Byte-for-byte the edit [`migrates_uniformly`] makes — every rule in
/// [`ForbiddenShape`] is satisfied by it and the checks pass over it — with the
/// declaration understated by exactly one path. So the only thing left that can
/// refuse it is the declaration rule.
fn migrates_and_understates_it() -> Vec<MockTurn> {
    let mut script = migrates_uniformly();
    script.pop();
    script.push(MockTurn::text(
        r#"{"changed_files":["main.go"],"summary":"applied the rename","claimed_complete":true}"#,
    ));
    script
}

/// A script that writes each `(path, contents)` in turn, runs the check and
/// signs off.
///
/// One builder rather than six near-copies, so that the difference between the
/// clean script and each forbidden one is visible in the caller as the one
/// string it changed.
fn edits(files: &[(&str, String)]) -> Vec<MockTurn> {
    let mut script = vec![MockTurn::tool_call(
        "c0",
        "read_file",
        json!({ "path": SOURCE }),
    )];
    for (n, (path, contents)) in files.iter().enumerate() {
        script.push(MockTurn::tool_call(
            format!("w{n}"),
            "write_file",
            json!({ "path": path, "contents": contents }),
        ));
    }
    script.push(MockTurn::tool_call("k", "run_check", json!({})));
    script.push(MockTurn::text(CLAIMS_DONE));
    script
}

// ---------------------------------------------------------------------------
// Reading what actually went to the provider
// ---------------------------------------------------------------------------

/// Everything one attempt put on the wire, in the two renderings between them
/// nothing can hide in.
///
/// # Why both, and why the pair is stronger than either
///
/// [`Sent::json`] is the document a provider integration renders:
/// `CompletionRequest` is `Serialize`, so this is the request rather than a
/// summary of the builders that assembled it. It is the primary instrument, and
/// the positive assertion is made against it alone.
///
/// [`Sent::debug`] closes the one gap serialization has. `CompletionRequest`
/// carries a `#[serde(skip)]` field, and while today that field is a `bool`
/// which could not hold a sentinel, a rule that depends on rig's field list not
/// changing is a rule about today's rig. `Debug` is derived over every field and
/// no serde attribute can exclude anything from it — the same reasoning Task 6
/// gave for rendering `ProjectedFinding` through `Debug`, applied here to the
/// residue rather than to the whole. So the *negative* assertions go through
/// [`Sent::carries`], which is true if **either** rendering holds the needle:
/// absent from the union is absent from both.
struct Sent {
    json: String,
    debug: String,
}

impl Sent {
    /// Is `needle` anywhere in what went out?
    fn carries(&self, needle: &str) -> bool {
        self.json.contains(needle) || self.debug.contains(needle)
    }
}

/// Run one migration and read what the model was sent.
///
/// Every request of the attempt, not the first: a prompt is composed once but a
/// tool-calling run sends the whole conversation back on each turn, and a host
/// fact that entered through a tool's *result* would be invisible in turn one.
fn sent(model: &MockCompletionModel) -> Sent {
    let requests = model.requests();
    assert!(
        !requests.is_empty(),
        "the model was never called, so there is nothing here to read and every \
         absence below would hold for the emptiest of reasons"
    );
    Sent {
        json: serde_json::to_string(&requests).expect("a CompletionRequest serializes"),
        debug: format!("{requests:#?}"),
    }
}

/// One bounded migration of `world`'s group, driven by `model`.
///
/// The attempt's own outcome is returned for the lanes that want it and ignored
/// by the ones that only want the request stream — a failed attempt still sent
/// what it sent, and a suite about the prompt must not depend on the model
/// having behaved.
async fn run_migration(
    model: MockCompletionModel,
    world: &MigrationWorld,
) -> Result<MigrationAttempt, fiddle_runtime::capability::CapabilityError> {
    GroupMigration::new(model, world.config())
        .migrate(&world.workspace(), &world.group)
        .await
}

// ---------------------------------------------------------------------------
// The premises
// ---------------------------------------------------------------------------

/// **Everything the prompt must not carry is really in this run.**
///
/// The denominator for the whole suite. Without it,
/// [`the_prompt_carries_the_projection_and_the_scope_rules_and_nothing_else`]
/// would be three assertions that a string is missing from a request, over a
/// world in which the string was never anywhere — which is the antipattern this
/// milestone keeps catching, and which no amount of care in the subject would
/// fix.
///
/// It is a lane and not a comment because a fixture can stop planting a sentinel
/// silently. If that happens this fails, and the failure names which one.
#[tokio::test]
async fn the_world_holds_everything_the_prompt_must_not() {
    let world = migration_world().await;

    assert!(
        world.report.raw().contains(SENTINEL_PROSE),
        "the document the findings were projected from must carry advisory prose"
    );
    assert!(
        world.resolved.contains("go list -m"),
        "attribution really ran the mechanical rule for this group: {}",
        world.resolved
    );
    assert!(
        world.workspace_root().to_string_lossy().contains(HOST_ROOT),
        "the attempt's worktree must live under a path carrying the host \
         sentinel, or `no host fact` is a claim about a path nothing holds: {}",
        world.workspace_root().display()
    );

    // And the group is not empty, which is what stops the *positive* assertion
    // being the only thing standing between the subject and an empty prompt.
    assert!(
        !world.group.findings().is_empty(),
        "a group with no findings would let a prompt carrying no projection pass \
         every assertion in this file"
    );
}

// ---------------------------------------------------------------------------
// The criterion
// ---------------------------------------------------------------------------

/// **The model receives the projection and the scope rules, and nothing else.**
///
/// Four assertions over the bytes that went to the provider, and each excluded
/// thing was established as present in the run by
/// [`the_world_holds_everything_the_prompt_must_not`] before this lane asserts
/// it is absent from the request.
///
/// The mechanical rules are asserted as a set rather than one at a time, because
/// the criterion is about the class: *no* mechanical rule is handed to the
/// model, not "not the one somebody remembered". Each needle is a string this
/// build really spells somewhere on the deterministic side — attribution's two
/// resolver commands and the probe's `go mod tidy`, the version comparison, the
/// deduplication and the fold — and each is decided before or after this step by
/// code the model never sees.
#[tokio::test]
async fn the_prompt_carries_the_projection_and_the_scope_rules_and_nothing_else() {
    let world = migration_world().await;
    let model = MockCompletionModel::new(migrates());
    let _ = run_migration(model.clone(), &world).await;
    let sent = sent(&model);

    // The projection reaches it: the advisory this group is about, by name.
    let cve = world.group.cves()[0].as_str().to_string();
    assert!(
        sent.json.contains(&cve),
        "the projection has to reach the model, or every absence below is the \
         absence of an empty prompt"
    );
    // And the scope rules reach it, which is the other half of what it may be
    // told: one phrase per rule there is, since M4c left two.
    //
    // Neither needle is a field name. `changed_files` would have been the obvious
    // one for the declared-files rule and is useless as evidence — it is also a
    // property name on the report schema in this same request, so it would be
    // present in a run whose prompt carried no rules at all. These two phrases
    // are spelled in `SCOPE_RULES` and nowhere else that reaches a provider.
    for rule in ["refuses the whole attempt", "report it as not attempted"] {
        assert!(
            sent.json.contains(rule),
            "the scope rules reach it, including `{rule}`"
        );
    }

    assert!(!sent.carries(SENTINEL_PROSE), "no advisory prose");

    for mechanical in [
        "go list -m",
        "go mod why",
        "go mod tidy",
        "at_least",
        "dedup",
        "fold",
    ] {
        assert!(
            !sent.carries(mechanical),
            "`{mechanical}` is decided in Rust; no mechanical rule is handed to \
             the model"
        );
    }

    assert!(
        !sent.carries(HOST_ROOT),
        "no host fact, as M1 already requires"
    );
}

/// **The projection is what reaches it, and not the record the projection was
/// made from.**
///
/// The other side of the prose assertion, and the reason it is a separate lane:
/// *the prose is absent* is satisfied by a prompt that carries nothing at all,
/// while this one fails for a prompt that carries the scanner's record instead
/// of the six fields. Every field of the projection arrives; `hasExploit` — a
/// key the record carries and the boundary reads without carrying past — does
/// not.
///
/// **`description` is deliberately not a needle here**, and the reason is worth
/// writing down because it was measured rather than reasoned about. It is the
/// prose sentinel's own key in a scanner document, and it is *also* JSON
/// Schema's key for a parameter's documentation — so every tool definition in
/// the request contains it legitimately, and an assertion on it fired against a
/// prompt that carried no advisory record at all. A needle that is true for two
/// different reasons discriminates nothing. What separates the record from the
/// projection is the *value* under that key, which is [`SENTINEL_PROSE`] and
/// which the lane above asserts.
#[tokio::test]
async fn the_six_fields_arrive_and_the_record_they_came_from_does_not() {
    let world = migration_world().await;
    let model = MockCompletionModel::new(migrates());
    let _ = run_migration(model.clone(), &world).await;
    let sent = sent(&model);

    let finding = world.group.findings()[0].finding();
    for value in [
        finding.cve.as_str(),
        finding.package.as_str(),
        finding.current.as_str(),
        finding
            .fixed_version
            .as_deref()
            .expect("a fixable finding names a fix"),
    ] {
        assert!(
            sent.json.contains(value),
            "the projected `{value}` must reach the model"
        );
    }
    assert!(
        sent.json.contains("Critical") || sent.json.contains("High"),
        "the grade is one of the six fields and must reach it too"
    );

    // A key the scanner's record carries, which `cve::project::record` reads at
    // the boundary and does not carry past it. See this lane's doc for the key
    // that is *not* here and why.
    assert!(
        !sent.carries("hasExploit"),
        "`hasExploit` is a key of the scanner's record, and the record is not \
         what goes to the model"
    );
}

/// **The target four mechanical rules elected does not travel with the group.**
///
/// A [`Group`](fiddle_runtime::cve::group::Group) carries a target per finding,
/// and the migration is handed the whole group — so the target really is in the
/// capability's hands when it composes the prompt, and leaving it out is a
/// choice the subject makes rather than a fact about what it was given.
///
/// It is asserted separately from the mechanical-rule set above because it is a
/// different kind of exclusion: `go list -m` is a *rule*, and this is the rule's
/// *answer*. The answer is why the tree is in the state the model is looking at,
/// and the model has no part in deciding it.
///
/// The needle is `Target`'s own rendering rather than the module path, because
/// the module path is the *package* of a rule-1 finding and is in the projection
/// legitimately. What must not appear is the attributed value.
#[tokio::test]
async fn the_bump_target_the_rules_elected_is_not_in_the_prompt() {
    let world = migration_world().await;
    let model = MockCompletionModel::new(migrates());
    let _ = run_migration(model.clone(), &world).await;
    let sent = sent(&model);

    let target = format!("{:?}", world.group.target());
    assert!(
        target.contains(&world.target_module()),
        "the premise: this group's target names the module, so a prompt that \
         rendered the target would be visible: {target}"
    );
    for rendering in ["Module(", "DockerfileBaseImage", "Rule::", "attribution"] {
        assert!(
            !sent.carries(rendering),
            "`{rendering}` belongs to the answer attribution gave, not to what \
             the model is asked"
        );
    }
}

/// **The attempt reaches the tools and the tree, so the request stream above is
/// a real one.**
///
/// Not a claim about the prompt, and here for the reason M1's success path is in
/// its protocol suite: every assertion in this file is about what a *working*
/// attempt sent, and a `GroupMigration` whose worktree never got made, or whose
/// tools all refused, would send a plausible-looking opening request and prove
/// nothing about the rest.
///
/// What is asserted is the three things that could each have been hollow: the
/// model's claim came back, git saw the edit the script wrote, and the tools
/// really ran — including `run_check`, which spawns the scripted `go` as a child
/// process under the workspace's four-name environment.
#[tokio::test]
async fn the_attempt_really_edits_the_tree_through_the_tools() {
    let world = migration_world().await;
    let migration = GroupMigration::new(MockCompletionModel::new(migrates()), world.config());
    let attempt = migration
        .migrate(&world.workspace(), &world.group)
        .await
        .expect("a scripted migration completes");

    assert!(
        attempt.report.claimed_complete,
        "the model's claim is carried back as evidence"
    );
    assert_eq!(
        attempt
            .changed
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>(),
        vec![SOURCE.to_string()],
        "git saw exactly the file the script wrote"
    );

    let receipts = migration.receipts();
    let called: Vec<&str> = receipts
        .calls
        .iter()
        .map(|call| call.tool.as_str())
        .collect();
    assert_eq!(
        called,
        vec!["read_file", "write_file", "run_check"],
        "the three tools the script calls really ran"
    );
    assert!(
        receipts.calls.iter().all(|call| call.outcome == "ok"),
        "a refusal would make the edit above somebody else's doing: {receipts:?}"
    );
}

/// **A migration leaves no worktree behind, whatever became of it.**
///
/// The host-fact assertion is about a path the attempt worked under; this is
/// about that path not surviving. Both directions of the same discipline, and
/// the second is cheap to lose — `Workspace`'s guard is a `Drop` on a value this
/// module holds in an `Arc`, and an `Arc` that escaped would leave the tree on
/// disk with nothing failing.
#[tokio::test]
async fn no_worktree_survives_the_attempt() {
    let world = migration_world().await;
    for (name, script) in [
        ("migrates", migrates()),
        // A final message that is not the schema at all: the attempt fails, and
        // the worktree must still be gone.
        ("malformed", vec![MockTurn::text("this is not the schema")]),
    ] {
        let _ = run_migration(MockCompletionModel::new(script), &world).await;

        // The root itself has to exist, or "empty" would be the vacuous truth of
        // an attempt that never prepared a workspace at all.
        assert!(
            world.workspace_root().exists(),
            "the `{name}` attempt never prepared a workspace, so nothing was proven"
        );
        let leftovers: Vec<String> = std::fs::read_dir(world.workspace_root())
            .expect("the workspace root is readable")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            // The scratch `HOME` is created beside the worktree and is removed
            // with it; anything else is a tree that outlived its attempt.
            .filter(|name| !name.ends_with(".home"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "the `{name}` attempt left a worktree behind: {leftovers:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The second criterion: the model cannot return a verdict
// ---------------------------------------------------------------------------

/// A tree that passed every check and whose rescan proved the repair.
///
/// `cve_evaluate`'s own acceptance world, reached through the same builders, so
/// that "accepted" here means exactly what it means there rather than what this
/// file decided it should mean.
async fn a_proved_tree() -> Evaluation {
    evaluate(&contract_scanned_by("1.2.3"), &tree_rescanned_by("1.2.3"))
        .await
        .expect("an evaluation that was not cancelled")
}

/// A tree whose build failed.
async fn a_tree_that_will_not_build() -> Evaluation {
    evaluate(
        &contract_scanned_by("1.2.3"),
        &tree_where(GO_BUILD, exit(1), stdout("")),
    )
    .await
    .expect("an evaluation that was not cancelled")
}

/// A tree every check passed and no rescan was ever compared against.
async fn a_tree_nothing_was_proved_about() -> Evaluation {
    evaluate(&contract(), &green_tree())
        .await
        .expect("an evaluation that was not cancelled")
}

/// Run `script` against a fresh migration world and hand back both.
///
/// The world travels with the attempt because [`MigrationWorld`] owns the
/// temporary directories the attempt ran in, and a helper that dropped it would
/// take the fixture tree with it.
async fn attempted(script: Vec<MockTurn>) -> (MigrationWorld, MigrationAttempt) {
    let world = migration_world().await;
    let attempt = run_migration(MockCompletionModel::new(script), &world)
        .await
        .expect("a scripted migration completes");
    (world, attempt)
}

/// The same, for a script that has to be built from the world.
async fn attempted_with(build: impl Fn(&MigrationWorld) -> Vec<MockTurn>) -> MigrationAttempt {
    let world = migration_world().await;
    let script = build(&world);
    run_migration(MockCompletionModel::new(script), &world)
        .await
        .expect("a scripted migration completes")
}

/// The one shape this attempt left behind, or a failure naming what it found
/// instead.
///
/// **Exactly one**, and that is the assertion the four shape lanes below rest
/// on. `assert!(!forbidden.is_empty())` would pass for a classifier that
/// answered every rule for every diff, which is the same defect as one that
/// answered `NeedsWork` for everything — it would just be harder to see.
fn the_one_shape(attempt: &MigrationAttempt) -> &ForbiddenShape {
    assert_eq!(
        attempt.forbidden.len(),
        1,
        "each script here is the uniform migration plus exactly one further \
         thing, so exactly one rule may fire: {:#?}",
        attempt.forbidden
    );
    &attempt.forbidden[0]
}

/// **The model's claim is recorded, and nothing branches on it.**
///
/// The model says it finished and changed nothing at all, so there is no diff to
/// classify and the checks are the only thing left that can decide. The same
/// attempt is then put to two evaluations, and it comes out differently — which
/// is the assertion, because a status derived from `claimed_complete` would come
/// out the same way twice.
///
/// The two premises are what stop this passing for the wrong reason: the claim
/// really is `true` in the record, and the diff really is empty, so neither
/// "the claim was false anyway" nor "a shape refused it" can be what produced
/// the refusal.
#[tokio::test]
async fn the_model_cannot_return_a_verdict() {
    let (_world, attempt) = attempted(claims_success_without_editing()).await;

    assert!(
        attempt.report.claimed_complete,
        "the claim is recorded as evidence, which is the premise for the rest \
         of this lane"
    );
    assert!(
        attempt.changed.is_empty(),
        "the model changed nothing, so nothing but the checks can be deciding \
         below: {:?}",
        attempt.changed
    );
    assert!(
        attempt.forbidden.is_empty(),
        "and no shape was found, for the same reason: {:#?}",
        attempt.forbidden
    );

    let refused = GroupStatus::of(
        &a_tree_that_will_not_build().await,
        &attempt.forbidden,
        attempt.undeclared.as_ref(),
    );
    assert!(
        matches!(
            &refused,
            GroupStatus::NeedsWork {
                reason: NeedsWork::CheckFailed { check }
            } if check == GO_BUILD
        ),
        "a model that says it finished does not make a tree that will not \
         build clean, and the refusal names the check that decided: {refused:?}"
    );

    let accepted = GroupStatus::of(
        &a_proved_tree().await,
        &attempt.forbidden,
        attempt.undeclared.as_ref(),
    );
    assert_eq!(
        accepted,
        GroupStatus::Clean,
        "and the same claim over a proved tree is clean — so what changed the \
         answer was the evaluation and not the claim"
    );
}

/// **A model that disowns its own edit does not thereby refuse the group.**
///
/// The other direction of the same rule, and the one a `claimed_complete` read
/// would be easiest to smuggle in as: it looks conservative. It is not — the
/// tree is proved better than the one it started from, and throwing that away
/// on a model's opinion of itself is the milestone's whole argument in reverse.
#[tokio::test]
async fn a_disowned_edit_the_checks_prove_is_still_clean() {
    let (_world, attempt) = attempted(migrates_and_disowns_it()).await;

    assert!(
        !attempt.report.claimed_complete,
        "the premise: the model said it had not finished"
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::Clean,
        "the checks decide, and they proved this tree"
    );
}

/// **Nothing anywhere in this workspace reads `claimed_complete` to decide
/// anything.**
///
/// The lanes above show that one path ignores the claim. This is the other half
/// of the criterion, which is a negative about the whole codebase and which no
/// amount of testing one path can establish: a second reader added tomorrow in
/// `disposition`, in the committer, in `fiddle-cli` would satisfy every
/// assertion above and break the rule.
///
/// So the source is read — every crate's `src`, every `.rs` file under it — and
/// what is looked for is a **field access**: `.claimed_complete`. That is the
/// only way the value is reachable in Rust, and it is what makes the rule
/// mechanical rather than a matter of reading each line. Every access has to be
/// a plain `field: report.claimed_complete,` recording; anything else fails here
/// naming the file and line.
///
/// An allowlist and not a search for `if`, deliberately: `let done =
/// report.claimed_complete;` followed by a branch on `done` is invisible to a
/// search for the branch and caught by this.
///
/// **What it does not see, stated rather than left to be found.** It does not
/// lex Rust, so an occurrence with no leading `.` is not examined — prose in a
/// prompt, a JSON key in a scripted model's reply, and the field's own
/// declaration are all of that shape, and so would a `RepairReport {
/// claimed_complete, .. }` destructuring be. That is a real gap and it is the
/// narrow one: it is not how anybody writes the read that the criterion is
/// worried about. This is evidence, not proof; what it buys is that adding a
/// reader is a thing somebody has to come here and argue for.
#[test]
fn nothing_in_this_workspace_decides_on_claimed_complete() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("this crate lives under the workspace's crates directory");

    let mut accesses = 0usize;
    let mut declared = false;
    let mut reads: Vec<String> = Vec::new();
    for file in rust_sources(crates) {
        let text = std::fs::read_to_string(&file).expect("a source file of this workspace");
        for (n, line) in text.lines().enumerate() {
            declared |= line.trim() == "pub claimed_complete: bool,";
            for (at, _) in line.match_indices("claimed_complete") {
                if !line[..at].ends_with('.') {
                    continue;
                }
                accesses += 1;
                if !recorded_rather_than_read(line) {
                    reads.push(format!("{}:{}: {}", file.display(), n + 1, line.trim()));
                }
            }
        }
    }

    // Two premises, because each of them failing would leave a lane that
    // walked the tree and asserted nothing. The first is that the name still
    // names the field; the second that somebody still reaches it, so the
    // allowlist is exercised rather than merely unviolated.
    assert!(
        declared,
        "no source under {} declares `pub claimed_complete: bool`, so this lane \
         is looking for the wrong name",
        crates.display()
    );
    assert!(
        accesses > 0,
        "nothing under {} reads the field at all, so an allowlist over its \
         readers proved nothing",
        crates.display()
    );
    assert!(
        reads.is_empty(),
        "claimed_complete is evidence and only evidence; these reach it \
         somewhere a plain recording does not explain:\n{}",
        reads.join("\n")
    );
}

/// Every `.rs` file under each crate's `src`, tests excluded.
///
/// `src` only: a test may read the claim freely — the lanes above do — and what
/// the criterion is about is the product.
fn rust_sources(crates: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let mut pending: Vec<std::path::PathBuf> = std::fs::read_dir(crates)
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

/// Whether this line copies the claim into a field of something else and does
/// nothing further with it.
///
/// The whole line has to be `<field>: <binding>.claimed_complete,` — nothing
/// before it, nothing after it. That is what rules out the ways a read hides in
/// plain sight: `claimed: report.claimed_complete && ran_clean,` has something
/// after it, and `let done = report.claimed_complete;` does not end in a comma
/// after a field name.
fn recorded_rather_than_read(line: &str) -> bool {
    line.trim()
        .strip_suffix(".claimed_complete,")
        .and_then(|prefix| prefix.split_once(": "))
        .is_some_and(|(field, from)| {
            let identifier =
                |s: &str| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_');
            identifier(field) && identifier(from)
        })
}

// ---------------------------------------------------------------------------
// The second criterion: every forbidden shape, each with its own case
// ---------------------------------------------------------------------------

/// **A uniform rename is not a forbidden shape.**
///
/// The positive control the four lanes below need, and the one assertion in this
/// section that a classifier answering `NeedsWork` for every diff would fail.
/// The edit it makes is a real one — both files rewritten, the call site renamed
/// in the `_test.go` too, which is exactly what the scope rules require of a
/// uniform migration — so this is not the empty diff of an attempt that did
/// nothing.
#[tokio::test]
async fn a_uniform_rename_reaching_the_test_file_is_in_scope() {
    let (_world, attempt) = attempted(migrates_uniformly()).await;

    assert_eq!(
        attempt
            .changed
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>(),
        vec![SOURCE.to_string(), TEST_SOURCE.to_string()],
        "the premise: this attempt really rewrote both files"
    );
    assert!(
        attempt.forbidden.is_empty(),
        "a uniform rename is the one exception the scope rules allow: {:#?}",
        attempt.forbidden
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::Clean
    );
}

/// **An added `t.Skip` puts the group back to a person.**
///
/// The shape that most needs its own case: switching a test off makes the checks
/// *pass*, so a table that consulted them first would commit this.
/// [`a_proved_tree`] is what makes that concrete — the evaluation handed in here
/// is the accepting one.
#[tokio::test]
async fn an_added_skip_makes_the_group_needs_work() {
    let (_world, attempt) = attempted(adds_a_skip()).await;

    let shape = the_one_shape(&attempt);
    assert!(
        matches!(shape, ForbiddenShape::AddedSkip { path, line }
            if path == TEST_SOURCE && line.contains("t.Skip(")),
        "the skip is named, with the line it was written on: {shape:?}"
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::NeedsWork {
            reason: NeedsWork::OutOfScope(shape.clone())
        },
        "and every check passing does not rescue it"
    );
}

/// **A weakened assertion puts the group back to a person.**
///
/// Changed rather than deleted, so the file is the same length and carries the
/// same number of lines: a rule counting lines would find nothing here.
#[tokio::test]
async fn a_changed_test_assertion_makes_the_group_needs_work() {
    let (_world, attempt) = attempted(weakens_an_assertion()).await;

    let shape = the_one_shape(&attempt);
    assert!(
        matches!(shape, ForbiddenShape::ChangedTestAssertion { path, assertion }
            if path == TEST_SOURCE && assertion.contains("t.Errorf(")),
        "the assertion that left the file is quoted as it read: {shape:?}"
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::NeedsWork {
            reason: NeedsWork::OutOfScope(shape.clone())
        }
    );
}

/// **A `replace` directive puts the group back to a person.**
///
/// The only one of the four that is not about Go source, and the reason the
/// rules are applied per file rather than to every changed path: a `replace`
/// line means something in a `go.mod` and nothing anywhere else.
#[tokio::test]
async fn a_replace_directive_makes_the_group_needs_work() {
    let attempt = attempted_with(adds_a_replace_directive).await;

    let shape = the_one_shape(&attempt);
    assert!(
        matches!(shape, ForbiddenShape::ReplaceDirective { path, directive }
            if path == "go.mod" && directive.starts_with("replace ")),
        "the directive is named, in the file it was written to: {shape:?}"
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::NeedsWork {
            reason: NeedsWork::OutOfScope(shape.clone())
        }
    );
}

/// **New control flow puts the group back to a person.**
///
/// The count is asserted on both sides, which is what separates this rule from
/// "the added line contains the word `if`". The uniform rename in the same
/// script leaves a `_test.go` whose own `if` is still there and still one, and
/// that file does not appear here.
#[tokio::test]
async fn new_control_flow_makes_the_group_needs_work() {
    let (_world, attempt) = attempted(adds_control_flow()).await;

    let shape = the_one_shape(&attempt);
    assert_eq!(
        shape,
        &ForbiddenShape::NewControlFlow {
            path: SOURCE.to_string(),
            keyword: "if",
            before: 0,
            after: 1,
        },
        "the branch that appeared is named, with what the file had before"
    );
    assert_eq!(
        GroupStatus::of(
            &a_proved_tree().await,
            &attempt.forbidden,
            attempt.undeclared.as_ref()
        ),
        GroupStatus::NeedsWork {
            reason: NeedsWork::OutOfScope(shape.clone())
        }
    );
}

/// **Renaming a call site that sits inside a branch is not new control flow.**
///
/// The false positive the counting rule exists to avoid, and the one a
/// line-based rule cannot: the edited line *is* an `if`, and the file has no
/// more branches than it had. Without this lane the four above are all satisfied
/// by a classifier that refuses any diff touching a control-flow keyword, which
/// would put every real migration back to a person.
#[tokio::test]
async fn a_rename_on_a_branch_line_is_not_new_control_flow() {
    let world = migration_world().await;
    // The one assertion's condition rewritten in place: same branch, different
    // call. `if testing.Short()` becomes `if !testing.Verbose()`.
    let rewritten = MIGRATION_TEST_BEFORE.replace("if testing.Short()", "if !testing.Verbose()");
    assert_ne!(
        rewritten, MIGRATION_TEST_BEFORE,
        "the premise: the replacement really rewrote the branch line"
    );
    let attempt = run_migration(
        MockCompletionModel::new(edits(&[(TEST_SOURCE, rewritten)])),
        &world,
    )
    .await
    .expect("a scripted migration completes");

    assert!(
        attempt.forbidden.is_empty(),
        "the branch line changed and the number of branches did not: {:#?}",
        attempt.forbidden
    );
}

/// **A clean group is exactly an accepted one.**
///
/// Task 13 left this as the open question — `GroupStatus::Clean` almost
/// certainly *is* [`Evaluation::accepted`] — and this is where it is settled
/// rather than assumed. Over an empty shape list the two answers agree on every
/// evaluation this suite can build, including the one that is neither accepted
/// nor rejected: five green checks and nothing proved is **not** clean, which is
/// the arm a status derived from `rejected()` would get backwards.
#[tokio::test]
async fn a_clean_group_is_exactly_an_accepted_one() {
    for (name, evaluation) in [
        ("proved", a_proved_tree().await),
        ("will not build", a_tree_that_will_not_build().await),
        ("nothing proved", a_tree_nothing_was_proved_about().await),
    ] {
        assert_eq!(
            GroupStatus::of(&evaluation, &[], None) == GroupStatus::Clean,
            evaluation.accepted(),
            "`{name}`: clean and accepted must be the same question"
        );
    }

    // And the third of them is the one worth naming: nothing went wrong with
    // the tree and nothing was proved about it either, so the group stops and
    // the record says why rather than blaming a check.
    let unproved = a_tree_nothing_was_proved_about().await;
    assert!(unproved.first_failure().is_none(), "every check passed");
    assert_eq!(
        GroupStatus::of(&unproved, &[], None),
        GroupStatus::NeedsWork {
            reason: NeedsWork::Unproved(RescanVerdict::NotCompared)
        }
    );
}

// ---------------------------------------------------------------------------
// The one rule that survives the ecosystem: the attempt's own declaration
// ---------------------------------------------------------------------------

/// A [`FileEdit`] whose two sides differ, because one whose sides match is not a
/// change and would make every fixture below prove nothing.
///
/// The paths are Python's on purpose. Nothing in [`undeclared`] can tell a
/// `requirements.txt` from a `go.mod` from a `README`, and a fixture written in
/// Go paths would leave that indistinguishable from a rule that happened to
/// allow them.
fn edit(path: &str) -> FileEdit {
    FileEdit {
        path: WorkspacePath::parse(path).expect("test path is relative and clean"),
        before: Content::Text("old".to_string()),
        after: Content::Text("new".to_string()),
    }
}

/// **An edit the attempt did not declare is refused, and the refusal names it.**
///
/// The direction that matters most: the attempt changed a file it did not
/// mention, which is exactly the shape of an edit nobody reviewed. Both files
/// really are in the diff, so this cannot pass because the fixture was empty.
#[test]
fn an_edit_the_attempt_did_not_declare_is_refused() {
    let declared = vec!["requirements.txt".to_string()];
    let touched = vec![edit("requirements.txt"), edit("setup.py")];
    let refusal = undeclared(&declared, &touched).expect("setup.py was not declared");
    assert!(
        refusal.to_string().contains("setup.py"),
        "the refusal must name the file: {refusal}"
    );
}

/// **A declared file the attempt did not touch is refused too**, so the
/// declaration cannot be padded.
///
/// Without this half, an attempt could satisfy the rule above by declaring every
/// path in the repository.
#[test]
fn a_declared_file_the_attempt_did_not_touch_is_refused() {
    let declared = vec!["requirements.txt".to_string(), "poetry.lock".to_string()];
    let touched = vec![edit("requirements.txt")];
    let refusal = undeclared(&declared, &touched).expect("poetry.lock was declared and untouched");
    assert!(refusal.to_string().contains("poetry.lock"), "{refusal}");
}

/// **An honest declaration is not a breach**, which is the positive control the
/// two lanes above need: a function answering `Some` for every input would
/// satisfy both of them.
#[test]
fn a_declaration_that_matches_the_diff_is_no_breach() {
    let declared = vec!["requirements.txt".to_string(), "app/main.py".to_string()];
    let touched = vec![edit("app/main.py"), edit("requirements.txt")];
    assert!(
        undeclared(&declared, &touched).is_none(),
        "the same set in a different order is the same set"
    );
}

/// **A real attempt that understated its diff is put back to a person, and the
/// verdict names the file it did not declare.**
///
/// The three lanes above are the rule on its own; this is the rule *wired*. The
/// premises are what stop it passing for another reason: the diff really holds
/// both files, no forbidden shape fired, and the evaluation handed in is the
/// accepting one — so a green check does not rescue an edit nobody declared,
/// which is why the row sits above the checks.
#[tokio::test]
async fn an_attempt_that_understated_its_diff_is_needs_work_over_green_checks() {
    let (_world, attempt) = attempted(migrates_and_understates_it()).await;

    assert_eq!(
        attempt
            .changed
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>(),
        vec![SOURCE.to_string(), TEST_SOURCE.to_string()],
        "the premise: git saw both files change"
    );
    assert_eq!(
        attempt.report.changed_files,
        vec![SOURCE.to_string()],
        "and the premise's other half: the attempt declared one of them"
    );
    assert!(
        attempt.forbidden.is_empty(),
        "no scope rule fired, so the declaration rule is the only thing that \
         can refuse this: {:#?}",
        attempt.forbidden
    );

    let status = GroupStatus::of(
        &a_proved_tree().await,
        &attempt.forbidden,
        attempt.undeclared.as_ref(),
    );
    let GroupStatus::NeedsWork {
        reason: NeedsWork::Undeclared(breach),
    } = &status
    else {
        panic!("an undeclared edit is not clean, whatever the checks said: {status:?}");
    };
    assert_eq!(
        breach.unannounced,
        vec![TEST_SOURCE.to_string()],
        "the one file it changed and did not mention: {breach:?}"
    );
    assert!(
        breach.unmet.is_empty(),
        "and it did mention the other one: {breach:?}"
    );
    assert!(
        breach.to_string().contains(TEST_SOURCE),
        "the sentence an operator reads names the file: {breach}"
    );
}

/// **And the uniform migration, whose declaration matches, still lands.**
///
/// The positive control for the lane above at the level it operates: without it,
/// a wiring that answered `NeedsWork` for every attempt would pass there.
/// [`a_uniform_rename_reaching_the_test_file_is_in_scope`] asserts the same
/// `Clean` and is now passed the breach as well, so it is the assertion that a
/// correct declaration reaches it.
#[tokio::test]
async fn an_attempt_whose_declaration_matches_its_diff_has_no_breach() {
    let (_world, attempt) = attempted(migrates_uniformly()).await;

    assert_eq!(
        attempt.undeclared, None,
        "both files declared and both changed: {:?}",
        attempt.undeclared
    );
}

/// **The run's own pre-briefing edit is not the attempt's to declare — and
/// nothing else is excused with it.**
///
/// A sweep applies the bump before the model is briefed, so the worktree is
/// already dirty when the attempt starts and `HEAD` is behind the tree by an edit
/// the attempt had no part in. Without this, the declaration rule refuses every
/// ordinary sweep: the honest report of a bump that needed no further work is an
/// empty `changed_files`, and the diff holds the bump.
///
/// The lane is one attempt with **both** kinds of path in its diff, which is what
/// makes it an assertion about the boundary rather than about either side. The
/// bumped file is excused and the undeclared source edit beside it is not, so an
/// exclusion widened tomorrow to cover the whole diff fails here.
#[tokio::test]
async fn what_the_run_changed_before_briefing_is_excused_and_nothing_beside_it_is() {
    let world = migration_world().await;
    let workspace = world.workspace();

    // Stands in for `go get` and `go mod tidy`: a tracked file the *run* moves,
    // in the attempt's worktree, before any model is briefed. A comment line, so
    // no scope rule reads anything into it — this lane is about the declaration
    // and not about `classify`.
    let manifest = workspace.root().join("go.mod");
    let bumped = std::fs::read_to_string(&manifest).expect("the fixture tree has a go.mod")
        + "\n// moved by the run, before the attempt began\n";
    std::fs::write(&manifest, bumped).expect("the worktree is writable");

    // And the attempt edits one file and declares nothing at all.
    let script = vec![
        MockTurn::tool_call("r", "read_file", json!({ "path": TEST_SOURCE })),
        MockTurn::tool_call(
            "w",
            "write_file",
            json!({ "path": TEST_SOURCE, "contents": RENAMED_TEST }),
        ),
        MockTurn::text(
            r#"{"changed_files":[],"summary":"the bump was enough","claimed_complete":true}"#,
        ),
    ];
    let attempt = GroupMigration::new(MockCompletionModel::new(script), world.config())
        .migrate(&workspace, &world.group)
        .await
        .expect("a scripted migration completes");

    assert_eq!(
        attempt
            .changed
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>(),
        vec!["go.mod".to_string(), TEST_SOURCE.to_string()],
        "the premise: the diff holds the run's edit and the attempt's together"
    );

    let breach = attempt
        .undeclared
        .as_ref()
        .expect("the attempt declared nothing and edited a file");
    assert_eq!(
        breach.unannounced,
        vec![TEST_SOURCE.to_string()],
        "the bumped manifest is the run's and is excused; the source edit beside \
         it is the attempt's and is not: {breach:?}"
    );
    assert!(
        breach.unmet.is_empty(),
        "the run's own paths are all still in the diff: {breach:?}"
    );
}

// ---------------------------------------------------------------------------
// The third criterion: commit named files, revert named files, rewrite nothing
// (Task 15)
// ---------------------------------------------------------------------------

/// The advisories a landing lane's group is about.
///
/// Two of them, because the body has to name **every** id and a group of one
/// cannot tell "names every id" from "names an id". Values no other fixture in
/// this workspace spells, so a body that named them came from this group.
const LANDED: [&str; 2] = ["CVE-2026-1", "CVE-2026-2"];

/// The advisory a group that is *not* landed is about.
///
/// Deliberately not one of [`LANDED`]: the whole of the second criterion is that
/// this id reaches no commit body, and an id shared with a group that does land
/// would make the absence unassertable.
const NOT_LANDED: &str = "CVE-2026-3";

/// A status a group reaches through a failing check.
///
/// The *ordinary* refusal, used wherever a lane only needs "this group is not
/// clean". The refusal that matters — a forbidden shape over green checks — has
/// a lane of its own; see
/// [`a_forbidden_shape_over_green_checks_reverts_rather_than_committing`].
fn refused() -> GroupStatus {
    GroupStatus::NeedsWork {
        reason: NeedsWork::CheckFailed {
            check: GO_BUILD.to_string(),
        },
    }
}

/// Land a clean group naming `cves`, and hand back the world it landed in.
///
/// The world travels with the result because it owns the temporary directory the
/// repository is in, and because every assertion afterwards is a question about
/// that repository.
async fn run_group_clean(cves: &[&str]) -> (LandingWorld, Landed) {
    let world = landing_world(cves);
    let landed = land(
        &world.tree,
        &world.group,
        &GroupStatus::Clean,
        &world.changed,
    )
    .await
    .expect("a clean group lands");
    (world, landed)
}

/// The same for a group that is going back to a person.
async fn run_group_needs_work(cves: &[&str]) -> (LandingWorld, Landed) {
    let world = landing_world(cves);
    let landed = land(&world.tree, &world.group, &refused(), &world.changed)
        .await
        .expect("a needs-work group reverts");
    (world, landed)
}

/// Nothing in `calls` stages by directory.
///
/// Two readings of the same rule, because the plan states it as a substring rule
/// and a substring rule is exactly as strong as the spellings somebody thought
/// of. The token reading catches `add --all`, `add -A -- .` and an `-a` that
/// arrived in a longer commit invocation; the substring reading is the one the
/// criterion is written in, and keeping both means neither can be quietly
/// satisfied by rewording.
///
/// One spelling deliberately does **not** trip this and is worth naming:
/// `commit --allow-empty`, which [`fold_commit_argv`] issues, contains
/// `commit --a` and not `commit -a`. That is the intended reading — an empty
/// commit stages nothing at all — and the token check below confirms it rather
/// than resting on where the hyphens fell.
///
/// [`fold_commit_argv`]: fiddle_runtime::cve::fold::fold_commit_argv
fn nothing_is_staged_by_directory(calls: &[String]) {
    assert!(
        !calls.is_empty(),
        "no git was recorded at all, so every negative below holds for the \
         emptiest of reasons: the seam was never wired in"
    );
    for call in calls {
        for forbidden in ["add -A", "add .", "commit -a"] {
            assert!(
                !call.contains(forbidden),
                "`{forbidden}` stages what nothing classified: {call}"
            );
        }
        let tokens: Vec<&str> = call.split_whitespace().collect();
        let has = |token: &str| tokens.contains(&token);
        assert!(
            !(has("add") && (has("-A") || has("--all") || has("."))),
            "an `add` that names a directory rather than the files the group \
             edited: {call}"
        );
        assert!(
            !(has("commit") && (has("-a") || has("--all"))),
            "a `commit` that stages on the caller's behalf: {call}"
        );
    }
}

/// Nothing in `calls` rewrites anything that is already on the branch.
///
/// The six operations of the criterion, and the one worth spelling out is
/// `--amend`: it is the obvious way to attach a further change to the commit
/// before it, and on a branch this run is *reusing* the commit before it may
/// belong to a previous run and already be pushed. Rewriting it would then need a
/// force push, which is the first entry on the same list.
fn nothing_rewrites_history(calls: &[String]) {
    assert!(
        !calls.is_empty(),
        "no git was recorded at all, so this proves nothing about what ran"
    );
    for call in calls {
        for forbidden in [
            "push --force",
            "--force-with-lease",
            "reset",
            "rebase",
            "commit --amend",
            "--amend",
            "--no-verify",
        ] {
            assert!(!call.contains(forbidden), "`{forbidden}`: {call}");
        }
    }
}

/// **The world a landing runs in holds a dirty file the group did not edit.**
///
/// The denominator for the staging criterion, and a lane rather than a comment
/// for [`the_world_holds_everything_the_prompt_must_not`]'s reason. If every
/// dirty path in the tree were also a path the group edited, `add -A` and
/// `add -- go.mod go.sum` would leave byte-identical commits and every assertion
/// below would hold for a subject that staged the whole worktree.
///
/// It also pins the second premise the negatives rest on: **nothing is recorded
/// before the subject runs**, so a non-empty call list afterwards is the
/// subject's doing and not the fixture's.
#[test]
fn a_landing_world_has_something_outside_the_change_set_to_get_wrong() {
    let world = landing_world(&LANDED);

    let changed: Vec<&str> = world.changed.iter().map(|path| path.as_str()).collect();
    assert_eq!(changed, ["go.mod", "go.sum"]);
    assert!(
        !changed.contains(&LANDING_UNRELATED),
        "the discriminating file must be outside the change set"
    );
    assert!(
        !world.tree.is_clean_at(&[LANDING_UNRELATED]),
        "and it must be dirty, or staging by name and by directory agree"
    );
    assert!(
        !world.tree.is_clean_at(&["go.mod", "go.sum"]),
        "and the bump must really have changed the tree, or a commit of nothing \
         would satisfy every lane below"
    );
    assert!(
        world.tree.git_calls().is_empty(),
        "construction must record nothing, or `what the subject ran` is a list \
         holding what this fixture ran: {:?}",
        world.tree.git_calls()
    );
    assert!(
        !world.tree.all_commit_bodies().is_empty(),
        "there has to be a history for an id to be absent from"
    );
}

/// **A clean group commits only the files it edited, and names every advisory.**
///
/// Three claims, and each is measured through a different instrument so that no
/// two of them can be satisfied by one accident:
///
/// - *only the files it edited* — read off the commit at `HEAD` rather than off
///   the recorded `add`, because what the criterion is about is what reached the
///   branch. `LANDING_UNRELATED` is dirty and stays dirty, which is the half a
///   commit that staged everything would fail.
/// - *names every advisory* — asked through
///   [`FixedInCommits`], which is the reader `cve::dedup` recovers the
///   already-fixed set with on the next run. A substring match here would be a
///   second opinion about what naming an advisory is, and the two would be free
///   to drift; through the real reader they cannot.
/// - *stages by name* — over the recorded call list, which is non-empty because
///   the fixture records nothing and the subject records everything.
#[tokio::test]
async fn a_clean_group_commits_only_the_files_it_edited_and_names_every_cve() {
    let (world, landed) = run_group_clean(&LANDED).await;

    assert_eq!(landed, Landed::Committed);
    assert_eq!(
        world.tree.staged_paths(),
        ["go.mod", "go.sum"],
        "the commit carries the group's own files and nothing beside them"
    );
    assert!(
        world.tree.is_clean_at(&["go.mod", "go.sum"]),
        "and they are on the branch rather than still sitting dirty"
    );
    assert!(
        !world.tree.is_clean_at(&[LANDING_UNRELATED]),
        "{LANDING_UNRELATED} was dirty and had nothing to do with this group, so \
         it must still be dirty"
    );

    let fixed = FixedInCommits::read(&world.tree.head_commit_body());
    for cve in LANDED {
        assert!(
            fixed.names(cve),
            "the log is what recovers the fixed set for OS findings next run, \
             and it does not name {cve}: {}",
            world.tree.head_commit_body()
        );
    }

    nothing_is_staged_by_directory(&world.tree.git_calls());
    assert_eq!(
        world.tree.git_calls().first().map(String::as_str),
        Some("add -f -- go.mod go.sum"),
        "staging is the group's paths, by name: {:?}",
        world.tree.git_calls()
    );
}

/// **A needs-work group reverts, and leaves no id in any commit body.**
///
/// An id in a body is a claim it was fixed, and the next run's log scan believes
/// it — that is the same claiming-proof-from-silence failure `cve::dedup`'s
/// header records from 2026-08-12, arriving from the other side.
///
/// The absence is only evidence if the reader would have found the id had it been
/// there, so the same [`FixedInCommits`] is asked about a word this history really
/// carries. And *no commit was made* is asserted as the history being byte-for-
/// byte what it was, rather than as the narrower claim that one particular id is
/// missing from it.
///
/// The revert is by name too: `LANDING_UNRELATED` was dirty before it and is dirty
/// after, which is what separates `git checkout HEAD -- go.mod go.sum` from
/// `git checkout .`.
#[tokio::test]
async fn a_needs_work_group_reverts_and_leaves_no_id_in_any_commit_body() {
    let (world, landed) = run_group_needs_work(&[NOT_LANDED]).await;

    assert_eq!(landed, Landed::Reverted);
    assert!(
        world.tree.is_clean_at(&["go.mod", "go.sum"]),
        "the group's own files are back the way HEAD has them"
    );
    assert!(
        !world.tree.is_clean_at(&[LANDING_UNRELATED]),
        "and a revert by name left the file it was not given alone"
    );

    let bodies = world.tree.all_commit_bodies();
    assert_eq!(
        bodies, world.history_before,
        "a needs-work group makes no commit at all, so the history is what it was"
    );
    let fixed = FixedInCommits::read(&bodies);
    assert!(
        !fixed.names(NOT_LANDED),
        "an id in a body is a claim it was fixed, and the next run's log scan \
         believes it: {bodies}"
    );
    assert!(
        fixed.names("chore"),
        "the reader really reads this history — otherwise the absence above is a \
         fact about the reader: {bodies}"
    );

    nothing_rewrites_history(&world.tree.git_calls());
}

/// **A forbidden shape with green checks reverts rather than committing.**
///
/// The one case [`GroupStatus`] and [`Evaluation::accepted`] come apart on, and
/// the one where committing lands a `t.Skip` on the branch. A landing derived
/// from `accepted()` would pass every other lane in this section and fail here —
/// and the damage would not stop at the commit, because `cve::fold`'s
/// `ended_clean` still reads `accepted()`, so this group would both land *and*
/// fold the next one.
///
/// The premises are what make it a divergence rather than a coincidence: the diff
/// really carries a forbidden shape, the evaluation really is accepted, and the
/// two really disagree.
#[tokio::test]
async fn a_forbidden_shape_over_green_checks_reverts_rather_than_committing() {
    let (_migrated, attempt) = attempted(adds_a_skip()).await;
    let evaluation = a_proved_tree().await;
    let status = GroupStatus::of(&evaluation, &attempt.forbidden, attempt.undeclared.as_ref());

    assert!(
        matches!(the_one_shape(&attempt), ForbiddenShape::AddedSkip { .. }),
        "the premise: this attempt switched a test off"
    );
    assert!(
        evaluation.accepted(),
        "the premise: every check passed and the rescan cleared, so a landing \
         that read the evaluation would commit this"
    );
    assert_ne!(
        status,
        GroupStatus::Clean,
        "and the status says otherwise, which is the divergence"
    );

    let world = landing_world(&LANDED);
    let landed = land(&world.tree, &world.group, &status, &world.changed)
        .await
        .expect("a refused group reverts");

    assert_eq!(
        landed,
        Landed::Reverted,
        "GroupStatus is the commit gate, not Evaluation::accepted"
    );
    assert_eq!(
        world.tree.all_commit_bodies(),
        world.history_before,
        "nothing was committed, so nothing on this branch claims a fix"
    );
    let fixed = FixedInCommits::read(&world.tree.all_commit_bodies());
    for cve in LANDED {
        assert!(
            !fixed.names(cve),
            "an out-of-scope group must not claim {cve} was fixed: {}",
            world.tree.all_commit_bodies()
        );
    }
    assert!(
        world.tree.is_clean_at(&["go.mod", "go.sum"]),
        "and the edit is off the tree"
    );
}

/// **A file the attempt created is reverted too.**
///
/// `git checkout HEAD --` cannot put back a path `HEAD` does not carry — it
/// refuses the pathspec, which would fail the revert for every path beside it —
/// so a changed set holding a created file is the case a one-command revert gets
/// wrong. Left behind, the file is still in the worktree when the *next* group
/// stages, and a `t.Skip` in it would reach the branch under that group's commit.
#[tokio::test]
async fn a_file_the_attempt_created_does_not_survive_the_revert() {
    let world = landing_world(&[NOT_LANDED]).and_a_created_file();
    assert!(
        !world.tree.is_clean_at(&[LANDING_CREATED]),
        "the premise: the created file is really in the tree"
    );

    let landed = land(&world.tree, &world.group, &refused(), &world.changed)
        .await
        .expect("a needs-work group reverts");

    assert_eq!(landed, Landed::Reverted);
    assert!(
        world
            .tree
            .is_clean_at(&["go.mod", "go.sum", LANDING_CREATED]),
        "every path the group changed is back the way HEAD has it, creations \
         included: {:?}",
        world.tree.git_calls()
    );
    assert!(
        !world.tree.is_clean_at(&[LANDING_UNRELATED]),
        "and still by name — the file the revert was not given is untouched"
    );
    nothing_rewrites_history(&world.tree.git_calls());
}

/// **A clean group that changed nothing is refused rather than committed.**
///
/// Neither nearby answer is honest. `--allow-empty` would put a body naming every
/// one of this group's advisories on the branch with no fix under it, which the
/// next run's log scan reads as *these are done*; answering
/// [`Landed::Committed`] with no commit would tell the fold rule the branch
/// carries a tree it does not.
#[tokio::test]
async fn a_clean_group_that_changed_nothing_commits_nothing_and_says_so() {
    let world = landing_world(&LANDED);

    let refusal = land(&world.tree, &world.group, &GroupStatus::Clean, &[])
        .await
        .expect_err("a clean group with an empty change set is refused");

    assert!(
        matches!(refusal, CapabilityError::NothingProposed),
        "and it is the refusal that says the tree did not change: {refusal:?}"
    );
    assert_eq!(
        world.tree.all_commit_bodies(),
        world.history_before,
        "no commit was made, empty or otherwise"
    );
    let fixed = FixedInCommits::read(&world.tree.all_commit_bodies());
    for cve in LANDED {
        assert!(!fixed.names(cve), "and nothing claims {cve} was fixed");
    }
}

/// **A fold is recorded as an empty commit that names every id and amends
/// nothing.**
///
/// `fold_commit_argv` decides the flag pair and spawns nothing; this is the
/// caller it was left for, so the two halves of that pair are asserted where they
/// actually run. `--allow-empty` is what makes a fold a commit at all — a fold
/// changes no file, which is the whole of what it is — and the *absence* of
/// `--amend` is the load-bearing half: on a reused branch the commit before this
/// one may belong to a previous run and already be pushed.
///
/// The body is read through [`FixedInCommits`] for the same reason the clean
/// group's is: a fold that left a commit naming no advisory would be invisible to
/// the next run's scan and the group would be re-derived from scratch.
#[tokio::test]
async fn a_fold_is_an_empty_commit_naming_every_id_and_amending_nothing() {
    let world = landing_world(&LANDED);
    let before = world.tree.staged_paths();

    record_fold(&world.tree, &world.group)
        .await
        .expect("a fold is recorded");

    assert!(
        world.tree.staged_paths().is_empty(),
        "a fold changes no file, and this commit carries {:?}",
        world.tree.staged_paths()
    );
    assert_ne!(
        world.tree.staged_paths(),
        before,
        "the premise: the commit before this one was not itself empty, so \
         `empty` above is a fact about the fold"
    );

    let fixed = FixedInCommits::read(&world.tree.head_commit_body());
    for cve in LANDED {
        assert!(
            fixed.names(cve),
            "a fold that named no advisory is invisible to the next run: {}",
            world.tree.head_commit_body()
        );
    }

    let calls = world.tree.git_calls();
    assert!(
        calls.iter().any(|call| call.contains("--allow-empty")),
        "a fold changes nothing, so it needs the flag to become a commit: {calls:?}"
    );
    nothing_rewrites_history(&calls);
    nothing_is_staged_by_directory(&calls);
}

/// **History is never rewritten, on any path a landing can take.**
///
/// The criterion asserted over every call site rather than over the one a
/// convenient lane happened to exercise: a clean landing, a refused one, and a
/// fold. The recorded list is non-empty in each case — [`nothing_rewrites_history`]
/// insists on it — because the fixture records nothing and the subject records
/// everything, so an absence here is an absence from what actually ran.
#[tokio::test]
async fn history_is_never_rewritten() {
    let (committed, _) = run_group_clean(&LANDED).await;
    let (reverted, _) = run_group_needs_work(&[NOT_LANDED]).await;
    let folded = landing_world(&LANDED);
    record_fold(&folded.tree, &folded.group)
        .await
        .expect("a fold is recorded");

    for (name, tree) in [
        ("clean", &committed.tree),
        ("needs-work", &reverted.tree),
        ("fold", &folded.tree),
    ] {
        let calls = tree.git_calls();
        assert!(
            !calls.is_empty(),
            "the `{name}` landing recorded nothing, so it proves nothing"
        );
        nothing_rewrites_history(&calls);
        nothing_is_staged_by_directory(&calls);
    }
}

/// **The production seam lands through the workspace, and adds no spawn site.**
///
/// Every lane above drives [`GoWorkspace`] as the [`Git`] port, which is what
/// makes the recorded call list readable — and it is also what would let the
/// three criteria hold of a double while the product did something else. This is
/// the other side: a real detached worktree, a real
/// [`Workspace`](fiddle_runtime::workspace::Workspace), and [`InWorktree`]
/// composing [`Workspace::run`] over it.
///
/// That composition is the whole of what [`InWorktree`] is. `Workspace::run` owns
/// the four-name environment a child of an attempt sees and the relativisation
/// applied to what it printed; a `git` spawned beside it would be a second
/// environment nobody had argued for, and
/// `workspace::a_workspace_command_inherits_no_credential` would stop being a
/// statement about how this crate's git children run.
///
/// [`Workspace::run`]: fiddle_runtime::workspace::Workspace
#[tokio::test]
async fn the_production_seam_lands_a_group_in_a_real_worktree() {
    let world = landing_world(&LANDED);
    let attempt = landing_worktree(&world);
    let root = attempt.workspace.root();

    let landed = land(
        &InWorktree::new(&attempt.workspace, Duration::from_secs(60)),
        &world.group,
        &GroupStatus::Clean,
        &attempt.changed,
    )
    .await
    .expect("a clean group lands in a real worktree");

    assert_eq!(landed, Landed::Committed);
    assert_eq!(
        ask_git(
            root,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        )
        .lines()
        .collect::<Vec<_>>(),
        ["go.mod", "go.sum"],
        "the commit the product made carries the group's own files"
    );

    let body = ask_git(root, &["log", "-1", "--format=%B"]);
    let fixed = FixedInCommits::read(&body);
    for cve in LANDED {
        assert!(
            fixed.names(cve),
            "the product's own body must name {cve}: {body}"
        );
    }
    assert!(
        ask_git(root, &["status", "--porcelain"]).is_empty(),
        "and the worktree is clean, so nothing was left staged or unstaged"
    );
}
