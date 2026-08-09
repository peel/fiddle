//! Everything that reaches GitHub.
//!
//! One module, so that "what can this process do to a repository?" is answered
//! by reading one directory rather than by grepping for a hostname. Today it
//! holds [`cli`], the single credential-carrying `gh` construction; the ref,
//! pull-request and check operations that M2 builds on top of it land beside it
//! and all of them go through that one construction rather than spawning their
//! own.

pub mod cli;

pub use cli::{GhCli, GhError, GhResponse};
