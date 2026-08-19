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
    accepted, body_of, calls, check_stub_binary, completion, gh_stub_binary, git, git_says,
    go_stub_binary, reports, toml_string, walkdir_files, wiz_stub_binary, Reply, Scenario,
    StubGateway, CREDENTIAL_VARS,
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

/// The two-group fixture and the world its sweep runs in are one fact, kept
/// honest the way the pair above is.
///
/// `cve-two-libraries` is never compiled — the sweep's `go` is scripted — so
/// nothing about it fails loudly on its own. What it depends on is a chain of
/// three agreements, and each link, broken, would present as a lane that folds
/// nothing rather than as a fixture defect:
///
/// 1. `document.rs`'s library table has to hold the second module at these two
///    versions, or the scanner reports a finding about a module the tree does
///    not have and the selection comes back empty;
/// 2. `go_proxy.rs`'s upstream has to have *published* the fix, or the second
///    group is refused a target for want of a release list and is blocked
///    before it ever reaches the fold rule;
/// 3. the fixture has to require both modules, at the versions the document
///    reports as current, or there is no second group at all.
///
/// Both support files are read as **text**, for
/// [`the_pair_is_pinned_to_the_module_and_versions_the_shared_scanner_document_names`]'s
/// reason: `crate_boundary.rs` asserts this package links neither library crate,
/// so a `#[path]` include would drag a dependency across a boundary this
/// repository checks.
#[test]
fn the_two_library_fixture_is_pinned_to_what_its_world_publishes() {
    let table = std::fs::read_to_string(
        repo_root().join("crates/fiddle-runtime/tests/support/document.rs"),
    )
    .expect("the shared scanner documents are where this suite says they are");
    let entry =
        format!(r#"("{SECOND_MODULE}", "{SECOND_VULNERABLE_VERSION}", "{SECOND_FIXED_VERSION}")"#);
    assert!(
        table.contains(&entry),
        "a document naming two library advisories puts the second one in \
         {SECOND_MODULE}, and document.rs's library table no longer has the row \
         {entry} that says so"
    );

    let proxy = std::fs::read_to_string(
        repo_root().join("crates/fiddle-runtime/tests/support/go_proxy.rs"),
    )
    .expect("the offline module proxy is where this suite says it is");
    // Each constant by name, and the release keyed on it, rather than "one of
    // these two names holds this string" — which both versions could satisfy
    // through the same constant.
    for (name, version) in [
        ("INDIRECT_VERSION", SECOND_VULNERABLE_VERSION),
        ("INDIRECT_FIXED", SECOND_FIXED_VERSION),
    ] {
        assert!(
            proxy.contains(&format!(r#"pub const {name}: &str = "{version}";"#)),
            "go_proxy.rs's {name} has to be {version}, which is what the \
             document names for {SECOND_MODULE}"
        );
        assert!(
            proxy.contains(&format!(
                "module: INDIRECT_MODULE,\n        version: {name},"
            )),
            "and the offline upstream has to have *published* it: a module with \
             no release list leaves the second group blocked for want of a \
             target, and the fold lane would then be asserting about a group \
             that never ran"
        );
    }

    let manifest = read_fixture_file(TWO_LIBRARIES, "go.mod");
    for (module, version) in [
        (MODULE, VULNERABLE_VERSION),
        (SECOND_MODULE, SECOND_VULNERABLE_VERSION),
    ] {
        assert!(
            manifest.contains(&format!("require {module} {version}")),
            "the two-group fixture must require {module} at the version the \
             document reports as current: {manifest}"
        );
    }
}

/// **The tree whose first bump clears its second group is pinned to what its
/// world publishes.**
///
/// Three halves have to agree for that world to exist at all, and none of them
/// can see the other two: the scanner document's library table, the offline
/// upstream's release list, and this fixture's manifest. The one that fails
/// silently is the middle one — a `CLEARING_FIXED` release that stopped requiring
/// `x/net` would leave the second group with an ordinary bump to make, the run
/// would attempt it, and the lane below would fail with a count nobody could
/// trace back to a table.
///
/// It is [`the_two_library_fixture_is_pinned_to_what_its_world_publishes`]
/// applied to the second of the two clearance worlds, and it asserts the one thing
/// that lane has no reason to: that a named release *requires* another module at a
/// named version.
#[test]
fn a_bump_that_moves_a_later_groups_requirement_is_pinned_to_what_its_world_publishes() {
    let table = std::fs::read_to_string(
        repo_root().join("crates/fiddle-runtime/tests/support/document.rs"),
    )
    .expect("the shared scanner documents are where this suite says they are");
    let entry = format!(
        r#"("{CLEARING_MODULE}", "{CLEARING_VULNERABLE_VERSION}", "{CLEARING_FIXED_VERSION}")"#
    );
    assert!(
        table.contains(&entry),
        "the clearing advisory is reported against {CLEARING_MODULE}, and \
         document.rs's library table no longer has the row {entry} that says so"
    );
    assert!(
        table.contains(&format!(
            r#"pub const CLEARING_LIBRARY_CVE: &str = "{CLEARING_LIBRARY_CVE}";"#
        )),
        "and under the id this suite spells, which it cannot import"
    );

    let proxy = std::fs::read_to_string(
        repo_root().join("crates/fiddle-runtime/tests/support/go_proxy.rs"),
    )
    .expect("the offline module proxy is where this suite says it is");
    for (name, version) in [
        ("CLEARING_MODULE", CLEARING_MODULE),
        ("CLEARING_VULNERABLE", CLEARING_VULNERABLE_VERSION),
        ("CLEARING_FIXED", CLEARING_FIXED_VERSION),
    ] {
        assert!(
            proxy.contains(&format!(r#"pub const {name}: &str = "{version}";"#)),
            "go_proxy.rs's {name} has to be {version}, which is what the document \
             names for {CLEARING_MODULE}"
        );
    }
    // The requirement itself, which is the whole world: without it the bump moves
    // one module and the second group has ordinary work to do.
    assert!(
        proxy.contains(
            "        module: CLEARING_MODULE,\n        version: CLEARING_FIXED,\n        \
             requires: &[(INDIRECT_MODULE, INDIRECT_FIXED)],"
        ),
        "the offline upstream has to publish {CLEARING_MODULE} \
         {CLEARING_FIXED_VERSION} as *requiring* {SECOND_MODULE} at \
         {SECOND_FIXED_VERSION} — that requirement is what raises the second \
         group's tree past its own fix, and without it this world is two \
         ordinary bumps"
    );

    let manifest = read_fixture_file(CLEARED_BY_A_BUMP, "go.mod");
    for (module, version) in [
        (CLEARING_MODULE, CLEARING_VULNERABLE_VERSION),
        (SECOND_MODULE, SECOND_VULNERABLE_VERSION),
    ] {
        assert!(
            manifest.contains(&format!("require {module} {version}")),
            "the clearing fixture must require {module} at the version the \
             document reports as current: {manifest}"
        );
    }
    assert!(
        !manifest.contains(MODULE),
        "and nothing else: a third requirement is a third group, and this world's \
         claim is about the second one — {manifest}"
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
///
/// **Nor does the product, and that is a decision rather than a gap in the
/// harness.** Design §2.1's Prepare ends in `docker build`; ADR 020 puts that
/// half in the host workflow, because a build that pulls base layers cannot live
/// in an offline credential-free gate and a *stubbed* build would produce a
/// digest meaning nothing. What fiddle does instead is publish the digest it
/// scanned beside the revision it remediated —
/// `observations.tree.scanned_image_digest`, asserted in
/// [`a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch`].
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
/// pass for the wrong reason — a run can exit 0 without the trackerless reading
/// having been anywhere near it — so the observation is checked too: the work
/// item is `not_applicable`, it carries no `available` and no `unavailable`, and
/// nothing in the payload names a source under `stub:work/`. That last one is
/// what says the port was *not asked*, rather than asked and found wanting.
///
/// The capability is **named**, and the deterministic one, because what is under
/// test is the *assessment* of a reference that names no work item — upstream of
/// every capability and identical for all five — over a world that has a scanner,
/// a toolchain and a forge in it for none of them. This lane read the default
/// until the default became scheme-derived: `fiddle run cve` now selects the
/// sweep, which this M0-shaped document cannot describe, so a lane that took the
/// default would be asserting the sweep's configuration requirements instead of
/// the assessment. Naming `stub_mark` is what keeps the subject the one the lane
/// is about; `the_documented_invocation_with_no_capability_flag_reaches_the_sweep`
/// is where the default itself is asserted.
#[test]
fn a_run_over_a_trackerless_reference_is_not_a_failed_run() {
    let scenario = Scenario::new();

    let payload = scenario.run_json_with(&["--capability", "stub_mark"], "cve", 0);

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

    // And the change set *is* read and written, under the reference's own slug.
    // A trackerless run that recorded nothing would be one no later reader could
    // see the shape of — which is the whole of what the marker is for here. It is
    // deliberately *not* what makes a repeat of this invocation `complete`: this
    // reference names no work item and so has no completion state, and reading the
    // marker as one is what let this very invocation account a sweep as done. See
    // `a_marker_against_a_trackerless_reference_does_not_account_the_sweep_as_done`
    // and ADR 023.
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

/// What the scripted scanner resolves [`SWEEP_IMAGE`] to, and publishes in its
/// banner.
///
/// `fiddle-runtime`'s own `tests/wiz_stub/wiz_stub.rs:STUB_DIGEST`, spelled again
/// rather than imported, for [`SENTINEL_SECRET`]'s reason — this package depends
/// on neither library. Keeping the value identical is what lets a reader
/// searching the repository find both halves.
///
/// It is deliberately unlike [`SWEEP_IMAGE`]: a lane that found the tag where it
/// expected the digest would be finding the name that can move, which is the
/// whole failure the key exists to prevent.
const SCANNED_DIGEST: &str =
    "sha256:6f1b0d2c9a4e7385bd1c05fa9e37642c8b0d5713ae629f04c8d17b6a3e59042d";

/// The scanner arm every ordinary sweep runs: a scan that worked, over both
/// package arrays.
///
/// Task 19 settled that no new arm was needed for the *input* scan of either
/// fixture: `libraries(DEFAULT_LIBRARY_CVES)` already names
/// `golang.org/x/crypto v0.31.0` fixed in `v0.35.0`, which is the vulnerable
/// tree's exact requirement and the fixed tree's exact shipped version. One
/// document, two dispositions, and the difference is the tree.
const SCAN_OK: &str = "ok";

/// The scanner arm that reports an image with nothing wrong with it: both
/// package arrays present, both empty.
///
/// Distinct from every failure arm on purpose. Design §3's first and last rows
/// are read from the same absence — *no findings* and *no document* — and a
/// suite that could only produce the second could not show that a run tells them
/// apart.
const SCAN_CLEAN: &str = "clean-image";

/// The scanner arm that reports the library advisory and nothing else.
///
/// The arm a run reaches Design §3 row 3 through, and it needs to exist because
/// [`SCAN_OK`]'s OS advisory names a fix: it is in the fixable set, and over a
/// tree that has already dealt with the library half it is the one thing still
/// open, so the run makes its one attempt and lands on row 4 or row 5. Row 3 is
/// *there is nothing left at all*, so it needs a document whose every finding the
/// tree can already have settled.
const SCAN_LIBRARY_ONLY: &str = "library-only";

/// The scanner arm that reports one library advisory naming **no published fix**,
/// and nothing else.
///
/// The arm a run reaches Design §3 row 2 through, and — as far as this suite can
/// build one — the only one that can. Row 2 is *nothing was attempted and there
/// is still something to report*, and a run attempts what its bound left of
/// `Projection::fixable`, so the world has to be one where that set is empty
/// while the verdict report is not. An advisory the scanner published no
/// `fixedVersion` for is exactly that: never offered to an attempt, and reported
/// as upstream-blocked regardless.
///
/// Its OS array is present and empty for the same arithmetic. Every other
/// document here writes an OS advisory that names a fix, which is fixable, which
/// would be attempted — and row 5 shadows row 2 whenever anything is attempted.
const SCAN_NO_FIX: &str = "no-published-fix";

/// The scanner arm that reports the library advisory and **two** OS advisories.
///
/// The one document over which a disposition's three finding sets — already
/// fixed, verdicts, deferred — are all non-empty and all hold different
/// advisories. Every other arm leaves at least one of them empty over every
/// tree, and an assertion that a deferred advisory is absent from an empty set
/// is satisfied by the emptiness rather than by the deferral. See the arm's own
/// comment in `wiz_stub.rs` for the arithmetic.
const SCAN_TWO_OS: &str = "two-os-advisories";

/// The scanner arm a *rescan* runs, and the reason it is a different arm.
///
/// `evaluate` calls a group clean only when the rescan clears it, and a rescan
/// answered with the input scan's document never clears anything. See the arm's
/// own comment in `wiz_stub.rs`.
const RESCAN_CLEAN: &str = "library-clean";

/// What the ordinary `[[workspace.checks]]` entry is given, and what makes it
/// fail instead.
///
/// The scripted check exits zero when nobody asks otherwise, so the passing
/// world is the empty argument list; `--exit 1` is a check that ran and said no.
/// Named rather than written at the two call sites, because the pair is only
/// meaningful together — a lane reading `&["--exit", "1"]` has to go and find
/// out what the *other* lanes pass before it knows what is being varied.
const PASSING_CHECK: &[&str] = &[];
const FAILING_CHECK: &[&str] = &["--exit", "1"];

/// The two advisories the shared document reports: one library, one OS package.
///
/// The OS one is not decoration. It names a fix like the library one, so it is
/// the second **fixable** finding — which is what makes `max_findings = 1`
/// observably different from `max_findings = 2`. What no tree in this suite can
/// do is *clear* it: `zlib` and `libssl3` are a base image's, so an attempt shown
/// it declines it or fails to move it, and it comes back as a verdict with an
/// attempt behind it. Before M4c four mechanical Go rules refused it before any
/// model saw it, and that is the difference the census below turns on.
const LIBRARY_CVE: &str = "CVE-2026-0001";
const OS_CVE: &str = "CVE-2026-0002";

/// The second OS advisory, reported by [`SCAN_TWO_OS`] and by nothing else.
///
/// Spelled here as well as in `tests/support/document.rs` because this suite
/// runs the stub as a child and cannot import from it; the arm is the only
/// producer, so a drift between the two shows up as a lane that finds no such
/// finding rather than as a lane that quietly asserts about the wrong one.
const SECOND_OS_CVE: &str = "CVE-2026-0005";

/// The scanner arm reporting the library advisory at **MEDIUM**, with an empty OS
/// array.
///
/// The world `[orchestration.cve] severities` is asked in. A deployment that names
/// no grades acts on `HIGH` and `CRITICAL`, so this whole document selects nothing
/// and the run has *nothing to do*; a deployment that names `MEDIUM` gets the same
/// group [`SCAN_OK`] produces, from the same bytes. The pair is what makes the key
/// wired rather than parsed — see
/// [`the_grades_the_document_named_are_the_grades_the_run_acted_on`].
const SCAN_MEDIUM_LIBRARY: &str = "medium-library-advisory";

/// The grades that document is written with, and the one it is not.
///
/// `MEDIUM` beside the two a document naming none already means, because that is
/// what an operator widening a sweep would actually write: this key is a set and
/// not a floor, so widening it means naming every grade you want, and a lane whose
/// document said `["MEDIUM"]` alone would be asserting about a deployment that
/// stopped acting on `CRITICAL`.
const GRADES_INCLUDING_MEDIUM: &[&str] = &["CRITICAL", "HIGH", "MEDIUM"];

/// The scanner arm that reports **two library advisories**, in two different
/// modules, beside the usual OS one.
///
/// The only arm from which a run forms two *attemptable* groups. A group is one
/// bump target, and every other document here has one library finding and an OS
/// one — and the OS half is a base image, which `target_version` refuses for
/// want of a registry, so it is blocked before it is ever attempted. One
/// attemptable group means `fold` is only ever asked about the first group of a
/// run, which is the case it answers `Proceed` to by definition.
const SCAN_TWO_LIBRARIES: &str = "two-library-advisories";

/// The second library advisory, reported by [`SCAN_TWO_LIBRARIES`] and by
/// nothing else, against [`SECOND_MODULE`].
///
/// Spelled here as well as in `tests/support/document.rs` for [`SECOND_OS_CVE`]'s
/// reason.
const SECOND_LIBRARY_CVE: &str = "CVE-2026-0003";

/// The module that advisory is in, and the two versions of it its world knows.
///
/// The second row of `document.rs`'s library table, which is the row a second
/// library finding lands in — that builder cycles its packages by position, so
/// two advisories are two modules rather than two findings against one.
/// [`the_two_library_fixture_is_pinned_to_what_its_world_publishes`] holds these
/// three strings, the fixture's `go.mod` and the module proxy's release table
/// together.
const SECOND_MODULE: &str = "golang.org/x/net";
const SECOND_VULNERABLE_VERSION: &str = "v0.24.0";
const SECOND_FIXED_VERSION: &str = "v0.28.0";

/// The fixture tree that requires both of those modules.
///
/// A third tree rather than a second requirement in [`VULNERABLE`], and its own
/// `README.md` gives the argument: the pair exists to isolate one mitigation,
/// and anything else added to either half is a second thing a difference in
/// outcome could be attributed to.
const TWO_LIBRARIES: &str = "cve-two-libraries";

/// The advisory against the module whose bump does the clearing, and that
/// module's two versions.
///
/// The third row of `document.rs`'s library table. It sorts before
/// [`SECOND_MODULE`] — `github.com` before `golang.org` — which is what makes it
/// the *earlier* group, because a run walks its groups in target order.
const CLEARING_LIBRARY_CVE: &str = "CVE-2026-0006";
const CLEARING_MODULE: &str = "github.com/docker/docker";
const CLEARING_VULNERABLE_VERSION: &str = "v24.0.7";
const CLEARING_FIXED_VERSION: &str = "v24.0.9";

/// The fixture tree that world runs in, requiring [`CLEARING_MODULE`] and
/// [`SECOND_MODULE`] at the versions the document reports as current.
///
/// A fourth tree rather than a second scanner arm over [`TWO_LIBRARIES`], and its
/// own `README.md` gives the argument: the offline upstream is one table read by
/// every tree, so the requirement that makes this world move `x/net` would move it
/// in the fold fixture too.
const CLEARED_BY_A_BUMP: &str = "cve-cleared-by-a-bump";

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
        Sweep::scanning_rescanning(fixture, scan, RESCAN_CLEAN, findings, script)
    }

    /// The same deployment with the **grades it acts on** named in its document.
    ///
    /// A fourth entry point for [`scanning_rescanning`]'s reason, and the knob it
    /// varies is the one every other lane here must not touch: a document that
    /// names no grades is what every neighbour is written as, and it means `HIGH`
    /// and `CRITICAL`. The rescan arm is a parameter too, because a world whose
    /// input scan reports no OS finding needs a rescan that reports none either —
    /// [`RESCAN_CLEAN`] carries the input scans' OS advisory, which such a world
    /// never had, and condition (b) would read it as a finding that just appeared.
    ///
    /// [`scanning_rescanning`]: Sweep::scanning_rescanning
    fn scanning_grades(
        fixture: &str,
        scan: &str,
        rescan: &str,
        grades: &[&str],
        findings: usize,
        script: Vec<Reply>,
    ) -> Self {
        Sweep::deployment(
            fixture,
            scan,
            rescan,
            findings,
            script,
            PASSING_CHECK,
            Some(grades),
        )
    }

    /// The same deployment with the **rescan**'s arm chosen too.
    ///
    /// A second entry point rather than a fifth argument on the one above,
    /// because the rescan arm is a default every lane but one wants: `evaluate`
    /// calls a group clean only when the rescan clears it, so a world that did
    /// not answer [`RESCAN_CLEAN`] here could not produce a pull request at all,
    /// and a lane that had to name it would be naming the reason its neighbours
    /// work.
    ///
    /// The one lane that varies it is about the row where *a move was made,
    /// judged and taken back* — which is reached by a rescan that proves
    /// nothing, and is the only honest way to reach it from outside the process:
    /// the checks and the tree are the product's, and the scanner's second
    /// answer is the seam an operator really does control.
    fn scanning_rescanning(
        fixture: &str,
        scan: &str,
        rescan: &str,
        findings: usize,
        script: Vec<Reply>,
    ) -> Self {
        Sweep::world(fixture, scan, rescan, findings, script, PASSING_CHECK)
    }

    /// The same deployment with the ordinary check **failing** on every group.
    ///
    /// A third entry point for [`scanning_rescanning`]'s reason, and it varies
    /// the one remaining half of a judgement. `Evaluation::accepted` is *every
    /// check passed **and** the rescan cleared*, and the two halves are reached
    /// from opposite ends of this world: a rescan that proves nothing is the
    /// scanner's second answer, and a check that fails is the check's exit
    /// status. A lane about the *first* half — a group that ends needs-work
    /// while its rescan is perfectly clean — can only be built here, and it has
    /// to be built somewhere, because a group whose rescan still reports its
    /// advisory is refused a fold by two conditions at once and could not say
    /// which one did it.
    ///
    /// [`scanning_rescanning`]: Sweep::scanning_rescanning
    fn scanning_with_a_failing_check(
        fixture: &str,
        scan: &str,
        findings: usize,
        script: Vec<Reply>,
    ) -> Self {
        Sweep::world(fixture, scan, RESCAN_CLEAN, findings, script, FAILING_CHECK)
    }

    /// What the three entry points above are, with nothing defaulted.
    ///
    /// Private, so that no lane names all six: each of the three says which one
    /// thing it varies, and a lane free to set every knob is a lane whose reader
    /// has to work out which of them the claim rests on.
    fn world(
        fixture: &str,
        scan: &str,
        rescan: &str,
        findings: usize,
        script: Vec<Reply>,
        check_args: &[&str],
    ) -> Self {
        Sweep::deployment(fixture, scan, rescan, findings, script, check_args, None)
    }

    /// [`world`] with the document's grade set named, or left to its default.
    ///
    /// `None` writes **no `severities` line at all**, which is not the same as
    /// writing the default's two grades: the property every lane in this file but
    /// one rests on is that an omitting document means what this build has always
    /// meant, and a harness that filled the key in would make that untestable from
    /// here — the same reason `Sweep::scanning` will not default the scanner arm.
    ///
    /// [`world`]: Sweep::world
    #[allow(clippy::too_many_arguments)]
    fn deployment(
        fixture: &str,
        scan: &str,
        rescan: &str,
        findings: usize,
        script: Vec<Reply>,
        check_args: &[&str],
        grades: Option<&[&str]>,
    ) -> Self {
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
        let tables = sweep.tables(scan, rescan, findings, check_args, grades);
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
    fn tables(
        &self,
        scan: &str,
        rescan: &str,
        findings: usize,
        check_args: &[&str],
        grades: Option<&[&str]>,
    ) -> String {
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
             {severities}\
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
             args = [{check_args}]\n\
             success = \"exit-zero\"\n\
             \n\
             [[workspace.checks]]\n\
             program = {wiz}\n\
             args = [\"{rescan}\"]\n\
             success = \"artefact-written\"\n",
            // Absent where no lane named a set, so every other document in this
            // file is byte-for-byte the one it was before this key existed.
            severities = grades
                .map(|grades| {
                    let named = grades
                        .iter()
                        .map(|grade| format!("{grade:?}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("severities = [{named}]\n")
                })
                .unwrap_or_default(),
            gh = toml_string(gh_stub_binary()),
            wiz = toml_string(wiz_stub_binary()),
            go = toml_string(go_stub_binary()),
            check = toml_string(check_stub_binary()),
            // `toml_string`'s rule applied to an argument rather than a path:
            // a TOML basic string, written by hand for that helper's reason.
            check_args = check_args
                .iter()
                .map(|argument| format!("{argument:?}"))
                .collect::<Vec<_>>()
                .join(", "),
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
        self.run_selecting(&["--capability", "cve_mitigate"], extra)
    }

    /// **The invocation the documents name**: `fiddle run cve --mode unattended`,
    /// with no `--capability` at all.
    ///
    /// A second entry point rather than an argument on the one above, because
    /// what it varies is not a knob of the world — it is *how the capability is
    /// chosen*, and every lane in this file but one deliberately names it. The
    /// flag is what made the rest of this suite pass while the entry point routed
    /// nowhere: thirty-two lanes reached `cve_mitigate` through a value that
    /// appears in no design section, no ADR and no bean, so none of them could
    /// see that an operator typing the documented command ran M0's `stub_mark`.
    ///
    /// `--mode unattended` is passed explicitly even though it is clap's default,
    /// because the string under test is the one a reader of the design copies out
    /// of it, not a shortest equivalent of it.
    fn run_unqualified(&self) -> Output {
        self.run_selecting(&[], &["--mode", "unattended"])
    }

    /// The same invocation **without `--json`**, which is the surface an operator
    /// gets by default.
    ///
    /// A third entry point rather than a flag on the one below, because `--json`
    /// is appended *there* — every lane in this file reads a payload, which is how
    /// `run_human` came to be missing a row the payload carries.
    fn run_plain(&self) -> Output {
        self.command_selecting(&["--capability", "cve_mitigate"], &[])
            .output()
            .unwrap()
    }

    /// One arrangement of the environment for all three, so the credential
    /// scrubbing and the three exports cannot differ between the invocation the
    /// documents name and the invocation the rest of this suite drives.
    fn run_selecting(&self, selection: &[&str], extra: &[&str]) -> Output {
        self.command_selecting(selection, extra)
            .arg("--json")
            .output()
            .unwrap()
    }

    /// Everything the three share, up to but not including the output surface.
    fn command_selecting(&self, selection: &[&str], extra: &[&str]) -> std::process::Command {
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
            .args(["run", SWEEP_REF])
            .args(selection)
            .args(["--config", self.scenario.config_path().to_str().unwrap()])
            .args(extra)
            .env(FORGE_TOKEN, "ghp_forge_token_for_the_sweep")
            .env(MODEL_KEY, "sk-model-key-for-the-sweep")
            .env(WIZ_ID, "wiz-client-id-for-the-sweep")
            .env(WIZ_SECRET, SENTINEL_SECRET);
        command
    }

    /// The correlation marker the last run left in `<stub.root>`, if it left one.
    ///
    /// Read rather than removed, and that is a change worth naming. This used to
    /// be `forget_that_the_last_run_happened`, which deleted
    /// `<stub.root>/changes/cve.json` between two runs of the same scenario
    /// because `orchestration::run` read it before the capability was reached,
    /// found a change set carrying this invocation's marker, derived
    /// `NextAction::Complete` and executed nothing — so two invocations in one
    /// scratch directory were one run and one no-op, whatever the forge held.
    ///
    /// That is fixed rather than worked around. A reference that names no work
    /// item has no completion state, so the marker is a record that a run
    /// happened and never a reason not to run again — ADR 023 — and
    /// [`a_second_run_reads_the_first_runs_own_commit_body`] now runs its second
    /// night with nothing at all happening in between, which is what a nightly
    /// job in CI really does. The marker is still *written*, and that lane asserts
    /// it survived the second run, so a fix that quietly stopped writing one
    /// would not pass unnoticed.
    fn change_marker(&self) -> Option<String> {
        self.scenario.read_change_marker(SWEEP_REF)
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

    /// The row a run published, out of the bundle a reader would open.
    ///
    /// **The bundle and not stdout.** A payload is what the process that ran saw
    /// and a bundle is what it left behind, and the whole question this key
    /// exists for is what somebody reading the record a week later can conclude.
    fn disposition(&self, run: &Output) -> serde_json::Value {
        let bundle = self.bundle(run);
        bundle
            .get("disposition")
            .cloned()
            .unwrap_or_else(|| panic!("this run published no disposition at all: {bundle}"))
    }

    /// Every evidence reference in a published bundle, from the executions and
    /// the progress entries alike.
    ///
    /// Both lists, because a receipt is copied into both and a check applied to
    /// one of them would be a check on neither.
    fn evidence(&self, run: &Output) -> Vec<String> {
        let bundle = self.bundle(run);
        ["capability_executions", "progress"]
            .iter()
            .filter_map(|key| bundle[key].as_array().cloned())
            .flatten()
            .filter_map(|entry| entry["evidence"].as_array().cloned())
            .flatten()
            .filter_map(|reference| reference.as_str().map(str::to_string))
            .collect()
    }

    /// Every evidence reference this run published is a **logical** one.
    ///
    /// # Why this is a claim worth making
    ///
    /// Because a receipt is the one thing in a bundle whose job is to be
    /// followed, and a host absolute path — `/var/folders/…/reports/verdicts.json`
    /// on the machine that happened to run — cannot be followed from anywhere
    /// else while describing the layout of the machine that could. Every other
    /// receipt this capability publishes is logical (`cve:acme/r/pull/7`), and
    /// `<report.dir>` is already the prefix a bundle's own `published` path is
    /// stripped against for exactly this reason.
    ///
    /// Two readings rather than one, because either alone has a way of passing
    /// for the wrong reason: a locator rooted at `/` is a POSIX absolute path
    /// whatever it names, and a locator holding this deployment's scratch
    /// directory is this machine's layout whatever shape it has. A logical
    /// reference is neither — it may well hold a `/`, as `acme/r/pull/7` does,
    /// and that is not what is refused here.
    fn assert_every_receipt_is_logical(&self, run: &Output) {
        let root = self.scenario.dir().display().to_string();
        for reference in self.evidence(run) {
            let locator = reference
                .split_once(':')
                .map(|(_, locator)| locator)
                .unwrap_or(&reference);
            assert!(
                !locator.starts_with('/'),
                "a published receipt quotes a host absolute path, which names                  nothing on any other machine: {reference}"
            );
            assert!(
                !reference.contains(&root),
                "a published receipt quotes this deployment's own scratch                  directory ({root}): {reference}"
            );
        }
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

/// A script **no attempt in a world should ever consume**, and a report that is
/// refused if one does.
///
/// Every lane that takes this is a world where the run reaches no attempt at all
/// — an empty image, a tree that settles everything, an unusable scanner, a
/// document naming nothing this deployment acts on, a pull request already
/// covering the rest. The gateway is scripted, so such a lane needs *a* reply to
/// hand over and needs it never to be asked for.
///
/// **It disposes of [`LIBRARY_CVE`] and nothing else, and that is deliberate.**
/// `unaccounted` refuses a report naming an advisory the prompt did not show, so
/// a world that unexpectedly reaches an attempt over anything but the library
/// advisory alone ends `retryable` and names the mismatch — which is a lane
/// failing loudly on the premise it was written under. A reply with an empty
/// disposition list would have been accepted by any world showing nothing, and
/// there is no such world here.
fn a_script_no_attempt_consumes() -> Vec<Reply> {
    vec![accepted(completion(
        serde_json::json!({
            "role": "assistant",
            "content": serde_json::json!({
                "changed_files": [],
                "summary": "the requirement already resolves to the fixed release",
                "claimed_complete": true,
                "findings": [{
                    "cve": LIBRARY_CVE,
                    "attempted": true,
                    "note": "the requirement already resolves to the fixed release",
                }],
            }).to_string(),
        }),
        "stop",
    ))]
}

/// The ordinary repair, in the worlds whose one selected advisory is
/// [`LIBRARY_CVE`]: the attempt moves the requirement and the sums itself, and
/// declares both files.
///
/// **The attempt does the editing because nothing else does any more.** Until M4c
/// a sweep ran `go get` and `go mod tidy` before the model was briefed, so the
/// honest script was a report claiming no change and the commit still carried
/// `go.mod` — which is why the helper this replaces was called
/// `a_bump_needing_no_edit`. There is no bump now: which release clears an
/// advisory, and which file carries it, is the attempt's own judgement, so a
/// script that changes no file leaves the landing with nothing to commit and the
/// run ends `retryable` with *the attempt changed no file, so there is nothing to
/// propose*.
fn a_repair_moving_the_requirement() -> Vec<Reply> {
    an_attempt(
        &[
            ("go.mod", vulnerable_manifest()),
            ("go.sum", vulnerable_sums()),
        ],
        &[LIBRARY_CVE],
        &[],
    )
}

/// The attempt over a world whose selected advisories are all the base image's:
/// it declines every one of them and changes nothing.
///
/// **Declining a base-image advisory is the agent's judgement now, and it used to
/// be Rust's.** M4a resolved every OS finding to the `Dockerfile` and then refused
/// it with `Unselectable { why: "selecting a base-image tag needs a registry this
/// build does not read" }` — a sentence about a limitation of the *build*, written
/// into a verdict before any model was consulted. M4c deletes the refusal along
/// with the rest of the ecosystem arithmetic: an OS finding is shown to the
/// attempt like any other, and *there is no fix I can apply without a registry* is
/// the attempt's own note. That is design §6's one gain, and this helper is where
/// the suite spends it.
///
/// It changes no file, so the landing has nothing to commit — and it does not need
/// one: a declined advisory is still there at the rescan, so the attempt is
/// needs-work and what the landing does is put back rather than commit.
fn an_attempt_declining(shown: &[&str]) -> Vec<Reply> {
    an_attempt(&[], &[], shown)
}

/// The vulnerable fixture's manifest with its requirement moved to the release
/// that carries the fix.
///
/// Derived from the fixture by replacing the version rather than spelled out, for
/// [`two_libraries_manifest`]'s reason: a fixture whose pin moves does not leave
/// every lane in this file writing a manifest nobody has.
fn vulnerable_manifest() -> String {
    read_fixture_file(VULNERABLE, "go.mod").replace(VULNERABLE_VERSION, FIXED_VERSION)
}

/// Its sums, moved with it. See [`vulnerable_manifest`].
fn vulnerable_sums() -> String {
    read_fixture_file(VULNERABLE, "go.sum").replace(VULNERABLE_VERSION, FIXED_VERSION)
}

/// One attempt's whole turn: every edit it makes, and then the report that
/// declares them and disposes of every advisory it was shown.
///
/// **A tool call per edit, and that is the shape of this capability now.** The
/// sweep applies no bump before the model is briefed — which version clears an
/// advisory, and which file carries it, is the attempt's own judgement — so a
/// script that changed no file leaves `land` with nothing to commit. The edits go
/// through the binary's own `write_file` tool, into the binary's own worktree,
/// exactly as `binary_repair`'s `a_real_repair` does: what is asserted afterwards
/// is then a tree the product wrote rather than one the fixture arranged.
///
/// `fixed` and `declined` are both taken because the report must account for
/// **every** advisory the prompt showed and for none it did not — `unaccounted`
/// refuses either way round, so which advisories a script names is part of the
/// world and not decoration. `claimed_complete` follows `declined` rather than
/// being fixed at `true`: an attempt that declined something has not claimed it
/// finished. Nothing in the product branches on it — `cve_protocol`'s
/// `nothing_in_this_workspace_decides_on_claimed_complete` is that claim — so it
/// is evidence here and no more.
fn an_attempt(edits: &[(&str, String)], fixed: &[&str], declined: &[&str]) -> Vec<Reply> {
    let mut script: Vec<Reply> = edits
        .iter()
        .map(|(path, contents)| {
            accepted(calls(
                "write_file",
                serde_json::json!({ "path": path, "contents": contents }),
            ))
        })
        .collect();

    let dispositions: Vec<serde_json::Value> = fixed
        .iter()
        .map(|cve| {
            serde_json::json!({
                "cve": cve,
                "attempted": true,
                "note": "moved the requirement to the release that carries the fix",
            })
        })
        .chain(declined.iter().map(|cve| {
            serde_json::json!({
                "cve": cve,
                "attempted": false,
                "note": "no fix I can apply to this project without reading a registry",
            })
        }))
        .collect();

    script.push(accepted(reports(serde_json::json!({
        "changed_files": edits.iter().map(|(path, _)| *path).collect::<Vec<_>>(),
        "summary": "the requirements this project pins were moved to the releases that carry the fixes",
        "claimed_complete": declined.is_empty(),
        "findings": dispositions,
    }))));
    script
}

/// The two-library fixture's manifest with **both** requirements moved.
///
/// Derived from the fixture by replacing the versions rather than spelled out, so
/// a fixture whose pins move does not leave this lane asserting about a manifest
/// nobody has.
fn two_libraries_manifest() -> String {
    read_fixture_file(TWO_LIBRARIES, "go.mod")
        .replace(VULNERABLE_VERSION, FIXED_VERSION)
        .replace(SECOND_VULNERABLE_VERSION, SECOND_FIXED_VERSION)
}

/// Its sums, moved with it. See [`two_libraries_manifest`].
fn two_libraries_sums() -> String {
    read_fixture_file(TWO_LIBRARIES, "go.sum")
        .replace(VULNERABLE_VERSION, FIXED_VERSION)
        .replace(SECOND_VULNERABLE_VERSION, SECOND_FIXED_VERSION)
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

/// Every commit `branch` adds to the base, oldest first, as its body and the
/// paths it changed.
///
/// Out of the bare repository for [`pushed_file`]'s reason. The changed paths
/// are `diff-tree` against the parent rather than anything a report said,
/// because that is the only place *an empty commit* is observable: an empty
/// commit is one whose tree is its parent's, and nothing in a body, a count or a
/// branch name distinguishes it from a commit that changed something.
fn pushed_commits(sweep: &Sweep, branch: &str) -> Vec<(String, Vec<String>)> {
    let revisions = git_says(
        &sweep.remote,
        &["rev-list", "--reverse", &format!("{SWEEP_BASE}..{branch}")],
    );
    revisions
        .lines()
        .map(|sha| {
            let body = git_says(&sweep.remote, &["log", "-1", "--format=%B", sha]);
            let changed = git_says(
                &sweep.remote,
                &["diff-tree", "--no-commit-id", "--name-only", "-r", sha],
            );
            let changed = match changed.is_empty() {
                true => Vec::new(),
                false => changed.lines().map(str::to_string).collect(),
            };
            (body, changed)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The four scenarios (Task 20.b)
// ---------------------------------------------------------------------------
//
// # Seven rows, and the mutation each one fails under
//
// Design §8: *every disposition must fail under a mutation its neighbours
// survive*. `cve_dispositions`' header records that proof at the unit tier, and
// for most of M4a that was the only tier that had it — five of the seven rows
// produced identical observable output from outside a run, so no mutation to
// `disposition` could be told apart out here at all. `fiddle-1cqg` published the
// row on the bundle, which is what makes the run-tier proof constructible; this
// is that proof.
//
// Each mutation was applied alone to `cve::verdict::disposition`, the whole of
// this file was run under it, and the lanes named are the complete set that went
// red. Every other lane in the file stayed green.
//
// **Re-measured under M4c's rewire**, all seven, because that commit deleted two
// lanes and changed the world of nine more — and a census nobody re-runs is worth
// less than no census, which is the note the previous re-measurement left here.
//
//   row 6  ScanUnusable            reason -> Reason::NothingToDo
//     an_unusable_scanner_exits_eleven_and_reaches_no_forge
//   row 4  PullRequest             the `any(Clean)` test -> `false`
//     a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch
//     the_bound_the_document_sets_is_the_bound_the_sweep_applies
//     the_grades_the_document_named_are_the_grades_the_run_acted_on
//     a_second_run_over_a_shared_pull_request_rewrites_its_body_and_works_in_its_tree
//     a_run_whose_shared_body_is_unchanged_dispatches_no_rewrite
//     a_second_run_reads_the_first_runs_own_commit_body
//   row 5  UnsafeWithoutDirection  the `!attempted.is_empty()` test -> `false`
//     an_unprovable_repair_is_reverted_and_filed_as_needing_direction
//     a_check_that_says_no_reverts_the_attempt_and_publishes_nothing
//     an_already_fixed_fixture_yields_a_no_change_the_bundle_files_as_needing_direction
//     a_deferred_finding_is_in_neither_the_verdict_set_nor_the_already_fixed_set
//   row 2  VerdictsOnly            the `!verdicts.is_empty()` test -> `false`
//     an_advisory_with_no_published_fix_is_reported_without_being_attempted
//   row 7  AlreadyInProgress       the `!covers.is_empty()` test -> `false`
//     an_open_pull_request_covering_the_rest_reaches_already_in_progress
//     the_plain_rendering_names_the_row_a_run_reached_and_its_pull_request
//     a_second_run_reads_the_first_runs_own_commit_body
//   row 3  AlreadyFixed            the `!already_fixed.is_empty()` test -> `false`
//     a_tree_that_settles_every_finding_reaches_already_fixed
//   row 1  NothingToDo             the fall-through -> Reason::AlreadyFixed
//     a_scan_of_an_empty_image_reaches_nothing_to_do
//     the_grades_the_document_named_are_the_grades_the_run_acted_on
//
// # Row 2 is reachable, and the census said otherwise for one commit
//
// This section used to read *row 2 is unreachable, and that is a fact about the
// design rather than a gap*, and every sentence of it was wrong. It is left
// described rather than silently replaced, because the failure was not a stale
// note: **the falsity was load-bearing**. The census is where a reader goes to
// find out which arms this file can discriminate, and a reader who believed this
// paragraph would not have written the lane below — which is what happened for
// the length of one commit.
//
// What it said, and what is actually the case:
//
// - *M4c deleted the only producer of such a run.* It deleted **a** producer. The
//   four mechanical Go rules really did refuse a finding before any model saw it —
//   an OS advisory got `Unselectable { why: "selecting a base-image tag needs a
//   registry this build does not read" }` and became a verdict with no attempt
//   behind it — and that is gone. But `verdicts_of` reads
//   `Projection::upstream_blocked` before it reads anything of the run's, and that
//   set predates M4c and survived it untouched.
// - *Nothing refuses a finding before an attempt now.* True, and it is the wrong
//   premise. An upstream-blocked advisory is not one this build refused; it is one
//   the **scanner published no fix for**. `Projection::fixable` never held it, so
//   the bound never took it, so no attempt was ever shown it — and it is in the
//   verdict report all the same, filed `upstream_blocked`.
// - *Every run with verdicts has attempts, so row 5 shadows row 2 in every world
//   this file can build.* A document whose only advisory names no `fixedVersion`
//   is a world this file can build, and does: `SCAN_NO_FIX`. Nothing is fixable,
//   so nothing is attempted, so `!attempted.is_empty()` is false and row 5 does
//   not fire; the verdict list is not empty, so row 2 does.
// - *A lane that constructed a verdict with no attempt would be asserting about a
//   run the product cannot produce.* The lane below constructs nothing. It runs
//   the binary against a scanner arm and reads the bundle, exactly as its six
//   neighbours do.
//
// The one true sentence in it was that `cve_dispositions` also reaches the row at
// the unit tier over a hand-assembled `Run`. That is still worth having and is
// still not a substitute: injectivity in memory is not a distinction anybody
// downstream can act on, which is the argument
// `a_scan_of_an_empty_image_reaches_nothing_to_do` makes at length for the three
// silent rows.
//
// Row 2 is reached through `upstream_blocked` and through nothing else, so of
// `verdicts_of`'s three producers exactly one can put a run here — see that
// function's own doc, which carries the same distinction.
//
// # Two lanes came off this list, and one came back reduced
//
// `a_group_an_earlier_bump_already_cleared_is_folded_into_an_empty_commit` and
// `a_group_a_bump_moved_past_its_fix_in_the_tree_is_not_reported_as_unfixed` were
// row 4's, and both were about grouping and folding — a run forms no groups now,
// so there is no earlier group whose rescan a later one could fold on. They are
// deleted rather than adapted. `a_needs_work_groups_rescan_is_not_folded_on` was
// row 5's and carried a second claim that is not about grouping at all — *a check
// that says no reverts the work* — which is why
// `a_check_that_says_no_reverts_the_attempt_and_publishes_nothing` stands in its
// place over the same world.
//
// # Why every mutation switches a row *off* rather than rewiring it
//
// Because `disposition` is a fall-through table and the direction matters. A
// mutation that makes row N fire *more widely* moves the rows below it as well —
// turn row 4's `any(Clean)` into `any(!Clean)` and the needs-work worlds start
// arriving on row 4, so row 5's lanes go red for a reason that has nothing to do
// with row 5, and the run would have proved nothing about either. Switching a row
// off leaves every world that never reached it untouched and moves only the
// worlds that did, one row down.
//
// What that costs is worth stating rather than hiding, because it is a fact
// about the table and not a defect in a lane: these mutations show each row is
// load-bearing for its own lanes, and they show the **shadowing** the table is
// built on. Under row 2's mutation both `verdicts_only` lanes land on
// `already_fixed`; under row 3's, the settled-tree world lands on
// `nothing_to_do`. That is the table saying out loud that row 3 is only ever
// reached when row 2's test is false, and row 1 only when row 3's is — which is
// why [`a_tree_that_settles_every_finding_reaches_already_fixed`] needs a
// document with nothing left over in it to reach row 3 at all.

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
/// part: **no rows at all**, beside a `deferred` list holding the advisory the
/// bound left. A run that had silently failed to fix the library advisory would
/// have a row for it, and a run that had fixed neither would still have opened no
/// pull request — so the report and the branch are two independent readings of the
/// same claim.
///
/// # Why the bound is one, which it was not
///
/// M4a ran this world at two and asserted a verdict row for the OS advisory
/// carrying `registry this build does not read` — a refusal Rust made before any
/// model was consulted. M4c deletes it: an OS finding is shown to the attempt like
/// any other, the attempt declines it, and a declined advisory is still there at
/// the rescan, so under design §3 the whole attempt is needs-work and the commit
/// is reverted. At a bound of two there is therefore **no pull request to assert
/// about**, and this lane is about the one there is. One takes the library
/// advisory and defers the base image's, which is the smallest world in which a
/// run publishes at all.
///
/// That world — both advisories shown, one declined, everything taken back — is
/// the verdict's claim rather than the publication's, and
/// [`the_bound_the_document_sets_is_the_bound_the_sweep_applies`] is where the
/// bound of two is still driven.
#[test]
fn a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());

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
    //
    // And beside it the fourth key, which is ADR 020's half of a question this
    // milestone could otherwise not answer: *which image were these verdicts
    // measured against?* Fiddle does not build the image it scans — the host
    // workflow does — so the pair here is a correspondence made checkable rather
    // than one this run verified. Asserted in the same object literal as the
    // three revisions on purpose: the pair is one record, and a run that
    // published a revision with no digest beside it would be the thing the key
    // exists to prevent.
    let bundle = sweep.bundle(&run);
    assert_eq!(
        bundle["observations"]["tree"],
        serde_json::json!({
            "base_revision": git_says(&sweep.remote, &["rev-parse", SWEEP_BASE]),
            "pr_head": serde_json::Value::Null,
            "attempt_tree": "base_revision",
            "scanned_image_digest": SCANNED_DIGEST,
        }),
        "{bundle}"
    );
    // The digest and never the tag. The configuration named
    // `ghcr.io/acme/icecube:latest`; what the bundle has to carry is what the
    // scan *resolved* that to, because a tag is a name whoever pushes next can
    // move and two scans of one tag are not two scans of one image. Publishing
    // the tag is the plausible wrong answer — it is the value the capability
    // already holds in its own configuration — and the equality above catches
    // it today.
    //
    // These two are not that assertion again in weaker form. They are the case
    // the equality cannot see: a **fixture** edit that changes what the stub
    // announces. `SCANNED_DIGEST` and `wiz_stub.rs`'s `STUB_DIGEST` are the same
    // value spelled twice, so moving both to something tag-shaped leaves the
    // equality green while the bundle no longer names an image by its bytes.
    // What is pinned here is the *shape of the fact*, which is not free to
    // follow the fixture.
    let published = bundle["observations"]["tree"]["scanned_image_digest"]
        .as_str()
        .unwrap_or_default();
    assert_ne!(
        published, SWEEP_IMAGE,
        "the bundle must name what the scan resolved, not the tag it was \
         handed: {bundle}"
    );
    assert!(
        published.starts_with("sha256:"),
        "and it must be a digest rather than any other spelling of the image: \
         {bundle}"
    );

    // The run is filed under this capability's own vocabulary and not a
    // neighbour's.
    assert_eq!(bundle["progress"][0]["stage"], "mitigate", "{bundle}");

    // Which row of Design §3 this is, and the evidence for it. The branch and
    // the number are named on this row and on no other — a run that observed a
    // shared branch and committed nothing to it has none to report — so this is
    // the assertion that separates it from the in-progress row, which carries a
    // number and no branch.
    let reached = sweep.disposition(&run);
    assert_eq!(reached["reason"], "pull_request", "{reached}");
    assert_eq!(reached["branch"], branch, "{reached}");
    assert_eq!(reached["pull_request"], pulls[0]["number"], "{reached}");
    // And the attempt behind it, including the claim the product branches on
    // nowhere. Design §2.5: `claimed_complete` is evidence beside the exit code
    // that overruled it — and evidence nothing publishes is a field, not
    // evidence. This is the first place a reader outside the process can see it.
    assert_eq!(
        reached["attempts"],
        serde_json::json!([{
            "cves": [LIBRARY_CVE],
            "status": "clean",
            "claimed_complete": true,
            "forbidden": [],
        }]),
        "{reached}"
    );
    sweep.assert_every_receipt_is_logical(&run);

    // No verdicts, and the base-image advisory on the deferred list instead.
    //
    // The emptiness is the positive claim and not an absence: the one advisory
    // this run took was fixed, so it leaves no row, and the one it did not take
    // was never judged, so a row for it would be this build claiming an opinion
    // it does not have. A run that had pushed a branch without really moving the
    // requirement would have a row for the library advisory here — which is what
    // makes this a second reading of the assertion above rather than a restatement
    // of the count.
    let verdicts = sweep.verdicts();
    assert_eq!(
        verdicts.as_array().map(Vec::len),
        Some(0),
        "the advisory this run took was fixed, and the one it did not take was \
         never judged: {verdicts}"
    );
    assert_eq!(
        reached["deferred"],
        serde_json::json!([{ "cve": OS_CVE, "bound": 1 }]),
        "and the advisory over the bound is on the record with the number that \
         put it there, rather than silently missing: {reached}"
    );
}

/// **The invocation the documents name reaches the sweep, with no `--capability`
/// at all.**
///
/// Design §1, design §6 and ADR 019 all say `fiddle run cve --mode unattended`.
/// Until this lane, that command executed M0's `stub_mark` and exited 0 reporting
/// `completed`: `main.rs` resolved an absent `--capability` to `Selection::Mark`
/// for every scheme, so the entry point every document points an operator at
/// reached the sweep from nowhere. Worse than a skipped scan — the stub run wrote
/// the correlation marker under this reference's own slug, after which the sweep
/// was *accounted for*, so a host running it nightly would report success having
/// never scanned.
///
/// # Why this lane cannot be written with the flag
///
/// Because thirty-two lanes in this file already are. Every one of them reaches
/// the capability through `--capability cve_mitigate`, which proves the
/// capability works and proves nothing about whether an operator can invoke it —
/// the seam between a reference and the capability it selects belonged to no
/// task, and that is exactly how twenty lanes passed over an unreachable entry
/// point. [`Sweep::run_unqualified`] is the whole point of this lane: if the
/// argument list it builds ever grows a `--capability`, this lane stops being
/// about anything.
///
/// # Why the capability id is not the only assertion
///
/// A payload naming `cve_mitigate` is satisfied by a run that selected the right
/// capability and then did nothing, so the id is asserted beside the sweep's own
/// two outputs: one pull request, and a branch that really carries `go.mod` at
/// [`FIXED_VERSION`]. Those are
/// [`a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch`]'s
/// assertions over the same world, deliberately: what differs between the two
/// lanes is one argument list, so any difference in outcome is that argument
/// list's.
///
/// The M0 stub is asserted *absent* too, and by name. It is the failure this
/// lane exists to catch, and a reader who sees `stub_mark` in the message knows
/// immediately which defect came back rather than that some id was unexpected.
#[test]
fn the_documented_invocation_with_no_capability_flag_reaches_the_sweep() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());

    let run = sweep.run_unqualified();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    let executed = &payload["capability_executions"][0]["capability_id"];
    assert_ne!(
        executed, "stub_mark",
        "`fiddle run cve --mode unattended` ran M0's deterministic stub, which is \
         the defect: the documented invocation reports `completed` having scanned \
         nothing, and files the marker that accounts the sweep as done: {payload}"
    );
    assert_eq!(
        executed, "cve_mitigate",
        "an absent `--capability` over a `cve` reference must select the sweep: \
         {payload}"
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    // And the sweep really ran: the forge holds the one shared pull request, and
    // the branch it names carries the requirement at the fixed release.
    let pulls = sweep.pull_requests();
    assert_eq!(
        pulls.len(),
        1,
        "the documented invocation must reach the same publication the flagged \
         one does: {pulls:?}"
    );
    let branch = the_one_new_branch(&sweep);
    assert_eq!(pulls[0]["head"]["ref"], branch, "{}", pulls[0]);
    let landed = pushed_file(&sweep, &branch, "go.mod");
    assert!(
        landed.contains(&format!("{MODULE} {FIXED_VERSION}")),
        "the branch must carry the requirement at the fixed release: {landed}"
    );

    // Filed under the sweep's own vocabulary, which is the other half of the
    // repro: the defect reported `stub_mark/mark completed — wrote correlation
    // marker …`, and a run that selected the sweep and published M0's stage
    // would leave a record indistinguishable from it.
    let bundle = sweep.bundle(&run);
    assert_eq!(
        bundle["progress"][0]["stage"], "mitigate",
        "the record must be written in the vocabulary of what ran: {bundle}"
    );
}

/// **A correlation marker filed against the sweep's reference does not account
/// the sweep as done.**
///
/// The other half of ADR 022's defect, and the half that decision does not
/// close. [ADR 022](../../../docs/technical/decisions/022-the-scheme-selects-the-capability.md)
/// stops `fiddle run cve` from *accidentally* reaching `stub_mark`; it prevents
/// new markers of that provenance and repairs no marker already on disk. A host
/// that ran the documented command before it has one, and
/// `fiddle run cve --capability stub_mark` is still a legal invocation that
/// writes another — so this world is reachable today and not only historically.
///
/// # What the marker did, and why the spelling `cve` is not the reason
///
/// `assess` read the change set for a marker equal to this invocation's
/// correlation key, and `correlation_key` is derived from the project and the
/// *reference* — no capability enters it. So the marker `stub_mark` wrote under
/// `changes/cve.json` is byte-identical to the one the sweep would write, the
/// assessment read `Satisfied`, `derive_next` returned `Complete` before the
/// capability was consulted, and the run exited 0 reporting `completed` having
/// scanned nothing.
///
/// Nothing in that mechanism is about `cve`. It is about a reference that names
/// no work item: such a reference has no completion state of its own, so a
/// marker on its change set records that some run wrote one and evidences
/// nothing about whether an image was scanned. Any capability sharing a
/// trackerless reference inherits it, which is why the fix is the trackerless
/// reading in [`fiddle_core::assess`] — a branch on
/// [`fiddle_core::WorkStateView::has_completion_state`], not on a spelling — and
/// [ADR 023](../../../docs/technical/decisions/023-a-sweep-has-no-completion-state.md)
/// is where the reasoning lives.
///
/// # Why the sweep is named here rather than defaulted
///
/// Because the subject is the *assessment* and not the selection.
/// `the_documented_invocation_with_no_capability_flag_reaches_the_sweep` above
/// owns the default; a lane that took it would fail for either reason and say
/// which one only by accident.
///
/// # Why the premise is written into the world rather than run
///
/// This lane used to establish its premise through the binary — `fiddle run cve
/// --capability stub_mark`, then a guard requiring that run to exit 0 — on the
/// reasoning that a world reached by a real invocation is a world that can
/// really happen. The guard was sound and the world was real, and the lane was
/// still the wrong instrument, for a reason worth writing down because it is not
/// obvious:
///
/// **a premise that runs the code under test cannot fail the claim.** The rule
/// this lane exists to hold is
/// [`fiddle_core::WorkStateView::has_completion_state`], and the marking run
/// derives from it too — `orchestration::concluded` asks it what a run whose
/// post-execution action is still `Execute` concluded. Inverted, that turned the
/// setup run into an exit-11 `Retryable`, so the guard fired and the lane went
/// red saying "the premise is not established" about a run that is not its
/// subject. It never reached its claim. A lane that cannot fail for the reason it
/// exists is worth less than its name promises, whatever colour it shows.
///
/// So the marker is written into `<stub.root>/changes/cve.json` directly. Now the
/// only invocation in this lane is the one under test, and inverting the rule reds
/// the claim: `capability_executions` is empty, `next_action` is `complete`, and
/// the assessment read the marker as completion.
///
/// The particular flip that caught the lane out is gone — `assess` now calls that
/// predicate rather than spelling the condition out a second time, so inverting it
/// moves the marker's meaning as well and a `stub_mark` run over `cve` would exit
/// 0 again. The premise still does not go through the runtime, and that is the
/// durable half of the lesson: a premise established by running fiddle is hostage
/// to every rule on that run's path, and the rule under test is always one of
/// them.
///
/// Nothing is lost by not running M0's capability here, because what this lane
/// needs from it is the *value*, and the value is `blake3(project + NUL +
/// reference)` — [`Scenario::expected_marker`] computes it from design §4.3's own
/// definition rather than by asking fiddle. That the value is what a real
/// `stub_mark` run leaves behind is
/// `a_run_over_a_trackerless_reference_is_not_a_failed_run`'s assertion, made
/// through the binary over the same reference; this lane depends on that one for
/// reachability and on nothing else for its claim.
///
/// The world and the closing assertions are
/// `the_documented_invocation_with_no_capability_flag_reaches_the_sweep`'s,
/// deliberately: what differs between the two lanes is one fact about the change
/// set, so any difference in outcome is that fact's.
#[test]
fn a_marker_against_a_trackerless_reference_does_not_account_the_sweep_as_done() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());

    // The premise: the reference's change set carries a marker equal to this
    // invocation's own correlation key, filed by something that scanned no image.
    let foreign_marker = sweep.scenario.expected_marker(SWEEP_REF);
    sweep
        .scenario
        .write_change_marker(SWEEP_REF, &foreign_marker);
    assert_eq!(
        sweep.scenario.read_change_marker(SWEEP_REF),
        Some(foreign_marker),
        "the reference must really be marked, and readable the way the change \
         port reads it, or this lane asserts nothing"
    );

    // And the sweep still scans. Asserted before the exit code, so that a rule
    // which accounts this sweep as done fails *this* assertion and says so: the
    // outcome of such a run is `completed` and its exit code is 0, exactly as a
    // run that did the work — which is what made the defect so quiet.
    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "cve_mitigate",
        "a marker some other capability wrote must not account the sweep as \
         done: the assessment read `satisfied` from it, `derive_next` returned \
         `complete` before the capability was consulted, and the run reported \
         success having executed nothing: {payload}"
    );
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    // The scan really happened, read out of the world rather than off the
    // payload: a run that named the capability and did nothing would satisfy
    // the assertion above on its own.
    let pulls = sweep.pull_requests();
    assert_eq!(
        pulls.len(),
        1,
        "the sweep must reach the publication an unmarked reference reaches: \
         {pulls:?}"
    );
    let branch = the_one_new_branch(&sweep);
    assert_eq!(pulls[0]["head"]["ref"], branch, "{}", pulls[0]);
    let landed = pushed_file(&sweep, &branch, "go.mod");
    assert!(
        landed.contains(&format!("{MODULE} {FIXED_VERSION}")),
        "the branch must carry the requirement at the fixed release: {landed}"
    );
}

/// **A reference that is not `cve` still selects `stub_mark` with no flag.**
///
/// The converse of the lane above, and the guard on M0's invariant: the default
/// became *scheme-dependent*, not *`cve_mitigate` everywhere*. It must pass
/// before that change and after it, which is what makes it a guard rather than a
/// restatement of the new behaviour — and it is why M0's own acceptance lane
/// needs no edit.
///
/// A `beans` reference in the M0 world, because that is the pairing the
/// invariant is about: the same absent flag, a different scheme, the deterministic
/// capability. The marker is asserted beside the id for the reason its neighbours
/// are: a payload naming `stub_mark` is satisfied by a run that named it and did
/// nothing.
#[test]
fn a_reference_that_is_not_cve_still_selects_the_deterministic_capability() {
    let scenario = Scenario::new();
    scenario.write_work_item("fiddle-m0-demo", "open");

    let payload = scenario.run_json("beans:fiddle-m0-demo", 0);

    assert_eq!(
        payload["capability_executions"][0]["capability_id"], "stub_mark",
        "a scheme-derived default must move the default for `cve` alone: \
         {payload}"
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");
    assert_eq!(
        scenario.read_change_marker("fiddle-m0-demo").as_deref(),
        Some(scenario.expected_marker("beans:fiddle-m0-demo").as_str()),
        "and the deterministic capability writes the marker it always wrote: \
         {payload}"
    );
}

/// **An already-fixed fixture yields no pull request, and a no-change the
/// bundle files as `unsafe_without_direction`.**
///
/// # Why the name says the row rather than `already_fixed`, and why the row moved
///
/// Because the row is what this world reaches, and for most of M4a nothing could
/// say so. The lane was called
/// `an_already_fixed_fixture_produces_an_evidenced_no_change` and read as though
/// it were Design §3 row 3; it is not. The library advisory is settled by the tree
/// and contributes nothing, and the OS advisory is one this run cannot settle — a
/// base image's, with no registry to read tags from — so it leaves a verdict and
/// the run lands below row 3.
///
/// **Which row that is changed with M4c, and the change is the design rather than
/// a drift.** M4a refused the OS advisory in Rust, before any model was consulted,
/// so the run reached `verdicts_only`: a verdict with no attempt behind it. There
/// is no such refusal now. The advisory is shown to the attempt, the attempt
/// declines it, and *something was attempted* is exactly what row 5 tests — so
/// this world reaches `unsafe_without_direction`, and the lane is renamed for the
/// row it now reaches rather than left carrying the old one's name.
///
/// A consequence worth stating because nothing else in this file can: with the
/// pre-attempt refusal gone, **`verdicts_only` is no longer reachable at all**. A
/// verdict is now always a finding an attempt was shown, so a run with verdicts
/// has attempts, and row 5 shadows row 2 in every world. The row stays in
/// `disposition`'s table; nothing here reaches it.
///
/// Row 3 proper is [`a_tree_that_settles_every_finding_reaches_already_fixed`],
/// which reaches it over a document whose findings *can* all be settled. Naming
/// each lane for the row it actually reaches is the point of the exercise: a
/// lane named for one row and asserting another is a lane that would go green
/// through the very collapse it looks like it guards.
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
fn an_already_fixed_fixture_yields_a_no_change_the_bundle_files_as_needing_direction() {
    let sweep = Sweep::scanning(FIXED, SCAN_OK, 2, an_attempt_declining(&[OS_CVE]));

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
    sweep.assert_every_receipt_is_logical(&run);

    // 4. And which row of Design §3 all of that adds up to, said in the record
    //    rather than left for a reader to infer from three absences. The verdict
    //    is what puts this run on row 2, and the already-fixed set is still
    //    published beside it — so the bundle carries both halves of *the tree
    //    settled one advisory and could not settle the other*, which is what
    //    made this world worth a lane in the first place.
    let disposition = sweep.disposition(&run);
    assert_eq!(
        disposition,
        serde_json::json!({
            "reason": "unsafe_without_direction",
            "verdicts": 1,
            "already_fixed": [LIBRARY_CVE],
            "deferred": [],
            "attempts": [{
                "cves": [OS_CVE],
                "status": "needs_work",
                "claimed_complete": false,
                "forbidden": [],
            }],
            "branch": serde_json::Value::Null,
            "pull_request": serde_json::Value::Null,
        }),
        "the row, and the evidence for it, whole"
    );
}

/// The document a run over an image with nothing in it publishes.
///
/// # Why the three silent rows keep their expectations here rather than inline
///
/// Because they are used twice and the second use is the point. Each is asserted
/// **whole** against one real run in the lane below it, which is what binds it to
/// the product; and the three are then compared against each other, with their
/// own names removed, by
/// [`the_three_rows_that_used_to_publish_one_document_publish_three`] — which
/// needs all three in one place and must not pay for three more sweeps to get
/// them. A literal written twice would be two literals, and the comparison would
/// stop being about the documents the runs actually produced.
fn nothing_to_do_publishes() -> serde_json::Value {
    serde_json::json!({
        "reason": "nothing_to_do",
        "verdicts": 0,
        "already_fixed": [],
        "deferred": [],
        "attempts": [],
        "branch": serde_json::Value::Null,
        "pull_request": serde_json::Value::Null,
    })
}

/// The document a run whose findings the tree has already settled publishes.
///
/// See [`nothing_to_do_publishes`] for why these live here.
fn already_fixed_publishes() -> serde_json::Value {
    serde_json::json!({
        "reason": "already_fixed",
        "verdicts": 0,
        "already_fixed": [LIBRARY_CVE],
        "deferred": [],
        "attempts": [],
        "branch": serde_json::Value::Null,
        "pull_request": serde_json::Value::Null,
    })
}

/// The document a run whose work an open pull request already carries publishes.
///
/// See [`nothing_to_do_publishes`] for why these live here.
fn already_in_progress_publishes() -> serde_json::Value {
    serde_json::json!({
        "reason": "already_in_progress",
        "verdicts": 0,
        "already_fixed": [LIBRARY_CVE, OS_CVE],
        "deferred": [],
        "attempts": [],
        "branch": serde_json::Value::Null,
        "pull_request": SHARED_PR,
    })
}

/// The three assertions every one of the three silent rows owes, which are the
/// ones that used to make them indistinguishable.
///
/// Exit 0, `completed`, and an empty verdict report. Made by each of the three
/// lanes below rather than by a loop over the three worlds, because a lane that
/// asserts its own premise is a lane that fails on its own when the premise goes.
/// The three calls being identical *is* the claim that the old surface does not
/// tell these runs apart — there is nothing left for a loop to add.
fn the_old_surface_says_nothing(sweep: &Sweep, run: &Output, world: &str) {
    assert_eq!(
        run.status.code(),
        Some(0),
        "{world} is not a failed run — stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(sweep.payload(run)["outcome"], "completed", "{world}");
    assert_eq!(
        sweep.verdicts(),
        serde_json::json!([]),
        "{world} must leave the same empty verdict report as its neighbours, or \
         the three are being told apart by something other than the row"
    );
    // And the receipts a run publishes stay logical while the record grows.
    sweep.assert_every_receipt_is_logical(run);
}

/// The row a run's stdout and its bundle agree on, or a failure naming both.
///
/// The payload a shell caller reads and the bundle a reader opens a week later
/// must not be two documents. `run_json` projects one from the other, and this is
/// what holds that: a caller who had to open the bundle to learn which of seven
/// situations they are in would be worse off than one who read the record.
fn the_row_both_surfaces_agree_on(sweep: &Sweep, run: &Output, world: &str) -> serde_json::Value {
    let document = sweep.disposition(run);
    assert_eq!(
        sweep.payload(run)["disposition"],
        document,
        "{world}: stdout and the bundle disagree about the row"
    );
    document
}

/// **A scan reporting an image with nothing in it reaches `NothingToDo`.**
///
/// # The defect these three lanes are about
///
/// Design §3's last paragraph is one sentence: *every `NoChange` carries the
/// evidence for its own reason; one whose reason cannot be checked from the
/// bundle is not evidenced.* Until the bundle carried a `disposition`, three of
/// those rows failed it completely. A run that found nothing to do, a run whose
/// findings the tree had already fixed, and a run whose work an open pull request
/// already carries each published: exit 0, `"outcome": "completed"`,
/// `verdicts.json == []`, one receipt naming the verdict report — and nothing
/// else. Byte for byte the same artefacts, for three situations with three
/// different remedies, one of which is *go and merge #41*.
///
/// Run-level lanes and not unit ones, because the unit tier already had this
/// covered and it was not enough: `cve_dispositions::seven_causes_reach_seven_
/// distinguishable_results` proved the table injective *inside the process* for
/// the whole of this milestone, and every one of these three runs was still
/// unreadable from outside it. Injectivity in memory is not a distinction
/// anybody downstream can act on.
///
/// # And one lane per row, which is the second thing being proved
///
/// The three worlds shared a single lane until this one was split out of it, and
/// a shared lane cannot show what Design §8 asks for. *Every disposition must
/// fail under a mutation its neighbours survive* is a claim about
/// discrimination: corrupt one arm of `disposition` and exactly the lanes for
/// that arm's row must go red. With three rows in one lane, deleting row 3's test
/// and deleting row 1's fall-through fail the same single test, and the suite
/// cannot say the two rows are two things. Split, they can:
/// [`the_three_rows_that_used_to_publish_one_document_publish_three`] still makes
/// the cross-world claim, over the very expectations these lanes bind to runs, and
/// costs no run of its own.
///
/// # This world
///
/// The tree is [`FIXED`] in all three, so the tree is not what decides. Here the
/// difference is the document: a scan reporting an image with nothing in it. Every
/// list is empty and the reason is the whole of the claim — which is precisely
/// what the two rows below *can* say and this one cannot.
#[test]
fn a_scan_of_an_empty_image_reaches_nothing_to_do() {
    let sweep = Sweep::scanning(FIXED, SCAN_CLEAN, 2, a_script_no_attempt_consumes());

    let run = sweep.run();
    the_old_surface_says_nothing(&sweep, &run, "nothing to do");

    assert_eq!(
        the_row_both_surfaces_agree_on(&sweep, &run, "nothing to do"),
        nothing_to_do_publishes(),
        "the row that is *nothing*: every list empty, and the reason is the \
         whole of it"
    );
}

/// **A tree that settles every finding there is reaches `AlreadyFixed`.**
///
/// Design §3 row 3, and the row the milestone's own acceptance suite could not
/// reach for most of M4a. See [`a_scan_of_an_empty_image_reaches_nothing_to_do`]
/// for the defect all three of these lanes are about.
///
/// # Why the document is [`SCAN_LIBRARY_ONLY`] and not [`SCAN_OK`]
///
/// Because `AlreadyFixed` is **row 3**, below both row 4 and row 5, so a run that
/// attempted anything never reaches it. Under [`SCAN_OK`] the OS advisory names a
/// fix, so it is fixable, and no tree in this suite settles a base image's
/// package: it survives deduplication, the bound takes it, and the one attempt is
/// shown it. That run therefore lands on row 5, which is exactly what
/// `an_already_fixed_fixture_yields_a_no_change_the_bundle_files_as_needing_direction`
/// asserts.
///
/// So this world's document names *only* the library advisory, the one `go list
/// -m` can settle against the tree. Then nothing is left to attempt and nothing is
/// left to report, and row 3 is reachable. The advisory in `already_fixed` is what
/// row 1 cannot say and is the evidence for the reason: somebody else already
/// dealt with this.
///
/// Row 2 sits between this row and row 5 and is reached from neither of those two
/// worlds — see
/// [`an_advisory_with_no_published_fix_is_reported_without_being_attempted`],
/// which needs a document this one has no reason to write.
#[test]
fn a_tree_that_settles_every_finding_reaches_already_fixed() {
    let sweep = Sweep::scanning(FIXED, SCAN_LIBRARY_ONLY, 2, a_script_no_attempt_consumes());

    let run = sweep.run();
    the_old_surface_says_nothing(&sweep, &run, "already fixed in the tree");

    assert_eq!(
        the_row_both_surfaces_agree_on(&sweep, &run, "already fixed in the tree"),
        already_fixed_publishes(),
        "row 3 names the advisory somebody else already dealt with, which is \
         exactly what row 1 cannot say"
    );
}

/// **An open pull request that already covers the rest reaches
/// `AlreadyInProgress`, and the number travels.**
///
/// The row whose remedy is not *nothing*: it is *go and merge #41*. See
/// [`a_scan_of_an_empty_image_reaches_nothing_to_do`] for the defect all three of
/// these lanes are about — this is the row that made it worth fixing, because a
/// run reporting "nothing changed" when the work is sitting in an open pull
/// request is a run that loses the pull request.
///
/// # How this world differs from the one above
///
/// The same tree and the full [`SCAN_OK`] document, plus an open labelled pull
/// request whose **commit body** names the one advisory the tree cannot settle.
/// Read from the commits and never from the pull request's body: a body lists
/// what a scan found when it was opened, so a mention there is evidence a CVE was
/// seen and not that it was fixed.
///
/// **The commit is written by this test because no run could write it**, and that
/// is worth separating from a shortcut. The advisory it names is an OS one; a
/// base-image group is refused for want of a registry and is blocked before
/// either commit producer is reached, so `cve::dedup`'s `PackageType::Os` arm has
/// no producer in M4a at all. A lane about it has to seed. What that leaves
/// untested is nothing this build does — see
/// [`a_second_run_reads_the_first_runs_own_commit_body`] for the *library* half,
/// where the producer exists and a second run really does read the first run's
/// own body.
///
/// The library advisory is deliberately *not* named in that commit. This run must
/// reach the in-progress row through a real dedup of both halves — one from the
/// tree, one from the log — and a commit naming both would leave the tree's half
/// unexercised. Both halves are then visible in the published `already_fixed`,
/// which is why it is asserted whole rather than as a length.
///
/// # The two fields that are the row
///
/// `pull_request` carries the number, because the number *is* the remedy;
/// `branch` stays null, because nothing landed on a branch this run made and
/// naming one would tell a reader work landed there.
#[test]
fn an_open_pull_request_covering_the_rest_reaches_already_in_progress() {
    let sweep = Sweep::scanning(FIXED, SCAN_OK, 2, a_script_no_attempt_consumes());
    sweep.seed_shared_pull_request_saying(
        STALE_BODY,
        &format!("bump the base image, fixes {OS_CVE}"),
    );

    let run = sweep.run();
    the_old_surface_says_nothing(&sweep, &run, "an open pull request covers it");

    assert_eq!(
        the_row_both_surfaces_agree_on(&sweep, &run, "an open pull request covers it"),
        already_in_progress_publishes(),
        "row 7's remedy is to go and merge a numbered pull request, so the \
         number is the evidence — and nothing landed on a branch this run made, \
         so it names none"
    );
}

/// **The operator who did not ask for JSON is told the row too, and the number
/// with it.**
///
/// `run_json`'s header argues the row into the payload in so many words: a
/// document carrying `observations` and not the disposition "would make the two
/// documents disagree about what is knowable ... a caller at a shell would have to
/// open the bundle to learn which of seven situations they are in". `run_human`
/// then had no such line, so the caller at a shell — the one that sentence is
/// *about* — was the only caller it was not true for.
///
/// # Why this world and not a cheaper one
///
/// Row 7 is the row where the omission costs something an operator can act on.
/// The other two silent rows resolve to *nothing to do*, and a reader who missed
/// the line would reach the same next move anyway; here the next move is **go and
/// merge #41**, and a rendering without the row loses the number entirely. It is
/// also the row with a non-empty `already_fixed`, so the counts in the line are
/// checkable rather than four zeroes that any arm would produce.
///
/// # What is compared against what
///
/// Not a literal. The row is read out of the **bundle this same plain run
/// published**, reached the way an operator would reach it — the `report` line of
/// that same stdout — so the claim is that the two surfaces of one run agree,
/// which is the claim `the_row_both_surfaces_agree_on` makes for `--json`. A
/// rendering that named a plausible row from the wrong arm reds here.
#[test]
fn the_plain_rendering_names_the_row_a_run_reached_and_its_pull_request() {
    let sweep = Sweep::scanning(FIXED, SCAN_OK, 2, a_script_no_attempt_consumes());
    sweep.seed_shared_pull_request_saying(
        STALE_BODY,
        &format!("bump the base image, fixes {OS_CVE}"),
    );

    let run = sweep.run_plain();
    let stdout = String::from_utf8_lossy(&run.stdout).to_string();
    let stderr = String::from_utf8_lossy(&run.stderr).to_string();
    assert_eq!(run.status.code(), Some(0), "stderr: {stderr}");
    // The premise: this is the plain surface and not a payload that happens to
    // contain the words. A lane that drifted onto `--json` would prove nothing.
    assert!(
        stdout.starts_with("run "),
        "this must be the plain rendering, not a payload: {stdout}"
    );

    assert!(
        stdout.contains(
            "disposition = already_in_progress \
             (0 unfixed, 2 already fixed, 0 deferred, 0 attempted), \
             pull request #41"
        ),
        "an operator at a terminal must be told which of the seven rows this run \
         reached and that the remedy is to go and merge #{SHARED_PR}: {stdout}"
    );

    // And the row it named is the row the record keeps. The bundle is reached
    // through the plain rendering's own `report` line, so a stdout that named a
    // bundle it did not publish would fail here rather than pass quietly.
    let relative = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("report      = "))
        .unwrap_or_else(|| panic!("the plain rendering must name its bundle: {stdout}"));
    let bytes = std::fs::read(sweep.scenario.report_dir().join(relative)).unwrap();
    let bundle: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        bundle["disposition"],
        already_in_progress_publishes(),
        "stdout and the bundle must not be two documents"
    );

    // The row travels; the credential does not. This run exports the sentinel
    // like every other, and a disposition line is new prose on the surface most
    // likely to be pasted into a bug report.
    assert!(
        !stdout.contains(SENTINEL_SECRET),
        "a credential reached stdout: {stdout}"
    );
    assert!(
        !stderr.contains(SENTINEL_SECRET),
        "a credential reached a diagnostic: {stderr}"
    );
}

/// **The three rows that used to publish one document publish three, and still
/// three with their own names removed.**
///
/// # What this lane adds to the three above, and why it starts no process
///
/// Each of the three lanes above asserts one run's whole document against one of
/// [`nothing_to_do_publishes`], [`already_fixed_publishes`] and
/// [`already_in_progress_publishes`]. That is what binds those expectations to
/// the product: if a run stops publishing what its lane names, that lane goes
/// red. What no one of them can say is anything about its *neighbours* — three
/// lanes passing separately is consistent with three worlds publishing one
/// document, if the three expectations happened to be the same document.
///
/// So this is the cross-world half, made over the same three values the runs are
/// checked against rather than over three more sweeps. Composed with the lanes
/// above it is the original claim, and strictly stronger than the loop it
/// replaced: whole-document equality against a named expectation, rather than
/// three documents landing in a set of size three.
///
/// **And with `reason` removed, which is the assertion that matters.** A set keyed
/// on the whole published document would be satisfied by a record carrying nothing
/// but its own name, and a name with no evidence beside it is what Design §3's
/// sentence refuses. Each row must be checkable from its evidence: the empty
/// image's lists are all empty, the settled tree names the advisory it settled,
/// and the in-progress run carries a pull request number none of the others has.
#[test]
fn the_three_rows_that_used_to_publish_one_document_publish_three() {
    let rows = [
        nothing_to_do_publishes(),
        already_fixed_publishes(),
        already_in_progress_publishes(),
    ];

    let named: std::collections::HashSet<String> =
        rows.iter().map(serde_json::Value::to_string).collect();
    assert_eq!(
        named.len(),
        3,
        "two of the three silent rows publish one document: {rows:?}"
    );

    let anonymous: std::collections::HashSet<String> = rows
        .iter()
        .map(|row| {
            let mut row = row.clone();
            row.as_object_mut()
                .expect("a disposition is an object")
                .remove("reason")
                .expect("every row publishes its reason");
            row.to_string()
        })
        .collect();
    assert_eq!(
        anonymous.len(),
        3,
        "a row must be checkable from its evidence and not from its own name \
         alone, and two of these carry the same evidence: {rows:?}"
    );
}

/// **An advisory the scanner published no fix for is reported without being
/// attempted, and that run reaches `VerdictsOnly`.**
///
/// Design §3 row 2, and the row the census above spent a commit calling
/// unreachable. It is not: `verdicts_of` reads `Projection::upstream_blocked`
/// before it reads anything of the run's, and an advisory with no `fixedVersion`
/// is in that set and in no other. The bound is applied to `Projection::fixable`,
/// which never held it, so there is nothing for the one attempt to be shown and
/// `mitigate` makes none — `!attempted.is_empty()` is false, row 5 does not fire,
/// and the fall-through reaches row 2.
///
/// # Why this is the row worth reaching from out here
///
/// Because it is the one where a run has *something to say and nothing to do*,
/// and those are the two halves an operator has to be able to see separately. A
/// reader who is told only "nothing to do" concludes the image is clean; a reader
/// who is told only "there are verdicts" goes looking for the attempt that
/// produced them. Row 2 is the answer *the scanner knows about this and upstream
/// has shipped no fix*, and the remedy is neither merge nor retry — it is wait, or
/// go and read the advisory. No other row carries that.
///
/// # Why this world and not a cheaper one
///
/// [`SCAN_NO_FIX`] over the **vulnerable** tree, which is the honest arrangement:
/// the project really does depend on the version the document reports, and there
/// is nowhere to move it to. The tree is not what decides — nothing reads it,
/// because deduplication is only asked about fixable findings — and that is worth
/// stating rather than leaving to be inferred from `already_fixed` being empty.
///
/// The model is handed [`a_script_no_attempt_consumes`], and it is a *premise*
/// here rather than a convenience: the claim is that no attempt was made, so a
/// script the run could consume would make the empty `attempts` list ambiguous
/// between *nothing was attempted* and *an attempt happened to change nothing*.
/// [`SCAN_NO_FIX`]'s own doc gives the arithmetic that makes the world minimal.
///
/// # What is asserted, and in what order
///
/// The document first, because *reported without being attempted* is also what an
/// advisory the scanner never mentioned looks like — the same reason
/// [`a_deferred_finding_is_in_neither_the_verdict_set_nor_the_already_fixed_set`]
/// reads its scan artefact before its sets. Then the row, whole, as one object, so
/// the empty `attempts` list is read beside the non-zero verdict count rather than
/// checked on its own. Then the verdict report, because the row carries a verdict
/// *count* and a count of one is satisfied by a row for the wrong advisory — and
/// the judgement on that row, `upstream_blocked`, which is the one thing that says
/// the verdict came from the projection rather than from an attempt nobody can see.
/// Then the forge, which was never asked to open anything.
#[test]
fn an_advisory_with_no_published_fix_is_reported_without_being_attempted() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_NO_FIX, 2, a_script_no_attempt_consumes());

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "an advisory with no fix is not a failed run — stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // 0. The scanner really named the advisory, so everything below is about a
    //    document rather than about a fixture that lost a finding.
    let scanned = std::fs::read_to_string(sweep.scenario.report_dir().join("scan/scan.json"))
        .expect("the scanner left no artefact, so nothing below is about a document");
    assert!(
        scanned.contains(LIBRARY_CVE),
        "the scan does not name {LIBRARY_CVE}, so its verdict below would be a \
         row about nothing: {scanned}"
    );
    assert!(
        !scanned.contains("fixedVersion"),
        "the whole of this world is that no advisory in the document names a \
         fix — one that did would be fixable, would be attempted, and the run \
         would land on row 5: {scanned}"
    );

    // 1. The row, whole. `verdicts` is not zero and `attempts` is empty, and it
    //    is the pair that is row 2: either half alone is a row this run is not on.
    assert_eq!(
        the_row_both_surfaces_agree_on(&sweep, &run, "an advisory with no published fix"),
        serde_json::json!({
            "reason": "verdicts_only",
            "verdicts": 1,
            "already_fixed": [],
            "deferred": [],
            "attempts": [],
            "branch": serde_json::Value::Null,
            "pull_request": serde_json::Value::Null,
        }),
        "row 2 is *something to report and nothing attempted*: a verdict count \
         that is not zero beside an attempt list that is empty"
    );

    // 2. Whose verdict it is, and on whose authority. `upstream_blocked` is the
    //    judgement the projection produces and the only one reachable with no
    //    attempt behind it — `needs_work` here would mean the count above came
    //    from an attempt the `attempts` list is not showing.
    let verdicts = sweep.verdicts();
    assert!(
        sweep.has_verdict(LIBRARY_CVE),
        "the advisory the scanner reported has no row: {verdicts}"
    );
    assert_eq!(
        verdicts[0]["verdict"],
        serde_json::json!("upstream_blocked"),
        "the verdict has to come from the projection rather than from an \
         attempt, or the empty attempt list above is hiding one: {verdicts}"
    );

    // 3. And nothing was published, which is what makes the two nulls above
    //    readable rather than incidental: no branch was pushed and no pull
    //    request opened, because there was no work to put on one.
    assert!(
        sweep.pull_requests().is_empty(),
        "a run that attempted nothing opens nothing: {:?}",
        sweep.pull_requests()
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and it pushes nothing, so the remote still holds only its base branch"
    );
    sweep.assert_every_receipt_is_logical(&run);
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
/// `an_already_fixed_fixture_yields_a_no_change_the_bundle_files_as_needing_direction`
/// — the genuine
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
        a_script_no_attempt_consumes(),
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

    // And the row, published on the arm that reaches it. This one is reached by
    // *returning an error*, so a bundle that only recorded the disposition of a
    // successful execution would be missing exactly the row §3 calls the one
    // this milestone is most likely to get wrong — and a reader would be back to
    // telling it from a clean night by an exit code alone.
    let reached = sweep.disposition(&run);
    assert_eq!(
        reached,
        serde_json::json!({
            "reason": "scan_unusable",
            "verdicts": 0,
            "already_fixed": [],
            "deferred": [],
            "attempts": [],
            "branch": serde_json::Value::Null,
            "pull_request": serde_json::Value::Null,
        }),
        "every list is empty because nothing was observed, which is what makes \
         the reason the whole of the claim"
    );
    // The diagnostic is not in the record, and that is deliberate: it is already
    // the outcome's own text in the same bundle, and a second copy would be a
    // second place for one fact. So the record is checked against the outcome
    // beside it rather than trusted to repeat it.
    let bundle = sweep.bundle(&run);
    assert!(
        bundle["outcome"]["retryable"]["reason"]
            .as_str()
            .is_some_and(|why| why.contains("wizcli")),
        "the row's diagnostic lives on the outcome, and has to actually be \
         there: {bundle}"
    );
    // And the other direction of the pair `observations.tree` carries. That key
    // holds a scanned image's digest beside the revision that was remediated
    // (ADR 020), and its worth depends on the two never being published apart:
    // a digest with no revision beside it, or a revision with no digest, would
    // be half a correspondence that reads like a whole one. Here the scan
    // produced no document, so `sweep` was never entered, so there is no
    // revision — and the key has to be **absent entirely** rather than present
    // with an empty digest or a null revision in it.
    assert!(
        bundle["observations"].get("tree").is_none(),
        "a run with no scan document chose no revision and measured nothing, so \
         it must publish neither half of the pair: {bundle}"
    );
}

/// **A run whose one move could not be shown safe reverts it, publishes no pull
/// request, and says which of the two silent rows it is.**
///
/// # The row, and the pair it has to be told apart from
///
/// Design §3 row 5 against row 2: *a move was made, judged and taken back*
/// against *there was no move to make*. Both exit 0, both open nothing, both
/// write verdicts, and until the bundle carried a disposition the only thing
/// separating them anywhere a reader could see was the **prose of a rationale
/// string** — which is the wording of whichever upstream value decided it and is
/// free to change without any of this changing. Only one of the two is something
/// a person can give direction about, and a nightly job that could not tell them
/// apart would file the same ticket for both.
///
/// # Why the rescan is the lever
///
/// Because it is the only one that is honestly outside the process. The tree and
/// the checks are the product's; the *scanner's second answer* is a seam an
/// operator really does control, and answering the rescan with the input scan's
/// own document is what a moved feed or an unreported array looks like from
/// here. `evaluate` then reaches [`RescanVerdict::NotCompared`]'s neighbour —
/// the group's advisory is still in the report — so the group is
/// `NeedsWork::Unproved`, which is precisely *a repair that may well be fine and
/// cannot be shown to be*. Nothing here forces a failure by breaking something.
///
/// # What the record has to carry, beyond the row
///
/// The attempt. Row 5 differs from row 2 by *something was attempted*, so an
/// empty `attempts` list beside `"unsafe_without_direction"` would be a name
/// with no evidence under it — and `claimed_complete` is in that list because
/// Design §2.5 says it is evidence beside the exit code that overruled it. This
/// run is where the two are visibly in disagreement: the model said it had
/// finished, and the rescan did not agree.
#[test]
fn an_unprovable_repair_is_reverted_and_filed_as_needing_direction() {
    // The rescan is answered with the *input* scan's own document: the library
    // advisory is still in it, so the repair proves nothing.
    let sweep = Sweep::scanning_rescanning(
        VULNERABLE,
        SCAN_OK,
        SCAN_OK,
        2,
        // Both advisories, in one attempt: the bound leaves nothing deferred,
        // so the attempt moves the requirement it can and declines the base
        // image it cannot. What makes this world *unprovable* is the rescan —
        // the input scan's own document, which clears nothing — so both
        // advisories are still there afterwards and both get a row.
        an_attempt(
            &[
                ("go.mod", vulnerable_manifest()),
                ("go.sum", vulnerable_sums()),
            ],
            &[LIBRARY_CVE],
            &[OS_CVE],
        ),
    );

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "a group left for a person is not a failed run — stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // Nothing landed. Asserted against the world rather than the report: a
    // reverted attempt that pushed anyway would satisfy every assertion about a
    // document and none of these.
    assert!(
        sweep.pull_requests().is_empty(),
        "an unproved repair opens nothing: {:?}",
        sweep.pull_requests()
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and leaves the remote exactly as it found it"
    );

    // The row, and the attempt that is the difference between it and row 2.
    let reached = sweep.disposition(&run);
    assert_eq!(reached["reason"], "unsafe_without_direction", "{reached}");
    assert_eq!(
        reached["attempts"],
        serde_json::json!([{
            "cves": [LIBRARY_CVE, OS_CVE],
            "status": "needs_work",
            "claimed_complete": false,
            "forbidden": [],
        }]),
        "the row's evidence is that something was attempted, and the model's own \
         claim beside the judgement that overruled it — one row for the one \
         attempt, naming every advisory it was shown: {reached}"
    );
    assert!(
        reached["branch"].is_null() && reached["pull_request"].is_null(),
        "nothing landed, so there is nothing to point at: {reached}"
    );
    sweep.assert_every_receipt_is_logical(&run);

    // And the verdict the operator actually reads, which is the *other* half —
    // both advisories are unfixed, for two different reasons, in the document
    // the workflow's Jira step parses.
    let verdicts = sweep.verdicts();
    assert!(
        sweep.has_verdict(LIBRARY_CVE) && sweep.has_verdict(OS_CVE),
        "both advisories are still unfixed and both get a row: {verdicts}"
    );
}

/// **A check that says no makes the attempt needs-work, the commit is taken back,
/// and nothing is published.**
///
/// The other half of `Evaluation::accepted`, which is *every check passed **and**
/// the rescan cleared*. [`an_unprovable_repair_is_reverted_and_filed_as_needing_direction`]
/// varies the rescan; this varies the check, and the two are reached from opposite
/// ends of the same world — the scanner's second answer against a command's exit
/// status. Here the rescan is [`RESCAN_CLEAN`] and really does clear the advisory,
/// so the **only** thing refusing this repair is the scripted check, which is what
/// makes the lane about the check rather than about a judgement in general.
///
/// # What this lane is, and what it replaces
///
/// It is what survives of `a_needs_work_groups_rescan_is_not_folded_on`, which
/// held this claim and a second one M4c deletes: that a needs-work *group*'s
/// rescan is not folded on by the group after it. There are no groups now, so
/// there is no group after — but *a check that says no reverts the work* was never
/// a claim about grouping, and deleting the lane whole would have taken it with
/// the part that had to go. The world is the same one; the fold half of the
/// assertions is gone and the bound is one, so the base-image advisory is deferred
/// and the attempt is judged on the library advisory alone.
#[test]
fn a_check_that_says_no_reverts_the_attempt_and_publishes_nothing() {
    let sweep = Sweep::scanning_with_a_failing_check(
        VULNERABLE,
        SCAN_OK,
        1,
        a_repair_moving_the_requirement(),
    );

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "an attempt left for a person is not a failed run — stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );

    // Nothing landed, read off the world rather than off a report.
    assert!(
        sweep.pull_requests().is_empty(),
        "a repair a check refused opens nothing: {:?}",
        sweep.pull_requests()
    );
    assert_eq!(
        sweep.remote_branches(),
        vec![SWEEP_BASE.to_string()],
        "and leaves the remote exactly as it found it"
    );

    // The row, and the attempt under it.
    let reached = sweep.disposition(&run);
    assert_eq!(reached["reason"], "unsafe_without_direction", "{reached}");
    assert_eq!(
        reached["attempts"],
        serde_json::json!([{
            "cves": [LIBRARY_CVE],
            "status": "needs_work",
            "claimed_complete": true,
            "forbidden": [],
        }]),
        "one attempt, refused — and `claimed_complete` is the model's own claim \
         beside the check that overruled it: {reached}"
    );
    assert!(
        reached["branch"].is_null() && reached["pull_request"].is_null(),
        "nothing landed, so there is nothing to point at: {reached}"
    );

    // And the advisory is reported unfixed, which is the half an operator reads.
    let verdicts = sweep.verdicts();
    assert!(
        sweep.has_verdict(LIBRARY_CVE),
        "the advisory the attempt was shown is still unfixed and gets a row: \
         {verdicts}"
    );
    sweep.assert_every_receipt_is_logical(&run);
}

/// **Every finding this run selected is one attempt, one commit and one pull
/// request — however many files the fix spans.**
///
/// The claim M4c's §2 is: *one bounded attempt, every selected finding, one
/// worktree*. Until this lane the sweep grouped its findings by the bump target
/// four mechanical Go rules elected and ran one attempt per group, so this world
/// — two library advisories in two modules — produced **two** attempts and two
/// commits on the branch. Grouping is what had to go: it cannot be computed
/// without knowing which file fixes a finding, which is exactly the judgement
/// this milestone hands to the agent.
///
/// # Why the count is the assertion, and what is asserted beside it
///
/// One commit is the observable difference between one attempt and several, and
/// it is the only one that cannot be faked by a run that did less work: a
/// grouping build reds here with two, and a build that attempted nothing at all
/// reds on the pull request and on the manifest below.
///
/// So four readings, each ruling out a different way of being green:
///
/// 1. **One commit**, read with `rev-list` out of the bare repository.
/// 2. **It carries both files the attempt declared**, which is the *different
///    files* half of the claim: the diff a single attempt lands is the whole of
///    what it changed, and a build that still committed per group would put the
///    manifest in one commit and the sums in another.
/// 3. **It names both advisories**, because the body is what the next run's log
///    scan reads and a commit naming one of two leaves the other to be
///    re-proposed against a tree that already carries its fix.
/// 4. **Both requirements really moved**, read off the remote. Without this the
///    three above are satisfied by one attempt that fixed one advisory and
///    claimed both.
///
/// # The advisory that is not in it
///
/// [`SCAN_TWO_LIBRARIES`] also reports the OS advisory every input document in
/// this file carries, and the bound of two leaves it deferred rather than
/// attempted — which is why the rescan may be [`RESCAN_CLEAN`]: its OS array
/// holds an advisory that is in the input scan's baseline, so condition (b) reads
/// it as nothing *new*. A world that took all three would need a rescan clearing
/// all three, which is
/// [`a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch`]'s
/// world and not this one's.
#[test]
fn two_findings_in_different_files_are_one_attempt_and_one_commit() {
    // Two, so both library advisories are taken and the OS one is deferred. The
    // script's one attempt is what this lane is about, and a second answer is
    // waiting for it: a build that still formed two groups gets a complete
    // second attempt and fails on the count below rather than by starving at the
    // socket, which would be evidence about the fixture instead.
    let sweep = Sweep::scanning(
        TWO_LIBRARIES,
        SCAN_TWO_LIBRARIES,
        2,
        [
            an_attempt(
                &[
                    ("go.mod", two_libraries_manifest()),
                    ("go.sum", two_libraries_sums()),
                ],
                &[LIBRARY_CVE, SECOND_LIBRARY_CVE],
                &[],
            ),
            an_attempt(&[], &[], &[LIBRARY_CVE, SECOND_LIBRARY_CVE]),
        ]
        .into_iter()
        .flatten()
        .collect(),
    );

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    let branch = the_one_new_branch(&sweep);
    let commits = pushed_commits(&sweep, &branch);
    assert_eq!(
        commits.len(),
        1,
        "every selected finding is one attempt, so the branch carries one \
         commit: {commits:?}"
    );
    let (body, paths) = &commits[0];

    // 2. Both files the attempt changed, in one commit.
    assert_eq!(
        paths,
        &["go.mod".to_string(), "go.sum".to_string()],
        "one attempt lands its whole diff, however many files it spans: {body}"
    );

    // 3. Both advisories in its body.
    for cve in [LIBRARY_CVE, SECOND_LIBRARY_CVE] {
        assert!(
            body.contains(cve),
            "a commit body naming one of two advisories leaves the other for the \
             next run to re-propose: {body}"
        );
    }

    // 4. And both requirements really moved.
    let landed = pushed_file(&sweep, &branch, "go.mod");
    for (module, fixed) in [
        (MODULE, FIXED_VERSION),
        (SECOND_MODULE, SECOND_FIXED_VERSION),
    ] {
        assert!(
            landed.contains(&format!("{module} {fixed}")),
            "the branch must carry {module} at {fixed}: {landed}"
        );
    }

    // One pull request, and one attempt in the record.
    let pulls = sweep.pull_requests();
    assert_eq!(pulls.len(), 1, "exactly one pull request: {pulls:?}");
    let reached = sweep.disposition(&run);
    assert_eq!(
        reached["attempts"],
        serde_json::json!([{
            "cves": [LIBRARY_CVE, SECOND_LIBRARY_CVE],
            "status": "clean",
            "claimed_complete": true,
            "forbidden": [],
        }]),
        "one row, naming every finding it covered: {reached}"
    );
    assert_eq!(
        sweep.gateway.served(),
        3,
        "two edits and a report is one attempt's whole turn, and a second \
         answer was waiting for a build that made a second attempt"
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
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());

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
    let bounded = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());

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

    // **And the deferral is on the record, with the bound that caused it.** The
    // two assertions above are absences — no verdict, nothing in its place — and
    // an absence is what a finding nobody scanned looks like too. Design §2.5's
    // whole distinction is that *this run stopped at one* is a different fact
    // from *fiddle looked at it and declined*, and until the record carried the
    // bound there was nowhere for the first of those to be said.
    let reached = bounded.disposition(&run);
    assert_eq!(
        reached["deferred"],
        serde_json::json!([{ "cve": OS_CVE, "bound": 1 }]),
        "the advisory over the bound, and the number that put it over: {reached}"
    );
    assert_eq!(
        reached["reason"], "pull_request",
        "deferring a finding does not change what the run came to: {reached}"
    );
}

/// **The grades the document named are the grades the run acted on.**
///
/// The sibling of the lane above, for the other key of the PRD's two-key
/// `[orchestration.cve]` example — and the one that survived a pass longer.
/// `severities = ["HIGH", "CRITICAL"]` was in the product document while this
/// table's `deny_unknown_fields` admitted three other names, so a deployment that
/// copied the manual exited 2; and the rule it would have set was a match arm in
/// `fiddle_core::selected`, so a deployment that wanted `MEDIUM` had nowhere to
/// write it. `config_check`'s lanes prove the key parses, defaults and
/// round-trips. Nothing proved it reached selection, and this is that.
///
/// The two runs differ in **one line of the document** and in nothing else — the
/// same scanner arm, the same tree, the same script, the same bound:
///
/// - naming **no grades** means `HIGH` and `CRITICAL`, so the document's one
///   `MEDIUM` finding is not one this deployment acts on, the projection is empty,
///   and the run has *nothing to do*;
/// - naming **`MEDIUM`** as well makes the same bytes the group `SCAN_OK` produces
///   at `HIGH`: one bump, one branch, one pull request carrying `go.mod` at the
///   fixed release.
///
/// A key that was read and thrown away leaves both runs on the first row. A
/// projection that ignored the set and admitted everything leaves both on the
/// second. The mitigation is read off the remote rather than off a report, because
/// what the key has to change is which findings a run *acts on*, not which ones it
/// mentions.
///
/// The finding carries **no public exploit**, and that is what makes the pair
/// about this key: the second selection arm is not configurable and would admit a
/// `MEDIUM` finding with a published fix whichever grades the document named, so a
/// fixture reporting `hasExploit: true` would produce the pull request in both
/// runs and prove nothing.
#[test]
fn the_grades_the_document_named_are_the_grades_the_run_acted_on() {
    // The rescan arm is [`SCAN_CLEAN`] rather than the usual [`RESCAN_CLEAN`]
    // because this world's input scan reports no OS advisory: a rescan carrying
    // one would be reporting a finding the input scan never had, which condition
    // (b) reads as a vulnerability that just appeared — and the repair would be
    // refused for a reason that has nothing to do with grades.
    let by_default = Sweep::scanning_rescanning(
        VULNERABLE,
        SCAN_MEDIUM_LIBRARY,
        SCAN_CLEAN,
        2,
        a_script_no_attempt_consumes(),
    );
    let run = by_default.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "a document with nothing this deployment acts on is not a failed run —          stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        by_default.disposition(&run),
        nothing_to_do_publishes(),
        "a MEDIUM finding is outside what a document naming no grades means, so          this run had nothing to act on"
    );
    assert_eq!(
        by_default.pull_requests().len(),
        0,
        "and it proposed nothing: {:?}",
        by_default.pull_requests()
    );

    // The same world with `severities` naming MEDIUM, which is the only
    // difference between the two documents.
    let widened = Sweep::scanning_grades(
        VULNERABLE,
        SCAN_MEDIUM_LIBRARY,
        SCAN_CLEAN,
        GRADES_INCLUDING_MEDIUM,
        2,
        a_repair_moving_the_requirement(),
    );
    let run = widened.run();
    let payload = widened.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        widened.disposition(&run)["reason"],
        "pull_request",
        "the deployment named MEDIUM, so the MEDIUM finding is work: {}",
        widened.disposition(&run)
    );
    assert_eq!(
        widened.pull_requests().len(),
        1,
        "exactly one pull request: {:?}",
        widened.pull_requests()
    );
    let branch = the_one_new_branch(&widened);
    let landed = pushed_file(&widened, &branch, "go.mod");
    assert!(
        landed.contains(&format!("{MODULE} {FIXED_VERSION}")),
        "and the branch carries the requirement at the fixed release, which is          the whole of what acting on a finding means here: {landed}"
    );
    assert!(
        !landed.contains(VULNERABLE_VERSION),
        "and no longer at the vulnerable one: {landed}"
    );
}

/// **A deferred finding is filed as deferred, and is in neither of the two sets
/// it could be mistaken for.**
///
/// # What the lane above cannot say
///
/// It shows a deferred advisory carrying its bound and producing no verdict, and
/// that is two thirds of Design §2.5's distinction. The third is *already
/// fixed*, and the world above cannot make the claim: its tree is the vulnerable
/// fixture, nothing in it was settled before the run started, so `already_fixed`
/// is `[]` and *the deferred advisory is not in it* is a sentence about an empty
/// list. It would hold of an advisory nobody deferred, of an advisory nobody
/// scanned, and of a run that never read the tree at all.
///
/// The three sets are not interchangeable and the remedies differ. **Already
/// fixed** means the tree already carries the patch and there is nothing to do.
/// **A verdict** means fiddle looked and will not move it, and the row says why.
/// **Deferred** means fiddle did not look, and the next run will. A record that
/// filed a deferral as either of the others would tell an operator the work was
/// finished or that it had been refused, when in fact it is still queued — so
/// the claim worth asserting is that the three are *populated and disjoint in
/// one run*, not that two of them happen to be empty.
///
/// # The world, and why it is the smallest one that says it
///
/// The already-fixed tree, [`SCAN_TWO_OS`], and a bound of one:
///
/// - the library advisory is the one `cve-fixed`'s `go.mod` already carries the
///   release for, so deduplication settles it before any group is formed —
///   `already_fixed`, exactly as
///   `an_already_fixed_fixture_yields_a_no_change_the_bundle_files_as_needing_direction`
///   reaches it;
/// - that leaves two findings open, both against the base layer. The bound takes
///   the first, and it is shown to the attempt, which declines it — nothing in
///   this build refuses a finding before an attempt sees it any more — and a
///   declined advisory is still there at the rescan, so it is a **verdict**;
/// - and the second is past the bound, so it is **deferred**, never judged.
///
/// The row the run lands on is therefore `unsafe_without_direction` rather than
/// `verdicts_only`: something *was* attempted. Before M4c the base-image advisory
/// was refused by four mechanical Go rules before any model was consulted, so a
/// verdict could exist with no attempt behind it; with the refusal deleted, every
/// verdict is a finding an attempt was shown. Which row this run reaches is not
/// what the lane is about — the three sets are — but it is asserted in the same
/// object literal, so a change to either is a change to a value a reader can see
/// whole.
///
/// Three sets, three different advisories, one run. `SCAN_OK` cannot produce it:
/// over this tree it leaves exactly one finding open, so a bound low enough to
/// defer anything defers the only thing there was to judge and the verdict set
/// empties — which is the vacuity this lane exists to avoid, arrived at from the
/// other side.
///
/// # Why the scan artefact is read first
///
/// Because *absent from two sets* is also what an advisory the scanner never
/// reported looks like. The document has to be shown to name all three
/// advisories before their distribution across the three sets is evidence about
/// a budget rather than about a fixture that lost a finding.
#[test]
fn a_deferred_finding_is_in_neither_the_verdict_set_nor_the_already_fixed_set() {
    let sweep = Sweep::scanning(FIXED, SCAN_TWO_OS, 1, an_attempt_declining(&[OS_CVE]));

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    // 0. The document really named all three, so the sets below are a
    //    distribution of findings that existed rather than of findings nobody
    //    reported.
    let scanned = std::fs::read_to_string(sweep.scenario.report_dir().join("scan/scan.json"))
        .expect("the scanner left no artefact, so nothing below is about a document");
    for cve in [LIBRARY_CVE, OS_CVE, SECOND_OS_CVE] {
        assert!(
            scanned.contains(cve),
            "the scan does not name {cve}, so its absence from a set below is \
             the scanner's silence and not this run's decision: {scanned}"
        );
    }

    // 1. Where each of the three ended up, as one value, so no set can be
    //    checked while another is quietly empty.
    let reached = sweep.disposition(&run);
    assert_eq!(
        reached,
        serde_json::json!({
            "reason": "unsafe_without_direction",
            "verdicts": 1,
            "already_fixed": [LIBRARY_CVE],
            "deferred": [{ "cve": SECOND_OS_CVE, "bound": 1 }],
            "attempts": [{
                "cves": [OS_CVE],
                "status": "needs_work",
                "claimed_complete": false,
                "forbidden": [],
            }],
            "branch": serde_json::Value::Null,
            "pull_request": serde_json::Value::Null,
        }),
        "three findings, three sets, and which advisory is in which"
    );

    // 2. And the verdict report itself, because the disposition carries the
    //    verdict *count* and a count of one is satisfied by a row for the wrong
    //    advisory. This is where the deferred finding is shown to be absent from
    //    a set that has something in it.
    let verdicts = sweep.verdicts();
    assert!(
        sweep.has_verdict(OS_CVE),
        "the finding inside the bound was judged, so it has a row: {verdicts}"
    );
    assert!(
        !sweep.has_verdict(SECOND_OS_CVE),
        "the finding past the bound was never judged, and a row for it would be \
         this build claiming an opinion it does not have — beside a report that \
         does hold a row, so this is not an empty report passing: {verdicts}"
    );
    assert!(
        !sweep.has_verdict(LIBRARY_CVE),
        "and the advisory the tree had already dealt with is not unfixed \
         either: {verdicts}"
    );

    // 3. Nothing was published, which is what makes the two nulls above readable
    //    rather than incidental: the attempt declined, so no group was clean, so
    //    there was nothing to put on a branch.
    assert!(
        sweep.pull_requests().is_empty(),
        "a run whose only attempt declined opens nothing: {:?}",
        sweep.pull_requests()
    );
    sweep.assert_every_receipt_is_logical(&run);
}

// ---------------------------------------------------------------------------
// The second night: a pull request that is already open (bean `fiddle-1muu`)
// ---------------------------------------------------------------------------
//
// Every lane above starts with an empty forge, so every one of them takes the
// *fresh* arm: nothing is open, a dated branch is cut, a pull request is created
// carrying this run's description, and `Checkout` records
// `attempt_tree: base_revision` with a null `pr_head`. That is one half of the
// shared-pull-request model and it was the only half anything ran.
//
// The other half is the one Design §7 calls the subtlest thing in the milestone.
// On the second night the pull request is already there, so:
//
//   * `EnsurePullRequest`'s postcondition holds — it matches on head, base and
//     `state=open`, and **deliberately not on the body** — so no create is
//     dispatched and nothing about the description follows from it;
//   * an `effect_id` is derived from `(project, invocation_ref, kind, target)`
//     and never from the payload, so an effect keyed on the pull request alone
//     would give last night's sentence and tonight's one identity, find the
//     postcondition satisfied, and rewrite nothing without saying so.
//
// `EnsurePullRequestBody` answers both: its target carries a digest of the body,
// which makes two sentences two effects, and its postcondition is a read of what
// the pull request currently says, which makes an unchanged sentence idempotent.
// The two lanes below are those two answers driven through the compiled binary —
// `publish_shared_work` dispatching it is the thing that had no caller, and a
// unit proof of the operation cannot show a caller exists.
//
// One seeded world reaches both, and that is why the seeding is shared. A run
// that finds an open labelled pull request is also the only run that reaches
// `Checkout::AtPullRequestHead`, so the same arrangement is what first drives
// `attempt_tree: pr_head` at the run level.

/// The branch an earlier night left behind, and the pull request open on it.
///
/// Under `security/` because that is the only namespace this capability may
/// push to — a head outside it is refused before any commit, which is its own
/// lane's business and not this one's — and dated, because that is what
/// `BRANCH_STEM` produces and this branch is standing in for one an earlier run
/// cut. The date is deliberately **not** today's: a reuse settles on the branch
/// the forge names, and a fixture dated today would let a run that ignored the
/// pull request and cut its own branch land on the same name.
const SHARED_BRANCH: &str = "security/cve-remediation-2026-01-02";

/// Its number, named rather than positional so the assertions below can say
/// which object they are about. See `gh_stub::pull_requests` on why a named
/// number is what makes an arranged world arrangeable.
const SHARED_PR: u64 = 41;

/// A file only the shared branch carries.
///
/// The second, independent witness that the attempt ran in the pull request's
/// tree: `pr_head` and `base_revision` are two shas, and a lane comparing shas
/// alone would still pass if the branch had been seeded at the base. A commit
/// the base has never seen cannot be reached from `origin/main`, so the pushed
/// branch carrying this file is a fact about *which tree the work was built on*
/// rather than about a string.
///
/// Markdown, and not a `.go` file: the fixture is a Go module, and a source file
/// the pair does not have would make the tree the sweep bumps differ from the
/// one `the_two_fixtures_differ_only_in_the_dependency_under_remediation` is
/// about.
const PRIOR_RUN_MARKER: &str = "EARLIER-RUN.md";

/// What the seeded pull request says before tonight's run touches it.
///
/// Recognisable prose rather than a nonsense string, because the failure this
/// suite exists to catch is a body that *stays* describing an earlier run — and
/// an assertion that the body is no longer this is only half of the claim. The
/// other half is [`RUN_BODY`].
const STALE_BODY: &str = "fiddle attempted 1 advisory for this repository's \
     container image in one bounded attempt and committed nothing.\n\nEvery \
     advisory this run did not fix is in the verdict report published beside this \
     run's bundle, with the sentence that decided it.";

/// What a sweep of the vulnerable fixture publishes: `cve::shared_body` over
/// `mitigate::summary_of`'s two paragraphs, with no anomaly note, because one
/// open labelled pull request is not an anomaly.
///
/// Spelled here and not imported, for [`SENTINEL_SECRET`]'s reason — this package
/// depends on neither library crate. That makes it a *pinned* sentence rather
/// than a derived one, which is what the idempotence lane needs: a body seeded
/// from the runtime's own function would be idempotent by construction and would
/// prove nothing about what a run writes.
///
/// `1 advisory` is this world's arithmetic and not a round number: the scan
/// reports two advisories and the bound of one takes the library one, so the
/// attempt is shown exactly one — and *committed what it changed* rather than a
/// count, because a run has one attempt now and *1 of 1* would be a sentence that
/// never varied. What varies is whether the tree it left is on the branch, which
/// is the difference between this constant and [`STALE_BODY`].
const RUN_BODY: &str = "fiddle attempted 1 advisory for this repository's \
     container image in one bounded attempt and committed what it changed.\n\nEvery \
     advisory this run did not fix is in the verdict report published beside this \
     run's bundle, with the sentence that decided it.";

impl Sweep {
    /// Put an open, labelled pull request in front of this deployment, on a
    /// branch of its own carrying a commit the base does not have, and answer the
    /// sha the forge will report for its head.
    ///
    /// **Arranged through the fixture's own files and a real `git`, never by
    /// driving the code under test.** The branch is a real ref in the bare
    /// repository — the scripted `gh` reads a head sha out of it rather than
    /// inventing one, so a world seeded any other way would report a tip the
    /// remote does not hold, and the checkout that resolves `<sha>^{commit}`
    /// would fail on a fixture defect rather than on the property under test.
    ///
    /// The commit is made in the clone and pushed, then the clone is put back on
    /// the base: what a run is pointed at is a repository sitting on `main`, and
    /// leaving it checked out on the shared branch would hand the run its answer.
    /// The local `security/…` branch that remains behind is deliberate in the
    /// other direction — `Approved::from` names `origin/<branch>` precisely so a
    /// stale local branch of the same name cannot be picked up, and a world
    /// without one could not tell the two apart.
    ///
    /// # Why it is seeded, and it is **brevity**
    ///
    /// A run really can leave this behind: the fresh arm cuts a dated branch,
    /// commits, pushes and opens a labelled pull request, which is exactly the
    /// arrangement this function fakes.
    /// [`a_second_run_reads_the_first_runs_own_commit_body`] is the lane that
    /// pays for it, and it costs a second whole sweep.
    ///
    /// Its two callers are about the pull request's **body** and about which tree
    /// the work was built in, and neither reads the commit log at all — the
    /// message here names no advisory, so `covers` is empty in both and their
    /// rows do not depend on it. Seeding is the cheaper way to put an open pull
    /// request in front of a run when what the run does with it is the subject.
    /// It is *not* the cheaper way when the question is what the log says, which
    /// is the distinction [`seed_shared_pull_request_saying`] draws next.
    ///
    /// [`seed_shared_pull_request_saying`]: Sweep::seed_shared_pull_request_saying
    fn seed_shared_pull_request(&self, body: &str) -> String {
        self.seed_shared_pull_request_saying(body, "an earlier night's work on the shared branch")
    }

    /// The same, with the shared branch's **commit message** chosen by the
    /// caller.
    ///
    /// A second entry point rather than a second seeder, because the commit
    /// message is the one part of that arrangement a lane has a reason to vary
    /// and the rest of it is delicate: dedup reads which advisories a branch
    /// already covers out of `origin/<base>..HEAD`'s commit *bodies* and never
    /// out of the pull request's body — `crate::cve::dedup`'s 2026-08-12
    /// incident settled that — so a lane about a pull request that already
    /// covers this run's work has to say so in a commit and nowhere else.
    ///
    /// # Why *its* seeding is not brevity: nothing this build runs writes that
    /// body
    ///
    /// Its one caller,
    /// [`an_open_pull_request_covering_the_rest_reaches_already_in_progress`],
    /// plants a body naming [`OS_CVE`], and **no M4a run can produce one.**
    /// Selecting a base-image tag needs a registry this build does not read, so
    /// `CveMitigate::target_version` refuses every base-image group and a refused
    /// group is blocked before either commit producer is reached —
    /// `cve::dedup`'s module header carries the whole argument. A lane about the
    /// OS half of the already-fixed set therefore *has* to write the body itself,
    /// and that is a scoped absence rather than an untested round trip.
    ///
    /// The library half is the opposite case and is now driven end to end. See
    /// [`a_second_run_reads_the_first_runs_own_commit_body`], which starts the
    /// binary twice and seeds nothing between the runs, so what its second run
    /// reads is a body its first run wrote.
    fn seed_shared_pull_request_saying(&self, body: &str, commit: &str) -> String {
        git(&self.tree, &["checkout", "-q", "-b", SHARED_BRANCH]);
        std::fs::write(
            self.tree.join(PRIOR_RUN_MARKER),
            "An earlier run's notes, which only this branch carries.\n",
        )
        .unwrap();
        git(&self.tree, &["add", "-A"]);
        git(
            &self.tree,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-qm",
                commit,
            ],
        );
        git(&self.tree, &["push", "-q", "origin", SHARED_BRANCH]);
        let head = git_says(&self.tree, &["rev-parse", "HEAD"]);
        git(&self.tree, &["checkout", "-q", SWEEP_BASE]);

        // And the forge's record of it. The head is `owner:branch`, which is
        // GitHub's own spelling and the one `EnsurePullRequest`'s postcondition
        // is matched on; the owner is the repository's, because that is what
        // `head_owner` is derived from.
        let owner = SWEEP_REPO.split('/').next().unwrap();
        std::fs::write(
            self.stub.join("pulls_seed"),
            serde_json::json!([{
                "number": SHARED_PR,
                "state": "open",
                "head": format!("{owner}:{SHARED_BRANCH}"),
                "base": SWEEP_BASE,
                "title": "acme: dependency advisories",
                "body": body,
                "labels": ["security/cve"],
            }])
            .to_string(),
        )
        .unwrap();
        head
    }

    /// One pull request, read back by number through the scripted `gh`.
    ///
    /// By number and not out of the listing, because the body is what these lanes
    /// are about and the two routes answer it differently *on purpose*: the
    /// listing describes the seed, and this route describes the seed with the
    /// mutations that really landed replayed over it — see
    /// `gh_stub::landed_transitions_applied`. A rewrite is observable only where
    /// the world replays it, and `GET /repos/{o}/{r}/pulls/{n}` is also the exact
    /// route `EnsurePullRequestBody`'s own postcondition read addresses.
    fn pull_request(&self, number: u64) -> serde_json::Value {
        let out = Command::new(gh_stub_binary())
            .args(["--stub-dir", self.stub.to_str().unwrap()])
            .args([
                "api",
                "--method",
                "GET",
                &format!("/repos/{SWEEP_REPO}/pulls/{number}"),
            ])
            .output()
            .unwrap();
        support::object_of(&String::from_utf8_lossy(&out.stdout))
            .unwrap_or_else(|| panic!("the forge holds no pull request #{number}"))
    }

    /// Every forge mutation that actually landed, by request key, in arrival
    /// order.
    ///
    /// The scripted `gh`'s world log and not its request directory, and the
    /// difference is the whole point: `requested_paths` records every call
    /// including the reads, and both of a body walk's postcondition reads address
    /// the very path its `PATCH` does. A recorder that could not tell a `GET`
    /// from a `PATCH` on one path could not say whether a rewrite happened, which
    /// is the question both lanes below ask — one expecting yes and one expecting
    /// no.
    fn mutations(&self) -> Vec<String> {
        std::fs::read_to_string(self.stub.join("world"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter_map(|landed| landed["key"].as_str().map(str::to_string))
            .collect()
    }
}

/// **A second run over an open labelled pull request rewrites its body, and runs
/// in that pull request's tree.**
///
/// The defect this lane exists for is silent by construction: every count above
/// is still one, the run still exits 0, and the only thing wrong is that the
/// description a person reads still belongs to last night. So the assertions are
/// about *content* — what the body became, and which tree the work was built on —
/// and each one has a wrong answer available to be caught.
///
/// # The body
///
/// Asserted to equal [`RUN_BODY`] exactly, and not merely to differ from
/// [`STALE_BODY`]. "It changed" is satisfied by a rewrite that put anything at
/// all there — a truncation, an empty description, the summary of some other
/// group — and the claim is that the pull request now says what *this run* did.
/// The `PATCH` is asserted beside it, out of the world log, so the new sentence
/// is known to have arrived by a mutation this run dispatched rather than by the
/// fixture agreeing with the test.
///
/// # The tree
///
/// `Checkout::AtPullRequestHead` is reached only when a pull request is open, so
/// until this lane the bundle's `attempt_tree` was `base_revision` and `pr_head`
/// was null on every run in this repository. Three readings, because a sha
/// comparison alone is weak: the bundle names the seeded head, it names the base
/// as a *different* revision, and the branch the run pushed carries
/// [`PRIOR_RUN_MARKER`] — a file reachable from the pull request's tip and from
/// nowhere on `main`, so the work provably sits on top of the pull request rather
/// than beside it.
#[test]
fn a_second_run_over_a_shared_pull_request_rewrites_its_body_and_works_in_its_tree() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());
    let seeded_head = sweep.seed_shared_pull_request(STALE_BODY);

    let run = sweep.run();
    let payload = sweep.payload(&run);
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}\npayload: {payload}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(payload["outcome"], "completed", "{payload}");

    // Still one, and still the one that was already there: a run that failed to
    // recognise it would have opened a second, and one that opened a second and
    // rewrote *that* would satisfy every body assertion below about the wrong
    // object.
    let pulls = sweep.pull_requests();
    assert_eq!(
        pulls.len(),
        1,
        "the shared pull request is shared: {pulls:?}"
    );
    assert_eq!(pulls[0]["number"], SHARED_PR, "{:?}", pulls[0]);

    // And which row that is. The *interesting* neighbour here is row 7: a run
    // that saw the same open pull request and decided its commits already
    // covered everything would also leave one pull request numbered 41 behind,
    // and would reach `already_in_progress` having pushed nothing. This run
    // landed work, so it is row 4 — and the branch it names is the shared one it
    // was given rather than a dated one it cut for itself.
    let reached = sweep.disposition(&run);
    assert_eq!(
        reached["reason"], "pull_request",
        "this run landed a clean group onto the shared branch, so it is row 4 \
         and not the row for work somebody else's pull request already carries: \
         {reached}"
    );
    assert_eq!(reached["pull_request"], SHARED_PR, "{reached}");
    assert_eq!(reached["branch"], SHARED_BRANCH, "{reached}");

    // What it now says, and that a mutation is what made it say so.
    let shared = sweep.pull_request(SHARED_PR);
    assert_eq!(
        shared["body"].as_str(),
        Some(RUN_BODY),
        "the shared pull request must describe tonight's run: {shared}"
    );
    assert_eq!(
        sweep.mutations(),
        vec![format!("PATCH_repos_acme_r_pulls_{SHARED_PR}")],
        "exactly one forge mutation landed, and it is the body rewrite — no \
         create, because the postcondition for a pull request on this head and \
         base already held"
    );

    // Which tree the attempt ran in. The base is read off the remote, so the two
    // revisions are the world's rather than the bundle's own two copies of one
    // value.
    let base_revision = git_says(&sweep.remote, &["rev-parse", SWEEP_BASE]);
    let bundle = sweep.bundle(&run);
    assert_eq!(
        bundle["observations"]["tree"],
        serde_json::json!({
            "base_revision": base_revision,
            "pr_head": seeded_head,
            "attempt_tree": "pr_head",
            "scanned_image_digest": SCANNED_DIGEST,
        }),
        "{bundle}"
    );
    assert_ne!(
        seeded_head, base_revision,
        "the seeded head has to be a commit the base does not have, or \
         `attempt_tree` could be either and the assertion above would not say \
         which tree was used"
    );

    // And the witness that is not a sha: the run's commit sits on top of the
    // pull request's, so the branch it pushed still carries the earlier run's
    // file — and carries the mitigation as well.
    assert!(
        pushed_file(&sweep, SHARED_BRANCH, PRIOR_RUN_MARKER).contains("An earlier run's notes"),
        "the work was built on the pull request's tip, so its history survives"
    );
    assert!(
        pushed_file(&sweep, SHARED_BRANCH, "go.mod").contains(&format!("{MODULE} {FIXED_VERSION}")),
        "and the branch carries the requirement at the fixed release"
    );
}

/// **A run whose description is already correct rewrites nothing.**
///
/// The other half, and the reason `EnsurePullRequestBody`'s postcondition is a
/// read of the world rather than a record of what was done before: nothing in
/// `fiddle-core` remembers that an effect was performed, so an unchanged body has
/// to be idempotent because the pull request already holds the sentence, which is
/// a fact a fresh process establishes with one read.
///
/// The world differs from the lane above **in one string** — the body the pull
/// request is seeded with — and in nothing else. That is what makes the two a
/// pair: the same branch, the same commit, the same scan, the same fixture, and
/// the observable is whether a `PATCH` landed.
///
/// # Why the positive assertions are here
///
/// "No mutation" is satisfied by a run that fell over before it reached the
/// publish, so the lane asserts the run got there: the branch on the remote
/// carries the mitigation, which only the push at the end of a successful sweep
/// can produce. Without that, this would pass just as well against a binary that
/// refused to start.
#[test]
fn a_run_whose_shared_body_is_unchanged_dispatches_no_rewrite() {
    let sweep = Sweep::scanning(VULNERABLE, SCAN_OK, 1, a_repair_moving_the_requirement());
    sweep.seed_shared_pull_request(RUN_BODY);

    let run = sweep.run();
    assert_eq!(
        run.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    assert_eq!(
        sweep.mutations(),
        Vec::<String>::new(),
        "a pull request that already says what this run did is a postcondition \
         that already holds, and an effect the world satisfies is not dispatched"
    );
    assert_eq!(
        sweep.pull_request(SHARED_PR)["body"].as_str(),
        Some(RUN_BODY),
        "and the description is left exactly as it was found"
    );

    // The run reached the publish, so the absence above is a decision and not a
    // failure: the remote branch carries the bump, which nothing but the push at
    // the end of a sweep puts there.
    assert!(
        pushed_file(&sweep, SHARED_BRANCH, "go.mod").contains(&format!("{MODULE} {FIXED_VERSION}")),
        "the sweep still did its work"
    );
    // And it reached it on row 4, which is the other way of saying the same
    // thing from the record rather than from the remote — a run that reached
    // `already_in_progress` would have dispatched no rewrite either, and would
    // have satisfied the assertion this lane is named for without doing any work
    // at all.
    let reached = sweep.disposition(&run);
    assert_eq!(
        reached["reason"], "pull_request",
        "no rewrite because the description was already right, and not because \
         the run decided there was nothing to land: {reached}"
    );
    assert_eq!(
        sweep.pull_requests().len(),
        1,
        "and opened nothing beside the pull request it was given"
    );
}

// ---------------------------------------------------------------------------
// The round trip: one run's commit body, read by the next (bean `fiddle-0c4l`)
// ---------------------------------------------------------------------------
//
// Every lane above hands the reader of a commit log a body **the test wrote**.
// The two lanes just above seed one through `seed_shared_pull_request`, and
// `an_open_pull_request_covering_the_rest_reaches_already_in_progress` seeds one
// through `seed_shared_pull_request_saying` — and each now says at its own site
// why it seeds.
//
// # What was already proved, and what was not
//
// The **format** agreement was not the gap, and saying so is the honest version
// of this lane's claim. `cve_protocol`'s
// `a_clean_group_commits_only_the_files_it_edited_and_names_every_cve` and
// `the_production_seam_lands_a_group_in_a_real_worktree` both read the commit
// `cve::land` really made and ask `FixedInCommits::read` about it, so
// `commit_body`'s trailers and that reader's word-splitting were already held
// against each other. Both go red if either end moves. Measured, not assumed:
// making `read` consult only a body's first line takes those two down.
//
// What no lane covered is everything *around* the two ends — the range, the
// process boundary, and the consumer. Those two are one process with the commit
// in hand and they call `read` on `git log -1`; neither goes anywhere near
// `commit_log_dedup`'s `origin/<base>..HEAD`, a pull request discovered by its
// label, a worktree made at that pull request's head, or `Run::in_progress`'s
// `covers`. So *a body a run wrote is readable* was proved and *the next run
// reads it* was not, and the second is what a nightly sweep spends its life in.
//
// The failure that hid there is silent in the direction nobody chases. A range
// that does not reach the earlier run's commit — or a format disagreement, since
// this lane catches that too — leaves `covers` empty, so the next run lands on
// row 3 instead of row 7 and reports work as merely *already fixed in the tree*
// when it is in fact sitting in an open pull request somebody has to go and
// merge. Exit 0, an empty verdict report, and a plausible-looking record.
//
// Both mutations were applied alone against the whole of this file. Narrowing
// the range to the base's own log takes down this lane and the seeded row-7 lane
// — the seeded one because it reads the same range. Making `read` consult only
// the first line takes down **this lane alone** out of the thirty here, because
// every other body in this file is one the test wrote and the seeded row-7 one
// is a single line. That asymmetry is the whole point of the lane.
//
// The lane below is the only one in this file that starts the binary twice.

/// **A second run reads the first run's own commit body, and reaches row 7 by
/// it.**
///
/// # No history is seeded, and that is the whole claim
///
/// Neither `seed_shared_pull_request` nor `seed_shared_pull_request_saying` is
/// called. The forge starts empty and the remote holds nothing but
/// [`SWEEP_BASE`], so the first run takes the *fresh* arm: it cuts a dated
/// branch, commits the bump with the `Fixes:` body `cve::land` produces, pushes,
/// and opens a labelled pull request. Between the two runs the test does
/// nothing at all — no commit, no ref, no `pulls_seed` write. Everything the
/// second run reads about what has already been fixed was written by the first
/// run's own commit producer, which is what makes this a round trip rather than
/// a second reading of a fixture.
///
/// # Why the document is [`SCAN_LIBRARY_ONLY`]
///
/// Because row 7 sits **below row 2** in `disposition`'s table, so a run with
/// anything left to report never reaches it — and because the library half is
/// the half that has a producer at all. Under [`SCAN_OK`] the OS advisory is one
/// no run can settle, so it produces a verdict and the second night would land
/// on row 2 whatever the log said. A document naming only the library advisory
/// is a document whose every finding this build can both *fix* and *record*.
///
/// # What discriminates a round trip that worked from one that did not
///
/// Row 7 against row 3, and they are one branch apart. On the second night the
/// worktree is made at the pull request's head, so the tree already ships
/// [`FIXED_VERSION`] and `already_fixed`'s *tree* arm settles the library
/// advisory on its own — which is row 3, `already_fixed`, and is exactly what
/// this run reaches if the commit body cannot be read. `covers` is built from
/// the log scan and from nothing else, so `already_in_progress` and the number
/// beside it are the log's answer and only the log's.
///
/// So a body whose format `FixedInCommits::read` cannot parse, or a range that
/// does not reach the first run's commit, moves this lane from row 7 to row 3.
/// Both mutations were applied and both were caught here — see the row census
/// above [`a_vulnerable_fixture_yields_exactly_one_pull_request_and_one_branch`].
///
/// # The three readings beside the row
///
/// The row alone would be satisfied by a second run that found *some* commit
/// naming the advisory, so the lane also pins what the first run wrote, that the
/// second run stood on it, and that it dispatched nothing: the pushed commit's
/// body is asserted to be `cve::land`'s trailer for the advisory the document
/// names; the second run's tree observation names the first run's head as
/// `attempt_tree`, so the range it scanned provably reached that commit; and the
/// forge's mutation log is unchanged across the second run, so the pull request
/// it reported is one it recognised rather than one it opened.
///
/// # What a second sweep concludes, which is what this lane is the record of
///
/// **A sweep is idempotent by rescanning, not by remembering.** The reference it
/// runs under names no work item, so nothing anywhere holds a completion state for
/// it and the second night has nothing to consult: it scans the image again, from
/// scratch, exactly as the first night did. What keeps that from doing the work
/// twice is design §4's dedup and nothing else — the commit-log read that finds
/// the first night's own `Fixes:` trailer, and the open pull request it names —
/// which is why the second night reaches row 7 `already_in_progress` and dispatches
/// no mutation rather than opening a second pull request. ADR 023 is the decision;
/// this lane is what makes it checkable from outside the process.
///
/// It is also why nothing happens between the two runs. It used to: the first
/// night's correlation marker was deleted, because `assess` read that marker as
/// the sweep's completion and the second invocation would otherwise have derived
/// `Complete` and executed nothing. The marker is still written and is now
/// asserted to be there both before and after — it records that a run happened,
/// which is all a trackerless reference's change set ever said.
#[test]
fn a_second_run_reads_the_first_runs_own_commit_body() {
    // One attempt in the whole scenario: the first night forms one group and the
    // second forms none, so a script of one is also the assertion that the
    // second run asks the model nothing.
    //
    // The rescan is [`SCAN_CLEAN`] and not [`RESCAN_CLEAN`], and the reason is
    // arithmetic about *this* document rather than a preference. `RESCAN_CLEAN`
    // empties the library array and carries [`SCAN_OK`]'s OS findings forward,
    // which is a rescan reporting nothing new only when the baseline held them —
    // and [`SCAN_LIBRARY_ONLY`]'s OS array is empty. Against this baseline it is
    // a rescan reporting a *new* OS advisory, so `evaluate` refuses the group
    // `NewFindingAppeared`, the first night lands nothing, and there is no body
    // for the second night to read. Both arrays present and both empty is what
    // this image honestly looks like once its one advisory is fixed.
    let sweep = Sweep::scanning_rescanning(
        VULNERABLE,
        SCAN_LIBRARY_ONLY,
        SCAN_CLEAN,
        2,
        a_repair_moving_the_requirement(),
    );

    // -- the first night -----------------------------------------------------

    let first = sweep.run();
    assert_eq!(
        first.status.code(),
        Some(0),
        "the first night must land its work, or the second reads nothing — \
         stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let opened = the_row_both_surfaces_agree_on(&sweep, &first, "the first night");
    assert_eq!(
        opened["reason"], "pull_request",
        "the first night is row 4: it cut a branch and landed a clean group on \
         it, which is the only way it can leave a body behind: {opened}"
    );
    let number = opened["pull_request"]
        .as_u64()
        .unwrap_or_else(|| panic!("the first night opened no pull request: {opened}"));
    assert_ne!(
        number, SHARED_PR,
        "and it is a pull request this run created rather than one a fixture \
         put there: nothing in this lane seeds a pull request, so the number \
         must not be the seeded one"
    );

    // What the first run wrote, read out of the bare repository — so what the
    // second run is about to scan is what a person cloning the branch would get.
    // The trailer is asserted in full rather than as "contains the id", because
    // the format *is* the contract between the two ends: `commit_body` writes
    // `Fixes: <id>` and `FixedInCommits::read` splits on everything that is
    // neither alphanumeric nor a hyphen.
    let branch = the_one_new_branch(&sweep);
    assert_eq!(
        opened["branch"].as_str(),
        Some(branch.as_str()),
        "the row names the branch the remote holds: {opened}"
    );
    let commits = pushed_commits(&sweep, &branch);
    assert_eq!(
        commits.len(),
        1,
        "one group, one commit — and the body below is that commit's: {commits:?}"
    );
    assert!(
        commits[0].0.contains(&format!("Fixes: {LIBRARY_CVE}")),
        "the first run's own commit has to name the advisory it fixed, or there \
         is nothing for the second run to read: {commits:?}"
    );
    let first_head = git_says(&sweep.remote, &["rev-parse", &branch]);

    // **Nothing happens between the two runs.** No commit, no ref, no seed, and
    // nothing on the remote or in the forge is touched — and, since ADR 023, not
    // the correlation marker either. The first night's marker is still sitting in
    // `<stub.root>/changes/cve.json`, and it has to be, or this lane no longer
    // says what it says: a sweep is idempotent by *rescanning*, so the second
    // night must scan with the first night's marker in front of it. Until ADR 023
    // this lane deleted that file, and the deletion was the only reason the second
    // invocation was a run at all.
    assert_eq!(
        sweep.change_marker(),
        Some(sweep.scenario.expected_marker(SWEEP_REF)),
        "the first night's marker must still be there, or the second night is a \
         second night for the wrong reason"
    );

    // -- the second night ----------------------------------------------------

    let second = sweep.run();
    assert_eq!(
        second.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&second.stderr)
    );
    the_old_surface_says_nothing(&sweep, &second, "the second night");

    assert_eq!(
        the_row_both_surfaces_agree_on(&sweep, &second, "the second night"),
        serde_json::json!({
            "reason": "already_in_progress",
            "verdicts": 0,
            "already_fixed": [LIBRARY_CVE],
            "deferred": [],
            "attempts": [],
            "branch": serde_json::Value::Null,
            "pull_request": number,
        }),
        "row 7, reached through the first run's own `Fixes:` trailer: the tree \
         arm settles the advisory too, which is row 3, so `already_in_progress` \
         and the number beside it are the commit log's answer and nothing else's"
    );

    // That the range really reached the earlier commit, said as a revision
    // rather than inferred from the row: the second night's worktree was made at
    // the first night's pushed head, so `origin/main..HEAD` spans it.
    let bundle = sweep.bundle(&second);
    assert_eq!(
        bundle["observations"]["tree"]["attempt_tree"], "pr_head",
        "the second night must work in the pull request's tree, or its log scan \
         cannot see the first night's commit: {bundle}"
    );
    assert_eq!(
        bundle["observations"]["tree"]["pr_head"].as_str(),
        Some(first_head.as_str()),
        "and that head is the commit the first night pushed: {bundle}"
    );

    // And that the second night recognised the pull request rather than acting
    // on it. Two readings, because either alone has a way of passing wrongly: a
    // run that opened a second pull request would still leave the first one's
    // number in the row above, and a run that fell over before reaching the
    // forge would also have dispatched nothing.
    let pulls = sweep.pull_requests();
    assert_eq!(
        pulls.len(),
        1,
        "the second night opens nothing beside the pull request it found: \
         {pulls:?}"
    );
    assert_eq!(
        sweep.mutations(),
        vec![
            "POST_repos_acme_r_pulls".to_string(),
            format!("POST_repos_acme_r_issues_{number}_labels"),
        ],
        "exactly the first night's two mutations — the create and the label \
         that makes the object discoverable — and nothing the second night \
         dispatched, because a run whose work is already open lands nothing"
    );

    // And the marker is still there afterwards. The fix is that a trackerless
    // reference's marker accounts for nothing, not that it stopped being written:
    // a capability that recorded nothing would leave a run no later reader could
    // see the shape of, and the assertion before the second night would then be
    // passing over an absence.
    assert_eq!(
        sweep.change_marker(),
        Some(sweep.scenario.expected_marker(SWEEP_REF)),
        "the second night records itself too — the marker is a record that a run \
         happened, and only the reading of it changed"
    );
}

/// `--help` is the only place an operator learns what to type, and the bare form
/// is the whole of this milestone's invocation: `fiddle run cve` discovers its own
/// findings, which no `<scheme>:<value>` example can suggest is legal. ADR 019
/// admits the form and `InvocationScheme::stands_alone` implements it, so help
/// text that names only the valued shape describes a grammar the binary stopped
/// having. Asserted for `run` and `inspect` together because a form accepted by
/// one and undocumented by the other is the same defect twice.
#[test]
fn help_names_the_bare_form_the_grammar_accepts() {
    for command in ["run", "inspect"] {
        let out = support::fiddle_command()
            .args([command, "--help"])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(0),
            "{command} --help failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let help = String::from_utf8_lossy(&out.stdout);
        // A bare occurrence specifically: help that named only
        // `cve:CVE-2026-1234` would contain "cve" while still describing the
        // grammar that refuses `fiddle run cve`.
        assert!(
            help.contains("`cve`"),
            "{command} --help never mentions the bare `cve` form:\n{help}"
        );
        assert!(
            help.contains("stands\n") || help.contains("stands alone"),
            "{command} --help mentions `cve` without saying it takes no value:\n{help}"
        );
    }
}
