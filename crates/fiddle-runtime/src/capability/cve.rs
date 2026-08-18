//! The one step of the CVE capability a model is consulted for.
//!
//! Everything else this milestone does is arithmetic over facts. The version
//! comparison ([`crate::cve::version`]), the four attribution rules and the
//! resolver commands behind them ([`crate::cve::attribute`]), the grouping and
//! the version each group moves to ([`crate::cve::group`]), the already-fixed
//! set ([`crate::cve::dedup`]), the fold rule ([`crate::cve::fold`]), the five
//! checks and both rescan conditions ([`crate::evaluate`]) — every one of those
//! is decided in Rust, before or after this module runs, and **none of them is
//! in the prompt**.
//!
//! That is not tidiness. A model that can be told the version comparison can
//! also get it wrong, and a wrong version comparison presents as a security
//! fix's commit message over a *downgrade*. Design §2's phase table settles each
//! row the same way and leaves exactly one for a model: *a uniform mechanical
//! migration forced by the bump* — reading code and editing every call site,
//! including the tests. Reading code is the thing no amount of arithmetic
//! replaces; everything around it is arithmetic, so everything around it stays
//! out.
//!
//! # What goes to the model
//!
//! Two things, and [`migration_task`] is the whole composition:
//!
//! 1. **The projection.** [`ProjectedFinding`]'s six fields, rendered by
//!    exhaustive destructuring so that a seventh field could not join them
//!    without this file failing to compile. The type is already the injection
//!    boundary — a real scanner record carries dozens of keys, most of them
//!    advisory prose, and nothing can deserialize one into a `ProjectedFinding`
//!    because it declares `deny_unknown_fields` over six names. This module is
//!    the next link in that chain: the projection is what reaches the model, and
//!    nothing joins it on the way.
//! 2. **The scope rules** — [`SCOPE_RULES`], the *scope* half of the skill's
//!    phase B1 and no other half of it. What a bump may touch, and what makes a
//!    group `needs-work`.
//!
//! And three things do not, each excluded for its own reason. **No advisory
//! prose**, because the projection cannot carry any. **No mechanical rule**, for
//! the reason above. **No host fact** — no workspace root, no fixture path —
//! which is M1's rule unchanged: everything here is sent to the provider, so a
//! preamble naming the worktree leaks it exactly as a tool argument would.
//!
//! `tests/cve_protocol.rs` asserts all four against the *serialized outbound
//! request* rather than against the builders that produced it, and it plants
//! each excluded thing somewhere upstream first, because an assertion that a
//! string is absent says nothing unless the string was there to be carried.
//!
//! # What this module does not decide, and who does
//!
//! Not the outcome. [`GroupMigration::migrate`] runs one bounded attempt and
//! hands back what the model claimed beside what git saw change; it branches on
//! neither. Classifying the diff the attempt left behind — the forbidden shapes,
//! and the group status that follows — is Task 14.b, and the check that overrules
//! a claim is [`crate::evaluate`]'s five-check contract rather than the single
//! command [`ToolHost`] carries for the `run_check` tool. Both of those read a
//! [`MigrationAttempt`], and this module deliberately reaches no verdict for them
//! to have to undo.
//!
//! # And the decision that comes before any of it: which tree to work in
//!
//! [`plan_shared_pull_request`] is the other half of this file and it runs
//! *first*, before a worktree exists and before a model is consulted. It answers
//! one question — which branch does this run add to — and it is the only thing
//! here that can refuse the run outright. See the section it opens.

use super::propose::COMMITTER;
use super::CapabilityError;
use crate::agent::{attempt_briefed, AgentBudget, Brief, RepairReport, ToolHost, ToolReceipts};
use crate::cve::attribute::Target;
use crate::cve::dedup::{Local, Spawn};
use crate::cve::fold::{fold_commit_argv, Landed};
use crate::cve::group::Group;
use crate::effect::{Executor, IntegrationOperation};
use crate::evaluate::{Evaluation, RescanVerdict};
use crate::github::{
    find_labelled_pull_request, EnsureBranchPublished, EnsurePullRequest, EnsurePullRequestBody,
    SharedPullRequest,
};
use crate::workspace::{
    Content, FileEdit, Workspace, WorkspaceCommand, WorkspaceError, WorkspacePath,
};
use crate::{GhCli, GhError};
use async_trait::async_trait;
use fiddle_core::{CapabilityId, EffectKind, ProjectedFinding, ProposedEffect};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// What the model is told about the situation it is in.
///
/// It names the tools rather than describing the host, for the reason the tool
/// schemas do: everything here is sent to the provider, so a preamble mentioning
/// the workspace root would leak it just as surely as a tool argument would. M1's
/// own preamble is the model for this one and is deliberately not *reused* — it
/// says "one small Rust project", and telling a model working in a Go tree that
/// it is in a Rust one is a lie with no upside.
const MIGRATION_PREAMBLE: &str = "\
You are making one mechanical change to one Go project. You can read its files, \
list them, replace a file's contents, and run the project's check. You cannot do \
anything else, and there is nothing outside the project you can reach.\n\
\n\
Work in small steps: read before you write, and run the check after you write. \
Change as few files as you can. When you are done — or when you are certain you \
cannot finish — reply with the structured report and nothing else. Report what \
you actually changed, whether or not it worked.";

/// What fiddle says before the findings, in fiddle's own voice.
///
/// Before, for [`crate::agent`]'s reason: a frame that followed the data could be
/// disowned by the data. The projection's *field names* are fiddle's, but three
/// of its values — `package`, `current`, `fixedVersion` — are strings a scanner
/// document supplied, so the ordering discipline is worth keeping even though the
/// six-field boundary already stopped the free text.
const FINDINGS_FRAME: &str = "\
A dependency bump has already been applied to this project to clear the \
advisories below. It may have broken the build. These are the advisories, and \
they are here so you know what the bump was for — there is nothing for you to \
decide about them.";

/// The scope half of the skill's rules, and the only half of it.
///
/// Design §2.5 states these in the same words, and they are the *instructions to
/// a judgment step* that section separates from the mechanical rules: what a bump
/// may touch, and what puts a group back to a person. The counts and comparisons
/// that decide everything else are not here and are not anywhere else in this
/// prompt.
///
/// Whether an edit actually stayed uniform is not settled by having said so. This
/// text is what the model is asked for; Task 14.b's classification of the diff is
/// what decides, and the five checks are what overrule both.
const SCOPE_RULES: &str = "\
Three kinds of edit are in scope, and no others: the dependency bump itself, a \
base-image tag bump, and one exception — a uniform mechanical migration the bump \
forced. Uniform means the same rename or the same signature change applied \
identically at every call site, including every file whose name ends _test.go.\n\
\n\
Everything else stops this group and must be left for a person. A source edit \
that is not uniform, any new control flow, any changed or removed test \
assertion, any added t.Skip, and any replace directive in go.mod are all out of \
scope. If the migration cannot be made uniform, change nothing further and say \
so in your report.";

/// The instruction that closes the prompt.
///
/// Last, so the final words are fiddle's — the same placement
/// [`crate::agent`]'s `INSTRUCTION_CLOSING` argues for.
const TASK: &str = "\
Make the project build again by carrying out the migration the bump forced, \
staying inside the scope above, then report what you did.";

/// One projected finding, as the model sees it.
///
/// **Destructured exhaustively rather than read field by field.** A seventh field
/// on [`ProjectedFinding`] fails to compile here, so this rendering cannot
/// silently begin carrying something the six-field contract was written to keep
/// out — which is the same device `cve::project::record` uses at the other end of
/// the boundary, where the six reads are the whole of what crosses.
///
/// `severity` and `package_type` are rendered through `Debug`, which for two
/// field-less enums is the variant name and nothing else. Nothing asserts over
/// the rendering's *shape*; it is prose for a model to read.
fn render(finding: &ProjectedFinding) -> String {
    let ProjectedFinding {
        cve,
        package,
        current,
        fixed_version,
        severity,
        package_type,
    } = finding;
    // The three spellings of "no fix published" that `ProjectedFinding`
    // documents, collapsed the way `project::names_a_fix` collapses them: a
    // `fixedVersion` of `""` names no release, and telling a model to upgrade to
    // it would be telling it to upgrade to nothing.
    let fixed = fixed_version
        .as_deref()
        .filter(|version| !version.trim().is_empty())
        .unwrap_or("no published fix");
    format!(
        "- {} in {package} {current}, fixed in {fixed} ({severity:?}, {package_type:?} package)",
        cve.as_str()
    )
}

/// The prompt one migration attempt opens with.
///
/// A pure function of the findings, separated from [`GroupMigration::migrate`] so
/// that what a model is shown can be asserted without a model, a socket or a
/// worktree — the same split [`crate::agent`]'s `task_for` is under, and for the
/// same reason.
///
/// The order is frame, findings, scope, instruction: fiddle's words on both sides
/// of the only part any of whose bytes came from outside.
fn migration_task(findings: &[&ProjectedFinding]) -> String {
    let rendered: Vec<String> = findings.iter().map(|finding| render(finding)).collect();
    format!(
        "{FINDINGS_FRAME}\n\n{}\n\n{SCOPE_RULES}\n\n{TASK}",
        rendered.join("\n")
    )
}

// ---------------------------------------------------------------------------
// Classifying the diff the attempt left behind
// ---------------------------------------------------------------------------

/// Go's own failure-reporting calls, and the two assertion libraries a Go
/// project is most likely to reach for.
///
/// A **named list rather than a heuristic**, because "is this line an
/// assertion" has no syntactic answer in Go: the language has no `assert`, and a
/// test fails by calling a method on its `*testing.T`. `t.Errorf` and `t.Fatalf`
/// are covered by their prefixes, which is why they are not spelled again.
///
/// A line matching one of these on the *removed* side is the whole rule, and
/// that is deliberate: a **changed** assertion is a removed line and an added
/// one, so one condition covers both halves of Design §2.5's "any changed or
/// removed test assertion". An assertion that is only *added* is not matched,
/// and should not be — a migration that had to re-spell a call inside a new
/// assertion has still stopped the group through the removed line it replaced.
const ASSERTIONS: [&str; 4] = ["t.Error", "t.Fatal", "assert.", "require."];

/// The three spellings of skipping a Go test.
///
/// Matched with the receiver left open — `.Skip(` rather than `t.Skip(` —
/// because the receiver is whatever the test named its `*testing.T`, and a rule
/// that only knew `t` would be defeated by a table-driven test whose parameter
/// is `tt`.
const SKIPS: [&str; 3] = [".Skip(", ".Skipf(", ".SkipNow("];

/// Go keywords that introduce a branch, a loop or a scheduled call.
///
/// **Counted as tokens, never matched as substrings.** `notify` contains `if`,
/// and a substring rule would put every group that renamed a notifier back to a
/// person. See [`keywords`].
///
/// `go` and `defer` are here beside the branches because the question the scope
/// rules ask is not "is this a branch" but "did the attempt write behaviour that
/// was not there". A goroutine and a deferred close are both that, and both are
/// exactly the kind of thing a bump's migration should stop for.
const CONTROL_FLOW: [&str; 11] = [
    "if", "else", "for", "switch", "select", "case", "goto", "break", "continue", "go", "defer",
];

/// One thing the scope rules forbid, and the evidence for it.
///
/// Each variant carries what was found rather than only which rule fired, for
/// [`RescanVerdict::StillReported`]'s reason: an operator reading *this group
/// needs work* has to be able to see the line without going and diffing the
/// worktree, which by then no longer exists.
///
/// # What is not here, and why it is not a gap being hidden
///
/// Design §2.5 names **five** things that stop a group, and the fifth — *any
/// non-uniform source edit* — is not a variant. It is not detectable from a
/// diff: whether two call-site edits are "the same rename" is the judgement the
/// model was asked to make, and a classifier that guessed at it would be the
/// mechanical-rule-in-the-prompt mistake made in the other direction. What
/// catches a non-uniform edit is the half that was always going to: `go build`
/// and `go vet` fail on the call site the model missed, and
/// [`GroupStatus::of`]'s second row refuses the group. The four below are the
/// ones a diff can settle, and they are exactly the four that a *green* build
/// would otherwise let through — an added `t.Skip` makes the tests pass, a
/// weakened assertion makes them pass, a `replace` directive makes the build
/// pass, and new control flow can make all of them pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForbiddenShape {
    /// A test the attempt switched off.
    AddedSkip {
        /// The file it was added to.
        path: String,
        /// The line, as written.
        line: String,
    },

    /// An assertion that a `_test.go` file used to make and does not now.
    ChangedTestAssertion {
        /// The file it left.
        path: String,
        /// The assertion, as it read before.
        assertion: String,
    },

    /// A `replace` directive the attempt put in a `go.mod`.
    ReplaceDirective {
        /// The `go.mod` it was written to.
        path: String,
        /// The directive, as written.
        directive: String,
    },

    /// More of some control-flow keyword than the file had before.
    NewControlFlow {
        /// The file it appeared in.
        path: String,
        /// Which keyword.
        keyword: &'static str,
        /// How many the file had.
        before: usize,
        /// How many it has now.
        after: usize,
    },

    /// A changed file whose bytes this build cannot read as text.
    ///
    /// **Forbidden rather than ignored**, and that is the fail-closed direction.
    /// The scope rules are an allowlist — three kinds of edit are in scope and
    /// no others — so an edit that cannot be read cannot be shown to be one of
    /// the three. Every edit a bump's migration legitimately makes is to Go
    /// source, a `go.mod`, a `go.sum` or a `Dockerfile`, all of which are text.
    UnreadableEdit {
        /// The file.
        path: String,
    },
}

/// Every forbidden shape in `edits`, in path order.
///
/// **All of them, not the first.** [`GroupStatus::of`] only needs one to refuse
/// a group, but an operator fixing the group by hand wants the list, and a
/// classifier that stopped early would make *how much is wrong here* a question
/// nobody could answer without re-running the attempt.
///
/// Every rule is applied to the files it is about — the assertion rule only to
/// `_test.go`, the directive rule only to a `go.mod` — because a rule applied
/// everywhere is a rule that fires on a `README` mentioning `t.Skip`.
///
/// # The limits, stated rather than discovered
///
/// This reads lines, not Go. A control-flow keyword inside a string literal or
/// a comment is counted, so a model that rewrote the message `"stop if empty"`
/// adds an `if` as far as this is concerned. That is a false *needs-work*, which
/// costs one group a person's attention; the alternative is a Go parser in a
/// crate that has no business having one, and every error it made would be a
/// false *clean*. The direction is chosen, not settled by accident.
fn classify(edits: &[FileEdit]) -> Vec<ForbiddenShape> {
    let mut found = Vec::new();
    for edit in edits {
        let path = edit.path.as_str();

        // Refused *before* any line rule rather than beside them. Both non-text
        // states render as no lines at all, so every line of a readable side
        // would look added — or removed — against an opaque one, and the file
        // would then be reported under whichever rules those phantom lines
        // happened to match. One shape that says "this could not be read" is
        // worth more than four that were invented from a side nobody read.
        if edit.unreadable() {
            found.push(ForbiddenShape::UnreadableEdit {
                path: path.to_string(),
            });
            continue;
        }

        if is_go_test(path) {
            for line in edit.added() {
                if SKIPS.iter().any(|skip| line.contains(skip)) {
                    found.push(ForbiddenShape::AddedSkip {
                        path: path.to_string(),
                        line: line.trim().to_string(),
                    });
                }
            }
            // The *removed* side, which is both halves of "changed or removed".
            // See [`ASSERTIONS`].
            for line in edit.removed() {
                if ASSERTIONS.iter().any(|call| line.contains(call)) {
                    found.push(ForbiddenShape::ChangedTestAssertion {
                        path: path.to_string(),
                        assertion: line.trim().to_string(),
                    });
                }
            }
        }

        if is_go_mod(path) {
            for line in edit.added() {
                if replaces(line) {
                    found.push(ForbiddenShape::ReplaceDirective {
                        path: path.to_string(),
                        directive: line.trim().to_string(),
                    });
                }
            }
        }

        // Go source only, `_test.go` included. The keywords are Go's, and two of
        // them are words other file types spell for their own reasons — a
        // `go.mod` opens with a `go` directive, and a `Dockerfile` that builds a
        // Go project says `golang` and may well say `go` — so a rule applied to
        // every changed path would refuse a group for a manifest line that means
        // nothing of the kind.
        if is_go(path) {
            for keyword in CONTROL_FLOW {
                let before = keywords(side(&edit.before), keyword);
                let after = keywords(side(&edit.after), keyword);
                // Strictly more, not merely different. A migration that *removed*
                // a branch has not written behaviour that was not there, and the
                // scope rules are about what the attempt added.
                if after > before {
                    found.push(ForbiddenShape::NewControlFlow {
                        path: path.to_string(),
                        keyword,
                        before,
                        after,
                    });
                }
            }
        }
    }
    found
}

/// Whether `path` is Go source.
///
/// `go.mod` is not, and the suffix says so on its own: it ends `.mod`.
fn is_go(path: &str) -> bool {
    path.ends_with(".go")
}

/// Whether `path` is a Go test file, by the only definition Go has — the one
/// the toolchain itself uses and the one [`SCOPE_RULES`] quotes to the model.
fn is_go_test(path: &str) -> bool {
    path.ends_with("_test.go")
}

/// Whether `path` is a module manifest.
///
/// The nested spelling is checked too, because a repository with more than one
/// module has a `go.mod` per module and a `replace` in any of them is the same
/// redirection. Anchored on the separator rather than matched as a suffix, so a
/// file called `nogo.mod` is not one.
fn is_go_mod(path: &str) -> bool {
    path == "go.mod" || path.ends_with("/go.mod")
}

/// Whether an added `go.mod` line introduces a module replacement.
///
/// Two conditions, because `go.mod` has two spellings and a rule that knew only
/// the first would be defeated by reformatting. `replace <old> => <new>` is a
/// single line carrying the keyword; a `replace ( … )` block puts the keyword on
/// a line of its own and every entry inside it on a line that does not carry it.
/// What those entries have instead is `=>`, and no other directive in the
/// `go.mod` grammar uses that operator — `module`, `go`, `toolchain`, `require`,
/// `exclude` and `retract` are all keyword-and-arguments — so between them the
/// two conditions catch a replacement added either way.
///
/// Both can hold of one line, which is why this answers a `bool` rather than
/// pushing a shape per condition: a single-line directive is one thing the rules
/// forbid, not two.
fn replaces(line: &str) -> bool {
    let line = line.trim();
    line.contains("=>")
        || line
            .strip_prefix("replace")
            .is_some_and(|rest| rest.starts_with([' ', '\t', '(']))
}

/// One side of an edit, as text.
///
/// A collapse local to this module rather than [`Content`]'s own, which is
/// private precisely because it is only sound where the caller has already
/// disposed of [`Content::Opaque`]. This one has: [`classify`] refuses an
/// unreadable edit and moves on before it counts anything, so the only state
/// being flattened here is [`Content::Absent`] — a file that is not there, which
/// really does contain no keywords.
fn side(content: &Content) -> &str {
    match content {
        Content::Text(text) => text,
        Content::Absent | Content::Opaque => "",
    }
}

/// How many times `keyword` appears in `text` as a whole word.
///
/// Splitting on everything that cannot be in a Go identifier is what makes this
/// a token count: `notify`, `elsewhere` and `switching` each split into one word
/// that is not a keyword, where a substring search finds `if`, `else` and
/// `switch` in them.
fn keywords(text: &str, keyword: &str) -> usize {
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|word| *word == keyword)
        .count()
}

/// How a group's one attempt ended.
///
/// **Deliberately separate from [`Fold`](crate::cve::fold::Fold)**, which says
/// whether to *run* an attempt. That one is decided before an attempt and over
/// another group's evidence, this one after one and over its own, and a single
/// type would let a caller ask a group that never ran how it went.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GroupStatus {
    /// The tree is proved better than the one it started from, and everything
    /// the attempt did was in scope. Commit it.
    Clean,

    /// Leave it for a person, and revert. See [`NeedsWork`].
    NeedsWork {
        /// Which of the three ways it ended up here.
        reason: NeedsWork,
    },
}

/// Why a group is being left for a person.
///
/// **Not [`crate::evaluate::Reason`]**, which is now closed at nine — the two an
/// *evaluation* produces and the seven a *disposition* does. These are the
/// reasons a *group* stops, one per row of [`GroupStatus::of`]'s table, and they
/// stayed separate through that extension rather than being folded into it: two
/// of the three are things an evaluation has no vocabulary for, and a run's
/// reason and a group's reason are different fields of different records — a
/// run has one and holds several groups.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NeedsWork {
    /// The attempt edited something the scope rules do not allow it to.
    OutOfScope(ForbiddenShape),

    /// A check refused the tree. Carries the check's own command line, which is
    /// how [`CheckResult`](crate::evaluate::CheckResult) names itself.
    CheckFailed {
        /// The earliest failing check, in declared order.
        check: String,
    },

    /// Every check passed and the rescan still did not prove the tree better.
    ///
    /// A third row rather than a second spelling of the first, because the two
    /// are different situations for an operator: a failing check is something
    /// wrong with the tree, and this is a repair that may well be fine and
    /// cannot be shown to be — a moved scanner feed, an array nobody reported
    /// on. [`Evaluation::accepted`] collapses them; this keeps them apart.
    Unproved(RescanVerdict),
}

/// The sentence a needs-work verdict reports, and **the wording is the
/// interface**.
///
/// Written here for the reason [`GroupError`](crate::cve::group::GroupError)
/// spells out on its own enum: the rationale a verdict carries is this value's
/// own `Display`, so a reader looking for what an operator will see in the
/// ticket finds it beside the variant that decided it rather than in the module
/// that prints it. [`crate::cve::verdict`] copies this text into a
/// [`Verdict`](crate::cve::verdict::Verdict) and alters nothing.
///
/// Each arm names the thing that was found and not merely which rule fired,
/// which is [`ForbiddenShape`]'s argument arrived at one layer out: by the time
/// anybody reads a verdict the worktree is gone, so a sentence that said only
/// *an out-of-scope edit* would leave nothing to act on.
impl std::fmt::Display for NeedsWork {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NeedsWork::OutOfScope(shape) => write!(f, "{shape}"),
            NeedsWork::CheckFailed { check } => {
                write!(f, "`{check}` did not pass over the tree the attempt left")
            }
            // The rescan arm is the one that is *not* something wrong with the
            // tree, and the wording keeps that distinction: a repair that may
            // well be fine and cannot be shown to be. See `NeedsWork::Unproved`.
            NeedsWork::Unproved(verdict) => write!(f, "{}", unproved_sentence(verdict)),
        }
    }
}

/// What a [`RescanVerdict`] that is not `Cleared` leaves a person to act on.
///
/// A free function rather than a `Display` on [`RescanVerdict`] itself, because
/// that type is `crate::evaluate`'s and the sentence is this capability's: the
/// same verdict is read by [`Evaluation::accepted`] as a boolean and by a
/// reviewer as prose, and only the second wants words.
///
/// [`RescanVerdict::Cleared`] is in the match because the match has no wildcard
/// — a verdict added to that enum has to be ruled on here rather than defaulting
/// to a sentence that would then be wrong. Reaching it means a caller asked for
/// the rationale of a group that passed, which is a bug in the caller and says
/// so.
fn unproved_sentence(verdict: &RescanVerdict) -> String {
    match verdict {
        RescanVerdict::Cleared => {
            "the rescan proved this group clean, so there is nothing to report".to_string()
        }
        RescanVerdict::NotCompared => {
            "no rescan was compared, so the repair is unproved".to_string()
        }
        RescanVerdict::Provisional(_) => {
            "the rescan ran at a different scanner version, so the comparison is provisional"
                .to_string()
        }
        RescanVerdict::NotObserved { array } => {
            format!("the rescan reported no `{array}` array at all, so it proved nothing about it")
        }
        RescanVerdict::StillReported(cves) => format!(
            "still reported after the bump: {}",
            cves.iter()
                .map(|cve| cve.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ),
        RescanVerdict::NewFinding(_) => {
            "the bump introduced a finding the input scan did not report".to_string()
        }
        RescanVerdict::Unreadable(why) => {
            format!("the rescan wrote a document this build cannot read: {why}")
        }
    }
}

/// What an operator has to go and undo, named with the evidence for it.
///
/// The path is first in every arm because it is what a person opens.
impl std::fmt::Display for ForbiddenShape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ForbiddenShape::AddedSkip { path, line } => {
                write!(f, "{path} added a skipped test: {line}")
            }
            ForbiddenShape::ChangedTestAssertion { path, assertion } => {
                write!(f, "{path} no longer asserts: {assertion}")
            }
            ForbiddenShape::ReplaceDirective { path, directive } => {
                write!(f, "{path} gained a replace directive: {directive}")
            }
            ForbiddenShape::NewControlFlow {
                path,
                keyword,
                before,
                after,
            } => write!(
                f,
                "{path} went from {before} to {after} `{keyword}` keywords, \
                 which is new control flow rather than a migration"
            ),
            ForbiddenShape::UnreadableEdit { path } => write!(
                f,
                "{path} was changed and this build cannot read its bytes as text"
            ),
        }
    }
}

impl GroupStatus {
    /// The first-match-wins table Design §2 puts in the *Rust* column.
    ///
    /// Three rows, in this order, and the order is the substance:
    ///
    /// 1. **A forbidden shape refuses the group whatever the checks said.** It
    ///    has to come first, because the shapes are precisely the edits that
    ///    make a check pass when it should not: a `t.Skip` turns a failing test
    ///    green, and a table that consulted the checks first would call that
    ///    group clean and commit it.
    /// 2. **Otherwise the checks decide**, earliest failure first.
    /// 3. **And a rescan that proved nothing is not a pass** — see
    ///    [`Evaluation::accepted`], whose exact condition rows 2 and 3 together
    ///    reproduce. `Clean` is returned if and only if `accepted()` is true and
    ///    nothing was out of scope; `a_clean_group_is_exactly_an_accepted_one`
    ///    is what holds that rather than this comment.
    ///
    /// # What this function is not given
    ///
    /// A [`MigrationAttempt`], and therefore a [`RepairReport`], and therefore
    /// `claimed_complete`. That is the point of the signature: *the model's
    /// claim is branched on nowhere* is not a property of the body below, which
    /// anybody could later edit — it is a property of what the body can reach,
    /// and the claim is not among it. The claim is still recorded, on
    /// [`MigrationAttempt::report`], where a disposition publishes it beside the
    /// verdict that overruled it.
    pub fn of(evaluation: &Evaluation, forbidden: &[ForbiddenShape]) -> GroupStatus {
        // Row 1. The *first* shape in path order, because [`classify`] returns
        // every one it found and this row needs a reason rather than a list —
        // the list is on [`MigrationAttempt::forbidden`], where an operator
        // reads it.
        if let Some(shape) = forbidden.first() {
            return GroupStatus::NeedsWork {
                reason: NeedsWork::OutOfScope(shape.clone()),
            };
        }

        // Row 2. The check's own command line, which is how a `CheckResult`
        // names itself, so the refusal an operator reads is the thing they would
        // type to see it again.
        if let Some(failed) = evaluation.first_failure() {
            return GroupStatus::NeedsWork {
                reason: NeedsWork::CheckFailed {
                    check: failed.name.clone(),
                },
            };
        }

        // Row 3. `Cleared` is the one arm that is proof — every other one is a
        // rescan that did not compare, could not be read, or contradicted the
        // repair. Matching the arm that passes and defaulting the rest is what
        // makes a verdict added to [`RescanVerdict`] tomorrow fail closed here.
        match evaluation.rescan() {
            RescanVerdict::Cleared => GroupStatus::Clean,
            unproved => GroupStatus::NeedsWork {
                reason: NeedsWork::Unproved(unproved.clone()),
            },
        }
    }
}

/// Everything one migration attempt needs that is not the model.
///
/// One struct rather than five arguments, for [`super::RepairConfig`]'s reason:
/// each field is a host decision an operator configures, none is derivable from
/// the others, and grouping them keeps the model — the one value with a
/// credential behind it — visibly separate.
///
/// There is no attempt id here, and **no tree and no workspace root either**.
/// All three were supplied by whoever built this until a run existed to own
/// them, and each moved out for its own reason:
///
/// - The attempt id arrives on the [`ExecutionGrant`](super::ExecutionGrant), so
///   there is nowhere for a second one to be supplied from — the defect that
///   type's doc records, avoided by construction rather than by remembering.
/// - The tree and the root moved to the *caller*, because
///   [`GroupMigration::migrate`] no longer creates the worktree it works in: a
///   run mitigates several groups onto **one** branch, and each landing has to
///   be a commit in the tree the next group starts from. A worktree per group
///   would put each group's commit in a different `HEAD`, and the push at the
///   end would carry one of them. See [`GroupMigration::migrate`]'s own header.
pub struct MigrationConfig {
    /// The command the `run_check` tool runs.
    ///
    /// **Not the check that decides anything.** M4's verdict comes from
    /// [`crate::evaluate`]'s five-check contract, run over the tree the attempt
    /// left behind; this is the one command the M1 tool surface offers a model so
    /// that it can see for itself whether its edit builds. Which command that
    /// should be is the operator's, and it is a host fact — so it is here and not
    /// in the prompt.
    pub check: WorkspaceCommand,

    /// What one bounded attempt runs inside.
    pub budget: AgentBudget,

    /// Stops the attempt, the tools and the check together.
    pub cancel: CancellationToken,
}

/// What one bounded migration attempt left behind.
///
/// Two fields, of two different kinds, and keeping them apart is the point.
/// [`MigrationAttempt::report`] is what the model *said*; every field of it is a
/// claim, `claimed_complete` included. [`MigrationAttempt::changed`] is what git
/// *saw*, asked of the tree rather than of the report, because a changed-file
/// list the model authored is a claim about something fiddle can simply go and
/// look at.
///
/// [`MigrationAttempt::forbidden`] is the third kind and is neither of the
/// first two: it is what the *scope rules* make of what git saw, computed while
/// the worktree still existed. It is still not a verdict — a group with no
/// forbidden shape is not thereby clean, because the checks have not been asked
/// yet. [`GroupStatus::of`] is the only thing here that answers, and it is a
/// free function over this and an [`Evaluation`] rather than a method, so that
/// the report cannot reach it.
#[derive(Debug)]
pub struct MigrationAttempt {
    /// What the model said it did.
    pub report: RepairReport,

    /// What git saw change in the worktree, under the ignore rules the project
    /// had committed before the attempt began.
    pub changed: Vec<WorkspacePath>,

    /// Every shape in that diff the scope rules forbid, in path order, and
    /// empty where there was none. See [`classify`].
    pub forbidden: Vec<ForbiddenShape>,
}

/// One bounded agent attempt at the migration a bump forced.
///
/// Generic over Rig's own completion-model trait for the reason
/// [`attempt_briefed`] is: a test substitutes a scripted model and drives the
/// real tools, the real worktree and the real prompt composition without a
/// credential or a socket, so what reaches a provider is provable offline.
///
/// Deliberately **not** a [`Capability`](super::Capability). A capability is a
/// thing an `ExecutionGrant` authorises and `CAPABILITIES` names, and this is one
/// step inside a larger one — the step where a model is consulted. Giving it an
/// id of its own would advertise it as separately executable, which it is not.
pub struct GroupMigration<M> {
    model: M,
    config: MigrationConfig,
    /// The record the tools append to, held here rather than only inside the
    /// [`ToolHost`] so it can be read *after* the attempt, including after one
    /// that failed. The host gets a clone of this same `Arc`, so there is one
    /// record and no copy-back step an early return could skip.
    receipts: Arc<Mutex<ToolReceipts>>,
}

impl<M> GroupMigration<M> {
    /// A migration that will run `model` under `config`.
    pub fn new(model: M, config: MigrationConfig) -> Self {
        GroupMigration {
            model,
            config,
            receipts: Arc::new(Mutex::new(ToolReceipts::default())),
        }
    }

    /// What the tools recorded, whatever became of the attempt.
    pub fn receipts(&self) -> ToolReceipts {
        self.receipts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl<M> GroupMigration<M>
where
    M: rig_core::completion::CompletionModel + 'static,
{
    /// Run one bounded attempt at `group`'s migration, in a worktree of its own.
    ///
    /// # What is read off the group, and what is left behind
    ///
    /// A [`Group`] is attribution's and grouping's answer: it carries a
    /// [`Target`](crate::cve::attribute::Target) per finding, which is the module
    /// or the `Dockerfile` line four mechanical rules elected, and which the
    /// bump — applied before this runs — was written against. **None of that
    /// reaches the model.** The single expression below takes `.finding()` off
    /// each [`Attributed`](crate::cve::group::Attributed) and drops the rest, and
    /// that narrowing is the criterion this module is written to: the target is
    /// how the tree got into the state the model is looking at, not something the
    /// model has any part in deciding.
    ///
    /// # The worktree is the run's, and this does not create or destroy one
    ///
    /// It used to. `migrate` branched a worktree of a configured tree at that
    /// tree's `HEAD` and dropped it before returning, which settled two questions
    /// wrongly at once and left a third unanswerable:
    ///
    /// - **`HEAD` is the wrong revision.** [`check_out`] is what says which
    ///   revision every worktree in a run is made at — the base, or the shared
    ///   pull request's remote tip — and its answer had nowhere to go.
    /// - **The landing had nowhere to happen.** [`land`] commits *in the
    ///   worktree*, through [`InWorktree`], and a worktree dropped before this
    ///   function returns is gone by the time its caller holds the attempt. So
    ///   `land` could not be called from a production path at all.
    /// - **One branch, several groups.** A run puts every clean group's commit on
    ///   one branch and pushes once. A worktree per group would leave each commit
    ///   on a different detached `HEAD`.
    ///
    /// So the caller creates the workspace, at [`Checkout::revision`], and keeps
    /// it for the whole run. What 14.a refused — *handing the workspace back*, so
    /// that a caller could classify at its leisure in a tree the attempt no longer
    /// owns — is still refused: [`classify`] runs below, before this returns, and
    /// nothing about the diff is left for a caller to compute.
    ///
    /// The `Arc` is the tools' — [`ToolHost`] holds one — and this function
    /// borrows the caller's rather than making one, so there is exactly one
    /// workspace and one `Drop` for it.
    pub async fn migrate(
        &self,
        workspace: &Arc<Workspace>,
        group: &Group,
    ) -> Result<MigrationAttempt, CapabilityError> {
        let findings: Vec<&ProjectedFinding> = group
            .findings()
            .iter()
            .map(|attributed| attributed.finding())
            .collect();
        let task = migration_task(&findings);

        let host = ToolHost {
            workspace: Arc::clone(workspace),
            cancel: self.config.cancel.clone(),
            check: self.config.check.clone(),
            receipts: Arc::clone(&self.receipts),
        };

        let report = attempt_briefed(
            self.model.clone(),
            host,
            self.config.budget.clone(),
            Brief {
                preamble: MIGRATION_PREAMBLE,
                task: &task,
            },
        )
        .await?;

        // Asked of git rather than of the report, because the worktree is what
        // git is being asked about.
        //
        // **And classified here, which is 14.a's rule and survives the worktree
        // becoming the run's.** The scope rules are about the bytes inside the
        // changed files, and the alternative — answering with the workspace so a
        // caller could classify at its leisure — was rejected there and is still
        // refused: nothing about the diff leaves this function uncomputed, so no
        // caller can classify a tree that a later group has already moved on
        // from. What changed is only who owns the `Drop`.
        //
        // The path list is taken *from the edits* rather than asked for
        // separately, so that what was classified and what is reported as
        // changed cannot be two different answers to one question.
        let edits = workspace.edits()?;
        let changed = edits.iter().map(|edit| edit.path.clone()).collect();
        let forbidden = classify(&edits);

        Ok(MigrationAttempt {
            report,
            changed,
            forbidden,
        })
    }
}

// ---------------------------------------------------------------------------
// What a group's outcome does to the branch
// ---------------------------------------------------------------------------

/// Every git this stage runs, and the one seam it runs them through.
///
/// **Not for substitutability.** [`crate::cve::dedup::Spawn`] gives the argument
/// in full and it is the same one here: a test holding an implementation of this
/// holds the *complete list* of what the landing ran, which is what turns "this
/// never rewrites history" and "this never stages by directory" from sentences in
/// a comment into assertions over a list. Both of those are negatives, and a
/// negative over a list nobody kept is satisfied by keeping no list.
///
/// One method and not one per porcelain verb. The three criteria are about the
/// *whole* set of invocations — a fifth verb added below has to appear in the
/// recorded list whether or not anybody remembered to widen a trait, and a trait
/// with a method per verb would let a new one arrive through a new method that no
/// existing assertion walks.
///
/// `Ok` is stdout for the one caller that reads it — the `ls-tree` probe in
/// [`revert`] — and a non-zero exit is an `Err` rather than a status a caller
/// interprets, because there is no call here where failing is an answer. That is
/// the opposite of [`crate::cve::dedup::Spawn`]'s choice, and deliberately so:
/// dedup has exactly one call whose non-zero exit *is* the answer
/// (`rev-parse --verify --quiet`), and this has none.
#[async_trait]
pub trait Git: Sync {
    /// Run `git` with `args` in the repository this is bound to, and hand back
    /// what it printed.
    async fn run(&self, args: &[&str]) -> Result<String, CapabilityError>;
}

/// The production one: git inside the attempt's worktree.
///
/// **It composes a spawn site and adds none**, which is
/// [`crate::evaluate::in_workspace`]'s arrangement and its reason.
/// [`Workspace::run`] owns the four-name environment a child of an attempt sees,
/// the relativisation applied to what it printed, and the process-group bound; a
/// `git` spawned beside that would be a second environment to keep in step, and
/// `workspace::a_workspace_command_inherits_no_credential` would stop being a
/// statement about how this crate's git children actually run.
///
/// Borrows the workspace rather than owning it for [`InWorkspace`]'s reason: the
/// workspace is the attempt's, its [`Drop`] removes the worktree, and a landing
/// is one of several things that happen inside one. It has to happen *before*
/// that drop — the commit is made in a worktree of the fixture, and a worktree
/// shares the object store it was branched from, which is what leaves the object
/// behind once the worktree is gone.
///
/// [`InWorkspace`]: crate::evaluate::in_workspace::InWorkspace
pub struct InWorktree<'a> {
    workspace: &'a Workspace,
    timeout: Duration,
}

impl<'a> InWorktree<'a> {
    /// Run git in `workspace`, giving each invocation `timeout`.
    ///
    /// The bound is the caller's — in practice [`AgentBudget::tool_timeout`], the
    /// ceiling the host already set on any single program this attempt runs, which
    /// is the same value [`super::ProposeChange`]'s committer uses. A second
    /// wall-clock policy for git alone is one nobody has written down.
    pub fn new(workspace: &'a Workspace, timeout: Duration) -> Self {
        InWorktree { workspace, timeout }
    }
}

#[async_trait]
impl Git for InWorktree<'_> {
    async fn run(&self, args: &[&str]) -> Result<String, CapabilityError> {
        let command = WorkspaceCommand {
            program: "git".to_string(),
            args: args.iter().map(|argument| argument.to_string()).collect(),
            timeout: self.timeout,
        };
        let result = self.workspace.run(&command).await?;
        match result.exit_code {
            0 => Ok(result.stdout),
            _ => Err(CapabilityError::Workspace(WorkspaceError::Git {
                command: args.join(" "),
                stderr: result.stderr,
            })),
        }
    }
}

/// The other production one: git in the repository the worktrees are branched
/// from.
///
/// [`InWorktree`] is the adapter for everything that happens *inside* an
/// attempt's tree, and there is exactly one thing that has to happen outside one:
/// [`check_out`], which fetches the refs a run cares about and resolves the
/// revision the worktree will be made at. There is no worktree yet when it runs —
/// choosing its revision is what it answers — so there is no [`Workspace`] to
/// compose, and this is the seam that was missing.
///
/// # It composes [`crate::cve::dedup::Local`], and adds no spawn of its own
///
/// That is the same arrangement [`InWorktree`] is under, pointed at the other
/// tree. `commit_log_dedup` already runs plain `git` in *this* directory, with
/// the ambient environment and no deadline, and its own header argues the case:
/// a local repository read carries no credential, so it does not go through the
/// one credential-carrying `git` this crate builds. Reusing that spawn rather
/// than writing a second one is what keeps the number of ways this crate starts
/// a `git` in the base repository at one.
///
/// **One thing is genuinely wider here and is worth saying rather than
/// inheriting.** [`crate::cve::dedup::Local`]'s doc says every command it runs is
/// a local read "with no network in it", and two of [`check_out`]'s four are
/// `git fetch`. So the deployment assumption is explicit: the checkout this run
/// is pointed at is one that can already reach its own remote — which is what a
/// CI checkout is, and what a developer's clone is. A repository whose remote
/// needs a credential this process holds and the checkout does not is a
/// deployment this adapter does not serve, and it fails at the fetch, by name,
/// before anything has been committed.
///
/// The spawn is blocking, so it is moved off the runtime rather than run on it:
/// a `fetch` can take seconds, and the only other task alive during a run is the
/// interrupt handler.
pub struct InRepository {
    repository: PathBuf,
}

impl InRepository {
    /// Run git in `repository` — the tree the run's worktree is branched from.
    pub fn new(repository: impl Into<PathBuf>) -> Self {
        InRepository {
            repository: repository.into(),
        }
    }
}

#[async_trait]
impl Git for InRepository {
    async fn run(&self, args: &[&str]) -> Result<String, CapabilityError> {
        let repository = self.repository.clone();
        let owned: Vec<String> = args.iter().map(|argument| argument.to_string()).collect();
        let spelled = owned.join(" ");
        let ran = tokio::task::spawn_blocking(move || {
            let borrowed: Vec<&str> = owned.iter().map(String::as_str).collect();
            Local.run("git", &borrowed, &repository)
        })
        .await
        // A panic inside the blocking pool is not something this can recover
        // from and is not something a caller can act on differently from a git
        // that would not start, so it arrives as the same refusal.
        .map_err(|joined| {
            CapabilityError::Workspace(WorkspaceError::Git {
                command: spelled.clone(),
                stderr: joined.to_string(),
            })
        })??;

        match ran.ok {
            true => Ok(ran.stdout),
            // [`Git`]'s contract: a non-zero exit is an `Err` and never a status
            // a caller interprets, because there is no call in this stage where
            // failing is an answer.
            false => Err(CapabilityError::Workspace(WorkspaceError::Git {
                command: spelled,
                stderr: ran.stderr,
            })),
        }
    }
}

/// Stage exactly the files this group edited and commit them, or put them back;
/// either way, say which happened.
///
/// # The commit gate is [`GroupStatus`], and it is not [`Evaluation::accepted`]
///
/// The two come apart on exactly one case — **a forbidden shape with green
/// checks** — and that is the case where committing lands a `t.Skip` on the
/// branch. [`GroupStatus::of`]'s first row exists for it: the forbidden shapes
/// are precisely the edits that make a check pass when it should not, so a
/// landing that consulted the checks would commit the one diff the classifier
/// was written to stop.
///
/// It is settled by the signature rather than by the body below. There is no
/// [`Evaluation`] here to read — the same device [`GroupStatus::of`] uses in the
/// other direction, where the model's claim is kept out of reach rather than left
/// unread — so *the checks are not the commit gate* is a property of what this
/// function can see, which nobody can edit away without changing the signature.
///
/// The consequence of getting it wrong is not local: [`crate::cve::fold`]'s
/// `ended_clean` still reads `accepted()`, so a group that both landed and folded
/// on a green-checked forbidden shape would record a *later* group's advisories
/// as fixed by an edit that should never have been on the branch.
///
/// # `changed` is what git saw, not what anybody asked for
///
/// It is [`MigrationAttempt::changed`], which is
/// [`Workspace::changed_files`] under the ignore rules the project had committed
/// before the attempt began — and it is also the list [`classify`] read. Those
/// being one list is the whole safety argument for staging by name: a commit may
/// carry only what was classified, because a file that reached the commit by some
/// other route reached it without any scope rule having been applied to it.
pub async fn land<G>(
    git: &G,
    group: &Group,
    status: &GroupStatus,
    changed: &[WorkspacePath],
) -> Result<Landed, CapabilityError>
where
    G: Git + ?Sized,
{
    match lands_as(status) {
        Landed::Committed => {
            stage_and_commit(git, group, changed).await?;
            Ok(Landed::Committed)
        }
        Landed::Reverted => {
            revert(git, changed).await?;
            Ok(Landed::Reverted)
        }
    }
}

/// What a status does to the branch, decided once.
///
/// A function of [`GroupStatus`] alone, and the only place the decision is taken:
/// [`land`] matches on what this answered rather than on the status again, so the
/// git that follows cannot come to a different conclusion from the value that is
/// handed back to [`crate::cve::fold`].
///
/// [`GroupStatus::Clean`] is the commit gate and everything else reverts. There is
/// no third arm and no arm that inspects [`NeedsWork`]'s reason: a group refused
/// for a failing check and a group refused for an added `t.Skip` are the same
/// thing to a branch.
fn lands_as(status: &GroupStatus) -> Landed {
    match status {
        GroupStatus::Clean => Landed::Committed,
        GroupStatus::NeedsWork { .. } => Landed::Reverted,
    }
}

/// Stage `changed` by name and make one commit of it.
async fn stage_and_commit<G>(
    git: &G,
    group: &Group,
    changed: &[WorkspacePath],
) -> Result<(), CapabilityError>
where
    G: Git + ?Sized,
{
    // A clean group that changed nothing has nothing to commit, and neither
    // nearby answer is honest: `--allow-empty` would put a body naming every one
    // of this group's advisories on the branch with no fix under it, which the
    // next run's log scan reads as *these are done*, and answering
    // [`Landed::Committed`] with no commit would tell the fold rule the branch
    // carries a tree it does not. So it is a refusal, and the same one
    // [`super::ProposeChange`] gives for the same tree.
    if changed.is_empty() {
        return Err(CapabilityError::NothingProposed);
    }

    // `add -f` over the named paths and never `add -A`. Two separate reasons, and
    // both matter:
    //
    // - **By name**, because the list is the one [`classify`] was applied to. A
    //   path that reached the commit any other way reached it with no scope rule
    //   having looked at it, and the four shapes are exactly the edits a green
    //   build would otherwise let through.
    // - **`-f`**, for [`super::ProposeChange::commit`]'s reason: `changed` is
    //   derived under the ignore rules the project had committed *before* the
    //   attempt, and an `add` honouring the worktree's own rules would let an
    //   attempt that wrote `*` into `.gitignore` decide what gets committed. The
    //   checks would then have passed over a tree that is not the tree the commit
    //   carries.
    let paths: Vec<&str> = changed.iter().map(|path| path.as_str()).collect();
    let mut add = vec!["add", "-f", "--"];
    add.extend_from_slice(&paths);
    git.run(&add).await?;

    // Two `-m` arguments rather than one string with a blank line in it: git's
    // own way of separating a subject from a body, so the separation is not a
    // `\n\n` this file has to get right.
    let subject = commit_subject(group);
    let body = commit_body(group);
    let mut commit: Vec<&str> = COMMITTER
        .iter()
        .flat_map(|setting| ["-c", setting])
        .collect();
    commit.extend(["commit", "-q", "-m", subject.as_str(), "-m", body.as_str()]);
    git.run(&commit).await?;
    Ok(())
}

/// Put `changed` back the way `HEAD` has it, and nothing else back.
///
/// # Two commands, because a changed path is not always a tracked one
///
/// `git checkout HEAD --` is the whole of the revert for a file the attempt
/// *edited*, and it is no part of it for a file the attempt *created*: git
/// refuses the pathspec outright, which would fail the revert for every path
/// beside it, and even if it did not there is no `HEAD` version of a new file to
/// restore. `changed` holds both kinds — [`Workspace::changed_files`] is the
/// tracked half from `git status` plus the created half from
/// `ls-files --others` — so the two halves are separated by asking `HEAD` which
/// paths it carries, and each is undone the only way it can be.
///
/// `git clean` is the created half's answer and is bounded to `--` and the named
/// paths, exactly as the checkout is. It is the one command here that deletes,
/// which is why it is never given a directory and never given `-d`.
///
/// # `HEAD --` and not a bare `--`
///
/// A bare `git checkout -- <path>` restores from the *index*, so a path the
/// attempt had somehow staged would come back as the staged version rather than
/// as the committed one. Nothing in an attempt runs git — the tool surface is
/// read, write, list and one check — so the index should already agree with
/// `HEAD`; naming `HEAD` makes the revert say what it means instead of resting on
/// that.
async fn revert<G>(git: &G, changed: &[WorkspacePath]) -> Result<(), CapabilityError>
where
    G: Git + ?Sized,
{
    // Nothing changed, so there is nothing to put back — and an unbounded
    // `checkout` or `clean` with an empty pathspec would be the whole worktree,
    // which is the one thing a revert by name must never become.
    if changed.is_empty() {
        return Ok(());
    }
    let paths: Vec<&str> = changed.iter().map(|path| path.as_str()).collect();

    let mut probe = vec!["ls-tree", "-r", "--name-only", "-z", "HEAD", "--"];
    probe.extend_from_slice(&paths);
    let listed = git.run(&probe).await?;
    let committed: BTreeSet<&str> = listed.split('\0').filter(|it| !it.is_empty()).collect();

    let (edited, created): (Vec<&str>, Vec<&str>) = paths
        .iter()
        .copied()
        .partition(|path| committed.contains(path));

    if !edited.is_empty() {
        let mut checkout = vec!["checkout", "HEAD", "--"];
        checkout.extend_from_slice(&edited);
        git.run(&checkout).await?;
    }
    if !created.is_empty() {
        let mut clean = vec!["clean", "-f", "-q", "--"];
        clean.extend_from_slice(&created);
        git.run(&clean).await?;
    }
    Ok(())
}

/// The one line a clean group's commit opens with.
///
/// **It names no advisory**, and that is not brevity. The subject is part of the
/// body `git log --format=%B` prints, so an id here is an id
/// [`crate::cve::dedup::FixedInCommits`] reads — which is fine for a group that
/// really was fixed and would be a false claim for anything else. Keeping the ids
/// to one place means there is one place to be careful about.
fn commit_subject(group: &Group) -> String {
    let count = group.cves().len();
    let advisories = match count {
        1 => "1 advisory".to_string(),
        many => format!("{many} advisories"),
    };
    match group.target() {
        Target::Module(path) => format!("fix: bump {path} for {advisories}"),
        Target::DockerfileBaseImage => format!("fix: bump the base image for {advisories}"),
    }
}

/// Every advisory this group's edit fixes, one per line.
///
/// **Every one of them, not the first.** [`crate::cve::dedup`]'s log scan is what
/// recovers the already-fixed set on the next run, and it matches each id
/// independently precisely because one body may name several — so a body naming
/// one of a group's four leaves three to be re-proposed against a tree that
/// already carries their fix, and `group::GroupError::AlreadyAtTheFix` is then the
/// only thing standing between that and a downgrade under a security fix's commit
/// message.
///
/// A `Fixes:` trailer per id rather than a bare list, because that is what the
/// line is for and because a person reads this log too. The scan splits on
/// everything that is not alphanumeric or a hyphen, so the word `Fixes` joins its
/// word set and can answer nothing wrong — see [`FixedInCommits::read`].
///
/// [`FixedInCommits::read`]: crate::cve::dedup::FixedInCommits::read
fn commit_body(group: &Group) -> String {
    group
        .cves()
        .iter()
        .map(|cve| format!("Fixes: {}", cve.as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Record a fold: the empty commit whose body is the whole of what it is.
///
/// [`fold_commit_argv`] decides the flags and the body and spawns nothing; this
/// is the caller it was left for. The pair it settles —
/// **`--allow-empty` and never `--amend`** — survives the move because this runs
/// its output rather than restating it, so the flags cannot drift from the test
/// that pins them.
///
/// # It does not answer [`Landed`], and that is not an omission
///
/// A fold *is* the decision not to attempt a group, so there is no attempt, no
/// evaluation and no rescan. [`crate::cve::fold::PriorRescan`] is built from an
/// [`Evaluation`] and a [`Landed`] together, and there is no evaluation here to
/// pair one with — a `Landed::Committed` handed back from a fold would be an
/// invitation to build a prior rescan out of somebody else's evidence, which is
/// the exact confusion the fold rule's two gates exist to prevent.
pub async fn record_fold<G>(git: &G, group: &Group) -> Result<(), CapabilityError>
where
    G: Git + ?Sized,
{
    let argv = fold_commit_argv(group);
    let mut call: Vec<&str> = COMMITTER
        .iter()
        .flat_map(|setting| ["-c", setting])
        .collect();
    call.extend(argv.iter().map(|argument| argument.as_str()));
    git.run(&call).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Which branch this run adds to, decided before it touches a tree
// ---------------------------------------------------------------------------

/// The label that makes the shared pull request findable.
///
/// **The discriminator, and not decoration.** Design §4's whole model is one
/// pull request per repository rather than one per advisory, and nothing else
/// identifies it: the branch name is dated so it changes, the title names no
/// advisory, and the body is prose that a rescan rewrites. A pull request opened
/// without this label is invisible to [`plan_shared_pull_request`], and the next
/// run opens a second one.
///
/// ADR 019 records why this rather than an identity: `effect_id` prevents a
/// duplicate *effect*, and this finds existing *work*. A run discovering the
/// world for itself has no earlier run's identity to recompute.
pub const CVE_LABEL: &str = "security/cve";

/// What a branch this capability may push to is named.
///
/// # A constant, and deliberately not a configuration key
///
/// It is the same fact as [`CVE_LABEL`] and [`BRANCH_STEM`], written a third
/// time: all three are `security/…`, and they are one convention rather than
/// three settings. Making one of them configurable would let a deployment set a
/// prefix that admits no branch this capability cuts, or that admits branches
/// nothing here would ever push — and the misconfiguration's symptom is exactly
/// the failure the guard exists to prevent, discovered after a commit.
/// `the_branch_this_capability_cuts_satisfies_its_own_push_guard` is what holds
/// the three together.
///
/// It is also **not** M2's `fiddle/` namespace, which
/// [`branch_name`](crate::github::branch_name) derives for a branch named after
/// an effect identity. That one is per-run and opaque; this one is a durable,
/// human-legible branch a person may look at for weeks, and a security team that
/// grants push to `security/*` is granting it to a name they can read.
///
/// The trailing `/` is part of it. Without one, `security-theatre/x` would be
/// admitted by a prefix meant to name a namespace.
pub const PUSHABLE_PREFIX: &str = "security/";

/// What a fresh branch is called, before the date.
///
/// Dated, because merged branches persist on the remote: a fixed name would
/// eventually be pushed onto a branch that had already been merged and deleted —
/// or worse, one that had been merged and not deleted, whose history is already
/// in the base. Design §4 states it.
pub const BRANCH_STEM: &str = "security/cve-remediation-";

/// Why a run may not proceed against the pull request it found.
///
/// One variant, and it is the whole of what this decision can refuse. Everything
/// else the discovery read can answer is a situation to work in: nothing open is
/// a fresh cut, one open is a reuse, and several open is a reuse with a note.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Refusal {
    /// The open pull request's head is a branch this capability may not push to.
    ///
    /// **Refused rather than worked around, because there is no workaround.**
    /// Opening a second pull request is the state the shared-PR model exists to
    /// prevent; pushing to the branch anyway is the state the prefix exists to
    /// prevent. What is left is to stop, and to stop *before* a commit — a run
    /// that checked the branch out, committed a bump onto it and only then found
    /// it could not push has written to a branch somebody else owns.
    ///
    /// Reachable by an ordinary mistake rather than by malice: a person adds
    /// `security/cve` to their own pull request, meaning *this is about the CVEs*,
    /// and fiddle now believes that branch is its shared one.
    #[error(
        "pull request #{number} carries the shared label and its head branch \
         `{head}` is not under `{prefix}`, which is the only namespace this \
         capability may push to"
    )]
    HeadOutsideThePushablePrefix {
        number: u64,
        head: String,
        prefix: &'static str,
    },
}

/// Everything that can stop this decision being reached.
///
/// Two variants and they are different kinds of fact, which is why they are not
/// one. A [`PlanError::Read`] is the forge being unreadable and says nothing
/// about the world; a [`PlanError::Refused`] is the world having been read
/// perfectly well and found to be one this run must not act in. A caller
/// reporting a run's reason has to be able to tell them apart — the first
/// invites another attempt and the second does not.
#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    /// The label search, or the pull request it named, could not be read.
    #[error("the shared pull request could not be looked up: {0}")]
    Read(#[from] GhError),

    /// The world was read, and this run may not proceed in it.
    #[error("{0}")]
    Refused(#[from] Refusal),
}

/// The branch this run works on, and how it came to be chosen.
///
/// **The only value that names a branch**, and that is the structural half of
/// *refuse before any commit*: [`plan`] answers this or a [`Refusal`], so after a
/// refusal there is nothing for a checkout, a commit or a push to be addressed
/// at. The ordering is not a convention a caller has to remember.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Approved {
    /// A shared pull request is already open; add to its branch.
    Reuse {
        /// The pull request being added to — the lowest, when there is more
        /// than one.
        number: u64,
        /// Its head branch, bare.
        branch: String,
        /// What that branch is at on the remote, as the forge reported it. See
        /// [`SharedPullRequest::head_sha`], and [`Checkout`] for what a run does
        /// with it.
        head_sha: String,
        /// The branch it is proposed into.
        ///
        /// Carried on this arm as well as on the fresh one, and not because a
        /// reuse cuts anything from it. Two things need it. The base revision is
        /// **observed on both arms** — Design §4 wants the bundle to carry the
        /// base revision *and* the pull request's head whichever the attempt ran
        /// against, because a run that recorded only one of them cannot be read
        /// afterwards. And [`EnsurePullRequest`]'s postcondition is a head *and
        /// a base*: a publication that guessed the base would fail to recognise
        /// the very pull request this arm was built from, and would propose a
        /// second.
        base: String,
        /// Every other open labelled pull request, ascending.
        duplicates: Vec<u64>,
    },

    /// Nothing is open; cut a dated branch and open one at the end.
    Fresh {
        /// [`BRANCH_STEM`] plus the date.
        branch: String,
        /// The branch it is cut from, bare — [`Approved::from`] is what turns it
        /// into the ref that is actually checked out.
        base: String,
    },
}

impl Approved {
    /// The branch this run commits onto.
    pub fn branch(&self) -> &str {
        match self {
            Approved::Reuse { branch, .. } | Approved::Fresh { branch, .. } => branch,
        }
    }

    /// The ref the worktree is created at, and it is a **remote-tracking ref in
    /// both arms**.
    ///
    /// Design §4: *never branch from local `HEAD` or local `main`*. Both are the
    /// same hazard seen twice — a clone this process did not create is a clone
    /// whose local refs are whatever the last thing to run in it left behind, and
    /// a `security/cve-remediation-…` from yesterday's run is exactly the kind of
    /// stale local branch a reuse would otherwise pick up. `origin/` is what says
    /// *the tip the forge is showing*, which is the tip the open pull request is
    /// actually about.
    ///
    /// Written here rather than at the call site because it is the one sentence
    /// this whole guard is for, and two call sites spelling it would be two
    /// chances to spell it `HEAD`.
    pub fn from(&self) -> String {
        match self {
            Approved::Reuse { branch, .. } => origin_ref(branch),
            Approved::Fresh { base, .. } => origin_ref(base),
        }
    }

    /// The branch this run's work is proposed into, bare.
    ///
    /// The same value on both arms and read for the same purpose: it is what
    /// `origin/<base>` in [`Checkout::base_revision`] is resolved from, and what
    /// a pull request's postcondition is matched on.
    pub fn base(&self) -> &str {
        match self {
            Approved::Reuse { base, .. } | Approved::Fresh { base, .. } => base,
        }
    }

    /// The remote tip of the shared pull request's head, or `None` for a fresh
    /// cut, where there is no pull request and therefore no head to have one.
    ///
    /// **The forge's observation, not git's.** It is what the bundle records as
    /// `pr_head` and what [`check_out`] makes the worktree at, so the revision a
    /// reader sees in the bundle is the revision the attempt provably ran
    /// against — see [`Checkout`] on the race this closes.
    pub fn pr_head(&self) -> Option<&str> {
        match self {
            Approved::Reuse { head_sha, .. } => Some(head_sha),
            Approved::Fresh { .. } => None,
        }
    }

    /// The pull request being added to, or `None` for a fresh cut.
    pub fn reused(&self) -> Option<u64> {
        match self {
            Approved::Reuse { number, .. } => Some(*number),
            Approved::Fresh { .. } => None,
        }
    }

    /// Every other open labelled pull request, ascending, and empty ordinarily.
    pub fn duplicates(&self) -> &[u64] {
        match self {
            Approved::Reuse { duplicates, .. } => duplicates,
            Approved::Fresh { .. } => &[],
        }
    }

    /// What the shared pull request's body has to say about this run's
    /// situation, or `None` when there is nothing to say.
    ///
    /// Today that is the duplicate anomaly and nothing else, which is why it is
    /// an `Option<String>` rather than a report type: one sentence, produced in
    /// one case. The rest of the body — the per-advisory disposition rows — is
    /// `cve::verdict`'s, and a shape invented here for it would be one that lane
    /// then had to fit into.
    ///
    /// **`None` when there is one pull request**, which is the ordinary run.
    /// A note that appeared every time would be an anomaly warning on a body
    /// nobody would then read.
    pub fn note(&self) -> Option<String> {
        duplicate_note(self.duplicates())
    }
}

/// The sentence a run puts in the body when it found more than one.
///
/// It names the extras and asks for them to be closed, and it does not close
/// them. Two open labelled pull requests is a state GitHub itself will not
/// produce — it refuses a second for one head and base — so it is something a
/// person did, and undoing somebody's deliberate act because a nightly job found
/// it surprising is not this run's decision to take. What the run can do is make
/// sure the person who did it finds out.
///
/// The numbers and not the branches, because a number is what a person clicks.
fn duplicate_note(duplicates: &[u64]) -> Option<String> {
    if duplicates.is_empty() {
        return None;
    }
    let listed: Vec<String> = duplicates.iter().map(|it| format!("#{it}")).collect();
    Some(format!(
        "More than one open pull request carries `{CVE_LABEL}`. This run added to \
         the lowest-numbered one and opened nothing new; {} {} still open and \
         should be closed by hand.",
        listed.join(", "),
        match duplicates.len() {
            1 => "is",
            _ => "are",
        }
    ))
}

/// The branch a run with nothing open cuts for itself.
fn dated_branch(today: &str) -> String {
    format!("{BRANCH_STEM}{today}")
}

/// Today, in UTC, as `YYYY-MM-DD`.
///
/// **The one clock read in this milestone, and it is deliberately not inside
/// [`plan`].** That function is pure precisely so every rule it encodes can be
/// asserted without a clock, and its own header says `today` is supplied "because
/// a branch name is a fact a caller has to be able to reproduce in a diagnostic
/// and in a test". This is what the binary supplies it from, kept beside the
/// value it produces so there is one spelling of the format the branch name
/// carries.
///
/// UTC and never local time. A branch name is compared across machines — a
/// nightly job's runner and the laptop of whoever is looking at the pull request
/// — and two of them in different zones would cut two branches for one day.
///
/// The arithmetic is Howard Hinnant's `civil_from_days`, written out rather than
/// taken as a dependency: it is fifteen lines of integer arithmetic that has not
/// changed since the Gregorian calendar was adopted, against a crate this
/// workspace would otherwise have no reason to carry. A clock before 1970 is not
/// a case this build has: `duration_since` refuses it, and a machine whose clock
/// is that wrong would cut a branch nobody could find whatever this returned, so
/// the epoch is the honest answer rather than a panic in a nightly job.
pub fn today_utc() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs() as i64)
        .unwrap_or(0);
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    format!("{year:04}-{month:02}-{day:02}")
}

/// The civil date `days` after 1970-01-01, by the era arithmetic that makes the
/// leap rule branchless.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01, which puts the leap day at the end of a
    // year and makes every 400-year era identical.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    // March is month 0 in this frame, so the two adjustments below put January
    // and February back into the following calendar year.
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = match month_prime < 10 {
        true => month_prime + 3,
        false => month_prime - 9,
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// Decide which branch this run works on, given what the forge was observed to
/// hold.
///
/// **Pure, and takes no [`Git`], no [`Workspace`] and no client.** That is what
/// makes it possible to answer *before any commit*: there is nothing here that
/// could commit, and the value it produces on a refusal names no branch for a
/// caller to commit onto. `plan_shared_pull_request` is the one that reads.
///
/// # One guard over one branch, both arms
///
/// The prefix is checked against *the branch this run will push*, whichever way
/// it was arrived at, rather than only against a discovered head. A fresh cut
/// satisfies it by construction — [`BRANCH_STEM`] is under [`PUSHABLE_PREFIX`] —
/// and checking it anyway costs one comparison and means the rule is stated once
/// rather than being true of one arm by accident.
///
/// # `base` is the fresh arm's, and the reuse arm takes the pull request's own
///
/// They are the same branch in every ordinary configuration and they are not the
/// same *fact*. `base` is what this deployment is configured to propose into; a
/// pull request that already exists was proposed into whatever it was proposed
/// into, and that is what its postcondition is matched on. A publication that
/// substituted the configured base for the observed one would fail to recognise
/// the pull request this arm was built from and would propose a second — which is
/// the one outcome the shared-PR model exists to prevent.
pub fn plan(
    found: Option<SharedPullRequest>,
    base: &str,
    today: &str,
) -> Result<Approved, Refusal> {
    let approved = match found {
        Some(shared) => Approved::Reuse {
            number: shared.number,
            branch: shared.head,
            head_sha: shared.head_sha,
            base: shared.base,
            duplicates: shared.duplicates,
        },
        None => Approved::Fresh {
            branch: dated_branch(today),
            base: base.to_string(),
        },
    };

    if !approved.branch().starts_with(PUSHABLE_PREFIX) {
        return Err(Refusal::HeadOutsideThePushablePrefix {
            // A refusal can only come from the reuse arm, because a fresh branch
            // is this capability's own name. `unwrap_or_default` rather than an
            // `expect` so an unreachable case is a `#0` in a diagnostic rather
            // than a panic in a nightly job.
            number: approved.reused().unwrap_or_default(),
            head: approved.branch().to_string(),
            prefix: PUSHABLE_PREFIX,
        });
    }
    Ok(approved)
}

/// Read the forge and decide, which is the whole of what happens before a run
/// touches a tree.
///
/// Two steps and no third: [`find_labelled_pull_request`] observes, [`plan`]
/// decides. Split that way because the observation needs a process and the
/// decision needs nothing at all, so every rule the decision encodes is testable
/// without one — and because a reader asking *what does this refuse* has one
/// function to read rather than a read interleaved with a rule.
///
/// `today` is supplied rather than read from a clock. A branch name is a fact a
/// caller has to be able to reproduce in a diagnostic and in a test, and a
/// function that reached for the wall clock would name a different branch every
/// midnight with nothing able to say which.
pub async fn plan_shared_pull_request(
    gh: &GhCli,
    repo: &str,
    base: &str,
    today: &str,
    cancel: &CancellationToken,
) -> Result<Approved, PlanError> {
    let found = find_labelled_pull_request(gh, repo, CVE_LABEL, cancel).await?;
    Ok(plan(found, base, today)?)
}

// ---------------------------------------------------------------------------
// Which revision the attempt's tree is made at, and both of the ones it saw
// ---------------------------------------------------------------------------

/// The remote a run fetches from and pushes to.
///
/// Written once here and once in [`crate::git::publish`], which is one more time
/// than ideal and cannot be helped: that module's is the name the push's argument
/// vector is asserted on, and this one is the name a fetch refspec is built from.
/// They are the same convention — a clone has exactly one remote and it is
/// `origin` — and a deployment that had renamed it would fail on the fetch here
/// before anything had been committed, which is the direction to fail in.
const REMOTE: &str = "origin";

/// Which of the two revisions a run observed the attempt's tree was made at.
///
/// Design §4: *the observation carries the base revision **and** the open PR's
/// head, and the bundle says which of the two the attempt actually ran against. A
/// run that recorded only one of them cannot be read afterwards.* This is the
/// "which".
///
/// The two spellings are the two keys [`Checkout::observed`] writes, so a reader
/// of a bundle finds the value of `attempt_tree` beside a key of the same name
/// rather than having to be told the mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttemptTree {
    /// Nothing was open, so the tree is `origin/<base>`.
    BaseRevision,
    /// A pull request was reused, so the tree is its remote tip.
    PrHead,
}

impl AttemptTree {
    /// The name the bundle carries, and the name of the key holding the value.
    pub fn as_str(&self) -> &'static str {
        match self {
            AttemptTree::BaseRevision => "base_revision",
            AttemptTree::PrHead => "pr_head",
        }
    }
}

/// The two revisions a run observed, and — by which variant it is — which of
/// them the attempt's worktree was made at.
///
/// # Why this is an enum and not three fields
///
/// Because the invariant is *the attempt ran at one of the two revisions this
/// value carries*, and three fields would let a caller build one that says
/// `attempt_tree: PrHead` with no pull request head in it. [`Checkout::revision`]
/// is then total, with no unreachable arm and no `unwrap` — the same device
/// [`Approved`] uses one step earlier, where only one variant names a pull
/// request number.
///
/// **The base revision is on both variants.** That is the sentence of Design §4
/// above, made structural: there is no way to build a checkout that recorded only
/// the revision it used.
///
/// # The revision is the forge's observation, not git's second look
///
/// On the reuse arm the tree is made at [`Approved::pr_head`] — the sha
/// `GET /pulls/{n}` reported — rather than at whatever `origin/<head>` resolves to
/// once the fetch has run. The two are the same object in the ordinary case and
/// they can differ: somebody pushes to the shared branch between the discovery
/// read and the fetch. Taking the fetched tip would leave the bundle naming one
/// revision and the attempt having run at another, which is the reading failure
/// this whole observation exists to prevent; taking the observed sha means the
/// bundle is right by construction, and the push that follows is refused as a
/// non-fast-forward — reported, and never forced.
///
/// What [`check_out`] does still need the fetch for is that the object be
/// *present*: a sha nothing brought into the store is a `git worktree add` that
/// fails, which is the failure to have rather than a silent branch from somewhere
/// else.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Checkout {
    /// Nothing open: the attempt's tree is the base revision.
    AtBaseRevision {
        /// What `origin/<base>` resolved to after the fetch.
        base_revision: String,
    },

    /// A pull request reused: the attempt's tree is its remote tip, and the base
    /// revision is observed beside it and used by nothing here.
    AtPullRequestHead {
        base_revision: String,
        /// The sha the forge reported for the pull request's head.
        pr_head: String,
    },
}

impl Checkout {
    /// The revision a worktree for this run is created at.
    pub fn revision(&self) -> &str {
        match self {
            Checkout::AtBaseRevision { base_revision } => base_revision,
            Checkout::AtPullRequestHead { pr_head, .. } => pr_head,
        }
    }

    /// What `origin/<base>` was observed to be, on both arms.
    pub fn base_revision(&self) -> &str {
        match self {
            Checkout::AtBaseRevision { base_revision }
            | Checkout::AtPullRequestHead { base_revision, .. } => base_revision,
        }
    }

    /// The reused pull request's remote tip, or `None` when none was open.
    pub fn pr_head(&self) -> Option<&str> {
        match self {
            Checkout::AtBaseRevision { .. } => None,
            Checkout::AtPullRequestHead { pr_head, .. } => Some(pr_head),
        }
    }

    /// Which of the two the attempt ran against.
    pub fn attempt_tree(&self) -> AttemptTree {
        match self {
            Checkout::AtBaseRevision { .. } => AttemptTree::BaseRevision,
            Checkout::AtPullRequestHead { .. } => AttemptTree::PrHead,
        }
    }

    /// What the run's bundle records about which tree this was.
    ///
    /// Three keys and no more. `pr_head` is `null` rather than absent on the
    /// fresh arm, because a reader asking *was a pull request reused* must be able
    /// to get an answer rather than a missing key that could equally mean a run of
    /// an older build.
    ///
    /// A [`serde_json::Value`] rather than a place in
    /// [`WorkStateView`](fiddle_core::WorkStateView), which is a closed set of
    /// four named ports belonging to M0's assessment and not to this capability.
    /// Where these three keys are placed in the published bundle is the wiring
    /// task's; what they are, and that a run produces all three, is this one's.
    pub fn observed(&self) -> serde_json::Value {
        serde_json::json!({
            "base_revision": self.base_revision(),
            "pr_head": self.pr_head(),
            "attempt_tree": self.attempt_tree().as_str(),
        })
    }
}

/// Bring the remote refs this run cares about into the store, and say which
/// revision the attempt's worktree is to be made at.
///
/// `git` runs in **the repository the worktrees will be branched from**, not in a
/// worktree — there is not one yet, and choosing its revision is what this
/// answers. [`Workspace::create_at`] is the caller's next call and it deliberately
/// fetches nothing; this is the fetch that makes the revision it is handed
/// resolvable.
///
/// # Why the base is fetched on the arm that does not use it
///
/// Two reasons and either would do. The bundle records it — see [`Checkout`] —
/// and [`crate::cve::dedup`]'s commit-log scan reads
/// `git log origin/<base>..HEAD` to recover which advisories the branch already
/// covers, which needs `origin/<base>` to be a ref that exists and is current. A
/// run that skipped it on the reuse arm would read an empty range and re-fix
/// everything the branch already carries.
///
/// # The refspec is explicit, and the `+` is not a force push
///
/// `git fetch origin <ref>` alone updates `FETCH_HEAD` and updates
/// `refs/remotes/origin/<ref>` only if the clone happens to have been configured
/// with a matching refspec — which a `--single-branch` clone has not, and which is
/// exactly the clone a CI checkout produces. Naming the destination makes the
/// remote-tracking ref a fact about this call rather than about the clone's
/// configuration.
///
/// The `+` forces the *local* remote-tracking ref to match the remote, which is
/// the whole job of a remote-tracking ref: without it a fetch after somebody
/// force-pushed the shared branch fails, and the run then works from a tip that
/// no longer exists. It is not `push --force`, `--force-with-lease`, `reset`,
/// `rebase` or `--amend`; nothing here writes to the remote or rewrites any local
/// history, which is what Design §2.7's list forbids.
pub async fn check_out<G>(git: &G, approved: &Approved) -> Result<Checkout, CapabilityError>
where
    G: Git + ?Sized,
{
    fetch(git, approved.base()).await?;

    let Some(pr_head) = approved.pr_head() else {
        // **The fresh arm's revision is [`Approved::from`]**, read through that
        // accessor rather than rebuilt here, because the one sentence this guard
        // exists for — never local `HEAD`, never local `main` — is written on it,
        // and two call sites spelling it would be two chances to spell it `HEAD`.
        return Ok(Checkout::AtBaseRevision {
            base_revision: resolve(git, &approved.from()).await?,
        });
    };

    // The reuse arm cannot use `from()` for either of its two revisions and that
    // is not an oversight: `from()` answers `origin/<head>` here, which is the
    // *branch* the tip is on rather than the base, and the revision the tree is
    // made at is the sha the forge reported rather than whatever that ref
    // resolves to. So the base is named through the same helper `from()` is built
    // from, which is what keeps one spelling of `origin/<x>` in this module.
    let base_revision = resolve(git, &origin_ref(approved.base())).await?;

    fetch(git, approved.branch()).await?;
    // Resolved rather than trusted, and this is the line that makes the fetch
    // above load-bearing rather than decorative: `<sha>^{commit}` fails unless the
    // store really holds that object *as a commit*. A `worktree add` at an absent
    // revision would fail too, and later, after the workspace root and the scratch
    // home had been created — so the failure is taken here, where it names the
    // revision the forge reported and nothing has been built yet.
    let pr_head = resolve(git, &format!("{pr_head}^{{commit}}")).await?;

    Ok(Checkout::AtPullRequestHead {
        base_revision,
        pr_head,
    })
}

/// The remote-tracking ref for `branch`.
///
/// One spelling, used by [`Approved::from`] and by [`check_out`]'s base
/// observation. A second would be a second chance to write a bare branch name
/// where a remote-tracking one belongs, which is the whole hazard Design §4 names.
fn origin_ref(branch: &str) -> String {
    format!("{REMOTE}/{branch}")
}

/// Bring `branch` from the remote into `refs/remotes/origin/<branch>`.
async fn fetch<G>(git: &G, branch: &str) -> Result<(), CapabilityError>
where
    G: Git + ?Sized,
{
    // `--no-tags` because a tag is a ref this run has no use for and every use
    // for not creating: fetching them by default is how a mirror of somebody
    // else's release history arrives in a clone that only wanted one branch.
    git.run(&[
        "fetch",
        "--no-tags",
        "--quiet",
        REMOTE,
        &format!("+refs/heads/{branch}:refs/remotes/{REMOTE}/{branch}"),
    ])
    .await
    .map(|_output| ())
}

/// What `revision` names, as a full object name.
///
/// `--verify` and a single argument, so a revision git cannot resolve is a
/// non-zero exit — [`Git`]'s `Err` — rather than the string echoed back. Without
/// it, `git rev-parse origin/nope` prints `origin/nope` and exits 128, and a
/// caller reading stdout alone would carry a branch name forward as a sha.
async fn resolve<G>(git: &G, revision: &str) -> Result<String, CapabilityError>
where
    G: Git + ?Sized,
{
    let printed = git
        .run(&["rev-parse", "--verify", "--quiet", revision])
        .await?;
    Ok(printed.trim().to_string())
}

// ---------------------------------------------------------------------------
// Publishing the shared work, and every external mutation through the executor
// ---------------------------------------------------------------------------

/// The host facts one publication of the shared work needs.
///
/// [`Approved`] already carries everything that was *decided* — the branch, the
/// base, the pull request being reused and the anomaly note. This carries what
/// nothing here can derive: which repository, whose fork the head lives on, what
/// the pull request is called, what this run has to say for itself, and the commit
/// the landing left on the branch.
pub struct SharedPublication {
    /// `owner/name`, as an API path spells it.
    pub repo: String,
    /// The owner the head branch lives under. Qualifying the head is what stops
    /// a lookup matching a branch of that name in another repository —
    /// `pull_request_effect.rs` states it in full.
    pub head_owner: String,
    /// The title, naming no advisory. The shared pull request outlives any one
    /// run's findings, which is the same reason the commit subject names none.
    pub title: String,
    /// What this run has to say about what it did, before the anomaly note is
    /// appended. See [`shared_body`].
    pub summary: String,
    /// The commit the branch must point at for the push's postcondition to hold.
    ///
    /// Supplied rather than resolved here for [`EnsureBranchPublished`]'s reason:
    /// an operation that read `HEAD` for itself could publish a commit its own
    /// proposal never named, with the payload hash still matching because the
    /// payload would never have carried it.
    pub head_sha: String,
}

/// What one publication of the shared work left behind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedWork {
    /// The branch, as the remote was *observed* to hold it.
    pub branch: String,
    /// What it was observed to point at — never what the push reported.
    pub head_sha: String,
    /// The one shared pull request: reused, or opened by this run.
    pub pull_request: u64,
}

/// Publish the branch, make sure the one shared pull request exists, and make
/// sure it says what this run did — all three through the effect executor, and
/// nothing else in this capability touches the forge at all.
///
/// # The routing is the point, and it is structural
///
/// [`AuthorizedEffect`](crate::effect::AuthorizedEffect) has no constructor
/// outside `effect/mod.rs`, so an [`IntegrationOperation`] cannot be applied
/// except by a walk of the authorization order — which announces every step it
/// takes to the attempt's journal. The consequence is what
/// `cve_shared_pr::every_external_mutation_passes_the_effect_executor` asserts: a
/// mutation reaching the forge outside a recorded `apply` window is not something
/// this function can produce, and the lane is what says nothing beside it does
/// either.
///
/// **A local commit is not one of these.** [`land`] runs `git add` and
/// `git commit` in the worktree and journals nothing, deliberately: an effect is
/// an *external* mutation, whose defining problem is that a lost answer leaves a
/// change out there that a fresh process has to recognise. A commit in a worktree
/// this process created, which is thrown away unless it is pushed, has none of
/// that — journaling it would put a record of something unrecoverable beside the
/// records a recovery is meant to act on.
///
/// # Both arms propose all three, and which of them mutate is the world's answer
///
/// There is no `if reused { skip }` here and no `if reused { rewrite }` either,
/// and both absences are the mechanism rather than an oversight. Each effect is
/// proposed unconditionally and the executor's step 3 decides whether there is
/// anything to do:
///
/// - [`EnsurePullRequest`]'s postcondition read finds the open, labelled pull
///   request for this head and base, so on a reuse no create is dispatched. One
///   code path, and *never a second pull request* is the executor's idempotence
///   rather than a branch somebody has to keep correct.
/// - [`EnsurePullRequestBody`]'s postcondition read asks the opposite question —
///   *does it already say this* — so on a fresh cut, where the create carried
///   this exact body, no rewrite is dispatched, and on a reuse of a pull request
///   still describing last night's run, one is.
///
/// # Why the third effect is not redundant with the second
///
/// Because [`EnsurePullRequest`]'s postcondition **deliberately excludes the
/// body**: it matches on head, base and `state=open`, so a reuse settles on the
/// pull request whatever it says. Design §7 is about what follows from that. An
/// [`EffectId`](fiddle_core::EffectId) is derived from the target and never from
/// the payload, so an effect keyed on the pull request alone would give
/// last night's sentence and tonight's one identity — and a rewrite proposed
/// under it would find a postcondition it believed satisfied and change nothing,
/// silently. [`EnsurePullRequestBody`]'s target carries a digest of the body,
/// which is what makes "say one thing" and "say another" two effects; this is
/// the caller that spends it.
///
/// The body is computed **once**, above, and handed to both — for
/// [`shared_body`]'s reason. Two spellings of one sentence would be two
/// identities, and a run would rewrite a pull request for having described it
/// twice.
///
/// # `capability` is a parameter, and step 1 is what makes that safe
///
/// The proposing capability is not something a proposal should get to choose, and
/// here it is an argument — because the capability that will call this is
/// registered by a later task and this function must not name an id that does not
/// exist yet. It is safe because the executor is *bound* to one capability and its
/// step 1 refuses any proposal made under another: a caller passing somebody
/// else's id gets a refusal on the first effect, before anything is dispatched.
pub async fn publish_shared_work(
    executor: &Executor<'_>,
    capability: CapabilityId,
    approved: &Approved,
    config: &SharedPublication,
) -> Result<SharedWork, CapabilityError> {
    // 1. The branch. Nothing after it can be proposed without it: a pull request
    //    needs a head that exists on the remote.
    let publish_branch = EnsureBranchPublished::new(
        config.repo.clone(),
        approved.branch().to_string(),
        config.head_sha.clone(),
    );
    let published = executor
        .execute(
            ProposedEffect {
                capability,
                kind: EffectKind::EnsureBranchPublished,
                target: publish_branch.target(),
                payload: publish_branch.payload(),
            },
            publish_branch,
        )
        .await?;

    // The sentence this run wants the shared pull request to carry, spelled once
    // and read twice — by the create below and by the rewrite after it. See this
    // function's header, and [`shared_body`].
    let body = shared_body(&config.summary, approved);

    // 2. The pull request, carrying the label that is the only thing which will
    //    find it again. Applied as part of the create rather than afterwards: a
    //    pull request without it is invisible to the next run's discovery read,
    //    which then opens a second — see [`CVE_LABEL`].
    let open = EnsurePullRequest::new(
        config.repo.clone(),
        config.head_owner.clone(),
        approved.branch().to_string(),
        approved.base().to_string(),
        config.title.clone(),
        body.clone(),
        false,
    )
    .labelled(vec![CVE_LABEL.to_string()]);
    let opened = executor
        .execute(
            ProposedEffect {
                capability,
                kind: EffectKind::EnsurePullRequest,
                target: open.target(),
                payload: open.payload(),
            },
            open,
        )
        .await?;

    // 3. What it says. Addressed at the number step 2 settled on — reused or
    //    freshly created — because that is the object this run is describing, and
    //    a rewrite addressed at anything else would be describing somebody's
    //    other pull request.
    let describe = EnsurePullRequestBody::new(config.repo.clone(), opened.value.number, body);
    executor
        .execute(
            ProposedEffect {
                capability,
                kind: EffectKind::EnsurePullRequestBody,
                target: describe.target(),
                payload: describe.payload(),
            },
            describe,
        )
        .await?;

    Ok(SharedWork {
        branch: published.value.branch,
        // The sha the *remote* was observed to hold, which is what the receipt
        // carries and the reason it carries it.
        head_sha: published.value.sha,
        pull_request: opened.value.number,
    })
}

/// The body a publication proposes: what the run did, and the anomaly if there
/// was one.
///
/// Two paragraphs and one rule — the note goes **last**, so a body that grows a
/// per-advisory table above it does not push the one sentence a person has to act
/// on out of sight. It is absent entirely on an ordinary run, which is
/// [`Approved::note`]'s own rule: a warning printed every time is a warning nobody
/// reads.
///
/// Separated from [`publish_shared_work`] because it is pure, and because
/// [`EnsurePullRequestBody`](crate::github::EnsurePullRequestBody) rewrites this
/// same body on a later run — a digest of it is that effect's identity, so two
/// spellings of one body would be two effects and a pull request would be
/// rewritten for having been described twice. `publish_shared_work` calls this
/// once and hands the one string to the create and to the rewrite, which is what
/// keeps the two spellings from existing.
pub fn shared_body(summary: &str, approved: &Approved) -> String {
    match approved.note() {
        Some(note) => format!("{summary}\n\n{note}"),
        None => summary.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::{AdvisoryId, PackageType, Severity};

    /// **The branch's date is a real calendar date**, over the three cases the
    /// arithmetic can get wrong.
    ///
    /// A dated branch is what stops a run pushing onto a name that has already
    /// been merged and deleted, so the date has to advance and has to be a date
    /// — and the era arithmetic below is the only thing in this crate that has to
    /// be right about a leap year. The days are counted from the epoch, which is
    /// exactly what [`today_utc`] hands it.
    #[test]
    fn the_calendar_arithmetic_agrees_with_the_calendar() {
        // The epoch itself, a leap day, and the day after a century that is not
        // a leap year — the three the naive `year % 4` rule gets wrong.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(59), (1970, 3, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(20_683), (2026, 8, 18));
    }

    /// And the rendering the branch name carries is fixed-width, so
    /// `security/cve-remediation-2026-01-02` sorts and reads the way a person
    /// expects rather than as `2026-1-2`.
    #[test]
    fn today_renders_zero_padded_and_the_branch_is_under_the_pushable_prefix() {
        let today = today_utc();
        assert_eq!(today.len(), 10, "{today}");
        assert!(
            today.chars().enumerate().all(|(at, character)| match at {
                4 | 7 => character == '-',
                _ => character.is_ascii_digit(),
            }),
            "{today}"
        );
        assert!(dated_branch(&today).starts_with(PUSHABLE_PREFIX));
    }

    /// A finding whose every field is a value nothing else in this file spells,
    /// so that "the rendering carries the projection" cannot be satisfied by a
    /// constant that happens to look right.
    fn finding() -> ProjectedFinding {
        ProjectedFinding {
            cve: serde_json::from_value::<AdvisoryId>(serde_json::json!("CVE-2026-4242"))
                .expect("a canonical advisory id"),
            package: "golang.org/x/text".to_string(),
            current: "v0.3.7".to_string(),
            fixed_version: Some("v0.3.8".to_string()),
            severity: Severity::High,
            package_type: PackageType::Library,
        }
    }

    /// **Every one of the six fields arrives.**
    ///
    /// The denominator for the exclusions the protocol suite asserts: a prompt
    /// that carried none of the projection would satisfy every *negative*
    /// assertion there, and this is what stops the composition being allowed to
    /// be empty.
    #[test]
    fn the_rendering_carries_all_six_fields_of_a_finding() {
        let task = migration_task(&[&finding()]);
        for expected in [
            "CVE-2026-4242",
            "golang.org/x/text",
            "v0.3.7",
            "v0.3.8",
            "High",
            "Library",
        ] {
            assert!(
                task.contains(expected),
                "the projection's `{expected}` did not reach the prompt: {task}"
            );
        }
    }

    /// A finding with no published fix still renders, and says so in words rather
    /// than by leaving a hole where a version goes.
    ///
    /// Reachable: an advisory fixable through one package and not through another
    /// is one `cve::project` deliberately keeps, and a group can hold both.
    #[test]
    fn a_finding_with_no_published_fix_is_named_as_such() {
        let mut unfixed = finding();
        unfixed.fixed_version = None;
        let mut blank = finding();
        blank.fixed_version = Some("  ".to_string());

        for finding in [unfixed, blank] {
            let task = migration_task(&[&finding]);
            assert!(
                task.contains("no published fix"),
                "an unfixed finding must not render an empty version: {task}"
            );
        }
    }

    /// The fetch refspec and the ref the checkout resolves name **the same
    /// branch on the same remote**.
    ///
    /// Two strings built in two places out of one branch name, and a run whose
    /// halves disagreed would fetch one ref and then resolve another — which
    /// succeeds, silently, whenever the clone happens to already hold the second
    /// one from an earlier run. That is the stale-ref failure this whole guard is
    /// about, arrived at from the inside.
    #[test]
    fn what_is_fetched_and_what_is_resolved_are_one_ref() {
        let fresh = plan(None, "main", "20260817").expect("nothing open is not a refusal");

        // The destination half of the refspec `fetch` writes.
        assert_eq!(
            format!("refs/remotes/{}", origin_ref(fresh.base())),
            format!("refs/remotes/{REMOTE}/main")
        );
        // And what the fresh arm goes on to resolve, which is `Approved::from`
        // itself rather than a second derivation beside it.
        assert_eq!(fresh.from(), origin_ref(fresh.base()));
        assert_eq!(fresh.from(), "origin/main");
    }

    /// The scope rules are in the prompt and the mechanical ones are not, asserted
    /// over the composition alone.
    ///
    /// The protocol suite asserts the same thing over the bytes that actually went
    /// to the model, which is the stronger claim and the one the criterion is
    /// about. This is here because it is the *unit* of that claim: it fails on a
    /// composition defect without a worktree, a tool loop or a mock.
    #[test]
    fn the_composition_carries_the_scope_rules_and_no_mechanical_rule() {
        let task = migration_task(&[&finding()]);
        assert!(task.contains("uniform"), "{task}");
        for mechanical in ["go list -m", "go mod why", "at_least", "dedup", "fold"] {
            assert!(
                !task.contains(mechanical),
                "`{mechanical}` is decided in Rust and must not be in the prompt: {task}"
            );
        }
    }
}
