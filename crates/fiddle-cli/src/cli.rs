use clap::builder::TypedValueParser;
use clap::{Parser, Subcommand};
use fiddle_core::Mode;
use std::path::PathBuf;

/// The version string `fiddle --version` prints after the binary name:
/// `<package version> (<source revision>)`. `concat!` needs both halves to be
/// literals, so this is a const rather than a function body — `env!` resolves
/// `FIDDLE_SOURCE_REVISION` at compile time from `build.rs`.
pub const FIDDLE_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("FIDDLE_SOURCE_REVISION"),
    ")"
);

/// The package version and source revision this binary was built from. Every
/// caller that needs to report the build — `--version` here, the report
/// bundle's `FiddleBuild` later — goes through this one accessor.
pub fn fiddle_version() -> &'static str {
    FIDDLE_VERSION
}

#[derive(Parser)]
#[command(name = "fiddle", version = fiddle_version(), disable_version_flag = false)]
pub struct Cli {
    /// Path to the fiddle configuration document.
    ///
    /// Global, because every command that acts on a project needs the same
    /// document; only its default location is a convention.
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
    /// Work with the fiddle configuration document.
    Config {
        #[command(subcommand)]
        action: ConfigCommand,
    },

    /// Report what fiddle observes about an invocation, without changing it.
    ///
    /// Read-only by contract: `inspect` never writes fixture state and never
    /// publishes a report bundle.
    Inspect {
        // Held as a string here and parsed by `fiddle_core::InvocationRef` in
        // the dispatcher, so the grammar has exactly one implementation and each
        // malformed shape can be reported with its own diagnostic rather than
        // clap's generic value error. Doc comments on this field become
        // `--help` text, so the rationale stays a plain comment.
        /// The work to inspect, as `<scheme>:<value>` — for example
        /// `beans:fiddle-m0-demo`. A scheme that finds its own work stands
        /// alone and takes no value: `cve` scans the configured image and
        /// inspects what it finds, while `cve:CVE-2026-1234` names one finding.
        #[arg(value_name = "INVOCATION_REF")]
        invocation_ref: String,

        // Why a selection flag exists on a command that changes nothing:
        // `inspect` reports the *next action*, and a next action names a
        // capability. Without the flag it named one particular capability
        // whatever the caller was about to run, so over a repair-configured
        // project `inspect` said `execute stub_mark` while `run --capability
        // fixture_repair` did something else — a read-only command whose whole
        // purpose is to say what a run would do, saying the wrong thing. The
        // flag is spelled and defaulted exactly as `run`'s is, so the two cannot
        // disagree while being asked the same question.
        //
        // Selecting is *all* it does here. The id reaches the derivation and
        // nothing else: no capability is built, no credential is resolved and no
        // configuration table is required, so `inspect --capability
        // fixture_repair` still answers offline over an M0 document and stays
        // read-only.
        //
        // The same holds for `--capability publish_change`, and it has to hold
        // for *every* value the flag takes rather than for the ones that happen
        // to need nothing: a capability that reaches a forge is exactly the one
        // whose selection could make a read-only command demand a credential,
        // and it does not, because selecting still stops at the derivation.
        /// Report the plan for one capability id rather than for the default.
        /// The same ids `run --capability` takes; an unknown id is a usage
        /// error.
        #[arg(long, value_name = "CAPABILITY_ID")]
        capability: Option<String>,

        /// Emit the machine-readable payload instead of the human summary.
        #[arg(long)]
        json: bool,
    },

    /// Execute the plan fiddle derives for an invocation.
    ///
    /// The only command that changes anything. What it may do is decided by the
    /// same derivation `inspect` reports, so a run over work that is already
    /// accounted for completes without executing.
    Run {
        /// The work to run, as `<scheme>:<value>` — for example
        /// `beans:fiddle-m0-demo`. A scheme that finds its own work stands
        /// alone and takes no value: `cve` scans the configured image and
        /// runs what it finds, while `cve:CVE-2026-1234` names one finding.
        #[arg(value_name = "INVOCATION_REF")]
        invocation_ref: String,

        // The mode's meaning and spelling belong to `fiddle-core`, because the
        // report bundle records it; what belongs here is only how the value is
        // parsed off the command line. `PossibleValuesParser` rather than the
        // bare `FromStr` so `--help` lists the choices and a bad value is
        // rejected by clap with the usual usage exit code.
        /// Whether a human is available to decide. Nothing branches on the
        /// value: the one decision point this build has is `propose_change`'s,
        /// and it asks its question whether or not a human was declared to be
        /// waiting. So both modes execute identically, and the mode is recorded
        /// in what the run publishes rather than acted on.
        #[arg(
            long,
            value_name = "MODE",
            default_value_t = Mode::Unattended,
            value_parser = clap::builder::PossibleValuesParser::new(Mode::NAMES)
                .map(|value| value.parse::<Mode>().expect("clap restricts this to a known mode")),
        )]
        mode: Mode,

        /// Restrict execution to one capability id. Absent selects the default,
        /// `stub_mark`. An unknown id is a usage error, never a silent no-op.
        #[arg(long, value_name = "CAPABILITY_ID")]
        capability: Option<String>,

        /// Emit the machine-readable payload instead of the human summary.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Load `--config` and report whether it satisfies the strict schema.
    Check {
        /// Emit the machine-readable payload instead of the human summary.
        #[arg(long)]
        json: bool,
    },
}
