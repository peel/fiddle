mod cli;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    let _cli = cli::Cli::parse();
    ExitCode::from(0)
}
