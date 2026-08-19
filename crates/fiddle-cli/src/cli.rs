use clap::builder::TypedValueParser;
use clap::{Parser, Subcommand};
use fiddle_core::Mode;
use std::path::PathBuf;

pub const FIDDLE_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("FIDDLE_SOURCE_REVISION"),
    ")"
);

pub fn fiddle_version() -> &'static str {
    FIDDLE_VERSION
}

#[derive(Parser)]
#[command(name = "fiddle", version = fiddle_version(), disable_version_flag = false)]
pub struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "PATH",
        default_value = "fiddle.toml"
    )]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Config {
        #[command(subcommand)]
        action: ConfigCommand,
    },

    Inspect {
        #[arg(value_name = "INVOCATION_REF")]
        invocation_ref: String,

        #[arg(long, value_name = "CAPABILITY_ID")]
        capability: Option<String>,

        #[arg(long)]
        json: bool,
    },

    Run {
        #[arg(value_name = "INVOCATION_REF")]
        invocation_ref: String,

        #[arg(
            long,
            value_name = "MODE",
            default_value_t = Mode::Unattended,
            value_parser = clap::builder::PossibleValuesParser::new(Mode::NAMES)
                .map(|value| value.parse::<Mode>().expect("clap restricts this to a known mode")),
        )]
        mode: Mode,

        #[arg(long, value_name = "CAPABILITY_ID")]
        capability: Option<String>,

        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    Check {
        #[arg(long)]
        json: bool,
    },
}
