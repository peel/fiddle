use crate::config::{Config, WrittenOrNamed};
use fiddle_core::{
    CapabilityAssessment, CapabilityExecution, InvocationRef, NextAction, Observation,
    ProgressEntry, ReportBundle, WorkStateView, CONFIG_CHECK_SCHEMA, INSPECT_SCHEMA, RUN_SCHEMA,
};
use fiddle_runtime::EvidenceError;
use std::path::Path;

#[derive(serde::Serialize)]
struct Payload {
    schema: &'static str,
    #[serde(flatten)]
    body: serde_json::Value,
}

fn payload(schema: &'static str, body: serde_json::Value) -> String {
    serde_json::to_string(&Payload { schema, body })
        .expect("a payload of an object body is always serializable")
}

const ATTEMPT_BOUND_DECISION: &str = "037-the-attempt-bound-is-per-pull-request";

const ATTEMPT_BOUND_STATUS: &str = "enforced-per-pull-request";

const ATTEMPT_BOUND_COUNTED_IN: &str = "pull-request-body";

const ACCEPTED_NOT_ENFORCED: &str = "accepted-not-enforced";

const OBSERVED_NOT_ENFORCED: &str = "observed-not-enforced";

const REQUIRED_CHECKS_DECISION: &str = "017-required-checks-are-observed-not-enforced";

const AUTHORIZED_MATCHED_ON: &str = "numeric_user_id";

const DECISION_STATUS: &str = ACCEPTED_NOT_ENFORCED;

pub fn config_check_json(config: &Config) -> String {
    let mut body = serde_json::json!({
        "status": "valid",
        "project": { "name": config.project.name },
        "stub": { "root": config.stub.root },
        "report": { "dir": config.report.dir },
    });
    if let Some(agent) = &config.agent {
        body["agent"] = serde_json::json!({
            "model": agent.model,
            "base_url": written_or_named_json(&agent.base_url),
            "api_key": { "env": agent.api_key.env },
            "max_turns": agent.max_turns,
            "max_tokens": agent.max_tokens,
            "max_changed_files": agent.max_changed_files,
            "deadline": agent.deadline.to_string(),
            "tool_timeout": agent.tool_timeout.to_string(),
            "max_capability_attempts": {
                "configured": agent.max_capability_attempts,
                "status": ATTEMPT_BOUND_STATUS,
                "counted_in": ATTEMPT_BOUND_COUNTED_IN,
                "decision": ATTEMPT_BOUND_DECISION,
            },
        });
    }
    if let Some(workspace) = &config.workspace {
        body["workspace"] = serde_json::json!({
            "root": workspace.root,
            "fixture": workspace.fixture,
            "check": workspace.check.as_ref().map(|check| serde_json::json!({
                "program": check.program,
                "args": check.args,
            })),
            "checks": workspace.checks.iter().map(|check| serde_json::json!({
                "program": check.program,
                "args": check.args,
                "success": success(check.success),
            })).collect::<Vec<_>>(),
            "isolation": isolation(workspace.isolation),
            "command_timeout": workspace.command_timeout.to_string(),
            "cleanup": cleanup(workspace.cleanup),
        });
    }
    if let Some(github) = &config.github {
        body["github"] = serde_json::json!({
            "repo": github.repo.to_string(),
            "base": github.base,
            "token": { "env": github.token.env },
            "cli": { "program": github.cli.program, "args": github.cli.args },
            "git": github.git,
            "work": github.work,
            "workflow": github.workflow,
            "required_checks": {
                "configured": github.required_checks,
                "enforced": Vec::<String>::new(),
                "status": OBSERVED_NOT_ENFORCED,
                "decision": REQUIRED_CHECKS_DECISION,
            },
            "config_dir": github.config_dir,
            "timeout": github.timeout.to_string(),
            "read_retry": {
                "attempts": github.read_retry.attempts,
                "initial": github.read_retry.initial.to_string(),
                "max": github.read_retry.max.to_string(),
            },
            "policy": {
                "ensure_branch_published": rule(github.policy.ensure_branch_published),
                "ensure_pull_request": rule(github.policy.ensure_pull_request),
                "ensure_check_requested": rule(github.policy.ensure_check_requested),
                "publish_decision_request": rule(github.policy.publish_decision_request),
                "ensure_pull_request_ready": rule(github.policy.ensure_pull_request_ready),
                "ensure_pull_request_body": rule(github.policy.ensure_pull_request_body),
            },
            "decision": github.decision.as_ref().map(|decision| serde_json::json!({
                "authorized": decision.authorized,
                "matched_on": AUTHORIZED_MATCHED_ON,
                "status": DECISION_STATUS,
            })),
        });
    }
    if let Some(scanner) = &config.scanner {
        body["scanner"] = serde_json::json!({
            "cli": { "program": scanner.cli.program, "args": scanner.cli.args },
            "timeout": scanner.timeout.to_string(),
        });
    }
    if let Some(cve) = config
        .orchestration
        .as_ref()
        .and_then(|orchestration| orchestration.cve.as_ref())
    {
        body["orchestration"] = serde_json::json!({
            "cve": {
                "image": cve.image,
                "severities": cve.severities.grades().collect::<Vec<_>>(),
                "max_findings": cve.max_findings,
            },
        });
    }
    payload(CONFIG_CHECK_SCHEMA, body)
}

fn rule(rule: fiddle_core::DeploymentRule) -> &'static str {
    match rule {
        fiddle_core::DeploymentRule::Allow => "allow",
        fiddle_core::DeploymentRule::RequireHuman => "require_human",
        fiddle_core::DeploymentRule::Deny => "deny",
    }
}

fn isolation(isolation: crate::config::Isolation) -> &'static str {
    match isolation {
        crate::config::Isolation::GitWorktree => "git-worktree",
    }
}

fn success(success: crate::config::Success) -> &'static str {
    match success {
        crate::config::Success::ExitZero => "exit-zero",
        crate::config::Success::ExitZeroAndNoOutput => "exit-zero-and-no-output",
        crate::config::Success::ArtefactWritten => "artefact-written",
    }
}

fn written_or_named_json(value: &WrittenOrNamed) -> serde_json::Value {
    match value {
        WrittenOrNamed::Written(written) => serde_json::json!(written),
        WrittenOrNamed::Named(variable) => serde_json::json!({ "env": variable }),
    }
}

fn written_or_named_line(key: &str, value: &WrittenOrNamed) -> String {
    match value {
        WrittenOrNamed::Written(written) => format!("{key} = {written}"),
        WrittenOrNamed::Named(variable) => format!("{key}.env = {variable}"),
    }
}

fn cleanup(cleanup: crate::config::Cleanup) -> &'static str {
    match cleanup {
        crate::config::Cleanup::Always => "always",
    }
}

pub fn config_check_human(config: &Config) -> String {
    let mut out = format!(
        "configuration valid\n  project.name = {}\n  stub.root    = {}\n  report.dir   = {}",
        config.project.name,
        config.stub.root.display(),
        config.report.dir.display(),
    );
    if let Some(agent) = &config.agent {
        out.push_str(&format!(
            "\n  agent.model = {}\
             \n  {}\
             \n  agent.api_key.env = {}\
             \n  agent.max_turns = {}\
             \n  agent.max_tokens = {}\
             \n  agent.max_changed_files = {}\
             \n  agent.deadline = {}\
             \n  agent.tool_timeout = {}\
             \n  agent.max_capability_attempts = {} \
             (enforced per pull request; the count lives in that pull \
             request's body — see decision {})",
            agent.model,
            written_or_named_line("agent.base_url", &agent.base_url),
            agent.api_key.env,
            agent.max_turns,
            agent.max_tokens,
            agent.max_changed_files,
            agent.deadline,
            agent.tool_timeout,
            agent.max_capability_attempts,
            ATTEMPT_BOUND_DECISION,
        ));
    }
    if let Some(workspace) = &config.workspace {
        out.push_str(&format!(
            "\n  workspace.root = {}\
             \n  workspace.fixture = {}\
             \n  workspace.check = {}\
             \n  workspace.isolation = {}\
             \n  workspace.command_timeout = {}\
             \n  workspace.cleanup = {}",
            workspace.root.display(),
            optional(workspace.fixture.as_ref().map(|p| p.display().to_string())),
            optional(
                workspace
                    .check
                    .as_ref()
                    .map(|check| { program_line(check) })
            ),
            isolation(workspace.isolation),
            workspace.command_timeout,
            cleanup(workspace.cleanup),
        ));
        for (index, check) in workspace.checks.iter().enumerate() {
            out.push_str(&format!(
                "\n  workspace.checks[{index}] = {} (success: {})",
                check_line(check),
                success(check.success),
            ));
        }
    }
    if let Some(github) = &config.github {
        out.push_str(&format!(
            "\n  github.repo = {}\
             \n  github.base = {}\
             \n  github.token.env = {}\
             \n  github.cli = {}\
             \n  github.git = {}\
             \n  github.work = {}\
             \n  github.workflow = {}\
             \n  github.required_checks = {} \
             (observed, not enforced: no outcome depends on them — see decision {})\
             \n  github.config_dir = {}\
             \n  github.timeout = {}\
             \n  github.read_retry = {} attempts (initial {}, max {})\
             \n  github.policy.ensure_branch_published = {}\
             \n  github.policy.ensure_pull_request = {}\
             \n  github.policy.ensure_check_requested = {}\
             \n  github.policy.publish_decision_request = {}\
             \n  github.policy.ensure_pull_request_ready = {}\
             \n  github.policy.ensure_pull_request_body = {}\
             \n  github.decision.authorized = {}",
            github.repo,
            github.base,
            github.token.env,
            program_line(&github.cli),
            github.git.display(),
            optional(github.work.as_ref().map(|p| p.display().to_string())),
            optional(github.workflow.clone()),
            match github.required_checks.is_empty() {
                true => "none".to_string(),
                false => github
                    .required_checks
                    .iter()
                    .map(|name| format!("{name:?}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            },
            REQUIRED_CHECKS_DECISION,
            github.config_dir.display(),
            github.timeout,
            github.read_retry.attempts,
            github.read_retry.initial,
            github.read_retry.max,
            rule(github.policy.ensure_branch_published),
            rule(github.policy.ensure_pull_request),
            rule(github.policy.ensure_check_requested),
            rule(github.policy.publish_decision_request),
            rule(github.policy.ensure_pull_request_ready),
            rule(github.policy.ensure_pull_request_body),
            optional(github.decision.as_ref().map(|decision| {
                format!(
                    "{} (matched on {}; accepted, not enforced: no capability in \
                     this build reads it)",
                    decision
                        .authorized
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<_>>()
                        .join(" "),
                    AUTHORIZED_MATCHED_ON,
                )
            })),
        ));
    }
    if let Some(scanner) = &config.scanner {
        out.push_str(&format!(
            "\n  scanner.cli = {}\
             \n  scanner.timeout = {}",
            program_line(&scanner.cli),
            scanner.timeout,
        ));
    }
    if let Some(cve) = config
        .orchestration
        .as_ref()
        .and_then(|orchestration| orchestration.cve.as_ref())
    {
        out.push_str(&format!(
            "\n  orchestration.cve.image = {}\
             \n  orchestration.cve.severities = {}\
             \n  orchestration.cve.max_findings = {}",
            cve.image,
            cve.severities
                .grades()
                .map(grade)
                .collect::<Vec<_>>()
                .join(" "),
            cve.max_findings,
        ));
    }
    out
}

fn grade(severity: fiddle_core::Severity) -> String {
    serde_json::to_value(severity)
        .ok()
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .expect("a grade serializes as the string its `rename_all` spells")
}

fn program_line(program: &crate::config::ProgramRef) -> String {
    std::iter::once(&program.program)
        .chain(program.args.iter())
        .map(|token| format!("{token:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn check_line(check: &crate::config::CheckRef) -> String {
    std::iter::once(&check.program)
        .chain(check.args.iter())
        .map(|token| format!("{token:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn optional(value: Option<String>) -> String {
    value.unwrap_or_else(|| "not configured".to_string())
}

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
    if let Some(disposition) = &bundle.disposition {
        body["disposition"] = serde_json::to_value(disposition)
            .expect("a disposition holds no value serde can refuse");
    }
    payload(RUN_SCHEMA, body)
}

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
    if let Some(disposition) = &bundle.disposition {
        out.push_str(&format!(
            "\n  disposition = {}",
            disposition_line(disposition)
        ));
    }
    out
}

fn projected_line(projected: Option<usize>) -> String {
    match projected {
        Some(projected) => format!("{projected} projected"),
        None => "no projection".to_string(),
    }
}

fn disposition_line(disposition: &fiddle_core::RunDisposition) -> String {
    let mut line = format!(
        "{} ({} unfixed of {}, {} already fixed, {} deferred, {} attempted)",
        disposition.reason,
        disposition.verdicts,
        projected_line(disposition.projected),
        disposition.already_fixed.len(),
        disposition.deferred.len(),
        disposition.attempts.len(),
    );
    if let Some(branch) = &disposition.branch {
        line.push_str(&format!(", branch {branch}"));
    }
    if let Some(pull_request) = disposition.pull_request {
        line.push_str(&format!(", pull request #{pull_request}"));
    }
    if let Some(bound) = disposition.attempt_bound {
        line.push_str(&format!(
            ", {} attempts of {} spent",
            bound.spent, bound.bound
        ));
    }
    line
}

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

fn outcome_line(outcome: &fiddle_core::RunOutcome) -> String {
    match outcome {
        fiddle_core::RunOutcome::Completed => "completed".to_string(),
        fiddle_core::RunOutcome::Suspended { reason } => format!("suspended — {reason}"),
        fiddle_core::RunOutcome::Retryable { reason } => format!("retryable — {reason}"),
        fiddle_core::RunOutcome::Failed { error } => format!("failed — {error}"),
    }
}

fn execution_line(execution: &CapabilityExecution) -> String {
    format!(
        "{} {} (evidence {})",
        execution.capability_id,
        execution.status,
        join_evidence(&execution.evidence)
    )
}

fn progress_line(entry: &ProgressEntry) -> String {
    format!(
        "{}/{} {} — {}",
        entry.capability_id, entry.stage, entry.status, entry.summary
    )
}

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

fn next_action_line(next_action: &NextAction) -> String {
    match next_action {
        NextAction::Execute { capability_id } => format!("execute {capability_id}"),
        NextAction::Complete => "complete".to_string(),
        NextAction::Blocked { reason } => format!("blocked — {reason}"),
    }
}

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

pub fn diagnostic(error: &dyn miette::Diagnostic) -> String {
    let mut rendered = String::new();
    let handler = miette::GraphicalReportHandler::new()
        .with_theme(miette::GraphicalTheme::unicode_nocolor())
        .with_width(120);
    match handler.render_report(&mut rendered, error) {
        Ok(()) => rendered,
        Err(_) => format!("{error}"),
    }
}
