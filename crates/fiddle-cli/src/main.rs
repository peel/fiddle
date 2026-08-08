mod cli;
mod config;
mod render;

use clap::Parser;
use config::ConfigError;
use fiddle_core::{
    AttemptId, CapabilityId, FiddleBuild, InvocationRef, InvocationRefError, Mode, ReportBundle,
    RunOutcome, WorkRef, WorkStateView,
};
use fiddle_runtime::{
    Capability, RunContext, RunReport, StubChangePort, StubMark, StubWorkItemPort, CAPABILITIES,
};
use std::process::ExitCode;

/// Usage error or invalid input — row `2` of the exit-code table. Clap already
/// exits with this code for usage errors, so the constant exists to keep every
/// half of the row visibly the same number.
const EXIT_INVALID_INPUT: u8 = 2;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    let termination = match dispatch(&cli) {
        Ok(outcome) => Termination::Ran(outcome),
        Err(error) => {
            eprintln!("{}", render::diagnostic(&error));
            Termination::Rejected(error)
        }
    };
    ExitCode::from(exit_code_for(&termination))
}

/// How an invocation ended: as a typed run outcome, or as a rejection before
/// any plan was executed.
///
/// The two halves are joined into one type so the exit-code table has one input
/// and therefore one mapping function. Without it, "the mapping lives in
/// exactly one place" would be a claim about discipline rather than about the
/// code.
enum Termination {
    /// The command reached a conclusion about the work.
    Ran(RunOutcome),
    /// The command was refused before it could. Read-only commands that
    /// succeed report [`RunOutcome::Completed`]; this arm is only reached when
    /// fiddle declined the invocation itself.
    Rejected(CliError),
}

/// Everything a command can fail with, unified so the exit-code mapping has a
/// single input type.
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
}

/// A `--capability` value naming nothing this build can execute.
///
/// Rejected rather than ignored: a run asked to do something fiddle has never
/// heard of and that exited 0 having done nothing would be indistinguishable
/// from a run that did the work. The diagnostic names the value *and* what this
/// build does know, because the usual cause is a typo or a stale script.
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

/// The capability `--capability` names, or a rejection listing what exists.
///
/// The known-id list comes from [`CAPABILITIES`], so the diagnostic cannot fall
/// out of step with what the binary can actually run.
fn resolve_capability(requested: &str) -> Result<CapabilityId, UnknownCapability> {
    CAPABILITIES
        .into_iter()
        .find(|candidate| candidate.0 == requested)
        .ok_or_else(|| UnknownCapability {
            requested: requested.to_string(),
            known: CAPABILITIES
                .iter()
                .map(|capability| capability.0)
                .collect::<Vec<_>>()
                .join(", "),
        })
}

/// Presentation for a rejected invocation reference.
///
/// The grammar and the defect taxonomy belong to `fiddle-core`, which stays free
/// of `miette`; what belongs to the CLI is how a defect is *shown*. This wrapper
/// supplies that and nothing else — a stable diagnostic code per defect and the
/// help text that tells the caller how to fix that specific defect, so `bogus`,
/// `mystery:x`, and `beans:` are never reported with the same words.
///
/// `Diagnostic` is implemented by hand rather than derived so the three defects
/// map to their codes and help text in one visible table instead of a mirrored
/// copy of the core enum.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
struct InvalidInvocationRef(#[from] InvocationRefError);

impl miette::Diagnostic for InvalidInvocationRef {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(match self.0 {
            InvocationRefError::Malformed(_) => "fiddle::invocation_ref::malformed",
            InvocationRefError::UnknownScheme(_) => "fiddle::invocation_ref::unknown_scheme",
            InvocationRefError::EmptyValue => "fiddle::invocation_ref::empty_value",
        }))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        Some(Box::new(match self.0 {
            InvocationRefError::Malformed(_) => {
                "name the source scheme first, separated by a colon: `fiddle inspect beans:fiddle-m0-demo`"
            }
            InvocationRefError::UnknownScheme(_) => {
                "fiddle addresses work by its source; use a scheme it knows, such as `beans:fiddle-m0-demo`"
            }
            InvocationRefError::EmptyValue => {
                "the scheme is recognised but names no work; append the identifier, as in `beans:fiddle-m0-demo`"
            }
        }))
    }
}

/// The single realisation of the exit-code table (design §4.5).
///
/// The whole table is one `match`, so every row is visible at once and a new
/// outcome variant cannot be added without the compiler demanding its code.
/// Nothing else in the binary decides an exit code.
///
/// | code | meaning                                                    |
/// |------|------------------------------------------------------------|
/// | 0    | completed, or `config check` valid, or `inspect` succeeded  |
/// | 2    | usage error or invalid configuration                        |
/// | 10   | suspended                                                   |
/// | 11   | retryable                                                   |
/// | 20   | failed                                                       |
fn exit_code_for(termination: &Termination) -> u8 {
    match termination {
        Termination::Ran(RunOutcome::Completed) => 0,
        Termination::Ran(RunOutcome::Suspended { .. }) => 10,
        Termination::Ran(RunOutcome::Retryable { .. }) => 11,
        Termination::Ran(RunOutcome::Failed { .. }) => 20,
        Termination::Rejected(
            CliError::Config(ConfigError::NotFound(_) | ConfigError::Invalid(_))
            | CliError::InvocationRef(_)
            | CliError::UnknownCapability(_),
        ) => EXIT_INVALID_INPUT,
    }
}

/// The two fixture-backed ports this configuration names.
///
/// M0 has one implementation of each; the rest of the binary depends on the
/// traits, so the only thing that changes when a real adapter arrives is this
/// one function.
fn ports(config: &config::Config) -> (StubWorkItemPort, StubChangePort) {
    (
        StubWorkItemPort::new(&config.stub.root),
        StubChangePort::new(&config.stub.root),
    )
}

/// Observe both sides of the world for one invocation.
///
/// Nothing here can fail: a port that cannot read its source returns an
/// `Unavailable` observation rather than an error, so an unobservable world is
/// *reported* to the caller instead of aborting the command. That is why
/// `inspect` still exits 0 over a missing fixture root — it succeeded at
/// looking, and what it saw was that it could not see.
fn observe(config: &config::Config, reference: &InvocationRef) -> WorkStateView {
    let (work_items, changes) = ports(config);
    fiddle_runtime::observe(&work_items, &changes, reference.value())
}

/// The build identity every bundle this binary publishes carries.
///
/// Both halves are compile-time constants: `CARGO_PKG_VERSION` from the
/// manifest and `FIDDLE_SOURCE_REVISION` from `build.rs`. Passing them through
/// [`FiddleBuild::new`] rather than into the struct fields directly is what
/// makes "never fabricated" structural — a revision that is neither a 40-hex
/// sha nor `unknown` is normalised to `unknown` there rather than trusted here.
fn build_identity() -> FiddleBuild {
    FiddleBuild::new(env!("CARGO_PKG_VERSION"), env!("FIDDLE_SOURCE_REVISION"))
}

/// The bundle this run publishes, assembled from what the run concluded.
///
/// Consumes the [`RunReport`] rather than borrowing it, so there is exactly one
/// copy of the executions, the progress, and the observations, and the payload
/// printed to stdout is a projection of the same value that was written to
/// disk.
///
/// `work_ref` is the invocation reference in M0, where a beans reference is
/// both the request and the identity of the work. It is a separate field
/// because the two diverge as soon as a second scheme can address the same work
/// — and because the stability proof compares `work_ref` across two attempts,
/// which would prove nothing if it were derived from the attempt.
fn bundle_for(
    reference: &InvocationRef,
    mode: Mode,
    attempt_id: AttemptId,
    report: RunReport,
) -> ReportBundle {
    ReportBundle {
        schema: fiddle_core::REPORT_SCHEMA,
        fiddle: build_identity(),
        invocation_ref: reference.as_str(),
        work_ref: Some(WorkRef(reference.as_str())),
        attempt_id,
        mode,
        outcome: report.outcome,
        next_action: report.next_action,
        capability_executions: report.executions,
        progress: report.progress,
        observations: report.observations,
    }
}

fn dispatch(cli: &cli::Cli) -> Result<RunOutcome, CliError> {
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
            json,
        } => {
            // Parsed through `fiddle-core` rather than re-implemented here: the
            // CLI's only job is to turn the rejection into a diagnostic and an
            // exit code.
            //
            // The reference is validated *before* the configuration is loaded,
            // so a caller who mistyped the argument is told about the argument
            // rather than about a document they never mentioned.
            let reference: InvocationRef =
                invocation_ref.parse().map_err(InvalidInvocationRef::from)?;
            let config = config::load(&cli.config)?;
            let observed = observe(&config, &reference);
            // The CLI owns the configuration, so the CLI computes the marker
            // this invocation expects and hands it to the core. `assess` and
            // `derive_next` never reach for it themselves — that is what keeps
            // them pure functions of their arguments.
            let expected_marker =
                fiddle_core::correlation_key(&config.project.name, &reference.as_str());
            let assessment = fiddle_core::assess(&observed, &expected_marker);
            let next_action = fiddle_core::derive_next(&observed, &expected_marker);
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
            // Same order as `inspect`, and for the same reason: a caller who
            // mistyped an argument is told about the argument rather than about
            // a document they never mentioned. `--capability` is validated here
            // too, before anything is observed and long before anything could
            // be executed, so a rejected invocation provably did nothing.
            let reference: InvocationRef =
                invocation_ref.parse().map_err(InvalidInvocationRef::from)?;
            if let Some(requested) = capability {
                // M0 knows exactly one capability, so a valid selection can only
                // ever name the capability the derivation would choose anyway.
                // The flag's job here is to reject an unknown id loudly rather
                // than to narrow a plan that cannot be narrowed.
                resolve_capability(requested)?;
            }
            let config = config::load(&cli.config)?;

            let (work_items, changes) = ports(&config);
            let marking = StubMark::new(&config.stub.root, &config.project.name);
            let invocation = reference.as_str();
            let report = fiddle_runtime::run(&RunContext {
                project: &config.project.name,
                invocation_ref: &invocation,
                work_id: reference.value(),
                work_items: &work_items,
                changes: &changes,
                capability: &marking as &dyn Capability,
            });

            // Minted once, here: an attempt id names this attempt, and one run
            // is one attempt.
            let attempt = fiddle_runtime::mint_attempt_id();
            let mut bundle = bundle_for(&reference, *mode, attempt, report);

            let publication = fiddle_runtime::publish(
                &config.report.dir,
                &reference.slug(),
                &bundle.attempt_id,
                &bundle,
            );
            // The published path — `None` when publication failed — and the
            // outcome the process exits on, decided together because they are
            // two halves of one fact.
            let (published, outcome) = match publication {
                Ok(path) => {
                    // Relative to `<report.dir>`, so the payload stays the same
                    // whatever absolute prefix the configuration happens to
                    // name. `path` was built by joining onto that directory, so
                    // the strip cannot fail.
                    let relative = path.strip_prefix(&config.report.dir).unwrap_or(&path);
                    (Some(relative.to_path_buf()), bundle.outcome.clone())
                }
                Err(error) => {
                    eprintln!(
                        "{}",
                        render::publication_failure(&config.report.dir, &error)
                    );
                    // `Failed`, not `Retryable`, and the distinction is the
                    // point. `Retryable` is the promise that repeating this
                    // invocation unchanged may work — true of the change-set
                    // write a capability attempts, which contends with the
                    // fixture the run is trying to change. `<report.dir>` is
                    // named by configuration and does not become writable on
                    // its own, so repeating the run would fail identically
                    // forever; it needs an operator, which is exactly what
                    // `Failed` says. The two reasons also read differently — a
                    // capability failure names the change set, this one names
                    // the report bundle — so a caller reading only the payload
                    // can still tell them apart.
                    let failure = RunOutcome::Failed {
                        error: error.to_string(),
                    };
                    // The bundle was not published, so nothing on disk
                    // contradicts this; what the caller reads on stdout must
                    // still agree with the exit code they get.
                    bundle.outcome = failure.clone();
                    (None, failure)
                }
            };

            if *json {
                println!("{}", render::run_json(&bundle, published.as_deref()));
            } else {
                println!("{}", render::run_human(&bundle, published.as_deref()));
            }
            // The payload is printed on every path, including the failing ones:
            // a caller learns *what* fiddle concluded from stdout and *that* it
            // failed from the exit code, rather than having to choose.
            Ok(outcome)
        }
    }
}
