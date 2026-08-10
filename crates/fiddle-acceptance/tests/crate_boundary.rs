use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

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

#[test]
fn fiddle_core_has_no_runtime_or_io_dependencies_anywhere_in_its_closure() {
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
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let closure = resolved_closure(&meta, "fiddle-core");
    for banned in ["tokio", "rig-core", "reqwest", "hyper", "mio"] {
        assert!(
            !closure.contains(banned),
            "fiddle-core's resolved closure must not contain {banned}; closure = {closure:?}"
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
