//! The strict `fiddle.toml` schema and its loader.
//!
//! Configuration is a typed, strict document: every struct carries
//! `#[serde(deny_unknown_fields)]`, so an unrecognised key is a hard error
//! rather than a silently ignored one. The schema admits no secret-valued
//! field. M0 has no credential at all, and when one arrives it will be named by
//! environment variable rather than carried here as a resolved value.
//!
//! Loading lives here rather than in `fiddle-core` because it reads the
//! filesystem, and `fiddle-core` is mechanically held pure.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The whole configuration document.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub project: Project,
    pub stub: Stub,
    pub report: Report,
}

/// Identity of the project a fiddle run acts on.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub name: String,
}

/// Where the fixture-backed stub ports read and write their state.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stub {
    pub root: PathBuf,
}

/// Where a run publishes its evidence bundles.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub dir: PathBuf,
}

/// A document that failed the strict schema, carrying enough context to point
/// at the offending bytes.
///
/// Design §4.6 requires the diagnostic to name the offending key *and its
/// line*, rendered through `miette`. `toml::de::Error::span` gives the byte
/// range; miette turns it into a source snippet with a caret. `thiserror`
/// alone would render the message and lose the location.
///
/// The whole source text lives in here, which is why [`ConfigError`] holds it
/// behind a `Box`: a configuration error is returned once, at the process
/// boundary, and there is no reason for every `Result` in the call chain to
/// reserve room for the file.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("invalid configuration in {path}")]
#[diagnostic(code(fiddle::config::invalid))]
pub struct InvalidConfig {
    /// The document that was rejected, as the caller named it.
    pub path: PathBuf,
    #[source_code]
    src: miette::NamedSource<String>,
    #[label("{message}")]
    span: Option<miette::SourceSpan>,
    message: String,
}

/// Everything that can go wrong loading a configuration document.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ConfigError {
    #[error("configuration file not found: {0}")]
    #[diagnostic(code(fiddle::config::not_found))]
    NotFound(PathBuf),

    #[error(transparent)]
    #[diagnostic(transparent)]
    Invalid(#[from] Box<InvalidConfig>),
}

/// Read and strictly deserialize the configuration document at `path`.
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    let text =
        std::fs::read_to_string(path).map_err(|_| ConfigError::NotFound(path.to_path_buf()))?;
    toml::from_str(&text).map_err(|e| {
        ConfigError::Invalid(Box::new(InvalidConfig {
            path: path.to_path_buf(),
            src: miette::NamedSource::new(path.display().to_string(), text.clone()),
            span: e.span().map(|r| (r.start, r.end - r.start).into()),
            message: e.message().to_string(),
        }))
    })
}
