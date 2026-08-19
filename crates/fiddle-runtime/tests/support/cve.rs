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
//! - Task 12.a adds the check contract and the trees it is judged over:
//!   [`contract`], [`contract_with`], [`green_tree`], [`tree_where`], [`exit`],
//!   [`stdout`] and the five command lines [`GO_BUILD`] … [`WIZCLI_RESCAN`].
//!   **Done.** An earlier version of this list assigned them to Task 11, which
//!   could never have added them: that task's scope was `crates/fiddle-cli`,
//!   and every one of these names a type — [`Check`], [`Success`], [`Tree`] —
//!   that `fiddle-runtime` had yet to build. Task 11 converged without them,
//!   which is what a signature-driven entry in this list is supposed to
//!   predict.
//! - Task 12.b adds [`contract_for`] and [`contract_scanned_by`], which are the
//!   rescan-condition builders rather than more of the above: one names the
//!   CVE ids a group must clear, the other the scanner version a rescan is
//!   compared against, and neither has anything to say about running commands.
//!   **Done**, together with the trees those contracts are judged over —
//!   [`tree_whose_rescan_reports`], [`tree_whose_rescan_reports_in_os_array`],
//!   [`tree_rescanned_by`] and [`tree_whose_rescan_is_unreadable`] — and
//!   [`and_the_input_also_reported`], which is what separates *this group's
//!   advisories* from *everything the input scan reported*. Task 12.a's
//!   [`contract`] and [`contract_with`] now answer a [`Contract`] rather than a
//!   bare check list, because a rescan cannot be judged without the premise it
//!   is compared against and a second argument alongside the checks would let a
//!   caller pair one attempt's premise with another's contract.
//!
//!   Then [`contract_for_a_partially_reported_rescan`] and the three worlds it
//!   is put to — [`tree_whose_rescan_omits_the_os_array`],
//!   [`tree_whose_rescan_reports_no_os_packages`] and
//!   [`tree_whose_rescan_omits_the_library_array`] — which are the first
//!   consumers of Task 6's absent-versus-empty distinction. The first two are
//!   [`report_with_os_absent`] and [`report_with_os_empty`] unmodified, so the
//!   pair really does differ in one key rather than in two documents that were
//!   written to look alike.
//! - Task 13 adds [`group_of`] and the five prior rescans a fold decision is
//!   taken against: [`rescan_from_committed_clean_group`],
//!   [`rescan_from_needs_work_group`],
//!   [`rescan_from_a_clean_group_that_was_not_committed`],
//!   [`rescan_from_a_committed_group_at_another_scanner_version`] and
//!   [`rescan_from_a_committed_group_that_reported_on_one_array`]. **Done.**
//!   They are `async` where every other builder here is not, and that is not a
//!   fold that needs awaiting: a [`PriorRescan`] is built from a real
//!   [`Evaluation`], so the "this group ended clean" half of each world is
//!   Task 12's judgement of a real tree rather than a flag this file set. The
//!   `await` is [`evaluate`]'s, and the rule under test is sync.
//! - Task 14.a adds [`MigrationWorld`] and [`migration_world`] — the world one
//!   bounded migration attempt runs in — together with [`document_of`],
//!   [`scan_of`] and [`scanned`], which move here from `cve_projection.rs`
//!   because that file's own note said they should the moment a second suite
//!   needed them. **Done.** The world is unusual among these builders in that
//!   three of its parts exist *to be leaked*: its document carries
//!   [`SENTINEL_PROSE`], its group's targets come from real [`attribute`] calls
//!   whose transcript names `go list -m -json`, and its worktree root is a path
//!   carrying [`HOST_ROOT`]. Task 14.a's whole criterion is a set of absences,
//!   and an absence is only evidence when the thing was there to be carried —
//!   see the section below on sentinels, and [`MigrationWorld`]'s own doc.
//! - Task 14.b adds [`MIGRATION_TEST_SOURCE`] and [`MIGRATION_TEST_BEFORE`] to
//!   that same world, and makes [`MIGRATION_SOURCE_BEFORE`] public. **Done.**
//!   Three of the four shapes the scope rules forbid are rules about a
//!   `_test.go` file, so a world holding only `main.go` could not reach them;
//!   and the *before* contents are public because a lane scripting an edit has
//!   to write the whole file, so a script that spelled the original out again
//!   would be a second copy to keep in step. See [`MIGRATION_TEST_BEFORE`]'s own
//!   doc for the one property of it that is easy to lose.
//! - Task 15 adds [`LandingWorld`] and [`landing_world`] — the tree one group's
//!   outcome is landed in — together with the four questions a landing lane asks
//!   of a tree it did not run git on ([`GoWorkspace::staged_paths`],
//!   [`GoWorkspace::head_commit_body`], [`GoWorkspace::all_commit_bodies`] and
//!   [`GoWorkspace::is_clean_at`]), [`GoWorkspace::try_git`], and the
//!   `impl Git for GoWorkspace` that makes the tree itself the subject's one
//!   spawn seam, plus [`LandingWorktree`], [`landing_worktree`] and [`ask_git`]
//!   for the one lane that drives the *production* adapter over a real
//!   [`Workspace`] instead. **Done.** Two of its parts exist to be *left alone*:
//!   [`LANDING_UNRELATED`] is dirty and outside the changed set, so staging by
//!   name and staging by directory produce different commits, and
//!   [`LANDING_CREATED`] is a file `git checkout` cannot put back. Neither is
//!   decoration — see each one's own doc.
//! - **Task 16 adds one thing, and it is in [`document`] rather than here:**
//!   [`unfixed_libraries`], a library array whose advisories name no published
//!   fix. Design §3's second row is the *fixable* set being empty while there is
//!   still something to report, and until now no builder in this family could
//!   produce a document that reached it — [`libraries`] writes a `fixedVersion`
//!   for every advisory it is given. It is in `document.rs` for that file's own
//!   reason: a scanner document is bytes a `[[bin]]` also has to be able to
//!   print.
//!
//!   Its seven **worlds** are local to `cve_dispositions.rs`, on 17.a's and
//!   18's stated precedent rather than in spite of the convention. Each is one
//!   `Run` value with a caller of exactly one, composed entirely out of pieces
//!   already here — [`report_with`], [`contract_for`], [`tree_whose_rescan_reports`],
//!   [`scanner_with`], [`available`] — so nothing about a document, a tree or an
//!   evaluation is spelled a second time. What a second suite would want from
//!   that lane is the *world builders*, and there is no second suite; if Task 20
//!   needs one, it moves then and there is one shape to move.
//! - **Task 17.a adds nothing here either, and for Task 18's reason rather than
//!   in spite of it.** This list assigned it `forge()` and the `scripted_gh_*`
//!   builders; it has neither, because the only suite that wants one is
//!   `cve_shared_pr.rs`, which already keeps a local `Forge` — and a second
//!   `forge()` here, used by nobody, is precisely the duplicate this list exists
//!   to prevent. What 17.a did instead was *widen the local one*: `empty()`,
//!   `seed_pull_request` with a named number, `seed_issue`, `gh()` on its own,
//!   and the two counts a duplicate hides between. If 17.b or Task 20 needs a
//!   forge from another suite, it moves then and there is one shape to move.
//!
//!   What every suite **does** inherit is in `gh_stub`, and it is additive:
//!   `GET /repos/{o}/{r}/issues?labels=…&state=open` answers the label search
//!   over pull requests *and* plain issues, distinguished by a `pull_request`
//!   key exactly as GitHub distinguishes them, and `issues_unfiltered` is its
//!   `pulls_unfiltered`. A seed entry may now **name its own number**, so a world
//!   can be arranged in which the lowest, the first and the last are three
//!   different pull requests; unnamed entries keep `7 + i` and skip the named
//!   ones. A create answers with the number it created, because a label is
//!   applied through `/issues/{n}/labels` and there is no way to address that
//!   without one. The by-number route falls back to the world when nothing is
//!   scripted for a number the world visibly holds — a number it does not hold
//!   is still a panic naming the file.
//! - **Task 18 adds nothing here, deliberately.** It landed before Task 17 and
//!   needed a forge, so the obvious move was to bring `forge()` forward. It did
//!   not: what a single-operation suite needs is a scratch directory for the
//!   scripted `gh` and a way to read its requests back, which is what
//!   `pull_request_effect.rs` and `ready_effect.rs` already each keep privately,
//!   and a `forge()` shaped to one operation's needs is worse than no `forge()` —
//!   Task 17 would have inherited a name it had to widen rather than a blank
//!   page. `cve_shared_pr.rs` therefore has a local `Forge` of its own, and Task
//!   17 owes it nothing. What Task 17 *does* inherit is in `gh_stub`: the
//!   by-number pull-request route now replays landed `PATCH` body rewrites over
//!   the seed, and `apply_effect` records a `PATCH` as a mutation. A `forge()`
//!   that arranges a shared pull request should seed `pulls_by_number/{n}.json`
//!   with a `body`, and can then read the rewrite back through the client rather
//!   than out of the fixture.
//! - **Task 17.b adds [`RemoteWorld`] and [`remote_world`]** — a clone whose
//!   local refs are deliberately stale against the remote they came from — and
//!   [`RemoteWorld::bump_into`], which writes a group's bump into a worktree the
//!   caller has already created. **Done.** It is here rather than beside
//!   `cve_shared_pr.rs` for the reason 17.a's `Forge` is not: this one is built
//!   out of [`shipped`], [`direct`], `DIRECT_MODULE` and `LANDING_BUMPED_VERSION`,
//!   all of which are private to this file, so a copy beside a suite would be a
//!   second spelling of the tree every landing lane is already judged over.
//!
//!   Its **remote is the caller's** and that is load-bearing: the scripted `gh`
//!   answers ref reads out of `remote.git` beside its own scratch directory, so
//!   the value of the world is that `git` and `gh` see one repository through two
//!   doors. See [`RemoteWorld`]'s own doc for the four commits it arranges and why
//!   every one of them has to be distinct.
//!
//!   17.b still adds no `forge()`, for 17.a's stated reason: the only suite that
//!   wants one is `cve_shared_pr.rs`, which widened its local one again — with a
//!   `remote.git` beside the stub directory, `Forge::seed_branch`, `Forge::pr`,
//!   `Forge::mutations` and a real [`FileJournal`] the effect steps are read back
//!   out of.
//!
//!   [`FileJournal`]: fiddle_runtime::journal::FileJournal
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

use fiddle_core::{AdvisoryId, AttemptId, PackageType, ProjectedFinding, Severities, Severity};
use fiddle_runtime::agent::AgentBudget;
use fiddle_runtime::capability::{CapabilityError, Git, MigrationConfig};
use fiddle_runtime::cve::attribute::{attribute, Manifest, ModuleGraph, ResolverError, Target};
use fiddle_runtime::cve::dedup::{DedupError, Local, Ran, Spawn};
use fiddle_runtime::cve::fold::{Landed, PriorRescan};
use fiddle_runtime::cve::go::Go;
use fiddle_runtime::cve::group::{group, Attributed, Group};
use fiddle_runtime::cve::project::project;
use fiddle_runtime::evaluate::{
    evaluate, Answered, Check, Contract, Evaluation, Repair, RescanVerdict, Success, Tree,
    Unanswered,
};
use fiddle_runtime::scanner::{ScanError, ScanReport, Scanner, WizCredential, Wizcli};
use fiddle_runtime::workspace::{Workspace, WorkspaceCommand, WorkspaceError, WorkspacePath};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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
pub use go_proxy::{
    run as offline_go, Answer as OfflineGo, CARRIED_BY_THE_VIABLE_LINE, REACHED_WITHOUT_THE_FIX,
    SWEEP_FIXED, SWEEP_MODULE, SWEEP_VULNERABLE,
};
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

    /// Does the tree match its `HEAD` **at these paths**?
    ///
    /// [`GoWorkspace::is_clean`] over a pathspec, and the distinction is the whole
    /// of Task 15's revert lane: a revert by explicit path puts back what it was
    /// given and must leave everything else exactly as dirty as it found it, so a
    /// world that could only ask about the whole tree could not tell
    /// `git checkout HEAD -- go.mod go.sum` from `git checkout .`.
    ///
    /// Not recorded, for [`GoWorkspace::is_clean`]'s reason.
    pub fn is_clean_at(&self, paths: &[&str]) -> bool {
        let mut args = vec!["status", "--porcelain", "--"];
        args.extend_from_slice(paths);
        run_git(&self.repo, &args).is_empty()
    }

    /// The paths the commit at `HEAD` carries, against its parent.
    ///
    /// **Read off the commit rather than off the `add` that made it.** What the
    /// staging criterion is about is what ended up on the branch, and a helper
    /// that parsed the recorded `git add` line would be asserting that the subject
    /// said the right thing rather than that the right thing happened — the two
    /// come apart for a subject that names its paths and then also runs `add -A`.
    /// The recorded call list is asserted separately and for the other half of the
    /// claim.
    ///
    /// `diff-tree` and not `show --name-only`, because `show` over a root commit
    /// lists the whole tree and would answer the same for a commit that staged
    /// everything.
    ///
    /// Not recorded, for [`GoWorkspace::is_clean`]'s reason.
    pub fn staged_paths(&self) -> Vec<String> {
        run_git(
            &self.repo,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"],
        )
        .lines()
        .map(|line| line.to_string())
        .collect()
    }

    /// The raw body — subject and message together — of the commit at `HEAD`.
    ///
    /// `%B` and not `%b`, matching what [`FixedInCommits`] is really read out of:
    /// `cve::dedup` runs `git log --format=%B`, so a subject line naming an
    /// advisory is one the next run sees. A helper that read only the message body
    /// would let an id hide in a subject.
    ///
    /// Not recorded, for [`GoWorkspace::is_clean`]'s reason.
    ///
    /// [`FixedInCommits`]: fiddle_runtime::cve::dedup::FixedInCommits
    pub fn head_commit_body(&self) -> String {
        run_git(&self.repo, &["log", "-1", "--format=%B"])
    }

    /// Every commit body in this repository's history, as one document.
    ///
    /// A `String` rather than a list, because that is the shape
    /// [`FixedInCommits::read`] takes — it is handed `git log --format=%B`'s whole
    /// output and splits it into words — and a lane that joined a list back
    /// together would be reconstructing the thing the subject is measured through.
    ///
    /// Not recorded, for [`GoWorkspace::is_clean`]'s reason.
    ///
    /// [`FixedInCommits::read`]: fiddle_runtime::cve::dedup::FixedInCommits::read
    pub fn all_commit_bodies(&self) -> String {
        run_git(&self.repo, &["log", "--format=%B"])
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
        self.try_git(args)
            .unwrap_or_else(|why| panic!("git {args:?} in {} failed: {why}", self.repo.display()))
    }

    /// The same, for a caller that has somewhere to put a failure.
    ///
    /// [`GoWorkspace::git`] panics because a fixture that failed quietly surfaces
    /// as an unrelated assertion further down; a *subject* running git has an
    /// error type of its own, and a seam that panicked would turn every refusal it
    /// is supposed to report into a downed test. Both spellings record through
    /// this one, so what a lane reads back is every invocation whichever way it
    /// was made.
    pub fn try_git(&self, args: &[&str]) -> Result<String, String> {
        self.calls.lock().unwrap().push(args.join(" "));
        try_run_git(&self.repo, args)
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
    try_run_git(dir, args)
        .unwrap_or_else(|why| panic!("git {args:?} in {} failed: {why}", dir.display()))
}

/// The same, answering the failure instead of panicking on it.
///
/// The one implementation, so the two spellings cannot disagree about what git
/// said — see [`GoWorkspace::try_git`] for who needs which.
///
/// Both streams are trimmed of their trailing newline: git ends everything with
/// one, and a caller comparing stdout to a path list would otherwise be comparing
/// it to a path list plus a blank line.
fn try_run_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|source| panic!("could not run git {args:?}: {source}"));
    let stdout = String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string();
    match output.status.success() {
        true => Ok(stdout),
        false => Err(String::from_utf8_lossy(&output.stderr)
            .trim_end()
            .to_string()),
    }
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
/// **This is not `cve_shared_pr.rs`'s `Forge`**, which is a scratch directory
/// for the scripted `gh` and the way a lane reads its requests back. That one
/// answers pull request and label queries for the lanes that legitimately make
/// them; this one proves an absence and needs no arms at all. Task 17.a widened
/// that one rather than bringing a `forge()` here — see the note in this
/// module's header — so nothing here should grow into it.
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
/// The first three are the arms a scan *succeeds* on. That
/// `exit-nonzero-with-file` is one of them is the whole of what this fixture is
/// for; see [`arm_was_exercised`].
///
/// `library-clean` is the only one whose meaning depends on another scan: it is
/// the *rescan* half, reporting the OS array unchanged and the library array
/// present and empty, which is what a repaired image looks like to the scanner
/// that was asked about it before. Nothing at this tier distinguishes it from
/// `ok` — both answer a readable document — and that is correct: what separates
/// them is what `evaluate`'s rescan conditions make of each, which is
/// `cve_evaluate`'s question and the acceptance suite's, not this array's.
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
pub const ARMS: [&str; 9] = [
    "ok",
    "library-clean",
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
        "ok" | "library-clean" | "exit-nonzero-with-file" => outcome.is_ok(),
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
        "ok" | "library-clean" | "empty-file" | "unparseable-file" => 0,
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

// ---------------------------------------------------------------------------
// The check contract, and the trees it is judged over
// ---------------------------------------------------------------------------

/// The five checks of design §2.6, spelled as their command lines.
///
/// Constants rather than literals at each use because a lane names the same
/// check three times in one test — in the contract, in the script that breaks
/// it, and in the assertion about which one broke — and three literals are
/// three chances for a typo to script a check nobody runs and assert about a
/// result nobody produced.
///
/// `go fmt ./...` is spelled the way the design spells it. It is the member of
/// the five whose criterion is not its exit status: it exits zero and names the
/// files it would rewrite, so the printed filename is the complaint. Nothing
/// anywhere reads that out of this string — see [`Success`] — and
/// [`WRAPPER`] is the fixture that proves it.
pub const GO_BUILD: &str = "go build ./...";

/// See [`GO_BUILD`]. The one whose output is the complaint.
pub const GO_FMT: &str = "go fmt ./...";

/// See [`GO_BUILD`].
pub const GO_VET: &str = "go vet ./...";

/// See [`GO_BUILD`].
///
/// With a context argument, which the design's enumeration leaves implicit:
/// `docker build` on its own is not an invocation, and a contract holding one
/// would be a fixture nobody could copy into a document.
pub const DOCKER_BUILD: &str = "docker build .";

/// See [`GO_BUILD`]. The rescan, and the only one of the five judged by
/// [`Success::ArtefactWritten`].
///
/// Two arguments and no more, because the rest of a `wizcli` invocation is the
/// adapter's: [`Wizcli::scan`] appends the policy flag, the output path and the
/// image itself. What a document writes down is the program and the
/// subcommand — the operator seam — which is exactly what this is.
pub const WIZCLI_RESCAN: &str = "wizcli docker scan";

/// An operator's formatter, pinned to an absolute path behind a wrapper script.
///
/// Nothing in it says `go` and nothing says `fmt`. It is the world in which
/// "the criterion travelled with the declaration" is a claim with a
/// counterexample available: a runner deriving its criterion from the program
/// name has nothing here to derive it from, and one that demanded no output
/// from everything would pass the inverse fixture — the check still spelled
/// [`GO_FMT`] and declared [`Success::ExitZero`] — which is why the pair is
/// only worth anything together.
pub const WRAPPER: &str = "/opt/acme/bin/tidy-sources --check";

/// The five checks a repaired tree is judged by, in the order they run.
///
/// All three criteria are represented, and that is what makes it a contract
/// rather than a list: [`Success::ExitZero`] for the build, the vet and the
/// image build, [`Success::ExitZeroAndNoOutput`] for the formatter, and
/// [`Success::ArtefactWritten`] for the rescan. A contract carrying one
/// criterion would let a runner that only understood exit statuses pass every
/// lane.
///
/// `go test ./...` is deliberately absent, and it is absent from the design for
/// a reason worth not re-deciding here: the suite needs a docker-compose
/// Postgres and a localstack under `-tags=external_deps`, and the runner starts
/// neither.
pub fn contract() -> Contract {
    Contract::of(vec![
        declared(GO_BUILD, Success::ExitZero),
        declared(GO_FMT, Success::ExitZeroAndNoOutput),
        declared(GO_VET, Success::ExitZero),
        declared(DOCKER_BUILD, Success::ExitZero),
        declared(WIZCLI_RESCAN, Success::ArtefactWritten),
    ])
}

/// [`contract`] with the check named `name` replaced by `command_line`,
/// declaring `success`.
///
/// A *replacement* and not an addition, so the contract stays five checks and a
/// lane's `checks().len()` assertion keeps meaning what it meant. It panics
/// when `name` is not one of the five, because the silent alternative is a
/// contract that grew a sixth entry and a suite that never noticed the fifth
/// was still the original.
pub fn contract_with(name: &str, command_line: &str, success: Success) -> Contract {
    let mut contract = contract();
    let at = contract
        .checks
        .iter()
        .position(|check| check.name() == name)
        .unwrap_or_else(|| panic!("{name} is not one of the five checks in the contract"));
    contract.checks[at] = declared(command_line, success);
    contract
}

/// The scanner version the rescan-condition worlds agree on unless a lane says
/// otherwise.
///
/// One value shared by [`contract_for`] and the trees below, so that the version
/// arm is *neutral* in every lane that is not about it: a fixture where the
/// input and the rescan happened to disagree would make every condition (a) and
/// (b) lane provisional, and the two conditions would then be asserted through
/// a disposition they never reach.
///
/// Not [`wiz_stub`]'s `0.0.0-fiddle-stub`, and it does not need to be: the trees
/// below hand a report over rather than spawning that program. See
/// [`tree_whose_rescan_reports`].
const FIXTURE_SCANNER_VERSION: &str = "1.2.3";

/// The image digest a handed-over report is filed under.
///
/// Nothing asserts on it. It is here because [`ScanReport`] has no default and a
/// blank digest would be a value that looks like a fixture forgot something, in
/// a field the rescan conditions deliberately do not read.
const FIXTURE_DIGEST: &str =
    "sha256:6f1b0d2c9a4e7385bd1c05fa9e37642c8b0d5713ae629f04c8d17b6a3e59042d";

/// The advisory a rescan-condition world is about, where the lane does not care
/// which advisory it is.
///
/// [`FIXTURE_ADVISORY`] is deliberately not reused: that one is what the
/// attribution and grouping families file everything under, and a rescan lane
/// asserting *this id is gone* over the same value would be one edit away from
/// agreeing with an unrelated fixture by accident.
const REPAIRED_ADVISORY: &str = "CVE-2026-4242";

/// [`contract`], for a group that set out to clear `cves`.
///
/// The input scan is taken to have reported exactly `cves` and nothing else,
/// which is the simple world and the one condition (b) is sharpest in: anything
/// the rescan reports is then something that appeared. A lane that needs the
/// other shape — an input scan carrying findings this group is not fixing —
/// wraps this in [`and_the_input_also_reported`], and the pair is what keeps
/// condition (b) from being satisfied by "the rescan is empty".
///
/// The scanner version is [`FIXTURE_SCANNER_VERSION`] on both sides, so a lane
/// about the two conditions is not also a lane about provenance.
pub fn contract_for(cves: &[&str]) -> Contract {
    let mut contract = contract();
    contract.repair = Some(Repair {
        must_clear: advisories(cves),
        input: advisories(cves),
        scanned_at: FIXTURE_SCANNER_VERSION.to_string(),
    });
    contract
}

/// The same contract, where the input scan **also** reported `cves` — findings
/// some other group owns.
///
/// It widens condition (b)'s baseline and leaves condition (a)'s list alone,
/// because that is the real difference between the two: a repair is judged on
/// the advisories *it* set out to clear, and on whether anything appeared that
/// the whole image did not already have.
pub fn and_the_input_also_reported(mut contract: Contract, cves: &[&str]) -> Contract {
    let repair = contract
        .repair
        .as_mut()
        .expect("a contract with a repair premise to widen");
    repair.input.extend(advisories(cves));
    contract
}

/// [`contract_for`] where what varies is the scanner version the input scan ran
/// at.
///
/// One advisory to clear, and which one does not matter — what the lane reading
/// this is about is whether the *comparison* between two scanner versions
/// decides anything. The advisory is there because a premise clearing nothing
/// would satisfy condition (a) vacuously, and then the provisionality assertion
/// would be about a rescan that had proved nothing to qualify.
pub fn contract_scanned_by(version: &str) -> Contract {
    let mut contract = contract_for(&[REPAIRED_ADVISORY]);
    contract
        .repair
        .as_mut()
        .expect("contract_for supplies a repair premise")
        .scanned_at = version.to_string();
    contract
}

/// The premise the three one-array-missing worlds are judged under.
///
/// **Every condition is arranged to hold, so that the array is the only thing
/// left that can decide anything.** `must_clear` is [`REPAIRED_ADVISORY`], which
/// none of Task 6's documents reports, so condition (a) is satisfied; the input
/// is widened to every advisory those documents *do* carry, so condition (b) is
/// too; and the scanner version matches on both sides, so the comparison is not
/// provisional either. A lane that skipped the widening would see its tree
/// refused for reporting a finding that appeared, and would be asserting
/// nothing about the missing array.
///
/// The widening reads the constants rather than restating their values: a lane
/// naming `CVE-2026-0001` here would go quietly wrong the day
/// [`DEFAULT_LIBRARY_CVES`] changed.
pub fn contract_for_a_partially_reported_rescan() -> Contract {
    let mut already_reported: Vec<&str> = DEFAULT_LIBRARY_CVES.to_vec();
    already_reported.extend(DEFAULT_OS_CVES);
    and_the_input_also_reported(contract_for(&[REPAIRED_ADVISORY]), &already_reported)
}

/// Canonical advisory ids, parsed the way every other value of this type is.
fn advisories(cves: &[&str]) -> Vec<AdvisoryId> {
    cves.iter()
        .map(|cve| AdvisoryId::parse(cve).expect("a fixture advisory id parses"))
        .collect()
}

/// One check, from the command line an operator would write and the criterion
/// they would declare beside it.
///
/// The split on whitespace is this fixture's convenience and is deliberately
/// not something the product does: `fiddle_cli::config::CheckRef` takes the
/// program and its arguments already separated, precisely because a shell
/// string has to be split by somebody and every splitter is wrong about quoting
/// somewhere. What a lane wants to read is one string that recomposes to
/// [`Check::name`], and no fixture command line here has a quoted argument in
/// it.
fn declared(command_line: &str, success: Success) -> Check {
    let mut words = command_line.split_whitespace().map(str::to_string);
    let program = words
        .next()
        .unwrap_or_else(|| panic!("a check needs a program, and {command_line:?} names none"));
    Check {
        program,
        args: words.collect(),
        success,
    }
}

/// A scripted exit status. See [`ScriptedTree::where_check`].
#[derive(Debug)]
pub struct Exit(i32);

/// The exit status a scripted check leaves.
///
/// A newtype rather than a bare `i32` beside a bare `&str`, so that
/// `where_check(GO_FMT, exit(0), stdout("main.go\n"))` cannot be written with
/// its last two arguments the wrong way round: the transposition is a type
/// error rather than a test that scripts something nobody meant.
pub fn exit(code: i32) -> Exit {
    Exit(code)
}

/// Scripted standard output. See [`ScriptedTree::where_check`].
#[derive(Debug)]
pub struct Stdout(String);

/// What a scripted check prints. See [`exit`] for why it is a type.
pub fn stdout(text: &str) -> Stdout {
    Stdout(text.to_string())
}

/// What a tree does when a particular check is run in it.
#[derive(Debug)]
enum Scripted {
    /// It ran, and left this behind.
    Answered { exit_code: i32, stdout: String },
    /// It never started, because the program is not on this machine.
    CannotStart,
}

/// A tree that answers the contract however a lane scripted it, and remembers
/// what it was asked to start.
///
/// # Why the world is scripted here where `dedup`'s is real
///
/// The Go trees above are real directories because a git history is cheap to
/// build and the subject reads one. This is the opposite situation: a tree in
/// which `docker build` fails and `go vet` passes cannot be built offline, and
/// neither can one where the container daemon is missing — and those situations
/// *are* the contract's subject. So the world is scripted and the seam is
/// [`Tree`], which is also what makes [`ScriptedTree::ran`] possible.
///
/// # An unscripted check passes
///
/// A tree scripts only what a lane wants to go wrong; everything else exits
/// zero, prints nothing, and passes. That makes [`green_tree`] the default and
/// a failure the thing a lane writes down, which is the right way round — but
/// it also means a *mis-spelled* name scripts nothing and leaves a green tree.
/// Every lane that scripts a failure asserts which check failed, so the
/// mis-spelling arrives as "expected a failure, found none" rather than as a
/// pass; and [`ScriptedTree::where_check`] refuses to script the same check
/// twice, which is the other half of the same worry.
/// What answers an [`Success::ArtefactWritten`] check in a scripted tree.
///
/// # Why there are two, and why the second is not a shortcut
///
/// [`Scanned::ByProgram`] is the ordinary one and the one the contract's own
/// claims rest on: a real [`Wizcli`] over the scripted `wizcli`, really spawned,
/// so *success is the artefact and not the status line* is decided by the code
/// that owns that rule.
///
/// The rescan conditions cannot be reached through it, and not for want of
/// trying. What those lanes vary is the **content** of the document and the
/// **version** the scanner announces, and both are fixed in that program: its
/// arms are a closed list ([`ARMS`]) whose documents come from
/// `document.rs`, and its banner announces one build-time constant. Reaching
/// them through the stub would mean either an arm per advisory set or an
/// environment channel the adapter's allowlist is specifically there to refuse.
///
/// So those worlds hand a [`ScanReport`] over directly. What is under test in
/// them is the *reading* of a report — two conditions and a version comparison,
/// none of which involves a process — and the spawning half is already measured
/// where it belongs: by `an_artefact_check_passes_at_a_non_zero_exit` here and
/// by the whole of `cve_evaluate_spawn`. The document is still built by
/// `document.rs`'s shared builders, so a handed-over report is the same bytes
/// the projection lanes assert against rather than a second scanner document.
enum Scanned {
    /// A real adapter over the scripted `wizcli`.
    ByProgram(ScriptedScanner),
    /// A report handed over without a child.
    AsReport(ScanReport),
}

pub struct ScriptedTree {
    /// By [`Check::name`], and only the checks a lane departed from.
    scripted: BTreeMap<String, Scripted>,
    /// What an [`Success::ArtefactWritten`] check reaches. See [`Scanned`].
    scanner: Scanned,
    /// Every check this tree was asked to start, in order.
    ///
    /// A [`Mutex`] because the subject holds this by shared reference — a
    /// runner that needed `&mut` to run a check would be a runner nobody could
    /// hold a recorder of. It is the arrangement [`RecordedCalls`] is under.
    ran: Mutex<Vec<String>>,
}

/// A tree that passes every check in [`contract`].
///
/// **The positive control, and the base every other tree is a departure from.**
/// Without it every rejection a lane asserts is satisfied by a runner that
/// rejects everything, and `rejected()` measures nothing.
///
/// Its scanner is the scripted `wizcli`'s `ok` arm, so the fifth check passes
/// the way the other four do: by the criterion it declared, over a program that
/// really ran.
pub fn green_tree() -> ScriptedTree {
    ScriptedTree {
        scripted: BTreeMap::new(),
        scanner: Scanned::ByProgram(scanner_with(wiz_stub("ok"))),
        ran: Mutex::new(Vec::new()),
    }
}

/// A [`green_tree`] whose rescan reports `cves` against **library** packages,
/// at the scanner version the fixture worlds agree on.
///
/// An empty list is a rescan that found nothing, which is what a repair that
/// worked looks like. See [`Scanned`] for why this world hands a report over
/// rather than spawning the scripted `wizcli`.
pub fn tree_whose_rescan_reports(cves: &[&str]) -> ScriptedTree {
    tree_reporting(
        report_with(libraries(cves), os_packages(&[])),
        FIXTURE_SCANNER_VERSION,
    )
}

/// The same, with `cves` in the **`osPackages`** array and nothing in
/// `libraries`.
///
/// The half that catches a condition (a) reading one array: an id surviving here
/// is not gone, and a reader that only walked `libraries` would call this tree
/// repaired. It is `crate::cve::project`'s `both_package_arrays_are_read` asked
/// one layer up, against the rule rather than against the projection.
pub fn tree_whose_rescan_reports_in_os_array(cves: &[&str]) -> ScriptedTree {
    tree_reporting(
        report_with(libraries(&[]), os_packages(cves)),
        FIXTURE_SCANNER_VERSION,
    )
}

/// A [`green_tree`] whose rescan found nothing and says `version` did the
/// looking.
///
/// The clean report is the point: with both conditions satisfied, the scanner
/// version is the only thing left that can decide whether the absence is proof.
pub fn tree_rescanned_by(version: &str) -> ScriptedTree {
    tree_reporting(report_with(libraries(&[]), os_packages(&[])), version)
}

/// A [`green_tree`] whose rescan wrote a document with **no `osPackages` key**.
///
/// Task 6's own fixture, unmodified. Its pair is
/// [`tree_whose_rescan_reports_no_os_packages`], which is the same document with
/// the key present and holding no packages — the two differ in nothing else, so
/// a lane running both is asking exactly *does the key's absence mean something
/// its emptiness does not*. Neither is buildable from
/// [`tree_whose_rescan_reports`], because an array a caller passes is an array
/// the document carries.
pub fn tree_whose_rescan_omits_the_os_array() -> ScriptedTree {
    tree_reporting(report_with_os_absent(), FIXTURE_SCANNER_VERSION)
}

/// A [`green_tree`] whose rescan reported on `osPackages` and found none — the
/// distroless shape, and the positive half of the pair above.
///
/// Without it, "an absent array is not proof" is indistinguishable from "an
/// image with no OS findings can never be proved repaired", which would refuse
/// every distroless runtime forever.
pub fn tree_whose_rescan_reports_no_os_packages() -> ScriptedTree {
    tree_reporting(report_with_os_empty(), FIXTURE_SCANNER_VERSION)
}

/// A [`green_tree`] whose rescan wrote a document with **no `libraries` key**.
///
/// The mirror of [`tree_whose_rescan_omits_the_os_array`], and it is not
/// redundant for the reason [`tree_whose_rescan_reports_in_os_array`]'s sibling
/// is not: a rule that named one array would leave the other half of the image
/// readable as clear from silence, and only a lane per side can tell a rule
/// about *an unreported array* from a rule about `osPackages`.
pub fn tree_whose_rescan_omits_the_library_array() -> ScriptedTree {
    tree_reporting(report_with_libraries_absent(), FIXTURE_SCANNER_VERSION)
}

/// [`tree_whose_rescan_reports`]'s world with the `osPackages` key taken out
/// altogether: an advisory still reported in `libraries`, and silence about the
/// other half of the image.
///
/// Task 6's [`report_with_os_absent`] cannot serve here — its library array is
/// fixed, and this world needs a *named* advisory in it — so the key is removed
/// from a real document rather than a second one being written out by hand, for
/// [`tree_whose_rescan_is_unreadable`]'s reason.
pub fn tree_whose_rescan_omits_the_os_array_and_reports(cves: &[&str]) -> ScriptedTree {
    let mut report = rescan_report(
        report_with(libraries(cves), os_packages(&[])),
        FIXTURE_SCANNER_VERSION,
    );
    report.document["result"]
        .as_object_mut()
        .expect("a fixture scanner document's result is an object")
        .remove("osPackages");
    ScriptedTree {
        scripted: BTreeMap::new(),
        scanner: Scanned::AsReport(report),
        ran: Mutex::new(Vec::new()),
    }
}

/// A [`green_tree`] whose rescan wrote a document this build cannot read as a
/// scan report.
///
/// `result.libraries` is an object where a list of packages belongs — which is a
/// document that parses as JSON and is still not a report, so the scanner's own
/// artefact rule is satisfied and the failure is the projection's. Built by
/// taking a real document and replacing that one key, rather than by writing a
/// broken document out by hand: a second literal here would be a second scanner
/// document, which is the thing this file's header rules out.
pub fn tree_whose_rescan_is_unreadable() -> ScriptedTree {
    let mut report = rescan_report(
        report_with(libraries(&[]), os_packages(&[])),
        FIXTURE_SCANNER_VERSION,
    );
    report.document["result"]["libraries"] = serde_json::json!({});
    ScriptedTree {
        scripted: BTreeMap::new(),
        scanner: Scanned::AsReport(report),
        ran: Mutex::new(Vec::new()),
    }
}

/// A [`green_tree`] whose rescan answers with `document`, filed under `version`.
fn tree_reporting(document: Report, version: &str) -> ScriptedTree {
    ScriptedTree {
        scripted: BTreeMap::new(),
        scanner: Scanned::AsReport(rescan_report(document, version)),
        ran: Mutex::new(Vec::new()),
    }
}

/// One scan's report: the shared document's bytes, parsed, and the provenance a
/// real scan would have read off the child's banner.
///
/// The bytes are round-tripped through the string form rather than built as a
/// `Value` here, so the document under test is exactly what the scripted
/// `wizcli` would have written to disk — the same construction, not a similar
/// one.
fn rescan_report(document: Report, version: &str) -> ScanReport {
    ScanReport {
        document: serde_json::from_str(document.raw())
            .expect("a fixture scanner document is valid JSON"),
        scanner_version: version.to_string(),
        image_digest: FIXTURE_DIGEST.to_string(),
    }
}

/// A [`green_tree`] in which the check named `name` does this instead.
///
/// The spelling `green_tree().where_check(..)` says the same thing; this one
/// exists because what a lane with one scripted check means is a whole world
/// rather than a departure it then applies — the same distinction
/// [`go_with_shipped`] draws.
pub fn tree_where(name: &str, exit: Exit, stdout: Stdout) -> ScriptedTree {
    green_tree().where_check(name, exit, stdout)
}

impl ScriptedTree {
    /// This tree, except that the check named `name` exits `exit` and prints
    /// `stdout`.
    ///
    /// Panics when `name` is already scripted: two scripts for one check are
    /// two things a lane believes about it, and silently keeping the second
    /// would make `first_failure_is_the_earliest_in_declared_order` — the one
    /// lane that scripts two checks — pass over a tree with one.
    pub fn where_check(mut self, name: &str, exit: Exit, stdout: Stdout) -> Self {
        self.script(
            name,
            Scripted::Answered {
                exit_code: exit.0,
                stdout: stdout.0,
            },
        );
        self
    }

    /// This tree, except that the check named `name` cannot be started at all.
    ///
    /// The program is not on this machine — an uninstalled `docker`, not a
    /// `docker build` that failed. The two are opposite remedies and the whole
    /// reason [`Unanswered::NotStarted`] is not an exit status: one is an
    /// operator's laptop to fix and the other is the repair to revert.
    pub fn where_check_cannot_start(mut self, name: &str) -> Self {
        self.script(name, Scripted::CannotStart);
        self
    }

    /// This tree, scanned by the scripted `wizcli`'s `arm` arm.
    ///
    /// `arm` is one of [`ARMS`]. The two that matter to the contract are
    /// `exit-nonzero-with-file`, which exits 3 and writes its report anyway —
    /// what `wizcli` does when it reports findings — and `exit-nonzero-no-file`,
    /// which exits the same way and leaves nothing.
    pub fn scanned_by(mut self, arm: &str) -> Self {
        self.scanner = Scanned::ByProgram(scanner_with(wiz_stub(arm)));
        self
    }

    /// Every check this tree was asked to start, in order.
    ///
    /// What was *asked for*, not what succeeded: a check that could not start
    /// is in here, because the claim this list carries is about the runner
    /// having issued five separate commands rather than about what came back.
    /// The result list is the other half and cannot show this one — five
    /// results could be five copies of one status.
    pub fn ran(&self) -> Vec<String> {
        self.ran.lock().unwrap().clone()
    }

    fn script(&mut self, name: &str, scripted: Scripted) {
        if let Some(already) = self.scripted.insert(name.to_string(), scripted) {
            panic!("{name} was already scripted as {already:?}");
        }
    }
}

#[async_trait::async_trait]
impl Tree for ScriptedTree {
    async fn run(&self, check: &Check) -> Result<Answered, Unanswered> {
        self.ran.lock().unwrap().push(check.name());
        match self.scripted.get(&check.name()) {
            Some(Scripted::CannotStart) => Err(Unanswered::NotStarted {
                program: check.program.clone(),
                // The kind and not a message, because the runner is what turns
                // a kind into the sentence a record carries — see
                // `fiddle_runtime::evaluate`. A fixture that wrote the sentence
                // would be a fixture the assertion was really about.
                source: std::io::Error::from(std::io::ErrorKind::NotFound),
            }),
            Some(Scripted::Answered { exit_code, stdout }) => Ok(Answered {
                exit_code: *exit_code,
                stdout: stdout.clone(),
                stderr: String::new(),
            }),
            // Unscripted is green. See the type's own doc.
            None => Ok(Answered {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            }),
        }
    }

    /// The arm of this fixture that can really spawn something.
    ///
    /// On [`Scanned::ByProgram`] it goes through the real [`Wizcli`] adapter
    /// over the scripted `wizcli`, so *success is the artefact, not the status
    /// line* is decided by the code that owns that rule rather than restated
    /// here. What the check declared is what routed the runner to this method;
    /// what it *names* is [`WIZCLI_RESCAN`], and the program actually started is
    /// the stub [`scanned_by`] put behind the operator seam — the same
    /// substitution every other scanner lane in this crate makes, and the reason
    /// the check's own `program` is recorded rather than executed.
    ///
    /// On [`Scanned::AsReport`] there is no child, and the check is recorded all
    /// the same: what that list carries is the claim that the runner *issued*
    /// five separate commands, and a world that answered one of them without
    /// spawning has still been asked.
    ///
    /// [`scanned_by`]: ScriptedTree::scanned_by
    async fn scan(&self, check: &Check) -> Result<ScanReport, ScanError> {
        self.ran.lock().unwrap().push(check.name());
        match &self.scanner {
            Scanned::ByProgram(scanner) => scanner.scan(&image()).await,
            Scanned::AsReport(report) => Ok(report.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// The fold rule's worlds (Task 13)
// ---------------------------------------------------------------------------

/// The group a fold decision is *about*, naming `cves`.
///
/// One target for all of them, because that is what makes this one group rather
/// than several: `cve::group` keys on [`Target`] alone, so findings sharing a
/// target are one edit. The package varies with the advisory only so that no
/// two findings in a group are the same value — nothing in the fold reads it.
///
/// Panics unless exactly one group comes back, which is the premise the lane
/// depends on and not an assertion it makes: a builder that quietly returned the
/// first of two would let a lane about *every id in this group* run against a
/// group holding one of them.
pub fn group_of(cves: &[&str]) -> Group {
    let findings: Vec<Attributed> = cves
        .iter()
        .map(|cve| attributed(cve, &format!("package-for-{cve}"), FOLD_TARGET))
        .collect();
    let mut groups = group(&findings);
    assert_eq!(
        groups.len(),
        1,
        "a fold lane's group is one edit, and {cves:?} produced {} of them",
        groups.len()
    );
    groups.remove(0)
}

/// Every advisory a world's group is about, as [`land`] and
/// [`GroupMigration::migrate`] now take them.
///
/// M4c took the `Group` out of both signatures: a run shows one attempt every
/// finding its bound left, so what the landing and the prompt need is the
/// findings and their ids rather than the bump target four mechanical rules
/// elected. These two are the adapter for the worlds in this file, which still
/// build a group because the fold lanes are still about one — see
/// [`group_of`]. When `cve::group` goes, both of these go with it and the worlds
/// hold their findings directly.
///
/// [`land`]: fiddle_runtime::capability::land
/// [`GroupMigration::migrate`]: fiddle_runtime::capability::GroupMigration::migrate
pub fn advisories_of(group: &Group) -> Vec<AdvisoryId> {
    group.cves().into_iter().cloned().collect()
}

/// Every finding in `group`, as an attempt is now shown them. See
/// [`advisories_of`].
pub fn shown_findings(group: &Group) -> Vec<ProjectedFinding> {
    group
        .findings()
        .iter()
        .map(|attributed| attributed.finding().clone())
        .collect()
}

/// The grade set every fixture world in this file reads its documents through:
/// what a document naming no grades means.
///
/// Named rather than spelled out at each call site, and the name says which of
/// two things it is. Every fixture finding here is `HIGH` or `CRITICAL` — see
/// `document.rs`'s `vulnerability` and [`finding_under`] — so the default admits
/// all of them, and none of these lanes is about the grade set. A lane that *is*
/// about it names its own set, and `cve_projection`'s
/// `a_deployment_that_names_a_lower_grade_projects_it` is the one that does.
pub fn every_fixture_grade() -> Severities {
    Severities::default()
}

/// The module every [`group_of`] finding is attributed to.
///
/// Its own value rather than one of the attribution family's, so that a fold
/// lane cannot start passing because some unrelated fixture's target happens to
/// agree with it.
const FOLD_TARGET: &str = "example.com/folded";

/// The advisory the *earlier* group in a fold lane set out to clear.
///
/// Never one of the ids a fold lane then asks about: the question is always
/// whether a **later** group's advisories are covered by this bump, and reusing
/// the id would make the answer true for the wrong reason — the earlier group
/// cleared its own advisory by definition.
const EARLIER_GROUPS_ADVISORY: &str = "CVE-2026-9001";

/// The rescan left behind by a group that ended clean and whose bump was
/// committed — the one provenance the fold rule may rest on.
///
/// `still_reported` is what the rescan *still* found: an empty list is an image
/// with nothing left in it, and a non-empty one is the findings some later group
/// still owns. Those ids are widened into the contract's input so that condition
/// (b) does not read them as findings that appeared — without that, this world
/// would be refused for the wrong reason and the lane would be asserting nothing
/// about the fold.
///
/// The premise is asserted rather than assumed. A world that quietly stopped
/// being accepted would turn `a_group_cleared_by_an_earlier_committed_bump…`
/// into a lane that passes against a rule that never folds.
pub async fn rescan_from_committed_clean_group(still_reported: &[&str]) -> PriorRescan {
    let evaluation = cleanly_evaluated(still_reported).await;
    assert!(
        evaluation.accepted(),
        "this world's premise is a group that ended clean"
    );
    PriorRescan::of(&evaluation, Landed::Committed, &every_fixture_grade())
}

/// The same rescan, from a group that ended **needs-work** — so its bump was
/// reverted and the branch does not carry the tree this document describes.
///
/// The group is needs-work because `go vet` failed, and for nothing to do with
/// the rescan: its verdict is still [`RescanVerdict::Cleared`], which is the
/// sharpest form of the hazard. The document is an accurate account of a tree
/// that no longer exists, and everything about it *looks* foldable.
///
/// Both halves of that premise are asserted, because a world that failed to be
/// clean-looking would make the lane pass for the ordinary reason instead of the
/// interesting one.
pub async fn rescan_from_needs_work_group(still_reported: &[&str]) -> PriorRescan {
    let evaluation = evaluate(
        &contract_for_a_fold(still_reported),
        &tree_whose_rescan_reports(still_reported).where_check(GO_VET, exit(1), stdout("")),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert_eq!(
        evaluation.rescan(),
        &RescanVerdict::Cleared,
        "the rescan itself is clean — the group is needs-work for another reason"
    );
    assert!(
        !evaluation.accepted(),
        "a failed check is what makes this group needs-work"
    );
    PriorRescan::of(&evaluation, Landed::Reverted, &every_fixture_grade())
}

/// A rescan from a group that ended clean and whose bump was **not** committed.
///
/// The other side of [`rescan_from_needs_work_group`], and the reason the two
/// facts are two: a clean group whose commit did not happen leaves exactly the
/// same hazard as a reverted one — a rescan describing a tree the branch does
/// not carry — and a rule that inferred the branch from the verdict would fold
/// on it.
pub async fn rescan_from_a_clean_group_that_was_not_committed(
    still_reported: &[&str],
) -> PriorRescan {
    let evaluation = cleanly_evaluated(still_reported).await;
    assert!(evaluation.accepted());
    PriorRescan::of(&evaluation, Landed::Reverted, &every_fixture_grade())
}

/// A rescan whose absences were observed through a **different scanner
/// version**, from a group whose bump *was* committed.
///
/// Committed and not reverted, deliberately. [`RescanVerdict::Provisional`] is
/// not a refusal over in `evaluate` — nothing went wrong with the tree — so a
/// disposition may well keep the bump and flag it, which makes this a reachable
/// state rather than a contrived one. It is the world that proves the fold's
/// clean gate does something the branch gate does not.
pub async fn rescan_from_a_committed_group_at_another_scanner_version() -> PriorRescan {
    let evaluation = evaluate(
        &contract_scanned_by("wizcli/0.0.0-the-version-the-input-was-scanned-at"),
        &tree_rescanned_by(FIXTURE_SCANNER_VERSION),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(
        matches!(evaluation.rescan(), RescanVerdict::Provisional(_)),
        "this world's premise is an absence seen through a moved feed"
    );
    PriorRescan::of(&evaluation, Landed::Committed, &every_fixture_grade())
}

/// A rescan whose document carried **no `osPackages` key at all**, from a group
/// whose bump was committed.
///
/// The scanner did not report that the OS findings were gone; it said nothing
/// about OS packages. Every id a fold might ask about is therefore absent from
/// this document for free, which is the purest form of claiming proof from
/// silence — and committed, for [`rescan_from_a_committed_group_at_another_scanner_version`]'s
/// reason.
pub async fn rescan_from_a_committed_group_that_reported_on_one_array() -> PriorRescan {
    let evaluation = evaluate(
        &contract_for_a_partially_reported_rescan(),
        &tree_whose_rescan_omits_the_os_array(),
    )
    .await
    .expect("an evaluation that was not cancelled");

    assert!(
        matches!(evaluation.rescan(), RescanVerdict::NotObserved { .. }),
        "this world's premise is an array the scanner never reported on"
    );
    PriorRescan::of(&evaluation, Landed::Committed, &every_fixture_grade())
}

/// An evaluation that is accepted, over a rescan still reporting `still_reported`.
fn contract_for_a_fold(still_reported: &[&str]) -> Contract {
    and_the_input_also_reported(contract_for(&[EARLIER_GROUPS_ADVISORY]), still_reported)
}

/// The shared half of the two worlds that end clean: five green checks over a
/// rescan reporting exactly `still_reported`.
async fn cleanly_evaluated(still_reported: &[&str]) -> Evaluation {
    evaluate(
        &contract_for_a_fold(still_reported),
        &tree_whose_rescan_reports(still_reported),
    )
    .await
    .expect("an evaluation that was not cancelled")
}

// ---------------------------------------------------------------------------
// Putting a document where a scan would have left one
// ---------------------------------------------------------------------------
//
// These three lived in `cve_projection.rs` while that suite was their only
// caller, with a note saying they belonged here the moment a second one needed
// them. Task 14.a is that second one: a migration's group is projected from a
// real document, so the bytes a builder produced have to cross the same type
// boundary into [`project`], which takes a [`ScanReport`] because that is what
// a real capability holds.

/// The document a fixture wrote, parsed.
pub fn document_of(report: &Report) -> serde_json::Value {
    serde_json::from_str(report.raw()).expect("a fixture document is JSON")
}

/// A scan that produced `document`.
///
/// The provenance is fixed and uninteresting: no assertion reads it, because a
/// projection is a function of the document alone. It is spelled implausibly on
/// purpose, so that a value from here turning up in an assertion about a real
/// scan would be visible rather than plausible.
pub fn scan_of(document: serde_json::Value) -> ScanReport {
    ScanReport {
        document,
        scanner_version: "wizcli 0.0.0-fixture".to_string(),
        image_digest: "sha256:fixture".to_string(),
    }
}

/// The two above, for the ordinary case where a test wants a fixture scanned.
pub fn scanned(report: &Report) -> ScanReport {
    scan_of(document_of(report))
}

// ---------------------------------------------------------------------------
// The world one bounded migration attempt runs in (Task 14.a)
// ---------------------------------------------------------------------------

/// The attempt every migration lane runs under.
///
/// Fixed rather than minted, so that a worktree path — and anything derived from
/// one — is a function of the run rather than of the clock.
pub const MIGRATION_ATTEMPT: &str = "01JQZX00000000000000000M4";

/// How long the scripted `go` behind a migration's `run_check` may take.
const MIGRATION_CHECK_TIMEOUT: Duration = Duration::from_secs(60);

/// The one Go source a migration world's tree holds.
///
/// [`write_tree`] writes `go.mod` and `go.sum` and nothing else, because every
/// lane before this one asked the tree about its module graph. A migration is an
/// edit to *source*, so this world adds a file for there to be a call site in —
/// added here rather than in [`write_tree`], which is shared with the attribution
/// lanes and whose committed path list is what their `is_clean` assertions are
/// measured against.
pub const MIGRATION_SOURCE: &str = "main.go";

/// What that file holds before the migration: one call site, in the shape a
/// forced rename would have to reach.
pub const MIGRATION_SOURCE_BEFORE: &str = "\
package main

func main() {
\tlegacyName()
}

func legacyName() {}
";

/// The test file a migration world's tree also holds (Task 14.b).
///
/// Added because three of Task 14.b's four forbidden shapes are rules **about a
/// `_test.go` file** — an added `t.Skip`, a changed or removed assertion, and
/// the uniformity requirement that names `*_test.go` explicitly — and a world
/// with no test file could not reach any of them. Committed alongside
/// [`MIGRATION_SOURCE`], so an attempt that does not touch it does not appear in
/// `git status`: Task 14.a's `the_attempt_really_edits_the_tree_through_the_tools`
/// asserts the changed set is exactly `main.go`, and that stays true.
pub const MIGRATION_TEST_SOURCE: &str = "main_test.go";

/// What that file holds before the migration.
///
/// Three properties, each of which a lane depends on:
///
/// - **It calls the function a bump's migration renames**, so a *uniform* edit
///   has to reach this file too — which is what makes [`MIGRATION_TEST_SOURCE`]
///   a real call site rather than decoration.
/// - **It makes exactly one assertion**, so a lane that weakens one is changing
///   the only one there is.
/// - **Its assertion message names no function.** That is the detail worth
///   writing down: a message reading `"legacyName must run"` would be rewritten
///   by an honest uniform rename, the `t.Errorf` line would leave the file, and
///   the *clean* fixture would classify as a changed test assertion. The
///   fixture would then be measuring the classifier against a world where clean
///   is unreachable.
/// - **It already carries one `if`**, so `new control flow` is a lane about a
///   count going *up* rather than about a keyword appearing in a file that had
///   none.
pub const MIGRATION_TEST_BEFORE: &str = "\
package main

import \"testing\"

func TestLegacyName(t *testing.T) {
\tlegacyName()
\tif testing.Short() {
\t\tt.Errorf(\"this test must run even in short mode\")
\t}
}
";

/// Everything one bounded migration attempt is driven from, **and the three
/// things a prompt must not carry, each really present somewhere in it.**
///
/// The second half is the reason this is a struct and not four loose builders.
/// `assert!(!body.contains(SENTINEL))` says nothing at all unless the sentinel
/// was somewhere upstream of `body`, and the three exclusions Task 14.a is about
/// each need their own upstream:
///
/// - **advisory prose.** [`MigrationWorld::report`] is a document carrying
///   [`SENTINEL_PROSE`] in a `description`, and [`MigrationWorld::group`] is
///   what [`project`] made of *that* document. So the prose was in the bytes the
///   findings came from, and Task 6's boundary is the only reason it is not in
///   the findings.
/// - **a mechanical rule.** The group's targets come from real [`attribute`]
///   calls against a real tree, not from a fixture that placed them:
///   [`MigrationWorld::resolved`] is the resolver transcript those calls
///   produced, and it contains `go list -m -json` verbatim because the run
///   really ran it.
/// - **a host fact.** [`MigrationWorld::workspace_root`] is a path carrying
///   [`HOST_ROOT`], so the worktree the attempt works in really is under a
///   directory whose name is the sentinel — which is the shape of the leak M1's
///   relativisation exists for.
///
/// A [`MigrationWorld`] therefore *fails* the exclusions if anything copies its
/// own contents into a prompt, rather than passing them because there was
/// nothing to copy.
pub struct MigrationWorld {
    /// The Go tree the attempt branches a worktree from, as a committed git
    /// repository. It really requires the module the document reported, which is
    /// what lets attribution answer rule 1 rather than refuse.
    pub tree: GoWorkspace,

    /// The scanner document the findings were projected from. Carries
    /// [`SENTINEL_PROSE`].
    pub report: Report,

    /// The one group that document produces, from the real projection and real
    /// attribution.
    pub group: Group,

    /// Every resolver command attribution ran for this group, and what each one
    /// printed. Carries `go list -m -json`.
    pub resolved: String,

    /// Where per-attempt worktrees go. Held so that [`Drop`] removes them; the
    /// path a caller wants is [`MigrationWorld::workspace_root`].
    workspaces: TempDir,
}

/// The world above, built.
///
/// Panics rather than returning a `Result` at every premise it depends on: a
/// builder that quietly produced two groups, or no fixable finding, would leave
/// a lane asserting about something other than what it says it is asserting
/// about.
pub async fn migration_world() -> MigrationWorld {
    let report = report_with_advisory_description(SENTINEL_PROSE);
    assert!(
        report.raw().contains(SENTINEL_PROSE),
        "the document a migration's findings come from has to carry the prose, \
         or no exclusion asserted downstream of it means anything"
    );

    let projection =
        project(&scanned(&report), &every_fixture_grade()).expect("a fixture document projects");
    let fixable: Vec<ProjectedFinding> = projection.fixable().cloned().collect();
    assert!(
        !fixable.is_empty(),
        "a migration is about findings there is a fix to write, and this \
         document produced none"
    );

    // A tree that really requires what the document reported, at the version it
    // reported. Without that agreement attribution refuses — and a group whose
    // targets were placed by a fixture would make the resolver transcript below
    // a thing this file wrote rather than a thing the run ran.
    let tree = go_with_shipped(&fixable[0].package, &fixable[0].current);
    // Committed, so that `git status` over a worktree of this tree reports what
    // an attempt *changed* rather than a file the fixture left untracked.
    std::fs::write(tree.path().join(MIGRATION_SOURCE), MIGRATION_SOURCE_BEFORE)
        .expect("the fixture tree is writable");
    std::fs::write(
        tree.path().join(MIGRATION_TEST_SOURCE),
        MIGRATION_TEST_BEFORE,
    )
    .expect("the fixture tree is writable");
    tree.git(&["add", "--", MIGRATION_SOURCE, MIGRATION_TEST_SOURCE]);
    tree.git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "the call site a migration rewrites",
    ]);

    let mut attributed = Vec::new();
    let mut resolved = String::new();
    for finding in &fixable {
        let attribution = attribute(finding, &tree)
            .await
            .unwrap_or_else(|why| panic!("{} has no bump target: {why}", finding.package));
        resolved.push_str(attribution.resolved());
        attributed.push(Attributed::new(
            finding.clone(),
            attribution.target().clone(),
        ));
    }
    assert!(
        resolved.contains("go list -m"),
        "attribution's own transcript has to name the mechanical rule, or \
         `the prompt carries no mechanical rule` is a claim about a string \
         nothing in this run ever held: {resolved}"
    );

    let mut groups = group(&attributed);
    assert_eq!(
        groups.len(),
        1,
        "a migration lane's group is one edit, and this document produced {} of them",
        groups.len()
    );

    MigrationWorld {
        group: groups.remove(0),
        resolved,
        tree,
        report,
        workspaces: TempDir::new().expect("a temporary directory for worktrees"),
    }
}

impl MigrationWorld {
    /// Where per-attempt worktrees are created — **a path carrying
    /// [`HOST_ROOT`]**.
    ///
    /// A directory *named* for the sentinel rather than a sentinel written into
    /// a file, because what leaks in the case M1's relativisation is about is a
    /// path: a component of the absolute location the runtime is working in.
    /// [`Workspace::create`] creates it, so the worktree really is under it.
    ///
    /// [`Workspace::create`]: fiddle_runtime::workspace::Workspace::create
    pub fn workspace_root(&self) -> PathBuf {
        self.workspaces
            .path()
            .join(HOST_ROOT.trim_start_matches('/'))
    }

    /// The configuration a migration of [`MigrationWorld::group`] runs under.
    ///
    /// The check is the scripted `go` answering a read-only question about the
    /// tree. It is what the `run_check` tool runs and nothing decides anything
    /// from it — M4's verdict is `evaluate`'s five-check contract — so what
    /// matters here is only that a model which calls the tool gets a real child
    /// process and a real answer rather than a refusal that would make the tool
    /// look broken.
    pub fn config(&self) -> MigrationConfig {
        let go = go_stub();
        let mut args = go.args.clone();
        args.extend(
            ["list", "-m", "-json", self.target_module().as_str()]
                .iter()
                .map(|arg| arg.to_string()),
        );
        MigrationConfig {
            check: WorkspaceCommand {
                program: go.program,
                args,
                timeout: MIGRATION_CHECK_TIMEOUT,
            },
            budget: AgentBudget {
                max_turns: 8,
                max_tokens: 4096,
                deadline: Duration::from_secs(300),
                max_changed_files: 16,
                tool_timeout: MIGRATION_CHECK_TIMEOUT,
            },
            cancel: CancellationToken::new(),
        }
    }

    /// The module this world's group edits.
    ///
    /// Panics on the `Dockerfile` arm, which this world cannot produce: its
    /// document reports no OS packages, so rule 4 is unreachable from here and a
    /// silent fallback would hide a fixture that had changed underneath the lane.
    pub fn target_module(&self) -> String {
        match self.group.target() {
            Target::Module(path) => path.clone(),
            other => panic!("this world's group edits a module, and edits {other:?}"),
        }
    }

    /// The attempt id every migration lane runs under, as the runtime wants it.
    pub fn attempt(&self) -> AttemptId {
        AttemptId(MIGRATION_ATTEMPT.to_string())
    }

    /// The worktree a migration of this world's group runs in.
    ///
    /// Built here rather than by `GroupMigration::migrate`, which no longer
    /// creates one: a run mitigates several groups onto one branch and each
    /// landing has to be a commit in the tree the next group starts from, so the
    /// worktree belongs to whoever owns the run. Under [`HOST_ROOT`] for the
    /// reason [`MigrationWorld::workspace_root`] gives.
    pub fn workspace(&self) -> Arc<Workspace> {
        Arc::new(
            Workspace::create(
                self.tree.path(),
                &self.workspace_root(),
                &self.attempt(),
                CancellationToken::new(),
            )
            .expect("a worktree of the migration world's tree"),
        )
    }
}

// ---------------------------------------------------------------------------
// The tree a group's outcome is landed in (Task 15)
// ---------------------------------------------------------------------------

/// The version a landing world's bump moves the direct requirement to.
///
/// Above [`DIRECT_VERSION`] and below [`SHIPPED_VERSION`], so it is a value no
/// other fixture in this file spells: a lane that started passing because the
/// bumped tree happened to equal some other world's tree would be passing for a
/// reason nobody wrote down.
const LANDING_BUMPED_VERSION: &str = "v0.40.0";

/// A tracked file the group's edit does not touch, dirty when the landing runs.
///
/// **The discriminator for the whole staging criterion**, and worth stating
/// plainly because it is easy to leave out and impossible to notice missing. If
/// every dirty path in the tree were also a path the group edited, `add -A` and
/// `add -- go.mod go.sum` would produce byte-identical commits and every
/// assertion about staging by name would hold for a subject that staged by
/// directory.
///
/// It also stands for a real hazard rather than a contrived one. A commit may
/// carry only what [`classify`] was applied to, because a file that reached the
/// commit some other way reached it with no scope rule having looked at it — and
/// the four forbidden shapes are exactly the edits a *green* build would
/// otherwise let through.
///
/// Tracked and modified rather than created, so that
/// [`GoWorkspace::is_clean_at`] over the group's own paths is the only question a
/// revert lane has to ask: an untracked file would leave `git status` non-empty
/// for a reason that has nothing to do with what was reverted.
///
/// [`classify`]: fiddle_runtime::capability::GroupStatus
pub const LANDING_UNRELATED: &str = "notes.txt";

/// What that file holds before anything dirties it.
const LANDING_UNRELATED_BEFORE: &str = "the host repository\n";

/// A file the attempt *created*, for the half of a revert a checkout cannot do.
///
/// `git checkout HEAD -- <path>` refuses a pathspec `HEAD` does not carry, so a
/// world whose changed set held only edited files could not tell a revert that
/// handles creations from one that fails on them — or worse, from one that
/// silently leaves the file on the branch for the *next* group's commit to stage.
pub const LANDING_CREATED: &str = "vendor_notes.md";

/// A tree with one group's bump in it, and the group that bump is for.
///
/// The three fields are the three arguments a landing takes, and they travel
/// together because they have to agree: `changed` is what git would report about
/// this tree, and a lane that assembled its own path list could hand the subject
/// a list naming files the tree never touched.
pub struct LandingWorld {
    /// The repository the landing runs in, and the record of what it ran.
    pub tree: GoWorkspace,

    /// The group whose outcome is being landed.
    pub group: Group,

    /// What git saw change — [`MigrationAttempt::changed`]'s stand-in, in the
    /// order [`Workspace::changed_files`] produces it, which is sorted.
    ///
    /// [`MigrationAttempt::changed`]: fiddle_runtime::capability::MigrationAttempt
    /// [`Workspace::changed_files`]: fiddle_runtime::workspace::Workspace
    pub changed: Vec<WorkspacePath>,

    /// Every commit body the repository held before the landing ran.
    ///
    /// Captured here rather than by each lane, so that *no commit was made* is
    /// the whole history being what it was rather than the narrower claim that
    /// one particular id is missing from it — and so that a lane asking the
    /// question does not have to build a second world to compare against.
    pub history_before: String,
}

/// The world above, built: `go.mod` and `go.sum` bumped, one unrelated file
/// dirty beside them, and nothing recorded yet.
///
/// # Construction does not record
///
/// Every git here goes through [`run_git`] and [`commit_paths`] rather than
/// [`GoWorkspace::git`], which is the load-bearing half of
/// [`GoWorkspace::git_calls`]'s own doc: a list that held the fixture's `init`,
/// `add` and `commit` would make "the subject staged by name" an assertion about
/// what this function staged. The list a lane reads back is therefore exactly
/// what the subject ran, and its being non-empty is evidence the seam was wired
/// in at all.
///
/// # The group is [`group_of`]'s
///
/// A group is a group — one target, a set of advisories — and Task 15 runs
/// [`fold_commit_argv`] over one of these too, so a second builder would be a
/// second fixture for one shape. The target is therefore `example.com/folded`,
/// which is what a landing's commit subject names.
///
/// [`fold_commit_argv`]: fiddle_runtime::cve::fold::fold_commit_argv
pub fn landing_world(cves: &[&str]) -> LandingWorld {
    let tree = go(direct());

    // A second commit, so the unrelated file is tracked and so `HEAD` has a
    // parent — `diff-tree` against a root commit lists the whole tree, and
    // [`GoWorkspace::staged_paths`] would then answer the same for a commit that
    // staged everything.
    std::fs::write(
        tree.path().join(LANDING_UNRELATED),
        LANDING_UNRELATED_BEFORE,
    )
    .expect("the fixture tree is writable");
    commit_paths(
        tree.path(),
        &[LANDING_UNRELATED],
        "chore: a file no bump touches",
    );

    // The bump, written as the tree a bump really leaves: `shipped` is the shape
    // whose requirement is at a given version, so the two files are the ones the
    // offline `go` would have rewritten rather than two strings this function
    // invented.
    let bumped = shipped(DIRECT_MODULE, LANDING_BUMPED_VERSION);
    std::fs::write(tree.path().join("go.mod"), bumped.go_mod())
        .expect("the fixture tree is writable");
    std::fs::write(
        tree.path().join("go.sum"),
        bumped
            .go_sum()
            .expect("a tree with a requirement has a go.sum"),
    )
    .expect("the fixture tree is writable");
    std::fs::write(
        tree.path().join(LANDING_UNRELATED),
        format!("{LANDING_UNRELATED_BEFORE}and a line nobody asked the group about\n"),
    )
    .expect("the fixture tree is writable");

    // The premises, asserted here rather than in each lane: a world whose bump
    // did not actually change the tree would let a commit of nothing pass every
    // staging assertion, and a world whose unrelated file was already clean would
    // make the discrimination above disappear.
    assert!(
        !tree.is_clean_at(&["go.mod", "go.sum"]),
        "a landing world's bump has to have changed the tree"
    );
    assert!(
        !tree.is_clean_at(&[LANDING_UNRELATED]),
        "{LANDING_UNRELATED} has to be dirty, or staging by name and staging by \
         directory produce the same commit"
    );

    LandingWorld {
        group: group_of(cves),
        changed: workspace_paths(&["go.mod", "go.sum"]),
        history_before: tree.all_commit_bodies(),
        tree,
    }
}

impl LandingWorld {
    /// The same world with a file the attempt created, in the changed set.
    ///
    /// A method rather than a second builder, so the two worlds differ by the one
    /// thing their names say they differ by. See [`LANDING_CREATED`].
    pub fn and_a_created_file(mut self) -> Self {
        std::fs::write(
            self.tree.path().join(LANDING_CREATED),
            "vendored, by the attempt\n",
        )
        .expect("the fixture tree is writable");
        self.changed = workspace_paths(&["go.mod", "go.sum", LANDING_CREATED]);
        self
    }
}

/// `paths` as the runtime's own containment-checked type.
///
/// Through [`WorkspacePath::parse`] rather than constructed, for the reason
/// `changes::listed` gives about paths that came from git: the type is the
/// carrier of the guarantee, and a fixture that skipped the parse would be
/// handing the subject a path nothing had checked.
fn workspace_paths(paths: &[&str]) -> Vec<WorkspacePath> {
    paths
        .iter()
        .map(|path| WorkspacePath::parse(path).expect("a fixture path is inside the workspace"))
        .collect()
}

/// A real per-attempt worktree of a landing world's tree, with the bump in it.
///
/// **The production adapter's world.** Every other landing lane drives
/// [`GoWorkspace`] as the [`Git`] seam, which is what makes the recorded call
/// list readable — and a port whose only implementation in the suite is the
/// test's own is a port whose production side is measured by nothing. This is
/// that side: a real detached worktree, a real [`Workspace`], and
/// [`InWorktree`](fiddle_runtime::capability::InWorktree) composing
/// [`Workspace::run`] over it.
///
/// The bump is written **into the worktree** rather than inherited from the
/// fixture, because `git worktree add --detach HEAD` branches at the commit and
/// not at the dirty tree beside it — a lane that assumed otherwise would land a
/// commit of nothing and read its own emptiness as success.
pub struct LandingWorktree {
    /// The worktree the landing runs in. Its [`Drop`] removes it.
    pub workspace: Workspace,

    /// What git would see change in it — the same two paths, since the same two
    /// files were written.
    pub changed: Vec<WorkspacePath>,

    /// Where per-attempt worktrees go. Held only so that [`Drop`] removes it,
    /// and dropped *after* the workspace because the workspace's own teardown
    /// runs `git worktree remove` against a path underneath it.
    _root: TempDir,
}

/// The worktree above, built from `world`'s fixture tree.
pub fn landing_worktree(world: &LandingWorld) -> LandingWorktree {
    let root = TempDir::new().expect("a temporary directory for worktrees");
    let workspace = Workspace::create(
        world.tree.path(),
        root.path(),
        &AttemptId(MIGRATION_ATTEMPT.to_string()),
        CancellationToken::new(),
    )
    .expect("a worktree of the fixture tree");

    let bumped = shipped(DIRECT_MODULE, LANDING_BUMPED_VERSION);
    std::fs::write(workspace.root().join("go.mod"), bumped.go_mod())
        .expect("the worktree is writable");
    std::fs::write(
        workspace.root().join("go.sum"),
        bumped
            .go_sum()
            .expect("a tree with a requirement has a go.sum"),
    )
    .expect("the worktree is writable");
    assert_eq!(
        workspace
            .changed_files()
            .expect("git can describe the worktree")
            .iter()
            .map(|path| path.as_str().to_string())
            .collect::<Vec<_>>(),
        ["go.mod", "go.sum"],
        "the premise: the bump really reached the worktree, and only it did"
    );

    LandingWorktree {
        changed: workspace_paths(&["go.mod", "go.sum"]),
        workspace,
        _root: root,
    }
}

/// git in `dir`, for a lane asking a question of a worktree.
///
/// The counterpart of [`GoWorkspace::is_clean`] and the four accessors beside it,
/// for the one world that is a [`Workspace`] rather than a [`GoWorkspace`]. It is
/// the test asking, so nothing records it — and nothing could, since the seam
/// under test here is [`Workspace::run`] rather than this file's handle.
pub fn ask_git(dir: &Path, args: &[&str]) -> String {
    run_git(dir, args)
}

/// The same, for the questions whose *failure* is the answer.
///
/// `git rev-parse --verify --quiet <ref>` exits non-zero for a ref that is not
/// there, and that is what "does this world already hold it?" is: a fixture
/// reaching for [`ask_git`] would panic on the case it was asking about.
pub fn try_ask_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    try_run_git(dir, args)
}

/// A [`GoWorkspace`] as the one seam Task 15's landing runs git through.
///
/// The tree *is* the recorder, rather than a wrapper holding one, because
/// [`GoWorkspace::git_calls`] already exists and thirteen tasks read it: a second
/// record beside it would be a second answer to *what did this run against this
/// tree*. Every invocation therefore lands in one list whether the subject or a
/// lane made it — which is why the questions a lane asks
/// ([`GoWorkspace::is_clean`] and the four accessors beside it) deliberately go
/// round it.
///
/// It records and delegates rather than answering, which is [`RecordedCalls`]'s
/// arrangement and its reason: a fixture that answered would be deciding what git
/// says, and what these lanes are about is what the subject *ran* and what really
/// happened to the branch because of it.
#[async_trait::async_trait]
impl Git for GoWorkspace {
    async fn run(&self, args: &[&str]) -> Result<String, CapabilityError> {
        self.try_git(args).map_err(|stderr| {
            CapabilityError::Workspace(WorkspaceError::Git {
                command: args.join(" "),
                stderr,
            })
        })
    }
}

// ---------------------------------------------------------------------------
// A clone whose local refs disagree with the remote's (Task 17.b)
// ---------------------------------------------------------------------------

/// The file the seed repository puts on the shared branch and nowhere else.
///
/// Its presence in a worktree is the second, independent witness that the
/// checkout took the pull request's tip: a lane could compare shas and be reading
/// two copies of one mistake, and this is a file that is simply there or not.
pub const ON_THE_SHARED_BRANCH: &str = "shared_branch_marker.txt";

/// The file the seed repository puts on `main` after the clone was taken.
///
/// The base half of the same witness, and the reason the base moves at all: a
/// remote whose `main` never advanced past the clone would make
/// `origin/main` and the clone's own `main` the same commit, and *the fetch
/// happened* would be indistinguishable from *nothing needed to*.
pub const ONLY_ON_THE_REMOTE_BASE: &str = "moved_on.txt";

/// A clone whose local refs are **stale**, beside the remote they are stale
/// against.
///
/// # What this world exists to make falsifiable
///
/// Design §4 says a run checks the shared pull request out at the **remote tip**
/// and cuts a fresh branch from `origin/<base>` — *never local `HEAD` or local
/// `main`*. Neither half of that can be tested in a clone whose local refs agree
/// with the remote's, because every candidate rule then produces the same commit.
///
/// So this world arranges the disagreement the rule is about, and arranges it the
/// way a real one arises: a clone is taken, the remote moves on, and the clone
/// accumulates local work of its own. Four distinct commits come out of it —
/// [`RemoteWorld::base_revision`] and [`RemoteWorld::pr_head`] on the remote,
/// [`RemoteWorld::stale_main`] and [`RemoteWorld::stale_head`] in the clone — and
/// a checkout that reached for a local ref lands on one this world can name.
///
/// # The remote is the caller's
///
/// It is passed in rather than built here because the scripted `gh` answers ref
/// reads out of a bare repository beside *its* scratch directory, and the whole
/// value of this world is that `git` and `gh` are looking at one remote through
/// two doors. A world that built its own would let a push land somewhere the
/// forge could never see, and every postcondition read would then be answered
/// about a different repository.
pub struct RemoteWorld {
    /// The clone the run works from, and the record of what it ran in there.
    ///
    /// It is the [`Git`] seam [`check_out`](fiddle_runtime::capability::cve)
    /// fetches through, so [`GoWorkspace::git_calls`] is what a lane reads to see
    /// which ref the subject actually named.
    pub tree: GoWorkspace,

    /// The group whose outcome a landing in this world commits.
    pub group: Group,

    /// What `refs/heads/<base>` is at on the remote — and what a fresh cut must
    /// be made from.
    pub base_revision: String,

    /// What the shared branch is at on the remote, or `None` when this world has
    /// none. The commit a reuse must be made at.
    pub pr_head: Option<String>,

    /// What the clone's own `main` is at: a local commit the remote has never
    /// seen. A run that branched from local `main` lands here.
    pub stale_main: String,

    /// What the clone's own copy of the shared branch is at, or `None` when this
    /// world has none. A run that checked out the branch *by name* lands here.
    pub stale_head: Option<String>,
}

/// Build [`RemoteWorld`] in `remote`, optionally with a shared branch already
/// open on it.
///
/// # Construction does not record
///
/// Every git below goes through [`run_git`] against the seed repository or the
/// clone's path, never through [`GoWorkspace::git`] — [`GoWorkspace::git_calls`]'s
/// own rule. The list a lane reads back is therefore exactly what the subject ran,
/// and its being non-empty is evidence the seam was wired in at all.
pub fn remote_world(remote: &Path, head_branch: Option<&str>, cves: &[&str]) -> RemoteWorld {
    run_git(
        remote.parent().expect("the remote has a parent directory"),
        &[
            "-c",
            "init.defaultBranch=main",
            "init",
            "--quiet",
            "--bare",
            &remote.display().to_string(),
        ],
    );

    // The seed: a repository that pushes the remote's history into place. Kept
    // apart from the clone so that the commits the clone must *not* have are
    // never in its store to begin with — a fixture that made them in the clone
    // and then reset would leave the objects behind, and `git worktree add` at a
    // sha the store happens to hold is exactly the accident this world is here to
    // exclude.
    let seed_root = TempDir::new().expect("a temporary directory for the seed repository");
    let seed = write_tree(seed_root.path(), "seed", &direct());
    commit_tree(
        &seed,
        &direct(),
        "the base, as it was when the clone was taken",
    );
    let cloned_from = run_git(&seed, &["rev-parse", "HEAD"]);
    run_git(
        &seed,
        &["remote", "add", "origin", &remote.display().to_string()],
    );
    run_git(
        &seed,
        &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
    );

    // The clone, taken here — so its `origin/main` is `cloned_from` and every
    // commit below is one it has to fetch.
    let root = TempDir::new().expect("a temporary directory for the clone");
    let repo = root.path().join("clone");
    run_git(
        root.path(),
        &[
            "clone",
            "--quiet",
            &remote.display().to_string(),
            &repo.display().to_string(),
        ],
    );

    // The remote moves on, on `main`.
    std::fs::write(seed.join(ONLY_ON_THE_REMOTE_BASE), "the base moved on\n")
        .expect("the seed repository is writable");
    commit_paths(
        &seed,
        &[ONLY_ON_THE_REMOTE_BASE],
        "chore: the base moved on",
    );
    let base_revision = run_git(&seed, &["rev-parse", "HEAD"]);
    run_git(
        &seed,
        &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
    );

    // And, when this world has one, a shared branch off the commit the clone was
    // taken at — a sibling of the base's new tip rather than a descendant, which
    // is what a pull request branch actually is.
    let pr_head = head_branch.map(|branch| {
        run_git(
            &seed,
            &["checkout", "--quiet", "-b", "shared", &cloned_from],
        );
        std::fs::write(
            seed.join(ON_THE_SHARED_BRANCH),
            "opened by an earlier run\n",
        )
        .expect("the seed repository is writable");
        commit_paths(&seed, &[ON_THE_SHARED_BRANCH], "fix: an earlier run's bump");
        let head = run_git(&seed, &["rev-parse", "HEAD"]);
        run_git(
            &seed,
            &[
                "push",
                "--quiet",
                "origin",
                &format!("HEAD:refs/heads/{branch}"),
            ],
        );
        head
    });

    // The clone's own work, which the remote has never seen. `main` first…
    std::fs::write(repo.join("stale.txt"), "left behind by an earlier run\n")
        .expect("the clone is writable");
    commit_paths(&repo, &["stale.txt"], "chore: a commit only this clone has");
    let stale_main = run_git(&repo, &["rev-parse", "HEAD"]);

    // …and then a local branch of the *same name* as the shared one, pointing
    // somewhere else entirely. This is the stale ref Design §4 is about: a
    // `security/cve-remediation-…` yesterday's run left in the same clone.
    let stale_head = head_branch.map(|branch| {
        run_git(
            &repo,
            &["branch", "--no-track", branch, cloned_from.as_str()],
        );
        run_git(&repo, &["rev-parse", &format!("refs/heads/{branch}")])
    });

    // The premises, asserted here rather than in each lane. Every one of these
    // being distinct is what makes a checkout assertion falsifiable; a world in
    // which two of them coincided would let a lane pass against the wrong rule.
    let distinct: Vec<&String> = [
        Some(&base_revision),
        pr_head.as_ref(),
        Some(&stale_main),
        stale_head.as_ref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut deduped = distinct.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        distinct.len(),
        "the remote's tips and the clone's stale refs must all differ, or a \
         checkout that took the wrong one would be indistinguishable: {distinct:?}"
    );
    assert_ne!(
        run_git(&repo, &["rev-parse", "refs/remotes/origin/main"]),
        base_revision,
        "the clone's idea of origin/main has to be stale before the run fetches, \
         or the fetch is doing nothing observable"
    );

    RemoteWorld {
        tree: GoWorkspace {
            repo: canonical(&repo),
            root,
            calls: Mutex::new(Vec::new()),
        },
        group: group_of(cves),
        base_revision,
        pr_head,
        stale_main,
        stale_head,
    }
}

impl RemoteWorld {
    /// Write the group's bump into `worktree` and answer the paths it changed.
    ///
    /// Written into the *worktree* rather than inherited from the clone, for
    /// [`landing_worktree`]'s reason: `git worktree add` branches at a commit and
    /// not at the dirty tree beside it, so a lane that assumed otherwise would
    /// land a commit of nothing and read its own emptiness as success.
    ///
    /// The premise is asserted here: git must really see these two paths change,
    /// or a landing has nothing to commit and every assertion below it is about a
    /// branch that never moved.
    pub fn bump_into(&self, worktree: &Path) -> Vec<WorkspacePath> {
        let bumped = shipped(DIRECT_MODULE, LANDING_BUMPED_VERSION);
        std::fs::write(worktree.join("go.mod"), bumped.go_mod()).expect("the worktree is writable");
        std::fs::write(
            worktree.join("go.sum"),
            bumped
                .go_sum()
                .expect("a tree with a requirement has a go.sum"),
        )
        .expect("the worktree is writable");
        assert!(
            !run_git(
                worktree,
                &["status", "--porcelain", "--", "go.mod", "go.sum"]
            )
            .is_empty(),
            "the bump has to have changed the worktree, or the landing commits nothing"
        );
        workspace_paths(&["go.mod", "go.sum"])
    }
}
