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

const DIRECT: &str = "golang.org/x/crypto";
const INDIRECT: &str = "golang.org/x/net";
const PARENT: &str = "gh.com/parent";
const SECOND: &str = "golang.org/x/text";

#[tokio::test]
async fn a_direct_dependency_is_its_own_target() {
    let attributed = attribute(&finding(DIRECT, Library), &go(direct()))
        .await
        .expect("a direct requirement has a target");
    assert_eq!(attributed.target(), &Target::Module(DIRECT.to_string()));
    assert_eq!(attributed.rule(), Rule::One);
}

#[tokio::test]
async fn an_indirect_finding_with_a_viable_parent_targets_the_parent() {
    let attributed = attribute(&finding(INDIRECT, Library), &go(indirect_via(PARENT)))
        .await
        .expect("an indirect requirement with a parent has a target");
    assert_eq!(attributed.target(), &Target::Module(PARENT.to_string()));
    assert_eq!(attributed.rule(), Rule::Two);
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
    let attributed = attribute(
        &finding(INDIRECT, Library),
        &go(indirect_without_a_direct_parent()),
    )
    .await
    .expect("an indirect requirement with no parent has a target");
    assert_eq!(attributed.target(), &Target::Module(INDIRECT.to_string()));
    assert_eq!(attributed.rule(), Rule::Three);
}

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

#[tokio::test]
async fn an_os_finding_targets_the_dockerfile() {
    let workspace = go(direct());
    let attributed = attribute(&finding("libssl3", Os), &workspace)
        .await
        .expect("an OS finding has a target");
    assert_eq!(attributed.target(), &Target::DockerfileBaseImage);
    assert_eq!(attributed.rule(), Rule::Four);

    let as_a_library = attribute(&finding("libssl3", Library), &workspace).await;
    assert!(
        matches!(as_a_library, Err(AttributionError::NoTarget { .. })),
        "only the package type separates the Dockerfile from needs-work, but a \
         library finding for the same package answered {as_a_library:?}"
    );
}

#[tokio::test]
async fn stdlib_and_an_unneeded_module_are_needs_work_not_an_invented_target() {
    let worlds = [
        (go(stdlib()), "crypto/tls"),
        (go(module_not_needed()), "gh.com/never-required"),
    ];

    let mut refusals = Vec::new();
    for (workspace, package) in &worlds {
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

fn group_for<'a>(groups: &'a [Group], target: &Target) -> &'a Group {
    let targets = groups.iter().map(Group::target).collect::<Vec<_>>();
    groups
        .iter()
        .find(|group| group.target() == target)
        .unwrap_or_else(|| panic!("no group for {target:?}; there are groups for {targets:?}"))
}

fn ids(group: &Group) -> Vec<&str> {
    group.cves().iter().map(|cve| cve.as_str()).collect()
}

#[test]
fn two_scanner_packages_resolving_to_one_parent_are_one_group() {
    let groups = group(&[
        attributed("CVE-2026-1", INDIRECT, PARENT),
        attributed("CVE-2026-2", SECOND, PARENT),
    ]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].target(), &Target::Module(PARENT.to_string()));
    assert_eq!(groups[0].cves().len(), 2, "one bump, not two");
    assert_eq!(ids(&groups[0]), ["CVE-2026-1", "CVE-2026-2"]);
}

#[test]
fn one_package_resolving_to_two_targets_is_two_groups() {
    let groups = group(&[
        attributed("CVE-2026-1", INDIRECT, "gh.com/a"),
        attributed("CVE-2026-1b", INDIRECT, "gh.com/b"),
    ]);
    assert_eq!(groups.len(), 2);
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
    let version = select_target_version(
        &["0.54.3"],
        &available(&["0.54.3", "0.54.10", "0.54.9"]),
        "0.54.0",
    );
    assert_eq!(version.expect("a release carries the fix"), "0.54.10");
}

#[test]
fn a_release_in_a_higher_minor_is_not_taken_even_though_it_carries_the_fix() {
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
    let version = select_target_version(
        &["3.19.1"],
        &available(&["3.19.0", "3.19.2", "latest"]),
        "latest",
    );
    assert_eq!(version.expect("a pinned tag carries the fix"), "3.19.2");
}

#[test]
fn a_floating_tag_with_no_newer_pinned_tag_is_needs_work() {
    let version = select_target_version(&["3.19.1"], &available(&["3.19.0", "latest"]), "latest");
    assert!(
        matches!(version, Err(GroupError::NoRelease { .. })),
        "a tag that floats is not a tag that carries the fix: {version:?}"
    );
}

#[test]
fn a_fix_this_cannot_read_is_refused_rather_than_rounded_down() {
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
    let none: &[&str] = &[];
    let version = select_target_version(none, &available(&["0.54.3"]), "0.54.0");
    assert!(
        matches!(version, Err(GroupError::NoFixedVersion)),
        "{version:?}"
    );
}

#[test]
fn a_tree_already_at_the_fix_is_not_moved_backwards() {
    let version = select_target_version(&["0.54.3"], &available(&["0.54.3", "0.55.1"]), "0.55.1");
    assert!(
        matches!(version, Err(GroupError::AlreadyAtTheFix { .. })),
        "the fix is below the tree, so there is no move to make: {version:?}"
    );
}

#[test]
fn the_proxys_v_prefix_and_the_scanners_bare_version_are_one_line() {
    let version =
        select_target_version(&["0.54.3"], &available(&["v0.54.3", "v0.54.10"]), "v0.54.0");
    assert_eq!(
        version.expect("the proxy's releases carry the scanner's fix"),
        "v0.54.10",
        "handed back in the spelling the proxy printed, because that is what a \
         `go get` has to be written with"
    );
}
