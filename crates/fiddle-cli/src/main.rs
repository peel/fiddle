mod cli;
mod config;
mod render;

use clap::Parser;
use config::ConfigError;
use fiddle_core::{InvocationRef, InvocationRefError};
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
            // At this milestone `inspect` reports the invocation identity only,
            // so it consults no configuration; the observations that need
            // `--config` arrive with the stub ports in a later task.
            let reference: InvocationRef =
                invocation_ref.parse().map_err(InvalidInvocationRef::from)?;
            if *json {
                println!("{}", render::inspect_json(&reference));
            } else {
                println!("{}", render::inspect_human(&reference));
            }
            Ok(())
        }
    }
}
