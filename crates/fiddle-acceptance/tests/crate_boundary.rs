use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

const FORBIDDEN: &[&str] = &["tokio", "rig-core", "rig-agent", "reqwest", "hyper", "mio"];

const THE_LIBRARY_UNDER_TEST: &[&str] = &["fiddle-core", "fiddle-runtime"];

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

#[test]
fn the_denylist_names_every_agent_crate() {
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
