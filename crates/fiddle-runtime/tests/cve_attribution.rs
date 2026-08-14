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
//! Task 8 was split along a seam that runs through rule 2. **8.a — this suite —
//! is the read-only half**: the rules matched over `go list -m -json` and
//! `go mod why -m`, with nothing written to any tree. **8.b is rule 2's measured
//! viability probe**: bump the parent, `go mod tidy`, confirm with
//! `version::at_least`, and on failure restore `go.mod`/`go.sum` and fall to
//! rule 3.
//!
//! That seam has a consequence this file should be read knowing. There are two
//! ways to have "no such parent": the chain holds no direct requirement at all,
//! which is read-only and is asserted here; or the parent it holds cannot carry
//! the fix, which is only knowable by probing and is 8.b's. So **rule 3 is
//! exercised here through its read-only arm only**, and the plan's own rule 3
//! scenario — a parent a minor short of the fix — is not reachable from this
//! side of the seam. It cannot be faked either: what makes a parent non-viable
//! is that no published version carries the fix, and `tests/support/cve.rs` says
//! in so many words that this is not a fact a `go.mod` can hold.
//!
//! # Nothing here runs `go`
//!
//! [`ModuleGraph`] is a port, and `GoWorkspace` implements it by answering out
//! of the tree on disk in `go`'s own output formats. That is the same
//! arrangement the scanner is under — a port with a scripted stand-in — and it
//! is what keeps this suite offline and credential-free. What is under test is
//! therefore the reading of `go`'s output and the matching of the rules over it,
//! which is the whole of 8.a; the adapter that spawns a real `go` belongs with
//! 8.b, which has to spawn one anyway to run `go mod tidy`.

mod support;

use fiddle_core::PackageType::{Library, Os};
use fiddle_runtime::cve::attribute::{attribute, AttributionError, ModuleGraph, Rule, Target};
use support::cve::{
    direct, finding, go, indirect_via, indirect_without_a_direct_parent, module_not_needed, stdlib,
};

/// The module every fixture tree requires directly.
const DIRECT: &str = "golang.org/x/crypto";
/// The module every fixture tree gets at one remove.
const INDIRECT: &str = "golang.org/x/net";
/// The parent that carries it, where a tree has one.
const PARENT: &str = "gh.com/parent";

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
