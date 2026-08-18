//! The CVE fixture pair, and what makes a check result attributable to a fix.
//!
//! # Why this suite is here and not in `fiddle-runtime`
//!
//! Task 20's black-box lanes for `fiddle run cve` are in this file, at the
//! bottom, and they drive the compiled binary as a subprocess in the ordinary
//! way — see the section header there for the world they run in and for what the
//! first of them found. What Task 19 adds below drives no binary at all: it is a
//! *fixture integrity* suite, and it reads the two trees in `tests/fixtures/`
//! and compiles them.
//!
//! That is not a departure from this crate's idiom. `crate_boundary.rs` is the
//! precedent — it shells out to `cargo metadata` and reads `.rs` files as text,
//! because the property it asserts is a property of the repository rather than
//! of a running program. So is this one: the claim is *these two fixtures differ
//! only in the dependency under remediation, and both compile*, and a claim
//! about two directories is checked by reading two directories.
//!
//! # What the pair is for
//!
//! Task 20 runs the gate twice: once against `cve-vulnerable`, which must
//! produce exactly one pull request, and once against `cve-fixed`, which must
//! produce an evidenced no-change. Those two runs are only evidence about the
//! *mitigation* if the two inputs differ in the mitigation and in nothing else.
//! A pair that also differed in a source file, a Dockerfile line, or a module
//! name would let a difference in outcome be attributed to any of them, and the
//! lane would keep passing while proving something else.
//!
//! Hence three separate claims, each with its own lane and its own diagnosis:
//!
//! 1. the trees differ in exactly `go.mod` and `go.sum` ([`the_two_fixtures_differ_only_in_the_dependency_under_remediation`]);
//! 2. within `go.mod`, the difference is exactly the version of the module under
//!    remediation, and not the module path or the language version;
//! 3. the dependency is **load-bearing** — the two trees are two programs, not
//!    one program with two manifests.
//!
//! The third is the one the task's criterion names in its last sentence: *a
//! fixture value that only appears where its value cannot matter is not tested*.
//! If `main.go` imported the module and never called it, "both build" would be
//! the only thing the pair could prove, and it would prove it just as well with
//! the requirement deleted. So the lane below runs both binaries and asserts
//! their behaviour differs in the way the advisory describes.
//!
//! # The offline constraint, and why the fixtures do not vendor
//!
//! The gate is offline and credential-free. A `go build` reaches
//! `https://proxy.golang.org` by default, so the toolchain has to be given the
//! dependency some other way. The two candidates were vendoring and a
//! module proxy served from the filesystem; this suite uses the proxy, and the
//! reasons are worth writing down because the choice is not obvious:
//!
//! * **Vendoring puts `go.sum` where its value cannot matter.** Under
//!   `-mod=vendor` the toolchain does not consult `go.sum` at all. The file
//!   would still be *present* in both fixtures and would still *differ* between
//!   them, so claim (1) above would keep passing — over a file no build reads.
//!   That is precisely the shape the criterion's last sentence rules out.
//! * **Vendoring breaks claim (1) outright.** A vendored tree carries the
//!   dependency's source and a `vendor/modules.txt` naming its version, so the
//!   two fixtures would differ in `vendor/**` as well, and the assertion the
//!   task specifies — `["go.mod", "go.sum"]` — would be false as written.
//! * **A bump cannot be vendored offline.** Task 20's remediation moves the
//!   vulnerable tree to the fixed version. A vendor directory holds one version
//!   of one module; the *other* version has to come from somewhere, which is a
//!   proxy or a cache either way.
//!
//! So [`Registry`] below builds a module proxy in a temporary directory out of
//! [`REGISTRY`], the checked-in source of the module's two releases, and every
//! child runs with `GOPROXY` pointing at it over `file://` and at nothing else.
//! There is no `,direct` fallback, so there is no network path to fall back
//! *to*, and [`the_build_is_satisfied_by_the_checked_in_registry_and_not_by_a_warm_module_cache`]
//! is the lane that proves it rather than asserting it.

mod support;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use support::{
    accepted, body_of, check_stub_binary, completion, gh_stub_binary, git, git_says,
    go_stub_binary, toml_string, walkdir_files, wiz_stub_binary, Reply, Scenario, StubGateway,
    CREDENTIAL_VARS,
};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// The world on disk
// ---------------------------------------------------------------------------

/// The module the fixture pair is pinned to, and the two versions of it.
///
/// These three strings are not free choices. `crates/fiddle-runtime/tests/
/// support/document.rs` holds the scanner documents the whole milestone asserts
/// against, and the first library package in its table is this module at
/// [`VULNERABLE_VERSION`], carrying a `fixedVersion` of [`FIXED_VERSION`]. Task
/// 20 seeds one of those documents against these fixtures; if the fixture's
/// requirement and the document's package ever came apart, the scanner would be
/// reporting a finding about a module the tree does not have, every selection
/// would come back empty, and the lane would pass by finding nothing to do.
///
/// [`the_pair_is_pinned_to_the_module_and_versions_the_shared_scanner_document_names`]
/// is what keeps them together, and it is why these are constants here rather
/// than literals in a `go.mod` nobody cross-reads.
const MODULE: &str = "golang.org/x/crypto";
const VULNERABLE_VERSION: &str = "v0.31.0";
const FIXED_VERSION: &str = "v0.35.0";

/// The two fixture trees, by directory name under `tests/fixtures/`.
const VULNERABLE: &str = "cve-vulnerable";
const FIXED: &str = "cve-fixed";

/// Where the module's releases are kept, as source, under `tests/fixtures/`.
const REGISTRY: &str = "cve-registry";

/// The repository root, derived from this package's manifest directory.
///
/// Absolute, unlike `config_check.rs`'s relative fixture path, because the
/// children this suite spawns run with their working directory set to a fixture
/// tree — a relative path would resolve against the wrong place the moment it
/// crossed a `current_dir`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the manifest directory is two levels below the repository root")
}

/// A fixture tree under `tests/fixtures/`.
fn fixture(name: &str) -> PathBuf {
    repo_root().join("tests/fixtures").join(name)
}

// ---------------------------------------------------------------------------
// Comparing the two trees
// ---------------------------------------------------------------------------

/// The relative paths of the files a fixture tree contributes to the repository,
/// as `git` sees them, with their bytes.
///
/// Tracked files rather than a directory walk, and that is a deliberate
/// narrowing of what "the fixture" means: the pair Task 20 runs against is the
/// pair a clone gets, so an untracked `.DS_Store` or a scratch file somebody
/// left in a working copy is not part of it. A directory walk would make this
/// suite fail for reasons that never reach CI, and — worse — it would make the
/// failure look like a fixture defect.
///
/// The asymmetry this cannot see is a fixture file somebody forgot to add. That
/// file is missing from a clone too, so it is missing from the pair, and the
/// build lanes below are what notice.
fn tracked_files(tree: &Path) -> BTreeMap<String, Vec<u8>> {
    let out = Command::new("git")
        .args(["ls-files", "-z", "--"])
        .arg(tree)
        .current_dir(repo_root())
        .output()
        .expect("git is on PATH");
    assert!(
        out.status.success(),
        "git ls-files failed for {}: {}",
        tree.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let listing = String::from_utf8(out.stdout).expect("git prints paths as UTF-8 here");
    let prefix = tree
        .strip_prefix(repo_root())
        .expect("a fixture tree is inside the repository")
        .to_str()
        .expect("the fixture path is UTF-8");

    let mut files = BTreeMap::new();
    for path in listing.split('\0').filter(|entry| !entry.is_empty()) {
        let relative = path
            .strip_prefix(prefix)
            .and_then(|rest| rest.strip_prefix('/'))
            .unwrap_or_else(|| panic!("git listed {path}, which is not under {prefix}"));
        let bytes = std::fs::read(repo_root().join(path))
            .unwrap_or_else(|source| panic!("git tracks {path} but it does not read: {source}"));
        files.insert(relative.to_string(), bytes);
    }
    assert!(
        !files.is_empty(),
        "{} tracks no files: the fixture is absent, or it was never committed",
        tree.display()
    );
    files
}

/// Every relative path at which two trees disagree.
///
/// A path counts as changed when the bytes differ **or** when only one tree has
/// it. Collapsing those two cases is what makes the answer usable as "the pair
/// differs here and nowhere else": a third file added to one side only is a
/// difference between the trees in exactly the sense that matters, and a
/// comparison that only looked at paths both sides hold would not see it.
fn changed_paths(a: &BTreeMap<String, Vec<u8>>, b: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    let mut changed: Vec<String> = a
        .keys()
        .chain(b.keys())
        .filter(|path| a.get(*path) != b.get(*path))
        .cloned()
        .collect();
    changed.sort();
    changed.dedup();
    changed
}

// ---------------------------------------------------------------------------
// The offline toolchain
// ---------------------------------------------------------------------------

/// A module proxy on the filesystem, and the temporary state a `go` needs.
///
/// One per lane that builds. The module cache is inside `root`, so no lane can
/// be satisfied by a module some other lane — or some other project on the
/// machine — left in `~/go/pkg/mod`.
struct Registry {
    root: TempDir,
}

impl Registry {
    /// Serve every release under `tests/fixtures/cve-registry/`.
    ///
    /// The layout is the module proxy protocol's: for each version, an `.info`,
    /// a `.mod`, and a `.zip`, under `<module>/@v/`. A `list` file goes beside
    /// them so that a query for the latest version — which is what a bump asks
    /// for — has something to resolve against.
    fn serve() -> Self {
        let registry = Self {
            root: TempDir::new().expect("a temporary directory"),
        };
        let source = fixture(REGISTRY).join(MODULE);
        let mut versions: Vec<String> = std::fs::read_dir(&source)
            .unwrap_or_else(|source_err| {
                panic!("no releases in {}: {source_err}", source.display())
            })
            .map(|entry| entry.expect("a readable directory entry").file_name())
            .map(|name| {
                name.into_string()
                    .expect("a release directory is named for its version, in UTF-8")
            })
            .collect();
        versions.sort();

        let at = registry.root.path().join("proxy").join(MODULE).join("@v");
        std::fs::create_dir_all(&at).expect("the temporary directory is writable");
        for version in &versions {
            let release = source.join(version);
            let files = files_under(&release);
            let go_mod = files
                .get("go.mod")
                .unwrap_or_else(|| panic!("{} has no go.mod", release.display()));

            std::fs::write(
                at.join(format!("{version}.info")),
                // The timestamp is fixed rather than "now": the proxy's answers
                // are part of the fixture, and a field that changed per run
                // would make two runs two different worlds.
                format!(r#"{{"Version":"{version}","Time":"2026-01-01T00:00:00Z"}}"#),
            )
            .expect("the temporary directory is writable");
            std::fs::write(at.join(format!("{version}.mod")), go_mod)
                .expect("the temporary directory is writable");
            std::fs::write(
                at.join(format!("{version}.zip")),
                module_zip(MODULE, version, &files),
            )
            .expect("the temporary directory is writable");
        }
        std::fs::write(
            at.join("list"),
            versions
                .iter()
                .map(|version| format!("{version}\n"))
                .collect::<String>(),
        )
        .expect("the temporary directory is writable");

        registry
    }

    /// A `go` command in `dir`, with an environment that has nowhere to reach.
    ///
    /// The environment is cleared and rebuilt rather than inherited, and every
    /// variable set here earns its place:
    ///
    /// * `GOPROXY` is the `file://` registry **and nothing else**. The default
    ///   is `https://proxy.golang.org,direct`; leaving even the `,direct` on the
    ///   end would give a failed lookup a network to fall through to.
    /// * `GOFLAGS=-mod=readonly` is what stops a build from *repairing* the
    ///   fixture. Under `-mod=mod` a `go build` rewrites `go.mod` and `go.sum`
    ///   when they are incomplete, which would turn "the fixture's `go.sum` is
    ///   correct" into "the fixture's `go.sum` was corrected", silently, in a
    ///   tracked file.
    /// * `GOSUMDB=off` because the checksum database is a network service, and
    ///   the module this registry serves is not in it.
    /// * `GOTOOLCHAIN=local` because a `go` directive naming a newer release
    ///   than the installed one makes the toolchain *download a toolchain*.
    /// * `GOENV=off` and `GOWORK=off` because both are files outside this
    ///   repository that can put a `GOPROXY` or an extra module back.
    /// * `GOMODCACHE`, `GOCACHE`, `GOPATH` and `GOTMPDIR` are inside `root`, so
    ///   the lane leaves nothing behind and inherits nothing.
    /// * `CGO_ENABLED=0` so the build needs no C toolchain, and `HOME` because
    ///   `go` refuses to start without one.
    fn go<I, S>(&self, dir: &Path, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.go_with_proxy(dir, &self.proxy_url(), args)
    }

    /// The same, with `GOPROXY` overridden — the one seam
    /// [`the_build_is_satisfied_by_the_checked_in_registry_and_not_by_a_warm_module_cache`]
    /// needs, and deliberately the only one.
    fn go_with_proxy<I, S>(&self, dir: &Path, proxy: &str, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let scratch = self.root.path();
        Command::new(go_binary())
            .args(args)
            .current_dir(dir)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", scratch)
            .env("GOPROXY", proxy)
            .env("GOFLAGS", "-mod=readonly")
            .env("GOSUMDB", "off")
            .env("GOTOOLCHAIN", "local")
            .env("GOENV", "off")
            .env("GOWORK", "off")
            .env("CGO_ENABLED", "0")
            .env("GOMODCACHE", scratch.join("modcache"))
            .env("GOCACHE", scratch.join("buildcache"))
            .env("GOPATH", scratch.join("gopath"))
            .env("GOTMPDIR", scratch)
            .output()
            .expect("go runs")
    }

    fn proxy_url(&self) -> String {
        format!("file://{}", self.root.path().join("proxy").display())
    }

    /// A path inside the registry's scratch space, for a build's output.
    fn scratch(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }
}

impl Drop for Registry {
    /// Make the module cache deletable again before [`TempDir`] tries.
    ///
    /// `go` marks everything it extracts read-only, including the directories,
    /// and a read-only directory cannot have its contents removed. `TempDir`
    /// discards the error, so the failure is invisible and the residue is
    /// permanent: a few hundred kilobytes per test run, forever, in the system
    /// temporary directory. `go clean -modcache` is the toolchain's own answer
    /// to that, and it runs before the fields drop.
    fn drop(&mut self) {
        let cache = self.root.path().join("modcache");
        if cache.exists() {
            let _ = self.go(self.root.path(), ["clean", "-modcache"]);
        }
    }
}

/// The `go` this suite drives.
///
/// A hard failure rather than a skipped lane when it is missing. The toolchain
/// is in this project's development shell for these fixtures specifically, and a
/// suite that quietly passed without it would be claiming the pair builds on the
/// evidence that nothing tried.
fn go_binary() -> PathBuf {
    which("go").unwrap_or_else(|| {
        panic!(
            "no `go` on PATH: this suite compiles the CVE fixtures, and the toolchain \
             is in the development shell — run under `nix develop -c`"
        )
    })
}

fn which(program: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    })
}

/// Every file under `root`, keyed by its slash-separated relative path.
fn files_under(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|source| panic!("{} does not read: {source}", dir.display()))
        {
            let entry = entry.expect("a readable directory entry");
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(root)
                .expect("the walk started at root")
                .to_str()
                .expect("fixture paths are UTF-8")
                .replace('\\', "/");
            files.insert(relative, std::fs::read(&path).expect("a readable file"));
        }
    }
    files
}

// ---------------------------------------------------------------------------
// The module zip
// ---------------------------------------------------------------------------

/// A module zip, as the proxy protocol defines one: every file under
/// `<module>@<version>/`.
///
/// Written here rather than pulled in as a dependency. The whole point of the
/// fixture pair is that the gate needs no network, and adding a crate to a lane
/// that exists to prove self-sufficiency is a poor trade for sixty lines of a
/// format that has not changed since 1989. Entries are **stored** — no
/// compression — which is why there is no deflate implementation below.
///
/// The bytes are deterministic: entries in sorted order, a fixed timestamp, no
/// extra fields. That matters less than it looks, because the hash `go.sum`
/// records is over the *contents and names* rather than over the archive, so a
/// differently-framed zip of the same files still satisfies the same `go.sum` —
/// but a proxy that answered differently on each call would be a fixture that
/// changed under its own tests.
fn module_zip(module: &str, version: &str, files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    // 1980-01-01, the earliest a DOS timestamp can express.
    const DOS_TIME: u16 = 0;
    const DOS_DATE: u16 = 0x0021;

    let mut body: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    let mut count = 0u16;

    for (relative, bytes) in files {
        let name = format!("{module}@{version}/{relative}");
        let offset = body.len() as u32;
        let crc = crc32(bytes);
        let size = bytes.len() as u32;

        // Local file header.
        body.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        body.extend_from_slice(&20u16.to_le_bytes()); // version needed
        body.extend_from_slice(&0u16.to_le_bytes()); // flags
        body.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        body.extend_from_slice(&DOS_TIME.to_le_bytes());
        body.extend_from_slice(&DOS_DATE.to_le_bytes());
        body.extend_from_slice(&crc.to_le_bytes());
        body.extend_from_slice(&size.to_le_bytes()); // compressed
        body.extend_from_slice(&size.to_le_bytes()); // uncompressed
        body.extend_from_slice(&(name.len() as u16).to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes()); // extra field length
        body.extend_from_slice(name.as_bytes());
        body.extend_from_slice(bytes);

        // Central directory header for the same entry.
        directory.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        directory.extend_from_slice(&20u16.to_le_bytes()); // version made by
        directory.extend_from_slice(&20u16.to_le_bytes()); // version needed
        directory.extend_from_slice(&0u16.to_le_bytes()); // flags
        directory.extend_from_slice(&0u16.to_le_bytes()); // method: stored
        directory.extend_from_slice(&DOS_TIME.to_le_bytes());
        directory.extend_from_slice(&DOS_DATE.to_le_bytes());
        directory.extend_from_slice(&crc.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&size.to_le_bytes());
        directory.extend_from_slice(&(name.len() as u16).to_le_bytes());
        directory.extend_from_slice(&0u16.to_le_bytes()); // extra field length
        directory.extend_from_slice(&0u16.to_le_bytes()); // comment length
        directory.extend_from_slice(&0u16.to_le_bytes()); // disk number
        directory.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
        directory.extend_from_slice(&0u32.to_le_bytes()); // external attributes
        directory.extend_from_slice(&offset.to_le_bytes());
        directory.extend_from_slice(name.as_bytes());

        count += 1;
    }

    let directory_offset = body.len() as u32;
    let directory_size = directory.len() as u32;
    body.extend_from_slice(&directory);
    body.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes()); // this disk
    body.extend_from_slice(&0u16.to_le_bytes()); // disk with the directory
    body.extend_from_slice(&count.to_le_bytes());
    body.extend_from_slice(&count.to_le_bytes());
    body.extend_from_slice(&directory_size.to_le_bytes());
    body.extend_from_slice(&directory_offset.to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes()); // comment length
    body
}

/// CRC-32, the IEEE polynomial a zip entry carries. Bitwise rather than
/// table-driven: the inputs here are a few hundred bytes.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

// ---------------------------------------------------------------------------
// Building a fixture
// ---------------------------------------------------------------------------

/// Compile `tree`, and hand back the binary it produced.
///
/// The build runs **in the fixture directory**, not in a copy of it, because the
/// thing under test is the tree the repository holds. `-mod=readonly` is what
/// makes that safe, and [`a_build_leaves_the_fixture_exactly_as_it_found_it`]
/// is what makes "safe" a checked claim rather than a reading of the manual.
fn build(registry: &Registry, tree: &Path, output: &str) -> Result<PathBuf, String> {
    let binary = registry.scratch(output);
    let out = registry.go(
        tree,
        [
            OsStr::new("build"),
            OsStr::new("-o"),
            binary.as_os_str(),
            OsStr::new("."),
        ],
    );
    if out.status.success() {
        Ok(binary)
    } else {
        Err(format!(
            "`go build` in {} exited {:?}\nstdout: {}\nstderr: {}",
            tree.display(),
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// What a built fixture prints.
fn run(binary: &Path) -> String {
    let out = Command::new(binary)
        .env_clear()
        .output()
        .unwrap_or_else(|source| panic!("{} does not run: {source}", binary.display()));
    assert!(
        out.status.success(),
        "{} exited {:?}: {}",
        binary.display(),
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("the fixture prints UTF-8")
}

// ---------------------------------------------------------------------------
// The lanes
// ---------------------------------------------------------------------------

/// The task's own assertion: the pair differs in the manifest pair and nowhere
/// else, and both halves compile.
#[test]
fn the_two_fixtures_differ_only_in_the_dependency_under_remediation() {
    let (a, b) = (fixture(VULNERABLE), fixture(FIXED));
    let changed = changed_paths(&tracked_files(&a), &tracked_files(&b));
    assert_eq!(
        changed,
        ["go.mod", "go.sum"],
        "a fixture pair that differs elsewhere cannot isolate the mitigation"
    );

    let registry = Registry::serve();
    // Both, and separately, because "the pair builds" is two claims and a
    // failure in either one has to name which half failed.
    if let Err(why) = build(&registry, &a, "vulnerable") {
        panic!("both must build, or a failing check proves nothing about the fix: {why}");
    }
    if let Err(why) = build(&registry, &b, "fixed") {
        panic!("both must build, or a passing check proves nothing about the fix: {why}");
    }
}

/// The paths are not enough: `go.mod` could differ in the module's own name, in
/// its language version, or in a second requirement, and claim (1) would not
/// notice. The difference has to be the *dependency under remediation*.
#[test]
fn the_only_difference_inside_go_mod_is_the_version_of_the_module_under_remediation() {
    let vulnerable = read_fixture_file(VULNERABLE, "go.mod");
    let fixed = read_fixture_file(FIXED, "go.mod");

    let differing: Vec<(&str, &str)> = vulnerable
        .lines()
        .zip(fixed.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(
        vulnerable.lines().count(),
        fixed.lines().count(),
        "the two manifests are the same file with one version changed, so they \
         have the same number of lines"
    );
    assert_eq!(
        differing,
        [(
            format!("require {MODULE} {VULNERABLE_VERSION}").as_str(),
            format!("require {MODULE} {FIXED_VERSION}").as_str()
        )],
        "exactly one line differs, and it is the requirement under remediation"
    );
}

/// The criterion's last sentence, made into a lane.
///
/// A `require` line whose module is imported but never *used* would satisfy
/// every assertion above: the trees would still differ in two files, and both
/// would still build. The pair would then be one program with two manifests, and
/// a difference in what the gate did with them could not be attributed to the
/// vulnerability, because nothing about the vulnerability would be present.
///
/// So: run both, and insist they disagree in the way the advisory says they
/// should. The vulnerable release buffers key-exchange traffic from a peer that
/// has not authenticated; the fixed release refuses it.
#[test]
fn the_dependency_is_load_bearing_so_the_pair_is_two_programs() {
    let registry = Registry::serve();
    let vulnerable = build(&registry, &fixture(VULNERABLE), "vulnerable").expect("it builds");
    let fixed = build(&registry, &fixture(FIXED), "fixed").expect("it builds");

    let (before, after) = (run(&vulnerable), run(&fixed));
    assert_ne!(
        before, after,
        "the two fixtures print the same thing, so the requirement they differ \
         in changes nothing the program does: the dependency is decoration and \
         the pair proves only that two manifests parse"
    );
    // Not just "different" — different in the direction the advisory describes,
    // so a pair that had been wired up backwards is a failure and not a pass.
    assert!(
        before.contains("buffering"),
        "the vulnerable fixture must exhibit the unbounded buffering the \
         advisory is about, and it printed: {before}"
    );
    assert!(
        after.contains("refused"),
        "the fixed fixture must exhibit the bound the fix introduced, and it \
         printed: {after}"
    );
}

/// The offline property, proved rather than asserted.
///
/// The build lanes above pass with `GOPROXY` pointing at the checked-in
/// registry. That on its own does not show the registry is what satisfied them:
/// a machine with `golang.org/x/crypto` already in `~/go/pkg/mod` would build
/// the same way with no proxy at all, and the suite would be green on a
/// developer's laptop and red in a clean container.
///
/// So take the proxy away and leave everything else — the same fresh module
/// cache, the same fixture — and require the build to fail for want of a module
/// lookup. Passing here means two things at once: nothing warm is being reached
/// for, and the dependency really does have to come from the registry.
#[test]
fn the_build_is_satisfied_by_the_checked_in_registry_and_not_by_a_warm_module_cache() {
    let registry = Registry::serve();
    let out = registry.go_with_proxy(
        &fixture(VULNERABLE),
        "off",
        ["build", "-o", "/dev/null", "."],
    );
    assert!(
        !out.status.success(),
        "a build with GOPROXY=off succeeded, so something outside this fixture \
         is supplying {MODULE}: the module cache is not the fresh one this \
         suite creates, and the offline lanes are proving nothing"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("GOPROXY=off"),
        "the build had to fail for want of a module lookup, and not for some \
         other reason: {stderr}"
    );
}

/// `-mod=readonly` keeps a build from repairing the tree it is checking.
///
/// Under the default the toolchain would edit `go.mod` and `go.sum` into
/// consistency, in a *tracked* file, and every lane above would then be
/// asserting about a fixture the test run had just written. The environment
/// [`Registry::go`] builds is what prevents it; this is the lane that shows it
/// prevented.
#[test]
fn a_build_leaves_the_fixture_exactly_as_it_found_it() {
    let tree = fixture(VULNERABLE);
    let before = tracked_files(&tree);
    let registry = Registry::serve();
    build(&registry, &tree, "vulnerable").expect("it builds");
    assert_eq!(
        tracked_files(&tree),
        before,
        "the build rewrote the fixture it was meant to be checking"
    );
}

/// The versions in the fixture pair and the versions in the shared scanner
/// document are one fact, and this is where it is kept honest.
///
/// `document.rs` is a test-support module of another crate, and this one cannot
/// link it — `crate_boundary.rs` asserts that `fiddle-acceptance` depends on
/// neither library crate, and a `#[path]` include would drag `serde_json`
/// fixtures across a boundary this repository checks. So the table is read as
/// **text**, which is what `crate_boundary` does with `fiddle-core`'s sources
/// for the same reason.
///
/// Without this lane the coupling is a comment. With it, a change to either side
/// fails here, naming both files, rather than surfacing in Task 20 as a scan
/// that found nothing to fix.
#[test]
fn the_pair_is_pinned_to_the_module_and_versions_the_shared_scanner_document_names() {
    let table = std::fs::read_to_string(
        repo_root().join("crates/fiddle-runtime/tests/support/document.rs"),
    )
    .expect("the shared scanner documents are where this suite says they are");
    let entry = format!(r#"("{MODULE}", "{VULNERABLE_VERSION}", "{FIXED_VERSION}")"#);
    assert!(
        table.contains(&entry),
        "the fixture pair moves {MODULE} from {VULNERABLE_VERSION} to \
         {FIXED_VERSION}, and document.rs's library table no longer has the row \
         {entry} that says so: a scanner document built from that table would \
         report a finding this fixture pair is not about"
    );

    assert!(
        read_fixture_file(VULNERABLE, "go.mod")
            .contains(&format!("require {MODULE} {VULNERABLE_VERSION}")),
        "the vulnerable fixture must require the version the document reports"
    );
    assert!(
        read_fixture_file(FIXED, "go.mod").contains(&format!("require {MODULE} {FIXED_VERSION}")),
        "the fixed fixture must require the version the document names as fixed"
    );
}

/// Each fixture's `Dockerfile` builds the fixture it sits beside.
///
/// The offline gate never runs `docker`: the scanner is scripted and takes an
/// image reference it never resolves, so nothing here proves an image can be
/// produced. What it does prove is the one thing that could rot silently — that
/// the recipe names files the fixture has. A `COPY` of a path that was renamed
/// would leave the pair with a Dockerfile that cannot describe it, and the
/// diff lane above would not notice, because both halves would be wrong
/// identically.
#[test]
fn each_dockerfile_copies_only_files_its_fixture_has() {
    for name in [VULNERABLE, FIXED] {
        let dockerfile = read_fixture_file(name, "Dockerfile");
        let mut copied = 0;
        for line in dockerfile.lines() {
            let Some(rest) = line.trim().strip_prefix("COPY ") else {
                continue;
            };
            // `COPY --from=<stage>` copies out of an earlier build stage rather
            // than out of the build context, so its source is not a path this
            // fixture is expected to have and there is nothing here to check.
            if rest.contains("--from=") {
                continue;
            }
            // `COPY <src>... <dest>`: everything but the last word is a source
            // path, relative to the build context, which is the fixture.
            let words: Vec<&str> = rest
                .split_whitespace()
                .filter(|word| !word.starts_with("--"))
                .collect();
            assert!(
                words.len() >= 2,
                "{name}/Dockerfile has a COPY with no source and destination: {line}"
            );
            for source in &words[..words.len() - 1] {
                assert!(
                    fixture(name).join(source).exists(),
                    "{name}/Dockerfile copies {source}, which the fixture does not have"
                );
                copied += 1;
            }
        }
        assert!(
            copied > 0,
            "{name}/Dockerfile copies nothing, so it cannot be the recipe for \
             an image of this fixture"
        );
    }
}

fn read_fixture_file(name: &str, file: &str) -> String {
    let path = fixture(name).join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|source| panic!("{} does not read: {source}", path.display()))
}

// ---------------------------------------------------------------------------
// Registration, driven through the compiled binary (Task 20.a)
// ---------------------------------------------------------------------------
//
// Everything below observes exit codes, `--json` payloads and files on disk,
// through `support::Scenario`, which resolves the binary with
// `support::fiddle_binary()`. Nothing here calls a library function, so what is
// asserted is what a caller at a shell would see.
//
// The *scenario* lanes — a vulnerable fixture yielding one pull request, an
// already-fixed one yielding an evidenced no-change, an unusable scanner exiting
// 11, a sentinel credential reaching no surface — are Task 20.b's. They need a
// world with a scripted scanner, a scripted `go`, a module proxy and a forge in
// it, and building that world is that task. What 20.a owns is whether the id
// exists at all, and the two run-level facts that could not be asserted until it
// did.

/// **A registered capability is selectable, and an unregistered one is refused.**
///
/// The distinction is in the *diagnostic* rather than in the exit code, and it
/// has to be: both invocations exit 2, so an assertion on the code alone would
/// pass for two different reasons. A selectable capability is refused for
/// something its **configuration** does not describe; an unknown one is refused
/// as an unknown id, and the diagnostic lists what this build can run.
///
/// `cve_mitigat` is a typo of the real id rather than a nonsense word, which is
/// the mistake an operator actually makes and the one a prefix match would let
/// through.
#[test]
fn the_mitigating_capability_is_selectable_and_an_unknown_one_is_not() {
    let scenario = Scenario::new();

    let selected = scenario.run_raw_with(&["--capability", "cve_mitigate"], "cve");
    let refused = String::from_utf8_lossy(&selected.stderr).to_string();
    assert_eq!(
        selected.status.code(),
        Some(2),
        "stdout = {}",
        String::from_utf8_lossy(&selected.stdout)
    );
    assert!(
        !refused.contains("unknown capability"),
        "a registered id must reach its builder, and this one was rejected at the \
         flag: {refused}"
    );
    assert!(
        refused.contains("cve_mitigate") && refused.contains("[github]"),
        "the refusal must name the capability and the table its deployment has \
         not described: {refused}"
    );

    let mistyped = scenario.run_raw_with(&["--capability", "cve_mitigat"], "cve");
    let unknown = String::from_utf8_lossy(&mistyped.stderr).to_string();
    assert_eq!(mistyped.status.code(), Some(2));
    assert!(
        unknown.contains("unknown capability"),
        "an id this build cannot run must be refused as one: {unknown}"
    );
    assert!(
        unknown.contains("cve_mitigate"),
        "the diagnostic lists every id this build can execute, and the new one \
         is among them: {unknown}"
    );
}

/// **A trackerless run does not exit 20.**
///
/// `fiddle_core::assess` has had an arm for a work item that does not apply
/// since Task 2, and until this milestone nothing built one: every production
/// caller passed a reference whose value named a tracker row. Both of Task 2's
/// arm-merge inversions therefore ran green, and *a trackerless run does not fail*
/// was asserted nowhere. This is the run-level half.
///
/// Two assertions and they are deliberately not one. The exit code alone would
/// pass for the wrong reason — a run can exit 0 without the trackerless arm
/// having been anywhere near it — so the observation is checked too: the work
/// item is `not_applicable`, it carries no `available` and no `unavailable`, and
/// nothing in the payload names a source under `stub:work/`. That last one is
/// what says the port was *not asked*, rather than asked and found wanting.
///
/// The capability is the default one, and that is the point rather than an
/// economy: what is under test is the *assessment* of a reference that names no
/// work item, which is upstream of every capability and identical for all five.
#[test]
fn a_run_over_a_trackerless_reference_is_not_a_failed_run() {
    let scenario = Scenario::new();

    let payload = scenario.run_json("cve", 0);

    assert_eq!(
        payload["outcome"], "completed",
        "a reference that names no work item is a run fiddle can act on: {payload}"
    );
    let work_item = &payload["observations"]["work_item"];
    assert!(
        work_item["not_applicable"]["reason"].is_string(),
        "the work item must be reported as a question that does not apply: {payload}"
    );
    assert!(
        work_item.get("available").is_none() && work_item.get("unavailable").is_none(),
        "a question that does not apply has no answer and no failure: {payload}"
    );
    assert!(
        !payload.to_string().contains("stub:work/"),
        "no port may be asked to read a work id that does not exist: {payload}"
    );

    // And the change set *is* read and written, under the reference's own slug —
    // which is what makes a repeat of the same sweep `complete` rather than a
    // second run of the work. A trackerless run that recorded nothing would be
    // one no later invocation could recognise.
    assert_eq!(
        scenario.read_change_marker("cve"),
        Some(scenario.expected_marker("cve")),
        "the marker is filed under the slug, because the empty value is not a name"
    );
}

/// The read-only half of the same fact: `fiddle inspect cve` succeeds and
/// derives an action, over a reference that names no work item.
///
/// Separate from the lane above because `inspect` and `run` reach the ports
/// through one call and are supposed to agree — and because this one is what a
/// person types first. Before Task 20.a it exited 0 reporting `blocked` with
/// source `stub:work/.json`: the empty value interpolated into a path, a file
/// that was never going to be there, and a diagnostic about a work item nobody
/// had asked about.
#[test]
fn inspecting_a_trackerless_reference_derives_an_action_rather_than_a_block() {
    let scenario = Scenario::new();

    let payload = scenario.inspect_json("cve");

    assert_eq!(payload["invocation_ref"], "cve", "{payload}");
    assert!(
        payload["next_action"]["execute"].is_object(),
        "a trackerless reference with no change set recorded is work to do: {payload}"
    );
    assert!(
        payload["assessment"].get("blocked").is_none(),
        "nothing about this world is unobservable: {payload}"
    );
    assert!(
        !payload.to_string().contains("stub:work/"),
        "the work-item port names no source, because it was not consulted: {payload}"
    );
}

// ---------------------------------------------------------------------------
// The world a whole sweep runs in (Task 20.b)
// ---------------------------------------------------------------------------
//
// Everything from here down drives `fiddle run cve --capability cve_mitigate` as
// a subprocess and observes exit codes, the `--json` payload, the bundle it
// names, the verdict report beside it, and a real bare repository's refs. No
// library function is called; `support::fiddle_binary()` resolves the binary, as
// `harness_discipline.rs` requires.
//
// # What had to be built, and why it is four programs rather than one fixture
//
// A sweep reaches five kinds of child, and the offline gate has none of them: a
// scanner (Wiz is testable only where its tenant credentials are), a Go
// toolchain and the module proxy behind it (this project's dev shell declares
// neither), the five checks of §2.6, and a forge. Each arrives through the
// product's own `program`/`args` seam — `[scanner] cli`, `[orchestration.cve]
// go`, a `[[workspace.checks]]` entry's `program`, `[github] cli` — and each is
// `fiddle-runtime`'s own scripted fixture rather than a second one written here.
// `support::gh_stub_binary`'s doc gives the argument in full: a second model of
// a world is free to disagree with the first, and two suites proving one
// property against two worlds prove less than one does.
//
// The `git` is **real**, and that is not an omission. The branch this run leaves
// behind is read back out of a bare repository's refs with `git for-each-ref`,
// so "one branch" is the world's answer and not fiddle's.
//
// # What the first run through this world found
//
// `cve_mitigate` could be built from **no document that parses**. Its arm read
// `[workspace] check` for the command the `run_check` tool offers a model while
// also requiring `[[workspace.checks]]`, and `config::Workspace`'s own
// conversion refuses a document naming both — so every document that satisfied
// one requirement failed the other. Eighteen tasks' modules were wired to a
// caller nothing could reach. It is fixed in `main.rs`, at the site, and this
// suite is what would have caught it again.

/// The reference every sweep runs under: the capability's own slug, naming no
/// tracker row.
///
/// A sweep is the trackerless case by construction — nobody files a ticket to
/// ask a nightly job to look at a container image — which is why
/// `a_run_over_a_trackerless_reference_is_not_a_failed_run` above is this lane's
/// upstream half.
const SWEEP_REF: &str = "cve";

/// The repository the shared pull request is proposed in, and the branch it is
/// proposed into.
const SWEEP_REPO: &str = "acme/r";
const SWEEP_BASE: &str = "main";

/// The two variables `[scanner]` names, and the forge's.
///
/// Spelled here rather than imported from `support`'s decision world because
/// this document is not that one: what the two worlds share is the *mechanism*
/// — a credential named by variable and never by value — and sharing the names
/// too would tie two documents together for no reason.
const WIZ_ID: &str = "WIZ_CLIENT_ID";
const WIZ_SECRET: &str = "WIZ_CLIENT_SECRET";
const FORGE_TOKEN: &str = "FIDDLE_GITHUB_TOKEN";
const MODEL_KEY: &str = "LITELLM_API_KEY";

/// The tenant secret a sweep is given, and the string criterion 3 hunts for.
///
/// It is `fiddle-runtime`'s own sentinel, `tests/support/cve.rs:SENTINEL_SECRET`,
/// spelled again rather than imported — this package depends on neither library,
/// which is the whole point of a black-box lane. Keeping the *value* identical is
/// what lets a reader searching the repository for a leak find both halves of the
/// proof: Task 5 asserted the argv and the diagnostic at the unit tier, and this
/// is the run.
const SENTINEL_SECRET: &str = "fiddle-secret-3b8e51d0";

/// The image every sweep is pointed at.
///
/// Never resolved by anything: `[orchestration.cve] image` has no default
/// precisely so that no build scans a tag it guessed, and the scripted scanner
/// echoes whatever it is handed. What it is *for* here is that it appears in the
/// scanner's banner, so a lane can tell a scan that happened from one that did
/// not.
const SWEEP_IMAGE: &str = "ghcr.io/acme/icecube:latest";

/// The scanner arm every ordinary sweep runs: a scan that worked, over both
/// package arrays.
///
/// Task 19 settled that no new arm was needed for the *input* scan of either
/// fixture: `libraries(DEFAULT_LIBRARY_CVES)` already names
/// `golang.org/x/crypto v0.31.0` fixed in `v0.35.0`, which is the vulnerable
/// tree's exact requirement and the fixed tree's exact shipped version. One
/// document, two dispositions, and the difference is the tree.
const SCAN_OK: &str = "ok";

/// The scanner arm a *rescan* runs, and the reason it is a different arm.
///
/// `evaluate` calls a group clean only when the rescan clears it, and a rescan
/// answered with the input scan's document never clears anything. See the arm's
/// own comment in `wiz_stub.rs`.
const RESCAN_CLEAN: &str = "library-clean";

/// The two advisories the shared document reports: one library, one OS package.
///
/// The OS one is not decoration. It is the finding no `go get` can move — `zlib`
/// and `libssl3` are a base image's, and `[orchestration.cve]` has no registry to
/// read tags from — so it is what a run *blocks* rather than fixes, and it is the
/// second fixable finding that makes `max_findings = 1` observably different from
/// `max_findings = 2`.
const LIBRARY_CVE: &str = "CVE-2026-0001";
const OS_CVE: &str = "CVE-2026-0002";

/// One disposable deployment a sweep runs in: a repository under remediation, a
/// bare "GitHub" behind the scripted `gh`, a loopback endpoint answering for the
/// model, and the document that names all four scripted programs.
///
/// It is a fourth world rather than a widening of `support::World`, and the
/// reason is what each is *for*: that one exists to drive a decision walk over a
/// repaired Rust fixture, and every knob it carries — the conversation, the
/// decision table, the repair script — is machinery a sweep has no use for. What
/// is shared is shared properly, in `support`: the four scripted binaries, the
/// model gateway, the credential removals and the project layout.
struct Sweep {
    scenario: Scenario,
    /// The scripted `gh`'s scratch directory: its request log and the bare
    /// repository it answers ref reads out of.
    stub: PathBuf,
    /// The bare repository standing in for the remote.
    remote: PathBuf,
    /// The repository being mitigated — a clone of one fixture tree, on `main`,
    /// with `origin` pointing at the bare one.
    tree: PathBuf,
    /// The endpoint the model is reached at.
    gateway: StubGateway,
}

impl Sweep {
    /// A deployment scanning `fixture` through the scanner arm `scan`, taking at
    /// most `findings` of them, with the model answering from `script`.
    ///
    /// Every argument is one a lane below varies, and none of them has a default:
    /// a builder that defaulted the scanner arm would let a lane about a *failed*
    /// scan be written as though it were about a successful one, and a builder
    /// that defaulted the budget would make `max_findings` untestable — the value
    /// nobody sets is the value the runtime already assumed.
    fn scanning(fixture: &str, scan: &str, findings: usize, script: Vec<Reply>) -> Self {
        let scenario = Scenario::new();

        // `remote.git` beside the scratch directory is the name the scripted `gh`
        // looks for; see `fiddle-runtime/tests/gh_stub/gh_stub.rs`.
        let stub = scenario.dir().join("gh-stub");
        std::fs::create_dir_all(stub.join("script")).unwrap();
        // Empty, and it stays empty: it is what a real `gh` would be pinned to,
        // and beside an absent `HOME` it is what makes an operator's keyring
        // unreachable.
        std::fs::create_dir_all(stub.join("config")).unwrap();
        let remote = stub.join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "-q", "--bare", "-b", SWEEP_BASE, "."]);

        let sweep = Sweep {
            tree: seed_repository(scenario.dir(), &remote, fixture),
            scenario,
            stub,
            remote,
            gateway: StubGateway::serving(script),
        };
        let tables = sweep.tables(scan, findings);
        sweep.scenario.append_config(&tables);
        sweep
    }

    /// The five tables a sweep needs, appended to the M0-shaped document
    /// `Scenario::new` wrote, so this document differs from the milestone
    /// baseline by exactly what this lane adds.
    ///
    /// `[[workspace.checks]]` is last because a TOML table array ends the table
    /// above it, and there are two entries because the contract needs both kinds
    /// of criterion: a check that decides by its exit status, and the rescan,
    /// which decides by the artefact it wrote. A list of one exit-zero check
    /// would leave `evaluate` with no report to compare and every group
    /// `NotCompared` — which fails closed, correctly, and would make this world
    /// unable to produce a pull request for a reason that has nothing to do with
    /// the mitigation.
    ///
    /// There is deliberately **no `[workspace] check`**. The schema refuses a
    /// document naming both shapes, and the arm that used to read it is what made
    /// this capability unbuildable; the model's `run_check` is the first entry of
    /// the list below.
    fn tables(&self, scan: &str, findings: usize) -> String {
        format!(
            "[github]\n\
             repo = \"{SWEEP_REPO}\"\n\
             base = \"{SWEEP_BASE}\"\n\
             token = {{ env = \"{FORGE_TOKEN}\" }}\n\
             cli = {{ program = {gh}, args = [\"--stub-dir\", {stub}] }}\n\
             git = \"git\"\n\
             config_dir = {config_dir}\n\
             timeout = \"120s\"\n\
             \n\
             [agent]\n\
             model = \"a-model\"\n\
             base_url = \"{base_url}\"\n\
             api_key = {{ env = \"{MODEL_KEY}\" }}\n\
             max_turns = 4\n\
             max_tokens = 512\n\
             max_changed_files = 4\n\
             deadline = \"300s\"\n\
             tool_timeout = \"300s\"\n\
             \n\
             [scanner]\n\
             cli = {{ program = {wiz}, args = [\"{scan}\"] }}\n\
             client_id = {{ env = \"{WIZ_ID}\" }}\n\
             client_secret = {{ env = \"{WIZ_SECRET}\" }}\n\
             timeout = \"300s\"\n\
             \n\
             [orchestration.cve]\n\
             image = \"{SWEEP_IMAGE}\"\n\
             max_findings = {findings}\n\
             go = {{ program = {go}, args = [] }}\n\
             \n\
             [workspace]\n\
             root = {workspaces}\n\
             fixture = {tree}\n\
             command_timeout = \"300s\"\n\
             \n\
             [[workspace.checks]]\n\
             program = {check}\n\
             args = []\n\
             success = \"exit-zero\"\n\
             \n\
             [[workspace.checks]]\n\
             program = {wiz}\n\
             args = [\"{RESCAN_CLEAN}\"]\n\
             success = \"artefact-written\"\n",
            gh = toml_string(gh_stub_binary()),
            wiz = toml_string(wiz_stub_binary()),
            go = toml_string(go_stub_binary()),
            check = toml_string(check_stub_binary()),
            stub = toml_string(&self.stub),
            config_dir = toml_string(&self.stub.join("config")),
            base_url = self.gateway.base_url(),
            workspaces = toml_string(&self.workspace_root()),
            tree = toml_string(&self.tree),
        )
    }

    // -- driving the binary --------------------------------------------------

    /// `fiddle run cve --capability cve_mitigate --json`, with all three
    /// credentials exported and the sentinel among them, run to completion and
    /// handed back unjudged.
    ///
    /// The sentinel is exported on **every** lane rather than only on the one
    /// that hunts for it. An absence is evidence only when the thing was there to
    /// be carried, and a world where the value is present exactly when somebody
    /// is looking for it is a world where every other lane's silence means
    /// nothing.
    fn run(&self) -> Output {
        self.run_with(&[])
    }

    /// The same invocation with `extra` flags appended.
    fn run_with(&self, extra: &[&str]) -> Output {
        let mut command = std::process::Command::new(support::fiddle_binary());
        // The four credential-shaped names no lane may need, removed *before* the
        // three this document names are exported, so a run ends up with exactly
        // the values it means to hand over rather than whatever the test binary
        // was launched with.
        for name in CREDENTIAL_VARS
            .iter()
            .chain([FORGE_TOKEN, MODEL_KEY, WIZ_ID, WIZ_SECRET].iter())
        {
            command.env_remove(name);
        }
        command
            .args([
                "run",
                SWEEP_REF,
                "--capability",
                "cve_mitigate",
                "--config",
                self.scenario.config_path().to_str().unwrap(),
            ])
            .args(extra)
            .arg("--json")
            .env(FORGE_TOKEN, "ghp_forge_token_for_the_sweep")
            .env(MODEL_KEY, "sk-model-key-for-the-sweep")
            .env(WIZ_ID, "wiz-client-id-for-the-sweep")
            .env(WIZ_SECRET, SENTINEL_SECRET);
        command.output().unwrap()
    }

    // -- reading what it did -------------------------------------------------

    /// The `--json` payload of a run, with its stderr quoted when it is not JSON.
    fn payload(&self, run: &Output) -> serde_json::Value {
        serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
            panic!(
                "stdout is not JSON ({e}): {}\nstderr: {}",
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr)
            )
        })
    }

    /// The bundle a run published, reached the way a downstream reader would:
    /// the payload names it in `report`, resolved against `<report.dir>`.
    fn bundle(&self, run: &Output) -> serde_json::Value {
        self.scenario.read_bundle(&self.payload(run))
    }

    /// The verdict report a run wrote beside its bundle, parsed.
    ///
    /// **Always present on every path that reached a disposition**, including the
    /// empty one — `Disposition::write_report` says why: a consumer that had to
    /// tell *the file is absent* from *there was nothing to report* would be
    /// distinguishing a failed run from a clean one by a missing file, and
    /// absence reads as success. So this panics rather than answering `None`.
    fn verdicts(&self) -> serde_json::Value {
        let path = self.scenario.report_dir().join("verdicts.json");
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("no verdict report at {} ({e})", path.display()));
        serde_json::from_slice(&bytes).unwrap()
    }

    /// Whether the verdict report holds a row for `cve`.
    fn has_verdict(&self, cve: &str) -> bool {
        self.verdicts()
            .as_array()
            .expect("the verdict report is an array")
            .iter()
            .any(|verdict| verdict["cve"] == cve)
    }

    /// Every open pull request the forge holds, read back through the scripted
    /// `gh` rather than out of its files.
    ///
    /// Through `gh` because the listing is answered from the world the writes
    /// built, so a pull request a *run* created appears here — which is the only
    /// kind this suite creates.
    fn pull_requests(&self) -> Vec<serde_json::Value> {
        let out = std::process::Command::new(gh_stub_binary())
            .args(["--stub-dir", self.stub.to_str().unwrap()])
            .args([
                "api",
                "--method",
                "GET",
                &format!("/repos/{SWEEP_REPO}/pulls?state=open"),
            ])
            .output()
            .unwrap();
        body_of(&String::from_utf8_lossy(&out.stdout))
    }

    /// Every branch the remote holds, in ref order.
    ///
    /// Read out of a real bare repository's refs rather than out of a report: a
    /// bundle saying "one branch" is fiddle's opinion, and this is the world's.
    fn remote_branches(&self) -> Vec<String> {
        let refs = git_says(
            &self.remote,
            &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
        );
        match refs.is_empty() {
            true => Vec::new(),
            false => refs.lines().map(str::to_string).collect(),
        }
    }

    /// Every REST path the scripted `gh` was asked for, in arrival order.
    ///
    /// The recorder a lane asserts a **negative** with — that the forge was never
    /// reached at all — so it is the widest record available: every call the stub
    /// was launched for, whatever it asked and whatever came back.
    fn requested_paths(&self) -> Vec<String> {
        walkdir_files(self.stub.join("requests"))
            .iter()
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .filter_map(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .filter_map(|request| {
                request["argv"].as_array()?.iter().find_map(|arg| {
                    let arg = arg.as_str()?;
                    arg.starts_with('/').then(|| arg.to_string())
                })
            })
            .collect()
    }

    /// Where an attempt branches its ephemeral worktree.
    fn workspace_root(&self) -> PathBuf {
        self.scenario.dir().join("workspaces")
    }

    /// Every file anywhere in this deployment that holds `needle`, by path
    /// relative to the project root.
    ///
    /// Paths rather than a concatenation, so a failing assertion names the file to
    /// open — and so the two kinds of hit can be told apart by
    /// [`is_fixture_recording`].
    fn files_holding(&self, needle: &str) -> Vec<String> {
        self.scenario
            .project_tree()
            .into_iter()
            .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(needle))
            .map(|(path, _)| path)
            .collect()
    }
}

/// Whether `path` is a fixture's recording of **its own environment**, rather
/// than something fiddle published.
///
/// The scripted `gh` and the scripted `wizcli` each write down every variable
/// they were handed, and the credential is one of them *by design*: those
/// recordings are how `github_cli` and Task 5's scanner lanes assert that a
/// child's environment is exactly the allowlist. So both necessarily hold a
/// secret, and neither is a surface — the child is where a credential is
/// supposed to arrive.
///
/// It is what makes criterion 3 a real question rather than a vacuous one. The
/// scanner's record is the *positive* half: the sentinel is in it, so the
/// credential genuinely travelled, and its absence everywhere else is then a
/// fact about redaction rather than about a value nobody set.
///
/// Named as a predicate rather than filtered out inside the search, so the
/// sentinel scan reports every other hit by name and cannot quietly grow an
/// exemption: a leak into a new file fails, loudly, with the path.
fn is_fixture_recording(path: &str) -> bool {
    path.starts_with("gh-stub/requests/")
        || path == "reports/scan/child.json"
        || path == "reports/rescan/child.json"
}

/// A one-commit git repository holding `fixture`'s tracked files, on
/// [`SWEEP_BASE`], with `origin` pointing at `remote` and the branch pushed.
///
/// The tracked files and not a directory walk, for [`tracked_files`]'s reason:
/// the tree a sweep is pointed at is the tree a clone gets, so a scratch file
/// somebody left in a working copy is not part of it.
///
/// It is a *copy* and never the fixture directory itself. A sweep branches a
/// worktree from this repository and commits to it, and pointing it at
/// `tests/fixtures/cve-vulnerable` would have a test suite committing into the
/// repository it is running from.
fn seed_repository(root: &Path, remote: &Path, name: &str) -> PathBuf {
    let tree = root.join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    for (path, bytes) in tracked_files(&fixture(name)) {
        let destination = tree.join(&path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(destination, bytes).unwrap();
    }
    git(&tree, &["init", "-q", "-b", SWEEP_BASE, "."]);
    git(&tree, &["add", "-A"]);
    git(
        &tree,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "the fixture under remediation",
        ],
    );
    git(
        &tree,
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    git(&tree, &["push", "-q", "origin", SWEEP_BASE]);
    tree
}

/// The model's whole turn for one group: a final report, claiming nothing was
/// edited.
///
/// **One turn and no tool call, and that is the shape of this capability rather
/// than an economy.** A sweep applies the bump itself — `go get` then
/// `go mod tidy`, before the model is briefed — so what a model is asked for is a
/// judgement about a tree that has already moved, and the ordinary answer is that
/// there is nothing further to do. The commit that follows still carries
/// `go.mod` and `go.sum`, because `land` stages `Workspace::changed_files` — what
/// **git** saw — and never the list the model reported. A script that claimed the
/// files would therefore prove nothing about which of the two the landing reads.
fn a_bump_needing_no_edit() -> Vec<Reply> {
    vec![accepted(completion(
        serde_json::json!({
            "role": "assistant",
            "content": serde_json::json!({
                "changed_files": [],
                "summary": "the requirement was moved to the fixed release; no source change was needed",
                "claimed_complete": true,
            }).to_string(),
        }),
        "stop",
    ))]
}

/// The branch a sweep opened, and the assertion that it opened exactly one.
///
/// The remote starts with [`SWEEP_BASE`] on it, so "one branch" is one branch
/// *beside* the base — and the base has to still be there, because a run that
/// deleted it would otherwise satisfy a bare count of one.
fn the_one_new_branch(sweep: &Sweep) -> String {
    let branches = sweep.remote_branches();
    let new: Vec<&String> = branches.iter().filter(|it| *it != SWEEP_BASE).collect();
    assert!(
        branches.contains(&SWEEP_BASE.to_string()),
        "the base branch must survive the run: {branches:?}"
    );
    assert_eq!(
        new.len(),
        1,
        "a sweep opens exactly one branch for its shared pull request: {branches:?}"
    );
    assert!(
        new[0].starts_with("security/cve-remediation-"),
        "the branch is named for what it carries and the day it was opened: {branches:?}"
    );
    new[0].clone()
}

/// One file of the commit `branch` points at, as the **remote** holds it.
///
/// Read out of the bare repository with `git show`, so what a lane asserts about
/// the mitigation is what a person cloning the branch would get — not what the
/// worktree held before it was torn down, and not what a report said.
fn pushed_file(sweep: &Sweep, branch: &str, path: &str) -> String {
    git_says(&sweep.remote, &["show", &format!("{branch}:{path}")])
}

// ---------------------------------------------------------------------------
// The four scenarios (Task 20.b)
// ---------------------------------------------------------------------------

/// **A vulnerable fixture yields exactly one pull request and one branch, and
/// the branch really carries the fix.**
///
/// The first half of the milestone's central claim, and the first thing in this
/// repository to drive `CveMitigate::execute` at all: every seam below it has a
/// suite, the composition is compile-checked in `main.rs`, and until this lane
/// nothing ran scan → plan → checkout → dedup → budget → attribution → group →
/// bump → attempt → judge → land → publish in that order against a real tree.
///
/// # Why the count is not the assertion
///
/// "Exactly one pull request" is satisfied by a run that opened one about
/// nothing, so the count is asserted beside three things that are about the
/// *mitigation*: the branch the forge is proposing carries `go.mod` at
/// [`FIXED_VERSION`] and not at [`VULNERABLE_VERSION`]; the pull request is
/// labelled `security/cve`, which is the label the *next* run discovers it by
/// and therefore the one that makes this a shared object rather than a fresh
/// one each night; and the tree observation in the bundle says which revision
/// the attempt ran at.
///
/// The verdict report is asserted too, and its content is the discriminating
/// part: exactly one row, for the OS advisory, with the sentence
/// `GroupError::Unselectable` produces. A run that had silently failed to fix
/// the library advisory would have *two* rows, and a run that had fixed neither
/// would still have opened no pull request — so the report and the branch are
/// two independent readings of the same claim.
#[test]
fn a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 2, a_bump_needing_no_edit());

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    // The forge holds one pull request, and it is the shared one: labelled, open,
    // proposed into the configured base.
    let pulls = sweep.pull_requests();
    assert_eq!(pulls.len(), 1, "exactly one pull request: {pulls:?}");
    let pull = &pulls[0];
    assert_eq!(pull["base"]["ref"], SWEEP_BASE, "{pull}");
    assert!(
        pull["labels"]
            .as_array()
            .is_some_and(|labels| labels.iter().any(|it| it["name"] == "security/cve")),
        "the label is how the next run finds this object again: {pull}"
    );

    // And one branch beside the base, which is the one the pull request names.
    let branch = the_one_new_branch(&sweep);
    assert_eq!(pull["head"]["ref"], branch, "{pull}");

    // The mitigation itself, read off the remote rather than off a report.
    let landed = pushed_file(&sweep, &branch, "go.mod");
    assert!(
        landed.contains(&format!("{MODULE} {FIXED_VERSION}")),
        "the branch must carry the requirement at the fixed release: {landed}"
    );
    assert!(
        !landed.contains(VULNERABLE_VERSION),
        "and must not still carry the vulnerable one: {landed}"
    );

    // Which revision the attempt ran at. Task 17's sentence — *the bundle records
    // base, PR head, and which the attempt ran against* — asserted at the run
    // level for the first time. Nothing was open, so the attempt ran at the base,
    // and the base is the sha the remote really holds for it.
    let bundle = sweep.bundle(&run);
    assert_eq!(
        bundle["observations"]["tree"],
        serde_json::json!({
            "base_revision": git_says(&sweep.remote, &["rev-parse", SWEEP_BASE]),
            "pr_head": serde_json::Value::Null,
            "attempt_tree": "base_revision",
        }),
        "{bundle}"
    );

    // The run is filed under this capability's own vocabulary and not a
    // neighbour's.
    assert_eq!(bundle["progress"][0]["stage"], "mitigate", "{bundle}");

    // One verdict, for the advisory a `go get` cannot reach, with the sentence
    // that says whose limitation it is.
    let verdicts = sweep.verdicts();
    assert_eq!(
        verdicts.as_array().map(Vec::len),
        Some(1),
        "the library advisory was fixed and contributes no verdict; the OS one \
         could not be: {verdicts}"
    );
    assert_eq!(verdicts[0]["cve"], OS_CVE, "{verdicts}");
    assert!(
        verdicts[0]["rationale"]
            .as_str()
            .is_some_and(|why| why.contains("registry this build does not read")),
        "the verdict says whose limitation it is rather than blaming upstream: \
         {verdicts}"
    );
}

/// **An already-fixed fixture yields no pull request and an evidenced
/// no-change.**
///
/// The other half, over the same document and the same scanner arm. Task 19
/// settled that the difference cannot be a property of the scanner — the stub
/// does not inspect what it scans — so the *only* thing that differs between
/// this lane and the one above is the tree, which is what makes the pair
/// evidence about a mitigation.
///
/// # What makes the no-change *evidenced* rather than merely quiet
///
/// Three readings, and each one rules out a different way of arriving here for
/// the wrong reason:
///
/// 1. **The scan happened, and it reported the library advisory.** The
///    scanner's artefact is on disk under `<report.dir>/scan` and names
///    [`LIBRARY_CVE`]. A run that reported nothing to do because it never looked
///    would have no artefact at all — the failure
///    `an_unusable_scanner_exits_eleven_and_reaches_no_forge` covers from the
///    other side — and a run whose document happened not to mention the advisory
///    would make every assertion below true for a reason that has nothing to do
///    with the tree.
/// 2. **The advisory was settled rather than left unfixed.** The verdict report
///    holds the OS advisory and *not* the library one. `verdicts_of`'s contract
///    is that every advisory a run leaves unfixed gets a row carrying the
///    sentence that decided it, so — given (1) — the library advisory's absence
///    is the positive claim "this run left nothing unfixed about a CVE it was
///    told about". Had the tree still been vulnerable and the fix failed, the
///    row would be there with the rationale that refused it.
/// 3. **Nothing was published.** No branch beside the base, no pull request, and
///    no branch or pull request receipt on the execution — a run that opened one
///    and then reported no change would fail all three.
#[test]
fn an_already_fixed_fixture_produces_an_evidenced_no_change() {
    let sweep = Sweep::scanning(FIXED, SCAN_OK, 2, a_bump_needing_no_edit());

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "an image whose advisories are already dealt with is not a failed run — \
         stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    // 1. The scan really happened, and it really named the advisory.
    let scanned = std::fs::read_to_string(sweep.scenario.report_dir().join("scan/scan.json"))
        .expect("the scanner left no artefact, so this no-change is not evidence about an image");
    assert!(
        scanned.contains(LIBRARY_CVE),
        "the document this run was answering does not name the advisory the \
         fixture is about, so nothing below is evidence about the tree: {scanned}"
    );

    // 2. The library advisory was settled; the OS one was not.
    let verdicts = sweep.verdicts();
    assert!(
        !sweep.has_verdict(LIBRARY_CVE),
        "an advisory the tree already ships the fix for is not something this \
         run left unfixed: {verdicts}"
    );
    assert!(
        sweep.has_verdict(OS_CVE),
        "and the advisory nothing could move must still be reported, or the \
         report is empty for two different reasons: {verdicts}"
    );

    // 3. Nothing was published.
    assert!(
        sweep.pull_requests().is_empty(),
        "a run with nothing to fix opens nothing: {:?}",
        sweep.pull_requests()
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and leaves the remote exactly as it found it"
    );
    let evidence = payload["capability_executions"][0]["evidence"].to_string();
    assert!(
        !evidence.contains("/tree/") && !evidence.contains("/pull/"),
        "a run that published nothing must quote no branch and no pull request: \
         {evidence}"
    );
}

/// **An unusable scanner exits 11 and opens nothing.**
///
/// The row Design §3 calls the one this milestone is most likely to get wrong,
/// asserted at the level where getting it wrong costs something: a nightly job
/// whose scanner was broken must not report "nothing to fix" and go green.
///
/// # Why the exit code is not the assertion either
///
/// Because a run can exit non-zero for a dozen reasons, and because the claim is
/// about what a *watcher* can conclude. So three things are asserted together,
/// and each of them separately distinguishes this run from
/// `an_already_fixed_fixture_produces_an_evidenced_no_change` — the genuine
/// clean scan, which is Task 16's
/// `a_scanner_that_found_nothing_and_one_that_never_ran_are_not_the_same_result`
/// raised to the whole run:
///
/// - **11 and not 0.** `Retryable`, which tells automation to come back, and not
///   the row an operator would read as done.
/// - **The outcome is not `completed`.** A watcher reading the payload rather
///   than the status line reaches the same conclusion.
/// - **The forge was never reached at all.** Not one REST path was requested, so
///   there is no branch, no pull request, and — this is the ordering claim — the
///   scan provably comes *before* the plan. `sweep` is what asks the forge which
///   pull request to share, and it is never entered when the scan has no
///   document.
///
/// The arm is `exit-nonzero-no-file`: a scanner that ran, said something on
/// stderr and wrote nothing. It is deliberately not `no-such-image` or
/// `no-daemon`, whose classifications are about the world rather than about the
/// scanner, and it ends identically to them — so what is under test here is the
/// adapter's reading of an absent artefact.
#[test]
fn an_unusable_scanner_exits_eleven_and_reaches_no_forge() {
    let sweep = Sweep::scanning(
        VULNERABLE,
        "exit-nonzero-no-file",
        2,
        a_bump_needing_no_edit(),
    );

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(11),
        "a scan that produced no document is retryable, never a successful \
         no-change — stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let payload = sweep.payload(&run);
    assert_ne!(
        payload["outcome"], "completed",
        "a watcher reading the payload must reach the same conclusion as one \
         reading the status line: {payload}"
    );

    assert_eq!(
        sweep.requested_paths(),
        Vec::<String>::new(),
        "the scan is asked before the forge is, so a scan with no document \
         reaches no forge at all"
    );
    assert!(
        sweep.pull_requests().is_empty(),
        "and opens nothing: {:?}",
        sweep.pull_requests()
    );
    // And no tree either. `sweep` is what makes a worktree, and it is never
    // entered without a document — so the ordering *scan, then plan, then a
    // tree* is readable from the outside as three absences behind one exit code.
    assert!(
        !sweep.workspace_root().exists(),
        "a scan with no document creates no worktree: {:?}",
        walkdir_files(sweep.workspace_root())
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and leaves the remote exactly as it found it"
    );
}

/// **A sentinel credential appears on no surface a run produces.**
///
/// The negative that would be vacuous if the value were not really there, so it
/// is planted: [`SENTINEL_SECRET`] is exported as `WIZ_CLIENT_SECRET` on *every*
/// run this suite drives, and the scanner is a child that records the whole
/// environment it received. The first assertion below is the positive half —
/// the credential genuinely reached the scanner — and every assertion after it
/// is only worth something because of it.
///
/// Three surfaces, named by the criterion: stdout, the diagnostic stream, and
/// the published bundle. The search is actually **wider** than that — every file
/// anywhere in the disposable project — with the two fixtures' recordings of
/// their own environment exempted by name; see [`is_fixture_recording`]. A hit
/// anywhere else fails with the path, so a leak into a file nobody thought of is
/// caught rather than skipped.
///
/// Task 5 proved the argv and the diagnostic halves at the unit tier. This is
/// the run: the same value, through a document, an environment, a spawn, a
/// report, a verdict file and a bundle.
#[test]
fn no_credential_reaches_stdout_a_diagnostic_or_a_published_bundle() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 2, a_bump_needing_no_edit());

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // The positive half. Without it every assertion below is satisfied by a value
    // that was never anywhere near the thing it is absent from.
    let planted = sweep.scenario.report_dir().join("scan").join("child.json");
    assert!(
        std::fs::read_to_string(&planted)
            .unwrap_or_default()
            .contains(SENTINEL_SECRET),
        "the scanner's record does not hold the credential, so its absence \
         elsewhere is not evidence: {}",
        planted.display()
    );

    assert!(
        !String::from_utf8_lossy(&run.stdout).contains(SENTINEL_SECRET),
        "a credential reached stdout: {}",
        String::from_utf8_lossy(&run.stdout)
    );
    assert!(
        !String::from_utf8_lossy(&run.stderr).contains(SENTINEL_SECRET),
        "a credential reached a diagnostic: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        !sweep.bundle(&run).to_string().contains(SENTINEL_SECRET),
        "a credential reached the published bundle"
    );

    let leaked: Vec<String> = sweep
        .files_holding(SENTINEL_SECRET)
        .into_iter()
        .filter(|path| !is_fixture_recording(path))
        .collect();
    assert_eq!(
        leaked,
        Vec::<String>::new(),
        "the credential reached a file that is not a fixture's record of its own \
         environment"
    );
}

/// **The bound the document sets is the bound the sweep applies.**
///
/// `[orchestration.cve] max_findings` was in the product document's example and
/// in no reader for the whole of M4a, so the number a deployment believed it had
/// set was a constant in the runtime — the *same* number, which is exactly why
/// nobody noticed. `config_check`'s lane proves the key parses and round-trips;
/// nothing proved it reached `Budget::apply`, and this is that.
///
/// The two runs differ in one digit and in nothing else. The scan reports two
/// fixable advisories, so:
///
/// - at **2**, both are taken: the library one is fixed and the OS one is judged
///   and reported as unselectable, which is the world
///   `a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch`
///   asserts against;
/// - at **1**, only the library one is taken. The OS advisory is *deferred* —
///   never judged — and a deferred finding contributes no verdict, because a row
///   for it would be this build claiming an opinion it does not have about
///   something it did not look at.
///
/// So the observable is a verdict row that is there at 2 and absent at 1, with
/// the pull request unchanged across both. A budget wired to a constant of 5
/// would take both in either run and leave the row in place; a budget applied as
/// a filter *before* deduplication would change which finding survives rather
/// than how many.
#[test]
fn the_bound_the_document_sets_is_the_bound_the_sweep_applies() {
    let bounded = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_bump_needing_no_edit());

    let run = bounded.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert!(
        !bounded.has_verdict(OS_CVE),
        "the second fixable finding is over the bound, so it was deferred rather \
         than judged, and a deferred finding is not a verdict: {}",
        bounded.verdicts()
    );
    assert_eq!(
        bounded.verdicts().as_array().map(Vec::len),
        Some(0),
        "and nothing else took its place: {}",
        bounded.verdicts()
    );

    // The finding *inside* the bound is still fixed, so this is a bound on how
    // much a run takes and not a bound that stopped it working.
    assert_eq!(bounded.pull_requests().len(), 1);
    let branch = the_one_new_branch(&bounded);
    assert!(
        pushed_file(&bounded, &branch, "go.mod").contains(&format!("{MODULE} {FIXED_VERSION}")),
        "the advisory within the bound is still mitigated"
    );
}
