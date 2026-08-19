#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Stdio;

pub const BROKEN: &str = "pub fn last_index(len: usize) -> usize { len }\n";

pub const REPAIRED: &str = "pub fn last_index(len: usize) -> usize { len - 1 }\n";

pub fn trivial_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("fixture");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "target/\n").unwrap();
    commit(&repo, "fixture");
    repo
}

pub fn broken_crate(dir: &Path) -> PathBuf {
    let repo = dir.join("fixture");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join("tests")).unwrap();
    std::fs::write(
        repo.join("Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n\
         [dependencies]\n",
    )
    .unwrap();
    std::fs::write(repo.join("src/lib.rs"), BROKEN).unwrap();
    std::fs::write(
        repo.join("tests/repair.rs"),
        "#[test]\nfn the_last_index_is_one_before_the_length() {\n    \
         assert_eq!(fixture::last_index(3), 2);\n}\n",
    )
    .unwrap();
    std::fs::write(repo.join(".gitignore"), "target/\nCargo.lock\n").unwrap();
    commit(&repo, "the broken fixture");
    repo
}

pub struct CheckResult {
    pub code: Option<i32>,
    pub output: String,
}

impl CheckResult {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

pub fn check(repo: &Path) -> CheckResult {
    let home = repo.with_extension("check-home");
    std::fs::create_dir_all(&home).unwrap();
    let mut command = std::process::Command::new("cargo");
    command
        .args(["test", "--offline"])
        .current_dir(repo)
        .env_clear()
        .env("HOME", &home)
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string()),
        )
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Ok(rustup_home) = std::env::var("RUSTUP_HOME") {
        command.env("RUSTUP_HOME", rustup_home);
    }
    let output = command
        .output()
        .unwrap_or_else(|source| panic!("could not run cargo in {}: {source}", repo.display()));
    CheckResult {
        code: output.status.code(),
        output: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

fn commit(repo: &Path, message: &str) {
    git(repo, &["init", "-q", "."]);
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            message,
        ],
    );
}

pub fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|source| panic!("could not run git {args:?}: {source}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn changed_files(dir: &Path) -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()
        .unwrap_or_else(|source| panic!("could not run git status: {source}"));
    assert!(output.status.success(), "git status failed in {dir:?}");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line[3..].trim().to_string())
        .collect()
}
