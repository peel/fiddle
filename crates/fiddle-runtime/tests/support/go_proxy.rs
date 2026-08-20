use std::path::{Path, PathBuf};

pub const HOST_MODULE: &str = "example.com/host";

pub const GO_VERSION: &str = "1.23";

pub const INDIRECT_MODULE: &str = "golang.org/x/net";

pub const INDIRECT_VERSION: &str = "v0.24.0";

pub const INDIRECT_FIXED: &str = "v0.28.0";

pub const FIXTURE_PARENT: &str = "gh.com/parent";

struct Release {
    module: &'static str,
    version: &'static str,
}

const UPSTREAM: &[Release] = &[
    Release {
        module: SWEEP_MODULE,
        version: SWEEP_VULNERABLE,
    },
    Release {
        module: SWEEP_MODULE,
        version: SWEEP_FIXED,
    },
    Release {
        module: INDIRECT_MODULE,
        version: INDIRECT_VERSION,
    },
    Release {
        module: INDIRECT_MODULE,
        version: INDIRECT_FIXED,
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
