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
//!   **Done.** [`wiz_stub`] names the binary the way cargo guarantees,
//!   [`scanner_with`] is below, and [`scanner_recording_env`] joined them in
//!   Task 5 — the task that decided the environment allowlist, which is the
//!   whole content of that helper and the reason it could not be written first.
//! - Task 8.a adds [`finding`], the [`ModuleGraph`] a tree answers about
//!   itself, and the [`Shape::IndirectWithoutADirectParent`] world that is the
//!   read-only way to reach attribution rule 3. **Done.**
//! - Task 8.b adds the module proxy those trees resolve against, [`go_stub`],
//!   and [`spawned_go`] — the same world reached through the adapter that really
//!   spawns a `go`. **Done.** The proxy is `go_proxy.rs` rather than more of this
//!   file, for [`document`]'s reason: a `[[bin]]` shares it, and a `[[bin]]` is
//!   compiled against `[dependencies]` alone.
//! - Task 10 adds [`finding_fixed_at`], [`os_finding`], [`full_clone`] and
//!   [`forge_recording_calls`]. **Done.** [`full_clone`] is the positive half
//!   [`shallow_clone`] needed and did not have; [`forge_recording_calls`] is a
//!   record of what `dedup` ran and is **not** Task 17's `forge()` — read its
//!   own doc before extending either.
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
//! Those builders live in `document.rs` and are re-exported here, so callers are
//! unaffected. The split is what makes the rule above satisfiable: the stub is a
//! `[[bin]]`, a `[[bin]]` sees `[dependencies]` only, and this file reaches
//! `tempfile` — see that file's header.
//!
//! # A sentinel is only evidence if something planted it
//!
//! The four constants below are all read by assertions of the form *"this string
//! is not in that output"*. Such an assertion says nothing at all unless the
//! world under test actually contains the sentinel somewhere upstream of the
//! output — see `docs/technical/evidence-discipline.md` on fixture values that
//! appear only where their value cannot matter.

use fiddle_core::{AdvisoryId, PackageType, ProjectedFinding, Severity};
use fiddle_runtime::cve::attribute::{Manifest, ModuleGraph, ResolverError, Target};
use fiddle_runtime::cve::dedup::{DedupError, Local, Ran, Spawn};
use fiddle_runtime::cve::go::Go;
use fiddle_runtime::cve::group::Attributed;
use fiddle_runtime::scanner::{ScanError, ScanReport, Scanner, WizCredential, Wizcli};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

// The scanner documents, which the scripted `wizcli` includes as well. Glob
// re-exported rather than named one by one so that a builder added there is
// reachable as `support::cve::*` without a second edit here — the split is a
// compilation constraint and must not become an interface.
#[path = "document.rs"]
mod document;
pub use document::*;

// The offline `go` and the module proxy behind it, shared with the scripted
// toolchain the same way. Named rather than glob re-exported: what a lane wants
// from it is the handful of constants that say where a probe can reach, and the
// rest is machinery this file drives on the lane's behalf.
#[path = "go_proxy.rs"]
mod go_proxy;
// Allowed unused for the reason `mod.rs`'s `wiz_stub` re-export is: this module
// is compiled once per suite, and only the attribution lane names these.
#[allow(unused_imports)]
pub use go_proxy::{CARRIED_BY_THE_VIABLE_LINE, REACHED_WITHOUT_THE_FIX};
use go_proxy::{FIXTURE_PARENT, GO_VERSION, HOST_MODULE, INDIRECT_MODULE, INDIRECT_VERSION};

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

/// The scripted `wizcli`, and which arm to ask it for.
///
/// `CARGO_BIN_EXE_<name>` is the construction cargo promises, and it is what
/// every other suite in this crate uses. It replaces the sibling-of-`gh_stub`
/// derivation this function carried while the `[[bin]]` did not yet exist — that
/// one assumed the two stubs land in one directory, which is cargo's layout
/// rather than anything cargo guarantees.
///
/// The arm is the stub's **first** argument, ahead of everything the adapter
/// appends, because it arrives through the same `args` seam an operator would
/// use to wrap a real `wizcli` — see [`ProgramRef`]. That the fixture is selected
/// through the product's own seam rather than through the environment is the
/// same arrangement `gh_stub` is under, and for the same reason: the environment
/// is pinned, so it cannot carry the test's own plumbing.
pub fn wiz_stub(arm: &str) -> ProgramRef {
    ProgramRef {
        program: env!("CARGO_BIN_EXE_wiz_stub").to_string(),
        args: vec![arm.to_string()],
    }
}

/// A scanner that is not installed.
///
/// The one situation the scripted `wizcli` cannot be asked for, and not by
/// oversight: an absent program is a spawn that never happened, so there is no
/// process left to script an arm in. It is reached the only way it can be — by
/// pointing the operator seam at a path holding nothing — which is why it is a
/// [`ProgramRef`] here rather than a seventh entry in [`ARMS`].
///
/// Sited under the stub's own build directory so the path is one cargo really
/// owns, rather than a name in a system directory that a host could turn out to
/// have. The suffix makes it unmistakable in the diagnostic the adapter reports.
pub fn absent_scanner() -> ProgramRef {
    let program = format!("{}-which-is-not-installed", env!("CARGO_BIN_EXE_wiz_stub"));
    assert!(
        !Path::new(&program).exists(),
        "{program} exists, so it cannot stand for a scanner that is not installed"
    );
    ProgramRef {
        program,
        args: Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Go trees on disk
// ---------------------------------------------------------------------------

/// The module a *direct* finding is in, and the version it is pinned at.
const DIRECT_MODULE: &str = "golang.org/x/crypto";
const DIRECT_VERSION: &str = "v0.31.0";

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
///
/// **Task 8.b supplied it**, and it is `go_proxy::UPSTREAM`. Read that table
/// beside these two constants: the `v1.2` line reaches a release that carries the
/// fix and the `v1.9` line reaches one that does not, so the two versions below
/// are no longer *only* the closest thing a tree can say — they are the key the
/// proxy resolves each shape's probe through. `v1.9.9` is still where the line
/// ends in the sense that matters, which is that nothing in it carries the fix;
/// the releases above it exist so that a probe has something to write and the
/// revert has something to undo.
const PARENT_A_MINOR_BEHIND: &str = "v1.2.0";
const PARENT_AT_THE_END_OF_ITS_LINE: &str = "v1.9.9";

/// A module the host requires that no finding is ever about, so
/// `go mod why -m <the finding's module>` answers that it is not needed.
const UNRELATED_MODULE: &str = "gh.com/unrelated";
const UNRELATED_VERSION: &str = "v1.0.0";

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
    /// The vulnerable module is marked `// indirect` and there is no direct
    /// requirement at all, so its `go mod why -m` chain runs straight from the
    /// main module to it and offers no parent to bump instead.
    ///
    /// A real tree and not a contrivance: an untidied `go.mod` looks exactly
    /// like this when the main module has come to import a package it once only
    /// got at one remove. It is here because it is the **read-only** way to
    /// reach attribution rule 3 — the other way is a parent that turns out not
    /// to carry the fix, and [`PARENT_AT_THE_END_OF_ITS_LINE`] explains why no
    /// tree on its own can say that.
    IndirectWithoutADirectParent,
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

/// The vulnerable module is indirect and nothing requires it directly.
pub fn indirect_without_a_direct_parent() -> Shape {
    Shape::IndirectWithoutADirectParent
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
const SHAPES: usize = 7;

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
            Shape::IndirectWithoutADirectParent => 6,
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
            // The indirect requirement and nothing beside it. The absence is the
            // shape: with no direct requirement in the tree there is no hop
            // between the main module and this one, which is what leaves
            // attribution rule 2 with no parent to elect.
            Shape::IndirectWithoutADirectParent => vec![(
                INDIRECT_MODULE.to_string(),
                INDIRECT_VERSION.to_string(),
                true,
            )],
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
    /// Built by `go_proxy::sum_for`, which is also what the offline `go` rewrites
    /// the file with after a bump — one construction, so a tree the proxy has
    /// touched and a tree it has not are the same bytes when the versions agree.
    /// Were they two, every probe would leave a `go.sum` that differs from `HEAD`
    /// for reasons that have nothing to do with a bump, and `is_clean()` would be
    /// answering about the fixture.
    fn go_sum(&self) -> Option<String> {
        go_proxy::sum_for(&self.requirements())
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
        indirect_without_a_direct_parent(),
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

/// The same clone with its history intact, and `bodies` committed on top of it.
///
/// The **positive half of [`shallow_clone`]**, and it is not optional garnish: a
/// lane that only has the truncated world cannot tell a reader that refuses one
/// bad repository from a reader that refuses every repository. Here there is an
/// `origin/main` to measure against and commits ahead of it to be read, so the
/// range the subject builds is exercised rather than assumed.
///
/// Cloned from a real origin rather than assembled in one directory, because
/// `origin/<base>..HEAD` is a range over a *remote-tracking* ref, and a
/// repository that was never cloned has none. The commits are empty for
/// [`log_of`]'s reason: the bodies are the whole content of the world, and a
/// tree that also changed would let a lane pass on the diff instead.
pub fn full_clone(bodies: &[&str]) -> GoWorkspace {
    let root = TempDir::new().expect("a temporary directory for a fixture tree");
    let shape = direct();
    let origin = write_tree(root.path(), "origin", &shape);
    commit_tree(&origin, &shape, "the fixture tree");

    // `file://` and not a plain path, for `shallow_clone`'s reason, and so that
    // the pair differ by exactly the `--depth` argument and nothing else.
    let url = format!("file://{}", canonical(&origin).display());
    run_git(root.path(), &["clone", "--quiet", &url, "host"]);

    let repo = root.path().join("host");
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
    GoWorkspace {
        repo: canonical(&repo),
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
// Findings, and the module graph a tree answers about itself
// ---------------------------------------------------------------------------

/// The advisory every fixture finding is filed under, where a lane does not care
/// which advisory it is.
///
/// One value, because no lane below Task 9 groups or deduplicates: a finding's
/// identity there is the package it names, and two spellings of "some advisory"
/// would be two things to keep in step for no assertion's benefit. Task 9's
/// grouping lanes are the ones that do care — *these two findings are one bump*
/// is a claim about two distinct advisories — so they name their own through
/// [`attributed`], and this constant stays what the rest of the family uses.
const FIXTURE_ADVISORY: &str = "CVE-2026-0008";

/// What a fixture finding says the artefact ships, and where the fix lands.
///
/// Spelled **without** a leading `v`, because that is how a scanner spells a
/// version and the mixed-prefix pair is the trap `cve::version::at_least` exists
/// for. A fixture that wrote `v0.33.0` here would hand every comparison the easy
/// case and hide the one that mis-ordered in the pipeline this milestone
/// replaces.
const FINDING_CURRENT: &str = "0.24.0";
const FINDING_FIXED: &str = "0.33.0";

/// A finding against `package`, of `package_type`.
///
/// Everything except those two is fixed, and that is the point: the lanes built
/// on this are about *where a finding is fixed*, which is a function of the
/// package and its type alone. A builder that let a lane vary the severity would
/// invite a test to pass because of a field attribution never reads.
///
/// `Critical` so the finding is one `fiddle_core::selected` acts on — a fixture
/// finding this build would have filtered out before attribution ever saw it
/// would be a world no lane is really in.
pub fn finding(package: &str, package_type: PackageType) -> ProjectedFinding {
    finding_under(FIXTURE_ADVISORY, package, package_type, FINDING_FIXED)
}

/// The distribution package every OS fixture finding is against.
///
/// One value, because no lane distinguishes two of them: what settles an OS
/// finding is the branch's commit bodies, which name advisories and not
/// packages. A builder that let a lane vary this would offer a knob that cannot
/// change any answer, which is an invitation to write a test that passes for the
/// wrong reason.
const OS_PACKAGE: &str = "openssl";

/// A `Library` finding against `package` whose fix lands in `fixed`.
///
/// The un-attributed sibling of [`attributed_fixed_at`], and it exists for the
/// stage that runs *before* attribution: Task 10's already-fixed set is asked
/// about a projected finding, because the whole point of it is to drop one
/// before `go mod why` is ever run for it.
///
/// `fixed` is the argument because it is the only field that lane varies — the
/// question is whether the tree is at or above it — and the package comes with
/// it so that the finding and the tree can be made to name the same module or
/// deliberately different ones.
pub fn finding_fixed_at(package: &str, fixed: &str) -> ProjectedFinding {
    finding_under(FIXTURE_ADVISORY, package, PackageType::Library, fixed)
}

/// An `Os` finding under `cve`.
///
/// The advisory is the argument and nothing else is, which is the shape of the
/// question: an OS finding is settled by whether some commit body on this branch
/// names *this advisory*. It carries a `fixedVersion` like every other fixture
/// finding — [`finding_under`]'s — precisely so a lane can tell a commit-body
/// answer from a version comparison that leaked across into the OS arm. A
/// fixture that left the field empty would make that arm untestable, because
/// then both readings would answer the same.
pub fn os_finding(cve: &str) -> ProjectedFinding {
    finding_under(cve, OS_PACKAGE, PackageType::Os, FINDING_FIXED)
}

/// The same finding with its advisory and its fix named.
///
/// Private, and [`finding`] delegates to it rather than holding a second literal:
/// two fixture findings built field by field in one file are two things to keep
/// in step, and the field that would drift is the one no grouping lane looks at
/// and every attribution lane depends on.
fn finding_under(
    cve: &str,
    package: &str,
    package_type: PackageType,
    fixed: &str,
) -> ProjectedFinding {
    ProjectedFinding {
        cve: AdvisoryId::parse(cve).expect("a fixture advisory id parses"),
        package: package.to_string(),
        current: FINDING_CURRENT.to_string(),
        fixed_version: Some(fixed.to_string()),
        severity: Severity::Critical,
        package_type,
    }
}

/// A `Library` finding under `cve`, against `package`, already attributed to the
/// module `target`.
///
/// **Three arguments and not two.** Task 9's two grouping lanes are *two
/// packages resolving to one parent* and *one package resolving to two targets*,
/// and neither story can be told by a builder that derives the package from the
/// advisory: the first needs two packages under one target and the second needs
/// one package under two. A fixture that could not vary the package would leave
/// both lanes passing against a grouping keyed on the package instead of on the
/// target, which is the very thing they exist to distinguish.
///
/// Attributed, and not attributed *here*: the target is handed in because
/// grouping is a pure operation over findings some earlier stage placed. Nothing
/// in this helper runs a rule — see [`fiddle_runtime::cve::attribute`] for the
/// code that does, and the lanes above for the trees it is measured against.
pub fn attributed(cve: &str, package: &str, target: &str) -> Attributed {
    attributed_fixed_at(cve, package, target, FINDING_FIXED)
}

/// The same, where the version the advisory is fixed in matters to the lane.
///
/// Separate from [`attributed`] rather than a fourth argument on it, because the
/// grouping lanes and the version lane want opposite things: the first want the
/// fix to be noise, and the one that asks what version a group moves to wants it
/// to be the only thing that varies.
pub fn attributed_fixed_at(cve: &str, package: &str, target: &str, fixed: &str) -> Attributed {
    Attributed::new(
        finding_under(cve, package, PackageType::Library, fixed),
        Target::Module(target.to_string()),
    )
}

/// An `Os` finding under `cve`, against the distribution package `package`.
///
/// The target is [`Target::DockerfileBaseImage`] and there is no argument for it,
/// because there is nothing to choose: every OS finding in this build is fixed by
/// moving one base image tag. That is the whole of why the OS lane needs no
/// special case in the grouping — the key is already the same value for all of
/// them — and a builder that let a lane pass a target would be inventing a
/// distinction the domain does not have.
pub fn attributed_os(cve: &str, package: &str) -> Attributed {
    Attributed::new(
        finding_under(cve, package, PackageType::Os, FINDING_FIXED),
        Target::DockerfileBaseImage,
    )
}

/// The releases a module proxy — or an image registry — says exist.
///
/// Owned strings, because that is what an adapter reading `go list -m -versions`
/// or a tag list would hand over, and a lane that passed borrowed literals would
/// be testing the selection against a shape no caller has.
///
/// Deliberately **not** sorted here. Which of these is the latest patch inside a
/// minor is the question the subject answers, and a fixture that handed it an
/// ordered list would answer half of it on the subject's behalf — a selection
/// that simply took the last entry would pass every lane.
pub fn available(versions: &[&str]) -> Vec<String> {
    versions.iter().map(|version| version.to_string()).collect()
}

/// A fixture tree answering the questions attribution asks of `go`, in `go`'s
/// own output formats, without a process.
///
/// # Why the tree answers rather than a real `go`
///
/// There is no `go` in this project's development shell and there is no module
/// proxy behind one: `go mod why` loads packages, which means source, which
/// means a populated module cache. A lane that needed one would be a lane that
/// runs nowhere, so the port `attribute` is written against is implemented here
/// — the same arrangement the scanner is under, where [`ScriptedScanner`] stands
/// in for a `wizcli` the offline gate can never reach.
///
/// What that leaves under test is the **reading** of `go`'s output, the matching
/// of the rules over it, and — since 8.b — the probe that measures rule 2's
/// viability. Nothing here decides a rule or names a target: every method prints
/// a document or writes a file, exactly as `go` does, and the subject parses.
/// That is the line the module header draws, and it is why they return text — a
/// stand-in that answered *this parent is viable* rather than *here is what `go
/// list` printed* would be answering rule 2 on the subject's behalf.
///
/// # This is the cheap half of a pair
///
/// [`spawned_go`] is the other: the same worlds, reached through the production
/// adapter that really spawns a child. Both are backed by `go_proxy`, which is
/// the single implementation of the offline toolchain — so a lane that runs here
/// and a lane that runs there cannot be shown different documents.
///
/// # What the answers are derived from
///
/// The tree on disk, read on every call, so an edit to `go.mod` changes what the
/// resolver says. That is not a convenience: the probe's confirm *is* the same
/// `go list` asked again after a bump, and a stand-in that remembered its first
/// answer could not tell a parent that carried the fix from one that did not.
#[async_trait::async_trait]
impl ModuleGraph for GoWorkspace {
    async fn list(&self, module: &str) -> Result<String, ResolverError> {
        Ok(self.go(&["list", "-m", "-json", module]))
    }

    async fn why(&self, module: &str) -> Result<String, ResolverError> {
        Ok(self.go(&["mod", "why", "-m", module]))
    }

    async fn manifest(&self) -> Result<Manifest, ResolverError> {
        Ok(Manifest {
            go_mod: self.go_mod(),
            go_sum: std::fs::read_to_string(self.repo.join("go.sum")).ok(),
        })
    }

    async fn get(&self, module: &str, query: &str) -> Result<String, ResolverError> {
        Ok(self.go(&["get", &format!("{module}@{query}")]))
    }

    async fn tidy(&self) -> Result<String, ResolverError> {
        Ok(self.go(&["mod", "tidy"]))
    }

    async fn restore(&self, manifest: &Manifest) -> Result<(), ResolverError> {
        std::fs::write(self.repo.join("go.mod"), &manifest.go_mod).unwrap();
        let go_sum = self.repo.join("go.sum");
        match &manifest.go_sum {
            Some(contents) => std::fs::write(&go_sum, contents).unwrap(),
            // Removed, not left alone: a probe that created a `go.sum` in a tree
            // that had none has changed the tree, and a restore that only ever
            // wrote files would leave it behind.
            None => {
                let _ = std::fs::remove_file(&go_sum);
            }
        }
        Ok(())
    }
}

impl GoWorkspace {
    /// Run the offline `go` in this tree and hand back what it said.
    ///
    /// Stdout when there is any and stderr otherwise, which is `Answer::text` —
    /// the same rule `fiddle_runtime::cve::go::Go` applies to a finished child.
    /// Written as one call rather than inlined at six methods so the in-process
    /// stand-in cannot start reading the two streams differently from the
    /// spawning one.
    fn go(&self, args: &[&str]) -> String {
        go_proxy::run(&self.repo, args).text()
    }

    /// Every `require` line the tree holds now: path, version, indirect.
    fn go_mod_requirements(&self) -> Vec<(String, String, bool)> {
        go_proxy::requirements(&self.repo)
    }
}

// ---------------------------------------------------------------------------
// The same worlds, through the adapter that really spawns a `go`
// ---------------------------------------------------------------------------

/// How long a scripted `go` may take. Far longer than any command needs, so a
/// test that fails has failed on the answer rather than on a loaded machine.
const SCRIPTED_GO_TIMEOUT: Duration = Duration::from_secs(60);

/// The scripted `go`.
///
/// `CARGO_BIN_EXE_<name>` is the construction cargo promises, exactly as
/// [`wiz_stub`] uses it. Unlike that one it takes no arm: `go` is a program with
/// subcommands, and which document comes back is a function of the tree it runs
/// in rather than of a fixture switch — see that program's header for why an arm
/// would defeat the probe it exists to make measurable.
pub fn go_stub() -> ProgramRef {
    ProgramRef {
        program: env!("CARGO_BIN_EXE_go_stub").to_string(),
        args: Vec::new(),
    }
}

/// `workspace`'s tree, answered by a real child process.
///
/// The point of it is what runs, not what answers: [`Go`] is **production code**
/// — it spawns under `crate::process::run_bounded`, in an environment built from
/// nothing, reads the child's two streams and puts `go.mod`/`go.sum` back — and
/// this is the only way the offline gate can drive any of that. Only the
/// toolchain is scripted, which is the arrangement [`ScriptedScanner`] is under.
///
/// The lanes that reach for it are the ones whose subject is the probe itself.
/// Everything a lane can establish without a process it should establish against
/// [`GoWorkspace`] directly, which is the same worlds and no spawn.
pub fn spawned_go(workspace: &GoWorkspace) -> SpawnedGo {
    let home = TempDir::new().expect("a temporary directory for a toolchain's caches");
    let stub = go_stub();
    SpawnedGo {
        go: Go::new(
            PathBuf::from(stub.program),
            stub.args,
            workspace.path().to_path_buf(),
            home.path().to_path_buf(),
            SCRIPTED_GO_TIMEOUT,
            CancellationToken::new(),
        ),
        home,
    }
}

/// A [`Go`] and the throwaway `HOME` it points its child at, with one lifetime.
///
/// The home is owned here for [`ScriptedScanner`]'s reason: a `TempDir` that
/// dropped while the adapter still held its path would point a child at a
/// directory that no longer exists. It implements the port rather than exposing
/// the adapter, so a suite drives attribution through the seam the capability
/// will hold.
pub struct SpawnedGo {
    go: Go,
    /// Held for its [`Drop`], as [`GoWorkspace::root`] is.
    home: TempDir,
}

#[async_trait::async_trait]
impl ModuleGraph for SpawnedGo {
    async fn list(&self, module: &str) -> Result<String, ResolverError> {
        self.go.list(module).await
    }

    async fn why(&self, module: &str) -> Result<String, ResolverError> {
        self.go.why(module).await
    }

    async fn manifest(&self) -> Result<Manifest, ResolverError> {
        self.go.manifest().await
    }

    async fn get(&self, module: &str, query: &str) -> Result<String, ResolverError> {
        self.go.get(module, query).await
    }

    async fn tidy(&self) -> Result<String, ResolverError> {
        self.go.tidy().await
    }

    async fn restore(&self, manifest: &Manifest) -> Result<(), ResolverError> {
        self.go.restore(manifest).await
    }
}

/// A toolchain that is not installed.
///
/// Reached the only way it can be — by pointing the operator seam at a path
/// holding nothing — for [`absent_scanner`]'s reason: an absent program is a
/// spawn that never happened, so there is nothing left to script. Sited under the
/// stub's own build directory so the path is one cargo really owns.
pub fn absent_go(workspace: &GoWorkspace) -> SpawnedGo {
    let program = format!("{}-which-is-not-installed", env!("CARGO_BIN_EXE_go_stub"));
    assert!(
        !Path::new(&program).exists(),
        "{program} exists, so it cannot stand for a toolchain that is not installed"
    );
    let home = TempDir::new().expect("a temporary directory for a toolchain's caches");
    SpawnedGo {
        go: Go::new(
            PathBuf::from(program),
            Vec::new(),
            workspace.path().to_path_buf(),
            home.path().to_path_buf(),
            SCRIPTED_GO_TIMEOUT,
            CancellationToken::new(),
        ),
        home,
    }
}

/// Where the scripted `go` writes down what it was started with.
const GO_CHILD_RECORD: &str = "child.json";

impl SpawnedGo {
    /// The `HOME` the child is given, so a lane can assert that a toolchain's
    /// caches land outside the tree whose diff is the evidence.
    pub fn home(&self) -> &Path {
        self.home.path()
    }

    /// Every environment variable the child actually received.
    ///
    /// Read off the disk on each call rather than cached, and a [`BTreeMap`] so
    /// the names come back in one order whatever order the operating system
    /// handed them over in — [`ScriptedScanner::child_env`] gives the whole of
    /// the argument, and this is the same record answering it for a different
    /// spawn site.
    pub fn child_env(&self) -> BTreeMap<String, String> {
        self.child()["env"]
            .as_array()
            .expect("the scripted go records its environment as an array")
            .iter()
            .map(|entry| {
                let entry = entry.as_str().expect("an environment entry is a string");
                let (name, value) = entry
                    .split_once('=')
                    .unwrap_or_else(|| panic!("{entry} is not a NAME=VALUE entry"));
                (name.to_string(), value.to_string())
            })
            .collect()
    }

    /// The names alone, in order. See [`SpawnedGo::child_env`].
    pub fn child_env_names(&self) -> Vec<String> {
        self.child_env().into_keys().collect()
    }

    /// The record itself, or a panic naming what is missing.
    ///
    /// Panics rather than returning an [`Option`], because every path that
    /// reaches it has already run a command: an absent record means no child
    /// started, and reporting that as an empty environment would turn a fixture
    /// that failed to spawn into a boundary assertion that passed.
    fn child(&self) -> serde_json::Value {
        let record = self.home.path().join(GO_CHILD_RECORD);
        let raw = std::fs::read_to_string(&record).unwrap_or_else(|source| {
            panic!(
                "no record at {}, so no child of this adapter was observed: {source}",
                record.display()
            )
        });
        serde_json::from_str(&raw)
            .unwrap_or_else(|source| panic!("{} is not a record: {source}", record.display()))
    }
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

/// Every program the already-fixed set was read with, in order.
///
/// # What this is, and what it is deliberately not
///
/// It is not a forge, and it is not [`fiddle_runtime::cve::dedup`]'s stand-in
/// for one. It is that module's [`Spawn`] seam — the single way it starts any
/// program at all — wrapped so the invocations can be counted, and it hands each
/// one straight to the real [`Local`] underneath. The lane it exists for asserts
/// that **no** call was a forge call, and the reason it can assert that rather
/// than merely observe an empty list is that the list is not empty: the `git`
/// reads are in it, so a recorder that had never been wired in is a reading the
/// lane can exclude.
///
/// Recording and delegating rather than answering is [`GoWorkspace::git`]'s
/// arrangement, and for the same reason: a fixture that answered would be
/// deciding what git says, and what the lane is about is what the subject *ran*.
///
/// **Task 17 brings `forge()` and the `scripted_gh_*` builders**, which are a
/// different object — a scripted `gh` that answers pull request queries for the
/// lanes that legitimately make them. Nothing here should grow into that: this
/// one proves an absence and needs no arms at all.
pub struct RecordedCalls {
    calls: Mutex<Vec<String>>,
}

impl RecordedCalls {
    /// Each invocation as `program arg arg`, in the order it was made.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl Spawn for RecordedCalls {
    fn run(&self, program: &str, args: &[&str], dir: &Path) -> Result<Ran, DedupError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{program} {}", args.join(" ")));
        Local.run(program, args, dir)
    }
}

/// A [`RecordedCalls`] with nothing in it yet.
pub fn forge_recording_calls() -> RecordedCalls {
    RecordedCalls {
        calls: Mutex::new(Vec::new()),
    }
}

// ---------------------------------------------------------------------------
// The scripted scanner
// ---------------------------------------------------------------------------

/// The image every scanner test scans.
///
/// A tag rather than a digest, because a tag is what a caller has: resolving one
/// to the digest a report is filed under is the scanner's job, and a fixture
/// that handed over a digest would let an adapter that never resolved anything
/// pass. The value is not an image anybody can pull, which is the point — the
/// gate is offline, and the only thing that ever answers for it is
/// [`wiz_stub`].
pub fn image() -> String {
    "ghcr.io/acme/widget:fiddle-fixture".to_string()
}

/// How long a scripted scan may take. Far longer than any arm needs, so a test
/// that fails has failed on the arm rather than on a loaded machine.
const SCRIPTED_SCAN_TIMEOUT: Duration = Duration::from_secs(60);

/// The tenant identifier every scripted scan authenticates as.
///
/// **Not a sentinel, and deliberately not in [`ALL_SENTINELS`].** The three
/// sentinels above are read by assertions that a value *never* appears; this one
/// is read by an assertion that it *does* — it is how a test says "the
/// diagnostic I am looking at is the one that arm wrote" before going on to
/// assert that the secret beside it was taken out. A client id is a public
/// identifier and is not redacted, for the reason [`WizCredential`] gives: a
/// failed authentication that names no account is one nobody can act on.
pub const FIXTURE_CLIENT_ID: &str = "fiddle-client-1c93f0a5";

/// The credential every scripted scan is given.
///
/// One value for every arm, so that the credential the boundary tests assert
/// about is the credential an ordinary scan runs under — a secret planted only
/// in the test that looks for it would be a secret nothing else could leak.
fn scripted_credential() -> WizCredential {
    WizCredential {
        client_id: FIXTURE_CLIENT_ID.to_string(),
        client_secret: SENTINEL_SECRET.to_string(),
    }
}

/// Every arm the scripted scanner has.
///
/// A fixed-length array rather than a `Vec` or a slice literal at each use, for
/// [`all_shapes`]'s reason: deleting an entry has to be a compile error, and
/// with a `Vec` it was not — the loop simply got shorter and every test stayed
/// green. [`arm_was_exercised`] matches on these names exhaustively, so the two
/// halves cannot drift apart in the other direction either.
///
/// The first two are the arms a scan *succeeds* on. That
/// `exit-nonzero-with-file` is one of them is the whole of what this fixture is
/// for; see [`arm_was_exercised`].
///
/// `no-daemon` sits beside `no-such-image` because the two are neighbours: both
/// end on the status line `exit-nonzero-no-file` ends on and neither writes an
/// artefact, so all three differ by their diagnostic alone — which is exactly
/// the discrimination the adapter has to make and the reason none of them can be
/// dropped.
///
/// `leaks-its-credential` is the last and is not a scanner outcome at all: it is
/// the same failure as `exit-nonzero-no-file` with a diagnostic that quotes the
/// secret it was given. It is listed here rather than kept beside the one test
/// that drives it so that this array stays *every* arm the stub has, which is
/// what lets [`arm_was_exercised`] and [`arm_exits_with`] match exhaustively.
pub const ARMS: [&str; 8] = [
    "ok",
    "exit-nonzero-with-file",
    "exit-nonzero-no-file",
    "empty-file",
    "unparseable-file",
    "no-such-image",
    "no-daemon",
    "leaks-its-credential",
];

/// A scanner that runs `program`.
///
/// Returns a [`Scanner`] rather than a [`Wizcli`] because the scratch directory
/// has to outlive the scan and a temporary directory is owned, not borrowed: a
/// bare adapter handed a path whose `TempDir` had already dropped would look for
/// a report in a directory that no longer exists. [`ScriptedScanner`] holds both,
/// so `scanner_with(..).scan(..)` is a single expression that still has its
/// scratch directory when the child writes into it.
///
/// Every scanner this module builds carries [`scripted_credential`], including
/// the ones whose test never mentions a credential. That is the point: the
/// boundary assertions are about the environment an *ordinary* scan runs under,
/// and a secret supplied only where it is looked for could not have leaked from
/// anywhere else.
pub fn scanner_with(program: ProgramRef) -> ScriptedScanner {
    let scratch = TempDir::new().expect("a temporary directory for a scan's report");
    ScriptedScanner {
        wizcli: Wizcli::new(
            PathBuf::from(program.program),
            program.args,
            scratch.path().to_path_buf(),
            SCRIPTED_SCAN_TIMEOUT,
            CancellationToken::new(),
            scripted_credential(),
        ),
        scratch,
    }
}

/// A scanner whose child writes down what it was started with.
///
/// The extension convention in this file's header assigned this to Task 4, which
/// left it out on purpose: its whole content is the environment allowlist, and
/// that set was Task 5's to decide and to assert.
///
/// What it turned out to be is *nothing*, and that is the honest shape of it. The
/// scripted scanner records its argv and its environment on **every** arm — see
/// that program's header for why a `record-env` arm would have been the wrong
/// construction — so this is an ordinary successful scan, and the recording is
/// read back through [`ScriptedScanner`]. The function exists anyway, because the
/// convention is that a suite names the world it wants rather than the arm that
/// happens to produce it, and because a caller reading `scanner_with(wiz_stub(
/// "ok"))` would have no way to know a record was waiting for it.
pub fn scanner_recording_env() -> ScriptedScanner {
    scanner_with(wiz_stub("ok"))
}

/// A [`Wizcli`] and the scratch directory it writes into, with one lifetime.
///
/// It implements the port rather than exposing the adapter, so a suite drives a
/// scan through [`Scanner::scan`] — the seam a real capability will hold — and
/// not through a concrete type the capability never sees.
pub struct ScriptedScanner {
    wizcli: Wizcli,
    /// Held for its [`Drop`], as [`GoWorkspace::root`] is — and read, unlike that
    /// one, because the child's record lands in it.
    scratch: TempDir,
}

#[async_trait::async_trait]
impl Scanner for ScriptedScanner {
    async fn scan(&self, image: &str) -> Result<ScanReport, ScanError> {
        self.wizcli.scan(image).await
    }
}

/// What the scripted scanner writes its record into, so a suite can assert that
/// a path the adapter handed the child points back inside this scan's own
/// directory rather than at something ambient.
const CHILD_RECORD: &str = "child.json";

impl ScriptedScanner {
    /// This scan's scratch directory.
    pub fn scratch(&self) -> &str {
        self.scratch
            .path()
            .to_str()
            .expect("a temporary directory whose path is UTF-8")
    }

    /// Every environment variable the child actually received.
    ///
    /// Read off the disk on each call rather than cached, so that a record from
    /// a scan that has not happened yet is a panic naming the missing file — a
    /// cached empty map would make "the child received nothing" indistinguishable
    /// from "nobody has scanned".
    ///
    /// A [`BTreeMap`], so the names come back in one order whatever order the
    /// operating system handed them over in: an allowlist assertion that had to
    /// sort its expectation to match would be an assertion nobody could read.
    pub fn child_env(&self) -> BTreeMap<String, String> {
        self.child()["env"]
            .as_array()
            .expect("the scripted scanner records its environment as an array")
            .iter()
            .map(|entry| {
                let entry = entry.as_str().expect("an environment entry is a string");
                // `splitn(2, ..)`, because a value may contain `=` and only the
                // first one separates a name from what it holds.
                let (name, value) = entry
                    .split_once('=')
                    .unwrap_or_else(|| panic!("{entry} is not a NAME=VALUE entry"));
                (name.to_string(), value.to_string())
            })
            .collect()
    }

    /// The names alone, in order. See [`ScriptedScanner::child_env`].
    pub fn child_env_names(&self) -> Vec<String> {
        self.child_env().into_keys().collect()
    }

    /// The child's whole `argv`, including the program itself.
    ///
    /// Whole, because the property asserted over it is that a value does *not*
    /// appear anywhere in it, and a record that dropped a position would be a
    /// record that could not have found the value there.
    pub fn child_argv(&self) -> Vec<String> {
        self.child()["argv"]
            .as_array()
            .expect("the scripted scanner records its argv as an array")
            .iter()
            .map(|argument| {
                argument
                    .as_str()
                    .expect("an argument is a string")
                    .to_string()
            })
            .collect()
    }

    /// The record itself, or a panic naming what is missing.
    ///
    /// Panics rather than returning an [`Option`], because every path that
    /// reaches it has already run a scan: an absent record means the child never
    /// started, and reporting that as an empty environment would turn a fixture
    /// that failed to spawn into a boundary assertion that passed.
    fn child(&self) -> serde_json::Value {
        let record = self.scratch.path().join(CHILD_RECORD);
        let raw = std::fs::read_to_string(&record).unwrap_or_else(|source| {
            panic!(
                "no record at {}, so no child of this scan was observed: {source}",
                record.display()
            )
        });
        serde_json::from_str(&raw)
            .unwrap_or_else(|source| panic!("{} is not a record: {source}", record.display()))
    }
}

/// Did asking the stub for `arm` actually reach the situation `arm` names?
///
/// The map from an arm to its outcome, in one place, so that every suite driving
/// the scripted scanner agrees about what each arm means. Two things are worth
/// reading rather than skimming:
///
/// **The first two arms are successes.** `exit-nonzero-with-file` is a scanner
/// that exited non-zero having written a perfectly good report, which is what an
/// organisation policy hit looks like — and a scan is judged by its artefact, not
/// by its status line. If that arm ever starts failing, the adapter has begun
/// reading the exit code first, and the capability will go dark the next time
/// somebody's tenant flags an unrelated finding.
///
/// **Which is exactly why this is not the whole check.** Those two arms share an
/// outcome by design, so this function cannot tell them apart and must not try:
/// the moment it could, the adapter would have to be discriminating on the status
/// line. What separates them is the status itself, and it is asserted by
/// [`arm_exits_with`] against [`observed_exit`] — see those two.
///
/// **An unknown arm panics.** Returning `false` would be a failing assertion in
/// the caller, which is a worse diagnostic: a typo in an arm name would read as
/// *the stub cannot produce this situation* rather than as *there is no such
/// situation*.
pub fn arm_was_exercised(arm: &str, outcome: &Result<ScanReport, ScanError>) -> bool {
    match arm {
        "ok" | "exit-nonzero-with-file" => outcome.is_ok(),
        // `leaks-its-credential` shares this outcome with the arm above it, and
        // must: what separates them is what the diagnostic *says*, not what the
        // scan came back as, so an arm list that told them apart here would be
        // asserting the wrong thing about both.
        "exit-nonzero-no-file" | "leaks-its-credential" => {
            matches!(outcome, Err(ScanError::Failed { .. }))
        }
        "empty-file" => matches!(outcome, Err(ScanError::NoOutput { .. })),
        "unparseable-file" => matches!(outcome, Err(ScanError::Unparseable { .. })),
        "no-such-image" => matches!(outcome, Err(ScanError::ImageAbsent { .. })),
        // Its own classification and not the neighbour above it. A host that is
        // down is not an image that does not exist: one is an obstacle a repeat
        // gets past and the other is a conclusion about the tag, and an arm list
        // that let them share a variant would be agreeing with the collapse this
        // arm exists to rule out.
        "no-daemon" => matches!(outcome, Err(ScanError::DaemonUnreachable { .. })),
        other => panic!("{other} is not an arm the scripted wizcli has; see ARMS"),
    }
}

/// The status line each arm is *defined* to end on.
///
/// Every arm's exit code is a deliberate choice in the stub and every one of them
/// is load-bearing, which is the reason this is a table over all of them rather than
/// a single assertion about the one arm that provoked it:
///
/// - **`exit-nonzero-with-file` exits 3.** Without this, that arm and `ok` are
///   indistinguishable from outside — [`arm_was_exercised`] maps both to a
///   successful report, correctly — and the fixture would still pass having
///   quietly stopped exiting non-zero at all. Then the suite's evidence for *the
///   artefact decides, not the status line* would be a scan that never had a
///   status line to ignore. 3 rather than 1 for the reason the stub gives at that
///   arm: 1 is what a generic failure exits with, so an assertion satisfied by 1
///   is not yet an assertion about a policy hit.
/// - **`empty-file` and `unparseable-file` exit 0.** Their claim is that a *bad
///   artefact alone* is refused, and a scanner that also exited non-zero would
///   leave the refusal attributable to either.
/// - **`exit-nonzero-no-file`, `no-such-image` and `no-daemon` exit 3**, matching
///   `exit-nonzero-with-file` on purpose: those four differ by artefact and
///   diagnostic while ending identically, which is what makes the adapter's
///   separation of them a fact rather than an exit-code lookup. `no-daemon` is
///   the newest of them and the one this matters most for: a daemon that is not
///   listening reaches a different exit row from the other two, and if the
///   status line could have been read for it, nothing would show that the
///   wording is what decided.
///
/// The arm names are matched exhaustively here for [`arm_was_exercised`]'s
/// reason, and an unknown one panics for the same one.
pub fn arm_exits_with(arm: &str) -> i32 {
    match arm {
        "ok" | "empty-file" | "unparseable-file" => 0,
        "exit-nonzero-with-file"
        | "exit-nonzero-no-file"
        | "no-such-image"
        | "no-daemon"
        | "leaks-its-credential" => 3,
        other => panic!("{other} is not an arm the scripted wizcli has; see ARMS"),
    }
}

/// What the operating system saw the stub exit with, asking it for `arm`.
///
/// # Why this runs the program a second time
///
/// The adapter reads the artefact first and consults the status only to
/// disambiguate its *absence*, so on a successful scan there is nothing in a
/// [`ScanReport`] that the exit code reached — and there must not be, or the
/// policy-hit arm stops being a case the adapter ignores. The status is therefore
/// only observable by running the program and looking, which is what this does.
///
/// It is still the subprocess contract: [`wiz_stub`] supplies the program and the
/// arm, exactly as a scan would, and nothing here links the stub as a library.
/// The two arguments added after it are the stub's own documented argv — a report
/// path and an image reference, both of which it requires of any caller — and not
/// a copy of how the adapter happens to build its command line. Deriving them
/// from the adapter would make this a test of `Wizcli`; what is under test here is
/// the fixture's ability to produce the situation.
///
/// Panics rather than returning an [`Option`], because a status with no code is a
/// death by signal: no arm has one, so it would mean the fixture crashed, and a
/// crash reported as a mismatched exit code sends the reader to the wrong file.
pub fn observed_exit(arm: &str) -> i32 {
    let scratch = TempDir::new().expect("a temporary directory for a scan's report");
    let stub = wiz_stub(arm);
    let output = std::process::Command::new(&stub.program)
        .args(&stub.args)
        .arg("--json-output-file")
        .arg(scratch.path().join("scan.json"))
        .arg(image())
        .output()
        .unwrap_or_else(|source| panic!("could not run the scripted wizcli for {arm}: {source}"));
    output.status.code().unwrap_or_else(|| {
        panic!(
            "the scripted wizcli died by signal on {arm}: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}
