use std::path::{Path, PathBuf};

const WORKFLOW: &str = ".github/workflows/release.yml";
const TARGET: &str = "x86_64-unknown-linux-gnu";
const BINARY_ASSET: &str = "fiddle-linux-amd64";
const CHECKSUM_ASSET: &str = "fiddle-linux-amd64.sha256";
const DIST: &str = "dist/";
const RELEASE_COMMAND: &str = "gh release create";

#[test]
fn the_repository_carries_a_release_workflow() {
    let path = workflow();
    assert!(
        path.is_file(),
        "{} does not exist, so a tag publishes nothing and the host has no \
         binary to download",
        path.display()
    );
}

#[test]
fn the_release_triggers_on_a_version_tag() {
    let block = trigger_block(&read(&workflow()));
    assert!(
        block.contains("tags:"),
        "the trigger block of {WORKFLOW} names no tag filter, so it does not \
         fire on a version tag. block = {block:?}"
    );
    assert!(
        block.contains("'v*'"),
        "the tag filter of {WORKFLOW} does not match a version tag `v*`. \
         block = {block:?}"
    );
    for other in ["branches", "schedule", "pull_request", "workflow_dispatch"] {
        assert!(
            !block.contains(other),
            "{WORKFLOW} also triggers on `{other}`; a release must come from a \
             version tag alone, because any other trigger republishes the \
             assets under a name the host already pinned. block = {block:?}"
        );
    }
}

#[test]
fn the_release_builds_the_linux_amd64_target_on_a_linux_runner() {
    let src = read(&workflow());
    assert!(
        src.contains("runs-on: ubuntu-latest"),
        "{WORKFLOW} does not run on `ubuntu-latest`; the host runs that image, \
         so the release builds on it"
    );
    let installs_target = src
        .lines()
        .any(|line| line.contains("targets:") && line.contains(TARGET));
    assert!(
        installs_target,
        "{WORKFLOW} does not install the `{TARGET}` target, so the build has no \
         standard library for it"
    );
    assert!(
        src.contains(&format!("--target {TARGET}")),
        "{WORKFLOW} does not build the `{TARGET}` target; the host runs a \
         glibc image, so a different target gives it a binary it cannot run"
    );
}

#[test]
fn the_release_uploads_the_binary_and_its_checksum() {
    let src = read(&workflow());
    let assets = uploaded_assets(&src);
    assert!(
        assets.iter().any(|name| name == BINARY_ASSET),
        "`{RELEASE_COMMAND}` in {WORKFLOW} does not upload `{BINARY_ASSET}`, \
         so the host has no binary to download. assets = {assets:?}"
    );
    assert!(
        assets.iter().any(|name| name == CHECKSUM_ASSET),
        "`{RELEASE_COMMAND}` in {WORKFLOW} does not upload `{CHECKSUM_ASSET}`, \
         so the host cannot verify the binary before it runs it, and a re-cut \
         tag changes what a scheduled job runs. assets = {assets:?}"
    );
}

#[test]
fn the_asset_scan_reads_the_release_command_and_not_the_rest_of_the_workflow() {
    let computed_but_not_uploaded = "\
      - name: Compute the checksum\n\
      \x20       run: sha256sum dist/fiddle-linux-amd64 > dist/fiddle-linux-amd64.sha256\n\
      - name: Publish\n\
      \x20       run: gh release create \"$TAG\" dist/fiddle-linux-amd64\n";
    assert_eq!(
        uploaded_assets(computed_but_not_uploaded),
        vec![BINARY_ASSET],
        "a checksum the workflow writes but never uploads must not count as an \
         asset, or this suite passes whether or not the host can verify \
         anything"
    );

    let continued = "\
      run: |\n\
      \x20 gh release create \"$TAG\" \\\n\
      \x20   dist/fiddle-linux-amd64 \\\n\
      \x20   dist/fiddle-linux-amd64.sha256\n";
    assert_eq!(
        uploaded_assets(continued),
        vec![BINARY_ASSET, CHECKSUM_ASSET],
        "the command carries its arguments over continuation lines, so the \
         scan reads them all"
    );

    assert!(
        uploaded_assets("run: echo no release here\n").is_empty(),
        "a workflow with no release command uploads nothing"
    );
}

fn uploaded_assets(src: &str) -> Vec<String> {
    release_command(src)
        .split_whitespace()
        .filter_map(|token| token.strip_prefix(DIST))
        .map(str::to_owned)
        .collect()
}

fn release_command(src: &str) -> String {
    let mut command = String::new();
    let mut lines = src
        .lines()
        .skip_while(|line| !line.contains(RELEASE_COMMAND));
    for line in lines.by_ref() {
        let trimmed = line.trim();
        let continues = trimmed.ends_with('\\');
        command.push_str(trimmed.trim_end_matches('\\').trim_end());
        command.push(' ');
        if !continues {
            break;
        }
    }
    command
}

fn trigger_block(src: &str) -> String {
    src.lines()
        .skip_while(|line| line.trim_end() != "on:")
        .skip(1)
        .take_while(|line| line.starts_with(' ') || line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn workflow() -> PathBuf {
    repo_root().join(WORKFLOW)
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the manifest directory is two levels below the repository root")
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {} ({e})", path.display()))
}
