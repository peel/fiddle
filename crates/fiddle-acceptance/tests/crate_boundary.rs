use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Crates that must never appear anywhere in `fiddle-core`'s resolved closure.
///
/// Naming the crates rather than the capabilities is deliberate: the boundary is
/// checked against the resolved graph, and the graph speaks package names.
const FORBIDDEN: &[&str] = &["tokio", "rig-core", "rig-agent", "reqwest", "hyper", "mio"];

/// The two library crates **this** crate must never be able to reach.
///
/// A different boundary from [`FORBIDDEN`] and worth keeping apart from it. That one
/// is about what a *pure domain* may depend on; this one is about what a *black-box
/// lane* may depend on, and the two lists would never acquire the same entries.
const THE_LIBRARY_UNDER_TEST: &[&str] = &["fiddle-core", "fiddle-runtime"];

/// The workspace's resolved dependency graph, with every feature on.
///
/// One reader rather than one per test, so two tests cannot come to make their claims
/// about two different graphs — and `--all-features`, because a boundary that held
/// only under the default feature set is a boundary a `--features` flag walks through.
fn cargo_metadata() -> serde_json::Value {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--all-features"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Every `*.rs` path under `root`, recursively.
fn walkdir_rs_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walkdir_rs_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
    files
}

/// Every package reachable from `root_name` in the resolved dependency graph.
///
/// This walks the transitive closure rather than the direct edges: a `tokio`
/// pulled in three levels down violates the boundary just as surely as one
/// written into `fiddle-core`'s own manifest.
fn resolved_closure(meta: &serde_json::Value, root_name: &str) -> HashSet<String> {
    let nodes = meta["resolve"]["nodes"].as_array().unwrap();
    let packages = meta["packages"].as_array().unwrap();
    let id_of = |name: &str| {
        packages
            .iter()
            .find(|p| p["name"] == name)
            .unwrap_or_else(|| panic!("no package {name}"))["id"]
            .clone()
    };
    let name_of = |id: &serde_json::Value| {
        packages
            .iter()
            .find(|p| &p["id"] == id)
            .unwrap_or_else(|| panic!("no package with id {id}"))["name"]
            .as_str()
            .unwrap()
            .to_string()
    };

    let mut seen = HashSet::new();
    let mut stack = vec![id_of(root_name)];
    while let Some(id) = stack.pop() {
        let node = nodes
            .iter()
            .find(|n| n["id"] == id)
            .unwrap_or_else(|| panic!("no resolve node for id {id}"));
        for dep in node["deps"].as_array().unwrap() {
            let dep_id = dep["pkg"].clone();
            let dep_name = name_of(&dep_id);
            if seen.insert(dep_name) {
                stack.push(dep_id);
            }
        }
    }
    seen
}

/// The denylist's *contents* are themselves under test, so weakening the
/// boundary by deleting an entry fails here rather than passing silently: the
/// closure walk below cannot tell "nothing forbidden is reachable" apart from
/// "nothing was forbidden".
#[test]
fn the_denylist_names_every_agent_crate() {
    // rig-core alone is not enough: rig 0.41 moved Agent/AgentBuilder/AgentRun
    // into rig-agent, so a denylist naming only rig-core would let the model
    // reach the pure domain through the crate that actually carries the agent
    // runtime.
    assert!(
        FORBIDDEN.contains(&"rig-core"),
        "denylist lost rig-core; FORBIDDEN = {FORBIDDEN:?}"
    );
    assert!(
        FORBIDDEN.contains(&"rig-agent"),
        "denylist lost rig-agent, which carries Agent/AgentBuilder/AgentRun in \
         rig 0.41; FORBIDDEN = {FORBIDDEN:?}"
    );
    assert!(
        FORBIDDEN.contains(&"tokio"),
        "denylist lost tokio; FORBIDDEN = {FORBIDDEN:?}"
    );
}

#[test]
fn fiddle_core_has_no_runtime_or_io_dependencies_anywhere_in_its_closure() {
    let closure = resolved_closure(&cargo_metadata(), "fiddle-core");
    for banned in FORBIDDEN {
        assert!(
            !closure.contains(*banned),
            "fiddle-core's resolved closure must not contain {banned}; closure = {closure:?}"
        );
    }
}

/// **This crate depends on neither library crate, and that is now a test rather than
/// a comment.**
///
/// # Why the rule exists, which is the part a comment could not enforce
///
/// The acceptance lane drives the compiled `fiddle` binary as a subprocess, *"so what
/// the tests observe is exactly what a caller at a shell would observe"*. Its value is
/// that it is a **second opinion**. The moment a test here calls the library it is
/// testing, it stops being one and becomes a **mirror**: an acceptance test that checks
/// the product's output with the product's own parser **passes on a wrong parser**,
/// because the test and the product then share the defect and neither can see it.
///
/// That is not hypothetical. `support::parse_marker` re-derives the marker grammar from
/// the design rather than calling `fiddle_core::parse_marker`, and
/// `support::expected_request_id` re-derives two identities from the design rather than
/// calling `fiddle_core::decision_request_id` — both for exactly this reason, and both
/// stating it at the function. `Cargo.toml` carries `blake3` as a **dev-dependency**
/// specifically so those derivations can be written from the specification.
///
/// The rule was stated in two places — that manifest and `support/mod.rs`'s header —
/// and enforced in neither. A rule nothing enforces is a rule the next lane will not
/// know it is breaking: a bean adding `fiddle-core` to reach one helper would turn every
/// test in this crate grey-box, and nothing would object.
///
/// # The whole resolved closure, and not the `[dev-dependencies]` table
///
/// Recorded because it is a real choice. The closure is the right thing on two counts.
/// [`resolved_closure`] walks a resolve node's `deps`, which carry **every** dependency
/// kind, so one walk catches a `[dependencies]` entry and a `[dev-dependencies]` entry
/// alike — and the harm is identical either way, because `#[cfg(test)]` code *is* what
/// this lane consists of. And an edge three levels down is as harmful as one written
/// here: if some future test dependency itself depended on `fiddle-core`, these tests
/// would link the library and could call into it, which reading this crate's own
/// manifest would never reveal.
///
/// # The three assertions, and what each one alone would miss
///
/// The denylist's *contents* are checked first, for
/// [`the_denylist_names_every_agent_crate`]'s reason: the walk below cannot tell
/// "neither library is reachable" from "no library was named". It is folded in here
/// rather than given its own test because two entries read by one caller do not earn a
/// second binary.
///
/// Then the **denominator**, and it carries more than "something was examined".
/// `blake3`, `regex` and `serde_json` are in this closure *only* as dev-dependencies,
/// so their presence is what proves this walk **sees dev-dependencies at all**. Without
/// it the guard could be inspecting a graph in which test-only edges never appear — and
/// it would then pass forever with a `fiddle-core` dev-dependency sitting in the
/// manifest, which is the precise failure it exists to prevent. They are also the three
/// that must stay permitted, and asserting them present is the stronger way to say so.
///
/// Only then the boundary itself.
#[test]
fn fiddle_acceptance_depends_on_neither_library_crate_anywhere_in_its_closure() {
    for required in ["fiddle-core", "fiddle-runtime"] {
        assert!(
            THE_LIBRARY_UNDER_TEST.contains(&required),
            "the denylist lost {required}, so the walk below would report a boundary it \
             never checked; THE_LIBRARY_UNDER_TEST = {THE_LIBRARY_UNDER_TEST:?}"
        );
    }

    let closure = resolved_closure(&cargo_metadata(), "fiddle-acceptance");

    for permitted in ["blake3", "regex", "serde_json"] {
        assert!(
            closure.contains(permitted),
            "{permitted} must be reachable and is not, so this walk is not seeing \
             dev-dependencies and the boundary below would pass vacuously; closure = \
             {closure:?}"
        );
    }

    for banned in THE_LIBRARY_UNDER_TEST {
        assert!(
            !closure.contains(*banned),
            "fiddle-acceptance's resolved closure must not contain {banned}: a \
             black-box lane that can call the library it is testing is a mirror, and a \
             wrong implementation passes. Re-derive from the design instead — see \
             `support::parse_marker` and `support::expected_request_id`, and \
             `Cargo.toml`'s reason for carrying blake3. closure = {closure:?}"
        );
    }
}

#[test]
fn fiddle_core_performs_no_process_or_filesystem_access() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("fiddle-core/src");
    let sources = walkdir_rs_files(&root);
    assert!(
        !sources.is_empty(),
        "found no sources under {}; the purity scan would pass vacuously",
        root.display()
    );

    let mut offenders = Vec::new();
    for entry in sources {
        let src = std::fs::read_to_string(&entry).unwrap();
        for banned in [
            "std::process",
            "std::fs",
            "std::net",
            "std::env",
            "SystemTime::now",
            "Instant::now",
        ] {
            if src.contains(banned) {
                offenders.push(format!("{}: {banned}", entry.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "fiddle-core must stay pure; found {offenders:?}"
    );
}
