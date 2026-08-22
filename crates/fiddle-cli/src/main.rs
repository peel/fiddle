mod cli;
mod config;
mod render;

use clap::Parser;
use config::ConfigError;
use fiddle_core::{
    CapabilityId, FiddleBuild, InvocationRef, InvocationRefError, InvocationScheme, RunOutcome,
    WorkStateView,
};
use fiddle_runtime::effect::{EffectContext, Executor};
use fiddle_runtime::human::interpret::InterpretationBounds;
use fiddle_runtime::{
    Addressed, AgentBudget, AttemptContext, AttemptTrace, Capability, DeclaredCommand, Extend,
    FixtureRepair, GatewayError, GhCli, GitCli, ProposeChange, ProposeConfig, PublishChange,
    PublishConfig, RepairConfig, StubChangePort, StubMark, StubWorkItemPort, WorkspaceCommand,
    CAPABILITIES,
};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tokio_util::sync::CancellationToken;

const EXIT_INVALID_INPUT: u8 = 2;

const EXIT_INTERRUPTED: i32 = 130;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let termination = match dispatch(&cli).await {
        Ok(outcome) => Termination::Ran(outcome),
        Err(error) => {
            eprintln!("{}", render::diagnostic(&error));
            Termination::Rejected(error)
        }
    };
    ExitCode::from(exit_code_for(&termination))
}

enum Termination {
    Ran(RunOutcome),
    Rejected(CliError),
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
enum CliError {
    #[error(transparent)]
    #[diagnostic(transparent)]
    Config(#[from] ConfigError),

    #[error(transparent)]
    #[diagnostic(transparent)]
    InvocationRef(#[from] InvalidInvocationRef),

    #[error(transparent)]
    #[diagnostic(transparent)]
    UnknownCapability(#[from] UnknownCapability),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Unconfigured(#[from] Unconfigured),

    #[error(transparent)]
    #[diagnostic(transparent)]
    CredentialAbsent(#[from] CredentialAbsent),

    #[error(transparent)]
    #[diagnostic(transparent)]
    NamedValueAbsent(#[from] NamedValueAbsent),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Gateway(#[from] GatewayUnavailable),

    #[error(transparent)]
    #[diagnostic(transparent)]
    PathUnusable(#[from] PathUnusable),

    #[error(transparent)]
    #[diagnostic(transparent)]
    UnimplementedForm(#[from] UnimplementedForm),
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("unknown capability `{requested}`")]
#[diagnostic(
    code(fiddle::capability::unknown),
    help("this build can execute: {known}")
)]
struct UnknownCapability {
    requested: String,
    known: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Selection {
    Mark,
    Repair,
    Publish,
    Propose,
    Mitigate,
}

impl Selection {
    fn id(self) -> CapabilityId {
        match self {
            Selection::Mark => fiddle_core::STUB_MARK,
            Selection::Repair => fiddle_core::FIXTURE_REPAIR,
            Selection::Publish => fiddle_core::PUBLISH_CHANGE,
            Selection::Propose => fiddle_core::PROPOSE_CHANGE,
            Selection::Mitigate => fiddle_core::CVE_MITIGATE,
        }
    }

    fn parse(requested: &str) -> Result<Self, UnknownCapability> {
        if requested == fiddle_core::STUB_MARK.0 {
            Ok(Selection::Mark)
        } else if requested == fiddle_core::FIXTURE_REPAIR.0 {
            Ok(Selection::Repair)
        } else if requested == fiddle_core::PUBLISH_CHANGE.0 {
            Ok(Selection::Publish)
        } else if requested == fiddle_core::PROPOSE_CHANGE.0 {
            Ok(Selection::Propose)
        } else if requested == fiddle_core::CVE_MITIGATE.0 {
            Ok(Selection::Mitigate)
        } else {
            Err(UnknownCapability {
                requested: requested.to_string(),
                known: CAPABILITIES
                    .iter()
                    .map(|capability| capability.0)
                    .collect::<Vec<_>>()
                    .join(", "),
            })
        }
    }

    fn resolve(
        requested: Option<&str>,
        reference: &InvocationRef,
    ) -> Result<Self, UnknownCapability> {
        match requested {
            Some(requested) => Selection::parse(requested),
            None => Ok(Selection::default_for(reference.scheme())),
        }
    }

    fn default_for(scheme: InvocationScheme) -> Self {
        match scheme {
            InvocationScheme::Cve => Selection::Mitigate,
            InvocationScheme::Beans
            | InvocationScheme::Jira
            | InvocationScheme::Scheduled
            | InvocationScheme::Scanner => Selection::Mark,
        }
    }
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("`{capability}` needs {missing}, and {path} does not have it")]
#[diagnostic(
    code(fiddle::config::capability_unconfigured),
    help("add {missing} to {path}, or run a capability that does not need one")
)]
struct Unconfigured {
    capability: CapabilityId,
    missing: &'static str,
    path: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CredentialPurpose {
    Model,
    Forge,
}

impl CredentialPurpose {
    #[cfg(test)]
    const ALL: [CredentialPurpose; 2] = [CredentialPurpose::Model, CredentialPurpose::Forge];

    fn table(self) -> &'static str {
        match self {
            CredentialPurpose::Model => "[agent]",
            CredentialPurpose::Forge => "[github]",
        }
    }
}

impl std::fmt::Display for CredentialPurpose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CredentialPurpose::Model => "model",
            CredentialPurpose::Forge => "forge",
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("the {purpose} credential {variable} is not set")]
struct CredentialAbsent {
    purpose: CredentialPurpose,
    variable: String,
}

impl miette::Diagnostic for CredentialAbsent {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new("fiddle::config::credential_absent"))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(format!(
            "export {variable}, or set it as a repository secret, before running \
             a capability that reaches the {purpose}; {table} is where the \
             document names it",
            variable = self.variable,
            purpose = self.purpose,
            table = self.purpose.table(),
        )))
    }
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{key} names {variable}, and {variable} is not set")]
#[diagnostic(
    code(fiddle::config::named_value_absent),
    help(
        "export {variable} before the run, or write the value into {key} in the \
         configuration document"
    )
)]
struct NamedValueAbsent {
    key: &'static str,
    variable: String,
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error(transparent)]
#[diagnostic(
    code(fiddle::gateway::unavailable),
    help("check the endpoint and the credential the document names")
)]
struct GatewayUnavailable(GatewayError);

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{key} names {path}, which could not be used: {reason}")]
#[diagnostic(
    code(fiddle::config::path_unusable),
    help("check {key} in the configuration document")
)]
struct PathUnusable {
    key: &'static str,
    path: String,
    reason: String,
}

#[derive(Debug, thiserror::Error)]
#[error(transparent)]
struct InvalidInvocationRef(#[from] InvocationRefError);

impl miette::Diagnostic for InvalidInvocationRef {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(match self.0 {
            InvocationRefError::Malformed(_) => "fiddle::invocation_ref::malformed",
            InvocationRefError::UnknownScheme(_) => "fiddle::invocation_ref::unknown_scheme",
            InvocationRefError::EmptyValue { .. } => "fiddle::invocation_ref::empty_value",
            InvocationRefError::IllegalValueCharacter { .. } => {
                "fiddle::invocation_ref::illegal_value_character"
            }
        }))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        let help = match self.0 {
            InvocationRefError::Malformed(_) => format!(
                "write the scheme the work comes from, in the shape that scheme takes. \
                 {shapes}",
                shapes = schemes_by_shape(),
            ),
            InvocationRefError::UnknownScheme(_) => {
                "fiddle addresses work by its source; use a scheme it knows, such as `beans:fiddle-m0-demo`".to_string()
            }
            InvocationRefError::EmptyValue { scheme } => match scheme {
                Some(scheme) if scheme.stands_alone() => format!(
                    "`{scheme}` discovers its own work, so it needs no value: write \
                     `{scheme}` to sweep what the configuration names; acting on one named \
                     item is not implemented in this build"
                ),
                Some(_) => "the scheme is recognised but names no work; append the identifier, as in `beans:fiddle-m0-demo`".to_string(),
                None => format!(
                    "no value follows the scheme, and the scheme is not one fiddle knows. \
                     {shapes}",
                    shapes = schemes_by_shape(),
                ),
            },
            InvocationRefError::IllegalValueCharacter { .. } => {
                "a reference names work, never a location: fiddle derives the paths it writes from this value, so it is an identifier only — write it with ASCII letters, digits, `-`, `_` and `:`, as in `beans:fiddle-m0-demo`".to_string()
            }
        };
        Some(Box::new(help))
    }
}

fn schemes_by_shape() -> String {
    format!(
        "Schemes that name the work they act on take a value, as in \
         `beans:fiddle-m0-demo`: {naming}. Schemes that discover their own work \
         need none: {alone}",
        naming = InvocationScheme::listed_naming_work(),
        alone = InvocationScheme::listed_standing_alone(),
    )
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("`{reference}` is not implemented in this build")]
#[diagnostic(
    code(fiddle::invocation_ref::unimplemented_form),
    help(
        "`{scheme}` discovers its own work; write `{scheme}` to sweep what the configuration names"
    )
)]
struct UnimplementedForm {
    reference: String,
    scheme: &'static str,
}

fn reference_from(argument: &str) -> Result<InvocationRef, CliError> {
    let reference: InvocationRef = argument.parse().map_err(InvalidInvocationRef::from)?;
    if reference.scheme().stands_alone() && !reference.value().is_empty() {
        return Err(UnimplementedForm {
            reference: reference.as_str().to_string(),
            scheme: reference.scheme().as_str(),
        }
        .into());
    }
    Ok(reference)
}

fn exit_code_for(termination: &Termination) -> u8 {
    match termination {
        Termination::Ran(RunOutcome::Completed) => 0,
        Termination::Ran(RunOutcome::Suspended { .. }) => 10,
        Termination::Ran(RunOutcome::Retryable { .. }) => 11,
        Termination::Ran(RunOutcome::Failed { .. }) => 20,
        Termination::Rejected(
            CliError::Config(ConfigError::NotFound(_) | ConfigError::Invalid(_))
            | CliError::InvocationRef(_)
            | CliError::UnknownCapability(_)
            | CliError::Unconfigured(_)
            | CliError::CredentialAbsent(_)
            | CliError::NamedValueAbsent(_)
            | CliError::Gateway(_)
            | CliError::PathUnusable(_)
            | CliError::UnimplementedForm(_),
        ) => EXIT_INVALID_INPUT,
    }
}

fn ports(config: &config::Config) -> (StubWorkItemPort, StubChangePort) {
    (
        StubWorkItemPort::new(&config.stub.root),
        StubChangePort::new(&config.stub.root),
    )
}

fn observe(config: &config::Config, reference: &InvocationRef) -> WorkStateView {
    let (work_items, changes) = ports(config);
    fiddle_runtime::observe(&work_items, &changes, Addressed::of(reference))
}

fn build_identity() -> FiddleBuild {
    FiddleBuild::new(env!("CARGO_PKG_VERSION"), env!("FIDDLE_SOURCE_REVISION"))
}

fn resolve_credential(
    purpose: CredentialPurpose,
    variable: &str,
) -> Result<String, CredentialAbsent> {
    std::env::var(variable).map_err(|_| CredentialAbsent {
        purpose,
        variable: variable.to_string(),
    })
}

const MODEL_ENDPOINT: &str = "agent.base_url";

fn resolve_named(
    key: &'static str,
    value: &config::WrittenOrNamed,
) -> Result<String, NamedValueAbsent> {
    match value {
        config::WrittenOrNamed::Written(written) => Ok(written.clone()),
        config::WrittenOrNamed::Named(variable) => std::env::var(variable)
            .ok()
            .filter(|resolved| !resolved.trim().is_empty())
            .ok_or_else(|| NamedValueAbsent {
                key,
                variable: variable.clone(),
            }),
    }
}

fn model_client(agent: &config::Agent) -> Result<fiddle_runtime::Gateway, CliError> {
    let base_url = resolve_named(MODEL_ENDPOINT, &agent.base_url)?;
    let credential = resolve_credential(CredentialPurpose::Model, &agent.api_key.env)?;
    fiddle_runtime::completion_model(&base_url, credential, &agent.api_key.env, &agent.model)
        .map_err(|error| CliError::Gateway(GatewayUnavailable(error)))
}

fn cancel_on_interrupt(token: &CancellationToken) {
    let token = token.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_err() {
            return;
        }
        eprintln!("interrupted; stopping the attempt (interrupt again to exit immediately)");
        token.cancel();
        if tokio::signal::ctrl_c().await.is_ok() {
            std::process::exit(EXIT_INTERRUPTED);
        }
    });
}

struct Forge {
    ctx: EffectContext,
    trace: AttemptTrace,
    publishing: Option<Publishing>,
}

struct Publishing {
    head_sha: String,
    workflow: String,
}

async fn resolve_forge(
    config: &config::Config,
    config_path: &Path,
    cancel: &CancellationToken,
    selection: Selection,
    reference: &InvocationRef,
) -> Result<Forge, CliError> {
    let missing = |missing: &'static str| Unconfigured {
        capability: selection.id(),
        missing,
        path: config_path.display().to_string(),
    };
    let unusable = |key: &'static str, path: &Path, reason: String| PathUnusable {
        key,
        path: path.display().to_string(),
        reason,
    };

    let github = config.github.as_ref().ok_or_else(|| missing("[github]"))?;
    let (work, workflow): (PathBuf, Option<String>) = match selection {
        Selection::Publish => (
            github
                .work
                .as_ref()
                .ok_or_else(|| missing("github.work"))?
                .clone(),
            Some(
                github
                    .workflow
                    .clone()
                    .ok_or_else(|| missing("github.workflow"))?,
            ),
        ),
        Selection::Propose | Selection::Mitigate => {
            let workspace = config
                .workspace
                .as_ref()
                .ok_or_else(|| missing("[workspace]"))?;
            (
                fiddle_runtime::attempt_worktree(
                    &workspace.root,
                    &config.project.name,
                    &reference.as_str(),
                ),
                None,
            )
        }
        Selection::Mark | Selection::Repair => return Err(missing("[github]").into()),
    };

    let credential = resolve_credential(CredentialPurpose::Forge, &github.token.env)?;
    let timeout = github.timeout.as_duration();
    let gh = GhCli::new(
        PathBuf::from(&github.cli.program),
        github.cli.args.clone(),
        credential.clone(),
        &github.token.env,
        github.config_dir.clone(),
        timeout,
    );
    let git = GitCli::new(github.git.clone(), credential, &github.token.env, timeout);

    std::fs::create_dir_all(&github.config_dir)
        .map_err(|e| unusable("github.config_dir", &github.config_dir, e.to_string()))?;

    let publishing = match workflow {
        Some(workflow) => Some(Publishing {
            head_sha: git
                .head_sha(&work, cancel)
                .await
                .map_err(|e| unusable("github.work", &work, e.to_string()))?,
            workflow,
        }),
        None => None,
    };

    Ok(Forge {
        ctx: EffectContext::new(gh, git, work, cancel.clone()),
        trace: AttemptTrace::new(),
        publishing,
    })
}

fn build_capability<'a>(
    selection: Selection,
    config: &'a config::Config,
    config_path: &Path,
    cancel: &CancellationToken,
    reference: &InvocationRef,
    forge: Option<&'a Forge>,
) -> Result<Box<dyn Capability + 'a>, CliError> {
    let missing = |missing: &'static str| Unconfigured {
        capability: selection.id(),
        missing,
        path: config_path.display().to_string(),
    };

    match selection {
        Selection::Mark => Ok(Box::new(StubMark::new(
            &config.stub.root,
            &config.project.name,
        ))),
        Selection::Publish => {
            let github = config.github.as_ref().ok_or_else(|| missing("[github]"))?;
            let forge = forge.ok_or_else(|| missing("[github]"))?;
            let publishing = forge
                .publishing
                .as_ref()
                .ok_or_else(|| missing("github.work"))?;

            cancel_on_interrupt(cancel);

            let executor = Executor::new(
                fiddle_core::PUBLISH_CHANGE,
                config.project.name.clone(),
                reference.as_str(),
                &github.policy,
                &forge.ctx,
                &forge.trace,
                github.read_retry.as_read_retry(),
            );

            Ok(Box::new(PublishChange::new(
                executor,
                PublishConfig {
                    repo: github.repo.to_string(),
                    head_owner: github.repo.owner.clone(),
                    base: github.base.clone(),
                    head_sha: publishing.head_sha.clone(),
                    title: format!("{}: {}", config.project.name, reference.as_str()),
                    body: format!(
                        "Opened by fiddle for {} in project {}.\n\n\
                         This branch and this pull request are named after the \
                         effect identity fiddle derives from that pair, so a \
                         later attempt at the same work finds them rather than \
                         creating a second set.\n",
                        reference.as_str(),
                        config.project.name,
                    ),
                    workflow: publishing.workflow.clone(),
                    required_checks: github.required_checks.clone(),
                    stub_root: config.stub.root.clone(),
                    project: config.project.name.clone(),
                },
            )))
        }
        Selection::Repair => {
            let agent = config.agent.as_ref().ok_or_else(|| missing("[agent]"))?;
            let workspace = config
                .workspace
                .as_ref()
                .ok_or_else(|| missing("[workspace]"))?;
            let fixture = workspace
                .fixture
                .as_ref()
                .ok_or_else(|| missing("workspace.fixture"))?;
            let check = workspace
                .check
                .as_ref()
                .ok_or_else(|| missing("workspace.check"))?;

            let gateway = model_client(agent)?;

            cancel_on_interrupt(cancel);

            let config::Isolation::GitWorktree = workspace.isolation;
            let config::Cleanup::Always = workspace.cleanup;

            Ok(Box::new(FixtureRepair::new(
                gateway.model,
                RepairConfig {
                    fixture: fixture.clone(),
                    workspace_root: workspace.root.clone(),
                    stub_root: config.stub.root.clone(),
                    project: config.project.name.clone(),
                    check: WorkspaceCommand {
                        program: check.program.clone(),
                        args: check.args.clone(),
                        timeout: workspace.command_timeout.as_duration(),
                    },
                    commands: declared_commands(workspace),
                    command_timeout: workspace.command_timeout.as_duration(),
                    budget: AgentBudget {
                        max_turns: agent.max_turns,
                        max_tokens: agent.max_tokens,
                        deadline: agent.deadline.as_duration(),
                        max_changed_files: agent.max_changed_files,
                        tool_timeout: agent.tool_timeout.as_duration(),
                    },
                    redaction: gateway.redaction,
                    cancel: cancel.clone(),
                },
            )))
        }

        Selection::Propose => {
            let github = config.github.as_ref().ok_or_else(|| missing("[github]"))?;
            let decision = github
                .decision
                .as_ref()
                .ok_or_else(|| missing("[github.decision]"))?;
            let agent = config.agent.as_ref().ok_or_else(|| missing("[agent]"))?;
            let workspace = config
                .workspace
                .as_ref()
                .ok_or_else(|| missing("[workspace]"))?;
            let fixture = workspace
                .fixture
                .as_ref()
                .ok_or_else(|| missing("workspace.fixture"))?;
            let check = workspace
                .check
                .as_ref()
                .ok_or_else(|| missing("workspace.check"))?;
            let forge = forge.ok_or_else(|| missing("[github]"))?;

            let gateway = model_client(agent)?;

            cancel_on_interrupt(cancel);

            let config::Isolation::GitWorktree = workspace.isolation;
            let config::Cleanup::Always = workspace.cleanup;

            let executor = Executor::new(
                fiddle_core::PROPOSE_CHANGE,
                config.project.name.clone(),
                reference.as_str(),
                &github.policy,
                &forge.ctx,
                &forge.trace,
                github.read_retry.as_read_retry(),
            );

            Ok(Box::new(ProposeChange::new(
                executor,
                &forge.ctx,
                &forge.trace,
                gateway.model,
                ProposeConfig {
                    repo: github.repo.to_string(),
                    head_owner: github.repo.owner.clone(),
                    base: github.base.clone(),
                    title: format!("{}: {}", config.project.name, reference.as_str()),
                    body: format!(
                        "Opened by fiddle for {} in project {}, as a draft.\n\n\
                         The change was produced by one bounded attempt and passed \
                         the check this deployment configured. Marking it ready for \
                         review is the step fiddle will not take on its own: it \
                         asks in a comment below and acts only on a reply from \
                         somebody this deployment nominated.\n",
                        reference.as_str(),
                        config.project.name,
                    ),
                    project: config.project.name.clone(),
                    fixture: fixture.clone(),
                    workspace_root: workspace.root.clone(),
                    stub_root: config.stub.root.clone(),
                    check: WorkspaceCommand {
                        program: check.program.clone(),
                        args: check.args.clone(),
                        timeout: workspace.command_timeout.as_duration(),
                    },
                    commands: declared_commands(workspace),
                    command_timeout: workspace.command_timeout.as_duration(),
                    budget: AgentBudget {
                        max_turns: agent.max_turns,
                        max_tokens: agent.max_tokens,
                        deadline: agent.deadline.as_duration(),
                        max_changed_files: agent.max_changed_files,
                        tool_timeout: agent.tool_timeout.as_duration(),
                    },
                    redaction: gateway.redaction,
                    deciders: decision.authorized.clone(),
                    interpretation: interpretation_bounds(agent),
                    cancel: cancel.clone(),
                },
            )))
        }

        Selection::Mitigate => {
            let github = config.github.as_ref().ok_or_else(|| missing("[github]"))?;
            let scanner = config
                .scanner
                .as_ref()
                .ok_or_else(|| missing("[scanner]"))?;
            let sweep = config
                .orchestration
                .as_ref()
                .and_then(|orchestration| orchestration.cve.as_ref())
                .ok_or_else(|| missing("[orchestration.cve]"))?;
            let agent = config.agent.as_ref().ok_or_else(|| missing("[agent]"))?;
            let workspace = config
                .workspace
                .as_ref()
                .ok_or_else(|| missing("[workspace]"))?;
            let Some(check) = workspace.checks.first() else {
                return Err(missing("[[workspace.checks]]").into());
            };
            let forge = forge.ok_or_else(|| missing("[github]"))?;

            let gateway = model_client(agent)?;

            cancel_on_interrupt(cancel);

            let config::Isolation::GitWorktree = workspace.isolation;
            let config::Cleanup::Always = workspace.cleanup;

            let scans = config.report.dir.join("scan");
            let rescans = config.report.dir.join("rescan");

            let executor = Executor::new(
                fiddle_core::CVE_MITIGATE,
                config.project.name.clone(),
                reference.as_str(),
                &github.policy,
                &forge.ctx,
                &forge.trace,
                github.read_retry.as_read_retry(),
            );

            Ok(Box::new(fiddle_runtime::CveMitigate::new(
                executor,
                &forge.ctx,
                fiddle_runtime::Wizcli::new(
                    PathBuf::from(&scanner.cli.program),
                    scanner.cli.args.clone(),
                    scans,
                    scanner.timeout.as_duration(),
                    cancel.clone(),
                ),
                gateway.model,
                fiddle_runtime::MitigateConfig {
                    repo: github.repo.to_string(),
                    head_owner: github.repo.owner.clone(),
                    base: github.base.clone(),
                    title: format!("{}: dependency advisories", config.project.name),
                    project: config.project.name.clone(),
                    stub_root: config.stub.root.clone(),
                    tree: workspace
                        .fixture
                        .as_ref()
                        .ok_or_else(|| missing("workspace.fixture"))?
                        .clone(),
                    workspace_root: workspace.root.clone(),
                    image: sweep.image.clone(),
                    severities: sweep.severities.clone(),
                    scratch: rescans,
                    checks: workspace
                        .checks
                        .iter()
                        .map(|check| fiddle_runtime::evaluate::Check {
                            program: check.program.clone(),
                            args: check.args.clone(),
                            success: match check.success {
                                config::Success::ExitZero => {
                                    fiddle_runtime::evaluate::Success::ExitZero
                                }
                                config::Success::ExitZeroAndNoOutput => {
                                    fiddle_runtime::evaluate::Success::ExitZeroAndNoOutput
                                }
                                config::Success::ArtefactWritten => {
                                    fiddle_runtime::evaluate::Success::ArtefactWritten
                                }
                            },
                        })
                        .collect(),
                    check: WorkspaceCommand {
                        program: check.program.clone(),
                        args: check.args.clone(),
                        timeout: workspace.command_timeout.as_duration(),
                    },
                    commands: declared_commands(workspace),
                    budget: AgentBudget {
                        max_turns: agent.max_turns,
                        max_tokens: agent.max_tokens,
                        deadline: agent.deadline.as_duration(),
                        max_changed_files: agent.max_changed_files,
                        tool_timeout: agent.tool_timeout.as_duration(),
                    },
                    redaction: gateway.redaction,
                    command_timeout: workspace.command_timeout.as_duration(),
                    findings: fiddle_runtime::cve::verdict::Budget::of(sweep.max_findings),
                    max_attempts: u32::try_from(agent.max_capability_attempts).unwrap_or(u32::MAX),
                    report_dir: config.report.dir.clone(),
                    today: fiddle_runtime::capability::cve::today_utc(),
                    cancel: cancel.clone(),
                },
            )))
        }
    }
}

fn declared_commands(workspace: &config::Workspace) -> std::sync::Arc<Vec<DeclaredCommand>> {
    std::sync::Arc::new(
        workspace
            .commands
            .iter()
            .map(|command| DeclaredCommand {
                program: command.program.clone(),
                args: command.args.clone(),
                extend: match command.extend {
                    config::Extend::None => Extend::None,
                    config::Extend::Arguments => Extend::Arguments,
                },
            })
            .collect(),
    )
}

fn interpretation_bounds(agent: &config::Agent) -> InterpretationBounds {
    InterpretationBounds {
        max_reply_bytes: 4_096,
        max_tokens: agent.max_tokens,
        deadline: agent.deadline.as_duration(),
    }
}

async fn dispatch(cli: &cli::Cli) -> Result<RunOutcome, CliError> {
    match &cli.command {
        cli::Command::Config { action } => match action {
            cli::ConfigCommand::Check { json } => {
                let config = config::load(&cli.config)?;
                if *json {
                    println!("{}", render::config_check_json(&config));
                } else {
                    println!("{}", render::config_check_human(&config));
                }
                Ok(RunOutcome::Completed)
            }
        },
        cli::Command::Inspect {
            invocation_ref,
            capability,
            json,
        } => {
            let reference = reference_from(invocation_ref)?;
            let selection = Selection::resolve(capability.as_deref(), &reference)?;
            let config = config::load(&cli.config)?;
            let observed = observe(&config, &reference);
            let expected_marker =
                fiddle_core::correlation_key(&config.project.name, &reference.as_str());
            let assessment = fiddle_core::assess(&observed, &expected_marker);
            let next_action = fiddle_core::derive_next(&observed, &expected_marker, selection.id());
            if *json {
                println!(
                    "{}",
                    render::inspect_json(&reference, &observed, &assessment, &next_action)
                );
            } else {
                println!(
                    "{}",
                    render::inspect_human(&reference, &observed, &assessment, &next_action)
                );
            }
            Ok(RunOutcome::Completed)
        }

        cli::Command::Run {
            invocation_ref,
            mode,
            capability,
            json,
        } => {
            let reference = reference_from(invocation_ref)?;
            let selection = Selection::resolve(capability.as_deref(), &reference)?;
            let config = config::load(&cli.config)?;

            let (work_items, changes) = ports(&config);
            let cancel = CancellationToken::new();
            let forge = match selection {
                Selection::Publish | Selection::Propose | Selection::Mitigate => {
                    Some(resolve_forge(&config, &cli.config, &cancel, selection, &reference).await?)
                }
                Selection::Mark | Selection::Repair => None,
            };
            let selected = build_capability(
                selection,
                &config,
                &cli.config,
                &cancel,
                &reference,
                forge.as_ref(),
            )?;
            let record = fiddle_runtime::attempt(&AttemptContext {
                project: &config.project.name,
                reference: &reference,
                mode: *mode,
                build: build_identity(),
                report_dir: &config.report.dir,
                work_items: &work_items,
                changes: &changes,
                capability: selected.as_ref(),
                trace: forge.as_ref().map(|forge| &forge.trace),
            })
            .await;

            if let Some(failure) = &record.evidence_failure {
                eprintln!("{}", render::evidence_failure(&config.report.dir, failure));
            }
            if *json {
                println!(
                    "{}",
                    render::run_json(&record.bundle, record.published.as_deref())
                );
            } else {
                println!(
                    "{}",
                    render::run_human(&record.bundle, record.published.as_deref())
                );
            }
            Ok(record.bundle.outcome)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fiddle_core::{ChangeSetState, NextAction, Observation, Published};
    use fiddle_runtime::ChangePort;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn every_outcome_maps_to_the_row_the_table_documents() {
        let rows: [(RunOutcome, u8); 4] = [
            (RunOutcome::Completed, 0),
            (
                RunOutcome::Suspended {
                    reason: Published::of("awaiting a decision"),
                },
                10,
            ),
            (
                RunOutcome::Retryable {
                    reason: Published::of("try again"),
                },
                11,
            ),
            (
                RunOutcome::Failed {
                    error: Published::of("will not succeed"),
                },
                20,
            ),
        ];
        for (outcome, code) in rows {
            assert_eq!(
                exit_code_for(&Termination::Ran(outcome.clone())),
                code,
                "{outcome:?} must exit {code}"
            );
        }

        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::UnknownCapability(
                UnknownCapability {
                    requested: "nonsense".into(),
                    known: "stub_mark".into(),
                }
            ))),
            EXIT_INVALID_INPUT
        );
        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::InvocationRef(
                InvalidInvocationRef(InvocationRefError::EmptyValue { scheme: None })
            ))),
            EXIT_INVALID_INPUT
        );
        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::CredentialAbsent(
                CredentialAbsent {
                    purpose: CredentialPurpose::Model,
                    variable: "LITELLM_API_KEY".into()
                }
            ))),
            EXIT_INVALID_INPUT
        );
        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::NamedValueAbsent(
                NamedValueAbsent {
                    key: MODEL_ENDPOINT,
                    variable: "FIDDLE_MODEL_BASE_URL".into()
                }
            ))),
            EXIT_INVALID_INPUT
        );
        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::Unconfigured(
                Unconfigured {
                    capability: fiddle_core::FIXTURE_REPAIR,
                    missing: "[agent]",
                    path: "fiddle.toml".to_string(),
                }
            ))),
            EXIT_INVALID_INPUT
        );
        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::Gateway(
                GatewayUnavailable(fiddle_runtime::GatewayError {
                    base_url: "https://gateway.invalid/v1".into(),
                    variable: "LITELLM_API_KEY".into(),
                })
            ))),
            EXIT_INVALID_INPUT
        );
        assert_eq!(
            exit_code_for(&Termination::Rejected(CliError::UnimplementedForm(
                UnimplementedForm {
                    reference: "cve:CVE-2026-1234".into(),
                    scheme: "cve",
                }
            ))),
            EXIT_INVALID_INPUT,
            "a form this build cannot act on is invalid input, not a run that failed"
        );
    }

    #[test]
    fn every_registered_capability_can_be_selected() {
        for registered in CAPABILITIES {
            let selection = Selection::parse(registered.0).unwrap_or_else(|error| {
                panic!(
                    "`{registered}` is advertised by CAPABILITIES and cannot be \
                     selected: {error}"
                )
            });
            assert_eq!(
                selection.id(),
                registered,
                "a selection must run the capability it was asked for"
            );
        }
    }

    #[test]
    fn an_unknown_capability_is_refused_with_the_known_list() {
        let error = Selection::parse("nope").unwrap_err();
        assert!(error.known.contains("stub_mark"), "{}", error.known);
        assert!(error.known.contains("fixture_repair"), "{}", error.known);
    }

    #[test]
    fn an_absent_credential_is_reported_under_the_name_that_was_asked_for() {
        let error = resolve_credential(
            CredentialPurpose::Model,
            "FIDDLE_A_VARIABLE_NOTHING_EXPORTS",
        )
        .unwrap_err();
        assert_eq!(error.variable, "FIDDLE_A_VARIABLE_NOTHING_EXPORTS");
        let rendered = render::diagnostic(&error);
        assert!(
            rendered.contains("FIDDLE_A_VARIABLE_NOTHING_EXPORTS"),
            "an operator must learn which variable to export: {rendered}"
        );
    }

    #[test]
    fn a_written_base_url_resolves_to_the_value_the_document_carries() {
        let resolved = resolve_named(
            MODEL_ENDPOINT,
            &config::WrittenOrNamed::Written("https://gateway.invalid/v1".to_string()),
        )
        .expect("a written endpoint needs no variable");
        assert_eq!(resolved, "https://gateway.invalid/v1");
    }

    #[test]
    fn a_named_base_url_that_nothing_exports_refuses_rather_than_defaults() {
        let error = resolve_named(
            MODEL_ENDPOINT,
            &config::WrittenOrNamed::Named("FIDDLE_A_VARIABLE_NOTHING_EXPORTS".to_string()),
        )
        .expect_err("an endpoint named by an unset variable is not a value");
        assert_eq!(error.key, MODEL_ENDPOINT);
        assert_eq!(error.variable, "FIDDLE_A_VARIABLE_NOTHING_EXPORTS");
        let rendered = render::diagnostic(&error);
        assert!(
            rendered.contains("FIDDLE_A_VARIABLE_NOTHING_EXPORTS")
                && rendered.contains(MODEL_ENDPOINT),
            "an operator must learn the variable to export and the key that \
             names it: {rendered}"
        );
    }

    #[test]
    fn a_repair_whose_named_endpoint_is_unset_is_refused_before_the_credential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(
            &path,
            "[project]\nname=\"icecube\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n\
             [agent]\nmodel=\"m\"\n\
             base_url={env=\"FIDDLE_A_VARIABLE_NOTHING_EXPORTS\"}\n\
             api_key={env=\"FIDDLE_A_VARIABLE_NOTHING_EXPORTS\"}\n\
             [workspace]\nfixture=\"/nonexistent\"\n\
             check={program=\"true\",args=[]}\n",
        )
        .unwrap();
        let loaded = config::load(&path).unwrap();

        let Err(error) = build_capability(
            Selection::Repair,
            &loaded,
            &path,
            &CancellationToken::new(),
            &a_reference(),
            None,
        ) else {
            panic!("nothing exports that variable, so no endpoint exists")
        };
        match error {
            CliError::NamedValueAbsent(absent) => {
                assert_eq!(absent.key, MODEL_ENDPOINT);
                assert_eq!(absent.variable, "FIDDLE_A_VARIABLE_NOTHING_EXPORTS");
            }
            other => panic!("expected the endpoint to be named, got {other:?}"),
        }
    }

    #[test]
    fn each_credential_is_described_by_the_thing_that_needs_it() {
        for purpose in CredentialPurpose::ALL {
            let rendered = render::diagnostic(&CredentialAbsent {
                purpose,
                variable: "A_VARIABLE".to_string(),
            });
            assert!(
                rendered.contains(&format!("the {purpose} credential A_VARIABLE is not set")),
                "a {purpose} credential must be reported as one: {rendered}"
            );
            assert!(
                rendered.contains(purpose.table()),
                "and the help must name the table the document writes it in: {rendered}"
            );
            for other in CredentialPurpose::ALL
                .into_iter()
                .filter(|candidate| *candidate != purpose)
            {
                assert!(
                    !rendered.contains(&other.to_string()),
                    "a {purpose} credential described with `{other}` sends an \
                     operator to {}: {rendered}",
                    other.table()
                );
            }
        }
    }

    #[test]
    fn the_deterministic_capability_is_built_without_a_credential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(
            &path,
            "[project]\nname=\"icecube\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n\
             [agent]\nmodel=\"m\"\nbase_url=\"http://127.0.0.1:9/v1\"\n\
             api_key={env=\"FIDDLE_A_VARIABLE_NOTHING_EXPORTS\"}\n",
        )
        .unwrap();
        let loaded = config::load(&path).unwrap();

        let Ok(built) = build_capability(
            Selection::Mark,
            &loaded,
            &path,
            &CancellationToken::new(),
            &a_reference(),
            None,
        ) else {
            panic!("the deterministic capability needs nothing but the document")
        };
        assert_eq!(built.id(), fiddle_core::STUB_MARK);
    }

    fn a_reference() -> InvocationRef {
        "beans:fiddle-m0-demo".parse().unwrap()
    }

    #[test]
    fn the_sweep_takes_the_model_s_check_from_the_list_it_already_requires() {
        let sweep = "[project]\nname=\"icecube\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n\
             [github]\nrepo=\"peel/fiddle\"\nbase=\"main\"\n\
             token={env=\"FIDDLE_A_VARIABLE_NOTHING_EXPORTS\"}\n\
             [scanner]\n\
             [orchestration.cve]\nimage=\"ghcr.io/acme/icecube:latest\"\n\
             [agent]\nmodel=\"m\"\nbase_url=\"http://127.0.0.1:9/v1\"\n\
             api_key={env=\"FIDDLE_A_VARIABLE_NOTHING_EXPORTS\"}\n\
             [workspace]\nfixture=\"/nonexistent\"\n\
             [[workspace.checks]]\nprogram=\"true\"\nargs=[]\nsuccess=\"exit-zero\"\n";
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(&path, sweep).unwrap();
        let loaded = config::load(&path).unwrap();

        let Err(error) = build_capability(
            Selection::Mitigate,
            &loaded,
            &path,
            &CancellationToken::new(),
            &a_reference(),
            None,
        ) else {
            panic!("no forge was supplied, so nothing can be built")
        };
        match error {
            CliError::Unconfigured(unconfigured) => assert_eq!(
                unconfigured.missing, "[github]",
                "the sweep must get past the check question without a \
                 `[workspace] check`, and be refused for the next thing instead"
            ),
            other => panic!("expected the forge to be named, got {other:?}"),
        }

        let both = dir.path().join("both.toml");
        std::fs::write(
            &both,
            format!("{sweep}[workspace]\ncheck={{ program = \"true\", args = [] }}\n"),
        )
        .unwrap();
        assert!(
            config::load(&both).is_err(),
            "a document naming both check shapes must not load, or the arm above \
             could have gone on reading the singular one"
        );
    }

    #[test]
    fn a_publication_over_a_document_with_no_forge_names_the_missing_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(
            &path,
            "[project]\nname=\"icecube\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n",
        )
        .unwrap();
        let loaded = config::load(&path).unwrap();

        let Err(error) = build_capability(
            Selection::Publish,
            &loaded,
            &path,
            &CancellationToken::new(),
            &a_reference(),
            None,
        ) else {
            panic!("a publication needs a forge to publish to")
        };
        match error {
            CliError::Unconfigured(unconfigured) => {
                assert_eq!(unconfigured.missing, "[github]");
                assert_eq!(unconfigured.capability, fiddle_core::PUBLISH_CHANGE);
            }
            other => panic!("expected a missing-table refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_forge_names_each_key_it_cannot_invent_before_the_credential() {
        let forge = "[project]\nname=\"icecube\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n\
             [github]\nrepo=\"peel/fiddle\"\nbase=\"main\"\n\
             token={env=\"FIDDLE_A_VARIABLE_NOTHING_EXPORTS\"}\n";
        let reference: InvocationRef = "beans:m3-demo".parse().unwrap();
        for (selection, extra, expected) in [
            (Selection::Publish, "", "github.work"),
            (
                Selection::Publish,
                "work=\"/nonexistent\"\n",
                "github.workflow",
            ),
            (Selection::Propose, "", "[workspace]"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("fiddle.toml");
            std::fs::write(&path, format!("{forge}{extra}")).unwrap();
            let loaded = config::load(&path).unwrap();

            let Err(error) = resolve_forge(
                &loaded,
                &path,
                &CancellationToken::new(),
                selection,
                &reference,
            )
            .await
            else {
                panic!("the document is incomplete and must be refused");
            };
            match error {
                CliError::Unconfigured(unconfigured) => {
                    assert_eq!(unconfigured.missing, expected);
                    assert_eq!(unconfigured.capability, selection.id());
                }
                other => panic!("expected {expected} to be named, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn a_proposal_reads_no_head_off_the_tree_its_attempt_has_yet_to_create() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(
            &path,
            "[project]\nname=\"icecube\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n\
             [github]\nrepo=\"peel/fiddle\"\nbase=\"main\"\n\
             token={env=\"FIDDLE_A_VARIABLE_NOTHING_EXPORTS\"}\n\
             [workspace]\nroot=\"/nonexistent/workspaces\"\n",
        )
        .unwrap();
        let loaded = config::load(&path).unwrap();
        let reference: InvocationRef = "beans:m3-demo".parse().unwrap();

        let Err(error) = resolve_forge(
            &loaded,
            &path,
            &CancellationToken::new(),
            Selection::Propose,
            &reference,
        )
        .await
        else {
            panic!("nothing exports that variable and it must be refused");
        };
        match error {
            CliError::CredentialAbsent(absent) => {
                assert_eq!(absent.variable, "FIDDLE_A_VARIABLE_NOTHING_EXPORTS");
            }
            other => panic!("expected the variable to be named, got {other:?}"),
        }
    }

    #[test]
    fn a_proposals_worktree_is_derived_from_the_runs_own_two_names() {
        let root = Path::new("/w");
        let derived = fiddle_runtime::attempt_worktree(root, "icecube", "beans:m3-demo");

        assert_eq!(derived.parent(), Some(root), "under the configured root");
        assert_ne!(
            derived,
            fiddle_runtime::attempt_worktree(root, "icecube", "beans:m3-demo-again")
        );
        assert_ne!(
            derived,
            fiddle_runtime::attempt_worktree(root, "another-project", "beans:m3-demo")
        );
        assert_eq!(
            derived,
            fiddle_runtime::attempt_worktree(root, "icecube", "beans:m3-demo")
        );
    }

    #[tokio::test]
    async fn a_complete_forge_without_its_credential_names_the_variable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(
            &path,
            "[project]\nname=\"icecube\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n\
             [github]\nrepo=\"peel/fiddle\"\nbase=\"main\"\n\
             token={env=\"FIDDLE_A_VARIABLE_NOTHING_EXPORTS\"}\n\
             work=\"/nonexistent\"\nworkflow=\"verify.yml\"\n",
        )
        .unwrap();
        let loaded = config::load(&path).unwrap();
        let reference: InvocationRef = "beans:m3-demo".parse().unwrap();

        let Err(error) = resolve_forge(
            &loaded,
            &path,
            &CancellationToken::new(),
            Selection::Publish,
            &reference,
        )
        .await
        else {
            panic!("nothing exports that variable and it must be refused");
        };
        match error {
            CliError::CredentialAbsent(absent) => {
                assert_eq!(absent.variable, "FIDDLE_A_VARIABLE_NOTHING_EXPORTS");
            }
            other => panic!("expected the variable to be named, got {other:?}"),
        }
    }

    #[test]
    fn a_repair_over_an_m0_document_names_the_missing_table_not_the_credential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fiddle.toml");
        std::fs::write(
            &path,
            "[project]\nname=\"icecube\"\n[stub]\nroot=\"s\"\n[report]\ndir=\"r\"\n",
        )
        .unwrap();
        let loaded = config::load(&path).unwrap();

        let Err(error) = build_capability(
            Selection::Repair,
            &loaded,
            &path,
            &CancellationToken::new(),
            &a_reference(),
            None,
        ) else {
            panic!("a repair needs a model and somewhere to work")
        };
        match error {
            CliError::Unconfigured(unconfigured) => {
                assert_eq!(unconfigured.missing, "[agent]");
                assert_eq!(unconfigured.capability, fiddle_core::FIXTURE_REPAIR);
            }
            other => panic!("expected a missing-table refusal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_race_after_executing_still_exits_on_the_table() {
        let root = tempfile::tempdir().unwrap();
        let stub_root = root.path().join("stub-state");
        std::fs::create_dir_all(stub_root.join("work")).unwrap();
        std::fs::create_dir_all(stub_root.join("changes")).unwrap();
        std::fs::write(
            stub_root.join("work/fiddle-m0-demo.json"),
            r#"{"id":"fiddle-m0-demo","status":"open"}"#,
        )
        .unwrap();

        let reference: InvocationRef = "beans:fiddle-m0-demo".parse().unwrap();
        let work_items = StubWorkItemPort::new(&stub_root);
        let changes = OvertakenAfterTheFirstLook {
            inner: StubChangePort::new(&stub_root),
            change_set: stub_root.join("changes/fiddle-m0-demo.json"),
            looks: AtomicUsize::new(0),
        };
        let marking = StubMark::new(&stub_root, "icecube");
        let record = fiddle_runtime::attempt(&AttemptContext {
            project: "icecube",
            reference: &reference,
            mode: fiddle_core::Mode::Unattended,
            build: build_identity(),
            report_dir: &root.path().join("reports"),
            work_items: &work_items,
            changes: &changes,
            capability: &marking as &dyn Capability,
            trace: None,
        })
        .await;

        assert!(
            matches!(record.bundle.next_action, NextAction::Blocked { .. }),
            "the race must have produced a blocked re-derivation, got {:?}",
            record.bundle.next_action
        );
        assert!(
            matches!(record.bundle.outcome, RunOutcome::Failed { .. }),
            "a blocked re-derivation is not a completed run, got {:?}",
            record.bundle.outcome
        );
        assert_eq!(
            exit_code_for(&Termination::Ran(record.bundle.outcome)),
            20,
            "the same row an unobservable world exits on, because the world it \
             leaves behind is the same one a later invocation will block on"
        );
    }

    struct OvertakenAfterTheFirstLook {
        inner: StubChangePort,
        change_set: PathBuf,
        looks: AtomicUsize,
    }

    impl ChangePort for OvertakenAfterTheFirstLook {
        fn observe(&self, work_id: &str) -> Observation<ChangeSetState> {
            if self.looks.fetch_add(1, Ordering::Relaxed) == 1 {
                std::fs::write(&self.change_set, r#"{"marker":"0123456789abcdef"}"#).unwrap();
            }
            self.inner.observe(work_id)
        }
    }
}
