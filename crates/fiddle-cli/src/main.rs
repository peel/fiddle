mod cli;
mod config;
mod render;

use clap::Parser;
use config::ConfigError;
use fiddle_core::{InvocationRef, InvocationRefError, WorkStateView};
use fiddle_runtime::{ChangePort, StubChangePort, StubWorkItemPort, WorkItemPort};
use std::process::ExitCode;

/// Usage error or invalid input — row `2` of the exit-code table. Clap already
/// exits with this code for usage errors, so the constant exists to keep every
/// half of the row visibly the same number.
const EXIT_INVALID_INPUT: u8 = 2;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match dispatch(&cli) {
        Ok(()) => ExitCode::from(0),
        Err(error) => {
            eprintln!("{}", render::diagnostic(&error));
            ExitCode::from(exit_code_for(&error))
        }
    }
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

/// The single realisation of the exit-code table. Every future outcome variant
/// gains its row here and nowhere else, so the mapping can never drift between
/// commands.
fn exit_code_for(error: &CliError) -> u8 {
    match error {
        CliError::Config(ConfigError::NotFound(_) | ConfigError::Invalid(_)) => EXIT_INVALID_INPUT,
        CliError::InvocationRef(_) => EXIT_INVALID_INPUT,
    }
}

/// Observe both sides of the world for one invocation.
///
/// Nothing here can fail: a port that cannot read its source returns an
/// `Unavailable` observation rather than an error, so an unobservable world is
/// *reported* to the caller instead of aborting the command. That is why
/// `inspect` still exits 0 over a missing fixture root — it succeeded at
/// looking, and what it saw was that it could not see.
///
/// M0 has one implementation of each port; the CLI depends on the traits, so
/// the only thing that changes when a real adapter arrives is these two
/// constructor calls.
fn observe(config: &config::Config, reference: &InvocationRef) -> WorkStateView {
    let work_id = reference.value();
    let work_item = StubWorkItemPort::new(&config.stub.root);
    let changes = StubChangePort::new(&config.stub.root);
    WorkStateView {
        work_item: WorkItemPort::observe(&work_item, work_id),
        changes: ChangePort::observe(&changes, work_id),
    }
}

fn dispatch(cli: &cli::Cli) -> Result<(), CliError> {
    match &cli.command {
        cli::Command::Config { action } => match action {
            cli::ConfigCommand::Check { json } => {
                let config = config::load(&cli.config)?;
                if *json {
                    println!("{}", render::config_check_json(&config));
                } else {
                    println!("{}", render::config_check_human(&config));
                }
                Ok(())
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
            if *json {
                println!("{}", render::inspect_json(&reference, &observed));
            } else {
                println!("{}", render::inspect_human(&reference, &observed));
            }
            Ok(())
        }
    }
}
