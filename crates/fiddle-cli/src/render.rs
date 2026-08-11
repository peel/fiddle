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

/// How many attempts at one capability a run actually makes.
///
/// A literal rather than a value read from the document, because that is
/// precisely the point: `agent.max_capability_attempts` is parsed and not
/// consumed, so the number that applies is this one whatever the document says.
/// See `decisions/013-one-attempt-bound-not-two.md`, and
/// [`crate::config::Agent::max_capability_attempts`].
const ENFORCED_CAPABILITY_ATTEMPTS: usize = 1;

/// The decision a reader following the unenforced bound is sent to.
const ATTEMPT_BOUND_DECISION: &str = "013-one-attempt-bound-not-two";

/// What a key that parses and fires nothing is reported as.
const ACCEPTED_NOT_ENFORCED: &str = "accepted-not-enforced";

/// What a key that is *read*, *acted on*, and still decides nothing is reported
/// as.
///
/// A second word rather than a reuse of [`ACCEPTED_NOT_ENFORCED`], because the
/// two are different and an operator debugging one would be misled by the
/// other's promise. `agent.max_capability_attempts` is read by nothing at all:
/// no code path anywhere consults the value. `github.required_checks` is
/// consulted — it is handed to `Executor::observe_checks`, it decides which
/// checks are looked up on the published head, and the answer reaches the
/// bundle as `observations.verification`. What it does *not* do is change any
/// outcome: `fiddle_core::assess` matches on the work item and the change set
/// and never on the verification, so a required check that is missing, failed or
/// pending leaves the run's conclusion exactly where an all-green one does.
///
/// Reported, in other words, and never required. See
/// `decisions/017-required-checks-are-observed-not-enforced.md`.
const OBSERVED_NOT_ENFORCED: &str = "observed-not-enforced";

/// The decision a reader following the reported-but-unenforced check list is
/// sent to.
const REQUIRED_CHECKS_DECISION: &str = "017-required-checks-are-observed-not-enforced";

/// What an entry in `[github.decision] authorized` is matched on.
///
/// Reported rather than left to be inferred, because the *kind* of identity is
/// the property: an immutable numeric user id cannot be changed or reclaimed,
/// which is exactly what a login can, and a reader who cannot tell which of the
/// two `505401` is cannot tell whether the allowlist means what they think. A
/// constant rather than a literal in the payload, so this word and the type in
/// `config::Decision::authorized` cannot come to disagree without a reader of
/// either being sent here.
const AUTHORIZED_MATCHED_ON: &str = "numeric_user_id";

/// What the decision channel is reported as, and **it is not `"enforced"`**.
///
/// The keys parse, they are strict, and they are read by nothing. What an
/// approver list and a page bound could feed are `ProposeConfig::deciders` and
/// the two private `CONVERSATION_PAGES` constants — one in
/// `fiddle_runtime::capability::propose`, one in `fiddle_runtime::human` — and
/// none of the three is reachable from a document, because
/// `main::build_capability` cannot yet construct `propose_change`; see the arm
/// there. So this is [`ACCEPTED_NOT_ENFORCED`]'s case exactly, and it is reported
/// as that word rather than as a promise the build does not keep: a document
/// naming an approver who cannot be consulted is precisely the state an operator
/// running `config check` needs to be told about.
///
/// The page bound carries one extra caution for whoever threads it through. Those
/// two constants are deliberately the same number, because this capability reads
/// one conversation twice — once to find its question, once to find the reply
/// below it — and two reads that saw different amounts of it could find the first
/// and miss the second. A document value has to replace **both** or neither.
///
/// **This is the one line to change when that arm lands**, together with the
/// human rendering's clause and the two tests that pin the word.
const DECISION_STATUS: &str = ACCEPTED_NOT_ENFORCED;

/// The machine-readable `config check` payload.
///
/// The shape is part of the CLI contract: `status` is `"valid"` and every
/// configuration section the document carries is echoed back, so a caller can
/// confirm *which* document was accepted without re-reading it. That is the
/// command's whole stated purpose, and it is why `[agent]` and `[workspace]`
/// are here: a payload naming only the three M0 tables could not show an
/// operator the model, the endpoint, the fixture under repair, the check that
/// decides whether a repair earned anything, or any of the six bounds.
///
/// # Three rules this payload keeps
///
/// **Never the resolved credential — the variable's name only.** `api_key` is
/// echoed as `{ "env": "NAME" }`, the shape the document itself writes, and
/// there is nothing else it could be: `config::EnvRef` has no value to hold.
/// The same rule holds for `[github]`'s `token`, which is the schema's second
/// `EnvRef` and is echoed the same way.
///
/// This function is also not where a credential could be resolved — the only
/// reader of `std::env::var` in this binary is `main::resolve_credential`,
/// called from the repairing arm of `build_capability` and from
/// `main::resolve_forge`, neither of which `config check` reaches.
///
/// **A table the document does not carry produces no key at all**, rather than
/// a `null` one. A deployment that names no model has not left `[agent]` blank;
/// it has described a deployment that does not have one, and the two are
/// different claims. It is also what keeps the schema version honest: an
/// M0-shaped document — three tables, which is what
/// `crates/fiddle-acceptance/tests/m0_skeleton.rs` runs against — produces
/// exactly the bytes it produced before either table existed, so
/// [`CONFIG_CHECK_SCHEMA`] stays `v0`. Nothing a v0 reader ever saw has moved
/// or changed meaning; the change is purely additive.
///
/// A key *inside* a table the document does carry is the opposite case, and is
/// reported as `null`: `workspace.fixture` and `workspace.check` are the two a
/// repair refuses by name when they are absent, so an operator confirming a
/// document should learn which refusal is waiting for them rather than having
/// to notice a missing key.
///
/// **A bound that does not fire does not look like one that does.** Every
/// enforced bound is a plain scalar; `max_capability_attempts` is an object
/// carrying what the document `configured`, what is `enforced`, a `status` a
/// machine can key on, and the `decision` that explains it. See
/// [`ENFORCED_CAPABILITY_ATTEMPTS`].
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
            "base_url": agent.base_url,
            // The name, never the value. See the rules above.
            "api_key": { "env": agent.api_key.env },
            "max_turns": agent.max_turns,
            "max_tokens": agent.max_tokens,
            "max_changed_files": agent.max_changed_files,
            "deadline": agent.deadline.to_string(),
            "tool_timeout": agent.tool_timeout.to_string(),
            "max_capability_attempts": {
                "configured": agent.max_capability_attempts,
                "enforced": ENFORCED_CAPABILITY_ATTEMPTS,
                "status": ACCEPTED_NOT_ENFORCED,
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
            "isolation": isolation(workspace.isolation),
            "command_timeout": workspace.command_timeout.to_string(),
            "cleanup": cleanup(workspace.cleanup),
        });
    }
    if let Some(github) = &config.github {
        body["github"] = serde_json::json!({
            "repo": github.repo.to_string(),
            "base": github.base,
            // The name, never the value. See the rules above: this is the
            // second `EnvRef` in the schema and it is echoed the same way.
            "token": { "env": github.token.env },
            "cli": { "program": github.cli.program, "args": github.cli.args },
            "git": github.git,
            // The two keys a publication refuses by name when they are absent,
            // reported as `null` for the reason `workspace.fixture` is: an
            // operator confirming a document should learn which refusal is
            // waiting for them rather than having to notice a missing key.
            "work": github.work,
            "workflow": github.workflow,
            // **A list that does not gate does not look like one that does.**
            // The same object shape `max_capability_attempts` uses, for the same
            // reason and with a status of its own: `enforced` is the empty list
            // because no run outcome depends on any of these names, whatever the
            // document says. See [`OBSERVED_NOT_ENFORCED`].
            "required_checks": {
                "configured": github.required_checks,
                "enforced": Vec::<String>::new(),
                "status": OBSERVED_NOT_ENFORCED,
                "decision": REQUIRED_CHECKS_DECISION,
            },
            "config_dir": github.config_dir,
            "timeout": github.timeout.to_string(),
            // Echoed beside `timeout` because it is the same kind of thing: a
            // wall-clock bound an operator set or inherited, and one they
            // cannot otherwise confirm without reading this binary's defaults.
            "read_retry": {
                "attempts": github.read_retry.attempts,
                "initial": github.read_retry.initial.to_string(),
                "max": github.read_retry.max.to_string(),
            },
            // One row per effect kind this build can be given a rule for, so a
            // rule an operator wrote is a rule they can confirm. A kind missing
            // here is a document whose gate cannot be read back — which is what
            // the two M3 kinds were until this task.
            "policy": {
                "ensure_branch_published": rule(github.policy.ensure_branch_published),
                "ensure_pull_request": rule(github.policy.ensure_pull_request),
                "ensure_check_requested": rule(github.policy.ensure_check_requested),
                "publish_decision_request": rule(github.policy.publish_decision_request),
                "ensure_pull_request_ready": rule(github.policy.ensure_pull_request_ready),
            },
            // **Who may promote a change, and how the deployment is matched to
            // them.** `matched_on` is a fact about the *kind* of identity rather
            // than a detail: a reader who cannot tell whether an entry is an
            // immutable id or something a login could become cannot tell whether
            // the allowlist means what they think it means.
            //
            // `null` when the table is absent, for the reason `work` and
            // `workflow` are: an operator confirming a document should learn that
            // nobody is authorized here rather than having to notice a missing key.
            "decision": github.decision.as_ref().map(|decision| serde_json::json!({
                "authorized": decision.authorized,
                "matched_on": AUTHORIZED_MATCHED_ON,
                "max_pages": decision.max_pages,
                "status": DECISION_STATUS,
            })),
        });
    }
    payload(CONFIG_CHECK_SCHEMA, body)
}

/// The spelling a document writes a deployment rule as.
///
/// Matched rather than derived through a `Serialize`, for the reason
/// [`isolation`] is: [`fiddle_core::DeploymentRule`] deliberately has no
/// `Serialize`, so a fourth rule has to be answered *here*, at the place that
/// would otherwise report a new rule under an old name.
fn rule(rule: fiddle_core::DeploymentRule) -> &'static str {
    match rule {
        fiddle_core::DeploymentRule::Allow => "allow",
        fiddle_core::DeploymentRule::RequireHuman => "require_human",
        fiddle_core::DeploymentRule::Deny => "deny",
    }
}

/// The spelling a document writes an isolation mechanism as.
///
/// Matched rather than derived through `Serialize`, for the same reason
/// `main::build_capability` matches it: adding a variant then has to be
/// answered *here*, at the place that would otherwise report a new mechanism
/// under an old name, instead of silently acquiring a serialization.
fn isolation(isolation: crate::config::Isolation) -> &'static str {
    match isolation {
        crate::config::Isolation::GitWorktree => "git-worktree",
    }
}

/// The spelling a document writes a cleanup policy as. See [`isolation`].
fn cleanup(cleanup: crate::config::Cleanup) -> &'static str {
    match cleanup {
        crate::config::Cleanup::Always => "always",
    }
}

/// The human-readable `config check` summary.
///
/// The same facts the payload carries, in the order the document writes them,
/// so a reader at a terminal and a reader parsing `--json` confirm the same
/// document. Each line is `<table>.<key> = <value>`, spelled exactly as the key
/// is written in the file, because the reader's next move is to go and edit it.
///
/// The unenforced bound is the one line that says more than its value, and it
/// says it in prose rather than in the payload's four keys: a person needs the
/// consequence, which is that one attempt is made whatever the number is.
pub fn config_check_human(config: &Config) -> String {
    let mut out = format!(
        "configuration valid\n  project.name = {}\n  stub.root    = {}\n  report.dir   = {}",
        config.project.name,
        config.stub.root.display(),
        config.report.dir.display(),
    );
    if let Some(agent) = &config.agent {
        // The name of the variable, never its value — there is none here to
        // print, and `config check` never resolves one.
        out.push_str(&format!(
            "\n  agent.model = {}\
             \n  agent.base_url = {}\
             \n  agent.api_key.env = {}\
             \n  agent.max_turns = {}\
             \n  agent.max_tokens = {}\
             \n  agent.max_changed_files = {}\
             \n  agent.deadline = {}\
             \n  agent.tool_timeout = {}\
             \n  agent.max_capability_attempts = {} \
             (accepted, not enforced: {} attempt is made — see decision {})",
            agent.model,
            agent.base_url,
            agent.api_key.env,
            agent.max_turns,
            agent.max_tokens,
            agent.max_changed_files,
            agent.deadline,
            agent.tool_timeout,
            agent.max_capability_attempts,
            ENFORCED_CAPABILITY_ATTEMPTS,
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
             \n  github.policy.ensure_branch_published = {}\
             \n  github.policy.ensure_pull_request = {}\
             \n  github.policy.ensure_check_requested = {}\
             \n  github.policy.publish_decision_request = {}\
             \n  github.policy.ensure_pull_request_ready = {}\
             \n  github.decision.authorized = {}\
             \n  github.decision.max_pages = {}",
            github.repo,
            github.base,
            // The name of the variable, never its value — there is none here to
            // print, and `config check` never resolves one.
            github.token.env,
            program_line(&github.cli),
            github.git.display(),
            optional(github.work.as_ref().map(|p| p.display().to_string())),
            optional(github.workflow.clone()),
            // "none" rather than a blank, for the reason `optional` gives — and
            // the word is accurate rather than a stand-in: a deployment that
            // lists no required check requires nothing of CI, which is a
            // deployment decision and not a missing key. Each name quoted
            // separately, as `program_line` argues.
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
            rule(github.policy.ensure_branch_published),
            rule(github.policy.ensure_pull_request),
            rule(github.policy.ensure_check_requested),
            rule(github.policy.publish_decision_request),
            rule(github.policy.ensure_pull_request_ready),
            // The ids as the document wrote them, followed by what they are
            // matched on and by the fact that nothing reads them yet — a person
            // needs the consequence, which is that naming an approver does not
            // yet make one consultable. See [`DECISION_STATUS`].
            //
            // Through `optional`, so an absent table reads the way an absent
            // `work` or `fixture` does: this is the third key a capability refuses
            // by name when it is missing, and "not configured" is the word the
            // other two already use for it.
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
            optional(
                github
                    .decision
                    .as_ref()
                    .map(|decision| decision.max_pages.to_string()),
            ),
        ));
    }
    out
}

/// A configured program and its arguments, each token quoted separately.
///
/// Rather than joined into one space-separated line: a [`crate::config::ProgramRef`]
/// is a program and its arguments *already separated* precisely because a shell
/// string has to be split by somebody and every splitter is wrong about quoting
/// somewhere. Rendering one would put that ambiguity back at the surface an
/// operator reads, and an argument containing a space would be
/// indistinguishable from two arguments.
fn program_line(program: &crate::config::ProgramRef) -> String {
    std::iter::once(&program.program)
        .chain(program.args.iter())
        .map(|token| format!("{token:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A key a present table left out, said out loud.
///
/// "not configured" rather than a blank, because a blank after `=` reads as an
/// empty value — and for both keys that use this, absent is a fact a repair
/// will refuse on by name.
fn optional(value: Option<String>) -> String {
    value.unwrap_or_else(|| "not configured".to_string())
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
