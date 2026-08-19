use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const HOST_MODULE: &str = "example.com/host";

pub const GO_VERSION: &str = "1.23";

pub const INDIRECT_MODULE: &str = "golang.org/x/net";

pub const INDIRECT_VERSION: &str = "v0.24.0";

pub const INDIRECT_FIXED: &str = "v0.28.0";

pub const FIXTURE_PARENT: &str = "gh.com/parent";

pub const CARRIED_BY_THE_VIABLE_LINE: &str = "v0.33.0";

pub const REACHED_WITHOUT_THE_FIX: &str = "v0.31.0";

struct Release {
    module: &'static str,
    version: &'static str,
    requires: &'static [(&'static str, &'static str)],
}

const UPSTREAM: &[Release] = &[
    Release {
        module: FIXTURE_PARENT,
        version: "v1.2.0",
        requires: &[(INDIRECT_MODULE, INDIRECT_VERSION)],
    },
    Release {
        module: FIXTURE_PARENT,
        version: "v1.2.7",
        requires: &[(INDIRECT_MODULE, CARRIED_BY_THE_VIABLE_LINE)],
    },
    Release {
        module: FIXTURE_PARENT,
        version: "v1.9.9",
        requires: &[(INDIRECT_MODULE, INDIRECT_VERSION)],
    },
    Release {
        module: FIXTURE_PARENT,
        version: "v1.9.12",
        requires: &[(INDIRECT_MODULE, REACHED_WITHOUT_THE_FIX)],
    },
    Release {
        module: SWEEP_MODULE,
        version: SWEEP_VULNERABLE,
        requires: &[],
    },
    Release {
        module: SWEEP_MODULE,
        version: SWEEP_FIXED,
        requires: &[],
    },
    Release {
        module: INDIRECT_MODULE,
        version: INDIRECT_VERSION,
        requires: &[],
    },
    Release {
        module: INDIRECT_MODULE,
        version: INDIRECT_FIXED,
        requires: &[],
    },
];

pub const SWEEP_MODULE: &str = "golang.org/x/crypto";

pub const SWEEP_VULNERABLE: &str = "v0.31.0";

pub const SWEEP_FIXED: &str = "v0.35.0";

pub struct Answer {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl Answer {
    pub fn text(&self) -> String {
        match self.stdout.trim().is_empty() {
            true => self.stderr.clone(),
            false => self.stdout.clone(),
        }
    }
}

pub fn run(root: &Path, args: &[&str]) -> Answer {
    match args {
        ["list", "-m", "-versions", module] => versions(module),
        ["list", "-m", "-json", module] => list(root, module),
        ["mod", "why", "-m", module] => why(root, module),
        ["get", target] => get(root, target),
        ["mod", "tidy"] => tidy(root),
        other => panic!("the offline go has no `go {}`", other.join(" ")),
    }
}

fn versions(module: &str) -> Answer {
    let mut published: Vec<&str> = UPSTREAM
        .iter()
        .filter(|release| release.module == module)
        .map(|release| release.version)
        .collect();
    published.sort_by(|a, b| match newer(a, b) {
        true => std::cmp::Ordering::Greater,
        false => std::cmp::Ordering::Less,
    });
    let mut line = module.to_string();
    for version in published {
        line.push(' ');
        line.push_str(version);
    }
    Answer {
        stdout: format!("{line}\n"),
        stderr: String::new(),
        code: 0,
    }
}

fn list(root: &Path, module: &str) -> Answer {
    let record = match requirement(root, module) {
        Some((path, version, indirect)) => {
            let mut record = serde_json::json!({
                "Path": path,
                "Version": version,
                "GoVersion": GO_VERSION,
            });
            if indirect {
                record["Indirect"] = serde_json::Value::Bool(true);
            }
            record
        }
        None if module == HOST_MODULE => serde_json::json!({
            "Path": HOST_MODULE,
            "Main": true,
            "GoVersion": GO_VERSION,
        }),
        None => {
            return Answer {
                stdout: String::new(),
                stderr: format!("go: module {module}: not a known dependency\n"),
                code: 1,
            }
        }
    };
    Answer {
        stdout: format!(
            "{}\n",
            serde_json::to_string_pretty(&record).expect("a record serializes")
        ),
        stderr: String::new(),
        code: 0,
    }
}

fn why(root: &Path, module: &str) -> Answer {
    let mut answer = format!("# {module}\n");
    match requirement(root, module) {
        None => answer.push_str(&format!("(main module does not need module {module})\n")),
        Some((path, _, indirect)) => {
            answer.push_str(&format!("{HOST_MODULE}\n"));
            if indirect {
                if let Some((parent, _, _)) = requirements(root)
                    .into_iter()
                    .find(|(_, _, indirect)| !*indirect)
                {
                    answer.push_str(&format!("{parent}\n"));
                }
            }
            answer.push_str(&format!("{path}\n"));
        }
    }
    Answer {
        stdout: answer,
        stderr: String::new(),
        code: 0,
    }
}

fn get(root: &Path, target: &str) -> Answer {
    let Some((module, query)) = target.split_once('@') else {
        return refused(format!("go: invalid module version syntax {target:?}\n"));
    };
    let Some(resolved) = highest_matching(module, query) else {
        return refused(format!(
            "go: {module}@{query}: no matching versions for query {query:?}\n"
        ));
    };

    let mut selected = BTreeMap::new();
    selected.insert(module.to_string(), resolved.to_string());
    let was = requirement(root, module).map(|(_, version, _)| version);
    match write_selection(root, &selected) {
        true => Answer {
            stdout: String::new(),
            stderr: format!(
                "go: upgraded {module} {} => {resolved}\n",
                was.unwrap_or_default()
            ),
            code: 0,
        },
        false => Answer {
            stdout: String::new(),
            stderr: String::new(),
            code: 0,
        },
    }
}

fn tidy(root: &Path) -> Answer {
    let current = requirements(root);
    let mut selected: BTreeMap<String, String> = BTreeMap::new();
    for (path, version, indirect) in &current {
        if *indirect {
            continue;
        }
        for release in UPSTREAM {
            if release.module != path || release.version != version {
                continue;
            }
            for (needed, at) in release.requires {
                let entry = selected
                    .entry((*needed).to_string())
                    .or_insert_with(|| (*at).to_string());
                if newer(at, entry) {
                    *entry = (*at).to_string();
                }
            }
        }
    }
    for (path, version, _) in &current {
        if let Some(chosen) = selected.get_mut(path) {
            if newer(version, chosen) {
                *chosen = version.clone();
            }
        }
    }

    write_selection(root, &selected);
    Answer {
        stdout: String::new(),
        stderr: String::new(),
        code: 0,
    }
}

fn refused(message: String) -> Answer {
    Answer {
        stdout: String::new(),
        stderr: message,
        code: 1,
    }
}

pub fn requirements(root: &Path) -> Vec<(String, String, bool)> {
    read_go_mod(root)
        .lines()
        .filter_map(parse_requirement)
        .collect()
}

fn parse_requirement(line: &str) -> Option<(String, String, bool)> {
    let rest = line.trim().strip_prefix("require ")?;
    let (rest, indirect) = match rest.split_once("//") {
        Some((head, tail)) => (head.trim(), tail.trim() == "indirect"),
        None => (rest.trim(), false),
    };
    let (path, version) = rest.split_once(char::is_whitespace)?;
    Some((path.to_string(), version.trim().to_string(), indirect))
}

fn requirement(root: &Path, module: &str) -> Option<(String, String, bool)> {
    requirements(root)
        .into_iter()
        .find(|(path, _, _)| path == module)
}

fn write_selection(root: &Path, selected: &BTreeMap<String, String>) -> bool {
    let before = read_go_mod(root);
    let after: String = before
        .lines()
        .map(|line| match parse_requirement(line) {
            Some((path, _, indirect)) => match selected.get(&path) {
                Some(version) => format!(
                    "require {path} {version}{}",
                    match indirect {
                        true => " // indirect",
                        false => "",
                    }
                ),
                None => line.to_string(),
            },
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    let after = format!("{after}\n");

    let mut changed = false;
    if after != before {
        write(&go_mod_path(root), &after);
        changed = true;
    }

    let sum_path = root.join("go.sum");
    if let Some(sum) = sum_for(&requirements(root)) {
        if std::fs::read_to_string(&sum_path).ok().as_deref() != Some(sum.as_str()) {
            write(&sum_path, &sum);
            changed = true;
        }
    }
    changed
}

pub fn sum_for(requirements: &[(String, String, bool)]) -> Option<String> {
    if requirements.is_empty() {
        return None;
    }
    let digest = format!("h1:{}=", "A".repeat(43));
    let mut lines: Vec<String> = requirements
        .iter()
        .flat_map(|(module, version, _)| {
            [
                format!("{module} {version} {digest}"),
                format!("{module} {version}/go.mod {digest}"),
            ]
        })
        .collect();
    lines.sort();
    Some(format!("{}\n", lines.join("\n")))
}

fn go_mod_path(root: &Path) -> PathBuf {
    root.join("go.mod")
}

fn read_go_mod(root: &Path) -> String {
    std::fs::read_to_string(go_mod_path(root))
        .unwrap_or_else(|source| panic!("no go.mod in {}: {source}", root.display()))
}

fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents)
        .unwrap_or_else(|source| panic!("could not write {}: {source}", path.display()));
}

fn highest_matching(module: &str, query: &str) -> Option<&'static str> {
    UPSTREAM
        .iter()
        .filter(|release| release.module == module && matches_query(release.version, query))
        .map(|release| release.version)
        .reduce(|highest, candidate| match newer(candidate, highest) {
            true => candidate,
            false => highest,
        })
}

fn matches_query(version: &str, query: &str) -> bool {
    let (Some(version), Some(query)) = (components(version), components(query)) else {
        return false;
    };
    version.len() >= query.len() && version[..query.len()] == query[..]
}

fn newer(candidate: &str, incumbent: &str) -> bool {
    match (components(candidate), components(incumbent)) {
        (Some(mut candidate), Some(mut incumbent)) => {
            let width = candidate.len().max(incumbent.len());
            candidate.resize(width, 0);
            incumbent.resize(width, 0);
            candidate > incumbent
        }
        _ => false,
    }
}

fn components(version: &str) -> Option<Vec<u64>> {
    version
        .strip_prefix('v')
        .unwrap_or(version)
        .split('.')
        .map(|component| component.parse::<u64>().ok())
        .collect()
}
