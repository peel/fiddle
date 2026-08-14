//! Which module a finding is actually fixed by editing.
//!
//! A scanner names the package a vulnerability is *in*. That is very often not
//! the module anybody can edit: in the repository this milestone replaces
//! nearly every vulnerable dependency arrives `// indirect`, so `go.mod` holds
//! no line naming it, and a change written against it would either be reverted
//! by the next `go mod tidy` or refused outright. The module that gets edited is
//! what this module works out, and it is called the *bump target*.
//!
//! # The four rules, matched top-down
//!
//! 1. **Direct** — the module is the main module's own requirement, so it is its
//!    own target.
//! 2. **Indirect with a viable parent** — the first *direct* requirement in the
//!    module's `go mod why -m` chain, when that parent can carry the fix.
//! 3. **Indirect with no such parent** — the named module itself, added as a
//!    requirement so minimal version selection raises it for every consumer.
//! 4. **`OS`** — the `Dockerfile` base image tag, which is where a distribution
//!    package is fixed and where no Go rule can reach.
//!
//! Top-down is part of the contract rather than an implementation accident,
//! because rules 2 and 3 overlap: every world rule 2 applies to is a world rule
//! 3's guard also accepts, and a matcher that consulted 3 first would never
//! target a parent at all. The [`match_rules`] body is written as one ordered
//! sequence of early returns for that reason — an ordering expressed as a
//! sequence of statements is one a reader can check, where the same decision
//! spread across four `if`s in four functions is one they have to reconstruct.
//!
//! Rule 4 sits *after* the module rules and not before them, as the design
//! numbers it. It costs an OS finding two resolver calls that answer "not a
//! known dependency", and it buys the property that no rule is skipped: a
//! package type is a scanner's classification, and the module graph is the
//! thing that actually knows whether a path names a module.
//!
//! # No rule matched is not a rule
//!
//! A finding the main module does not need, and a finding in the standard
//! library, have **no bump target**, and this module says so rather than
//! guessing one. [`AttributionError::NoTarget`] carries the resolver's own
//! output verbatim, because a refusal is only actionable if the person reading
//! it can see what was asked and what answered — and because a refusal that
//! paraphrased would be a second place for this module's reading of `go` to be
//! wrong, silently.
//!
//! # This half is read-only
//!
//! Rule 2's parent is *viable* only when a newer release inside the parent's own
//! current minor resolves the named module to at least the finding's
//! `fixedVersion`, and that is not a fact any tree on disk holds — it lives in
//! whatever answers the module proxy. Establishing it means bumping the parent,
//! running `go mod tidy`, confirming with [`crate::cve::version::at_least`], and
//! restoring `go.mod`/`go.sum` when it fails. That probe mutates a tree and this
//! module does not: **[`ModuleGraph`] has no mutating method**, so the seam is
//! held by the port's shape rather than by a comment. See
//! [`the_direct_parent_in_the_chain`] for exactly where the probe attaches and
//! what rule 2 answers until it does.

use fiddle_core::{PackageType, ProjectedFinding};

/// Which of the four rules produced a target.
///
/// Carried beside the target rather than inside it, because three of the four
/// rules end in [`Target::Module`] and the target alone therefore does not say
/// which reasoning produced it. A pull request that bumps a module the scanner
/// never named has to be able to state why, and grouping (Task 9) keys on the
/// *target* — so a rule folded into the target would split one bump into as many
/// groups as there are rules that reached it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rule {
    /// The module is a direct requirement of the main module.
    One,
    /// The module is indirect and a direct requirement in its chain carries the
    /// fix.
    Two,
    /// The module is indirect and nothing in its chain can carry the fix, so it
    /// is raised itself.
    Three,
    /// The finding is against an OS package, which no Go rule can reach.
    Four,
}

/// What gets edited.
///
/// Two variants and no third: every Go rule ends in a module path, and every OS
/// finding ends at the base image tag. Deliberately rule-free — see [`Rule`] —
/// and ordered and hashable because grouping keys on it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Target {
    /// A module requirement in `go.mod`, by path.
    Module(String),
    /// The base image tag in the `Dockerfile`.
    DockerfileBaseImage,
}

/// A target, the rule that produced it, and what the resolver said on the way.
///
/// The third field is not decoration. The whole difficulty of attribution is
/// that the module edited is usually not the module reported, so the claim *this
/// finding is fixed by bumping that parent* is one nobody downstream can check
/// unless the chain travels with it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attribution {
    target: Target,
    rule: Rule,
    resolved: String,
}

impl Attribution {
    /// What gets edited.
    pub fn target(&self) -> &Target {
        &self.target
    }

    /// Which rule produced it.
    pub fn rule(&self) -> Rule {
        self.rule
    }

    /// Every resolver command run for this finding and what each one printed,
    /// in order.
    pub fn resolved(&self) -> &str {
        &self.resolved
    }
}

/// The resolver could not be asked.
///
/// Distinct from a resolver that answered unhelpfully: `go` reporting that a
/// module is not a known dependency is an *answer*, and one three of the four
/// rules are matched against. This type is for the case where there was no
/// answer at all — no `go` to run, a tree that is not a module, a deadline. A
/// caller cannot act on it and must not treat it as "no target".
#[derive(Debug, thiserror::Error)]
#[error("`{command}` could not be run: {message}")]
pub struct ResolverError {
    /// The command that was attempted, so the failure names itself.
    pub command: String,
    /// Whatever the runner reported.
    pub message: String,
}

/// Why a finding has no bump target.
#[derive(Debug, thiserror::Error)]
pub enum AttributionError {
    /// No rule matched, so there is nothing to edit and this build will not
    /// invent something.
    ///
    /// One field, and it is the resolver's own bytes. A `package` field would be
    /// redundant — every resolver command run for a finding names the package,
    /// so the output already carries it — and a `reason` field would be this
    /// module's paraphrase of the output sitting next to the output, free to
    /// disagree with it.
    #[error("no bump target; the resolver said:\n{resolved_output}")]
    NoTarget {
        /// Every command run and what it printed, verbatim.
        resolved_output: String,
    },
    /// The resolver itself failed. Not a needs-work verdict: nothing was
    /// established about the finding at all.
    #[error(transparent)]
    Resolver(#[from] ResolverError),
}

/// The read-only questions attribution asks about a Go module graph.
///
/// A port rather than a direct `go` invocation, for the reason the scanner is
/// one: the decision this module makes is a function of two commands' output,
/// and a suite that had to have a module proxy and a populated module cache in
/// order to assert anything about that function would assert nothing about it
/// at all. The adapter that spawns a real `go` under
/// [`crate::process::run_bounded`] belongs with the mutating half, which has to
/// spawn one anyway to run `go mod tidy`.
///
/// **Both methods return `go`'s output as text, including the text it prints
/// when it is refusing.** Parsing lives on this side of the port, so a stand-in
/// cannot quietly answer a *decision* where a real `go` answers a document —
/// and so [`AttributionError::NoTarget`] has something real to quote.
///
/// There is no method here that changes a tree, and that absence is the split
/// between this task and the one that measures parent viability.
#[async_trait::async_trait]
pub trait ModuleGraph: Sync {
    /// `go list -m -json <module>` — the module's record in the build list.
    async fn list(&self, module: &str) -> Result<String, ResolverError>;

    /// `go mod why -m <module>` — the chain by which the main module needs it.
    async fn why(&self, module: &str) -> Result<String, ResolverError>;
}

/// Work out what editing `finding` means, or refuse.
///
/// Generic and `?Sized` so a caller holding `&dyn ModuleGraph` can call it
/// without the port having to be object-safe at every call site.
pub async fn attribute<G>(
    finding: &ProjectedFinding,
    graph: &G,
) -> Result<Attribution, AttributionError>
where
    G: ModuleGraph + ?Sized,
{
    let package = finding.package.as_str();

    // Both observations are taken **before** any rule is matched, and both are
    // taken whichever rule ends up firing. That costs rule 1 one command it does
    // not read. What it buys is that the transcript a report carries is the same
    // set of observations no matter which branch produced it — so a reader
    // comparing two attributions is comparing answers rather than comparing what
    // each branch happened to have looked at — and that the text a refusal
    // quotes is the text the rules were matched over rather than a re-run.
    let mut resolved = Transcript::default();
    let listed = resolved.record(
        graph.list(package).await?,
        "go",
        ["list", "-m", "-json", package],
    );
    let why = resolved.record(
        graph.why(package).await?,
        "go",
        ["mod", "why", "-m", package],
    );

    let record = ModuleRecord::read(&listed);
    let chain = Chain::read(&why);

    match_rules(finding, &record, &chain, graph, &mut resolved).await
}

/// The four rules, in order, and the refusal underneath them.
///
/// One function of ordered early returns rather than a matcher per rule: the
/// order *is* the contract — see this module's header — and an order spread
/// across several functions is one a reader has to reconstruct rather than read.
async fn match_rules<G>(
    finding: &ProjectedFinding,
    record: &Option<ModuleRecord>,
    chain: &Chain,
    graph: &G,
    resolved: &mut Transcript,
) -> Result<Attribution, AttributionError>
where
    G: ModuleGraph + ?Sized,
{
    let package = finding.package.as_str();

    if let Some(record) = record {
        // Rule 1. `go list -m -json` omits `Indirect` for a direct requirement,
        // so this reads "the build list knows this module and does not call it
        // indirect" — see [`ModuleRecord`].
        if !record.indirect {
            return Ok(Attribution {
                target: Target::Module(package.to_string()),
                rule: Rule::One,
                resolved: resolved.take(),
            });
        }

        // Rule 2, then rule 3. These are the overlapping pair, and the only
        // thing separating them is whether the chain offers a parent to bump
        // instead.
        if let Some(parent) =
            the_direct_parent_in_the_chain(chain, package, graph, resolved).await?
        {
            return Ok(Attribution {
                target: Target::Module(parent),
                rule: Rule::Two,
                resolved: resolved.take(),
            });
        }

        return Ok(Attribution {
            target: Target::Module(package.to_string()),
            rule: Rule::Three,
            resolved: resolved.take(),
        });
    }

    // Rule 4. Last, as the design numbers it: the module graph has already been
    // asked and has said it does not know this path, which is what an OS package
    // name looks like from inside Go and is the honest reason to stop asking Go
    // about it.
    if finding.package_type == PackageType::Os {
        return Ok(Attribution {
            target: Target::DockerfileBaseImage,
            rule: Rule::Four,
            resolved: resolved.take(),
        });
    }

    Err(AttributionError::NoTarget {
        resolved_output: resolved.take(),
    })
}

/// The first module in `chain` that the main module requires directly.
///
/// # Where the viability probe attaches
///
/// The design's rule 2 has two halves: *the first direct requirement in the
/// chain*, and *and it can carry the fix*. This function is the first half. The
/// second is measured — bump the parent, `go mod tidy`, confirm the named module
/// resolves to at least `fixedVersion`, restore `go.mod`/`go.sum` if it does not
/// — and it mutates the tree, which is the seam this task stops at. Until it
/// lands, a parent that exists is taken as a parent that works, and rule 3 is
/// reached only through its other arm: a chain with no direct requirement in it
/// at all. That is a real world rather than a placeholder — an untidied `go.mod`
/// marks a module `// indirect` while the main module imports it directly — but
/// it is not the whole of rule 3, and a reader should not take the tests over it
/// as evidence that a non-viable parent falls through.
///
/// # Why each candidate is asked rather than assumed
///
/// `go mod why -m` prints the chain and nothing about how each hop is required,
/// so directness is a second question and needs a second command. The first
/// entry is the main module — that is what a chain from the main module means —
/// and it is skipped by position *and* by the `Main` flag its record carries: a
/// main module has no `Indirect` key either, so a walk that only skipped by
/// position would elect the repository under repair as its own parent the moment
/// `go` changed how it prints a chain.
async fn the_direct_parent_in_the_chain<G>(
    chain: &Chain,
    package: &str,
    graph: &G,
    resolved: &mut Transcript,
) -> Result<Option<String>, ResolverError>
where
    G: ModuleGraph + ?Sized,
{
    for (position, hop) in chain.hops.iter().enumerate() {
        // Not the main module, and not the module the finding is about: rule 2
        // exists to move the edit somewhere else.
        if position == 0 || hop == package {
            continue;
        }
        let listed = resolved.record(graph.list(hop).await?, "go", ["list", "-m", "-json", hop]);
        match ModuleRecord::read(&listed) {
            Some(record) if !record.indirect && !record.main => return Ok(Some(hop.clone())),
            _ => continue,
        }
    }
    Ok(None)
}

/// What `go list -m -json <module>` said, when it said anything readable.
///
/// # `Indirect` absent means direct
///
/// `go` writes that field only when it is true, so rule 1's guard is the
/// *absence* of a key. `#[serde(default)]` is what encodes that, and it is the
/// one line in this file whose inversion is silent: with the field required,
/// every direct requirement would fail to parse and fall through to the OS rule
/// or to needs-work, which reads as a resolver problem rather than as a reading
/// mistake.
///
/// # Deliberately not `deny_unknown_fields`
///
/// The rest of this capability refuses documents with fields it does not know,
/// because those documents are prose written outside the build and crossing into
/// it. This one is not that boundary: it is a tool's own output, `go list -m
/// -json` prints a dozen keys today — `Dir`, `GoMod`, `GoVersion`, `Time` — and
/// prints more with each release. Refusing them would make this module fail on
/// the next Go toolchain while claiming the module was not in the build list.
#[derive(Debug, serde::Deserialize)]
struct ModuleRecord {
    /// Whether the requirement is indirect. Absent means direct.
    #[serde(default, rename = "Indirect")]
    indirect: bool,
    /// Whether this record is the main module itself.
    #[serde(default, rename = "Main")]
    main: bool,
}

impl ModuleRecord {
    /// The record, or nothing at all.
    ///
    /// `None` covers every way `go` declines to describe a module: the
    /// `go: module …: not a known dependency` line it prints for a path outside
    /// the build list, an empty answer, anything unparseable. They are one
    /// answer here on purpose — *the module graph does not have this path* — and
    /// they are the answer rules 3 and 4 are reached through.
    fn read(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }
}

/// What `go mod why -m <module>` said.
///
/// The output is a `#` line naming the module, then either the chain — one path
/// per line, the main module first — or a parenthesised sentence saying the main
/// module does not need it. Both shapes are read by the same walk, because the
/// distinction this module acts on is *is there a chain*, and an empty chain is
/// that answer whichever of the two produced it.
#[derive(Debug, Default)]
struct Chain {
    hops: Vec<String>,
}

impl Chain {
    fn read(text: &str) -> Self {
        let hops = text
            .lines()
            .map(str::trim)
            .filter(|line| {
                // Blank separates one module's answer from the next; `#` names
                // the module being explained; a parenthesised line is `go`
                // speaking rather than naming a path. None of the three is a
                // hop, and treating the parenthesised one as a hop is the
                // mistake that would elect `(main` as a bump target.
                !line.is_empty() && !line.starts_with('#') && !line.starts_with('(')
            })
            .map(str::to_string)
            .collect();
        Chain { hops }
    }
}

/// Every resolver command run for one finding, and what each printed.
///
/// A transcript rather than the last command's output, because attribution asks
/// between two and several commands and the interesting ones are the later:
/// *this hop is a direct requirement and that one is not* is the whole of rule
/// 2's reasoning, and it is only legible if the answers that produced it are
/// there. It is also what a refusal quotes, so a case that ran three commands
/// refuses with three.
#[derive(Debug, Default)]
struct Transcript(String);

impl Transcript {
    /// Record `output` as the answer to `program args`, and hand the output
    /// back so a caller records by using rather than remembering to.
    ///
    /// The shape is a shell transcript because that is the form in which it is
    /// useful: somebody reading a needs-work verdict can copy the line above the
    /// output and run it themselves.
    fn record<'a, A>(&mut self, output: String, program: &str, args: A) -> String
    where
        A: IntoIterator<Item = &'a str>,
    {
        let args = args.into_iter().collect::<Vec<_>>().join(" ");
        self.0
            .push_str(&format!("$ {program} {args}\n{}\n", output.trim_end()));
        output
    }

    /// The transcript so far, leaving the recorder empty.
    ///
    /// Taken rather than cloned at each of the five exits, because every one of
    /// them is the end of this finding's attribution and a copy would be a copy
    /// nobody reads.
    fn take(&mut self) -> String {
        std::mem::take(&mut self.0)
    }
}
