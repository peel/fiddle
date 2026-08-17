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

use fiddle_runtime::capability::{GroupMigration, MigrationAttempt};
use rig_core::test_utils::{MockCompletionModel, MockTurn};
use serde_json::json;
use support::cve::{
    migration_world, MigrationWorld, HOST_ROOT, MIGRATION_SOURCE as SOURCE, SENTINEL_PROSE,
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
        .migrate(&world.attempt(), &world.group)
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
    // told: the word the whole exception turns on.
    assert!(sent.json.contains("uniform"), "the scope rules reach it");

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
        .migrate(&world.attempt(), &world.group)
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
