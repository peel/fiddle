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
        default_value = "fiddle.toml",
        help = "Path to the fiddle configuration document.",
        long_help = "Path to the fiddle configuration document.\n\nGlobal, because every command \
                     that acts on a project needs the same document; only its default location is \
                     a convention."
    )]
    pub config: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Work with the fiddle configuration document.")]
    Config {
        #[command(subcommand)]
        action: ConfigCommand,
    },

    #[command(
        about = "Report what fiddle observes about an invocation, without changing it.",
        long_about = "Report what fiddle observes about an invocation, without changing it.\n\n\
                      Read-only by contract: `inspect` never writes fixture state and never \
                      publishes a report bundle."
    )]
    Inspect {
        #[arg(
            value_name = "INVOCATION_REF",
            help = "The work to inspect, as `<scheme>:<value>` — for example \
                    `beans:fiddle-m0-demo`. A scheme that finds its own work stands alone and \
                    takes no value: `cve` scans the configured image and inspects what it finds."
        )]
        invocation_ref: String,

        #[arg(
            long,
            value_name = "CAPABILITY_ID",
            help = "Report the plan for one capability id rather than for the one the reference's \
                    scheme implies. The same ids and the same default as `run --capability`; an \
                    unknown id is a usage error."
        )]
        capability: Option<String>,

        #[arg(
            long,
            help = "Emit the machine-readable payload instead of the human summary."
        )]
        json: bool,
    },

    #[command(
        about = "Execute the plan fiddle derives for an invocation.",
        long_about = "Execute the plan fiddle derives for an invocation.\n\nThe only command that \
                      changes anything. What it may do is decided by the same derivation \
                      `inspect` reports, so a run over work that is already accounted for \
                      completes without executing."
    )]
    Run {
        #[arg(
            value_name = "INVOCATION_REF",
            help = "The work to run, as `<scheme>:<value>` — for example `beans:fiddle-m0-demo`. \
                    A scheme that finds its own work stands alone and takes no value: `cve` scans \
                    the configured image and runs what it finds."
        )]
        invocation_ref: String,

        #[arg(
            long,
            value_name = "MODE",
            default_value_t = Mode::Unattended,
            value_parser = clap::builder::PossibleValuesParser::new(Mode::NAMES)
                .map(|value| value.parse::<Mode>().expect("clap restricts this to a known mode")),
            help = "Whether a human is available to decide. Nothing branches on the value: the \
                    one decision point this build has is `propose_change`'s, and it asks its \
                    question whether or not a human was declared to be waiting. So both modes \
                    execute identically, and the mode is recorded in what the run publishes \
                    rather than acted on.",
        )]
        mode: Mode,

        #[arg(
            long,
            value_name = "CAPABILITY_ID",
            help = "Restrict execution to one capability id. Absent selects what the reference's \
                    scheme implies: `cve` sweeps its configured image, every other scheme marks. \
                    An unknown id is a usage error, never a silent no-op."
        )]
        capability: Option<String>,

        #[arg(
            long,
            help = "Emit the machine-readable payload instead of the human summary."
        )]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    #[command(about = "Load `--config` and report whether it satisfies the strict schema.")]
    Check {
        #[arg(
            long,
            help = "Emit the machine-readable payload instead of the human summary."
        )]
        json: bool,
    },
}
