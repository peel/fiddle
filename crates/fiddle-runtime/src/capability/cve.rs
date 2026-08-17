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

use super::CapabilityError;
use crate::agent::{attempt_briefed, AgentBudget, Brief, RepairReport, ToolHost, ToolReceipts};
use crate::cve::group::Group;
use crate::workspace::{Workspace, WorkspaceCommand, WorkspacePath};
use fiddle_core::{AttemptId, ProjectedFinding};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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

/// Everything one migration attempt needs that is not the model.
///
/// One struct rather than five arguments, for [`super::RepairConfig`]'s reason:
/// each field is a host decision an operator configures, none is derivable from
/// the others, and grouping them keeps the model — the one value with a
/// credential behind it — visibly separate.
///
/// There is no attempt id here. It arrives at [`GroupMigration::migrate`], which
/// is the call that *is* one attempt, so there is nowhere for a second one to be
/// supplied from — the defect `ExecutionGrant`'s doc records, avoided by
/// construction rather than by remembering.
pub struct MigrationConfig {
    /// The repository being mitigated. Each attempt branches a worktree from it
    /// and never writes to it.
    pub tree: PathBuf,

    /// Where per-attempt worktrees are created.
    pub workspace_root: PathBuf,

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
/// Nothing here is a verdict, and there is deliberately no field that could be
/// mistaken for one. Task 14.b classifies the diff these paths name; the checks
/// decide.
#[derive(Debug)]
pub struct MigrationAttempt {
    /// What the model said it did.
    pub report: RepairReport,

    /// What git saw change in the worktree, under the ignore rules the project
    /// had committed before the attempt began.
    pub changed: Vec<WorkspacePath>,
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
    /// # Why the workspace is held across the whole function
    ///
    /// Its `Drop` guard is what removes the worktree, so it must outlive every
    /// path out — an early return, a `?`, a panic. The `Arc` exists only because
    /// the tools reach the same workspace; nothing outside this scope keeps a
    /// clone.
    pub async fn migrate(
        &self,
        attempt: &AttemptId,
        group: &Group,
    ) -> Result<MigrationAttempt, CapabilityError> {
        let findings: Vec<&ProjectedFinding> = group
            .findings()
            .iter()
            .map(|attributed| attributed.finding())
            .collect();
        let task = migration_task(&findings);

        let workspace = Arc::new(Workspace::create(
            &self.config.tree,
            &self.config.workspace_root,
            attempt,
            self.config.cancel.clone(),
        )?);

        let host = ToolHost {
            workspace: Arc::clone(&workspace),
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

        // Asked of git rather than of the report, and asked before the workspace
        // is dropped, because the worktree is what git is being asked about.
        let changed = workspace.changed_files()?;

        Ok(MigrationAttempt { report, changed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::{AdvisoryId, PackageType, Severity};

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
