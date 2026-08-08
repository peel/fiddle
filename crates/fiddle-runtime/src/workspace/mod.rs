//! The bounded rig's view of the filesystem.
//!
//! Everything a model asks for arrives as a string, and a string is not proof
//! of anything. This module is where such a string becomes a value that carries
//! its own guarantee: [`path`] turns a requested path into a [`WorkspacePath`],
//! which by construction names something inside the workspace and nothing
//! outside it. ADR 011 records what happens when a derived path is trusted
//! instead of proven, so containment is a property of the type here too, not a
//! check each call site is expected to remember.

pub mod path;

pub use path::WorkspacePath;

/// What can go wrong when a requested path is turned into a usable one.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceError {
    /// The requested path was refused because it does not provably name
    /// something inside the workspace. `reason` names the rule that fired, so
    /// an operator reading the diagnostic learns which shape was rejected
    /// rather than only that something was.
    #[error("path {path} escapes the workspace: {reason}")]
    Escape { path: String, reason: String },

    /// The path was legal but the filesystem operation on it failed.
    #[error("io error at {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}
