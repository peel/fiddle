//! Which module a finding is actually fixed by editing.
//!
//! The subject is [`fiddle_runtime::cve::attribute`], and what it decides is the
//! *bump target*: nearly every vulnerable dependency in the repository this
//! milestone replaces arrives `// indirect`, so the module a scanner names is
//! usually not the module anybody edits. Get this wrong and the change is
//! either impossible to write — a bump of a module the main module does not
//! require — or it is written against the wrong module and the CVE survives the
//! pull request.
//!
//! # The four rules, and which half of them this suite covers
//!
//! The design states four rules, **matched top-down**:
//!
//! 1. a direct requirement is its own target;
//! 2. an indirect one goes to the first *direct* requirement in its
//!    `go mod why -m` chain, when that parent can carry the fix;
//! 3. an indirect one with no such parent is bumped itself, so minimal version
//!    selection raises it for every consumer;
//! 4. an `OS` finding goes to the `Dockerfile` base image tag.
//!
//! Task 8 was split along a seam that runs through rule 2. **8.a is the
//! read-only half**: the rules matched over `go list -m -json` and `go mod why
//! -m`, with nothing written to any tree. **8.b is rule 2's measured viability
//! probe**: bump the parent, `go mod tidy`, confirm with `version::at_least`,
//! and on failure restore `go.mod`/`go.sum` and fall to rule 3. Both halves are
//! now here.
//!
//! Rule 3 is therefore reached two ways, and both are asserted. The chain may
//! hold no direct requirement at all — an untidied `go.mod`, and a world a tree
//! can build on its own — or the parent it holds may turn out not to carry the
//! fix, which nothing but a probe can establish. The two are separate tests
//! because they are separate reasons, and a suite that only had the first would
//! be taking rule 2's second half on trust.
//!
//! # Nothing here needs a Go toolchain, and one lane spawns one anyway
//!
//! [`ModuleGraph`] is a port. Most lanes drive it in process: `GoWorkspace`
//! answers out of the tree on disk in `go`'s own output formats, and writes to
//! that tree when the probe asks it to. The module proxy those trees resolve
//! against — the half a `go.mod` cannot hold — is `tests/support/go_proxy.rs`.
//!
//! The last attribution section drives the **production** adapter,
//! `cve::go::Go`, against a scripted toolchain built from that same proxy. That
//! is the only way the offline gate can exercise the spawn, the three-name
//! environment, the reading of `go`'s two streams and the `go.mod`/`go.sum`
//! restore — none of which a stand-in for the adapter would touch. Nothing in
//! either case reaches a real `go`, a module proxy or a credential.
//!
//! # And what becomes of the targets: grouping
//!
//! [`fiddle_runtime::cve::group`] is the other half of this file, and it is here
//! rather than in a suite of its own because it asks the next question about the
//! same answer: attribution says *this finding is fixed by editing that*, and
//! grouping says *so these findings are one edit*. Its lanes need no tree, no
//! process and no port — grouping is a pure function of findings something else
//! already placed — which is why they are plain `#[test]` among the async ones.

mod support;

use fiddle_core::PackageType::{Library, Os};
use fiddle_runtime::cve::attribute::{attribute, AttributionError, ModuleGraph, Rule, Target};
use fiddle_runtime::cve::group::{group, select_target_version, Group, GroupError};
use std::path::Path;
use support::cve::{
    absent_go, attributed, attributed_fixed_at, attributed_os, available, direct, finding, go,
    indirect_via, indirect_via_parent_without_the_fix, indirect_without_a_direct_parent,
    module_not_needed, spawned_go, stdlib, CARRIED_BY_THE_VIABLE_LINE, REACHED_WITHOUT_THE_FIX,
};

/// The module every fixture tree requires directly.
const DIRECT: &str = "golang.org/x/crypto";
/// The module every fixture tree gets at one remove.
const INDIRECT: &str = "golang.org/x/net";
/// The parent that carries it, where a tree has one.
const PARENT: &str = "gh.com/parent";
/// A second scanner package, for the grouping lanes: *two packages, one parent*
/// is not a claim any single package name can carry.
const SECOND: &str = "golang.org/x/text";

// ---------------------------------------------------------------------------
// Rule 1
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_direct_dependency_is_its_own_target() {
    let attributed = attribute(&finding(DIRECT, Library), &go(direct()))
        .await
        .expect("a direct requirement has a target");
    assert_eq!(attributed.target(), &Target::Module(DIRECT.to_string()));
    // The rule and not only the target: three of the four rules end in
    // `Target::Module`, so a target alone does not say which reasoning produced
    // it — and the reasoning is what a report has to be able to state.
    assert_eq!(attributed.rule(), Rule::One);
}

// ---------------------------------------------------------------------------
// Rules 2 and 3, which are the pair that overlaps
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_indirect_finding_with_a_viable_parent_targets_the_parent() {
    let attributed = attribute(&finding(INDIRECT, Library), &go(indirect_via(PARENT)))
        .await
        .expect("an indirect requirement with a parent has a target");
    assert_eq!(attributed.target(), &Target::Module(PARENT.to_string()));
    assert_eq!(attributed.rule(), Rule::Two);
    // The chain is recorded, because the pull request has to be able to say why
    // it is editing a module the scanner never named. Without this the target
    // is an assertion nobody downstream can check.
    let resolved = attributed.resolved();
    for hop in [PARENT, INDIRECT] {
        assert!(
            resolved.contains(hop),
            "the resolved chain does not name {hop}:\n{resolved}"
        );
    }
}

#[tokio::test]
async fn an_indirect_finding_with_no_direct_parent_targets_the_module_itself() {
    // The read-only arm of rule 3: a tree whose `go.mod` marks the module
    // `// indirect` while requiring nothing else, so the chain runs straight
    // from the main module to it and holds no direct requirement to bump
    // instead. Real, and not contrived: an untidied `go.mod` is exactly this,
    // and the right target is still the named module — raising it there raises
    // it for every consumer through minimal version selection.
    let attributed = attribute(
        &finding(INDIRECT, Library),
        &go(indirect_without_a_direct_parent()),
    )
    .await
    .expect("an indirect requirement with no parent has a target");
    assert_eq!(attributed.target(), &Target::Module(INDIRECT.to_string()));
    assert_eq!(attributed.rule(), Rule::Three);
}

/// Rule 3's other arm: a chain that *does* offer a parent, where the parent
/// turns out not to carry the fix.
///
/// Viability is measured rather than guessed — bump the parent inside its own
/// minor, `go mod tidy`, and confirm the named module now resolves to at least
/// the finding's `fixedVersion`. This world is the one where the confirm says
/// no, so the probing edit has to come back off the tree and rule 3 has to
/// answer instead.
#[tokio::test]
async fn a_parent_that_does_not_carry_the_fix_falls_through_and_reverts() {
    let ws = go(indirect_via_parent_without_the_fix(PARENT));
    let attributed = attribute(&finding(INDIRECT, Library), &ws)
        .await
        .expect("a parent that cannot carry the fix still leaves rule 3 a target");
    assert_eq!(attributed.target(), &Target::Module(INDIRECT.to_string()));
    assert_eq!(attributed.rule(), Rule::Three);
    assert!(
        ws.is_clean(),
        "the probing edit is reverted, not left behind:\n{}",
        ws.go_mod()
    );
}

/// The probe writes, and only a probe that failed is unwritten.
///
/// `is_clean()` in the test above says nothing on its own: a probe that never
/// touched a tree leaves it just as clean as one that undid itself, so that
/// assertion is only evidence if something really dirtied the tree first. This is
/// where that is established, and it establishes it by *contrast* rather than by
/// inspecting the fixture. The two worlds run the same code over the same finding
/// and differ in one fact — what the parent's line reaches — and they end in
/// opposite states: the viable one keeps a `go.mod` naming a version that was not
/// in it before, the doomed one is back where it started while the transcript
/// still holds the version the confirm read.
///
/// Four separate changes are caught here, which is what makes it worth the
/// length. Never bumping fails the viable world's `!is_clean()`. Skipping the
/// tidy leaves the build list pre-bump, so the viable world drops to rule 3.
/// A confirm that always says yes gives the doomed world rule 2. A revert that
/// does nothing leaves the doomed world dirty.
#[tokio::test]
async fn the_probe_really_writes_and_only_a_failed_one_is_undone() {
    let viable = go(indirect_via(PARENT));
    let attributed = attribute(&finding(INDIRECT, Library), &viable)
        .await
        .expect("a parent that carries the fix has a target");
    assert_eq!(attributed.rule(), Rule::Two);
    assert!(
        !viable.is_clean(),
        "a probe that wrote nothing measured nothing, so rule 2 was guessed:\n{}",
        viable.go_mod()
    );
    assert!(
        viable.go_mod().contains(CARRIED_BY_THE_VIABLE_LINE),
        "the tree does not hold the version the bump resolved to:\n{}",
        viable.go_mod()
    );

    let doomed = go(indirect_via_parent_without_the_fix(PARENT));
    let attributed = attribute(&finding(INDIRECT, Library), &doomed)
        .await
        .expect("a parent that cannot carry the fix still leaves rule 3 a target");
    assert_eq!(attributed.rule(), Rule::Three);
    // The version only a bump could have produced, in the transcript — so the
    // probe demonstrably happened — and *not* in the tree, so it was undone.
    assert!(
        attributed.resolved().contains(REACHED_WITHOUT_THE_FIX),
        "the confirm never read a bumped tree:\n{}",
        attributed.resolved()
    );
    assert!(
        !doomed.go_mod().contains(REACHED_WITHOUT_THE_FIX),
        "the probing edit is still on the tree:\n{}",
        doomed.go_mod()
    );
    assert!(doomed.is_clean(), "and neither is anything else it wrote");
}

/// Every command the probe ran, in the order the design names them.
///
/// The transcript is what a pull request has to be able to state, and rule 2's
/// claim — *this parent carries the fix* — is only checkable if the three
/// commands that established it travel with it. Asserted as an ordered sequence
/// rather than as three `contains`, because bump-tidy-confirm is an *order*: a
/// confirm read before the tidy is the pre-bump build list, and a transcript that
/// listed the three in any arrangement would say nothing about which happened.
#[tokio::test]
async fn rule_two_records_the_bump_the_tidy_and_the_confirm_it_measured_with() {
    let attributed = attribute(&finding(INDIRECT, Library), &go(indirect_via(PARENT)))
        .await
        .expect("a target");
    let resolved = attributed.resolved();

    let mut at = 0;
    for command in [
        "$ go get gh.com/parent@v1.2",
        "$ go mod tidy",
        "$ go list -m -json golang.org/x/net",
    ] {
        let found = resolved[at..].find(command).unwrap_or_else(|| {
            panic!("the probe's transcript has no `{command}` after position {at}:\n{resolved}")
        });
        at += found + command.len();
    }
}

/// Rules 2 and 3 are the only pair that can both match, so this is where
/// "matched top-down" is a fact rather than a comment.
///
/// Both apply to an indirect requirement; rule 3's guard is satisfied by every
/// world rule 2's is. The two worlds below differ in **one** fact — whether the
/// `go mod why -m` chain holds a direct requirement — and the finding, the
/// module and the package type are identical across them. So a different rule
/// firing is attributable to that one difference and to nothing else, and a
/// matcher that consulted rule 3 first would answer `Rule::Three` for both.
#[tokio::test]
async fn rule_two_is_matched_before_rule_three() {
    let same_finding = finding(INDIRECT, Library);
    let with_a_parent = attribute(&same_finding, &go(indirect_via(PARENT)))
        .await
        .expect("a target");
    let without_a_parent = attribute(&same_finding, &go(indirect_without_a_direct_parent()))
        .await
        .expect("a target");

    assert_eq!(with_a_parent.rule(), Rule::Two);
    assert_eq!(without_a_parent.rule(), Rule::Three);
    assert_ne!(
        with_a_parent.target(),
        without_a_parent.target(),
        "the earlier rule has to move the target off the named module, or the \
         ordering is unobservable and rule 3 could be answering for both"
    );
}

// ---------------------------------------------------------------------------
// Rule 4
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_os_finding_targets_the_dockerfile() {
    let workspace = go(direct());
    let attributed = attribute(&finding("libssl3", Os), &workspace)
        .await
        .expect("an OS finding has a target");
    assert_eq!(attributed.target(), &Target::DockerfileBaseImage);
    assert_eq!(attributed.rule(), Rule::Four);

    // The same package name, in the same tree, as a library finding. Rule 4's
    // guard is the package type and nothing else, and without this half the
    // assertion above would also pass for a matcher that sent *everything* it
    // could not place in the module graph to the Dockerfile — which is the
    // invented target `m4-no-invented-target` exists to rule out, wearing rule
    // 4's clothes.
    let as_a_library = attribute(&finding("libssl3", Library), &workspace).await;
    assert!(
        matches!(as_a_library, Err(AttributionError::NoTarget { .. })),
        "only the package type separates the Dockerfile from needs-work, but a \
         library finding for the same package answered {as_a_library:?}"
    );
}

// ---------------------------------------------------------------------------
// No rule matched
// ---------------------------------------------------------------------------

/// Two worlds with no bump target, and they must reach it for two reasons.
///
/// A loop over cases that asserts one weak thing per case proves one fact
/// twice. So each case carries the module *its own* tree cannot place, and the
/// assertions are three: the refusal quotes the resolver's own bytes, verbatim
/// and not paraphrased; it is about the module that finding named; and the two
/// refusals are not the same text. A matcher answering with a constant string,
/// or with the last thing it happened to have resolved, fails the third.
#[tokio::test]
async fn stdlib_and_an_unneeded_module_are_needs_work_not_an_invented_target() {
    // A standard-library import path, which is in no module graph and is fixed
    // by moving the toolchain — the `toolchain` line the `stdlib` tree carries.
    // And a module path the `module_not_needed` tree genuinely does not require.
    let worlds = [
        (go(stdlib()), "crypto/tls"),
        (go(module_not_needed()), "gh.com/never-required"),
    ];

    let mut refusals = Vec::new();
    for (workspace, package) in &worlds {
        // What the resolver actually printed, read through the same port the
        // subject reads it through. Asserting against *this* rather than
        // against a phrase spelled in the test is what makes the refusal a
        // quotation: a matcher that composed its own sentence about the module
        // would satisfy a `contains("does not need")` and fail here.
        let said = workspace.why(package).await.expect("the resolver answered");
        match attribute(&finding(package, Library), workspace).await {
            Err(AttributionError::NoTarget { resolved_output }) => {
                assert!(!resolved_output.is_empty(), "quote what the resolver said");
                assert!(
                    resolved_output.contains(said.trim()),
                    "the refusal for {package} does not quote the resolver:\n\
                     resolver said:\n{said}\nrefusal carried:\n{resolved_output}"
                );
                refusals.push(resolved_output);
            }
            other => panic!("must not invent a target for {package}: {other:?}"),
        }
    }

    let [stdlib_refusal, unneeded_refusal] = <[String; 2]>::try_from(refusals).unwrap();
    assert!(
        stdlib_refusal.contains("crypto/tls") && !stdlib_refusal.contains("gh.com/never-required"),
        "the standard-library refusal is about the other world's module:\n{stdlib_refusal}"
    );
    assert!(
        unneeded_refusal.contains("gh.com/never-required")
            && !unneeded_refusal.contains("crypto/tls"),
        "the unneeded-module refusal is about the other world's module:\n{unneeded_refusal}"
    );
}

// ---------------------------------------------------------------------------
// The adapter that really spawns a `go`
// ---------------------------------------------------------------------------

/// The same two verdicts, through a real child process.
///
/// Everything above answers the port in process, which is fast and is the right
/// place to establish what the *rules* do. It leaves one thing untested and it is
/// the thing that ships: `cve::go::Go` builds a command, clears an environment,
/// spawns under `process::run_bounded`, reads two streams and puts two files
/// back. None of that is exercised by a stand-in for the adapter, and none of it
/// is exercised by the offline gate unless something drives the adapter itself.
///
/// So these lanes script only the toolchain — `tests/go_stub/go_stub.rs`, which
/// answers out of the same `go_proxy` the in-process stand-in does — and run the
/// production adapter over it. Both worlds are here rather than one, for the
/// reason the in-process pair are: an adapter that never wrote would pass a lane
/// that only checked the reverted world.
#[tokio::test]
async fn the_adapter_that_spawns_a_go_measures_both_verdicts_and_reverts_one() {
    let viable = go(indirect_via(PARENT));
    let attributed = attribute(&finding(INDIRECT, Library), &spawned_go(&viable))
        .await
        .expect("a spawned toolchain answers a viable parent");
    assert_eq!(attributed.target(), &Target::Module(PARENT.to_string()));
    assert_eq!(attributed.rule(), Rule::Two);
    assert!(
        !viable.is_clean(),
        "a child that changed nothing on disk cannot have measured a bump:\n{}",
        viable.go_mod()
    );

    let doomed = go(indirect_via_parent_without_the_fix(PARENT));
    let attributed = attribute(&finding(INDIRECT, Library), &spawned_go(&doomed))
        .await
        .expect("and a parent whose line ends short of the fix");
    assert_eq!(attributed.target(), &Target::Module(INDIRECT.to_string()));
    assert_eq!(attributed.rule(), Rule::Three);
    assert!(
        attributed.resolved().contains(REACHED_WITHOUT_THE_FIX),
        "the child never confirmed against a bumped tree:\n{}",
        attributed.resolved()
    );
    assert!(
        doomed.is_clean(),
        "the adapter left its probing edit behind:\n{}",
        doomed.go_mod()
    );
}

/// What a `go` child receives, asserted against what one actually got.
///
/// The adapter's environment is an allowlist, and an allowlist is only honest
/// against a record from a real child — a `Command` nobody spawned proves that a
/// builder was called and nothing more. Three names exactly, so a fourth cannot
/// arrive without this assertion changing, and `HOME` pointed outside the module
/// root so a toolchain's caches cannot land in the tree whose diff is the
/// evidence.
///
/// A rule 1 world on purpose: it runs no probe, so the last assertion says that
/// *reading* a module graph changes nothing at all. Without it a leak of writes
/// into the read path would show up only as a puzzling failure somewhere in
/// Task 15's commit.
#[tokio::test]
async fn a_go_child_gets_three_names_and_a_home_outside_the_tree() {
    let workspace = go(direct());
    let graph = spawned_go(&workspace);
    attribute(&finding(DIRECT, Library), &graph)
        .await
        .expect("a direct requirement has a target");

    assert_eq!(graph.child_env_names(), ["HOME", "LANG", "PATH"]);
    let home = graph.child_env()["HOME"].clone();
    assert_eq!(home, graph.home().display().to_string());
    assert!(
        !Path::new(&home).starts_with(workspace.path()),
        "{home} is inside the tree whose diff is the evidence"
    );
    assert!(
        workspace.is_clean(),
        "asking a module graph a question changed the tree:\n{}",
        workspace.go_mod()
    );
}

/// A toolchain that is not installed is a resolver failure, not a missing target.
///
/// The distinction [`AttributionError`] exists for, and the only lane that
/// reaches it. It matters in the direction that is easy to get wrong: a spawn
/// that failed produces no output, an adapter that handed that back as *`go` said
/// nothing* would leave the rules matched over an empty document, and the
/// finding would come out attributed to itself under rule 3 — a bump nobody
/// asked for, produced by a toolchain that never ran.
#[tokio::test]
async fn a_toolchain_that_is_not_installed_is_a_resolver_failure() {
    let workspace = go(indirect_via(PARENT));
    let outcome = attribute(&finding(INDIRECT, Library), &absent_go(&workspace)).await;
    match outcome {
        Err(AttributionError::Resolver(source)) => assert!(
            source.command.contains("go_stub-which-is-not-installed"),
            "the failure does not name the program it tried to run: {source}"
        ),
        other => panic!("a toolchain that cannot be run is not an answer: {other:?}"),
    }
    assert!(workspace.is_clean(), "and nothing was written on the way");
}

// ---------------------------------------------------------------------------
// Grouping: one bump per target
// ---------------------------------------------------------------------------

/// The group in `groups` whose target is `target`, or a failure naming what was
/// there instead.
///
/// Looked up rather than indexed, so a lane asserting *what the Dockerfile group
/// holds* says that, and does not quietly also assert which position grouping
/// happened to put it in. The one lane that is about position asserts it
/// directly, below.
fn group_for<'a>(groups: &'a [Group], target: &Target) -> &'a Group {
    let targets = groups.iter().map(Group::target).collect::<Vec<_>>();
    groups
        .iter()
        .find(|group| group.target() == target)
        .unwrap_or_else(|| panic!("no group for {target:?}; there are groups for {targets:?}"))
}

/// The advisory ids a group names, in the order it names them.
fn ids(group: &Group) -> Vec<&str> {
    group.cves().iter().map(|cve| cve.as_str()).collect()
}

#[test]
fn two_scanner_packages_resolving_to_one_parent_are_one_group() {
    // Two *different* packages, which is the world rule 2 produces all over the
    // repository this milestone replaces: a handful of vulnerable modules that
    // all arrive through one direct requirement. Bumping that requirement is one
    // edit, one branch and one pull request, and a grouping that keyed on the
    // package the scanner named would open as many as there are findings.
    let groups = group(&[
        attributed("CVE-2026-1", INDIRECT, PARENT),
        attributed("CVE-2026-2", SECOND, PARENT),
    ]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].target(), &Target::Module(PARENT.to_string()));
    assert_eq!(groups[0].cves().len(), 2, "one bump, not two");
    // The ids themselves, because a commit body lists them and `len() == 2` is
    // satisfied by a group that collected two of anything.
    assert_eq!(ids(&groups[0]), ["CVE-2026-1", "CVE-2026-2"]);
}

#[test]
fn one_package_resolving_to_two_targets_is_two_groups() {
    // The counterweight to the lane above, and it is what makes that one an
    // assertion at all: a grouping that returned a single group for everything
    // passes `len() == 1` there and fails here. One package, because the tree
    // decides the target and not the scanner — the same module reached through
    // two different direct requirements is two edits.
    let groups = group(&[
        attributed("CVE-2026-1", INDIRECT, "gh.com/a"),
        attributed("CVE-2026-1b", INDIRECT, "gh.com/b"),
    ]);
    assert_eq!(groups.len(), 2);
    // `CVE-2026-1B` upper-case, because `AdvisoryId` canonicalizes at its parse
    // boundary and the fixture spelled it `…1b`. Asserted in the canonical
    // spelling rather than routed around, since that is the spelling a commit
    // body will carry and the next run's log scan will look for.
    for (target, cve) in [("gh.com/a", "CVE-2026-1"), ("gh.com/b", "CVE-2026-1B")] {
        let group = group_for(&groups, &Target::Module(target.to_string()));
        assert_eq!(
            ids(group),
            [cve],
            "{target} carries the other target's finding"
        );
    }
}

#[test]
fn every_os_finding_lands_in_one_group_without_a_special_case() {
    // Three distribution packages, fixed by moving one base image tag. They
    // arrive from the scanner as three unrelated findings and there is no rule
    // anywhere that says "collect the OS ones" — attribution gave all three the
    // same target, and a map keyed on the target collects them because they are
    // equal, not because anything looked at what they are.
    let groups = group(&[
        attributed_os("CVE-2026-1", "libssl3"),
        attributed_os("CVE-2026-2", "zlib1g"),
        attributed_os("CVE-2026-3", "libxml2"),
    ]);
    assert_eq!(
        groups.len(),
        1,
        "keyed by target, so the Dockerfile collects them by construction"
    );
    assert_eq!(groups[0].target(), &Target::DockerfileBaseImage);
    assert_eq!(ids(&groups[0]), ["CVE-2026-1", "CVE-2026-2", "CVE-2026-3"]);
}

#[test]
fn the_dockerfile_group_collects_the_os_findings_and_only_those() {
    // The lane above cannot tell "keyed by target" from "one group for
    // everything", so the same three OS findings arrive here beside a module
    // one. The Dockerfile group has to hold exactly the three, and the module
    // bump has to survive as its own edit — an OS special case that swept
    // findings into the Dockerfile would take this one with it.
    let groups = group(&[
        attributed_os("CVE-2026-1", "libssl3"),
        attributed("CVE-2026-4", INDIRECT, PARENT),
        attributed_os("CVE-2026-2", "zlib1g"),
        attributed_os("CVE-2026-3", "libxml2"),
    ]);
    assert_eq!(groups.len(), 2);
    assert_eq!(
        ids(group_for(&groups, &Target::DockerfileBaseImage)),
        ["CVE-2026-1", "CVE-2026-2", "CVE-2026-3"]
    );
    assert_eq!(
        ids(group_for(&groups, &Target::Module(PARENT.to_string()))),
        ["CVE-2026-4"]
    );
}

#[test]
fn one_advisory_reaching_a_target_twice_is_named_once() {
    // One advisory against two packages that resolve to one parent — a single
    // upstream flaw vendored twice, which is ordinary. The group is still one
    // bump fixing one advisory, and the id belongs in the commit body once:
    // Task 13's fold asks whether every id in a group is absent from a rescan,
    // and an id listed twice is one that question has to answer twice.
    let groups = group(&[
        attributed("CVE-2026-1", INDIRECT, PARENT),
        attributed("CVE-2026-1", SECOND, PARENT),
    ]);
    assert_eq!(ids(&groups[0]), ["CVE-2026-1"]);
    assert_eq!(
        groups[0].findings().len(),
        2,
        "both findings are still in the group; it is the id list that dedupes"
    );
}

#[test]
fn the_order_groups_come_back_in_does_not_depend_on_the_order_they_arrived() {
    // A run walks the groups: one attempt, one commit and one branch each, in
    // the order they come back. A grouping over a hash map would shuffle that
    // between runs on one unchanged report, and the first thing anybody would
    // notice is two runs whose evidence cannot be compared.
    let one = group(&[
        attributed("CVE-2026-1", INDIRECT, "gh.com/b"),
        attributed("CVE-2026-2", SECOND, "gh.com/a"),
        attributed_os("CVE-2026-3", "libssl3"),
    ]);
    let other = group(&[
        attributed_os("CVE-2026-3", "libssl3"),
        attributed("CVE-2026-2", SECOND, "gh.com/a"),
        attributed("CVE-2026-1", INDIRECT, "gh.com/b"),
    ]);
    let targets = |groups: &[Group]| {
        groups
            .iter()
            .map(|g| g.target().clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(targets(&one), targets(&other));
    assert_eq!(
        targets(&one),
        [
            Target::Module("gh.com/a".to_string()),
            Target::Module("gh.com/b".to_string()),
            Target::DockerfileBaseImage,
        ],
        "ordered by the key, so the order is a property of the report and not of \
         the order attribution happened to finish in"
    );
}

// ---------------------------------------------------------------------------
// The version a group moves to
// ---------------------------------------------------------------------------

#[test]
fn a_group_takes_the_latest_patch_inside_the_highest_fixed_minor() {
    let version = select_target_version(
        &["0.54.0", "0.54.3"],
        &available(&["0.54.0", "0.54.3", "0.55.1"]),
        "0.54.0",
    );
    assert_eq!(
        version.expect("a release inside the fixed minor carries the fix"),
        "0.54.3",
        "latest patch in that minor, never crossing it"
    );
}

#[test]
fn the_latest_patch_inside_the_minor_is_numeric_and_not_lexical() {
    // The pair `cve::version` was written for, one component along: `0.54.10`
    // sorts *below* `0.54.3` as text and above it as a version. A selection that
    // sorted its candidates as strings would pick `0.54.3` here and report a
    // finding fixed by a release that does not contain the later patches.
    let version = select_target_version(
        &["0.54.3"],
        &available(&["0.54.3", "0.54.10", "0.54.9"]),
        "0.54.0",
    );
    assert_eq!(version.expect("a release carries the fix"), "0.54.10");
}

#[test]
fn a_release_in_a_higher_minor_is_not_taken_even_though_it_carries_the_fix() {
    // Nothing inside `0.54` reaches the fix and `0.55.1` is above it, so a
    // selection that merely took the highest release carrying the fix would
    // answer `0.55.1` — a minor crossed on this build's initiative, in a change
    // whose whole claim is that it is the smallest one that fixes the finding.
    // Refused instead, and the bound is named.
    let version = select_target_version(&["0.54.3"], &available(&["0.54.0", "0.55.1"]), "0.54.0");
    match version {
        Err(GroupError::NoRelease { minor, .. }) => assert_eq!(minor, "0.54"),
        other => panic!("the minor is a ceiling, not a preference: {other:?}"),
    }
}

#[test]
fn crossing_a_major_is_needs_work_with_the_span_named() {
    match select_target_version(&["2.0.0"], &available(&["1.9.9"]), "1.9.9") {
        Err(GroupError::MajorBump { from, to }) => {
            assert_eq!((from.as_str(), to.as_str()), ("1", "2"));
        }
        other => panic!("must not attempt it: {other:?}"),
    }

    // And refused *because it crosses a major*, not because there was nothing to
    // move to: here the release exists and is the one that carries the fix. A
    // selection that reached the refusal by finding its candidate list empty
    // would answer `Ok("2.0.0")` for this one and leave a bump nobody reviewed
    // across an API break.
    let reachable = select_target_version(&["2.0.0"], &available(&["1.9.9", "2.0.0"]), "1.9.9");
    match reachable {
        Err(error @ GroupError::MajorBump { .. }) => assert_eq!(
            error.to_string(),
            "requires a major version bump from 1 to 2",
            "the span reaches the person reading the verdict, not only the type"
        ),
        other => panic!("an available crossing is still a crossing: {other:?}"),
    }
}

#[test]
fn a_groups_move_is_bounded_by_the_highest_fix_among_its_findings() {
    // Two advisories against one module, fixed in two different minors. The
    // group is one bump and it has to clear both, so it is the *higher* fix that
    // says which minor bounds the move — and the two fixes are a minor apart on
    // purpose, because inside one minor the latest patch clears both fixes
    // whichever of them was consulted, and the lane would say nothing.
    let groups = group(&[
        attributed_fixed_at("CVE-2026-1", INDIRECT, INDIRECT, "0.54.0"),
        attributed_fixed_at("CVE-2026-2", INDIRECT, INDIRECT, "0.55.2"),
    ]);
    let fixed = groups[0].fixed_versions();
    assert_eq!(
        select_target_version(
            &fixed,
            &available(&["0.54.0", "0.54.9", "0.55.2"]),
            "0.24.0"
        )
        .expect("a release carries both fixes"),
        "0.55.2",
        "the lower fix in the group does not bound the move"
    );

    // And where the lower fix's minor is all that is published, the group has
    // nothing it may move to. A selection bounded by the lower fix would answer
    // `0.54.9` here and report both advisories fixed by a release that fixes one
    // of them.
    let short = select_target_version(&fixed, &available(&["0.54.0", "0.54.9"]), "0.24.0");
    match short {
        Err(GroupError::NoRelease { minor, fixed }) => {
            assert_eq!((minor.as_str(), fixed.as_str()), ("0.55", "0.55.2"))
        }
        other => panic!("the higher fix is the bound, so nothing carries it: {other:?}"),
    }
}

#[test]
fn a_floating_tag_moves_to_the_pinned_tag_that_carries_the_fix() {
    // The `Dockerfile` case, and the same function: a base image on a floating
    // tag names no version to compare, so there is no major to cross and the
    // bound that still applies is the minor the fix lands in. Nothing here
    // branches on the target — this is `select_target_version` with a tag list
    // where a module has a release list.
    let version = select_target_version(
        &["3.19.1"],
        &available(&["3.19.0", "3.19.2", "latest"]),
        "latest",
    );
    assert_eq!(version.expect("a pinned tag carries the fix"), "3.19.2");
}

#[test]
fn a_floating_tag_with_no_newer_pinned_tag_is_needs_work() {
    // The same world with the fixed tag not published, which is the one the
    // design calls needs-work. It is the counterweight to the lane above: a
    // selection that refused every floating tag would pass this one and fail
    // that one, and a selection that answered `latest` — a tag whose contents
    // nobody can pin — would pass neither.
    let version = select_target_version(&["3.19.1"], &available(&["3.19.0", "latest"]), "latest");
    assert!(
        matches!(version, Err(GroupError::NoRelease { .. })),
        "a tag that floats is not a tag that carries the fix: {version:?}"
    );
}

#[test]
fn a_fix_this_cannot_read_is_refused_rather_than_rounded_down() {
    // A release candidate among the fixes. Taking the highest of the versions it
    // *can* read would move the group to `0.54.0` and report the advisory fixed
    // in `0.54.3-rc1` as fixed by it — the silent wrong answer `cve::version`
    // exists to keep out, arriving here instead through a maximum taken over a
    // set with an incomparable member.
    let version = select_target_version(
        &["0.54.0", "0.54.3-rc1"],
        &available(&["0.54.0", "0.54.3"]),
        "0.53.0",
    );
    match version {
        Err(GroupError::Unreadable { version }) => assert_eq!(version, "0.54.3-rc1"),
        other => panic!("an unreadable fix is not a fix this can bound: {other:?}"),
    }
}

#[test]
fn a_finding_with_no_published_fix_names_no_version_to_move_to() {
    // Ordinary, and distinct from every other refusal here: the scanner named no
    // fix at all, so there is nothing to select from and nothing about the
    // available releases could change that.
    let none: &[&str] = &[];
    let version = select_target_version(none, &available(&["0.54.3"]), "0.54.0");
    assert!(
        matches!(version, Err(GroupError::NoFixedVersion)),
        "{version:?}"
    );
}

#[test]
fn a_tree_already_at_the_fix_is_not_moved_backwards() {
    // The fix is behind the tree, which happens when one group's findings were
    // cleared by another group's bump. Selecting the latest patch inside the
    // fixed minor would answer `0.54.3` and write a *downgrade* — a change that
    // reintroduces whatever `0.55` fixed, presented as a security fix.
    let version = select_target_version(&["0.54.3"], &available(&["0.54.3", "0.55.1"]), "0.55.1");
    assert!(
        matches!(version, Err(GroupError::AlreadyAtTheFix { .. })),
        "the fix is below the tree, so there is no move to make: {version:?}"
    );
}

#[test]
fn the_proxys_v_prefix_and_the_scanners_bare_version_are_one_line() {
    // The mixed-prefix pair, in this file's own terms: the scanner names the fix
    // as `0.54.3` and a module proxy lists releases as `v0.54.10`. They are one
    // line of one module, and the selection compares them as such.
    let version =
        select_target_version(&["0.54.3"], &available(&["v0.54.3", "v0.54.10"]), "v0.54.0");
    assert_eq!(
        version.expect("the proxy's releases carry the scanner's fix"),
        "v0.54.10",
        "handed back in the spelling the proxy printed, because that is what a \
         `go get` has to be written with"
    );
}
