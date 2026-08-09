//! Human and `--json` renderers.
//!
//! Everything fiddle says *about a command it ran* — every `--json` payload,
//! every human rendering, and the miette diagnostic for a request it refused —
//! is produced here, so the observable output contract lives in one file rather
//! than being scattered through the command handlers.
//!
//! Two things the process writes are outside that, and naming them is what
//! stops the claim above from being the kind that quietly stops being true:
//!
//! - **clap's own output.** `--version`, `--help`, and the error for a
//!   malformed command line are written and exited on by the parser, before
//!   `dispatch` is reached and therefore before there is any command to say
//!   anything about. `crates/fiddle-acceptance/tests/version.rs` pins the
//!   `--version` shape at the boundary where it is actually produced.
//! - **The interrupt notice.** `main.rs` writes "interrupted; stopping the
//!   attempt" from inside the signal handler. It describes something happening
//!   *to* the process rather than anything a run concluded, it is emitted while
//!   the run is still in flight, and every payload this module produces is
//!   still emitted afterwards unchanged. Routing it through here would have put
//!   a renderer with no bundle to render beside the ones that always have one.

use crate::config::Config;
use fiddle_core::{
    CapabilityAssessment, CapabilityExecution, InvocationRef, NextAction, Observation,
    ProgressEntry, ReportBundle, WorkStateView, CONFIG_CHECK_SCHEMA, INSPECT_SCHEMA, RUN_SCHEMA,
};
use fiddle_runtime::EvidenceError;
use std::path::Path;

/// One `--json` payload, led by the schema it is an instance of.
///
/// All three stdout contracts carry a discriminator, not only the `run` payload
/// design §3.2 names: a consumer parsing `inspect` or `config check` stdout has
/// exactly the same versioning problem, and a surface where only some payloads
/// can be dispatched on is worse than one where none can — the absence of the
/// key stops meaning anything. Each value is a named constant in `fiddle-core`
/// beside `REPORT_SCHEMA`, never a literal spelled here, so a payload whose
/// shape changes must change its version in the same edit.
///
/// This is a struct wrapping the body rather than one more key in the
/// `serde_json` object because that object is a sorted map: `schema` would sort
/// into the middle of the payload, and design §3.2 shows it *leading*. Serde
/// writes struct fields in declaration order, which is the same mechanism that
/// already puts `schema` first in a published [`ReportBundle`].
#[derive(serde::Serialize)]
struct Payload {
    schema: &'static str,
    /// Flattened, so the body's keys sit beside `schema` rather than nested
    /// under a key of their own. Always a `serde_json` object — flattening
    /// anything else is a serialization error, which is why every construction
    /// site below builds one with `json!({ .. })`.
    #[serde(flatten)]
    body: serde_json::Value,
}

/// Render `body` as the payload for `schema`, discriminator first.
fn payload(schema: &'static str, body: serde_json::Value) -> String {
    serde_json::to_string(&Payload { schema, body })
        .expect("a payload of an object body is always serializable")
}

/// The machine-readable `config check` payload.
///
/// The shape is part of the CLI contract: `status` is `"valid"` and the three
/// configuration sections are echoed back so a caller can confirm *which*
/// document was accepted without re-reading it.
pub fn config_check_json(config: &Config) -> String {
    payload(
        CONFIG_CHECK_SCHEMA,
        serde_json::json!({
            "status": "valid",
            "project": { "name": config.project.name },
            "stub": { "root": config.stub.root },
            "report": { "dir": config.report.dir },
        }),
    )
}

/// The human-readable `config check` summary.
pub fn config_check_human(config: &Config) -> String {
    format!(
        "configuration valid\n  project.name = {}\n  stub.root    = {}\n  report.dir   = {}",
        config.project.name,
        config.stub.root.display(),
        config.report.dir.display(),
    )
}

/// The machine-readable `inspect` payload.
///
/// `invocation_ref` is the canonical text of the *parsed* reference rather than
/// the argument as typed, so a caller can confirm the round trip; `scheme` is
/// the typed scheme, serialized by `fiddle-core`, so the spelling here can
/// never drift from the spelling the parser accepts.
///
/// `observations` is `WorkStateView`'s own serialization rather than a shape
/// re-derived here, so the payload a caller reads is the domain value fiddle
/// reasoned about — including the fail-closed distinction that an unreadable
/// source appears under `unavailable` with a reason and contributes no
/// `available` key at all. `assessment` and `next_action` are likewise the
/// core's own serializations: what a caller reads is the verdict fiddle acted
/// on, not a restatement of it that could drift.
pub fn inspect_json(
    reference: &InvocationRef,
    observed: &WorkStateView,
    assessment: &CapabilityAssessment,
    next_action: &NextAction,
) -> String {
    payload(
        INSPECT_SCHEMA,
        serde_json::json!({
            "invocation_ref": reference.as_str(),
            "scheme": reference.scheme(),
            "observations": observed,
            "assessment": assessment,
            "next_action": next_action,
        }),
    )
}

/// The human-readable `inspect` summary.
pub fn inspect_human(
    reference: &InvocationRef,
    observed: &WorkStateView,
    assessment: &CapabilityAssessment,
    next_action: &NextAction,
) -> String {
    format!(
        "invocation {}\n  scheme      = {}\n  value       = {}\n  work item   = {}\n  changes     = {}\n  assessment  = {}\n  next action = {}",
        reference.as_str(),
        reference.scheme(),
        reference.value(),
        observation_line(&observed.work_item, |state| format!(
            "status {}",
            state.status
        )),
        observation_line(&observed.changes, |state| match &state.marker {
            Some(marker) => format!("marked {marker}"),
            None => "not marked".to_string(),
        }),
        assessment_line(assessment),
        next_action_line(next_action),
    )
}

/// The machine-readable `run` payload — design §3.2's canonical output.
///
/// It leads with `schema`, exactly as §3.2 shows, on every path: the
/// discriminator is a property of the payload, not of the outcome, so a caller
/// can parse a failed run's stdout without first knowing it failed.
///
/// Projected from the very [`ReportBundle`] that was published rather than
/// rebuilt from the run, so a caller reading stdout and a caller reading the
/// published bundle cannot be reading two different documents. The payload is
/// the subset of the bundle a caller at a shell needs plus `report`, the path
/// of the bundle itself relative to `<report.dir>` — the pointer that lets them
/// go and read the rest.
///
/// `report` is absent when publication failed. A key naming a bundle that is
/// not there would be worse than no key: it invites a reader to open a path
/// that does not exist and conclude something about the run from its absence.
///
/// `outcome`, `next_action`, `capability_executions`, `progress` and
/// `observations` are the core's own serializations rather than shapes
/// re-derived here — what a caller reads is what fiddle concluded and did, not
/// a restatement of it that could drift. `next_action` in particular is the
/// action derived *after* the run finished, so it describes the state the run
/// left behind.
pub fn run_json(bundle: &ReportBundle, published: Option<&Path>) -> String {
    let mut body = serde_json::json!({
        "invocation_ref": bundle.invocation_ref,
        "mode": bundle.mode,
        "outcome": bundle.outcome,
        "next_action": bundle.next_action,
        "capability_executions": bundle.capability_executions,
        "progress": bundle.progress,
        "observations": bundle.observations,
    });
    if let Some(path) = published {
        body["report"] = serde_json::Value::String(path.display().to_string());
    }
    payload(RUN_SCHEMA, body)
}

/// The human-readable `run` summary.
///
/// A reader at a terminal gets the same facts the payload carries: what was
/// run, under which mode, how it ended, what it did, what is left, and where
/// the published bundle can be found.
pub fn run_human(bundle: &ReportBundle, published: Option<&Path>) -> String {
    let mut out = format!(
        "run {}\n  mode        = {}\n  outcome     = {}\n  next action = {}",
        bundle.invocation_ref,
        bundle.mode,
        outcome_line(&bundle.outcome),
        next_action_line(&bundle.next_action),
    );
    if bundle.capability_executions.is_empty() {
        out.push_str("\n  executions  = none");
    }
    for execution in &bundle.capability_executions {
        out.push_str(&format!("\n  executed    = {}", execution_line(execution)));
    }
    for entry in &bundle.progress {
        out.push_str(&format!("\n  progress    = {}", progress_line(entry)));
    }
    if let Some(path) = published {
        out.push_str(&format!("\n  report      = {}", path.display()));
    }
    out
}

/// The diagnostic for an attempt that could not record itself.
///
/// Plain lines rather than a `miette` report: the whole point of this
/// diagnostic is that an operator can read `<report.dir>` out of it and go fix
/// the permissions, and a graphical handler reflows long messages, which is
/// exactly the wrong thing to do to a path. The directory is named on its own
/// line, whole, whatever its length.
///
/// The headline distinguishes the two moments, because they have different
/// consequences an operator needs to know about: a journal that could not be
/// written means the run *did not act*, while a bundle that could not be
/// published means it acted and the record of it is missing.
pub fn evidence_failure(report_dir: &Path, error: &EvidenceError) -> String {
    let headline = match error {
        EvidenceError::Journal { .. } => "could not record this attempt before executing it",
        EvidenceError::Write { .. } | EvidenceError::Render { .. } => {
            "could not publish the report bundle"
        }
    };
    format!(
        "error: {headline}\n  report.dir  = {}\n  cause       = {error}",
        report_dir.display(),
    )
}

/// One outcome as a single line of prose. A non-completing outcome leads with
/// its reason, so a reader learns why without re-deriving it.
fn outcome_line(outcome: &fiddle_core::RunOutcome) -> String {
    match outcome {
        fiddle_core::RunOutcome::Completed => "completed".to_string(),
        fiddle_core::RunOutcome::Suspended { reason } => format!("suspended — {reason}"),
        fiddle_core::RunOutcome::Retryable { reason } => format!("retryable — {reason}"),
        fiddle_core::RunOutcome::Failed { error } => format!("failed — {error}"),
    }
}

/// One capability execution as a single line of prose.
fn execution_line(execution: &CapabilityExecution) -> String {
    format!(
        "{} {} (evidence {})",
        execution.capability_id,
        execution.status,
        join_evidence(&execution.evidence)
    )
}

/// One progress entry as a single line of prose.
fn progress_line(entry: &ProgressEntry) -> String {
    format!(
        "{}/{} {} — {}",
        entry.capability_id, entry.stage, entry.status, entry.summary
    )
}

/// One assessment as a single line of prose.
///
/// A blocked verdict leads with its reason: the whole point of the variant is
/// that a reader learns *why* fiddle stopped without having to re-derive it.
fn assessment_line(assessment: &CapabilityAssessment) -> String {
    match assessment {
        CapabilityAssessment::NotStarted { evidence } => {
            format!("not started (evidence {})", join_evidence(evidence))
        }
        CapabilityAssessment::Satisfied { evidence } => {
            format!("satisfied (evidence {})", join_evidence(evidence))
        }
        CapabilityAssessment::Blocked { reason, evidence } => {
            format!("blocked — {reason} (evidence {})", join_evidence(evidence))
        }
    }
}

/// One derived action as a single line of prose.
fn next_action_line(next_action: &NextAction) -> String {
    match next_action {
        NextAction::Execute { capability_id } => format!("execute {capability_id}"),
        NextAction::Complete => "complete".to_string(),
        NextAction::Blocked { reason } => format!("blocked — {reason}"),
    }
}

/// The references a verdict rests on, as a comma-separated list. An empty list
/// says so rather than rendering as a bare pair of brackets.
fn join_evidence(evidence: &[fiddle_core::EvidenceRef]) -> String {
    if evidence.is_empty() {
        return "none".to_string();
    }
    evidence
        .iter()
        .map(|reference| reference.0.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// One observation as a single line of prose.
///
/// The three cases are spelled out differently on purpose: a reader of the
/// human output must be able to tell "there is no marker" from "fiddle could
/// not look", which is the same distinction the `--json` payload draws by
/// variant name.
fn observation_line<T>(observed: &Observation<T>, describe: impl Fn(&T) -> String) -> String {
    match observed {
        Observation::Available { value, source, .. } => {
            format!("{} (from {source})", describe(value))
        }
        Observation::Unavailable { source, reason } => {
            format!("unavailable — {reason} (source {source})")
        }
        Observation::NotApplicable { reason } => format!("not applicable — {reason}"),
    }
}

/// Render a diagnostic the way design §4.6 requires: through miette's graphical
/// report handler, so the offending source line and its caret are visible
/// rather than only the error message.
///
/// The theme is chosen explicitly instead of inherited from the terminal so the
/// rendering is identical whether stderr is a TTY, a pipe, or a test harness.
pub fn diagnostic(error: &dyn miette::Diagnostic) -> String {
    let mut rendered = String::new();
    let handler = miette::GraphicalReportHandler::new()
        .with_theme(miette::GraphicalTheme::unicode_nocolor())
        .with_width(120);
    match handler.render_report(&mut rendered, error) {
        Ok(()) => rendered,
        // A formatter failure must not swallow the error itself; fall back to
        // the plain message so the caller still learns what went wrong.
        Err(_) => format!("{error}"),
    }
}
