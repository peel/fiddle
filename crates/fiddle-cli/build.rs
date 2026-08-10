use std::path::PathBuf;
use std::process::Command;

/// Capture the source revision the binary was built from and hand it to the
/// compiler as `FIDDLE_SOURCE_REVISION`. When git is unavailable — a source
/// tarball, a vendored build — the revision degrades to the literal `unknown`
/// rather than failing the build, because the version contract admits both.
///
/// Capturing it is only half the job. A build script runs once and its output
/// is cached until one of its declared inputs changes, so a script that
/// declares only itself is captured *once* and then reports whatever commit
/// happened to be checked out that day for the rest of the checkout's life.
/// That is not a cosmetic drift: the value ends up in a published evidence
/// bundle, where a stale-but-plausible sha attributes an attempt to a commit
/// that did not produce it — a fabricated provenance claim, and worse than the
/// honest `unknown` this script falls back to. So the script also declares the
/// files git rewrites when `HEAD` moves, and Cargo reruns it when they do.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for path in revision_inputs() {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    let revision = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=FIDDLE_SOURCE_REVISION={revision}");
}

/// Run `git` and hand back its trimmed stdout, or `None` if it is unavailable,
/// failed, or said nothing.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The files whose contents change when `HEAD` moves.
///
/// Three of them, because git has three ways of moving it:
///
/// - `HEAD` itself, rewritten by `git checkout` and holding the sha directly
///   when the head is detached;
/// - the loose ref `HEAD` names, rewritten by every `git commit` and `git
///   reset` on an attached head — the case that actually caused the staleness;
/// - `packed-refs`, which holds that ref instead after `git gc` or `git
///   pack-refs`.
///
/// All are resolved through `git rev-parse --git-path`, which is worktree-aware:
/// in a linked worktree `HEAD` is private to the worktree while the branch ref
/// lives in the common directory, and hardcoding `.git/...` would watch neither.
///
/// The loose ref and `packed-refs` are declared whether or not they exist right
/// now, because which of the two holds the branch changes over a checkout's
/// life. Cargo reruns this script on every build while a declared path is
/// missing, and that is the direction to err in: the rerun costs one
/// `git rev-parse`, and Cargo still skips recompiling when the sha comes back
/// unchanged, whereas a missed rerun costs a reader their trust in the bundle.
fn revision_inputs() -> Vec<PathBuf> {
    let Some(head) = git(&["rev-parse", "--git-path", "HEAD"]) else {
        // Not a git checkout at all; the revision is `unknown` and nothing about
        // this build can change that.
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
