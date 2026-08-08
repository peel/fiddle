use clap::{Parser, Subcommand};
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
