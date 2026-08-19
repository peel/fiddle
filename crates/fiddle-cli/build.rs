use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for path in revision_inputs() {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let revision = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=FIDDLE_SOURCE_REVISION={revision}");
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn revision_inputs() -> Vec<PathBuf> {
    let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) else {
        return Vec::new();
    };
    let mut watched = vec![PathBuf::from(head)];

    if let Some(branch) = git(&["symbolic-ref", "--quiet", "HEAD"]) {
        if let Some(loose) = git(&["rev-parse", "--git-path", &branch]) {
            watched.push(PathBuf::from(loose));
        }
    }
    if let Some(packed) = git(&["rev-parse", "--git-path", "packed-refs"]) {
        watched.push(PathBuf::from(packed));
    }
    watched
}
