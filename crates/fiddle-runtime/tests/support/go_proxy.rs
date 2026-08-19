//! An offline `go`, and the module proxy behind it.
//!
//! There is no Go toolchain in this project's development shell — `flake.nix`
//! declares a Rust toolchain, `alejandra`, `gh` and `jq`, and nothing else — and
//! there is no module proxy behind one either. Production runs in the host
//! repository's CI, which has both by definition. The offline gate has neither,
//! and the whole of attribution rule 2 is a question that can only be answered by
//! *changing a tree and asking again*. So this file is what answers.
//!
//! # What it is a stand-in for, and what it is not
//!
//! It stands in for the toolchain **and** for the upstream it resolves against.
//! `tests/support/cve.rs` says, at [`PARENT_A_MINOR_BEHIND`], that what makes a
//! parent non-viable is that no published version of it carries the fix, and that
//! this is not a fact a `go.mod` can hold. [`UPSTREAM`] is where that fact lives:
//! a handful of releases and what each of them requires. Nothing offline can
//! conjure a version that exists, so a fixture that needs the probe to genuinely
//! succeed and genuinely fail has to supply that half itself.
//!
//! It is **not** a decision. Every function here prints a document or writes a
//! file, exactly as `go` does; which rule fires, and whether a probed version
//! satisfies a finding, are read out of those documents by
//! [`fiddle_runtime::cve::attribute`] and by `cve::version::at_least`. A proxy
//! that answered *this parent is viable* would be answering rule 2 on the
//! subject's behalf, which is the same line 8.a drew for `list` and `why`.
//!
//! # Why one file drives two stand-ins
//!
//! There are two ways the suite reaches a `go`:
//!
//! - **In process.** `GoWorkspace` implements the `ModuleGraph` port by calling
//!   [`run`] directly. No spawn, no binary, and every 8.a lane keeps running at
//!   the speed it did.
//! - **As a child.** `tests/go_stub/go_stub.rs` is a `[[bin]]` that reads its
//!   `argv`, calls [`run`], prints and exits — so the *production* adapter,
//!   `fiddle_runtime::cve::go::Go`, can be driven end to end with only the
//!   toolchain scripted. That is the arrangement `wizcli` is under, and it is the
//!   reason M4a ships a real spawning adapter rather than a port with nothing
//!   behind it.
//!
//! One implementation for both, in a file of its own, because a stand-in that
//! exists twice drifts — and the drift would present as the spawning lane and the
//! in-process lane disagreeing about a rule neither of them owns. The split is
//! also a compilation constraint, the same one `document.rs` is under: a `[[bin]]`
//! is compiled against `[dependencies]` alone, so nothing here may reach
//! `tempfile` or the crate's own test helpers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// The world these trees are in
// ---------------------------------------------------------------------------

/// The module path every fixture tree calls itself, standing in for the host
/// repository under repair.
pub const HOST_MODULE: &str = "example.com/host";

/// The language version every fixture tree declares. One value, so that a
/// difference between two trees is never an accident of this line.
pub const GO_VERSION: &str = "1.23";

/// The module an *indirect* finding is in. Reached through a parent rather than
/// required by the host, which is the whole of what makes attribution rule 2
/// different from rule 1.
///
/// It is also the **second** module the two-group sweep fixture requires, there
/// as a direct requirement. One constant for both, and deliberately: it is the
/// second row of `document.rs`'s library table, which is what a scanner document
/// naming two library advisories reports — and this file's whole reason for
/// existing is that a module path spelled twice is two worlds that can come
/// apart. The two roles never meet, because they are reached from different
/// fixture trees: rule 2's shapes require it indirectly through
/// [`FIXTURE_PARENT`], and `tests/fixtures/cve-two-libraries` requires it
/// directly and has no parent in it at all.
pub const INDIRECT_MODULE: &str = "golang.org/x/net";

/// What that module is pinned at before anything probes, and what the two-group
/// sweep fixture requires it at.
pub const INDIRECT_VERSION: &str = "v0.24.0";

/// What the second library advisory names as fixing it, and therefore the only
/// release the second group of a two-group sweep can be moved to.
///
/// It is `document.rs`'s library table again — the second row's `fixedVersion` —
/// and `the_two_library_fixture_is_pinned_to_what_its_world_publishes` in
/// `fiddle-acceptance` is what fails if the two drift.
///
/// Published rather than merely named, in [`UPSTREAM`] below, because a whole
/// sweep asks [`versions`] before it chooses a bump target: a module with no
/// releases yields an empty candidate list, which is `GroupError::NoRelease` —
/// the group would be *blocked* before it ever reached the fold rule, and the
/// lane would pass or fail for a reason that has nothing to do with folding.
pub const INDIRECT_FIXED: &str = "v0.28.0";

/// The parent every indirect shape routes through.
///
/// Here rather than in `cve.rs` because [`UPSTREAM`] has to key on it: the shape
/// of a tree and the releases the proxy will resolve for it are two halves of one
/// world, and two spellings of "some parent" would let them come apart.
pub const FIXTURE_PARENT: &str = "gh.com/parent";

/// Where the viable parent's line reaches, and what it brings with it.
///
/// `v0.33.0` is the finding's `fixedVersion` — spelled without the `v` on the
/// scanner's side, which is the mixed-prefix pair `cve::version::at_least` exists
/// for. A probe that bumps [`FIXTURE_PARENT`] inside the `v1.2` minor reaches
/// this, and the confirm says yes.
pub const CARRIED_BY_THE_VIABLE_LINE: &str = "v0.33.0";

/// Where the other parent's line reaches: newer than the tree was pinned at, and
/// still short of the fix.
///
/// **This is what makes the revert assertion mean anything.** A parent whose
/// newest release is the one already in `go.mod` would give a probe nothing to
/// write, and `is_clean()` afterwards would then be satisfied by a probe that
/// never happened. So the `v1.9` line does have releases above the pin — they
/// simply top out below `fixedVersion`, which is the ordinary shape of a
/// dependency that has been maintained and still cannot carry a fix. The tree
/// really moves, the confirm really says no, and the restore really has something
/// to undo.
pub const REACHED_WITHOUT_THE_FIX: &str = "v0.31.0";

/// One published release of one module, and what it requires.
///
/// A flat table rather than a map keyed by module, because it is read in both
/// directions — the highest release matching a query, and what a named release
/// requires — and neither reading is hot.
struct Release {
    module: &'static str,
    version: &'static str,
    requires: &'static [(&'static str, &'static str)],
}

/// Every release the module proxy holds.
///
/// Two lines of one parent, and they differ in the only way that decides rule 2:
///
/// - **`v1.2`** — the tree pins `v1.2.0`, and `v1.2.7` above it requires the
///   named module at [`CARRIED_BY_THE_VIABLE_LINE`]. Bumping inside the minor
///   fixes the finding, so the parent is the bump target.
/// - **`v1.9`** — the tree pins `v1.9.9`, and the line continues to `v1.9.12`,
///   which requires only [`REACHED_WITHOUT_THE_FIX`]. Bumping inside the minor
///   moves the tree and does *not* fix the finding, so the probe has to put the
///   tree back and rule 3 has to answer.
///
/// `v1.9.12` above `v1.9.9` is deliberate on a second count: a prefix query that
/// picked the *lexically* highest match would choose `v1.9.9`, which carries no
/// fix either — and the lane would pass for the wrong reason. Resolution here is
/// component-wise and numeric, for the reason `cve::version` gives.
///
/// And two releases of [`SWEEP_MODULE`], which are a different fixture's world in
/// the same table. They carry no requirements at all and are not reached by any
/// rule-2 probe: what needs them is [`versions`], which a whole sweep asks before
/// it chooses a bump target, and which cannot answer about a module the proxy has
/// never published. They are here rather than in a second table for this file's
/// own reason — one implementation of the upstream, shared by the `[[bin]]` and
/// the in-process stand-in, so the black-box lane and the attribution lanes
/// cannot come to disagree about what has been released.
const UPSTREAM: &[Release] = &[
    Release {
        module: FIXTURE_PARENT,
        version: "v1.2.0",
        requires: &[(INDIRECT_MODULE, INDIRECT_VERSION)],
    },
    Release {
        module: FIXTURE_PARENT,
        version: "v1.2.7",
        requires: &[(INDIRECT_MODULE, CARRIED_BY_THE_VIABLE_LINE)],
    },
    Release {
        module: FIXTURE_PARENT,
        version: "v1.9.9",
        requires: &[(INDIRECT_MODULE, INDIRECT_VERSION)],
    },
    Release {
        module: FIXTURE_PARENT,
        version: "v1.9.12",
        requires: &[(INDIRECT_MODULE, REACHED_WITHOUT_THE_FIX)],
    },
    Release {
        module: SWEEP_MODULE,
        version: SWEEP_VULNERABLE,
        requires: &[],
    },
    Release {
        module: SWEEP_MODULE,
        version: SWEEP_FIXED,
        requires: &[],
    },
    // And the two releases of [`INDIRECT_MODULE`] the two-group sweep needs a
    // target selected out of. They require nothing, exactly as the two above do,
    // and for the same reason: nothing resolves *through* them, and a
    // requirement here would reach `tidy` in every tree that has this module —
    // which is every rule-2 shape.
    Release {
        module: INDIRECT_MODULE,
        version: INDIRECT_VERSION,
        requires: &[],
    },
    Release {
        module: INDIRECT_MODULE,
        version: INDIRECT_FIXED,
        requires: &[],
    },
];

/// The module the black-box sweep's fixture pair disagrees about.
///
/// It is the requirement `tests/fixtures/cve-vulnerable/go.mod` and
/// `tests/fixtures/cve-fixed/go.mod` differ in, and the package the shared
/// scanner document names — `the_pair_is_pinned_to_the_module_and_versions_the_shared_scanner_document_names`
/// in `fiddle-acceptance` is what fails if any of the three drifts. A second
/// spelling here would let this proxy publish releases of a module nothing in
/// that world requires.
pub const SWEEP_MODULE: &str = "golang.org/x/crypto";

/// What the vulnerable fixture pins, and what the advisory reports as current.
pub const SWEEP_VULNERABLE: &str = "v0.31.0";

/// What the advisory names as fixed, what the already-fixed fixture pins, and
/// therefore the only release a bump can select.
///
/// Two releases and no third: [`versions`] answers the whole line, so a stray
/// `v0.36.0` here would be selected instead and the tree would land at a version
/// the fixture pair says nothing about.
pub const SWEEP_FIXED: &str = "v0.35.0";

// ---------------------------------------------------------------------------
// What a `go` invocation leaves behind
// ---------------------------------------------------------------------------

/// What this `go` printed and what it exited with.
///
/// Both streams and the status, rather than one string, because the two callers
/// need different parts of it: the `[[bin]]` has to reproduce all three for a real
/// child, and the in-process stand-in reads [`Answer::text`] — which is the same
/// rule `fiddle_runtime::cve::go::Go` applies to a finished child, written down
/// once so the two cannot disagree about what `go` "said".
pub struct Answer {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl Answer {
    /// What the port hands back: stdout when there is any, and otherwise stderr.
    ///
    /// Not the two concatenated. `go list -m -json` prints its document on stdout
    /// and its progress on stderr, so a reader parsing the pair would fail on a
    /// download line; and the answers this build matches rules against — `go:
    /// module …: not a known dependency`, a `go mod tidy` that says nothing —
    /// arrive on stderr with an empty stdout. Picking one is what makes both
    /// readable.
    pub fn text(&self) -> String {
        match self.stdout.trim().is_empty() {
            true => self.stderr.clone(),
            false => self.stdout.clone(),
        }
    }
}

/// The whole of this `go`: a tree, an `argv`, and what running it there does.
///
/// One entry point rather than a function per subcommand, so the `[[bin]]` and
/// the in-process stand-in dispatch through the same match and an argument form
/// one of them accepts cannot be one the other rejects.
///
/// An `argv` this does not know panics rather than exiting non-zero, for
/// `arm_was_exercised`'s reason: a command the fixture cannot produce is a defect
/// in the caller, and reporting it as "`go` refused" would send whoever is reading
/// the failure to the subject instead.
pub fn run(root: &Path, args: &[&str]) -> Answer {
    match args {
        // Before the `-json` arm below only because a `match` reads in order;
        // the two argument vectors are disjoint, so neither shadows the other.
        ["list", "-m", "-versions", module] => versions(module),
        ["list", "-m", "-json", module] => list(root, module),
        ["mod", "why", "-m", module] => why(root, module),
        ["get", target] => get(root, target),
        ["mod", "tidy"] => tidy(root),
        other => panic!("the offline go has no `go {}`", other.join(" ")),
    }
}

// ---------------------------------------------------------------------------
// The read-only pair, which 8.a's rules are matched over
// ---------------------------------------------------------------------------

/// `go list -m -versions <module>` — every release the upstream has published.
///
/// The module path, then its versions, space-separated on one line, which is
/// what `go` prints and what `cve::go::Go::versions` splits. **Not derived from
/// the tree**, unlike every other answer in this file: what a tree pins is one
/// version and the question here is what a caller could move it to, so a proxy
/// that answered from `go.mod` would tell a sweep its only option is where it
/// already is.
///
/// A module [`UPSTREAM`] has never published prints its path and nothing after
/// it — `go`'s own answer for a path with no releases, and the shape
/// `Go::versions` reads as an empty candidate list rather than as a failure.
/// Exit zero for the same reason: it is an answer, not a refusal.
///
/// Ascending, because a proxy that emitted its table's order would let a
/// selection that took the *last* line pass while a selection that ranked them
/// failed, and the two are only distinguishable against a list that is sorted.
fn versions(module: &str) -> Answer {
    let mut published: Vec<&str> = UPSTREAM
        .iter()
        .filter(|release| release.module == module)
        .map(|release| release.version)
        .collect();
    published.sort_by(|a, b| match newer(a, b) {
        true => std::cmp::Ordering::Greater,
        false => std::cmp::Ordering::Less,
    });
    let mut line = module.to_string();
    for version in published {
        line.push(' ');
        line.push_str(version);
    }
    Answer {
        stdout: format!("{line}\n"),
        stderr: String::new(),
        code: 0,
    }
}

/// `go list -m -json <module>` — the module's record in the build list.
///
/// Derived from the tree, read on every call, so an edit to `go.mod` changes what
/// the resolver says. That is what makes the probe's confirm a measurement: the
/// same command, over a tree the bump moved.
fn list(root: &Path, module: &str) -> Answer {
    let record = match requirement(root, module) {
        Some((path, version, indirect)) => {
            let mut record = serde_json::json!({
                "Path": path,
                "Version": version,
                // A key the subject has no use for, present so that its
                // tolerance of unknown keys is exercised rather than asserted in
                // a comment. `go list -m -json` prints a dozen and gains more
                // with each release.
                "GoVersion": GO_VERSION,
            });
            // Written only when true, exactly as `go` writes it: the field is
            // `omitempty`, so a direct requirement is one with **no** `Indirect`
            // key. A fixture that always wrote the key would let a subject that
            // required it pass, and that subject would then read every real
            // direct requirement as unknown.
            if indirect {
                record["Indirect"] = serde_json::Value::Bool(true);
            }
            record
        }
        None if module == HOST_MODULE => serde_json::json!({
            "Path": HOST_MODULE,
            "Main": true,
            "GoVersion": GO_VERSION,
        }),
        // What `go list -m` prints for a path outside the build list. It goes to
        // stderr and the status line is non-zero, because that is where `go` puts
        // it — and it is still an *answer*, which is why the port hands the text
        // back rather than raising. Rules 3 and 4 are reached through it.
        None => {
            return Answer {
                stdout: String::new(),
                stderr: format!("go: module {module}: not a known dependency\n"),
                code: 1,
            }
        }
    };
    Answer {
        stdout: format!(
            "{}\n",
            serde_json::to_string_pretty(&record).expect("a record serializes")
        ),
        stderr: String::new(),
        code: 0,
    }
}

/// `go mod why -m <module>` — the chain by which the main module needs it.
///
/// The `#` line names what is being explained, and `go` prints it whether or not
/// there is a chain underneath. A hop in between appears exactly when the tree has
/// a direct requirement to route through, which is what leaves
/// `indirect_without_a_direct_parent` a chain with no parent in it rather than a
/// chain this proxy special-cases by shape.
fn why(root: &Path, module: &str) -> Answer {
    let mut answer = format!("# {module}\n");
    match requirement(root, module) {
        None => answer.push_str(&format!("(main module does not need module {module})\n")),
        Some((path, _, indirect)) => {
            answer.push_str(&format!("{HOST_MODULE}\n"));
            // `go` prints package paths here rather than module paths; in these
            // trees the two coincide, and a fixture that invented a package path
            // under each module would be inventing the very thing the subject
            // reads.
            if indirect {
                if let Some((parent, _, _)) = requirements(root)
                    .into_iter()
                    .find(|(_, _, indirect)| !*indirect)
                {
                    answer.push_str(&format!("{parent}\n"));
                }
            }
            answer.push_str(&format!("{path}\n"));
        }
    }
    Answer {
        stdout: answer,
        stderr: String::new(),
        code: 0,
    }
}

// ---------------------------------------------------------------------------
// The mutating pair, which rule 2's viability is measured with
// ---------------------------------------------------------------------------

/// `go get <module>@<query>` — move a requirement to the highest release the
/// query names, writing `go.mod` and `go.sum`.
///
/// A *prefix* query is the whole point: rule 2 asks whether a newer release
/// **inside the parent's own minor** carries the fix, and `v1.2` is how that is
/// spelled to `go` — it resolves to the highest release with that prefix. A query
/// naming an exact version would be a different question, and a bump across a
/// minor is a change nobody asked this rule to make.
///
/// Nothing is written when the query resolves to the version already pinned, and
/// that is `go`'s behaviour rather than an optimisation: a probe that dirtied a
/// tree it did not move would make `is_clean()` answer about the probe's
/// tidiness rather than about its revert.
fn get(root: &Path, target: &str) -> Answer {
    let Some((module, query)) = target.split_once('@') else {
        return refused(format!("go: invalid module version syntax {target:?}\n"));
    };
    let Some(resolved) = highest_matching(module, query) else {
        return refused(format!(
            "go: {module}@{query}: no matching versions for query {query:?}\n"
        ));
    };

    let mut selected = BTreeMap::new();
    selected.insert(module.to_string(), resolved.to_string());
    let was = requirement(root, module).map(|(_, version, _)| version);
    match write_selection(root, &selected) {
        // `go`'s own wording for an upgrade, on stderr, where `go` puts it.
        true => Answer {
            stdout: String::new(),
            stderr: format!(
                "go: upgraded {module} {} => {resolved}\n",
                was.unwrap_or_default()
            ),
            code: 0,
        },
        false => Answer {
            stdout: String::new(),
            stderr: String::new(),
            code: 0,
        },
    }
}

/// `go mod tidy` — re-resolve the build list after a requirement moved.
///
/// **This is the step that makes a bump visible to the confirm**, and it is why
/// the plan names three commands rather than two. `go get` moves the parent and
/// nothing else; what the finding is *about* is the module the parent brings in,
/// and that requirement only follows once the build list is resolved again. A
/// probe that skipped this would read the pre-bump version back out of the tree
/// and conclude that every parent is non-viable.
///
/// Minimal version selection, over the tree's own direct requirements: each one's
/// release in [`UPSTREAM`] names what it needs, and the selected version of a
/// module is the highest anything asks for — never lower than what the tree
/// already pins, which is the whole of what "minimal version selection raises it
/// for every consumer" means.
///
/// It rewrites versions and does not add or remove requirement lines. That is a
/// real limit rather than an oversight: adding one would make this a `go.mod`
/// writer, and the shapes the CVE lanes are built from all carry the lines a tidy
/// would have produced.
fn tidy(root: &Path) -> Answer {
    let current = requirements(root);
    let mut selected: BTreeMap<String, String> = BTreeMap::new();
    for (path, version, indirect) in &current {
        if *indirect {
            continue;
        }
        for release in UPSTREAM {
            if release.module != path || release.version != version {
                continue;
            }
            for (needed, at) in release.requires {
                let entry = selected
                    .entry((*needed).to_string())
                    .or_insert_with(|| (*at).to_string());
                if newer(at, entry) {
                    *entry = (*at).to_string();
                }
            }
        }
    }
    // Never below what the tree already pins.
    for (path, version, _) in &current {
        if let Some(chosen) = selected.get_mut(path) {
            if newer(version, chosen) {
                *chosen = version.clone();
            }
        }
    }

    write_selection(root, &selected);
    // Silent on success, as `go mod tidy` is. The transcript records the command
    // with nothing under it, which is what the reader would see running it.
    Answer {
        stdout: String::new(),
        stderr: String::new(),
        code: 0,
    }
}

/// A refusal: nothing on stdout, `go`'s complaint on stderr, non-zero.
fn refused(message: String) -> Answer {
    Answer {
        stdout: String::new(),
        stderr: message,
        code: 1,
    }
}

// ---------------------------------------------------------------------------
// The tree on disk
// ---------------------------------------------------------------------------

/// Every `require` line the tree holds now: path, version, indirect.
///
/// **Not a `go.mod` parser**, and it must not grow into one. It reads the
/// single-line `require` directive the fixture shapes write and nothing else — no
/// block form, no `replace`, no `exclude`. A fixture that parsed the whole grammar
/// would be a second implementation of a thing `go` already does, drifting from
/// it, in a file whose job is to build worlds.
pub fn requirements(root: &Path) -> Vec<(String, String, bool)> {
    read_go_mod(root)
        .lines()
        .filter_map(parse_requirement)
        .collect()
}

/// One `require` line, or nothing.
fn parse_requirement(line: &str) -> Option<(String, String, bool)> {
    let rest = line.trim().strip_prefix("require ")?;
    let (rest, indirect) = match rest.split_once("//") {
        Some((head, tail)) => (head.trim(), tail.trim() == "indirect"),
        None => (rest.trim(), false),
    };
    let (path, version) = rest.split_once(char::is_whitespace)?;
    Some((path.to_string(), version.trim().to_string(), indirect))
}

/// The tree's requirement on `module`, if it has one.
fn requirement(root: &Path, module: &str) -> Option<(String, String, bool)> {
    requirements(root)
        .into_iter()
        .find(|(path, _, _)| path == module)
}

/// Move every requirement `selected` names, and rewrite `go.sum` to match.
///
/// Returns whether anything on disk changed. Rewritten **in place**, one version
/// token at a time, rather than regenerated from the requirement list: a
/// regenerated file would have to reproduce the shape's own formatting exactly or
/// every probe would leave a tree that differs from `HEAD` for reasons that have
/// nothing to do with a bump — and `is_clean()` would then be answering about this
/// function.
fn write_selection(root: &Path, selected: &BTreeMap<String, String>) -> bool {
    let before = read_go_mod(root);
    let after: String = before
        .lines()
        .map(|line| match parse_requirement(line) {
            Some((path, _, indirect)) => match selected.get(&path) {
                Some(version) => format!(
                    "require {path} {version}{}",
                    match indirect {
                        true => " // indirect",
                        false => "",
                    }
                ),
                None => line.to_string(),
            },
            None => line.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n");
    // `lines()` drops the trailing newline every shape writes; putting it back is
    // what keeps an unchanged rewrite byte-identical to what was committed.
    let after = format!("{after}\n");

    let mut changed = false;
    if after != before {
        write(&go_mod_path(root), &after);
        changed = true;
    }

    let sum_path = root.join("go.sum");
    if let Some(sum) = sum_for(&requirements(root)) {
        if std::fs::read_to_string(&sum_path).ok().as_deref() != Some(sum.as_str()) {
            write(&sum_path, &sum);
            changed = true;
        }
    }
    changed
}

/// The `go.sum` a set of requirements implies, or `None` where there are none.
///
/// The hashes are fabricated and cannot be otherwise: a real one is the digest of
/// a module zip that no offline fixture holds. What the file is for is the *path*
/// — Task 15 asserts a commit stages `go.mod` and `go.sum` and no third thing,
/// which needs the second file to exist, and rule 2's probe has to be shown
/// putting **both** back.
///
/// Shared with the shape that first writes the file, so a tree the proxy has
/// rewritten and a tree it has not are the same bytes when the versions agree.
pub fn sum_for(requirements: &[(String, String, bool)]) -> Option<String> {
    if requirements.is_empty() {
        return None;
    }
    // 43 characters and a pad: a `h1:` line is base64 over a 32-byte digest, and
    // go rejects one that is not the right length before it ever gets as far as
    // disagreeing about the value.
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

fn go_mod_path(root: &Path) -> PathBuf {
    root.join("go.mod")
}

/// What `go.mod` says now, or a panic naming the tree.
///
/// Panics rather than answering emptily, because every caller has already been
/// pointed at a module: an absent `go.mod` means the fixture handed over the wrong
/// directory, and reporting that as "no requirements" would surface as a rule
/// firing for a reason nobody can trace.
fn read_go_mod(root: &Path) -> String {
    std::fs::read_to_string(go_mod_path(root))
        .unwrap_or_else(|source| panic!("no go.mod in {}: {source}", root.display()))
}

fn write(path: &Path, contents: &str) {
    std::fs::write(path, contents)
        .unwrap_or_else(|source| panic!("could not write {}: {source}", path.display()));
}

// ---------------------------------------------------------------------------
// Resolving a version query
// ---------------------------------------------------------------------------

/// The highest release of `module` whose version begins with `query`.
fn highest_matching(module: &str, query: &str) -> Option<&'static str> {
    UPSTREAM
        .iter()
        .filter(|release| release.module == module && matches_query(release.version, query))
        .map(|release| release.version)
        .reduce(|highest, candidate| match newer(candidate, highest) {
            true => candidate,
            false => highest,
        })
}

/// Does `version` fall inside `query`?
///
/// Component-wise, so `v1.2` selects `v1.2.7` and never `v1.23.0`. A string prefix
/// would accept the second, and a probe that bumped a parent across a minor
/// because of a shared digit is precisely the change rule 2 is not allowed to
/// make.
fn matches_query(version: &str, query: &str) -> bool {
    let (Some(version), Some(query)) = (components(version), components(query)) else {
        return false;
    };
    version.len() >= query.len() && version[..query.len()] == query[..]
}

/// Is `candidate` above `incumbent`?
///
/// Numeric and component-wise, so `v1.9.12` is above `v1.9.9`. Deliberately
/// **not** `cve::version::at_least`: that function is the subject of Task 7's
/// lane and of this rule's confirm, and a fixture that resolved its own upstream
/// with it would be a world whose shape is decided by the code under test.
fn newer(candidate: &str, incumbent: &str) -> bool {
    match (components(candidate), components(incumbent)) {
        (Some(mut candidate), Some(mut incumbent)) => {
            let width = candidate.len().max(incumbent.len());
            candidate.resize(width, 0);
            incumbent.resize(width, 0);
            candidate > incumbent
        }
        _ => false,
    }
}

/// A version as numbers, without its leading `v`, or nothing at all.
fn components(version: &str) -> Option<Vec<u64>> {
    version
        .strip_prefix('v')
        .unwrap_or(version)
        .split('.')
        .map(|component| component.parse::<u64>().ok())
        .collect()
}
