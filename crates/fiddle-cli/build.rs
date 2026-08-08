use std::process::Command;

/// Capture the source revision the binary was built from and hand it to the
/// compiler as `FIDDLE_SOURCE_REVISION`. When git is unavailable — a source
/// tarball, a vendored build — the revision degrades to the literal `unknown`
/// rather than failing the build, because the version contract admits both.
fn main() {
    let rev = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=FIDDLE_SOURCE_REVISION={rev}");
    println!("cargo:rerun-if-changed=build.rs");
}
