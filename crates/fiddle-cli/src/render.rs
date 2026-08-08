//! Human and `--json` renderers.
//!
//! Every byte the CLI writes to stdout or stderr is produced here, so the
//! observable output contract lives in one file rather than being scattered
//! through the command handlers.

use crate::config::Config;
use fiddle_core::{CapabilityAssessment, InvocationRef, NextAction, Observation, WorkStateView};

/// The machine-readable `config check` payload.
///
/// The shape is part of the CLI contract: `status` is `"valid"` and the three
/// configuration sections are echoed back so a caller can confirm *which*
/// document was accepted without re-reading it.
pub fn config_check_json(config: &Config) -> String {
    let value = serde_json::json!({
        "status": "valid",
        "project": { "name": config.project.name },
        "stub": { "root": config.stub.root },
        "report": { "dir": config.report.dir },
    });
    serde_json::to_string(&value).expect("config check payload is always serializable")
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
    let value = serde_json::json!({
        "invocation_ref": reference.as_str(),
        "scheme": reference.scheme(),
        "observations": observed,
        "assessment": assessment,
        "next_action": next_action,
    });
    serde_json::to_string(&value).expect("inspect payload is always serializable")
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
