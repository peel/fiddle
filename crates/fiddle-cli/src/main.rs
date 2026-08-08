mod cli;
mod config;
mod render;

use clap::Parser;
use config::ConfigError;
use std::process::ExitCode;

/// Usage error or invalid configuration. Clap already exits with this code for
/// usage errors, so the constant exists to keep the two halves of the row in
/// the exit-code table visibly the same number.
const EXIT_INVALID_CONFIG: u8 = 2;

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

/// The single realisation of the exit-code table. Every future outcome variant
/// gains its row here and nowhere else, so the mapping can never drift between
/// commands.
fn exit_code_for(error: &ConfigError) -> u8 {
    match error {
        ConfigError::NotFound(_) | ConfigError::Invalid(_) => EXIT_INVALID_CONFIG,
    }
}

fn dispatch(cli: &cli::Cli) -> Result<(), ConfigError> {
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
    }
}
