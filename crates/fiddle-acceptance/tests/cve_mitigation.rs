//! The CVE fixture pair, and what makes a check result attributable to a fix.
//!
//! # Why this suite is here and not in `fiddle-runtime`
//!
//! Task 20 puts the black-box lane for `fiddle run cve` in this file, and those
//! lanes drive the compiled binary as a subprocess in the ordinary way. What
//! Task 19 adds below drives no binary at all: it is a *fixture integrity*
//! suite, and it reads the two trees in `tests/fixtures/` and compiles them.
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

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
