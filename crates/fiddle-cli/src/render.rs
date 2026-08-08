//! Human and `--json` renderers.
//!
//! Every byte the CLI writes to stdout or stderr is produced here, so the
//! observable output contract lives in one file rather than being scattered
//! through the command handlers.

use crate::config::Config;
use fiddle_core::InvocationRef;

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
/// never drift from the spelling the parser accepts. Later M0 tasks add
/// observation and assessment members beside these two.
pub fn inspect_json(reference: &InvocationRef) -> String {
    let value = serde_json::json!({
        "invocation_ref": reference.as_str(),
        "scheme": reference.scheme(),
    });
    serde_json::to_string(&value).expect("inspect payload is always serializable")
}

/// The human-readable `inspect` summary.
pub fn inspect_human(reference: &InvocationRef) -> String {
    format!(
        "invocation {}\n  scheme = {}\n  value  = {}",
        reference.as_str(),
        reference.scheme(),
        reference.value(),
    )
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
