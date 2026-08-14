//! The worlds the CVE lanes are written against: Go trees on disk, scanner
//! documents as bytes, git history, and the sentinels.
//!
//! Every world here is *constructed* and does nothing else. There is no
//! behaviour to substitute for and no assertion to share — a lane's subject is
//! the code that reads one of these, so anything this module decided on a lane's
//! behalf would be a decision the lane could no longer test.
//!
//! # What is deliberately absent
//!
//! Only the helpers whose signatures need no type this milestone has yet to
//! build. An earlier draft of this module's task also listed `scanner_with`,
//! `world_with`, `contract` and the `scripted_gh_*` builders, whose signatures
//! need `Scanner` and `ScanError`, `ProjectedFinding`, and the check list — none
//! of which exists, so that version could not have compiled.
//!
//! **The extension convention, which the later tasks follow:** a task that
//! introduces a type adds the helpers built on it *here*, rather than defining
//! them beside its own suite, so two lanes cannot end up with differently-named
//! versions of one fixture. Concretely:
//!
//! - Task 4 adds `scanner_with` and `scanner_recording_env`, and replaces
//!   [`wiz_stub`]'s derived path with the `env!` cargo guarantees.
//! - Task 11 adds `contract`, `contract_for` and `contract_scanned_by`.
//! - Task 17 adds `forge()` and the `scripted_gh_*` builders.
//! - Task 19 adds `fixture` and `world_with`.
//!
//! # What a scanner document here is, and is not
//!
//! [`report_with`] and its variants produce the *bytes* a scanner would have
//! written, and nothing writes them to disk. The thing that puts a scan on a
//! filesystem is the scripted `wizcli` of Task 4, and a writer here as well would
//! be a second one to drift from — the same argument that put `mod.rs`'s scripted
//! world in one file. So the stub is where a document meets the disk, and that
//! stub's arms should print these bytes rather than embed a second copy of them.
//!
//! # A sentinel is only evidence if something planted it
//!
//! The four constants below are all read by assertions of the form *"this string
//! is not in that output"*. Such an assertion says nothing at all unless the
//! world under test actually contains the sentinel somewhere upstream of the
//! output — see `docs/technical/evidence-discipline.md` on fixture values that
//! appear only where their value cannot matter.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Sentinels
// ---------------------------------------------------------------------------

/// A credential's value, where a test's subject is that it never surfaces.
///
/// Distinct from [`SENTINEL_SECRET`] because the two are read by different
/// assertions — this one for a value leaving through *any* channel, that one for
/// a value reaching a child's `argv` — and an inversion has to be able to say
/// which of them a mutation broke.
pub const SENTINEL: &str = "fiddle-sentinel-9f14c2a7";

/// The scanner credential specifically, planted so that "no credential reaches
/// `argv`" is a fact about a process rather than a claim about one.
pub const SENTINEL_SECRET: &str = "fiddle-secret-3b8e51d0";

/// Advisory prose, planted where a scanner document carries a description.
///
/// The projection is meant to carry six fields and no free text, and a report
/// whose description is something innocuous cannot tell *dropped the prose* apart
/// from *there was no prose*.
pub const SENTINEL_PROSE: &str = "fiddle-prose-c47a06f9";

/// A host filesystem fact, planted where one could leak into published output.
///
/// Shaped like an absolute path because that is what leaks: a check runner
/// announces where it is working, and M1's relativisation exists for it.
pub const HOST_ROOT: &str = "/fiddle-host-root-5d2b8e13";

/// Every sentinel, so that "no two of them can be confused" is asserted over all
/// of them rather than over the pairs somebody remembered.
pub const ALL_SENTINELS: [&str; 4] = [SENTINEL, SENTINEL_SECRET, SENTINEL_PROSE, HOST_ROOT];

// ---------------------------------------------------------------------------
// Where the scripted scanner will be
// ---------------------------------------------------------------------------

/// A program and the arguments it is run with.
///
/// The same shape as `fiddle_cli::config::ProgramRef`, and deliberately not that
/// type: `fiddle-runtime` does not depend on `fiddle-cli`, and acquiring a
/// dependency on the binary crate so a fixture can name a program would invert
/// the layering for the convenience of one test helper.
#[derive(Debug, Clone)]
pub struct ProgramRef {
    pub program: String,
    pub args: Vec<String>,
}

/// Where Task 4 will build the scripted `wizcli`, and which arm to ask it for.
///
/// This fixes the *location* so that Task 4 has one place to satisfy instead of
/// inventing its own, and it deliberately does not require the binary to exist:
/// the path is derived from a sibling stub cargo already builds rather than from
/// `env!("CARGO_BIN_EXE_wiz_stub")`, which would not compile until the `[[bin]]`
/// is declared.
///
/// **The derivation is a placeholder and should not survive Task 4.** It assumes
/// the two stubs land in one directory, which is cargo's layout rather than
/// anything cargo promises; `CARGO_BIN_EXE_<name>` is the construction that is
/// promised, and it is what every other suite in this crate uses.
/// `the_derived_stub_path_is_a_placeholder_until_task_4_declares_the_binary` in
/// `support.rs` fails on the day the swap becomes possible.
pub fn wiz_stub(arm: &str) -> ProgramRef {
    ProgramRef {
        program: Path::new(env!("CARGO_BIN_EXE_gh_stub"))
            .with_file_name("wiz_stub")
            .display()
            .to_string(),
        args: vec![arm.to_string()],
    }
}

// ---------------------------------------------------------------------------
// Go trees on disk
// ---------------------------------------------------------------------------

/// The module path every fixture tree calls itself, standing in for the host
/// repository under repair.
const HOST_MODULE: &str = "example.com/host";

/// The language version every fixture tree declares. One value, so that a
/// difference between two trees is never an accident of this line.
const GO_VERSION: &str = "1.23";

/// The module a *direct* finding is in, and the version it is pinned at.
const DIRECT_MODULE: &str = "golang.org/x/crypto";
const DIRECT_VERSION: &str = "v0.31.0";

/// The module an *indirect* finding is in. Reached through a parent rather than
/// required by the host, which is the whole of what makes attribution rule 2
/// different from rule 1.
const INDIRECT_MODULE: &str = "golang.org/x/net";
const INDIRECT_VERSION: &str = "v0.24.0";

/// A parent with room above it, and a parent with none.
///
/// What actually makes a parent non-viable is that no published version of it
/// carries the fix, and **that is not a fact `go.mod` can hold** — it lives in
/// whatever answers the module proxy. The two shapes therefore differ by the
/// version the parent is pinned at, which is the closest thing a tree can say:
/// one is a minor behind, the other is where its line ends. A lane that needs the
/// probe in attribution rule 2 to genuinely succeed and genuinely fail has to
/// supply the upstream half as well — a `replace` onto a local module tree, or a
/// scripted `go` — because nothing offline can conjure a version that exists.
const PARENT_A_MINOR_BEHIND: &str = "v1.2.0";
const PARENT_AT_THE_END_OF_ITS_LINE: &str = "v1.9.9";

/// A module the host requires that no finding is ever about, so
/// `go mod why -m <the finding's module>` answers that it is not needed.
const UNRELATED_MODULE: &str = "gh.com/unrelated";
const UNRELATED_VERSION: &str = "v1.0.0";

/// The parent [`all_shapes`] uses, so the listed shapes are built from one value
/// rather than from three spellings of "some parent".
const FIXTURE_PARENT: &str = "gh.com/parent";

/// What [`all_shapes`] ships already fixed: a tree pinned *above* the version a
/// finding names as fixed, which is the case `version::at_least` exists for.
const SHIPPED_VERSION: &str = "v0.54.1";

/// Which world a Go tree is.
///
/// Named for the situation the tree puts the code under test in, rather than for
/// the file contents, because the contents are this module's business and the
/// situation is the lane's.
#[derive(Debug, Clone)]
pub enum Shape {
    /// The vulnerable module is required by the host itself.
    Direct,
    /// The vulnerable module arrives through a parent that has a newer minor.
    IndirectVia(String),
    /// The same, where the parent's line ends before the fix.
    IndirectViaParentWithoutTheFix(String),
    /// The finding is in the standard library, so there is no module to bump and
    /// the tree pins a toolchain instead.
    Stdlib,
    /// The finding names a module this tree does not require at all.
    ModuleNotNeeded,
    /// The tree already requires `module` at `version`.
    Shipped { module: String, version: String },
}

/// The vulnerable module is the host's own requirement — attribution rule 1.
pub fn direct() -> Shape {
    Shape::Direct
}

/// The vulnerable module is indirect, through a parent that can carry the fix.
pub fn indirect_via(parent: &str) -> Shape {
    Shape::IndirectVia(parent.to_string())
}

/// The same, through a parent that cannot — see [`PARENT_AT_THE_END_OF_ITS_LINE`]
/// for what this tree can and cannot say about that.
pub fn indirect_via_parent_without_the_fix(parent: &str) -> Shape {
    Shape::IndirectViaParentWithoutTheFix(parent.to_string())
}

/// The finding is in the Go standard library.
pub fn stdlib() -> Shape {
    Shape::Stdlib
}

/// The finding names a module the tree does not require.
pub fn module_not_needed() -> Shape {
    Shape::ModuleNotNeeded
}

/// A tree that already requires `module` at `version`.
pub fn shipped(module: &str, version: &str) -> Shape {
    Shape::Shipped {
        module: module.to_string(),
        version: version.to_string(),
    }
}

/// How many shapes there are, pinning [`all_shapes`]'s length at compile time.
///
/// The count is here rather than inferred because an inferred one cannot be
/// wrong: `every_go_shape_is_listed` first computed its expectation *from the list
/// it was checking*, so deleting the last entry left five positions numbered 0..5
/// and the test passed. That is a guard comparing a list to itself. Measured, not
/// argued — the mutation is `inv-m7-all-shapes-drops-one`, and it was green.
const SHAPES: usize = 6;

impl Shape {
    /// This shape's position in [`all_shapes`].
    ///
    /// The match is exhaustive, so a new shape cannot be added without being given
    /// a position, and the highest position here has to agree with [`SHAPES`] three
    /// lines above it.
    ///
    /// **What the pair of guards catches, and what it does not.** Deleting a listed
    /// shape is a *compile* error, because [`all_shapes`] returns an array of
    /// [`SHAPES`]. Listing one twice, or giving two shapes one position, fails
    /// `every_go_shape_is_listed`. Adding a variant and leaving it off the list is
    /// caught by neither — nothing in Rust can enumerate an enum's variants — and
    /// what stands in for it is that the new `index` arm cannot be written without
    /// reading the constant it has to exceed.
    pub fn index(&self) -> usize {
        match self {
            Shape::Direct => 0,
            Shape::IndirectVia(_) => 1,
            Shape::IndirectViaParentWithoutTheFix(_) => 2,
            Shape::Stdlib => 3,
            Shape::ModuleNotNeeded => 4,
            Shape::Shipped { .. } => 5,
        }
    }

    /// What the tree requires: the module, the version, and whether the
    /// requirement is indirect.
    fn requirements(&self) -> Vec<(String, String, bool)> {
        let require =
            |module: &str, version: &str| (module.to_string(), version.to_string(), false);
        match self {
            Shape::Direct => vec![require(DIRECT_MODULE, DIRECT_VERSION)],
            Shape::IndirectVia(parent) => vec![
                require(parent, PARENT_A_MINOR_BEHIND),
                (
                    INDIRECT_MODULE.to_string(),
                    INDIRECT_VERSION.to_string(),
                    true,
                ),
            ],
            Shape::IndirectViaParentWithoutTheFix(parent) => vec![
                require(parent, PARENT_AT_THE_END_OF_ITS_LINE),
                (
                    INDIRECT_MODULE.to_string(),
                    INDIRECT_VERSION.to_string(),
                    true,
                ),
            ],
            // Nothing is required, and that absence is the shape: a standard
            // library finding has no requirement to edit, only the toolchain
            // line `go_mod` writes for this variant.
            Shape::Stdlib => Vec::new(),
            Shape::ModuleNotNeeded => vec![require(UNRELATED_MODULE, UNRELATED_VERSION)],
            Shape::Shipped { module, version } => vec![require(module, version)],
        }
    }

    /// The `go.mod` this shape writes.
    fn go_mod(&self) -> String {
        let mut text = format!("module {HOST_MODULE}\n\ngo {GO_VERSION}\n");
        // A `toolchain` line only where the standard library is the thing a fix
        // would have to move, so the directive is present exactly where it would
        // be edited.
        if matches!(self, Shape::Stdlib) {
            text.push_str(&format!("\ntoolchain go{GO_VERSION}.0\n"));
        }
        let requirements = self.requirements();
        for (module, version, _) in requirements.iter().filter(|(_, _, i)| !*i) {
            text.push_str(&format!("\nrequire {module} {version}\n"));
        }
        for (module, version, _) in requirements.iter().filter(|(_, _, i)| *i) {
            text.push_str(&format!("\nrequire {module} {version} // indirect\n"));
        }
        text
    }

    /// The `go.sum` this shape writes, or `None` where it requires nothing.
    ///
    /// The hashes are fabricated and cannot be otherwise: a real one is the
    /// digest of a module zip that no offline fixture holds. What the file is for
    /// is the *path* — Task 15 asserts a commit stages `go.mod` and `go.sum` and
    /// no third thing, which needs the second file to exist. A lane that needs
    /// `go mod tidy` to actually verify against it has to bring a module cache.
    fn go_sum(&self) -> Option<String> {
        let requirements = self.requirements();
        if requirements.is_empty() {
            return None;
        }
        // 43 characters and a pad: a `h1:` line is base64 over a 32-byte digest,
        // and go rejects one that is not the right length before it ever gets as
        // far as disagreeing about the value.
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
}

/// Every shape there is, built from one value each.
///
/// A function rather than the `const` array `mod.rs`'s [`Script::ALL`] uses,
/// because two of these carry an owned parent path — but an *array* of [`SHAPES`]
/// rather than a `Vec`, which is the half of `ALL` that was load-bearing: with a
/// `Vec`, deleting an entry compiled and every test stayed green.
///
/// [`Script::ALL`]: super::Script::ALL
pub fn all_shapes() -> [Shape; SHAPES] {
    [
        direct(),
        indirect_via(FIXTURE_PARENT),
        indirect_via_parent_without_the_fix(FIXTURE_PARENT),
        stdlib(),
        module_not_needed(),
        shipped(DIRECT_MODULE, SHIPPED_VERSION),
    ]
}

/// A Go repository on disk, in a temporary directory of its own.
///
/// Real files rather than an in-memory double, because a lane's evidence has to
/// be something a reader can go and inspect: an attribution that claims a parent
/// was probed and reverted is a claim about a file, and a fake filesystem can
/// only ever prove it against itself.
pub struct GoWorkspace {
    /// Held only so that [`Drop`] removes the tree. The explicit-`remove`-plus-
    /// guard arrangement `crate::workspace::Workspace` uses is for a teardown
    /// failure that has to be *reported*; nothing here has anybody to report to,
    /// and what matters is that a suite of a few dozen tests does not leave a few
    /// dozen directories behind.
    root: TempDir,
    repo: PathBuf,
    /// Every git invocation made *through this handle*. See [`GoWorkspace::git`].
    calls: Mutex<Vec<String>>,
}

impl GoWorkspace {
    /// The repository's root, absolute and canonical.
    ///
    /// Canonical because macOS puts temporary directories under `/var`, a symlink
    /// to `/private/var`, so a child process that resolves its own working
    /// directory reports a path that is not the string the directory was created
    /// with — the same trap `workspace::command`'s relativisation handles from the
    /// other end.
    pub fn path(&self) -> &Path {
        &self.repo
    }

    /// What `go.mod` says *now*.
    ///
    /// Read from the file on every call rather than remembered from
    /// construction, so that an assertion about a tree that was edited and
    /// reverted is an assertion about the tree. A remembered string would answer
    /// the same before and after a revert that never happened.
    pub fn go_mod(&self) -> String {
        std::fs::read_to_string(self.repo.join("go.mod"))
            .unwrap_or_else(|source| panic!("no go.mod in {}: {source}", self.repo.display()))
    }

    /// Does the tree match its `HEAD`?
    ///
    /// Not recorded in [`GoWorkspace::git_calls`]: this is a question the test
    /// asks, and an answer that contained the asking would put the assertion into
    /// its own evidence.
    pub fn is_clean(&self) -> bool {
        run_git(&self.repo, &["status", "--porcelain"]).is_empty()
    }

    /// Run git in this repository, recording the invocation.
    ///
    /// The record is what makes "history is never rewritten" and "nothing staged
    /// everything" assertable, and it covers what goes through *this handle*
    /// rather than every git on the machine — a fixture that intercepted git
    /// generally would be a `git` implementation, which is what
    /// `tests/git_stub/git_stub.rs` already is for the suites that need one.
    ///
    /// Construction does not record, and that is the load-bearing half: a fresh
    /// workspace is initialised, staged and committed by git, and a record that
    /// held those would make an assertion about what the code under test staged
    /// into an assertion about what this module staged.
    pub fn git(&self, args: &[&str]) -> String {
        self.calls.lock().unwrap().push(args.join(" "));
        run_git(&self.repo, args)
    }

    /// Every git invocation made through [`GoWorkspace::git`], in order.
    pub fn git_calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

/// Build the tree `shape` describes, as a committed git repository.
///
/// Committed rather than merely written, because every lane that uses one goes on
/// to commit or revert: with no `HEAD` there is nothing for `git status` to be
/// clean *against*, and [`GoWorkspace::is_clean`] would answer the same for a
/// reverted tree and for an unborn one.
pub fn go(shape: Shape) -> GoWorkspace {
    let root = TempDir::new().expect("a temporary directory for a fixture tree");
    let repo = write_tree(root.path(), "host", &shape);
    commit_tree(&repo, &shape, "the fixture tree");
    GoWorkspace {
        repo: canonical(&repo),
        root,
        calls: Mutex::new(Vec::new()),
    }
}

/// A tree that already requires `module` at `version`.
///
/// The spelling `go(shipped(..))` says the same thing; this one exists because
/// what a lane means here is a whole world rather than a shape it then builds.
pub fn go_with_shipped(module: &str, version: &str) -> GoWorkspace {
    go(shipped(module, version))
}

/// A repository whose history is truncated, as a `--depth 1` clone is.
///
/// Cloned from a real repository with two commits rather than constructed to look
/// shallow, because what a lane asserts about one is that a fixed set cannot be
/// read out of it — and a repository that merely *has* one commit is not
/// truncated, it is short. `git rev-parse --is-shallow-repository` tells those
/// apart and `support.rs` asserts it does.
///
/// `file://` and not a plain path: git ignores `--depth` for a local path and says
/// so in a warning, which would leave this fixture a full clone.
pub fn shallow_clone() -> GoWorkspace {
    let root = TempDir::new().expect("a temporary directory for a fixture tree");
    let shape = direct();
    let origin = write_tree(root.path(), "origin", &shape);
    commit_tree(&origin, &shape, "the fixture tree");
    // A second commit, so there is something for the truncation to leave behind.
    std::fs::write(origin.join("README.md"), "the host repository\n").unwrap();
    commit_paths(&origin, &["README.md"], "chore: earlier work");

    let url = format!("file://{}", canonical(&origin).display());
    run_git(
        root.path(),
        &["clone", "--depth", "1", "--quiet", &url, "host"],
    );
    GoWorkspace {
        repo: canonical(&root.path().join("host")),
        root,
        calls: Mutex::new(Vec::new()),
    }
}

/// Write `shape`'s files into a fresh `name` directory under `parent`.
fn write_tree(parent: &Path, name: &str, shape: &Shape) -> PathBuf {
    let repo = parent.join(name);
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("go.mod"), shape.go_mod()).unwrap();
    if let Some(go_sum) = shape.go_sum() {
        std::fs::write(repo.join("go.sum"), go_sum).unwrap();
    }
    repo
}

/// Initialise `repo` and commit exactly the files `shape` wrote.
fn commit_tree(repo: &Path, shape: &Shape, message: &str) {
    run_git(
        repo,
        &["-c", "init.defaultBranch=main", "init", "--quiet", "."],
    );
    let mut paths = vec!["go.mod"];
    if shape.go_sum().is_some() {
        paths.push("go.sum");
    }
    commit_paths(repo, &paths, message);
}

/// Stage exactly `paths` and commit them.
///
/// Named paths rather than `add -A`: the milestone's own rule is that a commit
/// names what it changed, and a fixture that staged by directory would be the one
/// place in the repository doing the thing every lane asserts against.
fn commit_paths(repo: &Path, paths: &[&str], message: &str) {
    let mut add = vec!["add", "--"];
    add.extend_from_slice(paths);
    run_git(repo, &add);
    run_git(
        repo,
        &[
            // Passed per invocation rather than assumed: a CI runner has no
            // `user.email` and `git commit` refuses outright without one, so a
            // fixture leaning on the ambient config passes locally and fails
            // there. `tests/fixture.rs` was written this way for the same reason.
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    );
}

/// `path` with symlinks resolved, which on macOS is what a child process will
/// report as its own working directory. See [`GoWorkspace::path`].
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|source| panic!("could not resolve {}: {source}", path.display()))
}

/// Run git in `dir` and return its stdout, trailing newline trimmed.
///
/// Panics with git's own stderr, because a fixture that failed quietly surfaces
/// as an unrelated assertion further down whichever test happened to build it.
fn run_git(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|source| panic!("could not run git {args:?}: {source}"));
    assert!(
        output.status.success(),
        "git {args:?} in {} failed: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

// ---------------------------------------------------------------------------
// Git history
// ---------------------------------------------------------------------------

/// A commit history, and what a log over it says.
///
/// A real repository rather than a string, because the thing a lane reads it with
/// is a `git log` invocation: a fixture that handed over prepared text would
/// prove the scanner of that text right and say nothing about the command.
pub struct CommitLog {
    /// Held for its [`Drop`], exactly as [`GoWorkspace::root`] is.
    root: TempDir,
    repo: PathBuf,
    raw: String,
}

impl CommitLog {
    /// What `git log` printed: every commit body, newest first.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The repository the log came out of, so a lane can point its own git at it.
    pub fn path(&self) -> &Path {
        &self.repo
    }
}

/// A history with one commit per body, oldest first.
///
/// The bodies are what the OS-package arm recovers a previously-fixed set from,
/// so they are the whole content of the world: the commits are empty on purpose,
/// because a tree that also changed would let a lane pass on the diff instead.
pub fn log_of(bodies: &[&str]) -> CommitLog {
    let root = TempDir::new().expect("a temporary directory for a fixture history");
    let repo = root.path().join("history");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(
        &repo,
        &["-c", "init.defaultBranch=main", "init", "--quiet", "."],
    );
    for body in bodies {
        run_git(
            &repo,
            &[
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "--allow-empty",
                "--quiet",
                "-m",
                body,
            ],
        );
    }
    // Decided by the argument and not by tolerating an error: over a repository
    // with no commits `git log` *fails* rather than printing nothing, and a
    // helper that swallowed that would answer "" for a broken invocation too.
    let raw = match bodies.is_empty() {
        true => String::new(),
        false => run_git(&repo, &["log", "--format=%B"]),
    };
    CommitLog {
        repo: canonical(&repo),
        root,
        raw,
    }
}

// ---------------------------------------------------------------------------
// Scanner documents
// ---------------------------------------------------------------------------

/// Library packages a scanner document can report, cycled by position so that two
/// advisory ids produce two different packages.
const LIBRARY_PACKAGES: [(&str, &str, &str); 3] = [
    ("golang.org/x/crypto", "v0.31.0", "v0.35.0"),
    ("golang.org/x/net", "v0.24.0", "v0.28.0"),
    ("github.com/docker/docker", "v24.0.7", "v24.0.9"),
];

/// The same for OS packages, whose versions are a distribution's and not a
/// module's — which is why the two arrays cannot share a projection rule.
const OS_PACKAGES: [(&str, &str, &str); 3] = [
    ("libssl3", "3.0.11-r0", "3.0.12-r0"),
    ("busybox", "1.36.1-r5", "1.36.1-r7"),
    ("zlib", "1.3-r0", "1.3.1-r0"),
];

/// The advisory description a document carries unless a test asked for prose.
///
/// Innocuous, and it has to be: a default of [`SENTINEL_PROSE`] would put the
/// sentinel in every world, and "the prose did not cross the boundary" would then
/// be untestable because no document lacks it.
const BENIGN_DESCRIPTION: &str = "a benign advisory summary";

/// What the library array holds when a variant does not say.
const DEFAULT_LIBRARY_CVES: [&str; 1] = ["CVE-2026-0001"];

/// What the OS array holds when a variant does not say.
const DEFAULT_OS_CVES: [&str; 1] = ["CVE-2026-0002"];

/// A vulnerable package, as a scanner reports one.
#[derive(Debug, Clone)]
struct Package {
    name: String,
    version: String,
    vulnerabilities: Vec<serde_json::Value>,
}

/// The `libraries` half of a scanner document.
///
/// A type of its own, and so is [`OsPackages`], for one reason:
/// `report_with(libraries(..), os_packages(..))` takes two arrays of the same
/// shape, and two `Vec`s could be handed over the wrong way round with nothing to
/// notice. A projection bug and a transposed fixture look identical in the
/// result, so the transposition is made a compile error instead.
#[derive(Debug, Clone)]
pub struct Libraries(Vec<Package>);

/// The `osPackages` half. See [`Libraries`].
#[derive(Debug, Clone)]
pub struct OsPackages(Vec<Package>);

/// Library packages, one per advisory id.
pub fn libraries(cves: &[&str]) -> Libraries {
    Libraries(packages(cves, &LIBRARY_PACKAGES))
}

/// OS packages, one per advisory id.
pub fn os_packages(cves: &[&str]) -> OsPackages {
    OsPackages(packages(cves, &OS_PACKAGES))
}

fn packages(cves: &[&str], table: &[(&str, &str, &str); 3]) -> Vec<Package> {
    cves.iter()
        .enumerate()
        .map(|(at, cve)| {
            let (name, current, fixed) = table[at % table.len()];
            Package {
                name: name.to_string(),
                version: current.to_string(),
                vulnerabilities: vec![vulnerability(cve, Some(fixed), BENIGN_DESCRIPTION)],
            }
        })
        .collect()
}

/// One reported vulnerability.
///
/// `HIGH` because that is the severity the selection rule admits on its own: a
/// fixture at a lower severity would be selected only through `hasExploit`, and
/// then every test about selection would be about the other arm.
fn vulnerability(cve: &str, fixed: Option<&str>, description: &str) -> serde_json::Value {
    let mut value = serde_json::json!({
        "name": cve,
        "severity": "HIGH",
        "hasExploit": false,
        "description": description,
    });
    // Absent rather than null where there is no fix, because absent is what the
    // reference pipeline produces and the two are not the same document.
    if let Some(fixed) = fixed {
        value["fixedVersion"] = serde_json::Value::String(fixed.to_string());
    }
    value
}

fn as_json(packages: &[Package]) -> serde_json::Value {
    serde_json::Value::Array(
        packages
            .iter()
            .map(|package| {
                serde_json::json!({
                    "name": package.name,
                    "version": package.version,
                    "vulnerabilities": package.vulnerabilities,
                })
            })
            .collect(),
    )
}

/// Which scanner document a world holds.
#[derive(Debug, Clone)]
pub enum ReportVariant {
    /// The ordinary document: whatever the two arrays were given.
    Plain(Libraries, OsPackages),
    /// No `osPackages` key at all.
    OsAbsent,
    /// An `osPackages` key holding an empty array.
    OsEmpty,
    /// One advisory reported twice, once with a fix and once without.
    DuplicateCve(String),
    /// A document carrying advisory prose.
    AdvisoryDescription(String),
}

/// How many document variants there are, pinning [`canonical_reports`]'s length.
/// See [`SHAPES`] for why the count is written down rather than inferred.
const REPORT_VARIANTS: usize = 5;

impl ReportVariant {
    /// This variant's position in [`canonical_reports`]. The pair of guards
    /// [`Shape::index`] describes, with the same limit, for the documents.
    pub fn index(&self) -> usize {
        match self {
            ReportVariant::Plain(_, _) => 0,
            ReportVariant::OsAbsent => 1,
            ReportVariant::OsEmpty => 2,
            ReportVariant::DuplicateCve(_) => 3,
            ReportVariant::AdvisoryDescription(_) => 4,
        }
    }

    /// A short name for a failure message. Derived from the variant rather than
    /// written beside each construction, so it cannot label the wrong document.
    pub fn label(&self) -> String {
        match self {
            ReportVariant::Plain(Libraries(l), OsPackages(o)) => {
                format!("plain({} libraries, {} os packages)", l.len(), o.len())
            }
            ReportVariant::OsAbsent => "os-absent".to_string(),
            ReportVariant::OsEmpty => "os-empty".to_string(),
            ReportVariant::DuplicateCve(cve) => format!("duplicate({cve})"),
            ReportVariant::AdvisoryDescription(_) => "advisory-description".to_string(),
        }
    }

    fn render(&self) -> Report {
        let mut result = serde_json::Map::new();
        match self {
            ReportVariant::Plain(Libraries(l), OsPackages(o)) => {
                result.insert("libraries".to_string(), as_json(l));
                result.insert("osPackages".to_string(), as_json(o));
            }
            // The key is left out entirely, which is the whole of this world: a
            // reader that treats absent as empty and one that refuses cannot be
            // told apart by a document that has the key.
            ReportVariant::OsAbsent => {
                result.insert(
                    "libraries".to_string(),
                    as_json(&packages(&DEFAULT_LIBRARY_CVES, &LIBRARY_PACKAGES)),
                );
            }
            ReportVariant::OsEmpty => {
                result.insert(
                    "libraries".to_string(),
                    as_json(&packages(&DEFAULT_LIBRARY_CVES, &LIBRARY_PACKAGES)),
                );
                result.insert("osPackages".to_string(), serde_json::json!([]));
            }
            // Two packages, one advisory, one fix between them. The rule this is
            // for splits fixable from upstream-blocked by subtraction, and a
            // document where the id appears once cannot show a filter putting it
            // in both sets.
            ReportVariant::DuplicateCve(cve) => {
                let (fixable_name, fixable_version, fixed) = LIBRARY_PACKAGES[0];
                let (blocked_name, blocked_version, _) = LIBRARY_PACKAGES[1];
                result.insert(
                    "libraries".to_string(),
                    as_json(&[
                        Package {
                            name: fixable_name.to_string(),
                            version: fixable_version.to_string(),
                            vulnerabilities: vec![vulnerability(
                                cve,
                                Some(fixed),
                                BENIGN_DESCRIPTION,
                            )],
                        },
                        Package {
                            name: blocked_name.to_string(),
                            version: blocked_version.to_string(),
                            vulnerabilities: vec![vulnerability(cve, None, BENIGN_DESCRIPTION)],
                        },
                    ]),
                );
                result.insert("osPackages".to_string(), serde_json::json!([]));
            }
            ReportVariant::AdvisoryDescription(prose) => {
                let (name, version, fixed) = LIBRARY_PACKAGES[0];
                result.insert(
                    "libraries".to_string(),
                    as_json(&[Package {
                        name: name.to_string(),
                        version: version.to_string(),
                        vulnerabilities: vec![vulnerability(
                            DEFAULT_LIBRARY_CVES[0],
                            Some(fixed),
                            prose,
                        )],
                    }]),
                );
                result.insert("osPackages".to_string(), serde_json::json!([]));
            }
        }
        Report {
            raw: serde_json::to_string_pretty(&serde_json::json!({ "result": result }))
                .expect("a document built from json! values serializes"),
        }
    }
}

/// A scanner document, as bytes.
///
/// The scanner version and the image digest are not in here. Those are what the
/// *scan* recorded rather than what the document said, and Task 5 resolves them
/// at the adapter. If it turns out `wizcli` puts them in the file after all, the
/// fields belong here and this note is what should change.
#[derive(Debug, Clone)]
pub struct Report {
    raw: String,
}

impl Report {
    /// The bytes a scanner would have written.
    ///
    /// Pretty-printed, which no parser cares about and a failing `assert_ne!`
    /// does: the two documents a lane could not tell apart are readable side by
    /// side.
    pub fn raw(&self) -> &str {
        &self.raw
    }
}

/// A document holding exactly these two arrays.
pub fn report_with(libraries: Libraries, os_packages: OsPackages) -> Report {
    ReportVariant::Plain(libraries, os_packages).render()
}

/// A document with no `osPackages` key.
pub fn report_with_os_absent() -> Report {
    ReportVariant::OsAbsent.render()
}

/// A document whose `osPackages` key holds an empty array.
pub fn report_with_os_empty() -> Report {
    ReportVariant::OsEmpty.render()
}

/// A document reporting `cve` twice, once with a fix and once without.
pub fn report_with_duplicate_cve_one_fixed_one_not(cve: &str) -> Report {
    ReportVariant::DuplicateCve(cve.to_string()).render()
}

/// A document whose advisory carries `text` as its description.
pub fn report_with_advisory_description(text: &str) -> Report {
    ReportVariant::AdvisoryDescription(text.to_string()).render()
}

/// One document per variant, so completeness can be checked. See
/// [`ReportVariant::index`], and [`all_shapes`] for why this is an array.
pub fn canonical_reports() -> [ReportVariant; REPORT_VARIANTS] {
    [
        ReportVariant::Plain(
            libraries(&DEFAULT_LIBRARY_CVES),
            os_packages(&DEFAULT_OS_CVES),
        ),
        ReportVariant::OsAbsent,
        ReportVariant::OsEmpty,
        ReportVariant::DuplicateCve("CVE-2026-0777".to_string()),
        ReportVariant::AdvisoryDescription(SENTINEL_PROSE.to_string()),
    ]
}

/// Every document a lane needs to tell from every other, labelled.
///
/// Built on top of [`canonical_reports`] rather than beside it, so a variant added
/// there is compared here without anybody remembering to; the two extra entries
/// are the one-sided arrays the projection has to read both of.
pub fn distinct_reports() -> Vec<(String, Report)> {
    let mut variants: Vec<ReportVariant> = canonical_reports().into_iter().collect();
    variants.push(ReportVariant::Plain(
        libraries(&["CVE-1"]),
        os_packages(&[]),
    ));
    variants.push(ReportVariant::Plain(
        libraries(&[]),
        os_packages(&["CVE-1"]),
    ));
    variants
        .into_iter()
        .map(|variant| (variant.label(), variant.render()))
        .collect()
}
